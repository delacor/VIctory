# Linux RX 5500 XT investigation

Updated 2026-11-27 (local date; run timestamps below are UTC). This is a working engineering report, not a release claim.
The target is sustained 20 real presentation frames per second in the same opening
3D sequence, with correct scenery and the existing DS4 support retained.

## Current result and limits

The latest fixed-setting replay completes its six-minute 3D window with a
median 5.9 FPS in compilation-idle, unprofiled frame-log intervals (5.2–6.4).
Geometry and lighting are still incomplete. Sustained 20 FPS has not been
achieved or verified. Details, earlier controlled A/B gains and rejected
experiments follow.

The initial Series S trial reached approximately 4.6–4.9 FPS after compilation
settled, then crashed after about five minutes. A replay with the fence/compositor
corrections completed a six-minute 3D observation window without that exception
or a GPU reset: 66 compilation-idle frame-log samples had median 3.9 FPS
(range 3.5–5.5). It was then stopped by the controller (exit 143).
The later barycentric experiment renders the character and surrounding scene,
but geometry and lighting remain incomplete. The original experiment ran around
3–4 FPS; allocation changes below subsequently improve timing. No result here
establishes complete rendering or sustained 20 FPS.

The pooled-view-buffer A/B run now measures a warmed improvement at comparable
draw counts: 267.7 ms/frame (3.74 FPS) with committed buffers versus
194.0 ms/frame (5.15 FPS) with pooled allocations. This uses three completed
30-second off/on pairs, each starting at least 90 seconds after first 3D,
with mean draw counts differing by less than 5%. No compiler or sampling
profiler ran during those pairs. The ray-tracing structure builder remains
disabled by the fork's default on both sides; this is not a full-feature or
20-FPS verification.

The separate mesh-sizing A/B, with view pooling fixed on, measured 210.7 ms/frame
(4.75 FPS) with 16-MiB first chunks versus 177.8 ms/frame (5.62 FPS) with 1-MiB
first chunks. Four warm matched pairs contain 553 off frames and 654 on frames.
This is a within-run comparison; do not add gains from different runs.

A previous run showed incomplete scenery. Correctness must be checked alongside
timing: black frames, omitted draws, or missing geometry cannot count as a gain.
A later diagnostic run visually set Subtitles to On; HUD was unchanged.
Menu/settings interactions are excluded from scene timing windows.

## Environment and reproducibility

- AMD Ryzen 7 3700X, RX 5500 XT 8 GB, approximately 36 GB system RAM.
- Linux 7.3.5, Mesa 25.3.6, Wine 11.0 staging (Fedora 11.0-2.fc43).
- Game data on SSD. Runtime prefixes, caches, logs, and derived captures are
  outside the repository. No game assets, shader dumps, screenshots, crash
  report payloads, or proprietary code are included in this report.
- Windows GNU LLVM target; Rust 1.98.1 and LLVM MinGW 20261117. Diagnostic
  builds use optimization level 3 with line information.
- The measured branch perf20/linux-rx5500 starts at Belobug's a-publier
  cd8dfc06a. The last measured code is d9e5ad914 (view pooling plus mesh sizing).
  A separate refresh branch perf20/belobug-refresh-20261127 merges a-publier
  4eb6cfdf9 at 334df1b17; its runtime validation is tracked separately below.
  The earlier measured source and runtime remain available for comparison.
- Private vkd3d-proton build: official commit
  71cae091b06571e51446cf2a937f0b04c237c04d. Private Wine server described below.
  Neither replaces the system installation.

For a comparable replay, keep the same sequence and checkpoint; record executable
commit, DLL versions, console mode, output dimensions, all VICTORY_* variables,
shader backlog, draw/pipeline refusal counts, memory use, and whether scenery is
complete. Use the application's frame log after compilation settles, not a menu's
FPS or a synthetic microbenchmark. Do not build in parallel with the game.

The latest trial uses --console series-s and VICTORY_OUTPUT=1920x1080. This is
Xbox Series S/Lockhart mode, not Xbox One S. An earlier XML-only resolution change
did not change the actual output, so the CLI/runtime log and output dimensions
must be checked.

## Findings supported by measurements

### Wine virtual-memory remapping

The existing Wine 11 server kept process memory views in a linear list. The
private server backports Wine commit
3217449b1f719c8f7a73a6de768df536495ddcbf (view tree). At 65,536 views:

| Synthetic operation | Original server | Patched server |
| --- | ---: | ---: |
| Populate views | 11,556.854 ms | 2,520.138 ms |
| Remap 512 views | 292.271 ms | 27.275 ms |

Alias correctness checks passed. This is a microbenchmark, not an FPS comparison.
The user also observed approximately 70% shorter loading and more frequent scene
updates after the memory work. SSD speed alone does not explain the earlier
multi-second stalls.

The server is the matching Fedora Wine 11/staging source, with only this server
change, bound over /usr/bin/wineserver64 inside the private bubblewrap capsule.
Setting WINESERVER alone did not select the patched binary. The backport patch
and source remain in the private runtime directory; this is an environment
dependency, not a VIctory source change.

Exposing /dev/ntsync to the same capsule reduced a separate 10,000-event
round-trip probe from 4.868950 s to 0.722888 s. The real process's descriptor
was checked. Again, this is not an equivalent FPS gain.

### Current CPU cost

A 30-second, 199-Hz CPU profile was captured after the latest shader backlog
reached zero. It contained 29,343 samples, including 11,992 attributed to VIctory.
Leading inlined owners included:

| Owner | Samples | Interpretation |
| --- | ---: | --- |
| safecopy::differ::blocks | 2,653 | Comparing memory remains expensive |
| victory_safe_copy_at | 797 | Protected memory copying |
| texture::digest | 638 | Texture change detection |
| victory_safe_merge | 578 | GPU-to-title merge |
| BTreeMap::retain in shadow::exact | 526 | Cache maintenance under a shared lock |
| HostHeap::make_listed | 465 | Descriptor allocation |

These are sampled CPU costs, not percentages of wall-frame duration. The frame
log reported roughly 115–140 ms preparation and 88–100 ms copying back about
46 MB in this run; categories can overlap and must not be added as a frame budget.
Shader compilation and decompression were not the leading steady-state costs.

The exact shadow cache currently walks its entire map on every miss once it has
256 entries. It retained around 2,500 entries while admitting about 280–317
new entries per frame. A second 30-second profile with the barycentric experiment confirms the same
pattern: 27,255 samples, 11,624 in VIctory, including 2,524 in differ::blocks
and 543 in the exact-cache retain walk.

Local commit 37d89d3b5 sweeps once per eight misses, preserving the existing
reference/age eligibility checks and delaying reconsideration by at most seven
new views. Its regression uses actual D3D12 resources to check live Arc references,
live host-view references, recency and eventual idle reclamation. The real-resource ownership/reclamation test and optimized Windows test build
pass, as does the optimized Windows application build. The cache-only replay completed 360 seconds in 3D without an exception or GPU
reset and had median 3.9 FPS (range 3.4–5.6 over 65 compilation-idle log samples).
Its initial CPU profile had different draw/decompression load, so a causal FPS
gain is not established for the sweep batching alone.

### Exact-view allocations and GPU mapping waits

The cache-only replay created over 300,000 small shared-buffer resources.
Every committed buffer passes through the fork's resident/mapped path, which
submits a small copy to a separate queue and waits for its fence. The cumulative
mapping-wait statistic includes lock waits across threads; it must not be added
to the wall-frame budget.

