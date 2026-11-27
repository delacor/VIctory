//! Diagnostics: the card's time by kind of list in an ordinary run
//! (`VICTORY_LIST_STAMPS=1`; off by default).
//!
//! The timing run (`VICTORY_GPU_TIMING`) sends every list on its own and
//! waits for it: the relay is off, and each list costs a sending and a wake
//! (some 45 us, 13.5 ms a frame over 294 lists in run gputiming-cmds), so
//! its sums by kind are not those of the frames we play. Here the lists go
//! as they always do; a timestamp of the card's goes ahead of each run of
//! lists of one kind (a prelude, the title's list, the steps on the card,
//! the compare) and behind the last, in the same calls. The time between
//! two stamps is the kind's; between one submission's last stamp and the
//! next one's first lie what goes on the queue apart from the submissions'
//! lists (the sends up to copies in the card's memory, a present's lists)
//! and the card standing.

use std::collections::VecDeque;

use windows::core::Interface;

use super::*;
use crate::copystat::Timed;

/// Whether it is on (`VICTORY_LIST_STAMPS=1`).
pub(super) fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_LIST_STAMPS").is_some_and(|v| v == "1"))
}

/// Stamps in the ring: some 320 a frame, and three frames in the air.
const SLOTS: usize = 4096;

/// The kinds summed, as their names on the line.
const KINDS: [&str; 5] = ["preludes", "the title's lists", "steps on the card", "compares", "others of ours"];

fn kind(t: Timed) -> usize {
    match t {
        Timed::Prelude => 0,
        Timed::Title(_) => 1,
        Timed::Steps => 2,
        Timed::Compare => 3,
        Timed::Other => 4,
    }
}

struct Stamps {
    /// A list for each slot: a timestamp, and its way into `back`.
    lists: Vec<ID3D12CommandList>,
    /// The mapping of what they write, a tick count each.
    back: usize,
    _kept: (ID3D12CommandAllocator, ID3D12QueryHeap, ID3D12Resource),
}

// SAFETY: free-threaded objects, and a mapping only read and zeroed by us
// once the card is done with the slot.
unsafe impl Send for Stamps {}
// SAFETY: as above.
unsafe impl Sync for Stamps {}

static STAMPS: OnceLock<Option<Stamps>> = OnceLock::new();

fn stamps() -> Option<&'static Stamps> {
    STAMPS
        .get_or_init(|| {
            let made = host().ok_or_else(windows::core::Error::empty).and_then(Host::make_list_stamps);
            made.inspect_err(|e| tracing::warn!(error = %e, "no stamps between lists (VICTORY_LIST_STAMPS)")).ok()
        })
        .as_ref()
}

/// A submission's stamps: its first slot, the kind of each run of lists
/// (one slot each, and one more behind the last), and the fence value it is
/// signalled with (0 until sent).
struct Pending {
    first: usize,
    kinds: Vec<usize>,
    fence: u64,
}

#[derive(Default)]
struct Sums {
    /// Ticks and runs of lists by kind.
    ticks: [u64; 5],
    runs: [u64; 5],
    /// Ticks between submissions, and the last stamp read.
    between: u64,
    last: u64,
    submissions: u64,
    /// Submissions left out for want of free slots, or read with a stamp
    /// missing or out of order.
    left_out: u64,
    unread: u64,
}

struct State {
    next: usize,
    in_use: usize,
    pending: VecDeque<Pending>,
    sums: Sums,
}

static STATE: Mutex<State> =
    Mutex::new(State { next: 0, in_use: 0, pending: VecDeque::new(), sums: Sums { ticks: [0; 5], runs: [0; 5], between: 0, last: 0, submissions: 0, left_out: 0, unread: 0 } });

/// What goes to the queue in place of `lists`: the old lists by their
/// places, and `None` for a stamp ahead of each run (by `starts`, the run's
/// first list; increasing, the first 0) and behind the last.
pub(super) fn order(len: usize, starts: &[usize]) -> Vec<Option<usize>> {
    let mut out = Vec::with_capacity(len + starts.len() + 1);
    for (j, &from) in starts.iter().enumerate() {
        let to = starts.get(j + 1).copied().unwrap_or(len);
        out.push(None);
        out.extend((from..to).map(Some));
    }
    out.push(None);
    out
}

