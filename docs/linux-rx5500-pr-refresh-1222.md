# RX 5500 XT PR refresh and measurements, 2026-11-27

The 20 FPS objective remains open. The latest qualified build renders the
opening scene at about 5 FPS. The refresh inventories **3,297 open PRs** across
all 39 pages in the eight known repositories. It identifies 46 new or changed
heads since the complete 11:21 UTC scan. Full metadata and PR descriptions are
retained on the SSD; this does not mean every patch was source-reviewed.

Evidence: [refresh summary](../../perf20/pr-refresh-1222-summary.json),
[review matrix](../../perf20/pr-refresh-1222-review-matrix.json), and
[validation manifest](../../perf20/pr1222-validation.json). These links refer
to local investigation artifacts, outside this worktree.

## Integration and qualification

| Source | Local commit | Result |
| --- | --- | --- |
| Belobug #559: retain both native textures used by an alias copy | a90f7ae4b | Actual GPU resource-lifetime test passes |
| Belobug #565: join 16-bit global loads that cross a dword boundary | fbbe96998 | Red regression reproduced; green passes 8 hardware configurations and 384 lane results |
| Belobug #556: retain a translated shader when cache publication fails | e6321199b | Qualified: cache/descriptor tests, workspace check and app build pass |
| Belobug #562: clear a descriptor when a null CBV is requested | 964d66d8a | Qualified: cache/descriptor tests, workspace check and app build pass |

The first two fixes passed the Windows all-target workspace check, application
build and 30 targeted test functions. The raw-load regression failed before
the fix at byte offset 3: 0x00f2 was returned instead of 0x17f2. The correction
preserves half-view and native-16 metadata. Translator version 8 separates its
cache entries from earlier version-6/7 experiments; the VSH4 format is unchanged.
There is no independent WARP coverage or established gameplay speedup.

The last two imports passed an additional 22 targeted tests: 5 shader-cache,
10 descriptor, 6 actual GPU mesh-indirect and 1 list-factory tests. The Windows
all-target workspace check and application build also pass. Strict Clippy was
not run on this candidate.

The completed buffer-budget comparison used the earlier fbbe96998 binary,
SHA-256
3305fdf99c11a7c1ba1b8bd95d41238bdfd5b5c4270d9b1d4564e4dd8a71ec78.
Original authorship and upstream commit references are retained.

All main refs were fetched, then the actual default branches were checked.
Belobug uses a-publier (4eb6cfdf9, unchanged); its main branch is a diverged
older line and must not replace our integration. myokex uses vi-fx3
(aa8399e5b), which was also fetched.

## Live buffer-budget comparison

Control 20261127T124556Z uses shared mesh storage, portable mesh indirect
commands, the existing Wine memory fix and DS4 support, and a 1,024 MiB
hot-buffer budget. With compilation finished, the 120–410 second window has
51 samples: mean 189.88 ms/frame, effective 5.266 FPS, and mean 1,029.8 guest
draws/frame. Scenery and the character are visible. Existing rendering defects
and dropped pipelines remain. The run ended normally with exit 143, no kernel
messages, and at least 11.1 GiB available system RAM.

Run 20261127T130904Z changes only the buffer budget to 1,536 MiB on the same
binary. Its 51 warm samples average 196.46 ms/frame, effective 5.090 FPS, and
1,120.2 draws/frame. Scenery remains visible. It exits143 normally and logs one
AMD fence fallback timer expiration on comp_1.2.1, without an observed GPU reset.
The 1,024 MiB profile is retained: no speedup was established.

These are sequential replays of a changing scene; unequal draw counts can
confound small differences. Component timings overlap and must not be added
together. See [comparison](../../perf20/pr1222-buffer-budget-comparison.json).

## Short mesh probe after the additional fixes

Run 20261127T131957Z uses the four qualified fixes plus opt-in diagnostic
commit 5609da821, source HEAD d95c13c50. Its binary SHA-256 is
4339846d58c972b698036c412e2434b4dc5e003e1e4a7ddf302cbeea795a5d37.
The probe adds CPU counters only and preserves GPU commands and full outputs.
It returns to the original 1,024 MiB budget and ends normally after 181 world
seconds, with scenery/character visible and an empty kernel log. Final sample:
4.9 FPS/205.6 ms,1,095 draws,255 dropped,0 pipelines compiling. This short
diagnostic is not a matched full-window FPS comparison.

Of 2,990,080 observed mesh API recording calls, 367,138 are direct and 2,622,942
are indirect. Every observed indirect call has exactly one maximum record;
the 2–8,9–64 and>64 bins are zero. Application-store metadata is set on 37,070
calls. These counts are cumulative API recordings, not active GPU draws.
They rule out batching only within a multi-record call as a useful optimization
for this workload. Cross-call batching still requires hazard and lifetime
analysis; absence of buffer stores alone is insufficient.

