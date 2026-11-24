//! End-to-end tests of the render-target and viewport index a vertex program
//! exports: synthetic GFX10 programs lowered to DXIL, validated by
//! `dxil.dll` when present, and drawn on the host GPU and WARP into a target
//! of several slices through two viewports.

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::core::Interface;

use super::host::{Host, stub_ps};
use super::{HostShader, container, container::SigElement, emit, validator};
use crate::native::{HwStage, NativeProgram, StageIo};
use crate::{Container, FourCC, io, ir, resource, structure};

const SLICES: u32 = 3;
const SIZE: u32 = 8;

/// A full-screen triangle (`v0`: the vertex), and in `v8` the two indices as
/// the hardware takes them: the slice (the instance, `v3`) in the low bits,
/// the viewport (its lowest bit) from bit 16.
const BODY: &[u32] = &[
    0x3608_0081, // v_and_b32 v4, 1, v0
    0x360A_0082, // v_and_b32 v5, 2, v0
    0x7E08_0D04, // v_cvt_f32_u32 v4, v4
    0x7E0A_0D05, // v_cvt_f32_u32 v5, v5
    0x1008_08F6, // v_mul_f32 v4, 4.0, v4
    0x100A_0AF4, // v_mul_f32 v5, 2.0, v5
    0x0608_08F3, // v_add_f32 v4, -1.0, v4
    0x060A_0AF3, // v_add_f32 v5, -1.0, v5
    0x7E0C_0280, // v_mov_b32 v6, 0
    0x7E0E_02F2, // v_mov_b32 v7, 1.0
    0x3610_0681, // v_and_b32 v8, 1, v3
    0x3410_1090, // v_lshlrev_b32 v8, 16, v8
    0x3810_1103, // v_or_b32 v8, v3, v8
    0x7E12_02F3, // v_mov_b32 v9, -1.0
    0xF800_00CF,
    0x0706_0504, // exp pos0 v4, v5, v6, v7
];
const MISC: &[u32] = &[0xF800_00D4, 0x0008_0000]; // exp pos1 off, off, v8, off
const CLIP: &[u32] = &[0xF800_00E1, 0x0000_0009]; // exp pos2 v9, off, off, off
const S_ENDPGM: u32 = 0xBF81_0000;

fn element(name: &str, kind: u8, comp: u8, row: u32, cols: u8) -> SigElement {
    SigElement {
        name: name.into(),
        index: 0,
        kind,
        comp,
        interp: 0,
        row: Some(row),
        col: 0,
        cols,
        usage: (1 << cols) - 1,
        half: false,
    }
}

/// Translates [`BODY`] and `exports` (the last of which is the last export)
/// with the outputs `kinds` beside the position.
fn build(exports: &[&[u32]], kinds: &[u8]) -> HostShader {
    let mut words = BODY.to_vec();
    for (i, e) in exports.iter().enumerate() {
        words.extend_from_slice(e);
        if i + 1 == exports.len() {
            let at = words.len() - 2;
            words[at] |= 0x800; // done
        }
    }
    words.push(S_ENDPGM);
    let native = NativeProgram {
        stage: HwStage::Vs,
        wave64: false,
        vgprs: 16,
        layout: vec![],
        used: vec![],
        io: StageIo::Vs {
            inputs: vec![],
            outputs: vec![],
        },
        code: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
    };
    let inputs = [
        element("SV_VertexID", 1, 5, 0, 1),
        element("SV_InstanceID", 2, 5, 1, 1),
    ];
    let mut outputs = vec![element("SV_Position", 3, 9, 0, 4)];
    for &kind in kinds {
        let (name, comp) = match kind {
            4 => ("SV_RenderTargetArrayIndex", 5),
            5 => ("SV_ViewportArrayIndex", 5),
            _ => ("SV_ClipDistance", 9),
        };
        outputs.push(element(name, kind, comp, outputs.len() as u32, 1));
    }
    let isg1 = container::signature(&inputs, true);
    let osg1 = container::signature(&outputs, false);
    let bytes = crate::dxbc::build(&[(FourCC::ISG1, &isg1), (FourCC::OSG1, &osg1)]);
    let c = Container::parse(&bytes).unwrap();
    let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
    let st = structure::structure(&p).expect("structure");
    let res = resource::track(&p, &native.layout, native.stage, Some(&st)).expect("track");
    let io = io::raise(&p, &native).expect("io::raise");
    let shader = emit(&c, &native, &p, &st, &io, &res).expect("translates");
    if let Ok(mut v) = validator::Validator::load(validator::Validator::default_path().as_deref()) {
        v.validate(&mut shader.dxil.clone())
            .expect("DXIL validation failed");
    } else {
        eprintln!("no dxil.dll; validation skipped");
    }
    shader
}

/// Draws the three instances over ones (the pixel shader writes zeros):
/// which texels of each slice were drawn, left half and right half. The
/// first viewport is the left half of the target, the second the right one.
fn draw(vs: &HostShader) -> Vec<(&'static str, Vec<[bool; 2]>)> {
    let _one = super::valu_tests::ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut out = Vec::new();
    for (label, warp) in [("hardware", false), ("warp", true)] {
        let Ok(host) = Host::new(warp) else {
            eprintln!("no SM 6.6 {label} device; skipped");
            continue;
        };
        let mut options = D3D12_FEATURE_DATA_D3D12_OPTIONS::default();
        // SAFETY: a plain query that fills our structure.
        unsafe {
            host.device.CheckFeatureSupport(
                D3D12_FEATURE_D3D12_OPTIONS,
                (&raw mut options).cast(),
                size_of_val(&options) as u32,
            )
        }
        .unwrap();
        if !options
            .VPAndRTArrayIndexFromAnyShaderFeedingRasterizerSupportedWithoutGSEmulation
            .as_bool()
        {
            eprintln!("{label}: no indices from vertex shaders; skipped");
            continue;
        }
        let (root, pso) = host
            .pipeline(vs, &stub_ps())
            .unwrap_or_else(|e| panic!("{label}: {e}"));
        out.push((label, run(&host.device, &root, &pso)));
    }
    out
}

