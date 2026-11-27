//! Opt-in cross-call software mesh windows on the one DIRECT queue.
//! Each record retains its full original output capacity in a disjoint slot.
//! The caller admits only independent buffer-reading shaders and flushes at
//! every external GPU operation or state boundary. Commands end each compute
//! phase UAV; flush transitions their distinct regions back to arguments.
use std::sync::OnceLock;
use windows::Win32::Graphics::Direct3D12::*;
use windows::core::Interface;
use crate::host::{HostList, ScratchRegion};
use crate::mesh_barriers::{Access, MeshBarriers};

pub(crate) const SLOTS: u32 = 8;
pub(crate) const STRIDE: u64 = crate::mesh_indirect::BYTES;

pub(crate) fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let asked = std::env::var_os("VICTORY_MESH_WINDOWS").is_some_and(|v| v == "1");
        #[cfg(feature = "diagnostics")]
        let recording = crate::replay::record::on();
        #[cfg(not(feature = "diagnostics"))]
        let recording = false;
        asked && crate::mesh_shared::enabled() && !recording
            && std::env::var_os("VICTORY_MESH_CAPTURE").is_none()
            && std::env::var_os("VICTORY_GPU_TIMING").is_none_or(|v| v != "1")
    })
}

/// One allocator per host list, reused only by HostList::reset after completion.
/// Each bundle of a recording has a separate command stream in that allocator.
#[derive(Default)]
pub(crate) struct Pool {
    allocator: Option<ID3D12CommandAllocator>,
    lists: Vec<(ID3D12GraphicsCommandList, bool)>,
    used: usize,
}

// SAFETY: D3D12 bundle objects are free-threaded; Pool is used under
// HostList's mutex, and reset is called only after all submissions complete.
unsafe impl Send for Pool {}

impl Pool {
    fn acquire(&mut self) -> windows::core::Result<ID3D12GraphicsCommandList> {
        let host = crate::host::host().ok_or_else(windows::core::Error::empty)?;
        // SAFETY: one list records at a time; no allocator reset until fenced.
        unsafe {
            if self.allocator.is_none() {
                self.allocator = Some(host.device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_BUNDLE)?);
            }
            let allocator = self.allocator.as_ref().unwrap();
            if self.used == self.lists.len() {
                let list = host.device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_BUNDLE, allocator, None)?;
                self.lists.push((list, true));
            } else {
                self.lists[self.used].0.Reset(allocator, None)?;
                self.lists[self.used].1 = true;
            }
            let list = self.lists[self.used].0.clone();
            self.used += 1;
            Ok(list)
        }
    }

    fn closed(&mut self, list: &ID3D12GraphicsCommandList) {
        let entry = self.lists[..self.used].iter_mut().find(|(l, _)| l.as_raw() == list.as_raw())
            .expect("bundle belongs to this host list");
        entry.1 = false;
    }

    pub(crate) fn reset(&mut self) -> windows::core::Result<()> {
        // SAFETY: caller guarantees all submitted work completed. An abandoned
        // recording may still own an open bundle, which is closed before reset.
        unsafe {
            for (list, open) in &mut self.lists[..self.used] {
                if *open {
                    list.Close()?;
                    *open = false;
                }
            }
            if let Some(allocator) = &self.allocator {
                allocator.Reset()?;
            }
        }
        self.used = 0;
        Ok(())
    }
}

pub(crate) struct Window {
    pub(crate) bundle: ID3D12GraphicsCommandList,
    pub(crate) pipeline: usize,
    pub(crate) heaps: [usize; 2],
    data: ScratchRegion,
    commands: Vec<ID3D12Resource>,
    used: u32,
    writes: u32,
}

// SAFETY: D3D12 resources are free-threaded. One recorder owns the window
// under the title list's State mutex; no GPU-visible object is reset here.
unsafe impl Send for Window {}

impl Window {
    pub(crate) fn new(list: &HostList, pipeline: usize, heaps: [usize; 2]) -> Option<Self> {
        let data = crate::mesh_shared::region(STRIDE * u64::from(SLOTS))?;
        let bundle = list.mesh_bundles.lock().unwrap_or_else(|e| e.into_inner()).acquire().ok()?;
        list.keep(&data.buffer);
        Some(Self { bundle, pipeline, heaps, data, commands: Vec::new(), used: 0, writes: 0 })
    }

    pub(crate) fn full(&self) -> bool { self.used == SLOTS }

    pub(crate) fn wrote(&mut self) { self.writes += 1; }

    pub(crate) fn reserve(&mut self, bytes: u64) -> Option<ScratchRegion> {
        if self.full() || bytes == 0 || bytes > STRIDE { return None; }
        let offset = self.data.offset + u64::from(self.used) * STRIDE;
        self.used += 1;
        Some(ScratchRegion { buffer: self.data.buffer.clone(), offset, first: false })
    }

    /// The converter and indirect compute dispatch consumed these arguments;
    /// caller has returned the whole command resource to UAV before recording
    /// another converter. Distinct offsets are not recycled until flush.
    pub(crate) fn commands(&mut self, buffer: &ID3D12Resource) {
        if !self.commands.iter().any(|b| b.as_raw() == buffer.as_raw()) {
            self.commands.push(buffer.clone());
        }
    }

    pub(crate) fn finish(self, list: &HostList) -> windows::core::Result<()> {
        // SAFETY: only bundle-supported graphics calls have been recorded.
        unsafe { self.bundle.Close()?; }
        list.mesh_bundles.lock().unwrap_or_else(|e| e.into_inner()).closed(&self.bundle);
        if self.writes != 0 {
            let barriers = MeshBarriers::new(&list.list);
            barriers.between(&self.data.buffer, Access::Compute, Access::Vertices);
            for commands in &self.commands {
                barriers.between(commands, Access::Compute, Access::Arguments);
                list.keep(commands);
            }
            // Pool owns bundle and allocator until this host list is reset
            // after completion. The list separately holds every named resource.
            unsafe { list.list.ExecuteBundle(&self.bundle); }
            barriers.between(&self.data.buffer, Access::Vertices, Access::Compute);
            for commands in &self.commands {
                barriers.between(commands, Access::Arguments, Access::Compute);
            }
        }
        list.mesh_scratch_recycle();
        Ok(())
    }
}
