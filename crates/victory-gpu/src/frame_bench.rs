//! A frame of our own through the layer, timed: the way a title's frame goes
//! (copies of title memory brought up to date, lists submitted, what shaders
//! wrote copied back), with none of a title's data. Ignored by default:
//!
//! cargo test -p victory-gpu --release --lib frame_bench -- --ignored --nocapture
//!
//! The switches are read once a process, so each setting is a run of its own.

use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D12::*;
use windows::core::s;

use crate::list::copy_tests::{close, submit, title_list, title_queue};

/// Reads `READS` dwords spread over a range and writes 64 dwords.
const SOURCE: &str = r#"
    ByteAddressBuffer i : register(t0);
    RWByteAddressBuffer o : register(u0);
    cbuffer C : register(b0) { uint first; uint bytes; uint seed; uint atomics; }
    [RootSignature("SRV(t0), UAV(u0), RootConstants(num32BitConstants=4, b0)")]
    [numthreads(64, 1, 1)]
    void main(uint3 id : SV_DispatchThreadID) {
        uint sum = seed;
        for (uint k = 0; k < 16; k++) {
            sum += i.Load(first + ((id.x * 16 + k) * 4) % bytes);
        }
        if (atomics != 0) {
            o.InterlockedAdd((id.x & 63) * 4, sum | 1);
        } else if (id.x < 64) {
            o.Store(id.x * 4, sum | 1);
        }
    }
"#;

fn pipeline() -> (ID3D12RootSignature, ID3D12PipelineState) {
    let device = &crate::host::host().unwrap().device;
    let mut code = None;
    // SAFETY: source and names outlive the call; the blob's own bytes.
    unsafe {
        D3DCompile(
            SOURCE.as_ptr().cast(),
            SOURCE.len(),
            s!("frame"),
            None,
            None,
            s!("main"),
            s!("cs_5_1"),
            0,
            0,
            &mut code,
            None,
        )
        .unwrap();
        let code = code.unwrap();
        let bytes = std::slice::from_raw_parts(code.GetBufferPointer().cast::<u8>(), code.GetBufferSize());
        let root: ID3D12RootSignature = device.CreateRootSignature(0, bytes).unwrap();
        let pso = device
            .CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
                pRootSignature: std::mem::transmute_copy(&root),
                CS: D3D12_SHADER_BYTECODE {
                    pShaderBytecode: bytes.as_ptr().cast(),
                    BytecodeLength: bytes.len(),
                },
                ..Default::default()
            })
            .unwrap();
        (root, pso)
    }
}

/// Write-watched memory, as the runtime's GPU memory is.
fn title_memory(bytes: usize) -> *mut u8 {
    use windows_sys::Win32::System::Memory::{MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc};
    const MEM_WRITE_WATCH: u32 = 0x20_0000;
    // SAFETY: fresh memory, kept for the process.
    let at = unsafe {
        VirtualAlloc(
            std::ptr::null(),
            bytes,
            MEM_RESERVE | MEM_COMMIT | MEM_WRITE_WATCH,
            PAGE_READWRITE,
        )
    };
    assert!(!at.is_null());
    at.cast()
}

#[test]
#[ignore = "a benchmark; see the module"]
fn a_frame_through_the_layer() {
    if crate::host::host().is_none() {
        return;
    }
    let number = |name: &str, default: usize| {
        std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
    };
    let (read_mb, lists, per_list) = (number("BENCH_READ_MB", 64), number("BENCH_LISTS", 16), number("BENCH_PER_LIST", 50));
    let changed = number("BENCH_CHANGED_PAGES", 400);
    let written_kb = number("BENCH_WRITTEN_KB", 256);
    let groups = number("BENCH_GROUPS", 256) as u32;
    let frames = number("BENCH_FRAMES", 30);
    // Every thread adds to the results, or one group stores them.
    let atomics = number("BENCH_ATOMICS", 0) as u32;
    let work = pipeline();
    let queue = title_queue();
    let read_bytes = read_mb << 20;
    let source = title_memory(read_bytes);
    // Results: a range of its own for each list.
    let results = title_memory(lists * (written_kb << 10).next_multiple_of(4 << 20));
    let stride = (written_kb << 10).next_multiple_of(4 << 20);
    // SAFETY: the bench's own memory.
    unsafe {
        for at in (0..read_bytes).step_by(4096) {
            source.add(at).cast::<u32>().write(at as u32);
        }
    }
    let mut times = Vec::new();
    let mut seed = 0u32;
    for frame in 0..frames + 5 {
        let started = std::time::Instant::now();
        // What the title's own threads write meanwhile.
        for k in 0..changed {
            let at = (k * 7919 + frame * 13) % (read_bytes / 4096) * 4096;
            // SAFETY: inside the source.
            unsafe { source.add(at).cast::<u32>().write((frame * 31 + k) as u32) };
        }
        let mut made = Vec::new();
        for l in 0..lists {
            let list = title_list();
            let host = crate::list::host_list(list).unwrap();
            let out = results as u64 + (l * stride) as u64;
            let written = crate::shadow::find(out, (written_kb << 10) as u64).unwrap();
            host.keep(&written.buffer);
            for d in 0..per_list {
                seed = seed.wrapping_add(1);
                let part = 1usize << 20;
                let first = ((l * per_list + d) * part) % read_bytes;
                let at = source as u64 + first as u64;
                let shadow = crate::shadow::find_any(at, part as u64).unwrap();
                shadow.refresh(host, at, part as u64);
                written.refresh(host, out, (written_kb << 10) as u64);
                written.gpu_writes(host, out, (written_kb << 10) as u64);
                host.keep(&shadow.buffer);
                // SAFETY: an open list and live objects.
                unsafe {
                    host.list.SetComputeRootSignature(&work.0);
                    host.list.SetPipelineState(&work.1);
                    host.list.SetComputeRootShaderResourceView(0, shadow.gpu(at));
                    host.list.SetComputeRootUnorderedAccessView(1, written.gpu(out));
                    host.list.SetComputeRoot32BitConstants(2, 4, [0u32, part as u32, seed, atomics].as_ptr().cast(), 0);
                    host.list.Dispatch(groups, 1, 1);
                }
            }
            host.keep(&work.0);
            host.keep(&work.1);
            close(list);
            made.push(list);
        }
        let recorded = started.elapsed();
        submit(queue, &made);
        let done = started.elapsed();
        for list in made {
            crate::object::slot::<extern "system" fn(crate::object::This) -> u32>(list, 0x10)(list);
        }
        if frame >= 5 {
            times.push((done.as_secs_f64() * 1e3, recorded.as_secs_f64() * 1e3));
        }
    }
    times.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (total, record) = times[times.len() / 2];
    // What the shaders wrote is in title memory.
    // SAFETY: inside the results.
    assert_ne!(unsafe { results.cast::<u32>().read() }, 0, "the results came back");
    println!("frame {total:7.2} ms (recording {record:6.2} ms), {:.1} fps", 1e3 / total);
}
