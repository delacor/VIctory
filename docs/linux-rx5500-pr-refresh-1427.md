# RX 5500 XT PR refresh, 2026-11-27 14:27 UTC

The sustained 20 FPS target is still unmet. This candidate follows the qualified
mesh-window branch and preserves its existing Wine memory fixes, portable
software mesh fallback and DS4 HID implementation. It changes no title menus.

## Complete inventory

The refresh read all 39 open-PR list pages across eight repositories: 3,346
distinct open PRs, zero duplicate IDs or request failures, and a short final
page for every repository. There are 115 new or changed heads relative to the
12:22 scan. Ten previously open IDs were absent; their closure/merge status is
not inferred from absence. This is a complete metadata refresh, not a claim
that all 3,346 patches received source review or testing.

Default branches were checked explicitly: Belobug uses a-publier, myokex uses
vi-fx3. The FQY fork added a full Scarlett source tree; that default head was
fetched for review, not merged wholesale into this candidate. The other seven
recorded default heads remain the same as the prior refresh.

## Imported distinct changes

Original authorship and cherry-pick attribution are retained.

- [Belobug #581](https://github.com/Belobug/VIctory-Belobug-AMD/pull/581):
  all three commits, covering a weak-reference queue of dirty GPU buffer
  copies, shared GPU comparison results for overlapping 64 KiB pieces,
  readback-buffer reuse after list reset, and removal of redundant staging
  scans. The compare module and our existing fetchguard module both remain.
  VICTORY_GPU_TIMING=0 no longer accidentally disables overlapped submission.
  Our existing ordinary launch does not set that variable, so this selector
  correction alone is not expected to speed that launch.
- [Belobug #614](https://github.com/Belobug/VIctory-Belobug-AMD/pull/614):
  count mapped tiles in texture residency and eviction accounting. This is the
  same fix as 7900XTX #468; import one copy.
- [Belobug #612](https://github.com/Belobug/VIctory-Belobug-AMD/pull/612):
  classify root-constant dword zero as a dynamic offset rather than an address
  base. A local follow-up raises the translator version from 8 to 9 so existing
  shader caches do not serve the old analysis.

## Reviewed without importing wholesale

- [fx #1476](https://github.com/fxceless/victory-fork/pull/1476) supports the
  input-locality hypothesis. Its measured card is a 32 GB RTX 5090; its 8 GB
  preset is explicitly untested. Our source has different window-placement
  switches, and our 8 GB card cannot fit the 4.3 GiB of ordinary title windows
  in addition to its other working set. Do not copy the large-card settings.
- [fx #1469](https://github.com/fxceless/victory-fork/pull/1469) is a larger
  raw-address/RT memory redesign. Our directory and page-arena path are already
  local; its remaining coherency and translator changes need separate tests.
- [fx #1468](https://github.com/fxceless/victory-fork/pull/1468) changes stream
  ordering of title copies. Its own measurements improve pacing with a small
  mean-frame regression, and current-main live validation is missing.
- [Belobug #620](https://github.com/Belobug/VIctory-Belobug-AMD/pull/620) pools
  prelude allocators and adds optional pool limits. It is a useful next isolated
  candidate, with fence/reuse stress coverage required on our Wine stack.
- [Belobug #613](https://github.com/Belobug/VIctory-Belobug-AMD/pull/613) enables
  DS4 products in the DualSense reader. Our existing dedicated HID module
  already supports DS4 USB and Bluetooth, including product IDs 05c4/09cc/0ba0
  and rumble; retain that implementation.
- [7900XTX #440](https://github.com/avatarmanz/VIctory-7900XTX/pull/440) was
  rechecked. Its Wine mmap, shader-loop and clamp-barrier ideas overlap work
  already investigated locally. Its anonymous imported-heap path must not
  replace our working path until the outstanding unmap lifetime issue is fixed.
- Marvisak #3525/#3528 offer small bitfield-mask lowering improvements with
  GPU semantic tests; #3522 improves descriptor census work. Source patches
  were read, but these remain separate candidates rather than mixing new
  translator and descriptor changes into this performance measurement.
- The consolidated 7900XTX #469/#470 supersede #460/#461; do not stack both
  versions of either collection. They also contain previously ported fixes.

## Measurements immediately before this refresh

Same-binary mesh windows A/B, 120-410 seconds after the first 3D draw:

| Window mode | Samples | Mean ms | Effective FPS | Mean draws |
|---|---:|---:|---:|---:|
| Off | 54 | 206.20 | 4.85 | 1114.54 |
| On | 51 | 192.41 | 5.20 | 1096.78 |

Both completed with the controller's normal exit 143 and empty kernel logs.
The final candidate capture contains the character, room and outdoor scenery.
Existing noisy lighting and dropped passes remain. The sequential scene is
not deterministic; unequal work prevents treating the ~7% frame-time reduction
as a proven causal gain.

Fifteen fresh one-frame replay processes compared original placement, GPU-local
stride metadata, list scratch, and ordinary windows limited to 64 KiB or 1 MiB.
Three runs per mode, original/reversed/original order, same replay binary and
capture. All exits were zero, kernel logs empty, and images stayed within
observed replay variation. Median GPU times were 158.85 / 164.86 / 158.88 /
158.29 / 160.75 ms respectively. No convincing improvement; keep placement
unchanged. These are replay GPU times, not live FPS.

## Qualification and live validation

The fresh candidate uses an independent SSD Cargo target. Qualification runs
under the exclusive GPU experiment lock: Windows workspace all-target check,
GPU/shader test builds, dirty-queue/landing tests, compare-plan tests, real
texture readback reuse, sparse fingerprinting, real mapped-tile accounting,
descriptor and mesh-indirect regressions, factory reuse, root-resource analysis,
and the 256-record visible mesh-window GPU test. The tile test prints its
actual mapped byte count; the runner requires that marker and the mesh
hardware marker rather than accepting skipped hardware work.

Qualification and the subsequent same-scene live result are recorded separately
in perf20/pr1427-validation.json and the durable runtime logs. The Windows workspace all-target check, test builds, all 64 targeted test
functions, and application build completed with exit zero. Actual mapped-tile
coverage was 1,048,576 bytes, and the mesh test verified all 256 draw records.
The new application SHA-256 is
49a0640a39aef4c823afbb5c793a98049e8ed04e05379d35dcdfcaf4a8a2524c.
Clippy has not been run for this combined candidate. Live FPS validation remains
pending; passing tests are not a speedup claim.

## Reported visual regressions

During this live run the user reports consistent face and wall rendering, but
unacceptably low speed. They also report that skin had more detail around five
or six builds earlier; the exact earlier binary is not identified. Keep that
regression open. Loading artwork at frame 1024 is not a valid 3D skin reference,
and the available later captures mostly show the character from behind.
Preserve archived binaries and compare the same face angle before declaring
rendering fixed. Scene differences alone are not evidence that a regression
has been resolved.

The dense opening replay captured the correct skin-visible close-up. The user
selected frame 1929 as the comparison shot. Its unmodified PNG, checksum,
source commit and binary hash are preserved privately with the run. This is
a reference selection, not confirmation that skin detail is correct. Do not
commit game screenshots or substitute loading artwork for that shot.