Upstream 6cc6aa352 addresses this class of allocations with shared arena slices.
This fork's descriptor slots retain ID3D12Resource references, and its exact cache
uses their per-resource reference counts for eviction. Local b7a32f2ed instead
reuses the existing placed-resource pool with CUSTOM WRITE_BACK/L0 heaps:

- Each view remains a distinct resource of its original width, starting at its
  own byte zero. Descriptors and copy-back offsets retain their existing meaning.
- New heaps are made resident and waited for before publishing any placed
  resource. Reusing a heap does not repeat that wait.
- A COM reclamation object returns a piece after the last resource reference
  goes. Existing resting/reclamation rules keep the heap alive.
- Reused resource bytes are explicitly zeroed, including the missing-title-page
  case. A refused placement falls back to the original committed-buffer path.
- VICTORY_VIEW_POOL=0 selects the original path. Diagnostic builds can alternate
  it through VICTORY_AB=30,VICTORY_VIEW_POOL.

An idle-GPU benchmark of 256 small buffers measured 148.03 us/resource for
committed buffers and 29.90 us/resource for placed buffers, including one new
shared heap. This is approximately 5 times faster allocation, not 5 times the FPS.
Twelve focused correctness checks passed: eleven pool checks (including actual
GPU UAV writes/readback and COM lifetime) plus the existing misaligned-view
descriptor/refresh regression. Optimized Windows builds passed.

The live A/B timing above is a separate measurement. No exception or GPU fault
was logged during its six-minute 3D observation window. Complete rendering is
not established.

The existing copy benchmark also checked 128 MiB of scattered source memory.
For 64-KiB pieces, protected rep-movsb copied into a GPU-readable write-back
buffer at 8.3 GB/s and into plain memory at 8.4 GB/s (memcpy 9.0/9.1 GB/s).
This does not support treating the shared destination as slow uncached memory.
Smaller-piece measurements and their overhead remain in the private logs.

### Mesh scratch size and allocation churn

The fork's mesh pools began with 16 MiB even for a few command words. Imported
allocator regressions from upstream 9398d08 reproduced that policy: both failed
before the change, including an observed [16 MiB,128 MiB] allocation sequence
where [1 MiB,128 MiB] was expected.

Local d9e5ad914 starts at 1 MiB and doubles later chunks up to the existing
16-MiB growth limit. A larger requested region still gets its required
power-of-two allocation; no geometry or group count is reduced. The fork
already recycles after each draw, so its recycling, global free-pool limit and
delayed retirement are retained rather than importing upstream's pool-wrap
policy.

All three allocator checks pass after the change, including preservation of
UAV state across recycling. The optimized Windows application build passes.
VICTORY_MESH_SMALL_CHUNKS=0 selects the old sizing; diagnostics can alternate
it. Live timing and memory comparison are pending.

### Rejected write-watch experiment

A standalone 128-MiB write-watch probe compared atomic GetWriteWatch/reset with
GetWriteWatch followed by selective ResetWriteWatch, checking dirty addresses and
that the reset really cleared them:

| Dirty pages | Atomic/reset | Selective reset |
| --- | ---: | ---: |
| None | 0.064825 ms | 0.058425 ms |
| One in sixteen | 0.220950 ms | 4.664305 ms |
| All | 2.003145 ms | 2.103315 ms |

Selective resetting was approximately 21 times slower in the sparse case.
VICTORY_ASK_NORESET was therefore not enabled. These results reject this
configuration on this Wine build; they do not prove every write-watch strategy
is unsuitable.

### Crashes and retained logs

An earlier trial was killed by the global OOM handler. Memory pressure included
other session processes, so it must not be misdiagnosed as a shader/GPU crash.

The crashed Series S trial, started at 22:15:58 UTC, raised an access violation
at approximately 22:21:14 UTC. The first reported frame was ntdll+0x3c268,
with application frames in queue::behind::Lane::push_behind's immediate
callback and queue::signal. A second exception occurred in DbgHelp while reporting
the first. No GPU reset appeared in the corresponding kernel log. Wine returned
exit status zero despite the confirmed crash; exit status alone is insufficient.

Queued waits/signals stored raw emulated-fence pointers without retaining COM
ownership. That is a real lifetime defect. It is a plausible contributor to this
crash, not a proven diagnosis. The ownership fix must survive a replay beyond the
previous failure point before it is described as a crash fix. The follow-up ran for 363.8 seconds
in 3D without the exception, then was stopped deliberately. This is one
encouraging observation, not proof of the suspected cause.

### Refused graphics pipelines: barycentric requirement

Offline replay selected eight actual refused pipeline records from the latest
run, including one vertex/pixel pipeline and seven mesh-fallback/pixel pipelines.
All eight reproduced the same vkd3d rejection: the translated pixel program
requires barycentrics, but the device capability is not exposed. This is a
specific cause for those eight failures, not proof about every refused pipeline.

