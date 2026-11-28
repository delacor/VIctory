# Large private allocation write watch on Linux

The baseline sends about 21 MB/frame up and back for a 0x1412200-byte scratch buffer. Its default heap allocations are not write-watched, so refresh cannot ask the kernel which pages changed. NVIDIA PR #1038 adds watched private reservations for padded allocations at least 1 MiB long, retaining the heap fallback and a WATCH_LARGE_XMEM=0 comparison switch.

Reviewed upstream commits: 783242121aa562499b4c67aff1b9eebaa2c92055 and 7cd552bd64432bd64ad17e83f1c2aea6af925ec6. They were cherry-picked onto 9dcecce2bcc5bc84398ddc130683c98671b1a97e as 276bb4c40 and d29d0d5e3. Local regression commit 1d4fdb9c1822a4ed6b4a31c4b221444836b17df5 adds eight sparse concurrent-write / reset / query generations at minimum, 64 KiB and 2 MiB alignment, in both switch modes. The reservation base is retained in the header and freed through VirtualFree. address_range(0) retains our existing bounds and keeps these allocations above the low 4 GiB used by 32-bit requests.

The PR also tries write watch for eligible explicit-address reservations, falling back to the original flags on refusal. That behavior and the larger allocation header are common to both switch modes. The comparison therefore isolates large default allocations, not the complete PR versus the old baseline.

## Qualification

Eight checks complete with exit zero: workspace/all-target check, HLE test build, native Wine321 xmem tests in each mode, no-diagnostics check, Clippy, app build and completion receipt. Each native mode has 19 passing functions and one ignored benchmark: 38 positive test executions. Cases include alignment, bounds, allocation/free, section aliases/remapping and the new dirty-page reset fixture. Clippy reports one pre-existing sort warning in unchanged GPU critical.rs; it is not warning-free.

Candidate binary SHA256: 00ac06c868c4d2b9845d606ab94814eccc19ad0299d1c48993002fce0a1f9de0.

The only production source change is xmem.rs. Wine memory fixes, DS4, shader translations, wave support, synchronization, pixel counts and geometry capacities stay as in the qualified baseline. LAND_ECHO=1 and CARD_AWAKE=1 are requested in both game modes. No lazy landing or no-access page machinery is imported from NVIDIA #1184.

## Scene comparison

Four fresh-seed 180-second runs complete in off/on/on/off order:

| Watch large | Run | Warm ms | Inverse mean FPS | Mean draws |
|---|---|---:|---:|---:|
| 0 | 20261128T164017Z | 158.436 | 6.312 | 1043.5 |
| 1 | 20261128T164420Z | 160.845 | 6.217 | 1098.9 |
| 1 | 20261128T165127Z | 158.527 | 6.308 | 1125.0 |
| 0 | 20261128T165540Z | 158.973 | 6.290 | 1094.7 |

All stop143 and have zero warm compiling. Three kernel intervals are empty; the first enabled run has one compute-ring fence fallback warning, without reset or page fault. The same warning has occurred in unmodified b0 runs and is not unique to this candidate.

Pooled warm log means: off158.705 ms / 6.301 FPS versus on159.686 ms / 6.262 FPS. Mean draws differ1069.1 /1112.0, so this is not an exact causal comparison. There is no reliable FPS gain and no promotion.

The intended copying effect is real. Unwatched whole copies fall34.16→0.34 MB/frame and5.69→0.10 ms/frame. Upload bus traffic falls51.82→31.54 MB/frame; fetched bytes stay70.10/69.86 MB/frame. Total CPU copying falls36.30→33.87 ms and preparation76.69→73.72 ms (means of logged windows). These counters overlap and cannot be added as critical-path savings.

The launcher restored and SHA-verified b0 after both pairs. The qualified source stays isolated for future memory work. A CPU-side copying reduction alone does not establish a local FPS improvement or sustained20FPS.

Local evidence: perf20/largexmem1630-validation.json, largexmem1630-results.json, both run indexes, analysis script and stage logs.