See [follow-up validation](../../perf20/pr1222-followup-validation.json) and
[histogram](../../perf20/pr1222-mesh-histogram.json).

## Reviewed overlaps and exclusions

- Belobug #578 duplicates the half-table implementation already carried here.
  Its halves.rs is identical.
- Belobug #428's fetchguard queue-ordering protection is already present and
  active. Preserve it.
- fxceless #1066 targets serial relay merges. This fork already defaults to
  parallel LAND jobs under a shared merging lock. A direct port conflicts with
  newer landing, timeline and beside-fetch logic; the uncommitted attempt was
  reverted.
- Marvisak #3487 and #3484 use a different Viewed/heap submission architecture.
  Adapt only a measured relevant part; do not replace whole files.
- Marvisak #2472/#3197 overlap each other and our active indirect window.
  Captured argument clamps cost 0.57 ms across 131 calls; they are not the
  dominant remaining work.
- fxceless #1298's option is already explicitly enabled in the profile.
  fxceless #1466 fixes a memory.rs page-pull path absent from this fork.
- fxceless #1455 is a candidate for exact conventional CBV scratch reuse.
  Existing root-view reuse overlaps part of it; local Step variants differ.
  An adapted test must verify both replays and intervening writes on AMD.
- fxceless #1461 works around NVIDIA pipeline-library static-sampler ordering.
  That failure has not been reproduced on this Wine/RADV setup.
- Native mesh launch PRs do not apply to this adapter's software fallback.
  The 7900 #459 replacement still reports failure when entering the world.
- Belobug #569 detects section imports that return success with disconnected
  storage. Our private-allocation probe passes, while section imports fail.
  Its coherency guard does not solve imported-heap lifetime during remapping
  or leave_home. Keep in-place imports disabled pending that separate fix.

See the matrix for other dispositions, including descriptions that have not
yet received patch-level review.

## Captured GPU work

Recorder fixes from 7900 #312 and Belobug #550/#551 are qualified.
Local guard 8b653a464 keeps copy ordering but uses recordable legacy copy
barriers during capture. Capture opening-b/capture-1 from run115113 contains
44 submissions, 302 lists, 32,404 resources and 1,388 pipelines. No Barrier
call was omitted. The 938 omitted sends belong to other queues, so every
replay still needs an image comparison. The preceding three presents prime
recorded lists; they are not three captured frames.

Restored replay115956 completed normally with loop times
175.12/145.67/147.53 ms. Image comparison gives 57.3 dB PSNR and mean absolute
channel error 0.079/255; only 262 of 2,073,600 pixels differ by more than 8 in
any channel. Restore copies consume 2,396 MiB, including 1,393 MiB in system
memory, leaving only 93 MiB under the queried VRAM budget. The warning checks
used + 256 MiB against budget; it does not establish actual migration or a
hardware performance floor.

Fresh single-loop replay120356 without restore snapshots takes 165.16 ms
and gives 57.4 dB PSNR. It also removes the 13 merge copies that require
masters, so this is not an isolated measurement of VRAM headroom.

Detailed replay120559 completes normally at median150.85 ms and 57.4 dB PSNR:

| Category | GPU time | Commands |
| --- | ---: | ---: |
| Indirect dispatch | 44.68 ms | 4,062 |
| Dispatch | 32.61 ms | 6,604 |
| Indirect draw | 23.74 ms | 3,475 |
| Barriers | 14.87 ms | 15,097 |
| Card-to-PC copies | 7.66 ms | 993 |
| PC-to-card copies | 6.92 ms | 556 |
| Draws | 6.22 ms | 709 |
| Hash/compare | 5.56 ms | 949 |

No single pipeline dominates; the largest costs 5.56 ms. Detailed timestamps
add overhead, and restore-memory pressure remains. Use this ranking to choose
experiments, not as a live FPS measurement or GPU hardware floor.

## Next work

1. Qualify ordinary asynchronous list timestamps (7900 #453) to distinguish
   GPU execution from CPU copy/remap gaps without serial per-list waits.
   Isolated worktree belobug-list-stamps adapts only the timestamp portion;
   new hardware coverage checks 1,536 stamped submissions across a full ring.
   Its qualification is pending.
2. Investigate hazards for batching mesh work across calls; within-call
   batching cannot help the observed single-record workload. Preserve full per-record capacity, every draw, query and
   timestamp order, and all dependencies. Disjoint output slots may remove
   repeated compute/graphics transitions without requiring native mesh or DGC.
   Flush on any input/output hazard and retain allocations through completion.

Private game captures stay outside Git. Publishing is handled by another chat,
per the user's instruction; this thread continues local work only.
