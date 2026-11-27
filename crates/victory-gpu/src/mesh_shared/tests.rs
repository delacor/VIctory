use windows::Win32::Graphics::Direct3D12::*;
use windows::core::Interface;
use crate::host::ScratchPool;
use crate::mesh_barriers::{Access, MeshBarriers};

/// Real GPU reads must see each list's own producer, even though all lists
/// are recorded before submission and share output storage. Check a batch
/// and separate ExecuteCommandLists calls, including arena growth.
#[test]
#[ignore = "isolated GPU process with VICTORY_MESH_SHARED=1"]
fn shared_output_preserves_many_recorded_lists_on_gpu() {
    assert!(super::enabled(), "VICTORY_MESH_SHARED=1 required");
    let host = crate::host::host().expect("hardware host");
    const WORDS: u32 = 1024;
    const LISTS: u32 = 32;
    const SOURCE: &str = r#"
#define PRODUCE "UAV(u0), RootConstants(num32BitConstants=1,b0)"
#define CONSUME "SRV(t0), UAV(u1), RootConstants(num32BitConstants=1,b0)"
RWByteAddressBuffer target : register(u0);
ByteAddressBuffer input : register(t0);
RWByteAddressBuffer output : register(u1);
cbuffer C : register(b0) { uint value; }
[RootSignature(PRODUCE)] [numthreads(64,1,1)]
void produce(uint3 id:SV_DispatchThreadID) {
    target.Store(id.x*4, value ^ (id.x*2654435761u));
}
[RootSignature(CONSUME)] [numthreads(64,1,1)]
void consume(uint3 id:SV_DispatchThreadID) {
    output.Store((value*1024+id.x)*4, input.Load(id.x*4));
}
"#;
    let cs = |entry| crate::host::compile(SOURCE, entry, windows::core::s!("cs_5_1")).unwrap();
    let producer = cs(windows::core::s!("produce"));
    let consumer = cs(windows::core::s!("consume"));
    // SAFETY: every object is held through a synchronous submission fence.
    unsafe {
        let make = |code: &[u8]| {
            let root: ID3D12RootSignature = host.device.CreateRootSignature(0, code).unwrap();
            let pso: ID3D12PipelineState = host.device.CreateComputePipelineState(
                &D3D12_COMPUTE_PIPELINE_STATE_DESC {
                    pRootSignature: std::mem::transmute_copy(&root),
                    CS: D3D12_SHADER_BYTECODE { pShaderBytecode: code.as_ptr().cast(), BytecodeLength: code.len() },
                    ..Default::default()
                }).unwrap();
            (root, pso)
        };
        let (pr, pp) = make(&producer);
        let (cr, cp) = make(&consumer);
        let before = super::bytes();
        for batched in [true, false] {
            let output = host.uav_buffer(u64::from(WORDS * LISTS * 4)).unwrap();
            let (back, mapped) = host.shared_buffer(u64::from(WORDS * LISTS * 4)).unwrap();
            let mut lists = Vec::new();
            let mut addresses = std::collections::BTreeSet::new();
            for slot in 0..LISTS {
                let list = host.new_list().unwrap();
                let region = list.mesh_scratch(ScratchPool::Data,
                    if slot == LISTS/2 { 2 << 20 } else { 4096 }).unwrap();
                assert!(!region.first);
                assert_eq!(region.offset, 0);
                let address = region.buffer.GetGPUVirtualAddress();
                addresses.insert(address);
                list.keep(&region.buffer);
                list.keep(&output);
                let l = &list.list;
                let barriers = MeshBarriers::new(l);
                assert!(!barriers.enhanced_for_test(), "shared output must use legacy barriers");
                barriers.begin(&region.buffer, region.first);
                l.SetComputeRootSignature(&pr); l.SetPipelineState(&pp);
                l.SetComputeRootUnorderedAccessView(0, address);
                l.SetComputeRoot32BitConstant(1, slot + if batched { 100 } else { 200 }, 0);
                l.Dispatch(WORDS/64,1,1);
                barriers.between(&region.buffer, Access::Compute, Access::Vertices);
                l.SetComputeRootSignature(&cr); l.SetPipelineState(&cp);
                l.SetComputeRootShaderResourceView(0, address);
                l.SetComputeRootUnorderedAccessView(1, output.GetGPUVirtualAddress());
                l.SetComputeRoot32BitConstant(2, slot, 0);
                l.Dispatch(WORDS/64,1,1);
                barriers.between(&region.buffer, Access::Vertices, Access::Compute);
                list.mesh_scratch_recycle();
                list.close();
                lists.push(list);
            }
            assert!(addresses.len() <= 2, "32 lists retained too many independent buffers");
            assert!(super::bytes().saturating_sub(before) <= 3 << 20,
                "output storage grew per list rather than per capacity");
            if batched {
                let runs: Vec<_> = lists.iter().rev().collect();
                host.execute(&runs);
            } else {
                for list in &lists { host.execute(&[list]); }
            }
            let copy = host.new_list().unwrap();
            copy.list.ResourceBarrier(&[crate::host::transition(&output,
                D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_COPY_SOURCE)]);
            copy.list.CopyResource(&back, &output);
            copy.close();host.execute(&[&copy]);
            let seen = std::slice::from_raw_parts(mapped.cast::<u32>(), (WORDS*LISTS) as usize);
            for slot in 0..LISTS {
                for word in 0..WORDS {
                    assert_eq!(seen[(slot*WORDS+word) as usize],
                        (slot + if batched { 100 } else { 200 }) ^ word.wrapping_mul(2654435761),
                        "batched={batched}, list={slot}, word={word}");
                }
            }
        }
        eprintln!("MESH_SHARED_TEST 64 lists preserved every GPU word; retained {} bytes", super::bytes());
    }
}
