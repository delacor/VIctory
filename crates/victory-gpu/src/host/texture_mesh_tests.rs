use super::*;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;

fn color_upload(
    list: &HostList,
    source: &std::sync::Arc<crate::resource::HostTexture>,
    color: [u8; 4],
) {
    let host = host().unwrap();
    let (upload, data) = list.upload(host, 256).unwrap();
    // SAFETY: owned mapped bytes and an open list. The actual production
    // settle path must flush prior sampled-texture bundles before mutation.
    unsafe {
        std::ptr::copy_nonoverlapping(color.as_ptr(), data, 4);
        list.settle(source, D3D12_RESOURCE_STATE_COPY_DEST.0 as u32);
        let to = D3D12_TEXTURE_COPY_LOCATION {
            pResource: std::mem::transmute_copy(&source.texture),
            Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                SubresourceIndex: 0,
            },
        };
        let from = D3D12_TEXTURE_COPY_LOCATION {
            pResource: std::mem::transmute_copy(&upload),
            Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                    Offset: 0,
                    Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                        Width: 1,
                        Height: 1,
                        Depth: 1,
                        RowPitch: 256,
                    },
                },
            },
        };
        list.list.CopyTextureRegion(&to, 0, 0, 0, &from, None);
        list.settle(source, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE.0 as u32);
    }
}

