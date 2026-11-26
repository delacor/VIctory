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
regression and all six async-lane tests pass under Wine. The game replay is
pending. A Linux access violation passed through an immediate queue-signal
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
supported: the latest tested sequence was about 4–5 FPS and crashed.