RADV's native fragment-barycentric feature is documented for GFX10.3 and later
([Mesa release notes](https://docs.mesa3d.org/relnotes/23.2.1.html)).
The tested RX 5500 XT is RDNA1. Merely supplying a mesh compute fallback does not
remove a pixel program's separate barycentric requirement.

The private vkd3d source already has an AMD explicit-vertex-parameter path,
normally gated by an application compatibility profile
([official vkd3d source](https://github.com/HansKristian-Work/vkd3d-proton/blob/71cae091b06571e51446cf2a937f0b04c237c04d/libs/vkd3d/device_workarounds.c)).
An experiment adds an explicit, default-off environment opt-in to that existing
path while retaining its AMD, extension, and subgroup guards. It does not enable
the profile's unrelated VRS override.

The 13 existing barycentric tests pass with this opt-in, including actual
hardware pixel readback of partial interpolation. All eight selected cached
pipelines then create successfully, versus zero of eight before the opt-in.
The subsequent live capture shows the character and surrounding scene.
This is a rendering improvement, not complete-image validation or an FPS gain.
The exact experimental patch is saved as
[plain-text backend patch](patches/vkd3d-rdna1-barycentric-opt-in.txt).

The backend experiment is not promoted as a full native-feature replacement. The converter calls
these barycentrics unstable and has incomplete centroid/per-sample handling.
Further coverage, especially clipped primitives and non-center sampling modes,
is still required; pipeline creation alone cannot establish image correctness. No shader feature requirement
is erased and no valid draw is deliberately skipped to improve a timing result.

## Code and provenance

Existing fork work is retained with its author history where practical. Imports
below are not presented as newly authored fixes.

| Change | Source / local commit | Validation or limitation |
| --- | --- | --- |
| Wine DXC path for built-in shaders | Existing local Wine support, fe10cc8213 | Three DXC tests and the GPU compare shader passed |
| Copy-region bounds checks | 7900XTX PR #351 | Imported safeguard; not independently an FPS result |
| Earlier freed-shadow reclamation | 7900XTX #271, 02076dea84 | Imported |
| AVX2 compare / avoid redundant references | 7900XTX #376, 4cb48b4029 | Imported; compare still a leading cost |
| Cached hole answers with range invalidation | 7900XTX #377, all three commits | Two targeted hole tests passed after Wine fallback |
| Bounded pools / lazy allocator retirement | Belobug #335, 2559ee63da | Imported via 68815969c |
| Root signatures from HLSL source under Wine | 7900XTX #338, a8c28bac2 | Adapted in ad4f29a57; sampler gap found below |
| Direct HID DualShock 4 support | Existing VIctory #1786 work | Port cf004525f; no controller connected in latest trial |
| Conservative Wine placeholder query | 7900XTX #200, 1b06b56d5 | bad77cb49; two hole tests passed |
| RDNA1 mesh compute fallback | 7900XTX #20, five-commit series | 957dcd3ac; four mesh and four indirect GPU tests passed after correction below |
| Multi-group mesh index bounds | New correction, 978fd7402 | Second-group indices failed before, passed after |
| Wine Winsock missing-export handling | VIctory #3091 | e821e6440; affected network test passed |
| Empty pipeline-library blob handling | 7900XTX #151, both commits | c0d3415e6 + 50ec3f9e9; all five library tests passed |
| Frame unwind guard | 7900XTX #375, 71eb8c58b4 | 95e95d5c1; included in both subsequent six-minute observation windows |
| Fence ownership | fxceless #404, plus local async-lane adaptation | 80548d943 + 7fe2b6869; held ownership and six async-lane tests passed |
| Linear sampler in source root-signature reader | Local correction to the #338 port | 1bedfd6ce; signature unit test and GPU pixel readback passed |
| Batched exact-view cache maintenance | New correction, 37d89d3b5 | Real-resource test passed; cache-only replay median 3.9 FPS, effect not isolated |
| Placed exact-view buffer pool | Fork adaptation, b7a32f2ed; upstream arena motivation 6cc6aa352 | Twelve focused correctness checks; warm A/B 3.74 to 5.15 FPS |
| Small first mesh scratch chunk | Adapted from upstream 9398d08, d9e5ad914 | Two allocator tests fail before; three pass after; live timing pending |

The earlier stable integration also preserves the narrowed VOP1-only shader
decoder fix (f4d713fd9) and the split HLE page/shown versus physical-map locks
(8f20ff17e). That HLE concurrency regression failed before and passed after.
Those changes are not all transplanted wholesale into the different Belobug base;
the fork already contains different memory and shader implementations.

### Newly authored mesh correction

The compute fallback previously added group_index * max_vertices before checking
an index against max_vertices. As a result, valid local indices in every group
after the first failed the bounds check and were replaced with zero. The fix
checks local indices and the whole primitive's cull condition first, then adds
the group offset. It preserves invalid-index culling.

A two-group synthetic GPU test reproduced the second group's zeroed indices
before the change, and passed with the fix. The other mesh image tests now retain
their fallback coverage checks even when the device cannot provide a native mesh
reference. On RX 5500 XT that native comparison is unavailable; it is not reported
as tested. This correction does not establish complete rendering of this title.

### Fence ownership correction

FenceRef retains/releases the emulated COM fence. Waiting queues and host late
signals own it until completion. The async lane must also own it in its Wait,
Signal, and joined-signal jobs, including ownership transfer into a late signal.
The local extension uses the owning type directly in all three async job variants,
without a redundant raw pointer.

The imported queue ownership regression passes. All six async-lane tests pass,
including a regression that releases the caller reference and checks final
release for Wait, Signal, and joined-signal jobs. The crash replay is a separate
validation step: the six-minute observation window passed without the earlier
exception, and further repetitions are still needed.

### Compositor correction

The source root-signature reader did not recognize FILTER_MIN_MAG_MIP_LINEAR,
which the existing two-plane compositor uses. The runtime warned about that word
and fell back to showing the base plane only. The missing enum mapping is now added. The actual-signature unit regression
and existing synthetic scaled/premultiplied red-green-blue GPU readback test
both pass; the readback is [128,0,0,255, 64,128,0,255, 0,0,255,255]. This is a correctness fix with a specific observable failure, not a claimed
FPS optimization.

## PR refresh coverage and compatibility

The public PR inventory was paginated across the repositories, not limited to
the newest 100 entries. The local inventory contains 2,984 open entries across
Marvisak/VIctory (1,547), fxceless/victory-fork (916), Belobug (200), 7900XTX (182),
DexNom (114), myokex (22), ZJOU (0), and the public DoubleGhosty/cool_fox mirror
(3). Counts are a changing snapshot; the 7900XTX update-ordered pagination is not
a transactional count. Metadata/title/body triage is not a claim to have reviewed
or tested all 2,984 code patches.

origin/main was fetched at 63bcca43c and fxceless/main at 52ac4ec0f. The attempted
merge of the new upstream into the earlier all-PR stack is quarantined on
perf20/upstream-20261126: it has 178 shader type errors and is not the runnable
candidate. The previous stable integration at 8f20ff17e is retained. Compatible
selected fixes are being tested on perf20/linux-rx5500; not every indexed PR
has been merged.

## Validation ledger

- Earlier stable native suites: six libraries, 364 passed / 8 ignored;
  shader suite 274 passed / 3 ignored; HLE 252 passed / 1 ignored.
  These are earlier-branch results, not proof for the entire current candidate.
- Belobug translation corpus: 656/656 shaders emitted and validated, versus
  447/656 on the earlier stable emitter. This corpus has no mesh shaders and
  validates compilation, not graphical equivalence.
- Current candidate before the fence/compositor edits: four mesh GPU
  tests, four indirect GPU tests, two hole tests, five pipeline-library tests,
  and the affected Wine Winsock test passed.
- A broader HLE run had a filesystem-dependent link-count failure; it has not
  been labelled fully passing.
- Windows workspace/all-target check with the fence/compositor edits: passed.
- Focused Windows/Wine checks: nine passed (one held-fence regression, six
  async-lane tests including the new ownership regression, one signature parser
  test, and one actual GPU compositor image/readback test).
- Optimized diagnostic Windows application and GPU test builds: passed.
- Fence/compositor replay: six-minute 3D observation window completed without
  the prior exception or a logged GPU reset, then deliberately terminated by
  the controller. This does not retroactively make the earlier crash a clean exit.
- Private barycentric backend: 13 tests passed, including actual hardware
  rendered-pixel checks; eight previously refused pipeline records now create.
  Full scene correctness is not established.
- Batched exact-cache maintenance: optimized Windows test build and actual
  D3D12 resource-ownership/reclamation test passed. Optimized Windows application
  build passed; six-minute cache-only replay completed. A causal FPS gain is not
  established for batching alone.
- Placed exact-view buffer pool: twelve focused correctness checks and optimized
  Windows builds pass; allocation microbenchmark and three warm live A/B pairs
  measured separately.
- Small first mesh scratch chunk: two fail-before allocator regressions; all
  three after-change checks and optimized Windows application build pass.
  Four warm matched live pairs measured 210.7 versus 177.8 ms/frame.
  The initial live-memory gauge is invalid; see its correction below.

## Submission plan

Use the separate PR drafts beside this report. Submit focused corrections on a
branch based on the intended repository, retaining attribution and only the
prerequisites it needs. Do not upload this entire experimental fork as a single
performance PR.

The mesh correction depends on a compute fallback being present. The sampler fix
depends on the source root-signature reader. Fence ownership overlaps existing
fxceless #404 / upstream lifetime work; adapt or contribute to the existing PR
rather than duplicate it. DS4 #1786 already exists and should not get a duplicate PR.

Before upload: refresh the target PR state, reproduce each regression on its
intended base, run the focused tests, and replace all pending claims with actual
results. Keep local crash reports, CPU profile data, shader cache files, and
screenshots outside Git.

## Target-base check for the cache change

The fetched Marvisak/VIctory main at 63bcca43c already has a different solution:
weak cache entries, a size-based pruning threshold, and shared arena buffers
(commit [6cc6aa352](https://github.com/Marvisak/VIctory/commit/6cc6aa352)).
The fetched 7900XTX main still has the strong-reference per-miss retain scan
used by the candidate. Therefore 37d89d3b5 is a focused 7900XTX/fork stopgap,
not a proposed duplicate fix for Marvisak main.

Porting upstream's arena is a broader option, but requires preserving slice
offsets, descriptor first-element calculations, refreshes and buffer lifetimes
throughout this fork. It must not be reduced to copying the map declaration.
Prefer that redesign if it can be adapted and validated; retain the bounded
sweep as a small independently reviewable change in the meantime.

## Remaining rendering gaps

The live log still reports draws without a host pipeline. Device creation slots
0x1c8 and 0x218 are explicitly unimplemented and return failure with a null output.
Their descriptor layout has not been established. Do not turn these into fake
successful calls: that would hand invalid pipeline objects to the title. Diagnose
these separately from the now-reproduced barycentric capability rejection.

## Remaining omitted work: a diagnostic-label correction

The frame line labels the entire DROPPED counter as without a host pipeline.
That counter also includes intentionally disabled stages and binding failures.
The detailed warmed run identifies the remaining roughly 220–270 omissions
per frame as stages of the ray-tracing structure builder, which is off by
default unless VICTORY_BVH_BUILDER=1. These are not hundreds of new graphics
PSO creation errors.

Keep this omission explicit when reporting FPS. Testing the builder and its
dependencies is separate from the allocation A/B comparison; neither side of
that comparison changes its setting. The unimplemented device slots above are
a separate confirmed limitation, not an established explanation of this
particular counter.

## Full refresh at 2026-11-26 23:42 UTC

All open-PR pages were fetched again, in creation order, across all eight
repositories: 3,022 unique entries, no duplicate entries or request errors.
This remains metadata/title/body triage, not validation of every code patch.

| Repository | Open entries | Pages |
| --- | ---: | ---: |
| Marvisak/VIctory | 1,566 | 16 |
| fxceless/victory-fork | 933 | 10 |
| Belobug/VIctory-Belobug-AMD | 194 | 2 |
| avatarmanz/VIctory-7900XTX | 191 | 2 |
| DexNom/VIctory | 115 | 2 |
| myokex/victory-amd | 22 | 1 |
| FQY55kai/ZJOU-VICTORY | 0 | 1 |
| DoubleGhosty/cool_fox | 1 | 1 |

PR lists can change while paging; these are a complete paginated sweep, not an
atomic snapshot. Raw refreshed metadata is outside the source tree.

All eight known fork branches were fetched as well. Relevant new heads include
fxceless/main 4c6046614, Belobug/a-publier 4eb6cfdf9, 7900XTX/main 014d52ded,
DexNom/main 8114ff779 and cool_fox/main ee11ca780. Marvisak main remains 63bcca43c.
The candidate is not yet merged with every newly fetched change.

The new fxceless default for one group per GPU hash piece was checked against
the candidate's actual kernel: this fork already uses one 64-thread group per
piece, so importing that default change is unnecessary. New Belobug work
includes descriptor-sync range fixes, crash-unwind checks and fingerprinted
warm failures; compatibility review is pending.

## Local evidence index

Persistent runtime artifacts are outside Git under the session's SSD runtime
directory, perf20/runs/<UTC start>. Relevant run IDs:

| Run ID | Meaning |
| --- | --- |
| 20261126T221558Z | Confirmed access violation; Wine misleadingly returned zero |
| 20261126T224506Z | Fence/compositor replay; 363.8 s observed in 3D, median 3.9 FPS, controller stopped |
| 20261126T225407Z | Barycentric opt-in: 363.4 s in 3D without logged exception/reset, median 3.5 FPS; visible character/scene, separate CPU profile |
| 20261126T230548Z | Cache-only: 360.1 s observed in 3D, median 3.9 FPS, controller stopped |
| 20261126T232619Z | View-pool A/B: three warm matched pairs, 267.7 ms off versus 194.0 ms on; builder disabled on both sides |
| 20261126T234854Z | Mesh-size A/B: four warm matched pairs, 210.7 ms off versus 177.8 ms on; view pool fixed on |

The private report directory out/pr-sync-2026-11-26/perf20 contains the complete
PR inventory, selection dispositions, build/test exit files, and offline pipeline
before/after logs. These are working evidence, not a PR payload. Public
submissions should contain code, synthetic tests and this written analysis only.

## Mesh-sizing result and measurement correction

The d9e5ad914 replay completed 361.3 seconds after the controller detected 3D,
then stopped deliberately (143); no reported exception or kernel GPU fault.
Completed phase pairs 6/7, 8/9, 10/11 and 12/13 begin after 90 seconds in 3D.
Their off/on mean draw counts are 1102/1100, 1069/1069, 1069/1073 and 1062/1094.
The frame-weighted means are 210.673 ms off and 177.821 ms on: about 15.6% less
frame time, or 18.5% higher throughput. Early loading/warm phases are excluded.

The same-process allocator and free lists persist between phases, so this
does not establish cold-start VRAM savings. The new mesh-scratch counter also
missed chunks freed directly when a host list was destroyed without another
reset. Its reported hundreds of GiB are invalid as live memory, not evidence
that that much VRAM was resident. The refreshed source decrements those
still-owned chunks in MeshScratch::drop; reset/drained chunks retain the
existing delayed-release accounting. This changes telemetry only.

## Refreshed Belobug integration and qualification

The isolated merge 334df1b17 imports the full fetched a-publier 4eb6cfdf9.
Both conflict resolutions are explicit: direct-HID DS4 fields coexist with the
new opt-in keyboard/mouse state, and the new default-on safe unwinder supersedes
the earlier raw frame-register guard. Its safe reader validates that same
register against the stack before calling RtlVirtualUnwind.

The optimized Windows workspace/all-target check passes. Input tests pass
20/20 with one ignored diagnostic; crash tests pass 24/24 when retaining
Cargo's test-executable basename. An earlier renamed test binary failed the
module-name assertion; that is a harness error, not an unwinder defect.

The broader shader suite is not fully passing. The quad-neighbour graphics
test asserts in Wine at vkCreateGraphicsPipelines; the same exact test asserts
on the preserved pre-refresh shader binary. With that one excluded, the suite
reports 305 passed, four ignored and one failure: stub_pipeline_is_created
expects malformed container hashes to be refused, but this vkd3d backend
accepts them. That test also fails on the baseline. Neither failure is waived
or fixed by this merge; no complete shader-suite success is claimed.

The optimized application build passes. Focused GPU checks pass: eleven pool
tests, three mesh allocator tests, seven cache/failure-fingerprint tests, four
descriptor-sync tests, the signature parser, six async-lane tests, held-fence
ownership, actual compositor pixel readback, actual off-element view refresh
and the Wine hole-cache check. An initial lane filter selected zero tests; its
zero exit was not counted, and the actual queue::behind tests were then run.
The refreshed executable completed the controlled replay described below.
The translator version changes from 1 to 2, invalidating old translations;
warm-up differences must be excluded from steady-state comparisons.

## Refreshed targeted discussion

Nine relevant issue/PR comment threads were fetched separately from the full
metadata sweep. The original [issue 1842 discussion](https://github.com/Marvisak/VIctory/issues/1842)
already distinguishes the Linux post-loading texture-guard workaround from the
Windows intro-video report; no broader resolved claim is justified.

[7900XTX #274](https://github.com/avatarmanz/VIctory-7900XTX/pull/274)
and its newer mapper-side follow-up report that Wine/RADV may accept a heap
opened on a section view while the GPU reads independent zero-filled memory.
A successful import call alone is insufficient. Any in-place-memory trial
must first verify actual GPU readback and reject unsupported imports before
rearranging the title's pages. These are other testers' findings; they have not
yet been reproduced on this RX 5500 XT.

## Observer overhead correction

The old external controller's regex ran across every log line, even lines
without frame timings. On the saved 600-kB tail it took 2157.19 ms; filtering
frame lines first took 0.90 ms and returned identical seven timing rows.
Future runs also read only the bounded tail rather than rereading the whole
file. This is a local test-harness fix, not an application performance PR.
Earlier within-run A/B pairs used the same observer on both sides. Cross-run
timings after this change cannot isolate an application-code improvement.

## Refreshed fixed-setting replay and all-buffer tracking trial

The refreshed executable e819e195e completed 361.4 seconds in 3D without a logged
exception or kernel GPU fault, then the controller stopped it (143). Its 35
compilation-idle frame-log rows from world seconds 90–280 have median 5.9 FPS
(range 5.2–6.4), mean interval-reported frame time 169.84 ms, and 946–1277
draws/frame. These are interval aggregates, not a per-present distribution or
sustained-20 verification. The faster observer and translator version/cache
change prevent an isolated cross-run upstream-merge performance claim.
Matched captures retain the same substantial geometry and lighting gaps.

A separate 30-second CPU profile began at world second 286.6, after the timing
window. Of 37,868 samples, 16,135 map to the exact archived application binary:
3,064 in safecopy::differ::blocks, 955 in protected copying, 863 in merging,
854 in HostHeap::make_listed, 840 in texture::digest, and 635 in draw::bind_as.
These are CPU samples, not wall-frame percentages.

The next same-binary replay enabled only VICTORY_CARD_ALL=1, retaining all other
settings. Twenty card-step/relay/hash tests passed beforehand. It completed
361.0 seconds in 3D without a logged exception/reset, then stopped deliberately.
Its corresponding 34 warm frame-log rows have median 5.3 FPS (4.7–5.9), mean
189.15 ms, and 912–1269 draws/frame. The image remains similarly incomplete.
CPU preparation decreased, but GPU-wait/copy-back timings increased; these
asynchronous counters are not independent GPU timestamp measurements.
This trial is not promoted: it shows no frame-time or visual improvement.
CARD_ALL stays off in the following diagnostic baseline.

## Opt-in mesh fallback capacity counters

Code inspection found a separate omission path in mesh_indirect::SOURCE:
when an indirect record does not fit the remaining 32-MiB output region, the
converter sets its group count to zero. The existing native-mesh budget log
does not account for this conversion path. This is a confirmed code behavior,
not yet a confirmed explanation of this machine's missing scenery.

VICTORY_MESH_DIAGNOSTICS=1 enables a private coherent mapped statistics buffer.
The conversion shader atomically records active records, requested groups,
largest record, capacity-refused records and capacity-refused groups.
The five cumulative u32 values are asynchronous snapshots and can wrap.
They never decide which work runs. With the switch off, the additional UAV
branch does not access the statistics buffer and no such buffer is allocated.

All five mesh-indirect tests pass on the RX 5500 XT. The new actual-GPU
readback regression independently checks an over-capacity record, a zero-group
record and count-excluded records. It compares every generated command with
measurement off and on, and checks the numeric counters. Optimized test and
application builds pass. Scene measurements with these counters remain pending.

## Next experiments and acceptance targets

- Use the mesh counters to accept or reject the output-capacity hypothesis;
  fix dropped valid geometry before treating a fast incomplete image as success.
- Test renderer guard/journal changes with their own nonzero regressions and
  matched runtime A/B phases. Keep settings that alter shader translation in
  separate cache identities.
- Consider GPU-written-page instrumentation from upstream cd94d3538 and
  940d32f69 to avoid full CPU buffer scans. The fork's asynchronous flights
  require explicit bitmap lifetime, visibility and clearing rules; a direct
  source merge alone is insufficient validation.
- Investigate fence-aware reuse of mesh buffers on list destruction if allocation
  remains material. Preserve in-flight ownership, state transitions and a bound
  on retained bytes; reducing a heuristic delay without proof is unacceptable.
- Preserve direct HID DS4 and the private Wine memory fixes. Exclude menu
  transitions from timing; a 30-FPS pause panel is not a 3D performance result.
- Final target remains sustained 20 actual FPS in the same complete opening
  scene, with cold/warm context, frame-time distribution, memory guard and kernel
  logs recorded. No result so far satisfies it.

## Capacity hypothesis rejected for the observed scene

In the 93d852ee8 diagnostic replay, the last cumulative counter snapshot before
the menu phase reports 1,960,552 active records, 13,713,940 requested groups,
largest record 564 groups, and zero capacity-refused records or groups.
Twenty-seven snapshots precede menu interaction. This rejects the output-cap
hypothesis for that observed interval; increasing the 32-MiB cap is not justified
as a scenery fix. It does not establish correctness of generated vertices,
shader resource bindings or pixel interpolation.

The complete diagnostic process later stopped at the configured observation
limit (143). Its later menu/settings phase is not a scene FPS benchmark.

The direct cherry-pick assessment of upstream written-page tracking found
conflicts in all twenty touched paths and incompatible shader interfaces:
the fork uses root_indexed_views, reserved root slots and different mesh/wave
entry points; upstream expects its Options API and MEMORY_SPACE metadata.
No side was blindly selected and no merged version was built. The attempt
was undone only in its fresh isolated worktree. A future adaptation needs
an explicit asynchronous-flight and shader-binding design.

The existing memory-watch and sharded-journal suites pass 52 and two tests.
A same-binary runtime A/B of VICTORY_PORT_GUARD and VICTORY_PORT_JOURNAL is
running separately, with capacity diagnostics disabled. No gain is claimed
until matched warm phases complete.


## Later overnight comparisons (2026-11-27 UTC)

All runs below retained the same Series S, 1920x1080 launch profile, private
Wine server and opt-in barycentric backend, view pooling and smaller first mesh
chunks. The controller stopped each after approximately 361 seconds in 3D
(exit 143); none logged an application exception or kernel GPU fault/reset.
The images still lack the character's body and most scenery, with a flat orange
environment and a checkerboard panel. These are incomplete-rendering results.

| Run | Experiment | Result |
| --- | --- | --- |
| 000823 | Refreshed Belobug e819e195e | 35 warm log rows, median 5.9 FPS, mean 169.84 ms; separate later CPU sampling excluded |
| 001611 | Same executable, CARD_ALL=1 | 34 warm rows, median 5.3 FPS, mean 189.15 ms; unchanged incomplete image; not promoted |
| 002727 | Mesh output-capacity diagnostic | 1,960,552 records / 13,713,940 groups before menu work, maximum 564 groups per record, zero capacity refusals |
| 004233 | Guard-fault merge and write-watch journal 2x2 A/B | All four warm states 5.51–5.58 FPS; no material benefit; both remain off |
| 010017 | Retire destroyed-list mesh chunks into free pool, A/B | Four matched warm pairs: 179.358 ms off versus 179.050 ms on (5.575 versus 5.585 FPS); no material benefit; remains off |
| 012017 | Bounded compute-pipeline waits, fixed on | 35 warm rows, median 5.7 FPS, mean 174.91 ms; missing body/scenery unchanged; not promoted |

The fixed-run warm interval is 90–280 seconds after first 3D. Cross-run frame
means are descriptive, not isolated causal comparisons. The timed A/B runs use
the same process and exclude each phase's first second; allocator state survives
between phases. No compilation or sampling profiler ran during timed intervals.

### Capacity and mesh-pool findings

The opt-in mesh capacity diagnostic 93d852ee8 counts generated and refused
records/groups without changing the generated commands. Its actual GPU test
forces a refusal and verifies exact commands both with and without diagnostics.
All five indirect-mesh tests pass. The real replay had zero refusals, so raising
the 32-MiB output cap is not justified as a fix for this missing scenery.

Experimental 6f38f270f keeps destroyed-list mesh chunks only after the existing
queued/started/finished retirement and bury lag, with a separate 128-MiB pending
cap and no other COM owner. The GPU regression uses a private identity marker
to establish actual reuse, verifies readback, waits the retirement lag and
excludes externally owned chunks. Three prior mesh tests and the new regression
pass; the regression fails before the change. The live run reached 112 MiB
pending, admitted 3,247 chunks and returned 2,777, but its FPS gain is within
noise. Keep this local experiment off; real reuse alone is not a performance
claim.

### Shader-wait selection and validation

The isolated compute-wait branch imports Beqren's f63017aaf from
[7900XTX #166](https://github.com/avatarmanz/VIctory-7900XTX/pull/166),
as 3ee19eb89. It prioritizes queued compute requests and bounds time spent
waiting, while preserving this fork's compiler-crash quarantine. The conflicting
fxceless alternative assumes OnceLock serializes construction; it was not
combined with this fork's deliberately nonblocking crash guard.

The workspace/all-target check passes. Initially, ten combined test assertions
passed but the Wine process exited 137 during lazy graphics teardown; isolated
tests and the seven-test baseline exited cleanly, with no OOM or GPU fault.
The worker published pipeline readiness before its cache-save cleanup finished.
Test-only commit c7249cdbc waits for that cleanup before test DLL teardown;
the combined ten tests and the Wine compatibility test then exit zero. The
optimized application build passes. The initial abnormal test exit remains
archived; it is not counted as a clean pass.

The known head-map shader 61e54e90adccaf6a was already compiled about 70 seconds
before its first bind in the baseline, so the PR's reported symptom is not
evidence that that shader was missed here. The six-minute replay did not restore
the body or scenery. Do not carry this scheduling change into the current
performance candidate solely on its PR title.

### Next isolated experiment: per-root UAV writes

[Marvisak #2948](https://github.com/Marvisak/VIctory/pull/2948)
contains 95110f370 and its correctness follow-up 9f392adc0. The first records
which root UAVs a shader may actually write instead of snapshotting/comparing
all bound roots; the follow-up preserves stores reached through root addresses,
which do not use a root handle. This is relevant to the measured copyback
comparison cost, but benefit is not established yet.

The local port starts from the refreshed Belobug candidate and preserves its
bounded root tables, root-indexed views, compute-mesh entry points and generic
address-write fallback. Shader-cache format VSH3 forces old entries to be
translated again with write metadata. The experimental filter defaults off and
has a diagnostic A/B switch; heap UAVs retain existing conservative handling.
Local commit 036278a8f passes the optimized Windows workspace/all-target
check, three direct/spilled/handle write-tracking tests (including DXIL
validation), three cache tests including explicit VSH2 rejection, and the
application build. The guarded live A/B (013347) completed 361.3 seconds in 3D without a
logged exception or kernel GPU fault, then stopped deliberately. Four matched
warm pairs (6/7, 8/9, 10/11, 12/13), each with draw counts within 5%, contain
681 frames per side: 170.715 ms off versus 170.792 ms on (5.858 versus
5.855 FPS). The image is unchanged. No benefit is established; the filter stays
off. The remaining comparison cost is predominantly outside these root bindings. Neither upstream's distinct page-table implementation nor its
overlapping-copy follow-up (#1963) has been blindly merged into this fork.


## Imported-memory gate and next profile

The mapper-side follow-up
[jtrudeau1530 #25](https://github.com/jtrudeau1530/VIctory-7900XTX/pull/25)
was fetched explicitly from the discussion link. Isolated branch
perf20/import-gate-20261127 imports b54eb8d49, 34c409656 and c2cdabb12 as
033fcf8be, 81ad609ec and e95859d47. It checks an actual nonzero GPU readback
from a private section before opening title regions; the mapper asks the same
gate before rearranging pages. Port 1 remains off in the current game profile.
This is a compatibility safeguard, not an FPS improvement.

The explicit physical-adapter regression ran on RX 5500 XT/RADV 25.3.6 and passed:
section-backed imports were rejected before opening title memory. This test
establishes that the path is unavailable here, not yet the precise API failure
or fallback contents. The optimized Windows workspace/all-target check and
three cache tests also pass. Commit 33090010b advances both cache namespace
FORMAT and entry encoding to 3; mixed old/new executables do not overwrite
each other's entries.

The next separate diagnostic uses the loader's Windows-aware stack sampler and
the existing in-place probe. It does not serve as an FPS benchmark. The normal
warm run still compares around 232 MB/frame after shader execution, including
about 43 ms of snapshots and 25–27 ms guarding in the periodic counters. Those
timings overlap threads and phases and must not be added as an end-to-end frame
breakdown. Root-only elision does not materially affect them. The diagnostic
should distinguish heap-buffer copyback, mapping, guarded landing and profiling
instrumentation before a further optimization is selected.


## Diagnostic failure and observer stack reservation

The full existing in-place probe in run 014338 caused a D3D device removal
(0x887A0005 / Vulkan -4) before the first world frame, exit 5. The kernel log
contains no reset/fault. Its private-memory copy reads/writes initially succeeded,
but its shader phase was refused and later operations failed; that is not a
valid proof of programmable imported-memory support. Section reads/writes did
not alias the CPU view. The full probe is disabled and will not be repeated in
the game. The smaller independent section-import rejection test remains valid.

The first stack-only retry 014603 failed its 185-MiB low graphics-heap allocation
at startup, then entered the game's breakpoint and Wine's debugger. It produced
no usable profile and was stopped by its own service. The controller now detects
explicit unhandled exceptions, host GPU failures and WineDbg attachment and stops
only its selected game process; normal handled first-chance guard faults do not
trigger this check. The saved memory map shows many 128-MiB anonymous stack
reservations in the low 4-GiB address space, with the executable near its top.

The runtime keeps RUST_MIN_STACK=134217728 for translator depth. The profiler's
otherwise idle std::thread::spawn also inherits that setting. Local f918f1381
uses an explicit named 4-MiB observer and reports a spawn error. A native
regression measures the actual reserved allocation, not just the committed
stack: 134,217,728 bytes before (fails the bound), 4,194,304 after (passes),
under the same global setting. All seven profiling tests and 24 crash/unwind
tests pass, as do the optimized Windows workspace/all-target check and build.

The regression's first harness placement in a subdirectory missed adjacent
runtime DLLs (exit 53, no test ran); rerunning beside those DLLs with the original
Cargo executable basename produced the actual before-fix failing assertion.
No empty-output or zero-test run is counted as evidence.

A replay with the bounded observer, full import probe off and an explicit
210–240-second sampling window began at 01:53:26 UTC (015326). The graphics-heap allocation now succeeds and the replay reaches 3D; its
30-second capture completes (582 rounds / 16,296 sampled stacks), followed by
the controller stopping at 301.4 seconds in 3D. No application exception or
kernel fault/reset was logged. This supports the startup-reservation fix;
profiling perturbs timing and establishes no FPS improvement.

## Accepted candidate versus experiments

Branch perf20/accepted-20261127 starts from the refreshed Belobug merge and
retains the measured view-pool and smaller-mesh-chunk gains, DS4 and prior memory,
mesh-index and fence fixes. It adds the import-gate commits as f8d8d75e9,
ca8f95d77, 9fe206f3a and the observer-stack fix as 2432cfb83.

The default-off root-write metadata trial, destroyed-list mesh pool and bounded
compute waits remain separately preserved experiments, not promoted performance
fixes. Removing the root metadata trial also avoids carrying its cache-format
change into that clean branch. This branch still needs its own final build and
runtime comparison before replacing a tested executable.


## Windows-aware profile: snapshot copy target

The exact executable and profile for 015326 were archived together. LLVM
resolved 1,519 distinct application RVAs. Of 16,296 sampled stacks, 13,498
start in ntdll, 1,314 in our executable, 619 in ucrtbase and 463 in the title.
These are stack observations including blocked threads, not percentages of
CPU time. Per-thread CPU counters separately show the submitter and four
wide-copy workers among the busiest.

Of the 619 ucrtbase-top stacks, 560 return first to copy_jobs' snapshot copy
(shadow.rs:1656 in that build). The exact local DLL resolves those addresses
to sse2_memmove; disassembly shows four 16-byte loads and stores per 64-byte
iteration. An initial scalar-loop description was incorrect. Separate active
application samples include 277 in the AVX2 compare loop, 111 in protected
copying and 82 in protected merging. The snapshot read/write path is a useful
target without changing which bytes are snapshotted or merged.

The next isolated branch perf20/snapshot-copy-20261127 starts from the clean
candidate, not the rejected root-write trial. It adds a default-off AVX2 copy
for valid, held host-buffer mappings into snapshots, with a CPU feature check
and standard fallback. The loop covers complete 32-byte blocks and copies
only the byte tail; it never rounds beyond either range. Unaligned/canary and
guard-page regressions plus a mapped-host-buffer-to-private-snapshot benchmark
are being run before a live off/on comparison. No benefit is claimed yet.

The cached-native pipeline slots were also reviewed against later fork evidence
(ffe09a4e7, 58de9bf4c): the title falls back from those methods to ordinary
pipeline descriptions. Their thousands of failed native-cache requests alone
are not proof that thousands of rendered pipelines are missing. The ACCC/FSPX
deserializer proposal is not imported as a speculative scenery fix.

## Copy-volume and allocation qualification (27 November, 02:40 UTC)

The snapshot SIMD microbenchmark did not beat the existing SSE2 memmove.
Across 64-KiB, 1-MiB and 8-MiB chunks of mapped write-back memory into private
snapshots, the existing path measured 8.66–8.85 GB/s versus 8.53–8.75 GB/s for
the explicit AVX2 loop. The copy branch remains a rejected local experiment;
no live FPS claim or accepted code change follows from it.

Mapping-barrier batching (fe42c2317) preserves residency waits and coalesces
only resource requests covered by a completed barrier. Two actual-GPU tests
check both eight pre-cutoff creations and a creation after the cutoff, with
numeric readback. Run 021721 completes 361.6 seconds in 3D; four matched warm
pairs measure 172.124 ms/frame off versus 172.262 on (5.810 versus 5.805 FPS).
Body/scenery remain incomplete. This is flat and is not promoted.

A separate default-off CPU-budget pool (2f9174f0f) reuses the existing placed
write-back heaps and their final-owner reclamation. Eight independent
256-byte resources need eight mapping waits on the old path and at most one
through the pool; widths, zeroing, disjoint GPU addresses and actual GPU bytes
pass. The pool, budget-reset, batch and existing pool tests all pass, along
with the Windows workspace/all-target check and optimized application build.
These establish allocation behavior, not a live performance benefit.

The original indirect test compiler fails with E_FAIL on the unchanged baseline
before budget allocation. Test-only a2f78eb03 uses production's DXC fallback and
source root-signature builder; the existing reset and batch numerical checks
then run successfully. This compatibility fix is separate from the pool.

Run 023521 changes only the card-buffer minimum from the old forced 64 KiB to
the normal 16 MiB for this memory budget, retaining the 512-MiB card-buffer
cap. The budget pool is off. This tests whether large writable buffers can
avoid CPU snapshot/compare volume within the same VRAM allowance. It is still
running at this update; early captures retain missing body/scenery and are not
a correctness or sustained-FPS success.

The complete fxceless #927 half-buffer series through d4b786fe1 is being adapted
on perf20/half-views-20261127. Keep the final mixed-access and runtime-table
exclusions, unsigned descriptor twins, and the fork's grouped tables and
per-record descriptor cache. Cache FORMAT4/VSH4 separates the new metadata from
both accepted VSH2 and the rejected root-write experiment's VSH3. Compilation
and GPU execution are pending; the PR title is not evidence that it fixes this
machine's missing geometry.

## Refreshed PRs and correctness priorities (27 November, 02:59 UTC)

The second complete open-PR refresh reads every list page again across eight
repositories: 3,131 entries, no duplicates or page errors, and 169 new/changed
heads relative to the previous refresh. Belobug's active a-publier remains
4eb6cfdf9; its older main is not its current development branch. This is complete
metadata coverage, not a claim that every patch has passed review or tests.

The 16-MiB threshold trial 023521 completed 361.8 seconds in 3D. Warm frame-log
medians over 120–350 seconds were 174.1 ms versus the reference run's 173.65 ms;
draw medians were 1080 versus 1117. Missing body/scenery persisted. It does not
establish a useful placement gain; subsequent comparisons restore the 64-KiB
control threshold and keep the same output dimensions.

The complete final half-buffer PR #927 is ported as 728182529 and retained on
the accepted branch as 9a2b5454a. The grouped-table adaptation keeps actual
descriptor offsets, mixed-use/runtime-selected exclusions, the existing
heap-picked fallback, and separate cache FORMAT4/VSH4 metadata. A half twin
bypasses the fork's title-word-only per-record descriptor cache. Windows
workspace/all-target checking and the optimized application build pass.
The half and halves shader filters execute 10 and 13 successful checks; GPU
metadata/table and numerical filters execute 3 and 2. Cache checks pass too.
A table_plan filter matched zero tests and is not validation evidence.

Actual RX 5500 XT GPU readback preserves all 65,536 half-bit patterns through
the unsigned view versus only one through the former float path. This qualifies
the numerical correction. Replay 024910 completes 361.6 seconds in 3D without
a logged exception/reset, with a 170.25-ms warm frame-log median, but body and
scenery remain missing. Its fresh VSH4 cache also affects loading. Do not claim
an isolated FPS gain or a fix for the full intro.

Belobug #428 (6bc90e3204, attributed local import 6b6bbf122) addresses an
unsynchronized cross-queue read/write in the enabled RELAY_COPY path. A fetch
of a copy still written by a later flight moves behind it on the drawing queue;
a later submission writing a copy being fetched waits on the copy-queue fence.
This supersedes attempting to repair values after catching a partial write.
The explicit queue-ownership test, seven relay checks, two torn-copy checks,
and both actual-GPU ordering regressions pass locally. With FETCH_CHECK=0,
the 12-round regression reports zero wrong title results, zero undone GPU
results and zero repaired/suspect pieces. The unchanged unsafe path was not
replayed locally just to provoke a race. The Windows workspace check passes;
application build and game replay subsequently passed; see the following replay entry.

Range PRs Belobug #440/#437 are preserved as 88d17d8c4/ab52b7e03 with a
default-off measured gate dbb3e63ee on perf20/range-sweeps-20261127. Their
compile/replay was deferred to validate relay ordering first. No speed claim.

Further triage:
- Belobug #449's legal-before-state adjustment is already covered by this
  fork's HostList::transition, which legalizes and deduplicates states.
- #421's Wine HLSL compiler workarounds overlap the working DXC/source-root
  fallback here; no current compilation failure justifies treating them as a
  new measured fix.
- fxceless #1422 needs the bake-source infrastructure absent from this branch;
  it cannot be applied as a standalone body/scenery fix.
- Belobug #453 combines writable placed textures, texture conversion, caching
  and warm-up changes. It needs separation/review and is not yet accepted.
- #443's cache-lock creation changes overlap our existing prune/pool work and
  require an adaptation that preserves placed-resource ownership.

## Relay replay, safe target capture and opaque-scene diagnosis (03:20 UTC)

Relay import 6b6bbf122 completed run 030034: 360.8 seconds in 3D, no explicit
unhandled exception or host GPU failure. The queue counters exercise both
paths (e.g. 3.5 copy-queue fetches, 3.3 drawing-queue fetches and 0.2 waits per
frame in the final line). It is retained as accepted 1173d5d8a for ordering
correctness. The body/scenery still disappear. CAPTURE_WORLD saves recurring
frames, so the 177.45-ms warm median is not an isolated performance comparison.

DexNom #127's bounded readback mapping is selectively ported as e477145d9.
A D3D12 footprint can omit the final row padding; capture maps the resource's
actual byte width and unmaps via a guard. All four capture checks, including an
actual GPU 268-byte/two-row readback, pass. The Windows workspace/all-target
check and optimized application build pass too. Deferred-input capture code is
absent from this fork and was not imported.

Run 031059 captures the targets of frame 2200 and completes its 241-second
3D window. The same-frame color/material G-buffers contain the room, window
framing, floor and outdoor scenery that the final presented image loses.
This is evidence to examine depth, lighting and composition, rather than assume
that every missing object was never drawn. The capture also shows populated
depth/stencil data. R32G32_UINT (17) and R32G32B32A32_UINT (3) targets currently
lack a diagnostic PNG decoder; a missing dump for them is not an empty target.
No native title assets, targets or operation dumps belong in a source PR.

The ambient stand-in is actually compiled/running in the logs (Radiance and
Resolve); no Wine compiler failure occurs there. The current launch profile
sets AMBIENT=0.8 but leaves AMBIENT_AIR unset, which is zero/off. The existing
ambient module and fork-ray-tracing.md document that this leaves a brown veil
in the day room without rays. Run 031926 tests the existing air correction at
0.25 with exactly the same executable and pixels. It is a diagnostic of the
disabled ray-tracing fallback, not a new renderer fix or native-lighting claim.

The prepared cache-lock adaptation cf18cdb00 preserves the placed-resource
pool and bounded eight-miss pruning. Atomic last-use stamps use fetch_max.
The concurrent-miss test 97dd7454d checks a single canonical Arc and actual GPU
bytes after losing allocations are dropped. Follow-up d8b2c557c keeps imported
window creation/publication under the same exclusion lock as in_place_let_go;
only ordinary copied-window allocation moves outside it. This prevents a remap
from removing an entry just before an unlocked creator republishes its old
imported mapping. These changes subsequently pass the combined checks; the following factorial replay does not establish a useful performance gain.

The combined experimental branch perf20/copy-performance-20261127 contains
the candidate budget pool, range algorithms and cache-lock adaptation on the
accepted correctness base, with the capture and test-compiler fixes. Pool and
range gates remain off by default. Its combined checks subsequently pass; the following factorial replay remains flat, so it does not replace the accepted build. The range comparison
also restores the true original empty-range scan on its off side: the imported
pairwise helper already had an early exit, which otherwise silently improves
both sides and hides part of the PR's gain.


## Rejected performance trials and refreshed PR review (04:03 UTC)

The all-pages inventory refresh at 03:44 UTC covers 3,156 open PRs across all
eight known repositories, without pagination errors or duplicate records.
There are 48 new/changed heads since the 02:42 inventory. This is metadata
coverage, not a claim to have reviewed the code of every PR. Sanitized inventory
and changes are in the local performance report; private runtime artifacts stay
outside source history.

The combined copy-performance candidate 8174bf246 passes the Windows workspace
and application build, eight targeted test groups, and both relay GPU ordering
regressions. Replay 033144 runs 361.5 seconds in 3D without a fatal diagnostic.
In the warm completed factorial phases, neither budget pooling nor range sweeps
helps: approximately 5.97 FPS with both off, 5.92 with budget only, 5.84 with
range only and 5.91 with both. Cache concurrency is enabled on every side, so
its effect is not isolated. The budget pool is exercised; no large range-index
path usage appears in the logs. Keep all three out of the accepted changes.
The same incomplete opaque scenery remains.

The ambient-air diagnostic 031926 completes 180.9 seconds with the existing
air correction set to 0.25. It does not restore the missing opaque surfaces;
the early frontal shot is darker. The frontal body was visible in the control
too, so its brief appearance is not evidence of a fix. Restore AIR to zero/unset.

The complete 7900XTX #98 streaming-copy series is preserved on the separate
streaming-copy branch, including its final default-off switches STREAM_COPY and
STREAM_REFRESH. Windows workspace/all-target checks, application build, protected
copy/guard-page checks and wide-copy checks pass. A broad host-test filter gives
51 passes and one failure: the root-flag rejection fixture. That exact fixture
also fails on the older fetchguard baseline because the private vkd3d serializer
accepts the artificial flag. The qualified broad run excluding that known fixture
passes 51 tests. Several native mesh tests skip on this GPU; this does not establish
native mesh support. No streaming-copy performance benefit is established yet.

New Belobug #454 addresses upload-ahead claim/publication races, failed-job
callbacks and pool-epoch flips. It is isolated for validation, with authorship
preserved. Its inherited one-second list timeout still needs scrutiny. Control
replay 035448 disables UPLOAD_AHEAD on the same diagnostic executable, runs
180 seconds, and leaves the same hair/sky/checkerboard result. Thus these races
are worth correcting but do not explain the entire visual failure.
The same-source plain release build passes workspace checks and is now being
replayed with upload-ahead and both streaming switches off. Its LTO/codegen
settings differ from the diagnostic profile; its frame meter remains available,
but diagnostic frame captures do not.

The new fxceless #1424 descriptor-page parallelism already exists in this fork.
Only its single-large-copy splitting is being adapted, default off, with the
existing protected read and sparse/remap fallback retained. New regression cases
cover unaligned ends, partial failed chunks, remaps and exclusion of nested pool
submission/title destinations. Compilation and replay are pending.
