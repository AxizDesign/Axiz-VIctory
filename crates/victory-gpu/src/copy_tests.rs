//! Buffer copies in their place among a list's commands.

use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D12::*;
use windows::core::s;

use crate::host::HostList;
use crate::object::{S_OK, This, guid, slot};

const LEN: usize = 0x1_0000;

fn buffer() -> (This, *mut u8) {
    sized_buffer(LEN)
}

/// A buffer of `len` bytes on a default heap, as title descriptions are
/// laid out.
pub(crate) fn sized_buffer(len: usize) -> (This, *mut u8) {
    let words: [u32; 14] = [1, 0, 0, 0, len as u32, (len >> 32) as u32, 1, 0x1_0001, 0, 1, 0, 1, 4, 0];
    // SAFETY: plain numbers in the descriptions' layouts.
    let (desc, heap): (crate::resource::ResourceDesc, crate::resource::HeapProperties) =
        unsafe {
            (
                std::mem::transmute::<[u32; 14], crate::resource::ResourceDesc>(words),
                std::mem::transmute::<[u32; 5], crate::resource::HeapProperties>([1, 0, 0, 0, 0]),
            )
        };
    let iid = guid(
        0x696442be,
        0xa72e,
        0x4059,
        [0xbc, 0x79, 0x5b, 0x5c, 0x98, 0x04, 0x0f, 0xad],
    );
    let mut made = std::ptr::null();
    let null = std::ptr::null();
    let hr = crate::resource::create_committed(null, &heap, 0, &desc, 0, null.cast(), &iid, &mut made);
    assert_eq!(hr, S_OK);
    let (data, size) = crate::resource::buffer_bytes(made).unwrap();
    assert_eq!(size, len as u64);
    (made, data)
}

pub(crate) fn title_list() -> This {
    let null = std::ptr::null();
    let allocator_iid = guid(
        0x6102dee4,
        0xaf59,
        0x4b09,
        [0xb9, 0x99, 0xb4, 0x4d, 0x73, 0xf0, 0x9b, 0x24],
    );
    let list_iid = guid(
        0x5b160d0f,
        0xac1b,
        0x4185,
        [0x8b, 0xa8, 0xb3, 0xae, 0x42, 0xa5, 0xa4, 0x55],
    );
    let (mut allocator, mut list) = (null, null);
    assert_eq!(super::create_allocator(null, 0, &allocator_iid, &mut allocator), S_OK);
    assert_eq!(super::create_list(null, 0, 0, allocator, null, &list_iid, &mut list), S_OK);
    list
}

pub(crate) fn title_queue() -> This {
    let iid = guid(
        0x0ec870a6,
        0x5d7e,
        0x4c22,
        [0x8c, 0xfc, 0x5b, 0xaa, 0xe0, 0x76, 0x16, 0xed],
    );
    let desc = [0u32; 4];
    let mut queue = std::ptr::null();
    assert_eq!(
        crate::queue::create(std::ptr::null(), desc.as_ptr().cast(), &iid, &mut queue),
        S_OK
    );
    queue
}

