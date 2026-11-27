//! Optional shared output storage for software mesh draws on the host's one
//! DIRECT queue. Each draw finishes its read -> UAV transition before another
//! draw writes the same bytes. Buffers can promote COMMON -> UAV on first
//! access and decay after ExecuteCommandLists; no COMMON transition is
//! recorded at an independently recorded list's beginning.
//!
//! Inspired by yoraudev's shared intermediate in Marvisak/VIctory #1648
//! (d62ba28cc). Keep this fork's translator, output layout and group limits.
//! Old sizes stay owned, so lists recorded before growth keep valid buffers.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use windows::Win32::Graphics::Direct3D12::ID3D12Resource;
use crate::host::ScratchRegion;

pub(crate) fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_MESH_SHARED").is_some_and(|v| v == "1"))
}

#[derive(Default)]
struct Arena {
    buffers: BTreeMap<u64, ID3D12Resource>,
}

impl Arena {
    fn region(&mut self, len: u64) -> Option<ScratchRegion> {
        let len = len.max(16).checked_next_multiple_of(256)?;
        let buffer = if let Some((_, buffer)) = self.buffers.range(len..).next() {
            buffer.clone()
        } else {
            let size = len.max(1 << 20).checked_next_power_of_two()?;
            let buffer = crate::host::host()?.uav_buffer(size)?;
            crate::host::name(&buffer, || format!("shared mesh output {size:#x}"));
            self.buffers.insert(size, buffer.clone());
            tracing::info!(size, held = self.bytes(), "shared mesh output storage grown");
            buffer
        };
        Some(ScratchRegion { buffer, offset: 0, first: false })
    }

    fn bytes(&self) -> u64 {
        self.buffers.keys().copied().sum()
    }
}

static ARENA: OnceLock<Mutex<Arena>> = OnceLock::new();

pub(crate) fn region(len: u64) -> Option<ScratchRegion> {
    ARENA.get_or_init(|| Mutex::new(Arena::default()))
        .lock().unwrap_or_else(|e| e.into_inner()).region(len)
}

pub(crate) fn bytes() -> u64 {
    ARENA.get().map_or(0, |arena| arena.lock().unwrap_or_else(|e| e.into_inner()).bytes())
}

#[cfg(test)]
mod tests;
