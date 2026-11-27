# Draft for VIctory / 7900XTX — not submitted

Proposed title: Loader: bound the profiling observer's stack reservation

With RUST_MIN_STACK=134217728 set for deep translator workers, enabling the
scheduled profiler starts another 128-MiB thread stack. On the tested Wine setup,
low address space was exhausted and the game's 185-MiB graphics heap failed
before the sampling window began.

Use a named observer thread with an explicit 4-MiB stack and report a spawn
failure. Keep the sampling schedule and other threads' stack settings unchanged.
The observer does no shader translation.

Validation: under that same global setting, the native reservation regression
fails before at 134,217,728 bytes and passes after at 4,194,304. All seven
profiling tests, 24 crash/unwind tests, the optimized Windows workspace/all-target
check and application build pass. A replay with the fixed observer reaches 3D, records 582 sampling rounds over
30 seconds and completes a 301.4-second 3D observation window without a logged
exception/reset; the controller then stops it. This is a diagnostic
reliability fix; it does not establish an FPS improvement or resolve the missing
scenery.

Implementation: f918f1381 (isolated experiment branch), 2432cfb83 (clean
candidate). Check the target's current profiler before applying; keep this
separate from the existing DS4, fence and GPU allocation PRs.