/// A shader that writes 0xC0DE0000 + i to the first 64 dwords.
fn writer() -> (ID3D12RootSignature, ID3D12PipelineState) {
    const SOURCE: &str = r#"
        RWByteAddressBuffer o : register(u0);
        [RootSignature("UAV(u0)")]
        [numthreads(64, 1, 1)]
        void main(uint3 id : SV_DispatchThreadID) { o.Store(id.x * 4, 0xC0DE0000u + id.x); }
    "#;
    let device = &crate::host::host().unwrap().device;
    let mut code = None;
    // SAFETY: source and names outlive the call; the blob's own bytes.
    unsafe {
        D3DCompile(
            SOURCE.as_ptr().cast(),
            SOURCE.len(),
            s!("writer"),
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
        let bytes = std::slice::from_raw_parts(
            code.GetBufferPointer().cast::<u8>(),
            code.GetBufferSize(),
        );
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

/// Records, as a title dispatch would, a shader that writes 256 bytes at
/// `address`.
fn dispatch(list: &HostList, work: &(ID3D12RootSignature, ID3D12PipelineState), address: u64) {
    let shadow = crate::shadow::find(address, 256).unwrap();
    shadow.refresh(list, address, 256);
    shadow.gpu_writes(list, address, 256);
    list.keep(&shadow.buffer);
    list.keep(&work.0);
    list.keep(&work.1);
    // SAFETY: an open list and live objects.
    unsafe {
        list.list.SetComputeRootSignature(&work.0);
        list.list.SetPipelineState(&work.1);
        list.list
            .SetComputeRootUnorderedAccessView(0, shadow.gpu(address));
        list.list.Dispatch(1, 1, 1);
    }
}

type Copy = extern "system" fn(This, This, u64, This, u64, u64);

fn copy(list: This, to: (This, u64), from: (This, u64)) {
    slot::<Copy>(list, 0x78)(list, to.0, to.1, from.0, from.1, 256);
}

pub(crate) fn close(list: This) {
    assert_eq!(
        slot::<extern "system" fn(This) -> crate::object::HRESULT>(list, 0x48)(list),
        S_OK
    );
}

/// Submits as the title does: with submissions sent ahead nothing is
/// waited for.
pub(crate) fn send(queue: This, lists: &[This]) {
    slot::<extern "system" fn(This, u32, *const This)>(queue, 0x50)(
        queue,
        lists.len() as u32,
        lists.as_ptr(),
    );
}

pub(crate) fn submit(queue: This, lists: &[This]) {
    send(queue, lists);
    // (With submissions sent ahead: what they write back is there.)
    if crate::host::late() {
        crate::host::settle_for_fence();
    }
}

fn words(at: *const u8) -> Vec<u32> {
    // SAFETY: at least 256 bytes of a buffer of ours.
    unsafe { std::slice::from_raw_parts(at.cast::<u32>(), 64) }.to_vec()
}

fn written() -> Vec<u32> {
    (0..64).map(|i| 0xC0DE_0000 + i).collect()
}

/// A copy of what a shader wrote before it copies that: in the same list
/// (between two buffers, and within one), and in a later list of the same
/// submission. A copy before any such write is of title memory.
#[test]
fn copies_see_what_shaders_wrote_before_them() {
    if crate::host::host().is_none() {
        return; // no host GPU here
    }
    let work = writer();
    let queue = title_queue();
    let (a, a_at) = buffer();
    let (b, b_at) = buffer();
    // SAFETY: buffers of `LEN` bytes.
    unsafe {
        a_at.write_bytes(0x11, LEN);
        b_at.write_bytes(0x22, LEN);
    }

    // In the same list.
    let list = title_list();
    let host = super::host_list(list).unwrap();
    copy(list, (b, 0x1000), (a, 0));
    dispatch(host, &work, a_at as u64);
    copy(list, (b, 0), (a, 0));
    copy(list, (a, 0x2000), (a, 0));
    // Of a copy made on the GPU: there too.
    copy(list, (b, 0x3000), (b, 0));
    close(list);
    submit(queue, &[list]);
    assert_eq!(words(a_at), written(), "what the shader wrote");
    // SAFETY (each): inside the buffers.
    unsafe {
        assert_eq!(words(b_at), written(), "between two buffers");
        assert_eq!(words(a_at.add(0x2000)), written(), "within one");
        assert_eq!(words(b_at.add(0x3000)), written(), "of a copy");
        assert_eq!(words(b_at.add(0x1000)), [0x1111_1111; 64], "before the dispatch");
        assert_eq!(words(b_at.add(0x100)), [0x2222_2222; 64], "nothing past the copy");
    }

    // In two lists of one submission.
    // SAFETY: as above.
    unsafe {
        a_at.write_bytes(0x33, LEN);
        b_at.write_bytes(0x44, LEN);
    }
    let (first, second) = (title_list(), title_list());
    dispatch(super::host_list(first).unwrap(), &work, a_at as u64);
    copy(second, (b, 0), (a, 0));
    close(first);
    close(second);
    submit(queue, &[first, second]);
    assert_eq!(words(b_at), written(), "in a later list of the submission");
    // SAFETY: a plain device query.
    assert!(unsafe { crate::host::host().unwrap().device.GetDeviceRemovedReason() }.is_ok());
}

/// Unordered access clears fill what their view shows, of integers and of
/// floats, and nothing else; a handle without a view clears nothing.
#[test]
fn unordered_clears_fill_the_view() {
    if crate::host::host().is_none() {
        return; // no host GPU here
    }
    let queue = title_queue();
    let (a, a_at) = buffer();
    // SAFETY: a buffer of `LEN` bytes.
    unsafe { a_at.write_bytes(0x11, LEN) };
    let null = std::ptr::null();
    // Eight views, seen by shaders.
    let mut heap_desc = [0u32; 16];
    (heap_desc[1], heap_desc[2]) = (8, 1);
    let iid = guid(
        0x8efb471d,
        0x616c,
        0x4f49,
        [0x90, 0xf7, 0x12, 0x7b, 0xb7, 0x63, 0xfa, 0x51],
    );
    let mut heap = null;
    assert_eq!(
        crate::descriptor::create_heap(null, heap_desc.as_ptr().cast(), &iid, &mut heap),
        S_OK
    );
    let start = crate::descriptor::heap(heap).start;
    let handle = |n: u64| start + n * 32;
    // R32_UINT from element 4, 8 of them; R32_FLOAT from 16, 8; raw from 64, 16.
    let view = |format: u32, first: u32, count: u32, flags: u32| [format, 1, first, 0, count, 0, 0, 0, flags, 0];
    crate::descriptor::create_uav(null, a, null, &view(42, 4, 8, 0), handle(0));
    crate::descriptor::create_uav(null, a, null, &view(41, 16, 8, 0), handle(1));
    crate::descriptor::create_uav(null, a, null, &view(39, 64, 16, 1), handle(2));
    let list = title_list();
    slot::<extern "system" fn(This, u32, *const This)>(list, 0x100)(list, 1, &heap);
    type Clear = extern "system" fn(This, u64, u64, This, *const [u32; 4], u32, *const u8);
    let clear = |at: usize, n: u64, values: [u32; 4]| {
        slot::<Clear>(list, at)(list, handle(n), handle(n), a, &values, 0, null.cast());
    };
    clear(0x1a8, 0, [0xABCD_0001; 4]);
    clear(0x1b0, 1, [1.5f32.to_bits(); 4]);
    clear(0x1b0, 2, [2.0f32.to_bits(); 4]);
    clear(0x1a8, 5, [7; 4]);
    close(list);
    submit(queue, &[list]);
    // SAFETY: inside the buffer.
    let got = unsafe { std::slice::from_raw_parts(a_at.cast::<u32>(), 96) };
    for (i, &word) in got.iter().enumerate() {
        let wanted = match i {
            4..12 => 0xABCD_0001,
            16..24 => 1.5f32.to_bits(),
            64..80 => 2.0f32.to_bits(),
            _ => 0x1111_1111,
        };
        assert_eq!(word, wanted, "dword {i}");
    }
}

/// One mip of a texture copied into another in one list, with the title's
/// barriers on each mip: both are in their own state (one state for the
/// whole texture is the wrong one for one of them), and the contents are
/// there. Run with the debug layer it says nothing.
#[test]
fn a_mip_is_copied_into_another() {
    if crate::host::host().is_none() {
        return; // no host GPU here
    }
    let queue = title_queue();
    let null = std::ptr::null();
    // A 64x64 RGBA8 render target with two mips.
    let words: [u32; 14] = [3, 0, 0, 0, 64, 0, 64, 0x2_0001, 28, 1, 0, 0, 1, 0];
    // SAFETY: plain numbers in the descriptions' layouts.
    let (desc, heap) = unsafe {
        (
            std::mem::transmute::<[u32; 14], crate::resource::ResourceDesc>(words),
            std::mem::transmute::<[u32; 5], crate::resource::HeapProperties>([1, 0, 0, 0, 0]),
        )
    };
    let iid = guid(
        0x696442be,
        0xa72e,
        0x4059,
        [0xbc, 0x79, 0x5b, 0x5c, 0x98, 0x04, 0x0f, 0xad],
    );
    const TARGET: u32 = 0x4;
    const COPY_SOURCE: u32 = 0x800;
    const COPY_DEST: u32 = 0x400;
    let mut texture = null;
    assert_eq!(
        crate::resource::create_committed(null, &heap, 0, &desc, TARGET, null.cast(), &iid, &mut texture),
        S_OK
    );
    let (back, back_at) = buffer();
    // Two target views, one a mip: descriptor memory of our own.
    let mut slots = vec![0u64; 64];
    let handle = |n: usize| slots_at(&slots) + n as u64 * 128;
    for mip in 0..2usize {
        let view = [28u32, 4, mip as u32, 0, 0, 0];
        crate::descriptor::create_rtv(null, texture, &view, handle(mip));
    }
    let list = title_list();
    type Clear = extern "system" fn(This, u64, *const [f32; 4], u32, *const u8);
    let clear = |mip: usize, colour: [f32; 4]| {
        slot::<Clear>(list, 0x1a0)(list, handle(mip), &colour, 0, null.cast());
    };
    #[repr(C)]
    struct Barrier {
        kind: u32,
        flags: u32,
        resource: This,
        sub: u32,
        before: u32,
        after: u32,
        pad: u32,
    }
    let barrier = |sub: u32, before: u32, after: u32| {
        let b = Barrier { kind: 0, flags: 0, resource: texture, sub, before, after, pad: 0 };
        slot::<extern "system" fn(This, u32, *const Barrier) -> u32>(list, 0xd0)(list, 1, &b);
    };
    #[repr(C)]
    struct Location {
        resource: This,
        kind: u32,
        /// Offset; format, width; height, depth; row pitch.
        footprint: [u64; 4],
    }
    let at_sub = |sub: u64| Location {
        resource: texture,
        kind: 0,
        // (the index lies where a footprint has its offset)
        footprint: [sub, 0, 0, 0],
    };
    let rows = Location {
        resource: back,
        kind: 1,
        footprint: [0, 28 | 64 << 32, 64 | 1 << 32, 256],
    };
    type Copy = extern "system" fn(This, *const Location, u32, u32, u32, *const Location, *const u8);
    let copy = |to: &Location, from: &Location| slot::<Copy>(list, 0x80)(list, to, 0, 0, 0, from, null.cast());
    clear(0, [1.0, 0.0, 0.0, 1.0]);
    clear(1, [0.0, 1.0, 0.0, 1.0]);
    // (Once as an enhanced barrier: the same mip there and back; and one on
    // something that is no texture, which is left out.)
    let layouts = |resource: This, before: u32, after: u32| {
        let mut words = [0u64; 8];
        words[2] = u64::from(before) | u64::from(after) << 32;
        words[3] = resource as u64;
        // Mip 1, one of them, slice 0, one.
        (words[4], words[5]) = (1 | 1 << 32, 1 << 32);
        words
    };
    const IN_TARGET: u32 = 2;
    const IN_SHADERS: u32 = 6;
    let barriers = [
        layouts(texture, IN_TARGET, IN_SHADERS),
        layouts(texture, IN_SHADERS, IN_TARGET),
        layouts(words.as_ptr().cast(), IN_TARGET, IN_SHADERS),
    ];
    let group = [1u64 | 3 << 32, barriers.as_ptr() as u64];
    super::barrier_groups(list, 1, group.as_ptr().cast());
    barrier(1, TARGET, COPY_SOURCE);
    barrier(0, TARGET, COPY_DEST);
    copy(&at_sub(0), &at_sub(1));
    barrier(0, COPY_DEST, COPY_SOURCE);
    copy(&rows, &at_sub(0));
    close(list);
    submit(queue, &[list]);
    slots.clear();
    // SAFETY: 64 rows of 256 bytes in a buffer of `LEN`.
    let texel = |x: usize, y: usize| unsafe { back_at.add(y * 256 + x * 4).cast::<u32>().read_unaligned() };
    assert_eq!(texel(0, 0), 0xff00_ff00, "the small mip's green");
    assert_eq!(texel(31, 31), 0xff00_ff00);
    assert_eq!(texel(32, 31), 0xff00_00ff, "the large mip's red around it");
    assert_eq!(texel(63, 63), 0xff00_00ff);
}

fn slots_at(slots: &[u64]) -> u64 {
    slots.as_ptr() as u64
}

/// A texture copied into a buffer, and the buffer copied by the next
/// submission, with nothing in between that waits for the first: the copy
/// is of the texture's bytes, also where submissions are sent ahead.
#[test]
fn the_next_submission_copies_what_a_texture_was_copied_into() {
    if crate::host::host().is_none() {
        return; // no host GPU here
    }
    let queue = title_queue();
    let null = std::ptr::null();
    let words: [u32; 14] = [3, 0, 0, 0, 64, 0, 64, 0x1_0001, 28, 1, 0, 0, 1, 0];
    // SAFETY: plain numbers in the descriptions' layouts.
    let (desc, heap) = unsafe {
        (
            std::mem::transmute::<[u32; 14], crate::resource::ResourceDesc>(words),
            std::mem::transmute::<[u32; 5], crate::resource::HeapProperties>([1, 0, 0, 0, 0]),
        )
    };
    let iid = guid(
        0x696442be,
        0xa72e,
        0x4059,
        [0xbc, 0x79, 0x5b, 0x5c, 0x98, 0x04, 0x0f, 0xad],
    );
    let mut texture = null;
    assert_eq!(
        crate::resource::create_committed(null, &heap, 0, &desc, 0x4, null.cast(), &iid, &mut texture),
        S_OK
    );
    let (a, _) = buffer();
    let (b, b_at) = buffer();
    let slots = vec![0u64; 32];
    crate::descriptor::create_rtv(null, texture, &[28, 4, 0, 0, 0, 0], slots_at(&slots));
    let (first, second) = (title_list(), title_list());
    type Clear = extern "system" fn(This, u64, *const [f32; 4], u32, *const u8);
    slot::<Clear>(first, 0x1a0)(first, slots_at(&slots), &[0.0, 0.0, 1.0, 1.0], 0, null.cast());
    // Resource, kind, then the place: a subresource; rows of 256 bytes.
    let from = [texture as u64, 0, 0, 0, 0, 0];
    let to = [a as u64, 1, 0, 28 | 64 << 32, 64 | 1 << 32, 256];
    type Region = extern "system" fn(This, *const u64, u32, u32, u32, *const u64, *const u8);
    slot::<Region>(first, 0x80)(first, to.as_ptr(), 0, 0, 0, from.as_ptr(), null.cast());
    copy(second, (b, 0), (a, 0));
    close(first);
    close(second);
    for list in [first, second] {
        slot::<extern "system" fn(This, u32, *const This)>(queue, 0x50)(queue, 1, &list);
    }
    if crate::host::late() {
        crate::host::settle_for_fence();
    }
    assert_eq!(words_of(b_at), [0xffff_0000; 64]);
}

fn words_of(at: *const u8) -> Vec<u32> {
    words(at)
}
