//! Who waits for whom in a frame (`VICTORY_CRITICAL_PATH=1`; off by
//! default): with the frame line, per frame on average,
//!
//! - the frame, present to present;
//! - the title's calls of `ExecuteCommandLists`: how many, the time inside
//!   them, on which threads;
//! - the time inside `Present`;
//! - each thread that records lists: its time inside our list calls (the
//!   outermost only, [`call_timer`]), its time between its first and last
//!   call of the frame outside them (the title's own work), and the rest of
//!   the frame (nothing recorded: waiting, or other work);
//! - one line of the chains a frame waits on: the submissions (inside
//!   `ExecuteCommandLists`, or our submitting thread's work with
//!   `VICTORY_ASYNC_SUBMIT`), the busiest recording thread, the relay's
//!   landings.
//!
//! A frame is cut at each present ([`frame_end`]); a thread's calls are
//! counted in the frame they end in. (From our port/v14, where it pointed
//! at the descriptor tables of `State::tables`.)

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, OnceLock};

/// Whether it is on (`VICTORY_CRITICAL_PATH=1`).
pub(crate) fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_CRITICAL_PATH").is_some_and(|v| v == "1"))
}

/// Nanoseconds since the first look.
fn now() -> u64 {
    static EPOCH: OnceLock<std::time::Instant> = OnceLock::new();
    EPOCH.get_or_init(std::time::Instant::now).elapsed().as_nanos() as u64
}

/// One thread's list calls in the frame so far.
struct Thread {
    id: u32,
    calls: AtomicU64,
    inside: AtomicU64,
    /// Its first call's start and last call's end in the frame
    /// (`u64::MAX` and 0: none).
    first: AtomicU64,
    last: AtomicU64,
    /// Summed over the frames since the line: inside, outside (between the
    /// first and last call), idle (the rest of those frames), calls, frames
    /// with calls.
    sums: [AtomicU64; 5],
}

static THREADS: Mutex<Vec<Arc<Thread>>> = Mutex::new(Vec::new());

