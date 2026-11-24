//! End-to-end tests of loads from multisampled images: every sample of a 4x
//! target is given a value of its own (a draw per pixel and sample, through
//! a scissor and a sample mask, scaled by the blend factor), and a synthetic
//! GFX10 program reads them back on the host GPU and WARP.

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::core::Interface;

use super::HostShader;
use super::export_index_tests::fullscreen;
use super::host::{Host, stub_ps};
use super::image_store_tests::{build_with, used};
use crate::native::ResourceType::{Srv, Uav};

const SAMPLES: u32 = 4;
const WIDTH: u32 = 4;
const HEIGHT: u32 = 2;
const FORMAT: DXGI_FORMAT = DXGI_FORMAT_R16G16B16A16_FLOAT;

/// `s[4:11]`: the image (heap index 0), `s[12:15]`: the raw buffer (1). A
/// lane's pixel is `v1`, `v2`, its sample `v3`.
const BEFORE: &[u32] = &[
    0xF40C_0100,
    0xFA00_0000, // s_load_dwordx8 s[4:11], s[0:1], null
    0xF408_0300,
    0xFA00_0020, // s_load_dwordx4 s[12:15], s[0:1], 0x20
    0x3602_0083, // v_and_b32 v1, 3, v0
    0x2C04_0084, // v_lshrrev_b32 v2, 4, v0
    0x2C06_0082, // v_lshrrev_b32 v3, 2, v0
    0x3606_0683, // v_and_b32 v3, 3, v3
];
/// `v4` to the lane's dword of the buffer.
const AFTER: &[u32] = &[
    0x3412_0082, // v_lshlrev_b32 v9, 2, v0
    0xE070_1000,
    0x8003_0409, // buffer_store_dword v4, v9, s[12:15], 0 offen
];

fn build(load: &[u32]) -> HostShader {
    build_with(
        &[BEFORE, load, AFTER].concat(),
        vec![used(Srv, 0, 0, 8), used(Uav, 1, 0, 4)],
    )
    .expect("translates")
}

/// What sample `lane / 4 % 4` of pixel (`lane % 4`, `lane / 16`) holds.
fn value(lane: u32) -> f32 {
    (lane + 1) as f32 / 64.0
}

fn check(shader: &HostShader, array: bool) {
    let _one = super::valu_tests::ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    for (label, warp) in [("hardware", false), ("warp", true)] {
        let Ok(host) = Host::new(warp) else {
            eprintln!("no SM 6.6 {label} device; skipped");
            continue;
        };
        let want: Vec<f32> = (0..32).map(value).collect();
        assert_eq!(run(&host, shader, array), want, "{label}");
    }
}

