//! Readonly mesh/texture batches with one closed graphics bundle per call.
//! Resource preparation may flush completed draws while the current call's
//! graphics bundle is still open. The reserved data slot and command region
//! stay owned; mutation flushes never recycle the current call's scratch.
use crate::host::{HostList, ScratchRegion};
use crate::mesh_barriers::{Access, MeshBarriers};
use crate::mesh_window::{Pool, SLOTS, STRIDE};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use windows::Win32::Graphics::Direct3D12::*;
use windows::core::Interface;

pub(crate) fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let requested = std::env::var_os("VICTORY_MESH_TEXTURE_WINDOWS").is_some_and(|v| v == "1");
        let no_async = [
            "VICTORY_UPLOAD_AHEAD",
            "VICTORY_STREAM_COPY",
            "VICTORY_STREAM_REFRESH",
        ]
        .iter()
        .all(|name| std::env::var_os(name).is_some_and(|v| v == "0"));
        requested && no_async && crate::mesh_window::enabled()
    })
}

pub(crate) struct Batch {
    pipeline: usize,
    heaps: [usize; 2],
    buffer: ID3D12Resource,
    used: u32,
    bundles: Vec<ID3D12GraphicsCommandList>,
    descriptor_heaps: Vec<Option<ID3D12DescriptorHeap>>,
    commands: Vec<ID3D12Resource>,
}

// SAFETY: one recording thread owns each HostList; its batch is also mutex
// protected. Only closed bundles enter Batch, and pool reset is fenced.
unsafe impl Send for Batch {}

pub(crate) struct Draw {
    pub(crate) bundle: ID3D12GraphicsCommandList,
    pub(crate) graphics_initialized: bool,
    pub(crate) descriptor_heaps: Vec<Option<ID3D12DescriptorHeap>>,
    pipeline: usize,
    heaps: [usize; 2],
    buffer: ID3D12Resource,
    slot: u32,
    pool: Arc<Mutex<Pool>>,
    generation: u64,
    closed: bool,
}

// SAFETY: Draw belongs to the title list's State mutex, not two recorders.
unsafe impl Send for Draw {}

impl Draw {
    pub(crate) fn region(&self) -> ScratchRegion {
        ScratchRegion {
            buffer: self.buffer.clone(),
            offset: u64::from(self.slot) * STRIDE,
            first: false,
        }
    }

    /// Closing this call does not close any other call's bundle. Pool keeps
    /// every submitted bundle/allocator until the host list's fenced reset.
    fn close(&mut self) -> windows::core::Result<()> {
        let mut pool = self.pool.lock().unwrap_or_else(|e| e.into_inner());
        if pool.generation() != self.generation {
            return Err(windows::core::Error::empty());
        }
        if !self.closed {
            // SAFETY: only this recorder can access this open bundle.
            unsafe {
                self.bundle.Close()?;
            }
            pool.closed(&self.bundle);
            self.closed = true;
        }
        Ok(())
    }

    pub(crate) fn commit(
        mut self,
        list: &HostList,
        drawn: bool,
        command: Option<&ID3D12Resource>,
        heaps: [usize; 2],
    ) {
        if let Err(e) = self.close() {
            crate::host::fail(format_args!(
                "a textured mesh draw bundle could not close ({e})"
            ));
        }
        if !drawn {
            return;
        }
        self.heaps = heaps;
        let different = list
            .texture_mesh_batch
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|b| {
                b.pipeline != self.pipeline
                    || b.heaps != heaps
                    || b.buffer.as_raw() != self.buffer.as_raw()
                    || !b.bundles.is_empty()
                        && !same_heaps(&b.descriptor_heaps, &self.descriptor_heaps)
            });
        if different {
            flush(list);
        }
        let mut pending = list
            .texture_mesh_batch
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let batch = pending.get_or_insert_with(|| Batch {
            pipeline: self.pipeline,
            heaps,
            buffer: self.buffer.clone(),
            used: self.slot + 1,
            bundles: Vec::new(),
            descriptor_heaps: self.descriptor_heaps.clone(),
            commands: Vec::new(),
        });
        batch.used = batch.used.max(self.slot + 1);
        batch.bundles.push(self.bundle.clone());
        if batch.descriptor_heaps.is_empty() {
            batch.descriptor_heaps = self.descriptor_heaps.clone();
        }
        if let Some(command) = command
            && !batch
                .commands
                .iter()
                .any(|b| b.as_raw() == command.as_raw())
        {
            batch.commands.push(command.clone());
        }
        list.texture_mesh_pending.store(true, Ordering::Release);
        STATS[0].fetch_add(1, Ordering::Relaxed);
    }
}

