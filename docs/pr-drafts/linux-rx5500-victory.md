# Draft for Marvisak/VIctory — not submitted

Proposed title: Retain emulated fences until queued work completes

Queued waits and signals must retain the fence after the caller releases its COM
reference. The imported fence-ownership change adds an RAII fence reference and
transfers it through held queue entries and host late signals.

This local candidate additionally has an async submission lane. Its waits,
signals, and signals folded into joined submissions need the same ownership.
That module is fork-specific; include it only if the target base contains it.

## Attribution and overlap

The shared ownership change comes from
[fxceless/victory-fork #404](https://github.com/fxceless/victory-fork/pull/404).
Check that PR and the target's current lifetime fixes before opening anything.
Prefer contributing the missing path and tests to an existing applicable PR.
This draft is not a reason to duplicate that work.

## Evidence and validation

The old code stores raw fence pointers in asynchronous entries. The regression
must release the caller's reference, then complete or discard queued work,
checking both correct completion and the final reference count.

Local commits 80548d943 (attributed import) and 7fe2b6869 (async-lane extension)
contain the changes. The Windows workspace/all-target check, the held-fence
regression and all six async-lane tests pass under Wine. A six-minute 3D observation window completed without the earlier exception
or a logged GPU reset, then the controller stopped it. This is one replay,
not proof that the lifetime defect caused the earlier crash. A Linux access violation passed through an immediate queue-signal
callback, but a causal link to fence ownership has not been established. Do not
claim this fixes that crash until a replay confirms it.

## Other changes need their own bases

- Mesh multi-group index correction (978fd7402): only applicable where the
  compute fallback exists. Keep separate from the fence PR.
- Linear-sampler mapping: only applicable with the source root-signature reader;
  keep with that implementation or its existing PR.
- Existing DS4 PR #1786 is retained locally; no duplicate DS4 submission.
- Wine server backport, ntsync exposure, local vkd3d DLL build, machine-specific
  launch flags and reduced output dimensions are environment setup, not this PR.

The investigation report is ../linux-rx5500-investigation.md. No 20-FPS claim is
supported. Later controlled allocation comparisons complete without a logged
exception/reset and reach about 5.6 FPS; geometry and lighting remain incomplete.

## Cache performance change belongs to the 7900XTX fork

The fetched Marvisak main already uses weak exact-view cache entries,
size-triggered pruning and shared arena buffers (6cc6aa352). It does not have
the same per-miss full retain scan found in the tested fork.

Local commit 37d89d3b5 is therefore a 7900XTX/fork stopgap, not a patch to submit
unchanged here. Its real-resource regression and optimized Windows builds pass;
its cache-only replay completed; that run does not isolate a causal FPS gain. Review the other draft for that independent change.
A broader port of upstream's arena should preserve all slice offsets, descriptor
addresses and refresh/lifetime rules, and needs its own validation.

## Additional fork-only adaptations

The 7900XTX draft records b7a32f2ed (placed-resource pooling) and d9e5ad914
(smaller first mesh chunks). Main already has the denser shared-buffer arena;
do not submit a duplicate pool implementation here. The mesh sizing originates
in upstream PR #961 and should be coordinated with that fallback, rather than
presented as newly invented code.

The view-pool live A/B improvement is about 3.74 to 5.15 FPS in the existing
feature profile. The ray-tracing structure builder is disabled in that profile.
No complete-rendering or 20-FPS claim is supported.

The later independent mesh-size A/B measured 4.75 to 5.62 FPS at similar draw
counts, with view pooling fixed on. This still does not verify sustained 20 FPS.
The refreshed Belobug merge is separately qualified; do not attribute the
measured gains to every new fork change.


## Overnight qualification

The later controlled tests found no material FPS improvement from guard/journal
tracking or destroyed-list mesh reuse; those experiments remain off. Bounded
compute waits also left the missing body/scenery unchanged in a six-minute
replay. None should be presented as a demonstrated performance fix. The report
records their tests, attribution and limits.

Per-root UAV write tracking from Marvisak #2948 is the next isolated experiment,
including its root-address correctness follow-up. It must be reviewed against
that existing PR rather than submitted as duplicate new work. Menu changes are
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
