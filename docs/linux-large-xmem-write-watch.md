# Large private allocation write watch on Linux

The baseline sends about 21 MB/frame up and back for a 0x1412200-byte scratch buffer. Its default heap allocations are not write-watched, so refresh cannot ask the kernel which pages changed. NVIDIA PR #1038 adds watched private reservations for padded allocations at least 1 MiB long, retaining the heap fallback and a WATCH_LARGE_XMEM=0 comparison switch.

Reviewed upstream commits: 783242121aa562499b4c67aff1b9eebaa2c92055 and 7cd552bd64432bd64ad17e83f1c2aea6af925ec6. They were cherry-picked onto 9dcecce2bcc5bc84398ddc130683c98671b1a97e as 276bb4c40 and d29d0d5e3. Local regression commit 1d4fdb9c1822a4ed6b4a31c4b221444836b17df5 adds eight sparse concurrent-write / reset / query generations at minimum, 64 KiB and 2 MiB alignment, in both switch modes. The reservation base is retained in the header and freed through VirtualFree. address_range(0) retains our existing bounds and keeps these allocations above the low 4 GiB used by 32-bit requests.

The PR also tries write watch for eligible explicit-address reservations, falling back to the original flags on refusal. That behavior and the larger allocation header are common to both switch modes. The comparison therefore isolates large default allocations, not the complete PR versus the old baseline.

## Qualification

Eight checks complete with exit zero: workspace/all-target check, HLE test build, native Wine321 xmem tests in each mode, no-diagnostics check, Clippy, app build and completion receipt. Each native mode has 19 passing functions and one ignored benchmark: 38 positive test executions. Cases include alignment, bounds, allocation/free, section aliases/remapping and the new dirty-page reset fixture. Clippy reports one pre-existing sort warning in unchanged GPU critical.rs; it is not warning-free.

Candidate binary SHA256: 00ac06c868c4d2b9845d606ab94814eccc19ad0299d1c48993002fce0a1f9de0.

The only production source change is xmem.rs. Wine memory fixes, DS4, shader translations, wave support, synchronization, pixel counts and geometry capacities stay as in the qualified baseline. LAND_ECHO=1 and CARD_AWAKE=1 are requested in both game modes. No lazy landing or no-access page machinery is imported from NVIDIA #1184.

## Scene comparison

A fresh verified prison seed is used for each 180-second run. Shader compilation is excluded from warm analysis, and captures, clocks, memory and kernel intervals are retained. Results are pending; qualification alone is not evidence of an FPS gain. The launcher restores and checks the known b0 baseline after the pair.

Local evidence: perf20/largexmem1630-validation.json, largexmem1630-runs.json and the stage logs.