impl Drop for Draw {
    fn drop(&mut self) {
        if !self.closed {
            let mut pool = self.pool.lock().unwrap_or_else(|e| e.into_inner());
            // A discarded State may outlive an abandoned recording's reset.
            // Never close an object that the pool has since begun reusing.
            if pool.generation() == self.generation {
                // SAFETY: this Draw is the open bundle's sole recorder.
                let _ = unsafe { self.bundle.Close() };
                pool.closed(&self.bundle);
            }
        }
    }
}

pub(crate) fn reserve(
    list: &HostList,
    pipeline: usize,
    heaps: [usize; 2],
    bytes: u64,
) -> Option<Draw> {
    if !enabled() || bytes == 0 || bytes > STRIDE {
        return None;
    }
    let different = list
        .texture_mesh_batch
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|b| b.used == SLOTS || b.pipeline != pipeline || b.heaps != heaps);
    if different {
        flush(list);
    }
    let mut pending = list
        .texture_mesh_batch
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if pending.is_none() {
        let region = crate::mesh_shared::region(STRIDE * u64::from(SLOTS))?;
        list.keep(&region.buffer);
        *pending = Some(Batch {
            pipeline,
            heaps,
            buffer: region.buffer,
            used: 0,
            bundles: Vec::new(),
            descriptor_heaps: Vec::new(),
            commands: Vec::new(),
        });
    }
    let batch = pending.as_mut().unwrap();
    let slot = batch.used;
    batch.used += 1;
    let buffer = batch.buffer.clone();
    drop(pending);
    let pool = list
        .texture_mesh_pool
        .get_or_init(|| Arc::new(Mutex::new(Pool::default())))
        .clone();
    let (bundle, generation) = {
        let mut pool = pool.lock().unwrap_or_else(|e| e.into_inner());
        (pool.acquire().ok()?, pool.generation())
    };
    Some(Draw {
        bundle,
        graphics_initialized: false,
        descriptor_heaps: Vec::new(),
        pipeline,
        heaps,
        buffer,
        slot,
        pool,
        generation,
        closed: false,
    })
}

/// Flush only prior, closed bundles. A current draw can keep its reserved slot
/// while a texture upload/state change is recorded. Do not recycle scratch.
pub(crate) fn before_texture_change(list: &HostList) {
    if enabled() && list.texture_mesh_pending.load(Ordering::Acquire) {
        STATS[2].fetch_add(1, Ordering::Relaxed);
        flush(list);
    }
}

pub(crate) fn flush(list: &HostList) {
    if !enabled() {
        return;
    }
    let batch = list
        .texture_mesh_batch
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    list.texture_mesh_pending.store(false, Ordering::Release);
    let Some(batch) = batch else {
        return;
    };
    if batch.bundles.is_empty() {
        return;
    }
    let barriers = MeshBarriers::new(&list.list);
    barriers.between(&batch.buffer, Access::Compute, Access::Vertices);
    for command in &batch.commands {
        barriers.between(command, Access::Compute, Access::Arguments);
        list.keep(command);
    }
    // SAFETY: closed, retained bundles. Explicitly restore the matching
    // heaps because resource preparation may have changed parent bindings.
    unsafe {
        if !batch.descriptor_heaps.is_empty() {
            list.list.SetDescriptorHeaps(&batch.descriptor_heaps);
        }
        for bundle in &batch.bundles {
            list.list.ExecuteBundle(bundle);
        }
    }
    barriers.between(&batch.buffer, Access::Vertices, Access::Compute);
    for command in &batch.commands {
        barriers.between(command, Access::Arguments, Access::Compute);
    }
    list.texture_mesh_generation.fetch_add(1, Ordering::Release);
    STATS[1].fetch_add(1, Ordering::Relaxed);
}

fn same_heaps(a: &[Option<ID3D12DescriptorHeap>], b: &[Option<ID3D12DescriptorHeap>]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| match (a, b) {
            (Some(a), Some(b)) => a.as_raw() == b.as_raw(),
            (None, None) => true,
            _ => false,
        })
}

static STATS: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];
pub(crate) fn line(frames: f64) {
    if !enabled() {
        return;
    }
    let [calls, windows, mutations] = STATS
        .each_ref()
        .map(|s| s.swap(0, Ordering::Relaxed) as f64 / frames.max(1.0));
    tracing::info!(
        calls,
        windows,
        mutations,
        mean_calls = if windows > 0.0 { calls / windows } else { 0.0 },
        "readonly textured mesh windows per frame"
    );
}
