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
regression and all six async-lane tests pass. It still needs a crash replay
before describing it as a crash fix; see the
VIctory draft and ../linux-rx5500-investigation.md.

Imports from this repository retain their attribution. PRs #151, #200, #271,
#351, #375, #376 and #377 are dependencies/previous work, not newly authored
contributions. The full experimental branch is not an appropriate PR payload.
