//! Generated streaming frames with concurrent PSO demand, compute and graphics.

use anyhow::{Context, Result, ensure};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM;
use windows::Win32::Graphics::Dxgi::*;

use super::gpu::{self, Submit, Timed, qpc, us};
use super::{Bench, Row};

const SIDE: u32 = 128;
const VS: &str = "
float4 vs(uint i : SV_VertexID) : SV_Position {
    return float4(float2((i << 1) & 2, i & 2) * 2 - 1, 0, 1);
}";
const CS: &str = "
StructuredBuffer<uint3> input : register(t0);
RWByteAddressBuffer output : register(u0);
[numthreads(1, 1, 1)] void cs() {
    uint color = input[1].y + input[2].z;
    output.Store(0, color);
}";
const PS: &str = "
ByteAddressBuffer color : register(t0);
float4 ps() : SV_Target {
    uint v = color.Load(0);
    uint mixed = (v + SALT) * 0x45d9f3bu;
    return float4((mixed >> 16) & 255, 11, 29, 255) / 255.0;
}";

pub fn run(b: &mut Bench) -> Result<()> {
    let device = b.adapter.device.clone();
    let range = D3D12_DESCRIPTOR_RANGE {
        RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
        NumDescriptors: 1,
        ..Default::default()
    };
    let table = D3D12_ROOT_PARAMETER {
        ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
        Anonymous: D3D12_ROOT_PARAMETER_0 {
            DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                NumDescriptorRanges: 1,
                pDescriptorRanges: &range,
            },
        },
        ..Default::default()
    };
    let compute_root = gpu::root_signature(
        &device,
        &[table, gpu::root_view(D3D12_ROOT_PARAMETER_TYPE_UAV, 0)],
    )?;
    let graphics_root =
        gpu::root_signature(&device, &[gpu::root_view(D3D12_ROOT_PARAMETER_TYPE_SRV, 0)])?;
    let compute =
        gpu::compute_pipeline(&device, &compute_root, &gpu::compile(CS, "cs", "cs_5_1")?)?;
    let vertex = gpu::compile(VS, "vs", "vs_5_1")?;
    let size = if b.quick { 1 << 20 } else { 32 << 20 };
    let upload = gpu::buffer(
        &device,
        D3D12_HEAP_TYPE_UPLOAD,
        size,
        D3D12_RESOURCE_FLAG_NONE,
    )?;
    let input = gpu::buffer(
        &device,
        D3D12_HEAP_TYPE_DEFAULT,
        size,
        D3D12_RESOURCE_FLAG_NONE,
    )?;
    let output = gpu::buffer(
        &device,
        D3D12_HEAP_TYPE_DEFAULT,
        256,
        D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
    )?;
    let target = gpu::committed(
        &device,
        D3D12_HEAP_TYPE_DEFAULT,
        &gpu::texture_desc(SIDE, D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET),
    )?;
    let bytes = 4 * SIDE * SIDE;
    let readback = gpu::buffer(
        &device,
        D3D12_HEAP_TYPE_READBACK,
        u64::from(bytes),
        D3D12_RESOURCE_FLAG_NONE,
    )?;
    let factory: IDXGIFactory4 = unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))? };
    // SAFETY: the device is alive and its adapter can be enumerated.
    let adapter: IDXGIAdapter3 = unsafe { factory.EnumAdapterByLuid(device.GetAdapterLuid())? };
    let local = || -> Result<DXGI_QUERY_VIDEO_MEMORY_INFO> {
        let mut memory = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
        // SAFETY: node zero and a local in/out structure.
        unsafe { adapter.QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut memory)? };
        Ok(memory)
    };
    let pressure = if b.quick || b.adapter.software {
        0
    } else {
        let m = local()?;
        (m.Budget.saturating_sub(m.CurrentUsage) * 3 / 4).min(8 << 30) >> 27
    };
    // Retain bounded generated allocations. No input file or VRAM budget override is used.
    let mut ballast = Vec::new();
    for _ in 0..pressure {
        ballast.push(gpu::buffer(
            &device,
            D3D12_HEAP_TYPE_DEFAULT,
            128 << 20,
            D3D12_RESOURCE_FLAG_NONE,
        )?);
    }
    // SAFETY: object creation and views with matching resource descriptions.
    let (views, targets) = unsafe {
        let views: ID3D12DescriptorHeap =
            device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                NumDescriptors: 1,
                Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                ..Default::default()
            })?;
        device.CreateShaderResourceView(
            &input,
            Some(&D3D12_SHADER_RESOURCE_VIEW_DESC {
                Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
                ViewDimension: D3D12_SRV_DIMENSION_BUFFER,
                Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                    Buffer: D3D12_BUFFER_SRV {
                        NumElements: 3,
                        StructureByteStride: 12,
                        ..Default::default()
                    },
                },
                ..Default::default()
            }),
            views.GetCPUDescriptorHandleForHeapStart(),
        );
        let targets: ID3D12DescriptorHeap =
            device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                NumDescriptors: 1,
                ..Default::default()
            })?;
        device.CreateRenderTargetView(&target, None, targets.GetCPUDescriptorHandleForHeapStart());
        (views, targets)
    };
    let source = gpu::Mapped::new(&upload)?;
    // SAFETY: the upload is mapped for size bytes and no GPU transfer has started.
    unsafe { std::ptr::write_bytes(source.at, 0x10, size as usize) };
    let frames = if b.quick { 12 } else { b.rounds.max(120) };
    let mut wall = Vec::new();
    let mut compile = Vec::new();
    let mut direct = Vec::<Timed>::new();
    let mut copy = Vec::<Timed>::new();
    let mut peak = local()?.CurrentUsage;
    let salt = std::env::var("VICTORY_MIXED_SALT")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1024);
    let pixels = (bytes / 4) as usize;
    let out = unsafe { targets.GetCPUDescriptorHandleForHeapStart() };
    // SAFETY: both resource owners live through every fence wait.
    let spot = D3D12_TEXTURE_COPY_LOCATION {
        pResource: unsafe { std::mem::transmute_copy(&readback) },
        Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
            PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    Width: SIDE,
                    Height: SIDE,
                    Depth: 1,
                    RowPitch: 4 * SIDE,
                },
                ..Default::default()
            },
        },
    };
    let picture = D3D12_TEXTURE_COPY_LOCATION {
        pResource: unsafe { std::mem::transmute_copy(&target) },
        Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
        ..Default::default()
    };
    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
    let builds = AtomicUsize::new(0);
    let share = std::env::var_os("VICTORY_PSO_COALESCE").is_none_or(|v| v != "0");
    let active = victory_pipeline::InFlight::new();
    let peers = std::env::var("VICTORY_MIXED_PEERS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4)
        .clamp(1, 4);
    let mut pso = None;
    let mut key = salt;
    for frame in 0..frames {
        let began = qpc();
        let seed = (frame % 127) as u32;
        // SAFETY: the previous iteration waited for the upload and draw.
        unsafe {
            source.at.cast::<u32>().add(4).write_volatile(seed);
            source.at.cast::<u32>().add(8).write_volatile(7);
        }
        copy.push(b.copy.timed(1, Submit::Together, &mut |_, cmd| {
            // SAFETY: fully initialized, fitting buffers with no outstanding reuse.
            unsafe { cmd.CopyBufferRegion(&input, 0, &upload, 0, size) };
            Ok(())
        })?);
        if frame % 4 == 0 {
            key = salt.wrapping_add(frame as u32 * 17);
            let start = qpc();
            // Vary the driver code even when two variants happen to draw the same color.
            let pixel = gpu::compile(&PS.replace("SALT", &format!("{key}u")), "ps", "ps_5_1")?;
            let tag = key.to_string();
            let demand = std::thread::scope(|scope| -> Result<ID3D12PipelineState> {
                let (tag, builds, active) = (&tag, &builds, &active);
                let (device, graphics_root, vertex, pixel) =
                    (&device, &graphics_root, &vertex, &pixel);
                let threads: Vec<_> = (0..peers)
                    .map(|_| {
                        scope.spawn(move || {
                            let build = || {
                                builds.fetch_add(1, Relaxed);
                                gpu::graphics_pipeline(device, graphics_root, vertex, pixel, &[])
                            };
                            if share {
                                active.get_or_try_build(tag, build)
                            } else {
                                build()
                            }
                        })
                    })
                    .collect();
                let mut pso = None;
                for thread in threads {
                    pso = Some(
                        thread
                            .join()
                            .map_err(|_| anyhow::anyhow!("PSO worker panicked"))??,
                    );
                }
                pso.context("no graphics PSO")
            })?;
            pso = Some(demand);
            compile.push(us(start, qpc()));
        }
        let pso = pso
            .as_ref()
            .context("first frame did not build a pipeline")?;
        direct.push(b.direct.timed(1, Submit::Together, &mut |_, cmd| {
            // SAFETY: all bound objects live until the queue has completed; the render
            // target and output buffer are restored to COMMON before this list ends.
            unsafe {
                cmd.SetDescriptorHeaps(&[Some(views.clone())]);
                cmd.SetPipelineState(&compute);
                cmd.SetComputeRootSignature(&compute_root);
                cmd.SetComputeRootDescriptorTable(0, views.GetGPUDescriptorHandleForHeapStart());
                cmd.SetComputeRootUnorderedAccessView(1, output.GetGPUVirtualAddress());
                cmd.Dispatch(1, 1, 1);
                cmd.ResourceBarrier(&[
                    gpu::transition(
                        &output,
                        D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
                        D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                    ),
                    gpu::transition(
                        &target,
                        D3D12_RESOURCE_STATE_COMMON,
                        D3D12_RESOURCE_STATE_RENDER_TARGET,
                    ),
                ]);
                cmd.SetPipelineState(pso);
                cmd.SetGraphicsRootSignature(&graphics_root);
                cmd.SetGraphicsRootShaderResourceView(0, output.GetGPUVirtualAddress());
                cmd.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                cmd.RSSetViewports(&[D3D12_VIEWPORT {
                    Width: SIDE as f32,
                    Height: SIDE as f32,
                    MaxDepth: 1.0,
                    ..Default::default()
                }]);
                cmd.RSSetScissorRects(&[RECT {
                    right: SIDE as i32,
                    bottom: SIDE as i32,
                    ..Default::default()
                }]);
                cmd.OMSetRenderTargets(1, Some(&out), false, None);
                cmd.ClearRenderTargetView(out, &[0.0; 4], None);
                cmd.DrawInstanced(3, 1, 0, 0);
                cmd.ResourceBarrier(&[gpu::transition(
                    &target,
                    D3D12_RESOURCE_STATE_RENDER_TARGET,
                    D3D12_RESOURCE_STATE_COPY_SOURCE,
                )]);
                cmd.CopyTextureRegion(&spot, 0, 0, 0, &picture, None);
                cmd.ResourceBarrier(&[
                    gpu::transition(
                        &target,
                        D3D12_RESOURCE_STATE_COPY_SOURCE,
                        D3D12_RESOURCE_STATE_COMMON,
                    ),
                    gpu::transition(
                        &output,
                        D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                        D3D12_RESOURCE_STATE_COMMON,
                    ),
                ]);
            }
            Ok(())
        })?);
        wall.push(us(began, qpc()));
        let color = seed
            .wrapping_add(7)
            .wrapping_add(key)
            .wrapping_mul(0x045d_9f3b);
        let want = u32::from_le_bytes([(color >> 16) as u8, 11, 29, 255]);
        ensure!(
            gpu::read::<u32>(&readback, pixels)?
                .into_iter()
                .all(|p| p == want),
            "frame {frame}: the streamed structured data or raster output differed"
        );
        peak = peak.max(local()?.CurrentUsage);
    }
    println!("  driver creates: {}", builds.load(Relaxed));
    let elapsed: f64 = wall.iter().sum();
    println!(
        "  total frame latency: {elapsed:.0} us; peak sampled local: {} MiB",
        peak >> 20
    );
    let gpu_gap: Vec<_> = wall
        .iter()
        .zip(&copy)
        .zip(&direct)
        .map(|((&w, c), d)| (w - c.gpu_busy.unwrap_or(0.0) - d.gpu_busy.unwrap_or(0.0)).max(0.0))
        .collect();
    b.row(
        Row::new("mixed", "end-to-end frame", "frames", 1)
            .cpu(&wall)
            .checked("all rendered pixels"),
    );
    b.row(Row::new("mixed", "program and PSO creation burst", "bursts", 1).cpu(&compile));
    b.row(Row::new("mixed", "outside timed GPU commands", "frames", 1).cpu(&gpu_gap));
    b.row(
        Row::new("mixed", "streaming copy", "frames", 1)
            .timed(&copy)
            .moving(size),
    );
    b.row(Row::new("mixed", "compute, graphics and readback", "frames", 1).timed(&direct));
    Ok(())
}
