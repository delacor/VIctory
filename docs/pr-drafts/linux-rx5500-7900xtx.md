# Drafts for avatarmanz/VIctory-7900XTX — not submitted

These are separate focused corrections. Recheck the target branch and open PRs
before selecting a base or posting either draft.

## 1. Check local mesh indices before adding the group offset

The RDNA1 compute fallback from
[PR #20](https://github.com/avatarmanz/VIctory-7900XTX/pull/20)
adds a per-group vertex offset. If the offset is added before checking against
the local maximum vertex count, valid indices from later groups are culled.

Local commit 978fd7402 checks the three local indices and primitive cull condition
first, then adds the offset to valid indices. A synthetic two-group GPU regression
failed before the change (second-group indices became zero) and passes after it.
All four mesh GPU tests and all four indirect GPU tests passed on RX 5500 XT.
Fallback image coverage is checked; a native mesh reference is unavailable on
this hardware and is explicitly not claimed.

This requires the compute fallback. Submit against PR #20 or an appropriate
integration base, rather than including unrelated fallback code or all local
performance experiments.

## 2. Recognize the compositor's linear static sampler under Wine

The source root-signature reader introduced by
[7900XTX PR #338](https://github.com/avatarmanz/VIctory-7900XTX/pull/338)
must map FILTER_MIN_MAG_MIP_LINEAR. The existing two-plane compositor uses
that enum; an unrecognized word currently prevents its creation and falls back
to showing the base plane.

Local commit 1bedfd6ce adds the missing enum mapping and a regression using the
actual compositor signature. That unit test and the existing synthetic GPU
composition/readback test both pass on RX 5500 XT under Wine/vkd3d. The latter
checks scaled and premultiplied display planes against expected pixel values.
No FPS gain is claimed.

Only submit where the source reader exists, or contribute this correction to
the source PR. It is separate from the mesh correction.

## Additional ownership fix

The local async-lane fence ownership adaptation extends fxceless #404. Its held-fence
regression and all six async-lane tests pass. One six-minute 3D observation window completed without the earlier exception;
further repetitions are needed before claiming a crash fix; see the
VIctory draft and ../linux-rx5500-investigation.md.

Imports from this repository retain their attribution. PRs #151, #200, #271,
#351, #375, #376 and #377 are dependencies/previous work, not newly authored
contributions. The full experimental branch is not an appropriate PR payload.

## Backend dependency — separate from the application PRs

The private vkd3d experiment enables its existing AMD explicit-vertex-parameter
barycentric path with a default-off switch. Thirteen existing shader tests pass,
including actual GPU pixel readback, and eight real pipeline records that were
refused before now create successfully. The character and surrounding scene
are visible in the subsequent replay. Some image detail is still incorrect,
the original backend experiment measured about 3–4 FPS, and the backend fallback has interpolation
limitations. Do not present it as native feature support or a 20-FPS result.

Keep that backend patch and its limitations separate from VIctory source fixes.
The investigation report includes the exact text patch for later review.

## 3. Amortize exact-view cache idle sweeps

Proposed title: Amortize exact-view cache maintenance over eight misses

Two separate steady-state profiles show more than 500 samples inside the
exact-view cache's full BTreeMap retain walk. It scans around 2,500 entries
on each miss, with roughly 280–317 new views per frame, under a shared lock.

Local commit 37d89d3b5 keeps the ownership and 512-asking recency checks, but
walks the map once per eight misses. Eligible idle copies can be reconsidered
up to seven misses later. This trades bounded extra cache retention for less
repeated work; it does not omit a draw, copy, or memory write.

The actual D3D12 resource regression passes: an externally held Shadow, a
buffer still referenced by a host view, and a recent entry survive a sweep;
after references are released and age expires, idle entries are reclaimed
within the bounded sweep interval. The optimized Windows test and application builds pass.
The cache-only replay completed; it does not isolate a causal FPS gain.

Submit only against a base that has this exact-view cache, preserving that
base's invalidation and reference checks. Keep this separate from correctness
fixes and the private backend experiment.

The target-base check confirms that fetched 7900XTX main retains this per-miss
scan. Marvisak main already uses a weak cache with size-triggered pruning and
a shared-buffer arena (6cc6aa352). Do not open the same batching PR against
Marvisak main; a tested adaptation of its broader arena design may supersede
this smaller fork-specific stopgap.

## 4. Pool exact-view resources in resident write-back heaps

Local b7a32f2ed preserves the fork's per-resource descriptor/lifetime model while
sharing 64-MiB CUSTOM WRITE_BACK/L0 heaps for small exact-view buffers. Each
view still has a distinct resource of its original width and offset zero.
Heap residency/mapping waits happen when a heap is created. Reused bytes are
zeroed; COM lifetime returns a piece only after the last resource reference.

Twelve correctness checks pass, including actual GPU UAV/readback and the
existing descriptor/refresh regression. Idle-GPU allocation measured
148.03 us/resource committed versus 29.90 us pooled. A separate live A/B run,
with three warm matched off/on pairs and similar draw counts, measured
267.7 ms/frame (3.74 FPS) versus 194.0 ms/frame (5.15 FPS). No compiler or
sampling profiler ran during those pairs.

This trades coarser placement alignment for fewer committed allocations. Main's
shared arena is denser and already solves the same class of problem through a
different ownership model; this is a fork adaptation, not a duplicate PR for
Marvisak main. The ray-tracing builder remains disabled on both sides of the
comparison, so this is not full-feature or 20-FPS validation.

## 5. Small first mesh scratch chunks

Local d9e5ad914 adapts upstream 9398d08 by Eduardo Bertozzi. Begin at 1 MiB and
double later chunks to the existing 16-MiB growth limit; larger regions still
allocate the requested power of two. Preserve the fork's per-draw recycling
and delayed retirement.

Two imported allocator tests fail on the original 16-MiB minimum and pass after
the change. A third verifies recycling retains UAV state. Optimized Windows
build passes. Four warm matched 30-second off/on pairs measured 210.7 versus
177.8 ms/frame (4.75 versus 5.62 FPS), with view pooling fixed on.
The reused allocator state prevents a cold-start VRAM-savings claim. This belongs with
the compute fallback prerequisite and remains separate from the view-buffer pool.

The first mesh live-memory gauge omitted buffers freed on direct list
destruction. Its inflated totals must not be cited as resident VRAM or a memory
saving. The refreshed candidate corrects that telemetry path; GPU ordering,
allocation sizes and delayed retirement are unchanged by the counter fix.

The separate Belobug refresh preserves DS4 and imports newer diagnostics,
descriptor synchronization and crash checks. It is not the same executable as
the measured allocation A/B; its complete validation status belongs in the
investigation ledger before submission.

## Diagnostic follow-up, separate from performance fixes

The opt-in indirect-mesh counters expose capacity-refused records that the
native mesh-budget log does not count. Five GPU conversion tests pass, including
a new numeric counter readback and byte-for-byte generated-command comparison
with measurement off/on. The pre-menu scene interval reports 1.96 million records and 13.71 million
requested groups with zero capacity refusals. The output cap is not the
observed missing-scenery cause in that interval.
Do not claim this diagnostic itself repairs geometry or improves FPS.

The refreshed fixed-setting replay now completes at a median 5.9 FPS in warm
frame-log intervals. The CARD_ALL experiment completes at 5.3 FPS and is not
promoted. Both retain incomplete rendering; see the investigation for controls
and limitations.


## Overnight qualification

The later controlled tests found no material FPS improvement from guard/journal
tracking or destroyed-list mesh reuse; those experiments remain off. Bounded
compute waits also left the missing body/scenery unchanged in a six-minute
replay. None should be presented as a demonstrated performance fix. The report
records their tests, attribution and limits.

Per-root UAV write tracking from Marvisak #2948, including its root-address
correctness follow-up, was tested separately. Coordinate that work with the
existing PR rather than submit a duplicate. Menu changes are
no longer requested or part of the testing procedure.


Per-root write elision also measured flat in four matched warm pairs (5.858 FPS
off, 5.855 on), with unchanged incomplete scenery; keep it as a local experiment.
The new imported-memory mapper gate has an actual RX 5500 XT negative-capability
test: unsupported section imports are rejected before title memory is rearranged.
That existing fork PR should receive any follow-up rather than a duplicate
performance submission.


A separate diagnostic reliability change gives the scheduled observer an explicit
4-MiB stack instead of inheriting the translator's 128-MiB global setting. Its
native before/after reservation regression and related loader tests pass. See
[the separate profiler draft](linux-profiler-stack.md); it makes no FPS claim.

The later snapshot SIMD and mapping-barrier batching trials also show no
measurable gain and remain out of the accepted build. CPU-budget heap pooling
has passed actual-GPU allocation/readback and budget-reset checks but still
needs a replay. The 16-MiB placement-threshold replay and complete half-buffer
PR port are under evaluation; neither supports a new performance or visual
correctness claim. The investigation ledger contains exact scope and results.
