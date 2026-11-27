# Experimental shared mesh output storage

VICTORY_MESH_SHARED=1 (off by default) shares software-mesh output buffers
across host lists on the one DIRECT queue. Keep indirect command buffers
private. Preserve all layouts, group counts, regions and allocation capacity.
Reuse complete buffers by capacity; keep earlier allocations owned so growth
cannot invalidate a previously recorded list. Native mesh shaders are unchanged.

The default pool already recycled output after every draw. This experiment
addresses retention across many live host lists, not per-draw reuse.
Inspired by yoraudev's global intermediate storage in Marvisak/VIctory #1648,
d62ba28cc; it does not replace our translator or import its indirect clamp.

Every output use ends with the existing read-to-UAV dependency. Buffers promote
COMMON to UAV on their first access and decay after ExecuteCommandLists.
Independently recorded lists therefore need no guessed initial transition.
The shared mode uses legacy barriers throughout; enhanced mesh barriers are
disabled process-wide when shared mode is selected.

API rules reviewed:
https://learn.microsoft.com/en-us/windows/win32/direct3d12/using-resource-barriers-to-synchronize-resource-states-in-direct3d-12
https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12commandqueue-executecommandlists

Qualification passed: the ignored real GPU regression records 32 lists before
submission, reads each producer through the shared output, checks reverse
batch order and separate submissions, grows the arena and checks every word
and retained bytes. All 64 lists match; retained test storage is 3 MiB.
Windows all-target check, two barrier/image tests, six indirect tests,
inline capture, factory reuse and application build pass.

Live same-binary comparison: shared 195.035 ms (5.127 FPS, 51 samples),
private 199.526 ms (5.012 FPS, 53 samples), warm seconds 120–410.
The changing scene and unequal draw counts limit attribution of the small
FPS difference; the goal of 20 FPS remains unmet. See the investigation
ledger for final/peak memory, limits, logs and the replay smoke-test issue.
