//! Opt-in GPU constant copies behind recent GPU-placed title copies.
//! Uses the existing place_source ordering and fence-owned resource holds.
//! No shader/root-layout changes, imported title heaps, or CPU read-ahead.

use super::*;
use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

const MOST: usize = 4;
const RECENT_MOST: usize = 256;
static RECENT: Mutex<Vec<(usize, usize, u64)>> = Mutex::new(Vec::new());
static COUNTS: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];

pub(super) fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_GPU_CONSTANTS").is_some_and(|v| v == "1"))
}

pub(super) fn note_destination(dst: usize, len: usize) {
    if !on() || len == 0 || len > 1 << 20 { return; }
    let Some(end) = dst.checked_add(len) else { return; };
    let frame = crate::frame::current();
    let mut recent = RECENT.lock().unwrap_or_else(|e| e.into_inner());
    recent.retain(|&(a, b, f)| f.saturating_add(2) >= frame && (a, b) != (dst, end));
    if recent.len() >= RECENT_MOST { recent.remove(0); }
    recent.push((dst, end, frame));
}

fn recent_source(src: usize, len: usize) -> bool {
    let Some(end) = src.checked_add(len) else { return false; };
    let frame = crate::frame::current();
    RECENT.lock().unwrap_or_else(|e| e.into_inner()).iter()
        .any(|&(a, b, f)| f.saturating_add(2) >= frame && a < end && src < b)
}

impl HostList {
    /// Constant bytes whose recent placed copy already exists on the GPU.
    /// Each copy has a new, aligned buffer; no overwritten per-draw slots.
    /// The four-buffer cap bounds additional live allocation per recording.
    pub(crate) fn constant_from_gpu(&self, address: u64, len: u64) -> Option<(ID3D12Resource, u64)> {
        if !on() || len == 0 || len > 1 << 16 { return None; }
        // Enhanced dependencies are not encoded by the frame recorder yet.
        #[cfg(feature = "diagnostics")]
        if crate::replay::record::on() { return None; }
        COUNTS[0].fetch_add(1, Relaxed);
        if !recent_source(address as usize, len as usize) {
            COUNTS[3].fetch_add(1, Relaxed);
            return None;
        }
        let shadow = crate::shadow::peek(address, len)?;
        if shadow.in_place() || shadow.card_sides().is_none() || !shadow.written() {
            COUNTS[4].fetch_add(1, Relaxed);
            return None;
        }
        if self.deferred().gpu_constants >= MOST {
            COUNTS[2].fetch_add(1, Relaxed);
            return None;
        }
        let buffer = self.place_source(address as usize, len as usize)?;
        let readable = D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER
            | D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE
            | D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE;
        // SAFETY: place_source returned a retained buffer in COPY_SOURCE;
        // all source writes and this copy were ordered on the direct list.
        unsafe {
            self.list.ResourceBarrier(&[transition(&buffer, D3D12_RESOURCE_STATE_COPY_SOURCE, readable)]);
        }
        self.deferred().gpu_constants += 1;
        COUNTS[1].fetch_add(1, Relaxed);
        COUNTS[5].fetch_add(len, Relaxed);
        // SAFETY: the buffer is live and place_source keeps it until reset.
        let gpu = unsafe { buffer.GetGPUVirtualAddress() };
        Some((buffer, gpu))
    }
}

#[cfg(feature = "diagnostics")]
pub(super) fn line(frames: f64) {
    if !on() { return; }
    let counts = COUNTS.each_ref().map(|x| x.swap(0, Relaxed) as f64 / frames.max(1.0));
    tracing::info!(attempts=counts[0], copied=counts[1], capped=counts[2],
        not_recent=counts[3], not_two_sided=counts[4], bytes=counts[5],
        "GPU constant copies per frame (VICTORY_GPU_CONSTANTS=1)");
}

#[cfg(test)]
mod tests;
