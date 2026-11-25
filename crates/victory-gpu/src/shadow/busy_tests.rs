//! Title writes under ranges that shaders of the same submission are
//! declared to write.

use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D12::*;
use windows::core::s;

use super::*;
use crate::list::copy_tests::{close, submit, title_list, title_queue};

/// Write-watched memory, as the runtime's GPU memory is; kept for the process.
fn pool(bytes: usize) -> usize {
    use windows_sys::Win32::System::Memory::{MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc};
    const MEM_WRITE_WATCH: u32 = 0x20_0000;
    // SAFETY: fresh memory.
    let at = unsafe {
        VirtualAlloc(std::ptr::null(), bytes, MEM_RESERVE | MEM_COMMIT | MEM_WRITE_WATCH, PAGE_READWRITE)
    };
    assert!(!at.is_null());
    at as usize
}

fn store(at: usize, byte: u8) {
    // SAFETY: a byte of a pool of ours.
    unsafe { (at as *mut u8).write_volatile(byte) }
}

fn load(at: usize) -> u8 {
    // SAFETY: a byte of a pool of ours.
    unsafe { (at as *const u8).read_volatile() }
}

/// The byte of the copy's CPU side for title address `at`.
fn held(copy: &Shadow, at: usize) -> u8 {
    // SAFETY: inside the copy's mapping.
    unsafe { copy.data.add(at - copy.start as usize).read_volatile() }
}

/// After Marvisak/VIctory ed0dbfd's test.
#[test]
fn title_writes_under_a_busy_range_reach_the_copy_and_stay() {
    if crate::host::host().is_none() {
        return;
    }
    let len = 2 * PAGE;
    let p = pool(len);
    register(p, p as u64, len as u64);
    let copy = find(p as u64, len as u64).unwrap();
    assert!(!copy.carded(), "a small buffer's copy is in the PC's memory");
    copy.update(p, len);
    // An earlier list declares all of it as written by shaders.
    copy.writing(p, len);
    store(p + PAGE + 20, 0x55);
    copy.update(p + PAGE + 16, 16);
    assert_eq!(held(&copy, p + PAGE + 20), 0x55, "the title's write reaches the copy");
    take_snapshots();
    // The shaders run: one byte written. The title writes meanwhile.
    // SAFETY: inside the copy's mapping.
    unsafe { copy.data.add(10).write_volatile(0xaa) };
    store(p + PAGE + 20, 0x66);
    copy.copy_back(p, len);
    copy.copy_back_done(&[(p, p + len)]);
    assert_eq!(load(p + 10), 0xaa, "the shader's write goes back");
    assert_eq!(load(p + PAGE + 20), 0x66, "the title's later write stays");
    unregister(p, p as u64);
}

/// A copy in the card's memory: what was merged into its CPU side has to
/// go to the card as well.
#[test]
fn title_writes_under_a_busy_range_go_to_the_card() {
    if crate::host::host().is_none() {
        return;
    }
    let len = 64 << 20;
    let p = pool(len);
    register(p, p as u64, len as u64);
    let copy = find(p as u64, len as u64).unwrap();
    if copy.carded() {
        copy.update(p, len);
        copy.card.as_ref().unwrap().dirty.lock().unwrap().clear();
        copy.writing(p, len);
        store(p + 5 * PAGE + 20, 0x55);
        copy.update(p + 5 * PAGE, 4096);
        assert_eq!(held(&copy, p + 5 * PAGE + 20), 0x55);
        let dirty = copy.card.as_ref().unwrap().dirty.lock().unwrap().clone();
        assert!(dirty.iter().any(|&(a, e)| a <= 5 * PAGE + 20 && e > 5 * PAGE + 20), "{dirty:?}");
        copy.copy_back_done(&[(p, p + len)]);
    }
    unregister(p, p as u64);
}