/// Where old place `at` is in [`order`]'s: a run's first at the stamp ahead
/// of it (a cut there keeps the stamp with its run).
pub(super) fn moved(starts: &[usize], at: usize) -> usize {
    // (Found: the runs before it; not found: the runs it lies behind.)
    at + starts.binary_search(&at).unwrap_or_else(|j| j)
}

/// The ticks of each run by kind, from a submission's stamps (one more than
/// runs); `None` if a stamp is missing (0) or out of order.
pub(super) fn spans(ticks: &[u64], kinds: &[usize]) -> Option<[u64; 5]> {
    if ticks.len() != kinds.len() + 1 || ticks.contains(&0) || ticks.windows(2).any(|w| w[1] < w[0]) {
        return None;
    }
    let mut out = [0; 5];
    for (k, w) in kinds.iter().zip(ticks.windows(2)) {
        out[*k] += w[1] - w[0];
    }
    Some(out)
}

/// Puts stamps between the runs of `lists` (`marks`: where each run starts,
/// and its kind) and moves `firsts` with them. `None` (nothing changed) if
/// there are no free slots.
pub(super) fn put(lists: &mut Vec<Option<ID3D12CommandList>>, marks: &[(usize, Timed)], firsts: &mut [usize]) -> Option<()> {
    let stamps = stamps()?;
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for &(at, t) in marks.iter().filter(|m| m.0 < lists.len()) {
        match runs.last_mut() {
            Some(last) if last.0 == at => last.1 = kind(t),
            _ => runs.push((at, kind(t))),
        }
    }
    if runs.first().is_none_or(|r| r.0 != 0) {
        return None;
    }
    let need = runs.len() + 1;
    let first = {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        read(&mut state, stamps);
        if state.in_use + need > SLOTS {
            state.sums.left_out += 1;
            return None;
        }
        let first = state.next;
        state.next = (first + need) % SLOTS;
        state.in_use += need;
        state.pending.push_back(Pending { first, kinds: runs.iter().map(|r| r.1).collect(), fence: 0 });
        first
    };
    let starts: Vec<usize> = runs.iter().map(|r| r.0).collect();
    let old = std::mem::take(lists);
    let mut slot = first;
    *lists = order(old.len(), &starts)
        .into_iter()
        .map(|i| match i {
            Some(i) => old[i].clone(),
            None => {
                let list = stamps.lists[slot].clone();
                slot = (slot + 1) % SLOTS;
                Some(list)
            }
        })
        .collect();
    for f in firsts.iter_mut() {
        *f = moved(&starts, *f);
    }
    Some(())
}

/// The submission whose stamps [`put`] took last was signalled with `fence`.
pub(super) fn sent(fence: u64) {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(p) = state.pending.back_mut().filter(|p| p.fence == 0) {
        p.fence = fence;
    }
}

/// Reads the stamps of submissions the card has finished, in order.
fn read(state: &mut State, stamps: &Stamps) {
    let Some(host) = host() else {
        return;
    };
    // SAFETY: a plain query.
    let done = unsafe { host.fence.GetCompletedValue() };
    let back = stamps.back as *mut u64;
    while let Some(p) = state.pending.front().filter(|p| p.fence != 0 && p.fence <= done) {
        let n = p.kinds.len() + 1;
        let ticks: Vec<u64> = (0..n)
            .map(|k| {
                let at = (p.first + k) % SLOTS;
                // SAFETY: inside the mapping of `SLOTS` tick counts; the card
                // wrote it before the fence, and nothing writes it until the
                // slot is handed out again.
                unsafe {
                    let t = back.add(at).read_volatile();
                    back.add(at).write_volatile(0);
                    t
                }
            })
            .collect();
        let s = &mut state.sums;
        match spans(&ticks, &p.kinds) {
            Some(by) => {
                for (k, t) in by.iter().enumerate() {
                    s.ticks[k] += t;
                }
                for &k in &p.kinds {
                    s.runs[k] += 1;
                }
                if s.last != 0 && ticks[0] >= s.last {
                    s.between += ticks[0] - s.last;
                }
                s.last = ticks[n - 1];
                s.submissions += 1;
            }
            None => {
                s.unread += 1;
                s.last = 0;
            }
        }
        state.in_use -= n;
        state.pending.pop_front();
    }
}

