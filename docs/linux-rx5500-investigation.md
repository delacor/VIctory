# Linux RX 5500 XT investigation

Updated 2026-11-27 (local date; run timestamps below are UTC). This is a working engineering report, not a release claim.
The target is sustained 20 real presentation frames per second in the same opening
3D sequence, with correct scenery and the existing DS4 support retained.

## Current result and limits

The initial Series S trial reached approximately 4.6–4.9 FPS after compilation
settled, then crashed after about five minutes. A replay with the fence/compositor
corrections completed a six-minute 3D observation window without that exception
or a GPU reset: 66 compilation-idle frame-log samples had median 3.9 FPS
(range 3.5–5.5). It was then stopped by the controller (exit 143).
The later barycentric experiment renders the character and surrounding scene,
but some detail remains incorrect and timing is still around 3–4 FPS. This is not a successful performance
baseline, and no result here establishes 20 FPS.

The pooled-view-buffer A/B run now measures a warmed improvement at comparable
draw counts: 267.7 ms/frame (3.74 FPS) with committed buffers versus
194.0 ms/frame (5.15 FPS) with pooled allocations. This uses three completed
30-second off/on pairs, each starting at least 90 seconds after first 3D,
with mean draw counts differing by less than 5%. No compiler or sampling
profiler ran during those pairs. The ray-tracing structure builder remains
disabled by the fork's default on both sides; this is not a full-feature or
20-FPS verification.

A previous run showed incomplete scenery. Correctness must be checked alongside
timing: black frames, omitted draws, or missing geometry cannot count as a gain.
Subtitles On and HUD Always have been requested but have not yet been verified
in the saved game settings.

## Environment and reproducibility

- AMD Ryzen 7 3700X, RX 5500 XT 8 GB, approximately 36 GB system RAM.
- Linux 7.3.5, Mesa 25.3.6, Wine 11.0 staging (Fedora 11.0-2.fc43).
- Game data on SSD. Runtime prefixes, caches, logs, and derived captures are
  outside the repository. No game assets, shader dumps, screenshots, crash
  report payloads, or proprietary code are included in this report.
- Windows GNU LLVM target; Rust 1.98.1 and LLVM MinGW 20261117. Diagnostic
  builds use optimization level 3 with line information.
- The tested fork starts at Belobug/VIctory-Belobug-AMD a-publier cd8dfc06a,
  on local branch perf20/linux-rx5500. The last launched executable was
  50ec3f9e9; 95e95d5c1 adds an unwind guard. The fence/compositor candidate at 1bedfd6ce
  passed its focused tests and the six-minute observation window. The document
  commit is 24255dd04; the later backend experiment uses the same application code.
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
  Live comparison pending.

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

The private report directory out/pr-sync-2026-11-26/perf20 contains the complete
PR inventory, selection dispositions, build/test exit files, and offline pipeline
before/after logs. These are working evidence, not a PR payload. Public
submissions should contain code, synthetic tests and this written analysis only.
