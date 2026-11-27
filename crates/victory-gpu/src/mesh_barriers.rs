//! Precise dependencies for the software mesh path's private scratch buffers.
//! Output is written by compute, then read by the passthrough vertex shader
//! and index fetch. The private command buffer is written by the converter
//! and consumed as indirect arguments. Neither is a pixel-shader resource.
//!
//! These buffers use enhanced barriers throughout when explicitly enabled and
//! supported. The legacy path remains intact. Mesh readback instrumentation
//! and command recording use legacy barriers, so those modes retain that path
//! for the whole process. No resource switches models within a submission.

use std::sync::OnceLock;
use windows::Win32::Graphics::Direct3D12::*;
use windows::core::Interface;

#[derive(Clone, Copy)]
pub(crate) enum Access {
    Compute,
    Vertices,
    Arguments,
}

impl Access {
    fn scope(self) -> (D3D12_BARRIER_SYNC, D3D12_BARRIER_ACCESS) {
        match self {
            Self::Compute => (
                D3D12_BARRIER_SYNC_COMPUTE_SHADING,
                D3D12_BARRIER_ACCESS_UNORDERED_ACCESS,
            ),
            Self::Vertices => (
                D3D12_BARRIER_SYNC_VERTEX_SHADING | D3D12_BARRIER_SYNC_INDEX_INPUT,
                D3D12_BARRIER_ACCESS_SHADER_RESOURCE | D3D12_BARRIER_ACCESS_INDEX_BUFFER,
            ),
            Self::Arguments => (
                D3D12_BARRIER_SYNC_EXECUTE_INDIRECT,
                D3D12_BARRIER_ACCESS_INDIRECT_ARGUMENT,
            ),
        }
    }
    fn legacy(self) -> D3D12_RESOURCE_STATES {
        match self {
            Self::Compute => D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
            Self::Vertices => {
                D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE | D3D12_RESOURCE_STATE_INDEX_BUFFER
            }
            Self::Arguments => D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT,
        }
    }
}

pub(crate) fn supported(device: &ID3D12Device) -> bool {
    let mut options = D3D12_FEATURE_DATA_D3D12_OPTIONS12::default();
    // SAFETY: a live device and a correctly sized feature-query output.
    unsafe {
        device.CheckFeatureSupport(
            D3D12_FEATURE_D3D12_OPTIONS12,
            (&raw mut options).cast(),
            std::mem::size_of_val(&options) as u32,
        )
    }
    .is_ok()
        && options.EnhancedBarriersSupported.as_bool()
}

pub(crate) struct MeshBarriers<'a> {
    list: &'a ID3D12GraphicsCommandList,
    enhanced: Option<ID3D12GraphicsCommandList7>,
}