#[test]
#[ignore = "isolated RX5500/native process with MESH_TEXTURE_WINDOWS=1 and asynchronous texture uploads disabled"]
fn textured_windows_preserve_upload_order_bindings_arguments_and_fenced_reuse() {
    assert!(crate::mesh_texture_window::enabled());
    let host = host().expect("actual hardware");
    // SAFETY: ordinary adapter description.
    let desc = unsafe {
        host.adapter
            .as_ref()
            .unwrap()
            .cast::<IDXGIAdapter1>()
            .unwrap()
            .GetDesc1()
            .unwrap()
    };
    assert_eq!(
        desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32,
        0,
        "software adapter refused"
    );
    const CS: &str = r#"
#define RS "UAV(u0), UAV(u1), RootConstants(num32BitConstants=1,b0)"
RWByteAddressBuffer data:register(u0);
RWByteAddressBuffer arguments:register(u1);
cbuffer C:register(b0){uint side;}
[RootSignature(RS)] [numthreads(1,1,1)] void main(){
 float left=-1.0+float(side)*0.125+0.01;
 data.Store2(0,asuint(float2(left,-0.8)));
 data.Store2(8,asuint(float2(left+0.105,-0.8)));
 data.Store2(16,asuint(float2(left+0.0525,0.8)));
 data.Store3(64,uint3(0,1,2));
 arguments.Store4(0,uint4(side%3==1?0:3,1,0,0));
 arguments.Store(16,0);
}
"#;
    const GRAPHICS: &str = r#"
#define RS "DescriptorTable(SRV(t1,numDescriptors=1)), SRV(t0), RootConstants(num32BitConstants=4,b0)"
ByteAddressBuffer data:register(t0);
Texture2D<float4> sampled:register(t1);
cbuffer C:register(b0){float4 color;}
struct Out{float4 p:SV_Position;float4 c:COLOR;};
[RootSignature(RS)] Out vertex(uint id:SV_VertexID){
 Out o;o.p=float4(asfloat(data.Load2(id*8)),0,1);o.c=color;return o;
}
[RootSignature(RS)] float4 pixel(Out i):SV_Target{return i.c*sampled.Load(int3(0,0,0));}
"#;
    let cs = compile(CS, windows::core::s!("main"), windows::core::s!("cs_5_1")).unwrap();
    let vs = compile(
        GRAPHICS,
        windows::core::s!("vertex"),
        windows::core::s!("vs_5_1"),
    )
    .unwrap();
    let ps = compile(
        GRAPHICS,
        windows::core::s!("pixel"),
        windows::core::s!("ps_5_1"),
    )
    .unwrap();
    const COLUMNS: u32 = 16;
    const ROWS: u32 = 16;
    const WIDTH: u32 = COLUMNS * 8;
    const HEIGHT: u32 = ROWS * 4;
    const PITCH: u32 = WIDTH * 4;
    // SAFETY: all resource ranges are owned, commands are fenced before
    // mappings/reset, and every bundle is retained by the recording pool.
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
        let mut sig: Option<ID3D12CommandSignature> = None;
        host.device
            .CreateCommandSignature(
                &D3D12_COMMAND_SIGNATURE_DESC {
                    ByteStride: 20,
                    NumArgumentDescs: 1,
                    pArgumentDescs: &op,
                    NodeMask: 0,
                },
                None::<&ID3D12RootSignature>,
                &mut sig,
            )
            .unwrap();
        let sig = sig.unwrap();
        let make_texture =
            |width: u32, height: u32, flags: D3D12_RESOURCE_FLAGS, state: D3D12_RESOURCE_STATES| {
                let mut t: Option<ID3D12Resource> = None;
                host.device
                    .CreateCommittedResource(
                        &D3D12_HEAP_PROPERTIES {
                            Type: D3D12_HEAP_TYPE_DEFAULT,
                            ..Default::default()
                        },
                        D3D12_HEAP_FLAG_NONE,
                        &D3D12_RESOURCE_DESC {
                            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
                            Width: u64::from(width),
                            Height: height,
                            DepthOrArraySize: 1,
                            MipLevels: 1,
                            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                            SampleDesc: DXGI_SAMPLE_DESC {
                                Count: 1,
                                Quality: 0,
                            },
                            Flags: flags,
                            ..Default::default()
                        },
                        state,
                        None,
                        &mut t,
                    )
                    .unwrap();
                t.unwrap()
            };
        let render = |windowed: bool, list: &HostList, generation: u32| {
            let target = make_texture(
                WIDTH,
                HEIGHT,
                D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
            );
            let sampled = crate::resource::HostTexture::test_owned(
                make_texture(
                    1,
                    1,
                    D3D12_RESOURCE_FLAG_NONE,
                    D3D12_RESOURCE_STATE_COPY_DEST,
                ),
                D3D12_RESOURCE_STATE_COPY_DEST.0 as u32,
            );
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
            let views: ID3D12DescriptorHeap = host
                .device
                .CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                    Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                    NumDescriptors: 1,
                    Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                    ..Default::default()
                })
                .unwrap();
            let view = D3D12_SHADER_RESOURCE_VIEW_DESC {
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
                Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
                Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                    Texture2D: D3D12_TEX2D_SRV {
                        MostDetailedMip: 0,
                        MipLevels: 1,
                        PlaneSlice: 0,
                        ResourceMinLODClamp: 0.0,
                    },
                },
            };
            host.device.CreateShaderResourceView(
                &sampled.texture,
                Some(&view),
                views.GetCPUDescriptorHandleForHeapStart(),
            );
            let (back, mapping) = host.shared_buffer(u64::from(PITCH * HEIGHT)).unwrap();
            let direct = host.uav_buffer(256).unwrap();
            for resource in [&target, &sampled.texture, &back, &direct] {
                list.keep(resource);
            }
            list.keep(&rtv);
            list.keep(&views);
            list.list.OMSetRenderTargets(1, Some(&handle), false, None);
            list.list.ClearRenderTargetView(handle, &[0.0; 4], None);
            let barriers = crate::mesh_barriers::MeshBarriers::new(&list.list);
            use crate::mesh_barriers::Access;
            barriers.begin(&direct, true);
            let mut state = crate::draw::State::default();
            for row in 0..ROWS {
                state.flush_mesh(list);
                list.list.RSSetViewports(&[D3D12_VIEWPORT {
                    TopLeftX: 0.0,
                    TopLeftY: (row * 4) as f32,
                    Width: WIDTH as f32,
                    Height: 4.0,
                    MinDepth: 0.0,
                    MaxDepth: 1.0,
                }]);
                list.list.RSSetScissorRects(&[RECT {
                    left: 0,
                    top: 0,
                    right: WIDTH as i32,
                    bottom: HEIGHT as i32,
                }]);
                color_upload(list, &sampled, [255, 0, 0, 255]);
                let mut used = std::collections::BTreeSet::new();
                for column in 0..COLUMNS {
                    let region = if windowed {
                        state.texture_mesh_test_begin(list, gp.as_raw() as usize, 256)
                    } else {
                        ScratchRegion {
                            buffer: direct.clone(),
                            offset: 0,
                            first: false,
                        }
                    };
                    let address = region.buffer.GetGPUVirtualAddress() + region.offset;
                    used.insert(address);
                    let commands = list.mesh_scratch(ScratchPool::Commands, 32).unwrap();
                    list.keep(&commands.buffer);
                    barriers.begin(&commands.buffer, commands.first);
                    if column == 4 && row % 3 == 0 {
                        let source = sampled.clone();
                        *list.texture_mesh_bind_probe.lock().unwrap() = Some((
                            true,
                            Box::new(move |list| color_upload(list, &source, [0, 0, 255, 255])),
                        ));
                    }
                    state.texture_mesh_test_bind(list, &cp, &cr, true);
                    list.list.SetComputeRootUnorderedAccessView(0, address);
                    list.list.SetComputeRootUnorderedAccessView(
                        1,
                        commands.buffer.GetGPUVirtualAddress() + commands.offset,
                    );
                    list.list.SetComputeRoot32BitConstant(2, column, 0);
                    list.list.Dispatch(1, 1, 1);
                    barriers.between(&commands.buffer, Access::Compute, Access::Arguments);
                    if windowed {
                        barriers.between(&commands.buffer, Access::Arguments, Access::Compute);
                    } else {
                        barriers.between(&direct, Access::Compute, Access::Vertices);
                    }
                    if column == 4 && row % 3 == 1 {
                        let source = sampled.clone();
                        *list.texture_mesh_bind_probe.lock().unwrap() = Some((
                            false,
                            Box::new(move |list| color_upload(list, &source, [0, 0, 255, 255])),
                        ));
                    }
                    if column == 4 && row % 3 == 2 {
                        color_upload(list, &sampled, [0, 0, 255, 255]);
                    }
                    state.texture_mesh_test_bind(list, &gp, &gr, false);
                    let graphics = if windowed {
                        state.texture_mesh_test_graphics(list)
                    } else {
                        (*list.list).clone()
                    };
                    graphics.SetDescriptorHeaps(&[Some(views.clone())]);
                    graphics.SetGraphicsRootDescriptorTable(
                        0,
                        views.GetGPUDescriptorHandleForHeapStart(),
                    );
                    graphics.SetGraphicsRootShaderResourceView(1, address);
                    let color = [
                        (column + 1) as f32 / 16.0,
                        (row + 1) as f32 / 16.0,
                        (generation + 1) as f32 / 2.0,
                        1.0,
                    ];
                    graphics.SetGraphicsRoot32BitConstants(2, 4, color.as_ptr().cast(), 0);
                    graphics.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                    graphics.IASetIndexBuffer(Some(&D3D12_INDEX_BUFFER_VIEW {
                        BufferLocation: address + 64,
                        SizeInBytes: 12,
                        Format: DXGI_FORMAT_R32_UINT,
                    }));
                    graphics.ExecuteIndirect(&sig, 1, &commands.buffer, commands.offset, None, 0);
                    if windowed {
                        state.texture_mesh_test_heap(&views);
                        state.texture_mesh_test_finish(list, &commands.buffer);
                    } else {
                        barriers.between(&direct, Access::Vertices, Access::Compute);
                        barriers.between(&commands.buffer, Access::Arguments, Access::Compute);
                        list.mesh_scratch_recycle();
                    }
                }
                if windowed {
                    assert!(used.len() >= 8, "full disjoint slots preserved");
                }
            }
            state.flush_mesh(list);
            list.list.ResourceBarrier(&[transition(
                &target,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                D3D12_RESOURCE_STATE_COPY_SOURCE,
            )]);
            let to = D3D12_TEXTURE_COPY_LOCATION {
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
                            RowPitch: PITCH,
                        },
                    },
                },
            };
            let from = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(&target),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    SubresourceIndex: 0,
                },
            };
            list.list.CopyTextureRegion(&to, 0, 0, 0, &from, None);
            list.close();
            host.execute(&[list]);
            assert!(host.device.GetDeviceRemovedReason().is_ok());
            std::slice::from_raw_parts(mapping, (PITCH * HEIGHT) as usize).to_vec()
        };
        let reference = host.new_list().unwrap();
        let candidate = host.new_list().unwrap();
        for generation in 0..2 {
            if generation != 0 {
                reference.reset();
                candidate.reset();
            }
            let before = render(false, &reference, generation);
            let after = render(true, &candidate, generation);
            assert_eq!(
                after, before,
                "exact sampled texture pixels and indirect arguments generation{generation}"
            );
            for row in 0..ROWS {
                for column in 0..COLUMNS {
                    let at = (((row * 4 + 2) * WIDTH + column * 8 + 4) * 4) as usize;
                    let expected = if column % 3 == 1 {
                        [0, 0, 0, 0]
                    } else if column < 4 {
                        [
                            ((column + 1) as f32 * 255.0 / 16.0).round() as u8,
                            0,
                            0,
                            255,
                        ]
                    } else {
                        [
                            0,
                            0,
                            ((generation + 1) as f32 * 255.0 / 2.0).round() as u8,
                            255,
                        ]
                    };
                    for channel in 0..4 {
                        assert!(
                            after[at + channel].abs_diff(expected[channel]) <= 1,
                            "row{row} col{column} expected{expected:?} got{:?}",
                            &after[at..at + 4]
                        );
                    }
                }
            }
            let pool = candidate.texture_mesh_pool.get().unwrap().lock().unwrap();
            assert!(pool.generation() >= generation as u64);
        }
        assert_eq!(host.log_messages(), None);
        eprintln!(
            "MESH_TEXTURE_NATIVE 512 records exact; red-before-blue uploads; compute/graphics bind mutation retries; zero arguments; 8 full slots; fenced pool reuse"
        );
    }
}