/// Copies the dword at the root SRV to the root UAV.
fn reader() -> (ID3D12RootSignature, ID3D12PipelineState) {
    const SOURCE: &str = r#"
        ByteAddressBuffer i : register(t0);
        RWByteAddressBuffer o : register(u0);
        [RootSignature("SRV(t0), UAV(u0)")]
        [numthreads(1, 1, 1)]
        void main() { o.Store(0, i.Load(0)); }
    "#;
    let device = &crate::host::host().unwrap().device;
    let mut code = None;
    // SAFETY: source and names outlive the call; the blob's own bytes.
    unsafe {
        D3DCompile(
            SOURCE.as_ptr().cast(),
            SOURCE.len(),
            s!("reader"),
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

/// One list's shaders may write all of a buffer (a root UAV has no size, so
/// the whole rest is declared). The next list of the same submission copies
/// constants into it, and its shader reads them.
fn constants_copied_by_a_later_list(len: usize) {
    if crate::host::host().is_none() {
        return;
    }
    let (p, up, out) = (pool(len), pool(PAGE), pool(PAGE));
    for (at, len) in [(p, len), (up, PAGE), (out, PAGE)] {
        register(at, at as u64, len as u64);
    }
    let copy = find(p as u64, len as u64).unwrap();
    let result = find(out as u64, 4).unwrap();
    let work = reader();
    let queue = title_queue();
    for round in 1..=3u32 {
        let value = 0xC0DE_0000 + round;
        // SAFETY: a dword of ours.
        unsafe { (up as *mut u32).write_volatile(value) };
        let constants = p + len - PAGE + 512;
        let (first, second) = (title_list(), title_list());
        let h = crate::list::host_list(first).unwrap();
        copy.refresh(h, p as u64, len as u64);
        copy.gpu_writes(h, p as u64, len as u64);
        h.keep(&copy.buffer);
        close(first);
        let h = crate::list::host_list(second).unwrap();
        h.defer_copy(constants as *mut u8, up as *const u8, 4);
        copy.refresh(h, constants as u64, 4);
        result.refresh(h, out as u64, 4);
        result.gpu_writes(h, out as u64, 4);
        for o in [&copy.buffer, &result.buffer] {
            h.keep(o);
        }
        h.keep(&work.0);
        h.keep(&work.1);
        // SAFETY: an open list and live objects.
        unsafe {
            h.list.SetComputeRootSignature(&work.0);
            h.list.SetPipelineState(&work.1);
            h.list.SetComputeRootShaderResourceView(0, copy.gpu(constants as u64));
            h.list.SetComputeRootUnorderedAccessView(1, result.gpu(out as u64));
            h.list.Dispatch(1, 1, 1);
        }
        close(second);
        submit(queue, &[first, second]);
        crate::host::settle_for_fence();
        // SAFETY: dwords of ours.
        unsafe {
            assert_eq!((out as *const u32).read_volatile(), value, "round {round}: what the shader read");
            assert_eq!((constants as *const u32).read_volatile(), value, "round {round}: title memory keeps it");
        }
        for list in [first, second] {
            crate::object::slot::<extern "system" fn(crate::object::This) -> u32>(list, 0x10)(list);
        }
    }
    for at in [p, up, out] {
        unregister(at, at as u64);
    }
}

#[test]
fn constants_copied_by_a_later_list_are_read_from_system_memory() {
    constants_copied_by_a_later_list(4 * PAGE);
}

#[test]
fn constants_copied_by_a_later_list_are_read_from_the_card() {
    constants_copied_by_a_later_list(64 << 20);
}

/// What bringing one big buffer up to date costs when thousands of
/// dispatches bind it from different places to its end:
///
/// cargo test -p victory-gpu --release --lib many_views -- --ignored --nocapture
#[test]
#[ignore = "a benchmark"]
fn many_views_of_one_heap() {
    if crate::host::host().is_none() {
        return;
    }
    let len = 174 << 20;
    let p = pool(len);
    register(p, p as u64, len as u64);
    let copy = find(p as u64, len as u64).unwrap();
    let queue = title_queue();
    let mut times = Vec::new();
    for frame in 0..25usize {
        for k in 0..800usize {
            store(p + (k * 7919 + frame * 13) % (len / 4096) * 4096, frame as u8);
        }
        let mut made = Vec::new();
        for l in 0..8usize {
            let list = title_list();
            let h = crate::list::host_list(list).unwrap();
            for d in 0..400usize {
                let at = (p + (l * 400 + d) % 500 * 4096) as u64;
                copy.refresh(h, at, (p + len) as u64 - at);
                copy.gpu_writes(h, at, (p + len) as u64 - at);
            }
            h.keep(&copy.buffer);
            close(list);
            made.push(list);
        }
        let started = std::time::Instant::now();
        submit(queue, &made);
        crate::host::settle_for_fence();
        if frame >= 5 {
            times.push(started.elapsed().as_secs_f64() * 1e3);
        }
        for list in made {
            crate::object::slot::<extern "system" fn(crate::object::This) -> u32>(list, 0x10)(list);
        }
    }
    times.sort_by(f64::total_cmp);
    println!("views {:7.2} ms a submission, on the card: {}", times[times.len() / 2], copy.carded());
    unregister(p, p as u64);
}