impl<'a> MeshBarriers<'a> {
    pub(crate) fn new(list: &'a ID3D12GraphicsCommandList) -> Self {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        let enabled = *ENABLED.get_or_init(|| {
            let asked =
                std::env::var_os("VICTORY_MESH_ENHANCED_BARRIERS").is_some_and(|v| v == "1");
            let capture = std::env::var_os("VICTORY_MESH_CAPTURE").is_some();
            #[cfg(feature = "diagnostics")]
            let recording = crate::replay::record::on();
            #[cfg(not(feature = "diagnostics"))]
            let recording = false;
            asked
                && !crate::mesh_shared::enabled()
                && !capture
                && !recording
                && crate::host::host().is_some_and(|h| supported(&h.device))
        });
        let enhanced = enabled.then(|| list.cast().ok()).flatten();
        static LOGGED: OnceLock<()> = OnceLock::new();
        LOGGED.get_or_init(|| tracing::info!(active = enhanced.is_some(),
            "private mesh buffers use enhanced barriers (VICTORY_MESH_ENHANCED_BARRIERS=1; hardware support required)"));
        Self { list, enhanced }
    }

    #[cfg(test)]
    pub(crate) fn enhanced_for_test(&self) -> bool {
        self.enhanced.is_some()
    }

    #[cfg(test)]
    fn explicit(list: &'a ID3D12GraphicsCommandList, enhanced: bool) -> Self {
        Self {
            list,
            enhanced: enhanced.then(|| {
                list.cast()
                    .expect("a command list supporting enhanced barriers")
            }),
        }
    }

    /// Initial buffers are COMMON. Enhanced-mode buffers are used only by
    /// enhanced barriers: their first buffer access needs no transition.
    pub(crate) fn begin(&self, buffer: &ID3D12Resource, first: bool) {
        if self.enhanced.is_none() && first {
            // SAFETY: the caller's private buffer starts COMMON.
            unsafe {
                self.list.ResourceBarrier(&[crate::host::transition(
                    buffer,
                    D3D12_RESOURCE_STATE_COMMON,
                    D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
                )])
            };
        }
    }

    pub(crate) fn between(&self, buffer: &ID3D12Resource, before: Access, after: Access) {
        if let Some(list) = &self.enhanced {
            let (from, to) = (before.scope(), after.scope());
            let barriers = [D3D12_BUFFER_BARRIER {
                SyncBefore: from.0,
                SyncAfter: to.0,
                AccessBefore: from.1,
                AccessAfter: to.1,
                // SAFETY: a borrowed resource, kept by the host list until completion.
                pResource: unsafe { std::mem::transmute_copy(buffer) },
                Offset: 0,
                Size: u64::MAX,
            }];
            // SAFETY: live private buffer and command list; stages match their accesses.
            unsafe {
                list.Barrier(&[D3D12_BARRIER_GROUP {
                    Type: D3D12_BARRIER_TYPE_BUFFER,
                    NumBarriers: 1,
                    Anonymous: D3D12_BARRIER_GROUP_0 {
                        pBufferBarriers: barriers.as_ptr(),
                    },
                }])
            };
        } else {
            // SAFETY: unchanged legacy transitions for the same private buffers.
            unsafe {
                self.list.ResourceBarrier(&[crate::host::transition(
                    buffer,
                    before.legacy(),
                    after.legacy(),
                )])
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
    use windows::Win32::Graphics::Dxgi::Common::*;

    /// Alternating compute writes change both vertices and indices before
    /// 128 indexed draws. Every draw has a distinct target tile, so a later
    /// correct draw cannot hide an earlier read/reuse ordering failure.
    #[test]
    fn compute_vertex_index_reuse_matches_legacy_pixels_on_gpu() {
        let host = crate::host::host().expect("a hardware host");
        assert!(
            supported(&host.device),
            "this capability regression requires enhanced barriers"
        );
        const CS: &str = r#"
#define RS "UAV(u0), RootConstants(num32BitConstants=1,b0)"
RWByteAddressBuffer data : register(u0);
cbuffer C : register(b0) { uint side; }
[RootSignature(RS)] [numthreads(1,1,1)] void main() {
    for (uint i=0; i<6; i++) data.Store2(i*8,asuint(float2(3,3)));
    uint first=side*3;
    float left=side==0 ? -0.9 : 0.1;
    data.Store2(first*8,asuint(float2(left,-0.8)));
    data.Store2((first+1)*8,asuint(float2(left+0.8,-0.8)));
    data.Store2((first+2)*8,asuint(float2(left+0.4,0.8)));
    data.Store3(64,uint3(first,first+1,first+2));
}
"#;
        const GRAPHICS: &str = r#"
#define RS "SRV(t0), RootConstants(num32BitConstants=4,b0)"
ByteAddressBuffer data : register(t0);
cbuffer C : register(b0) { float4 color; }
struct Out { float4 p : SV_Position; float4 c : COLOR; };
[RootSignature(RS)] Out vertex(uint id:SV_VertexID) {
    Out o; o.p=float4(asfloat(data.Load2(id*8)),0,1); o.c=color; return o;
}
[RootSignature(RS)] float4 pixel(Out i):SV_Target { return i.c; }
"#;
        let cs = crate::host::compile(CS, windows::core::s!("main"), windows::core::s!("cs_5_1"))
            .unwrap();
        let vs = crate::host::compile(
            GRAPHICS,
            windows::core::s!("vertex"),
            windows::core::s!("vs_5_1"),
        )
        .unwrap();
        let ps = crate::host::compile(
            GRAPHICS,
            windows::core::s!("pixel"),
            windows::core::s!("ps_5_1"),
        )
        .unwrap();
        const TILES: u32 = 64;
        const WIDTH: u32 = TILES * 8;
        const HEIGHT: u32 = 4;
        const ROW: u32 = WIDTH * 4;
        // SAFETY: owned GPU resources, valid shader bytecode and descriptors;
        // all mappings are read only after the actual submission fence.
        unsafe {
            let cr: ID3D12RootSignature = host.device.CreateRootSignature(0, &cs).unwrap();
            let gr: ID3D12RootSignature = host.device.CreateRootSignature(0, &vs).unwrap();
            let cp: ID3D12PipelineState = host
                .device
                .CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
                    pRootSignature: std::mem::transmute_copy(&cr),
                    CS: D3D12_SHADER_BYTECODE {
                        pShaderBytecode: cs.as_ptr().cast(),
                        BytecodeLength: cs.len(),
                    },
                    ..Default::default()
                })
                .unwrap();
            let mut gd = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
                pRootSignature: std::mem::transmute_copy(&gr),
                VS: D3D12_SHADER_BYTECODE {
                    pShaderBytecode: vs.as_ptr().cast(),
                    BytecodeLength: vs.len(),
                },
                PS: D3D12_SHADER_BYTECODE {
                    pShaderBytecode: ps.as_ptr().cast(),
                    BytecodeLength: ps.len(),
                },
                SampleMask: u32::MAX,
                RasterizerState: D3D12_RASTERIZER_DESC {
                    FillMode: D3D12_FILL_MODE_SOLID,
                    CullMode: D3D12_CULL_MODE_NONE,
                    DepthClipEnable: true.into(),
                    ..Default::default()
                },
                DepthStencilState: D3D12_DEPTH_STENCIL_DESC {
                    DepthEnable: false.into(),
                    StencilEnable: false.into(),
                    DepthWriteMask: D3D12_DEPTH_WRITE_MASK_ZERO,
                    DepthFunc: D3D12_COMPARISON_FUNC_ALWAYS,
                    ..Default::default()
                },
                PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
                NumRenderTargets: 1,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                ..Default::default()
            };
            gd.RTVFormats[0] = DXGI_FORMAT_R8G8B8A8_UNORM;
            gd.BlendState.RenderTarget[0] = D3D12_RENDER_TARGET_BLEND_DESC {
                SrcBlend: D3D12_BLEND_ONE,
                DestBlend: D3D12_BLEND_ZERO,
                BlendOp: D3D12_BLEND_OP_ADD,
                SrcBlendAlpha: D3D12_BLEND_ONE,
                DestBlendAlpha: D3D12_BLEND_ZERO,
                BlendOpAlpha: D3D12_BLEND_OP_ADD,
                LogicOp: D3D12_LOGIC_OP_NOOP,
                RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
                ..Default::default()
            };
            let gp: ID3D12PipelineState = host.device.CreateGraphicsPipelineState(&gd).unwrap();
            let render = |enhanced: bool| -> Vec<u8> {
                let buffer = host.uav_buffer(256).unwrap();
                let mut target: Option<ID3D12Resource> = None;
                host.device
                    .CreateCommittedResource(
                        &D3D12_HEAP_PROPERTIES {
                            Type: D3D12_HEAP_TYPE_DEFAULT,
                            ..Default::default()
                        },
                        D3D12_HEAP_FLAG_NONE,
                        &D3D12_RESOURCE_DESC {
                            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
                            Width: u64::from(WIDTH),
                            Height: HEIGHT,
                            DepthOrArraySize: 1,
                            MipLevels: 1,
                            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                            SampleDesc: DXGI_SAMPLE_DESC {
                                Count: 1,
                                Quality: 0,
                            },
                            Flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
                            ..Default::default()
                        },
                        D3D12_RESOURCE_STATE_RENDER_TARGET,
                        None,
                        &mut target,
                    )
                    .unwrap();
                let target = target.unwrap();
                let rtv: ID3D12DescriptorHeap = host
                    .device
                    .CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                        Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                        NumDescriptors: 1,
                        ..Default::default()
                    })
                    .unwrap();
                let handle = rtv.GetCPUDescriptorHandleForHeapStart();
                host.device.CreateRenderTargetView(&target, None, handle);
                let (back, mapping) = host.shared_buffer(u64::from(ROW * HEIGHT)).unwrap();
                std::ptr::write_bytes(mapping, 0xee, (ROW * HEIGHT) as usize);
                let list = host.new_list().unwrap();
                let barriers = MeshBarriers::explicit(&list.list, enhanced);
                barriers.begin(&buffer, true);
                list.list.OMSetRenderTargets(1, Some(&handle), false, None);
                list.list.ClearRenderTargetView(handle, &[0.0; 4], None);
                list.list
                    .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                list.list.IASetIndexBuffer(Some(&D3D12_INDEX_BUFFER_VIEW {
                    BufferLocation: buffer.GetGPUVirtualAddress() + 64,
                    SizeInBytes: 12,
                    Format: DXGI_FORMAT_R32_UINT,
                }));
                for tile in 0..TILES {
                    list.list.RSSetViewports(&[D3D12_VIEWPORT {
                        TopLeftX: (tile * 8) as f32,
                        TopLeftY: 0.0,
                        Width: 8.0,
                        Height: 4.0,
                        MinDepth: 0.0,
                        MaxDepth: 1.0,
                    }]);
                    list.list.RSSetScissorRects(&[RECT {
                        left: (tile * 8) as i32,
                        top: 0,
                        right: (tile * 8 + 8) as i32,
                        bottom: 4,
                    }]);
                    for side in 0..2u32 {
                        list.list.SetComputeRootSignature(&cr);
                        list.list.SetPipelineState(&cp);
                        list.list
                            .SetComputeRootUnorderedAccessView(0, buffer.GetGPUVirtualAddress());
                        list.list.SetComputeRoot32BitConstant(1, side, 0);
                        list.list.Dispatch(1, 1, 1);
                        barriers.between(&buffer, Access::Compute, Access::Vertices);
                        list.list.SetGraphicsRootSignature(&gr);
                        list.list.SetPipelineState(&gp);
                        list.list
                            .SetGraphicsRootShaderResourceView(0, buffer.GetGPUVirtualAddress());
                        let color = if side == 0 {
                            [1.0f32, 0.0, 0.0, 1.0]
                        } else {
                            [0.0, 0.0, 1.0, 1.0]
                        };
                        list.list
                            .SetGraphicsRoot32BitConstants(1, 4, color.as_ptr().cast(), 0);
                        list.list.DrawIndexedInstanced(3, 1, 0, 0, 0);
                        barriers.between(&buffer, Access::Vertices, Access::Compute);
                    }
                }
                list.list.ResourceBarrier(&[crate::host::transition(
                    &target,
                    D3D12_RESOURCE_STATE_RENDER_TARGET,
                    D3D12_RESOURCE_STATE_COPY_SOURCE,
                )]);
                let destination = D3D12_TEXTURE_COPY_LOCATION {
                    pResource: std::mem::transmute_copy(&back),
                    Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                    Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                        PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                            Offset: 0,
                            Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                                Width: WIDTH,
                                Height: HEIGHT,
                                Depth: 1,
                                RowPitch: ROW,
                            },
                        },
                    },
                };
                let source = D3D12_TEXTURE_COPY_LOCATION {
                    pResource: std::mem::transmute_copy(&target),
                    Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                    Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                        SubresourceIndex: 0,
                    },
                };
                list.list
                    .CopyTextureRegion(&destination, 0, 0, 0, &source, None);
                list.close();
                host.execute(&[&list]);
                assert!(host.device.GetDeviceRemovedReason().is_ok());
                std::slice::from_raw_parts(mapping, (ROW * HEIGHT) as usize).to_vec()
            };
            let legacy = render(false);
            let enhanced = render(true);
            assert_eq!(enhanced, legacy, "all pixels of all 128 draws match");
            for tile in 0..TILES {
                for (x, want) in [
                    (tile * 8 + 2, [255, 0, 0, 255]),
                    (tile * 8 + 5, [0, 0, 255, 255]),
                ] {
                    let offset = (2 * ROW + x * 4) as usize;
                    assert_eq!(&enhanced[offset..offset + 4], want, "tile {tile}, x {x}");
                }
            }
            println!(
                "MESH_BARRIER_TEST active=true 128 indexed draws matched legacy and analytical pixels"
            );
        }
    }

    #[test]
    fn compute_indirect_argument_reuse_preserves_every_dispatch_on_gpu() {
        let host = crate::host::host().expect("a hardware host");
        assert!(supported(&host.device));
        const PRODUCE: &str = r#"
#define RS "UAV(u0), RootConstants(num32BitConstants=1,b0)"
RWByteAddressBuffer args:register(u0);
cbuffer C:register(b0) {uint n;}
[RootSignature(RS)] [numthreads(1,1,1)] void main() {args.Store3(0,uint3(n,1,1));}
"#;
        const CONSUME: &str = r#"
#define RS "UAV(u0), RootConstants(num32BitConstants=1,b0)"
RWByteAddressBuffer result:register(u0);
cbuffer C:register(b0) {uint tag;}
[RootSignature(RS)] [numthreads(1,1,1)] void main(uint3 id:SV_DispatchThreadID) {result.Store(id.x*4,tag+id.x);}
"#;
        unsafe {
            let make = |source: &str| -> (ID3D12RootSignature, ID3D12PipelineState) {
                let code = crate::host::compile(
                    source,
                    windows::core::s!("main"),
                    windows::core::s!("cs_5_1"),
                )
                .unwrap();
                let root: ID3D12RootSignature = host.device.CreateRootSignature(0, &code).unwrap();
                let pso = host
                    .device
                    .CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
                        pRootSignature: std::mem::transmute_copy(&root),
                        CS: D3D12_SHADER_BYTECODE {
                            pShaderBytecode: code.as_ptr().cast(),
                            BytecodeLength: code.len(),
                        },
                        ..Default::default()
                    })
                    .unwrap();
                (root, pso)
            };
            let (pr, pp) = make(PRODUCE);
            let (cr, cp) = make(CONSUME);
            let desc = [D3D12_INDIRECT_ARGUMENT_DESC {
                Type: D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH,
                ..Default::default()
            }];
            let mut signature: Option<ID3D12CommandSignature> = None;
            host.device.CreateCommandSignature(
                &D3D12_COMMAND_SIGNATURE_DESC {
                    ByteStride: 12, NumArgumentDescs: 1, pArgumentDescs: desc.as_ptr(), NodeMask: 0,
                }, None::<&ID3D12RootSignature>, &mut signature,
            ).unwrap();
            let signature = signature.unwrap();
            for enhanced in [false, true] {
                let args = host.uav_buffer(256).unwrap();
                let outputs: Vec<_> = (0..32).map(|_| host.shared_buffer(64).unwrap()).collect();
                for (_, p) in &outputs {
                    std::ptr::write_bytes(*p, 0xee, 64);
                }
                let list = host.new_list().unwrap();
                let barriers = MeshBarriers::explicit(&list.list, enhanced);
                barriers.begin(&args, true);
                for (round, (out, _)) in outputs.iter().enumerate() {
                    list.list.SetComputeRootSignature(&pr);
                    list.list.SetPipelineState(&pp);
                    list.list
                        .SetComputeRootUnorderedAccessView(0, args.GetGPUVirtualAddress());
                    list.list
                        .SetComputeRoot32BitConstant(1, (round % 8 + 1) as u32, 0);
                    list.list.Dispatch(1, 1, 1);
                    barriers.between(&args, Access::Compute, Access::Arguments);
                    list.list.SetComputeRootSignature(&cr);
                    list.list.SetPipelineState(&cp);
                    list.list
                        .SetComputeRootUnorderedAccessView(0, out.GetGPUVirtualAddress());
                    list.list
                        .SetComputeRoot32BitConstant(1, 0x1000 * (round as u32 + 1), 0);
                    list.list.ExecuteIndirect(&signature, 1, &args, 0, None, 0);
                    barriers.between(&args, Access::Arguments, Access::Compute);
                }
                list.close();
                host.execute(&[&list]);
                assert!(host.device.GetDeviceRemovedReason().is_ok());
                for (round, (_, p)) in outputs.iter().enumerate() {
                    let got = std::slice::from_raw_parts(p.cast::<u32>(), 16);
                    for (i, &value) in got.iter().enumerate() {
                        let want = if i < round % 8 + 1 {
                            0x1000 * (round as u32 + 1) + i as u32
                        } else {
                            0xeeeeeeee
                        };
                        assert_eq!(
                            value, want,
                            "enhanced {enhanced}, dispatch {round}, word {i}"
                        );
                    }
                }
            }
            println!(
                "MESH_ARGUMENT_TEST active=true 32 argument rewrites preserved every indirect dispatch"
            );
        }
    }
}
