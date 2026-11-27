use super::*;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Dxgi::Common::*;
    #[test]
    #[ignore = "isolated hardware process with shared mesh/windows enabled"]
    fn windows_preserve_pixels_arguments_disjoint_outputs_and_fenced_pool_reuse() {
        let host = crate::host::host().expect("a hardware host");
        assert!(super::enabled(), "VICTORY_MESH_WINDOWS=1 and VICTORY_MESH_SHARED=1 required");
        const CS: &str = r#"
#define RS "UAV(u0), UAV(u1), RootConstants(num32BitConstants=1,b0)"
RWByteAddressBuffer data : register(u0);
RWByteAddressBuffer arguments : register(u1);
cbuffer C : register(b0) { uint side; }
[RootSignature(RS)] [numthreads(1,1,1)] void main() {
    float left=-1.0+float(side)*0.25+0.02;
    data.Store2(0,asuint(float2(left,-0.8)));
    data.Store2(8,asuint(float2(left+0.21,-0.8)));
    data.Store2(16,asuint(float2(left+0.105,0.8)));
    data.Store3(64,uint3(0,1,2));
    // Different active/empty records catch reuse of command offset zero.
    arguments.Store4(0,uint4(side%3==1 ? 0 : 3,1,0,0));
    arguments.Store(16,0);
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
        const ROWS: u32 = 16;
        const WIDTH: u32 = super::SLOTS * 8;
        const HEIGHT: u32 = ROWS * 4;
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
            let op = D3D12_INDIRECT_ARGUMENT_DESC {
                Type: D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED,
                ..Default::default()
            };
            let mut signature: Option<ID3D12CommandSignature> = None;
            host.device.CreateCommandSignature(&D3D12_COMMAND_SIGNATURE_DESC {
                ByteStride: 20, NumArgumentDescs: 1, pArgumentDescs: &op, NodeMask: 0,
            }, None::<&ID3D12RootSignature>, &mut signature).unwrap();
            let signature = signature.unwrap();
            let render = |windowed: bool, list: &crate::host::HostList, generation: u32| -> Vec<u8> {
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
                list.keep(&target); list.keep(&rtv); list.keep(&back); list.keep(&buffer);
                let barriers = MeshBarriers::new(&list.list);
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
                for row in 0..ROWS {
                    list.list.RSSetViewports(&[D3D12_VIEWPORT {
                        TopLeftX: 0.0, TopLeftY: (row*4) as f32,
                        Width: WIDTH as f32, Height: 4.0, MinDepth: 0.0, MaxDepth: 1.0,
                    }]);
                    list.list.RSSetScissorRects(&[RECT { left: 0, top: 0, right: WIDTH as i32, bottom: HEIGHT as i32 }]);
                    let mut window = windowed.then(|| super::Window::new(list, 123, [0;2]).unwrap());
                    if let Some(window) = &mut window {
                        assert!(window.reserve(super::STRIDE+1).is_none(), "never shrink an oversized draw");
                    }
                    let mut addresses = std::collections::BTreeSet::new();
                    for column in 0..super::SLOTS {
                        let region = match &mut window {
                            Some(window) => window.reserve(256).unwrap(),
                            None => crate::host::ScratchRegion { buffer: buffer.clone(), offset: 0, first: false },
                        };
                        let address = region.buffer.GetGPUVirtualAddress()+region.offset;
                        addresses.insert(address);
                        let commands = list.mesh_scratch(crate::host::ScratchPool::Commands, 32).unwrap();
                        list.keep(&commands.buffer);
                        barriers.begin(&commands.buffer, commands.first);
                        list.list.SetComputeRootSignature(&cr); list.list.SetPipelineState(&cp);
                        list.list.SetComputeRootUnorderedAccessView(0,address);
                        list.list.SetComputeRootUnorderedAccessView(1,commands.buffer.GetGPUVirtualAddress()+commands.offset);
                        list.list.SetComputeRoot32BitConstant(2,column,0);
                        if let Some(window) = &mut window { window.wrote(); }
                        list.list.Dispatch(1,1,1);
                        barriers.between(&commands.buffer,Access::Compute,Access::Arguments);
                        if let Some(window) = &mut window {
                            // Same invariant as production: the compute half
                            // consumed its arguments, then the next converter
                            // may write another offset of this command resource.
                            barriers.between(&commands.buffer,Access::Arguments,Access::Compute);
                            window.commands(&commands.buffer);
                        } else {
                            barriers.between(&buffer,Access::Compute,Access::Vertices);
                        }
                        let graphics = window.as_ref().map(|w| w.bundle.clone())
                            .unwrap_or_else(|| (&*list.list).clone());
                        graphics.SetGraphicsRootSignature(&gr); graphics.SetPipelineState(&gp);
                        graphics.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                        graphics.SetGraphicsRootShaderResourceView(0,address);
                        graphics.IASetIndexBuffer(Some(&D3D12_INDEX_BUFFER_VIEW {
                            BufferLocation: address+64, SizeInBytes:12, Format:DXGI_FORMAT_R32_UINT,
                        }));
                        let color = [(column+1) as f32/8.0,(row+1) as f32/16.0,(generation+1) as f32/2.0,1.0];
                        graphics.SetGraphicsRoot32BitConstants(1,4,color.as_ptr().cast(),0);
                        graphics.ExecuteIndirect(&signature,1,&commands.buffer,commands.offset,None,0);
                        if !windowed {
                            barriers.between(&buffer,Access::Vertices,Access::Compute);
                            barriers.between(&commands.buffer,Access::Arguments,Access::Compute);
                            list.mesh_scratch_recycle();
                        }
                    }
                    if let Some(mut window) = window {
                        assert!(window.full());
                        assert!(window.reserve(1).is_none());
                        assert_eq!(addresses.len(), super::SLOTS as usize);
                        window.finish(list).unwrap();
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
                host.execute(&[list]);
                assert!(host.device.GetDeviceRemovedReason().is_ok());
                std::slice::from_raw_parts(mapping, (ROW * HEIGHT) as usize).to_vec()
            };
            let legacy_list=host.new_list().unwrap();
            let window_list=host.new_list().unwrap();
            for generation in 0..2 {
                if generation != 0 { legacy_list.reset(); window_list.reset(); }
                let legacy=render(false,&legacy_list,generation);
                let windows=render(true,&window_list,generation);
                assert_eq!(windows,legacy,"all 128 draw records and their colors agree, generation {generation}");
                for row in 0..ROWS {
                    for column in 0..super::SLOTS {
                        let offset=(((row*4+2)*WIDTH+column*8+4)*4) as usize;
                        assert_eq!(windows[offset+3],if column%3==1 {0} else {255},
                            "active and empty indirect record: row{row} column{column}");
                    }
                }
                let pool=window_list.mesh_bundles.lock().unwrap_or_else(|e|e.into_inner());
                assert_eq!(pool.lists.len(),ROWS as usize,"fenced reset reuses bundle objects");
                assert_eq!(pool.used,ROWS as usize);
            }
            eprintln!("MESH_WINDOWS_TEST 256 records preserved exact visible pixels and empty GPU arguments, 8 disjoint full-capacity slots, fenced bundle pool reused");
        }
    }
