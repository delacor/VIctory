# Independent software mesh windows: experimental candidate

The 20 FPS objective remains open. This branch adds a production scheduling
experiment; no gameplay speedup has been established.

Branch: perf20/mesh-windows-20261127. Production commits da7fc7637 and
cb0f0312b follow the qualified PR-refresh integration. The binary was built
from cb0f0312bb9b6ac413ee80a4620fd7124670e32f, SHA-256
abc38695b6991d34529e088338ef66ea70dc0a4780882234531e708ecaee7136.
Later documentation commits do not identify a different tested binary.

## Work moved

With VICTORY_MESH_WINDOWS=1 and VICTORY_MESH_SHARED=1, consecutive eligible
software mesh calls record their compute passes on the ordinary DIRECT list
and their graphics bindings/draws in a native bundle. The bundle executes
after the window's compute work. Native mesh pipelines retain their path.
The feature is off by default and disabled during frame recording, mesh
readback instrumentation and serialized GPU timing.

Every call reserves a distinct 32 MiB slot, preserving the original capacity.
Eight slots share 256 MiB of output on the existing one-queue arena. Direct
draws requiring more than 32 MiB and indirect calls with multiple records use
the existing path in full. No vertex count, group count or output limit is
reduced by this experiment.

Each converter uses a distinct command-buffer offset. The compute half ends
with the command resource back in UAV state, so later converters may write
other offsets. Before executing the graphics bundle, the window transitions
all command resources to INDIRECT_ARGUMENT and the output to vertex/index
reads. Afterwards both return to UAV. Command scratch is recycled only after
all the window's draws have been recorded and ordered before reuse.

One BUNDLE allocator and a reusable bundle pool belong to each HostList.
They remain owned with the native DIRECT list through submission completion.
HostList::reset closes abandoned open bundles and resets their allocator only
under the existing fenced-reset contract. Allocation failure leaves the
ordinary path available. A bundle-close error is fatal rather than silently
dropping queued draws.

## Admission and boundaries

Admission decodes the same native DXIL/ACCC program chosen by translation.
It rejects unknown or undecodable code, buffer stores/atomics, image
instructions, global GDS operations, hardware-time reads and traps.
Synthetic ambient/probe shader replacements are excluded. The existing
metadata bounds and output layouts remain unchanged.

A window ends when full, when the pipeline or descriptor heaps change, and
before native copies, barriers, clears, queries, syncs, list cuts or Close.
Graphics-state changes flush before reaching the DIRECT list; repeated
identical viewports, scissors and targets may retain the window. Ordinary
compute/draw commands and incompatible mesh paths flush first. Graphics
root constants, views and PSO bindings are captured separately for each draw.
The central native-list accessor provides the GPU ordering boundary; pure
thin-recorder lookups use the non-flushing accessor.

The initial window deliberately admits only native buffer-reading programs.
Extending admission requires checking the generated operations and resource
dependencies, not merely changing a flag.

## Qualification

The isolated SSD target is target-mesh-windows. The Windows all-target
workspace check, GPU test-library build, application build, six existing
mesh-indirect tests and the list-factory test all pass.

The new actual-GPU test renders 128 records per generation with separate
colors and output slots. GPU-generated draw arguments alternate active and
empty records. Every pixel matches the ordinary reference, and analytical
pixel checks confirm both visible and empty tiles. A second recording after
fenced reset brings the total to 256 records and verifies reuse of the same
16 bundle objects. All eight full-capacity slots are distinct; oversized
reservations fail rather than being truncated. Strict Clippy was not run.

This test exercises the production Window resource/command ordering and pool.
The live State-binding path and arbitrary title dependency patterns require
additional validation. The prior shared-output fixture covered many recorded
lists; the new window fixture has not yet independently covered many windowed
HostLists recorded before one batched submission.

## First live smoke

Run 20261127T140208Z completes 181 world seconds, exit 143, with an empty kernel
log. Character, room and outdoor scenery are visible in the captured image.
The final sample is 5.0 FPS/198.5 ms, 1,089 draws, 266 dropped, 0 compiling.
Existing rendering defects and dropped pipelines remain.

The CPU recording counters report 273,408 completed windows and 1,399,644
compute calls, mean 5.119 calls/window. These are cumulative recording calls,
not a per-frame GPU draw count or a throughput proof.

Full same-binary control 20261127T140815Z now runs with only
VICTORY_MESH_WINDOWS changed to 0. After it finishes, flag 1 needs the same
420-world-second test and 120–410 second warm window. Compare draw counts,
compilation, scenery, memory and kernel errors. Small differences in this
changing scene are not sufficient to establish a causal improvement.

## Related investigations

Marvisak PR1385 provides native-mesh allocation-bound analysis. It cannot be
blindly ported into this software fallback: emitted storage pitches, fallback
MeshLayout, passthrough vertex indexing and argument conversion must agree.
Any narrowing must be proved, preserve every valid output, and receive its
own cache separation and negative tests.

The existing replay CLI supports --buffers vram with --only-buffers filters.
A controlled relocation experiment may distinguish shader execution cost
from GPU reads of system-memory inputs. Do not move the whole capture blindly:
the original replay already approached the adapter's VRAM budget.

Evidence lives in report/perf20/mesh-windows-validation.json and
mesh-windows-smoke-summary.json. Private captures stay outside Git.
Publishing remains handled by another chat.