fn run(
    dev: &ID3D12Device,
    root: &ID3D12RootSignature,
    pso: &ID3D12PipelineState,
) -> Vec<[bool; 2]> {
    const PITCH: u32 = 256;
    let (w, h) = (2 * SIZE, SIZE);
    let sample = DXGI_SAMPLE_DESC {
        Count: 1,
        Quality: 0,
    };
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
    let target = make(
        D3D12_HEAP_TYPE_DEFAULT,
        &D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Width: w.into(),
            Height: h,
            DepthOrArraySize: SLICES as u16,
            MipLevels: 1,
            Format: DXGI_FORMAT_R32G32B32A32_FLOAT,
            SampleDesc: sample,
            Flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
            ..Default::default()
        },
        D3D12_RESOURCE_STATE_RENDER_TARGET,
    );
    let readback = make(
        D3D12_HEAP_TYPE_READBACK,
        &D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: (PITCH * h * SLICES).into(),
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            SampleDesc: sample,
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            ..Default::default()
        },
        D3D12_RESOURCE_STATE_COPY_DEST,
    );
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
            .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &alloc, pso)
            .unwrap();
        let heap: ID3D12DescriptorHeap = dev
            .CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                NumDescriptors: 1,
                ..Default::default()
            })
            .unwrap();
        let rtv = heap.GetCPUDescriptorHandleForHeapStart();
        dev.CreateRenderTargetView(&target, None, rtv);
        list.ClearRenderTargetView(rtv, &[1.0; 4], None);
        list.SetGraphicsRootSignature(root);
        list.OMSetRenderTargets(1, Some(&rtv), false, None);
        let viewport = |x: u32| D3D12_VIEWPORT {
            TopLeftX: x as f32,
            Width: SIZE as f32,
            Height: h as f32,
            MaxDepth: 1.0,
            ..Default::default()
        };
        list.RSSetViewports(&[viewport(0), viewport(SIZE)]);
        let all = RECT {
            right: w as i32,
            bottom: h as i32,
            ..Default::default()
        };
        list.RSSetScissorRects(&[all, all]);
        list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        list.DrawInstanced(3, SLICES, 0, 0);
        list.ResourceBarrier(&[D3D12_RESOURCE_BARRIER {
            Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                    pResource: std::mem::transmute_copy(&target),
                    Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                    StateBefore: D3D12_RESOURCE_STATE_RENDER_TARGET,
                    StateAfter: D3D12_RESOURCE_STATE_COPY_SOURCE,
                }),
            },
            ..Default::default()
        }]);
        for slice in 0..SLICES {
            let to = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(&readback),
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                        Offset: (PITCH * h * slice).into(),
                        Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                            Format: DXGI_FORMAT_R32G32B32A32_FLOAT,
                            Width: w,
                            Height: h,
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
                    SubresourceIndex: slice,
                },
            };
            list.CopyTextureRegion(&to, 0, 0, 0, &from, None);
        }
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
        let texel = |slice: u32, x: u32, y: u32| {
            let at = PITCH * (h * slice + y) + 16 * x;
            p.cast::<u8>().add(at as usize).cast::<f32>().read()
        };
        let out = (0..SLICES)
            .map(|slice| {
                [0, SIZE].map(|x0| {
                    let drawn: Vec<bool> = (0..h)
                        .flat_map(|y| (x0..x0 + SIZE).map(move |x| (x, y)))
                        .map(|(x, y)| texel(slice, x, y) == 0.0)
                        .collect();
                    // A half is drawn whole or not at all.
                    assert!(drawn.iter().all(|&d| d == drawn[0]), "half a half drawn");
                    drawn[0]
                })
            })
            .collect();
        readback.Unmap(0, None);
        out
    }
}

const NONE: [bool; 2] = [false, false];
const LEFT: [bool; 2] = [true, false];
const RIGHT: [bool; 2] = [false, true];

#[test]
fn instances_go_to_the_slice_and_viewport_the_vertex_program_names() {
    for (label, drawn) in draw(&build(&[MISC], &[4, 5])) {
        assert_eq!(drawn, [LEFT, RIGHT, LEFT], "{label}");
    }
}

#[test]
fn either_index_is_taken_alone() {
    for (label, drawn) in draw(&build(&[MISC], &[4])) {
        assert_eq!(drawn, [LEFT, LEFT, LEFT], "{label}: the slice");
    }
    for (label, drawn) in draw(&build(&[MISC], &[5])) {
        assert_eq!(drawn, [[true, true], NONE, NONE], "{label}: the viewport");
    }
}

#[test]
fn clip_distances_follow_the_indices() {
    // Every vertex is clipped away: with the indices in pos1 the clip
    // distances start at pos2, and none of them is taken for an index.
    for (label, drawn) in draw(&build(&[MISC, CLIP], &[4, 5, 6])) {
        assert_eq!(drawn, [NONE, NONE, NONE], "{label}");
    }
}

#[test]
fn clip_distances_without_indices_stay_at_pos1() {
    let clip: &[u32] = &[0xF800_00D1, 0x0000_0009]; // exp pos1 v9, off, off, off
    for (label, drawn) in draw(&build(&[clip], &[6])) {
        assert_eq!(drawn, [NONE, NONE, NONE], "{label}");
    }
}
