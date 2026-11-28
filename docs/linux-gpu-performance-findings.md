# Linux GPU performance findings: RX 5500 XT

These are local experiments from the same opening 3D scene on an 8 GiB RX 5500 XT under Linux/Wine. Runs use saved scene inputs, a fixed rendering profile and warmed shader caches. Draw counts can differ, so the sampled comparisons below are observations rather than precise causal speedups. The current working configuration runs around 6.3 FPS over the measured interval; the 20 FPS goal remains unmet.

Three renderer changes below depend on facilities absent from current upstream main (63bcca43c6a00a758510398fa43096ce7e4f7df0). Their tested source branches and focused comparisons are preserved for porting and review. This document does not activate them.

## GPU constant copies

Small constant-buffer fills can wait for recently GPU-written bytes to return to the CPU. Our implementation copies eligible bytes directly in command order, binds distinct aligned buffers and retains them through completed reset. It handles root and dynamically copied constant views, caps per-recording allocation, refreshes CPU changes and refuses conflicting cases.

This adapts the idea from [fxceless #1468](https://github.com/fxceless/victory-fork/pull/1468); the local admission, lifetime and CPU-conflict handling are separate implementation work.

- [Focused patch](https://github.com/delacor/VIctory/compare/f92ee287994f3aab00be54deee2ca1b4032c6093...d6b303c18113888f4b3386ae3c5efb8ba6adf61d)
- Branch: pr/20261128-gpu-constant-copies.
- Integrated-tree qualification: 16 checks and 66 targeted test-function executions. Native and CPU-reference fixtures cover 32 copies across eight fenced generations, asynchronous CPU updates, ordered conflicts, exact bytes and completed reuse.
- Two seeded 420-second runs, warm seconds 120–410: disabled 195.884 ms / 5.105 FPS / 1103 mean draws; enabled 173.323 ms / 5.770 FPS / 1121 draws. Both stop normally, with empty captured kernel intervals. This is an observed 13% FPS difference with unequal work.
- Dependency: the ordered title-buffer copy engine introduced at 350f7f492, which current main lacks.

A separate [7900-base port](https://github.com/delacor/VIctory/tree/pr/20261128-gpu-constant-copies-7900) targets saved 7900XTX main 2de998014. Its [Wine compiler prerequisite](https://github.com/delacor/VIctory/tree/pr/20261128-wine-card-compiler-7900) reuses our existing DXC fallback from fe10cc821. Without it Wine disables the card-copy engine before the optimization fixture begins. Fresh hardware and CPU-reference tests pass and explicitly reject software adapters. No live game performance result is claimed for that older-base port.

## Textured software-mesh windows

Our readonly mesh-window extension permits texture-reading pixel shaders while keeping output slots disjoint. Texture mutations flush prior closed windows, and state generations force rebinding after a flush during compute or graphics binding. The current draw retains its reserved slot. Admission excludes stores, atomics and unsupported effects.

- [Focused patch](https://github.com/delacor/VIctory/compare/9dcecce2bcc5bc84398ddc130683c98671b1a97e...c00350355021860c3123c9e77363a734575d6d12)
- Branch: pr/20261128-texture-mesh-windows.
- Qualification: nine checks; 512 exact native rendering calls compared with immediate rendering, texture changes during both binding paths, full eight-slot reuse and empty arguments. Admission tests, fetch guard and the earlier 256-record window fixture also pass.
- Four seeded 180-second runs in disabled/enabled/enabled/disabled order: sampled means 166.886 ms disabled and 162.400 ms enabled, about 3%, with varying draw counts.
- One enabled run reports a compute-fence fallback warning also seen on the baseline. No reset or page fault occurs. The extension remains disabled by default pending broader validation.
- Dependencies: shared software-mesh allocation and readonly windows, including da7fc7637 and cb0f0312b. Neither exists on current main.

## Submission-safe diagnostic captures

Saving a readback only when a command list resets can observe bytes overwritten by a later execution. Our bounded pass captures now wait for their actual submission fence and save while holding the submission lock. They use a dedicated event, retain resources through legal closed-list reuse, discard abandoned recordings and record completion metadata.

- [Focused patch](https://github.com/delacor/VIctory/compare/9dcecce2bcc5bc84398ddc130683c98671b1a97e...c81daf238518042abdfeaa0e40c1f43236ffbbfe)
- Branch: pr/20261128-capture-fence-lifetime.
- Qualification: ten checks. The disabled control fails after GPU work is settled. Native tests cover asynchronous persistence, distinct-list batches, closed-list reuse, an actual competing GPU overwrite and abandoned recordings.
- A seeded diagnostic game run saves ten complete argument/payload pairs with matching completion metadata and no invalid bounds.
- Dependency: bounded per-pass snapshots at e858a6c51, absent from current main.
- This is diagnostic correctness work. Capture mode deliberately synchronizes; no FPS gain is claimed.

## Waterfall descriptor escape refusal

The shape proof from [NVIDIA #458](https://github.com/trustno1-coder/VIctory-NVIDIA/pull/458) must reject cases where the loaded uniform descriptor escapes its loop, as well as cases where its first-lane offset escapes. Otherwise folding can replace the final pass's scalar descriptor with each lane's entry.

Our follow-up includes loaded-descriptor SSA values in the escape set and checks outside reads and phi inputs. The regression reads an actual descriptor dword, s12, in both wave widths.

- [Focused guard only](https://github.com/delacor/VIctory/compare/dad477dba62ad0cdb7692eaa5b867b026b514b12...430a7e494a20c86b49d8a867376b51956a31281b)
- Branch: pr/20261128-waterfall-descriptor-escape.
- Fresh qualification on the original author branch: the regression fails before the fix and passes after; 212 CPU shader tests pass, two are ignored. Clippy completes with inherited warnings in unchanged emit helpers.
- The Marvisak port carries the credited shape-proof prerequisite as a separate commit and is qualified separately. This does not claim an FPS improvement or a complete GPU rendering proof.

## Retained large-allocation write watch

The allocation optimization originates in [NVIDIA #1038](https://github.com/trustno1-coder/VIctory-NVIDIA/pull/1038). Its authorship is retained. Our additional regression fixture checks sparse concurrent writes through eight reset/query generations at minimum, 64 KiB and 2 MiB alignments, and verifies release.

- [Preserved selected branch](https://github.com/delacor/VIctory/tree/pr/20261128-large-xmem-write-watch).
- Qualification: 38 native Wine test-function executions, 19 with the allocation switch disabled and 19 enabled.
- Four seeded 180-second runs in disabled/enabled/enabled/disabled order: pooled 158.705 ms / 6.301 FPS disabled and 159.686 ms / 6.262 FPS enabled. No reliable FPS improvement is established.
- Whole-buffer copying falls from 34.16 to 0.34 MB/frame; bus uploads from 51.82 to 31.54 MB/frame. Readback traffic remains about 70 MB/frame.
- Retained for its copying reduction. The allocation idea is not claimed as our original finding.

## What to optimize next

The copying reductions do not remove the main bottleneck. The next investigation is GPU-written memory landing and readback dependency timing, preserving CPU/GPU coherence and ordered conflicts. Lazy landing proposals need syscall-reader, overlap, lifetime and cross-queue ordering checks before a live comparison.

A traced compute shader was matched to native program 8bb272e763f76148 by byte-for-byte retranslation: 4852 bytes, SHA256 d31d882f212d71304869755c2398f99b11f7eda93bb2c4c7d18827f850f6bb6c, RGP hash 0bb2985cba2c1842. Its 1024-thread group, 12 KiB LDS and four barriers are static properties. Trace timings were instrumented and taken at different clocks; they do not establish normal FPS or a hardware ceiling.

Performance branches are independent experiments, not a qualified combination. An upstream port needs its own tests and same-scene comparison. Exact feature-name PR searches found no duplicate for the three local flags, but this is not an exhaustive semantic review of every other proposal.
