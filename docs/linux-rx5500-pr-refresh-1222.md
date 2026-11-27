# RX 5500 XT PR refresh and next measurements, 2026-11-27

The 20-FPS objective is not achieved; visible opening-scene live performance
is around 5 FPS. The refresh covers 3297 open PRs, all 39 pages across
eight known repositories, and identifies 46 new/changed heads since11:21UTC.
Metadata and complete descriptions are retained on the SSD. This is full
inventory coverage, not a claim that all3297patches were source-reviewed.
See the [refresh summary](../../perf20/pr-refresh-1222-summary.json),
[review matrix](../../perf20/pr-refresh-1222-review-matrix.json), and
[validation manifest](../../perf20/pr1222-validation.json).

All eight main refs were fetched; actual default branches were then checked.
myokex uses vi-fx3 (aa8399e5b), which was also fetched.
Belobug's actual default is a-publier
(4eb6cfdf9, unchanged), not main(4f62c2a5e); main is a diverged olderline
with a very different snapshot. It must not overwrite our integration.

Imported candidates: Belobug#559 retains both native textures an aliascopy
uses (a90f7ae4b); its actualGPUreference-lifetime testpasses. Belobug#565
fixes root-SRV global16bit loads crossing a dword boundary: the adapted
GPUregression fails at byte3 beforethefix (779e0be06), with0x00f2 where
0x17f2 was expected. Productioncorrection fbbe96998 preserveshalfview and
native16metadata, and uses translator8 toavoidV6/V7 experimentcachekeys.
The corrected regression passes all eight configurations (384 lane results).
Windows all-target workspace check, app build, and 30 targeted checks pass.
No independent WARP coverage or live FPS gain is claimed. The first live
control run is 20261127T124556Z at the original 1024 MiB buffer budget.

The new Belobug#578half-table port overlaps our existing code (halves.rs is
identical). fx1066targetsserialrelayloops; our fork already defaults to
LANDparalleljobs with a shared mergingRWlock. Replacing it would lose
newerlanding/timeline/beside logic; the uncommitted directportwasreverted.
Mar3487batchescalls in a different Viewed/StepHeap/StepMemory architecture.
Its recent cache-expiration fix is essential if adapting anypart.
The two main indirectclamp alternatives2472/3197overlap eachother and our
already active indirectwindow. In our capturedframe argumentclamps are
131calls /0.57ms, so importing another entireclamp path is not justified
by their separate NVIDIA examples. Native-mesh-onlylaunchPRs remain
inapplicable tothis adapter; the7900#459Win10bundle stillfailsworldentry.

The source for569 correctly reports disconnected sectionimports returning
S_OK. Our earlier private4MiBprobe passes but sectionimports fail too.
Coherency validation doesnot solve the known importedheap/VA lifetime
problem at remap or leave_home, so keepinplace/ports off pendingthat work.

## Captured-work evidence

Recorder fixes from existing7900#312 and Belobug550/551 arequalified.
Own guard8b653a464 uses legacycopybarriers whilerecording and keeps
inplacecopy ordering. Capture115113, opening-b/capture1 has44submissions,
302lists,32404resources,1388pipelines and no omittedBarrier call.
938otherqueue sends(awake/copyback) are omitted; imagequalification remains
partofeach replay. It is a single capturedpresentedframe: Armed waits
forfullyrecordedlists beforestarting Capture, and the earlierwaited3
presents are priming, not threecapturedframes.

Default restoredreplay115956 exits0,kernel0: loops175.12/145.67/147.53ms.
Image57.3dB; meanabsolutechannelerror0.079/255; only262of2073600pixels
haveanychanneloffbymorethan8. Restorecopiesconsume2396MiB(1393inPC),
leaving93MiBbelowqueriedVRAMbudget. The warning saysoverbudget when
used+256>budget; this is a reserveheuristic, not proofactualmigration.
Do not declare ahardwarefloor.

Freshsingleframereplay120356(no-restoresnapshots)uses6293MiBof7398MiB
and takes165.16ms, image57.4dB. It alsoremovesthe13mergecopies that
requiremasters, so it doesnot isolateexactlythecostofVRAMheadroom.
No later stale-state loops were used.

Detailed restoredreplay120559 exits0,kernel0, image57.4dB, median150.85ms.
Diagnostic categorytimes: indirectdispatch44.68ms/4062,dispatch32.61ms/6604,
indirectdraw23.74ms/3475, barriers14.87ms/15097, owncard-to-PCcopies7.66ms,
PC-to-card6.92ms, draws6.22ms, ownhash/compare5.56ms. Timestamps add
overhead and restoredmemorypressure remains; use ranking, not liveFPS.
No singlepipeline dominates (largest5.56ms).

## Next performance work

1. Finish distinctcorrectnessqualification, then runbaseline/candidate with
allwork intact. Preserve DS4 and the workingWineserver memoryfix.
2. Measure increased hotbuffer VRAMbudget1536MiB against1024 ononebinary,
usingheadroom freedbysharedmeshstorage, and monitoractualresidentusage.
3. Qualify ordinarynonstalling listtimestamps(PR453) toseparatetitleGPU
work fromCPUcopy/remap gaps; do not reuseGPU_TIMING'sserializedwaits.
4. Investigate batched softwaremesh compute/draw windows. The fallback's
originalsyntheticbenchmark showed a large batchingopportunity. A sound
window must preserve read/write order, query/timestamp semantics and
allmeshoutputs. CPU-known disjointoutputslots reservingfulloriginal
perrecordcapacity avoidGPUtruncationorDGCrequirements. Record eligible
meshcompute before its drawsegment onlywhen prior draws cannotmodify
itsinputs; flush on anyhazard. Retain outputbuffers and commandresources
throughGPUcompletion. Globalsharedstorage maybereused onlyafter every
draw ofthewindow hasfinished. Testindexed/expanded,multirecord,budget
limits,producer-consumerhazards andmanyretainedlists. Do notclaimsuccess
fromskippingdraws, clampingoutputs oronlyusinglesspixels.

Publishing is complete in anotherchat, pertheuser. Thisthread must not
create PRs orpostcomments. Onlylocaltesting anddocumentationcontinue.
