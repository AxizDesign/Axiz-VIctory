//! Title writes under ranges that shaders of the same submission are
//! declared to write.

use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D12::*;
use windows::core::s;

use super::*;
use crate::list::copy_tests::{close, send, submit, title_list, title_queue};

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
    program(
        r#"
        ByteAddressBuffer i : register(t0);
        RWByteAddressBuffer o : register(u0);
        [RootSignature("SRV(t0), UAV(u0)")]
        [numthreads(1, 1, 1)]
        void main() { o.Store(0, i.Load(0)); }
    "#,
    )
}

/// Writes 64 dwords at the root UAV: the root constant and up.
fn writer() -> (ID3D12RootSignature, ID3D12PipelineState) {
    program(
        r#"
        RWByteAddressBuffer o : register(u0);
        cbuffer C : register(b0) { uint first; }
        [RootSignature("UAV(u0), RootConstants(num32BitConstants=1, b0)")]
        [numthreads(64, 1, 1)]
        void main(uint3 id : SV_DispatchThreadID) { o.Store(id.x * 4, first + id.x); }
    "#,
    )
}

fn program(source: &str) -> (ID3D12RootSignature, ID3D12PipelineState) {
    let device = &crate::host::host().unwrap().device;
    let mut code = None;
    // SAFETY: source and names outlive the call; the blob's own bytes.
    unsafe {
        D3DCompile(
            source.as_ptr().cast(),
            source.len(),
            s!("program"),
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

/// A frame of many submissions, each binding one big buffer as a whole for
/// writing (root views have no size) and writing 64 dwords of it:
///
/// cargo test -p victory-gpu --release --lib many_submissions -- --ignored --nocapture
#[test]
#[ignore = "a benchmark"]
fn many_submissions_declare_one_heap() {
    if crate::host::host().is_none() {
        return;
    }
    let number = |name: &str, default: usize| std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default);
    let (mb, submissions, per) = (number("BENCH_MB", 174), number("BENCH_SUBMISSIONS", 40), number("BENCH_PER", 4));
    let len = (mb << 20).max((submissions * per + 2) << 16);
    let p = pool(len);
    register(p, p as u64, len as u64);
    let copy = find(p as u64, len as u64).unwrap();
    let work = writer();
    let queue = title_queue();
    let mut times = Vec::new();
    for frame in 0..number("BENCH_FRAMES", 30) {
        let mut made = Vec::new();
        let started = std::time::Instant::now();
        for l in 0..submissions {
            for k in 0..10usize {
                store(p + ((l * 10 + k) * 7919 + frame * 13) % (len / 4096) * 4096 + 2048, frame as u8);
            }
            let list = title_list();
            let h = crate::list::host_list(list).unwrap();
            for d in 0..per {
                let at = (p + (l * per + d) * 65536) as u64;
                copy.refresh(h, at, (p + len) as u64 - at);
                copy.gpu_writes(h, at, (p + len) as u64 - at);
                // SAFETY: an open list and live objects.
                unsafe {
                    h.list.SetComputeRootSignature(&work.0);
                    h.list.SetPipelineState(&work.1);
                    h.list.SetComputeRootUnorderedAccessView(0, copy.gpu(at));
                    h.list.SetComputeRoot32BitConstants(1, 1, [(frame * 1000 + l) as u32].as_ptr().cast(), 0);
                    h.list.Dispatch(1, 1, 1);
                }
            }
            h.keep(&copy.buffer);
            h.keep(&work.0);
            h.keep(&work.1);
            close(list);
            send(queue, &[list]);
            made.push(list);
        }
        let sent = started.elapsed().as_secs_f64() * 1e3;
        crate::host::settle_for_fence();
        if frame >= 5 {
            times.push((started.elapsed().as_secs_f64() * 1e3, sent));
        }
        for (l, list) in made.into_iter().enumerate() {
            // SAFETY: inside the pool.
            let got = unsafe { ((p + l * per * 65536) as *const u32).read_volatile() };
            assert_eq!(got, (frame * 1000 + l) as u32, "frame {frame} submission {l}");
            crate::object::slot::<extern "system" fn(crate::object::This) -> u32>(list, 0x10)(list);
        }
    }
    times.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (total, sent) = times[times.len() / 2];
    println!("submissions {total:7.2} ms a frame, {sent:7.2} ms on the submitting thread");
    unregister(p, p as u64);
}

/// A byte the title writes costs the kernel's page it lies in, not 64 KiB.
#[test]
fn a_written_byte_copies_its_kernel_page() {
    if crate::host::host().is_none() {
        return;
    }
    let len = 4 * PAGE;
    let p = pool(len);
    register(p, p as u64, len as u64);
    let copy = find(p as u64, len as u64).unwrap();
    // (A page nothing touched yet is told as written once it is read.)
    for at in (p..p + len).step_by(4096) {
        store(at, 9);
    }
    copy.update(p, len);
    assert_eq!(copy.update(p, len), 0, "nothing was written");
    store(p + PAGE + 5000, 1);
    assert_eq!(copy.update(p, len), 4096);
    assert_eq!(held(&copy, p + PAGE + 5000), 1);
    // The two ends of a page, and the next page's start: two pieces.
    store(p + PAGE, 2);
    store(p + 2 * PAGE - 1, 3);
    store(p + 2 * PAGE, 4);
    assert_eq!(copy.update(p, len), 3 * 4096);
    // A copy that missed a write of a page takes all of it: another copy
    // asked (and so cleared the kernel's bits) in between.
    store(p + 3 * PAGE + 100, 5);
    let _ = crate::watch::writes(p, p + len, true);
    store(p + 3 * PAGE + 50_000, 6);
    let _ = crate::watch::writes(p, p + len, true);
    assert_eq!(copy.update(p, len), PAGE);
    assert_eq!((held(&copy, p + 3 * PAGE + 100), held(&copy, p + 3 * PAGE + 50_000)), (5, 6));
    unregister(p, p as u64);
}

/// Title writes anywhere, refreshes of parts of two copies that overlap,
/// with ranges shaders are declared to write and without: what was
/// refreshed is what title memory holds.
#[test]
fn refreshed_ranges_hold_title_memory_whatever_was_written() {
    if crate::host::host().is_none() {
        return;
    }
    let len = 8 * PAGE;
    let p = pool(len);
    // The second is inside the first and starts off a page.
    let (inner, inner_len) = (p + 2 * PAGE + 4096 + 256, 3 * PAGE);
    register(p, p as u64, len as u64);
    register(inner, inner as u64, inner_len as u64);
    let outer = find(p as u64, len as u64).unwrap();
    let nested = find(inner as u64, inner_len as u64).unwrap();
    assert_ne!(outer.id(), nested.id());
    let mut seed = 0x1234_5678_9abc_def0u64;
    let mut next = move |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 20) as usize % n
    };
    let mut declared: Vec<(usize, usize)> = Vec::new();
    for round in 0..4000 {
        for _ in 0..next(6) {
            let most = if next(8) == 0 { 2 * PAGE } else { 64 };
            let (at, n) = (next(len), 1 + next(most));
            for k in at..(at + n).min(len) {
                store(p + k, next(256) as u8);
            }
        }
        if next(5) == 0 {
            // As a submission asks, once for all of it.
            crate::watch::ask_once(&[(p, p + len), (inner, inner + inner_len)]);
            let (at, n) = (next(len - 64), 1 + next(64));
            for k in at..at + n {
                store(p + k, next(256) as u8);
            }
            crate::watch::wrote(p + at, n);
        }
        // Shaders are declared to write some of the outer copy for a while
        // (and write nothing): the title's bytes must reach it all the same.
        if next(20) == 0 {
            let (at, n) = (next(len - 1), 1 + next(3 * PAGE));
            let n = n.min(len - at);
            outer.update(p + at, n);
            outer.writing(p + at, n);
            declared.push((p + at, p + at + n));
        }
        if next(40) == 0 {
            take_snapshots();
            for range in declared.drain(..) {
                outer.copy_back(range.0, range.1 - range.0);
                outer.copy_back_done(&[range]);
            }
        }
        let (copy, base, size) = if next(2) == 0 {
            (&outer, p, len)
        } else {
            (&nested, inner, inner_len)
        };
        let at = next(size);
        let most = if next(3) == 0 { size } else { 5000 };
        let n = (1 + next(most)).min(size - at);
        copy.update(base + at, n);
        for k in 0..n {
            let a = base + at + k;
            assert_eq!(held(copy, a), load(a), "round {round}, byte {:#x} of {at:#x}+{n:#x}", a - base);
        }
        crate::watch::ask_once_end();
    }
    unregister(inner, inner as u64);
    unregister(p, p as u64);
}
