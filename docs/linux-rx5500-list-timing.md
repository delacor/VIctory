# RX 5500 XT: ordinary submission timing and batching feasibility

The 20 FPS objective remains open. The qualified normal build is around 5 FPS;
the additional diagnostic code does not constitute a throughput improvement.

## Optional timing PR

The integration carries only the asynchronous timestamp portion of
[7900 PR #453](https://github.com/avatarmanz/VIctory-7900XTX/pull/453), upstream
74c383d2e569d97e0379f4d08b9b52403c5d8f75, preserving Jesper-Andersson's
authorship. Local adaptation 561e74d8f and hardware regression cd23efdfa were
qualified in belobug-list-stamps. They are integrated as 452fb997f/3fdf7edd6.
An exact diff of crates, apps, Cargo files, xtask and .cargo against the tested
tree is empty; integration documentation differs. The binary is still
attributed to its actual source cd23efdfa, SHA-256
d189cb6667cc8b99d2641a61cdab157a417481b906a327b8665a58dd5bcffa80.

VICTORY_LIST_STAMPS=1 adds native timestamp/resolve lists between runs of
prelude, title, card-step and compare lists. The ordinary submission batches
and relay remain active. Results are read after their existing completion
fence; there is no added CPU wait after each title list. The feature is off
by default.

Qualification passed: Windows all-target workspace check, application build,
4 pure timestamp tests, 6 existing GPU mesh-indirect tests, 1 list-factory test
and 1 new real-GPU stress test. The latter executes 1,536 stamped submissions
plus an unstamped baseline, validates all 98,368 copied dwords, crosses the full
4,096-slot ring, and observes no unread, missing or reversed timestamps.
Strict Clippy was not run.

## Live diagnostic result

Run 20261127T132753Z uses the same four correctness fixes, F32 portable mesh
fallback, shared mesh storage, 1,024 MiB hot-buffer budget, Wine memory fix and
DS4 support. It exits 143 normally after 181 world seconds. Scenery and the
character remain visible; the kernel log is empty.

The 120–175 second interval contains 49 valid diagnostic log samples. Medians
of their per-frame values:

| Queue category | Time |
| --- | ---: |
| All submitted lists | 146.2 ms |
| Title lists, including generated fallback work | 140.0 ms |
| Preludes | 0.9 ms |
| Card steps | 0.1 ms |
| Compare lists | 5.1 ms |
| Between submissions | 81.5 ms |

The median is 16 submissions/frame, with 0 left out and 0 unread. These are
medians of log samples, not exact frame-weighted averages. Do not add medians
to infer a precise frame time. Timestamp lists add overhead. Between-submission
time includes uploads, present work and idle periods; title-list time includes
emulator-generated copies and fallback passes. This is neither a hardware FPS
floor nor a proof that memory work is the only remaining bottleneck.

The earlier detailed replay ranks thousands of dispatches and barriers among
the largest costs. The new timing supports investigating GPU execution as well
as CPU copy/remap gaps. Keep the normal untimed build as the FPS reference.

## Batching scope and feasibility

The full-description search covered all 3,297 inventoried PRs with broader mesh
batching terms and found 37 candidates. fx #894 supplies a native-mesh regression
for distinct per-draw root constants, not a runtime batching implementation.
fx #1412 excludes mesh/compute and overlaps existing ordinary indirect batching.
No reusable full software-mesh cross-call implementation was found by this
search; this is a description search, not a code audit of every PR.

Our observed 2.62 million indirect mesh recording calls all contain one maximum
record. Useful batching therefore needs to cross API calls. Native bundles are
a possible mechanism: the supported indirect form has one operation and no count
buffer or indirect root changes. Render-target setup, viewports, resource
barriers, queries and copies must remain outside the bundle; descriptor heaps
must match the parent. See Microsoft's [bundle restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d12/recording-command-lists-and-bundles)
and [ExecuteIndirect bundle rules](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12graphicscommandlist-executeindirect).

The isolated belobug-mesh-bundle-probe fixture now verifies the legal recording
mechanism on RX5500/Wine: indexed and expanded outputs, groups 1/3, direct
indirect, bundle direct and bundle plain indirect produce matching visible
images. Four reference renders and 12 candidate renders agree within 1e-6.

Its first broader run exposed two inherited failures: state-changing indirect
commands executed no work with DGC disabled. Both failures reproduce on the
earlier fbbe shader-test binary. The fixture now defaults to the same plain
indirect dispatch and explicit UAV bindings as the game; an explicit optional
state-changing test preserves that separate coverage.

The green build and image test pass; the broader module reports 9 passed and 2
ignored. Three WARP-dependent cases return early because no SM6.6 software
adapter is available, so these must not be called 9 actual GPU tests. The new
bundle image test independently uses the real hardware adapter.

The probe is test-only. Production cross-call batching is not implemented.
Before implementing it, preserve full per-record capacities and distinct output
slots, capture all relevant root/pipeline state, retain every allocation through
completion, and flush at any read/write hazard, target/viewport/heap change,
copy, clear, query, timestamp or predication boundary. Pixel/mesh store metadata
alone cannot prove a batch safe.

The original fallback benchmark uses tiny subpixel triangles. Its large
synthetic batching ratio is not an estimate of gameplay FPS.

Evidence artifacts: list-stamps-validation.json, list-stamps-live-summary.json,
mesh-batching-full-inventory-search.json and mesh-bundle-probe-validation.json
in the neighboring report/perf20 directory. Private frame assets stay outside
Git. Publishing remains handled by the other chat.