fn run(host: &Host, shader: &HostShader, array: bool) -> Vec<f32> {
    let dev = &host.device;
    let (vs, ps) = (fullscreen(), stub_ps());
    let (draw_root, _) = host.pipeline(&vs, &ps).unwrap();
    let (root, pso) = host.compute_pipeline(shader).unwrap();
    // A pipeline per sample: the target times the blend factor, there only.
    let masked = |sample: u32| -> ID3D12PipelineState {
        let mut desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            // SAFETY: a borrowed pointer the descriptor never releases.
            pRootSignature: unsafe { std::mem::transmute_copy(&draw_root) },
            VS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: vs.dxil.as_ptr().cast(),
                BytecodeLength: vs.dxil.len(),
            },
            PS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: ps.dxil.as_ptr().cast(),
                BytecodeLength: ps.dxil.len(),
            },
            SampleMask: 1 << sample,
            RasterizerState: D3D12_RASTERIZER_DESC {
                FillMode: D3D12_FILL_MODE_SOLID,
                CullMode: D3D12_CULL_MODE_NONE,
                DepthClipEnable: true.into(),
                ..Default::default()
            },
            PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            NumRenderTargets: 1,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: SAMPLES,
                Quality: 0,
            },
            ..Default::default()
        };
        desc.RTVFormats[0] = FORMAT;
        desc.BlendState.RenderTarget[0] = D3D12_RENDER_TARGET_BLEND_DESC {
            BlendEnable: true.into(),
            SrcBlend: D3D12_BLEND_ZERO,
            DestBlend: D3D12_BLEND_BLEND_FACTOR,
            BlendOp: D3D12_BLEND_OP_ADD,
            SrcBlendAlpha: D3D12_BLEND_ZERO,
            DestBlendAlpha: D3D12_BLEND_BLEND_FACTOR,
            BlendOpAlpha: D3D12_BLEND_OP_ADD,
            RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
            ..Default::default()
        };
        // SAFETY: the description points at live data.
        unsafe { dev.CreateGraphicsPipelineState(&desc) }.unwrap()
    };
    let masks: Vec<ID3D12PipelineState> = (0..SAMPLES).map(masked).collect();
    let make = |heap, desc: &D3D12_RESOURCE_DESC, state| {
        let mut r: Option<ID3D12Resource> = None;
        // SAFETY: a plain creation with descriptions that outlive it.
        unsafe {
            dev.CreateCommittedResource(
                &D3D12_HEAP_PROPERTIES {
                    Type: heap,
                    ..Default::default()
                },
                D3D12_HEAP_FLAG_NONE,
                desc,
                state,
                None,
                &mut r,
            )
        }
        .unwrap();
        r.unwrap()
    };
    let buffer = |flags| D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Width: 128,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        Flags: flags,
        ..Default::default()
    };
    let target = make(
        D3D12_HEAP_TYPE_DEFAULT,
        &D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Width: WIDTH.into(),
            Height: HEIGHT,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: FORMAT,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: SAMPLES,
                Quality: 0,
            },
            Flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
            ..Default::default()
        },
        D3D12_RESOURCE_STATE_RENDER_TARGET,
    );
    let out = make(
        D3D12_HEAP_TYPE_DEFAULT,
        &buffer(D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS),
        D3D12_RESOURCE_STATE_COMMON,
    );
    let readback = make(
        D3D12_HEAP_TYPE_READBACK,
        &buffer(D3D12_RESOURCE_FLAG_NONE),
        D3D12_RESOURCE_STATE_COPY_DEST,
    );
    let transition = |r: &ID3D12Resource, before, after| D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                // SAFETY: a borrowed pointer the barrier never releases.
                pResource: unsafe { std::mem::transmute_copy(r) },
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: before,
                StateAfter: after,
            }),
        },
        ..Default::default()
    };
    // SAFETY: D3D12 calls with descriptions that outlive them; the resources
    // live until the fence says the GPU is done.
    unsafe {
        let queue: ID3D12CommandQueue = dev
            .CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                ..Default::default()
            })
            .unwrap();
        let alloc: ID3D12CommandAllocator = dev
            .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
            .unwrap();
        let list: ID3D12GraphicsCommandList = dev
            .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &alloc, &masks[0])
            .unwrap();
        let heap = |kind, n, flags| -> ID3D12DescriptorHeap {
            dev.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                Type: kind,
                NumDescriptors: n,
                Flags: flags,
                ..Default::default()
            })
            .unwrap()
        };
        let rtvs = heap(
            D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            1,
            D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
        );
        let rtv = rtvs.GetCPUDescriptorHandleForHeapStart();
        dev.CreateRenderTargetView(&target, None, rtv);
        let views = heap(
            D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            2,
            D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
        );
        let samplers = heap(
            D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER,
            1,
            D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
        );
        let mut at = views.GetCPUDescriptorHandleForHeapStart();
        let mut view = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: FORMAT,
            ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2DMS,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            ..Default::default()
        };
        if array {
            view.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY;
            view.Anonymous.Texture2DMSArray = D3D12_TEX2DMS_ARRAY_SRV {
                FirstArraySlice: 0,
                ArraySize: 1,
            };
        }
        dev.CreateShaderResourceView(&target, Some(&view), at);
        at.ptr += dev.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV)
            as usize;
        dev.CreateUnorderedAccessView(
            &out,
            None,
            Some(&D3D12_UNORDERED_ACCESS_VIEW_DESC {
                Format: DXGI_FORMAT_R32_TYPELESS,
                ViewDimension: D3D12_UAV_DIMENSION_BUFFER,
                Anonymous: D3D12_UNORDERED_ACCESS_VIEW_DESC_0 {
                    Buffer: D3D12_BUFFER_UAV {
                        NumElements: 32,
                        Flags: D3D12_BUFFER_UAV_FLAG_RAW,
                        ..Default::default()
                    },
                },
            }),
            at,
        );
        list.ClearRenderTargetView(rtv, &[1.0; 4], None);
        list.SetGraphicsRootSignature(&draw_root);
        list.OMSetRenderTargets(1, Some(&rtv), false, None);
        list.RSSetViewports(&[D3D12_VIEWPORT {
            Width: WIDTH as f32,
            Height: HEIGHT as f32,
            MaxDepth: 1.0,
            ..Default::default()
        }]);
        list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        for lane in 0..32u32 {
            let (x, y, sample) = ((lane & 3) as i32, (lane >> 4) as i32, lane >> 2 & 3);
            list.SetPipelineState(&masks[sample as usize]);
            list.RSSetScissorRects(&[RECT {
                left: x,
                top: y,
                right: x + 1,
                bottom: y + 1,
            }]);
            list.OMSetBlendFactor(Some(&[value(lane); 4]));
            list.DrawInstanced(3, 1, 0, 0);
        }
        list.ResourceBarrier(&[transition(
            &target,
            D3D12_RESOURCE_STATE_RENDER_TARGET,
            D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
        )]);
        list.SetPipelineState(&pso);
        list.SetDescriptorHeaps(&[Some(views.clone()), Some(samplers.clone())]);
        list.SetComputeRootSignature(&root);
        let ud = vec![0u32; shader.user_data_dwords().max(1) as usize];
        list.SetComputeRoot32BitConstants(0, ud.len() as u32, ud.as_ptr().cast(), 0);
        list.Dispatch(1, 1, 1);
        list.ResourceBarrier(&[transition(
            &out,
            D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
            D3D12_RESOURCE_STATE_COPY_SOURCE,
        )]);
        list.CopyBufferRegion(&readback, 0, &out, 0, 128);
        list.Close().unwrap();
        queue.ExecuteCommandLists(&[Some(list.cast().unwrap())]);
        let fence: ID3D12Fence = dev.CreateFence(0, D3D12_FENCE_FLAG_NONE).unwrap();
        queue.Signal(&fence, 1).unwrap();
        while fence.GetCompletedValue() < 1 {
            dev.GetDeviceRemovedReason().expect("device removed");
            std::thread::yield_now();
        }
        let mut p = std::ptr::null_mut();
        readback.Map(0, None, Some(&mut p)).unwrap();
        let words = std::slice::from_raw_parts(p.cast::<f32>(), 32).to_vec();
        readback.Unmap(0, None);
        words
    }
}

#[test]
fn every_sample_of_every_pixel_is_loaded() {
    // image_load v4, v[1:3], s[4:11] dmask:0x1 dim:SQ_RSRC_IMG_2D_MSAA unorm
    check(&build(&[0xF000_1130, 0x0001_0401]), false);
}

#[test]
fn the_sample_of_an_array_follows_the_slice() {
    let load = [
        0x7E08_0303, // v_mov_b32 v4, v3
        0x7E06_0280, // v_mov_b32 v3, 0
        // image_load v4, v[1:4], s[4:11] dmask:0x1 dim:SQ_RSRC_IMG_2D_MSAA_ARRAY unorm
        0xF000_1138,
        0x0001_0401,
    ];
    check(&build(&load), true);
}
