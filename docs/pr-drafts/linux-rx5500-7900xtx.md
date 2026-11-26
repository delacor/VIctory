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
frame rate remains about 3–4 FPS, and the backend fallback has interpolation
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
The in-game comparison is pending; no FPS gain is claimed.

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
build passes; live timing and memory comparison are pending. This belongs with
the compute fallback prerequisite and remains separate from the view-buffer pool.