/// For the frame-rate line.
pub(super) fn line(frames: f64) {
    if !on() {
        return;
    }
    let (Some(stamps), Some(host)) = (stamps(), host()) else {
        return;
    };
    let sums = {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        read(&mut state, stamps);
        let last = state.sums.last;
        std::mem::replace(&mut state.sums, Sums { last, ..Sums::default() })
    };
    // SAFETY: a plain query.
    let frequency = unsafe { host.queue.GetTimestampFrequency() }.unwrap_or(0);
    if frequency == 0 || frames <= 0.0 {
        return;
    }
    let ms = |ticks: u64| ticks as f64 * 1e3 / frequency as f64 / frames;
    let total: u64 = sums.ticks.iter().sum();
    let kinds: Vec<String> =
        KINDS.iter().enumerate().map(|(k, name)| format!("{name} {:.1} ms in {:.0}", ms(sums.ticks[k]), sums.runs[k] as f64 / frames)).collect();
    tracing::info!(
        "the card's time by kind of list, as the lists ordinarily go (VICTORY_LIST_STAMPS=1), per frame: {:.1} ms in the \
         submissions' lists: {}; between submissions {:.1} ms (our sends up to copies in the card's memory, a present's \
         lists, and the card standing); {:.1} submissions stamped, {:.1} left out (no free stamps), {:.1} unread",
        ms(total),
        kinds.join(", "),
        ms(sums.between),
        sums.submissions as f64 / frames,
        sums.left_out as f64 / frames,
        sums.unread as f64 / frames,
    );
}

impl Host {
    fn make_list_stamps(&self) -> windows::core::Result<Stamps> {
        let Some((buffer, back)) = self.shared_buffer(8 * SLOTS as u64) else {
            return Err(windows::core::Error::from(windows::Win32::Foundation::E_OUTOFMEMORY));
        };
        // SAFETY: plain object creation; a fresh mapping of that size.
        unsafe {
            std::ptr::write_bytes(back, 0, 8 * SLOTS);
            let mut heap: Option<ID3D12QueryHeap> = None;
            self.device.CreateQueryHeap(
                &D3D12_QUERY_HEAP_DESC {
                    Type: D3D12_QUERY_HEAP_TYPE_TIMESTAMP,
                    Count: SLOTS as u32,
                    NodeMask: 0,
                },
                &mut heap,
            )?;
            let heap = heap.ok_or_else(windows::core::Error::empty)?;
            let allocator: ID3D12CommandAllocator = self.device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)?;
            let mut lists = Vec::with_capacity(SLOTS);
            for at in 0..SLOTS as u32 {
                let list: ID3D12GraphicsCommandList =
                    self.device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)?;
                list.EndQuery(&heap, D3D12_QUERY_TYPE_TIMESTAMP, at);
                list.ResolveQueryData(&heap, D3D12_QUERY_TYPE_TIMESTAMP, at, 1, &buffer, u64::from(at) * 8);
                list.Close()?;
                lists.push(list.cast()?);
            }
            tracing::info!(slots = SLOTS, "stamps between lists made (VICTORY_LIST_STAMPS=1)");
            Ok(Stamps { lists, back: back as usize, _kept: (allocator, heap, buffer) })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stamp_goes_ahead_of_each_run_and_behind_the_last() {
        // Runs at 0 (a prelude and its list), 2, 3.
        let out = order(5, &[0, 2, 3]);
        assert_eq!(out, vec![None, Some(0), Some(1), None, Some(2), None, Some(3), Some(4), None]);
    }

    #[test]
    fn a_runs_first_moves_to_its_stamp() {
        let starts = [0, 2, 3];
        let out = order(5, &starts);
        for &s in &starts {
            let at = moved(&starts, s);
            assert_eq!(out[at], None, "the stamp of the run at {s}");
            assert_eq!(out[at + 1], Some(s));
        }
        // A place inside a run keeps its list.
        assert_eq!(out[moved(&starts, 1)], Some(1));
        assert_eq!(out[moved(&starts, 4)], Some(4));
    }

    #[test]
    fn runs_get_the_ticks_up_to_the_next_stamp() {
        assert_eq!(spans(&[10, 15, 40, 41], &[0, 1, 1]), Some([5, 26, 0, 0, 0]));
        assert_eq!(spans(&[10, 12, 20], &[3, 2]), Some([0, 0, 8, 2, 0]));
    }

    #[test]
    fn a_missing_or_backward_stamp_is_not_read() {
        assert_eq!(spans(&[10, 0, 20], &[1, 1]), None);
        assert_eq!(spans(&[10, 30, 20], &[1, 1]), None);
        assert_eq!(spans(&[10, 20], &[1, 1]), None);
    }
}