thread_local! {
    static ME: std::cell::OnceCell<Arc<Thread>> = const { std::cell::OnceCell::new() };
    /// Depth of our list calls on this thread: only the outermost counts.
    static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn thread_id() -> u32 {
    #[cfg(windows)]
    // SAFETY: no arguments.
    return unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
    #[cfg(not(windows))]
    0
}

/// Times a list call while it lives (the outermost of the thread's).
pub(crate) struct CallTimer(Option<std::time::Instant>);

impl Drop for CallTimer {
    fn drop(&mut self) {
        if let Some(began) = self.0 {
            DEPTH.with(|d| d.set(d.get() - 1));
            list_call(began.elapsed().as_nanos() as u64);
        }
    }
}

/// A list call starts: timed if it is the thread's outermost.
pub(crate) fn call_timer() -> CallTimer {
    if !on() {
        return CallTimer(None);
    }
    let outer = DEPTH.with(|d| {
        let n = d.get();
        d.set(n + 1);
        n == 0
    });
    if !outer {
        DEPTH.with(|d| d.set(d.get() - 1));
        return CallTimer(None);
    }
    CallTimer(Some(std::time::Instant::now()))
}

/// A list call of this thread ended now, after `ns`.
fn list_call(ns: u64) {
    let end = now();
    ME.with(|me| {
        let t = me.get_or_init(|| {
            let t = Arc::new(Thread {
                id: thread_id(),
                calls: AtomicU64::new(0),
                inside: AtomicU64::new(0),
                first: AtomicU64::new(u64::MAX),
                last: AtomicU64::new(0),
                sums: [const { AtomicU64::new(0) }; 5],
            });
            THREADS.lock().unwrap_or_else(|e| e.into_inner()).push(t.clone());
            t
        });
        t.calls.fetch_add(1, Relaxed);
        t.inside.fetch_add(ns, Relaxed);
        t.first.fetch_min(end.saturating_sub(ns), Relaxed);
        t.last.fetch_max(end, Relaxed);
    });
}

/// Since the line: frames, their nanoseconds; execute calls, ns inside;
/// ns inside `Present`; our submitting thread's work; the relay's landings.
static TOTALS: [AtomicU64; 7] = [const { AtomicU64::new(0) }; 7];

/// The threads that called `ExecuteCommandLists` since the line (ids).
static EXECUTERS: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// A title call of `ExecuteCommandLists` took `ns`.
pub(crate) fn execute(ns: u64) {
    if !on() {
        return;
    }
    TOTALS[2].fetch_add(1, Relaxed);
    TOTALS[3].fetch_add(ns, Relaxed);
    let id = thread_id();
    let mut e = EXECUTERS.lock().unwrap_or_else(|e| e.into_inner());
    if !e.contains(&id) {
        e.push(id);
    }
}

/// A call of `Present` took `ns`.
pub(crate) fn present(ns: u64) {
    if on() {
        TOTALS[4].fetch_add(ns, Relaxed);
    }
}

/// Our submitting thread (`VICTORY_ASYNC_SUBMIT`) was at work for `ns`.
pub(crate) fn worker(ns: u64) {
    if on() {
        TOTALS[5].fetch_add(ns, Relaxed);
    }
}

/// The relay's thread landed flights for `ns`.
pub(crate) fn relay(ns: u64) {
    if on() {
        TOTALS[6].fetch_add(ns, Relaxed);
    }
}

/// A frame ends: each recording thread's frame is summed and starts again.
pub(crate) fn frame_end() {
    if !on() {
        return;
    }
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = now();
    let began = LAST.swap(now, Relaxed);
    if began == 0 {
        return;
    }
    let frame = now - began;
    TOTALS[0].fetch_add(1, Relaxed);
    TOTALS[1].fetch_add(frame, Relaxed);
    for t in THREADS.lock().unwrap_or_else(|e| e.into_inner()).iter() {
        let calls = t.calls.swap(0, Relaxed);
        let inside = t.inside.swap(0, Relaxed);
        let (first, last) = (t.first.swap(u64::MAX, Relaxed), t.last.swap(0, Relaxed));
        if calls == 0 {
            continue;
        }
        let span = last.saturating_sub(first.max(began)).min(frame);
        t.sums[0].fetch_add(inside, Relaxed);
        t.sums[1].fetch_add(span.saturating_sub(inside), Relaxed);
        t.sums[2].fetch_add(frame.saturating_sub(span.max(inside)), Relaxed);
        t.sums[3].fetch_add(calls, Relaxed);
        t.sums[4].fetch_add(1, Relaxed);
    }
}

/// With the frame line: the averages per frame since the last.
pub(crate) fn line() {
    if !on() {
        return;
    }
    let t: Vec<u64> = TOTALS.iter().map(|v| v.swap(0, Relaxed)).collect();
    let frames = t[0].max(1) as f64;
    let ms = |ns: u64| ns as f64 / 1e6 / frames;
    let executers: Vec<String> =
        std::mem::take(&mut *EXECUTERS.lock().unwrap_or_else(|e| e.into_inner())).iter().map(|id| id.to_string()).collect();
    let mut busy: Vec<u64> = Vec::new();
    let mut threads: Vec<(u64, String)> = THREADS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter_map(|th| {
            let [inside, outside, idle, calls, with] = [0, 1, 2, 3, 4].map(|i| th.sums[i].swap(0, Relaxed));
            if with > 0 {
                busy.push(inside + outside);
            }
            (with > 0).then(|| {
                (
                    inside,
                    format!(
                        "thread {}: inside our calls {:.1} ms ({:.0} calls), its own work between them {:.1}, idle {:.1} (in {} of {} frames)",
                        th.id,
                        ms(inside),
                        calls as f64 / frames,
                        ms(outside),
                        ms(idle),
                        with,
                        t[0]
                    ),
                )
            })
        })
        .collect();
    threads.sort_by(|a, b| b.0.cmp(&a.0));
    tracing::info!(
        "critical path per frame (VICTORY_CRITICAL_PATH=1): frame {:.1} ms; ExecuteCommandLists {:.1} calls, {:.1} ms inside on the title's threads [{}]; our submitting thread at work {:.1} ms (VICTORY_ASYNC_SUBMIT); Present {:.1} ms; recording threads: {}",
        ms(t[1]),
        t[2] as f64 / frames,
        ms(t[3]),
        executers.join(", "),
        ms(t[5]),
        ms(t[4]),
        threads.into_iter().take(8).map(|t| t.1).collect::<Vec<_>>().join("; ")
    );
    // The submissions: our thread's work with VICTORY_ASYNC_SUBMIT, else
    // the time inside the title's ExecuteCommandLists.
    let submission = if t[5] > 0 { t[5] } else { t[3] };
    tracing::info!(
        "chains per frame (VICTORY_CRITICAL_PATH=1): frame {:.1} ms; submission {:.1} ms, recording (the busiest thread) {:.1} ms, relay {:.1} ms",
        ms(t[1]),
        ms(submission),
        ms(busy.into_iter().max().unwrap_or(0)),
        ms(t[6])
    );
}

/// Times a call while it lives and hands the nanoseconds to `to`.
pub(crate) struct Timed(Option<(std::time::Instant, fn(u64))>);

impl Timed {
    pub(crate) fn new(to: fn(u64)) -> Self {
        Timed(on().then(|| (std::time::Instant::now(), to)))
    }
}

impl Drop for Timed {
    fn drop(&mut self) {
        if let Some((began, to)) = self.0 {
            to(began.elapsed().as_nanos() as u64);
        }
    }
}
