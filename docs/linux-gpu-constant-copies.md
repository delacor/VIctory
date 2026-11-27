# GPU constant-copy FPS experiment

The user put further face tests on hold and asked to prioritize FPS.
The sustained target remains 20 FPS, not achieved; qualified baseline runs
are around 5-5.5 FPS in the opening scene.

## Bottleneck and change

Frame logs repeatedly show roughly 100 ms waits for small constant-memory
fills that overlap ranges GPU flights write. Our existing copy-at-command
engine can already leave a title copy on the GPU, but a later CPU fill of its
destination requires the flights to land before submission preparation.

With VICTORY_GPU_CONSTANTS=1 (off by default), a constant read that overlaps a
recent placed destination can use the existing place_source path: refresh
CPU changes, copy the GPU's current bytes in command order, and bind an aligned
temporary buffer. No CPU fill of that destination is recorded for this read.
Both root CBVs and CBVs copied into dynamic tables use the same helper.

This adapts the constant-chain idea of fxceless/victory-fork#1468 to our newer
placed-copy engine. The foreign Step variants and its weaker CPU-read checks
are not imported.

## Conservative scope

- Only recently recorded placed destinations of at most 1 MiB, retained for two
  frames and at most 256 ranges.
- Only existing GPU-written shadows with two sides; imported title memory and
  single-sided system-memory copies retain their old path.
- At most four constant buffers per recording, each at most 64 KiB. Each has
  a distinct retained buffer. No per-draw slot is overwritten.
- Existing place_source barriers order the copy; the returned buffer is moved
  from COPY_SOURCE to constant/shader read state.
- HostList holds resources until its normal completed reset. The count is
  reset with Deferred after completion. Caller COM release does not release
  the GPU's resource early.
- Frame recording retains the existing CPU path because the recorder does not
  encode the enhanced dependencies yet.
- Shader translation, cache version, title menus, Wine memory implementation,
  DS4 support and rendering resolution are unchanged.

## Required qualification

An independent SSD build target runs the Windows workspace all-target check,
GPU/shader test builds, the new isolated hardware test and the prior 64 targeted
functions, followed by the application build. No live use until every gate is
zero and the real hardware marker is present.

The hardware test requests eight completed recording generations and 32
disjoint copies. It combines GPU writes with CPU changes, reads the actual GPU
bytes back, releases caller references before execution, checks the allocation
cap and verifies that completed resets make the quota available again. It
does not prove every title-level binding or inter-queue use case.

## Live measurement

First use a short smoke run with the feature on. Inspect copies per frame,
fallback reasons, the original wait counters, memory and kernel logs. If the
path is actually used and remains healthy, compare the same binary with the
feature off and on through the ordinary warm opening replay. The new counters
report attempts, copies, cap refusals, absent recent copies, unavailable
two-sided sources and copied bytes.

These conservative global copy dependencies may cost GPU time. Removal of a
CPU wait does not establish a frame-rate improvement, and replay GPU timings
already show substantial shader work. No speedup is claimed before the live
comparison. Further skin comparisons remain on hold.
