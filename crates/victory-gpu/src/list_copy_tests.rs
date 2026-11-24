//! The order of buffer copies beside the host GPU's work: a title's list
//! that has the GPU fill buffer A (a texture read back: GPU work that needs
//! no shader) and then copies A to B must leave the GPU's bytes in B.

use windows_sys::Win32::Foundation::S_OK;

use crate::list::{create_allocator, create_list};
use crate::object::{This, guid, slot};
use crate::resource::{buffer_bytes, create_committed};

#[allow(clippy::upper_case_acronyms)]
type HRESULT = i32;

/// `HeapProperties`, `ResourceDesc` and a copy location, as a title lays
/// them out.
const DEFAULT_HEAP: [u32; 5] = [1, 0, 0, 0, 0];

#[repr(C)]
struct Desc {
    dimension: u32,
    alignment: u64,
    width: u64,
    height: u32,
    depth: u16,
    mips: u16,
    format: u32,
    samples: [u32; 2],
    layout: u32,
    flags: u32,
}

#[repr(C)]
struct Location {
    resource: This,
    kind: u32,
    offset: u64,
    /// Format, width, height, depth, row pitch; or a subresource in the
    /// first dword of `offset`.
    footprint: [u32; 5],
}

const RGBA8: u32 = 28;
const COPY_DEST: u32 = 0x400;
const PITCH: usize = 256;
const BYTES: usize = 4 * PITCH;

fn resource(texture: bool, state: u32) -> This {
    let desc = Desc {
        dimension: if texture { 3 } else { 1 },
        alignment: 0,
        width: if texture { 4 } else { BYTES as u64 },
        height: if texture { 4 } else { 1 },
        depth: 1,
        mips: 1,
        format: if texture { RGBA8 } else { 0 },
        samples: [1, 0],
        layout: u32::from(!texture),
        flags: 0,
    };
    let iid = guid(
        0x696442be,
        0xa72e,
        0x4059,
        [0xbc, 0x79, 0x5b, 0x5c, 0x98, 0x04, 0x0f, 0xad],
    );
    let mut out = std::ptr::null();
    let made = create_committed(
        std::ptr::null(),
        DEFAULT_HEAP.as_ptr().cast(),
        0,
        (&raw const desc).cast(),
        state,
        std::ptr::null(),
        &iid,
        &mut out,
    );
    assert_eq!(made, S_OK);
    out
}

fn whole(texture: This) -> Location {
    Location {
        resource: texture,
        kind: 0,
        offset: 0,
        footprint: [0; 5],
    }
}

fn rows(buffer: This) -> Location {
    Location {
        resource: buffer,
        kind: 1,
        offset: 0,
        footprint: [RGBA8, 4, 4, 1, PITCH as u32],
    }
}

fn new_list() -> This {
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
    let null = std::ptr::null();
    let (mut allocator, mut list) = (null, null);
    assert_eq!(
        create_allocator(null, 0, &allocator_iid, &mut allocator),
        S_OK
    );
    assert_eq!(
        create_list(null, 0, 0, allocator, null, &list_iid, &mut list),
        S_OK
    );
    list
}

fn copy_texture(list: This, to: &Location, from: &Location) {
    type Copy =
        extern "system" fn(This, *const Location, u32, u32, u32, *const Location, *const u8);
    slot::<Copy>(list, 0x80)(list, to, 0, 0, 0, from, std::ptr::null());
}

fn copy_buffer(list: This, to: This, from: This) {
    type Copy = extern "system" fn(This, This, u64, This, u64, u64);
    slot::<Copy>(list, 0x78)(list, to, 0, from, 0, BYTES as u64);
}

fn submit(lists: &[This]) {
    for &list in lists {
        assert_eq!(
            slot::<extern "system" fn(This) -> HRESULT>(list, 0x48)(list),
            S_OK
        );
    }
    let queue_iid = guid(
        0x0ec870a6,
        0x5d7e,
        0x4c22,
        [0x8c, 0xfc, 0x5b, 0xaa, 0xe0, 0x76, 0x16, 0xed],
    );
    let mut queue = std::ptr::null();
    let queue_desc = [0u32; 4];
    assert_eq!(
        crate::queue::create(
            std::ptr::null(),
            queue_desc.as_ptr().cast(),
            &queue_iid,
            &mut queue
        ),
        S_OK
    );
    slot::<extern "system" fn(This, u32, *const This)>(queue, 0x50)(
        queue,
        lists.len() as u32,
        lists.as_ptr(),
    );
}

/// With submissions sent ahead: what they write back is there.
fn settled() {
    if crate::host::late() {
        crate::host::settle_for_fence();
    }
}

fn bytes(buffer: This) -> &'static mut [u8] {
    let (data, _) = buffer_bytes(buffer).unwrap();
    // SAFETY: a title buffer of `BYTES` bytes, never released here.
    unsafe { std::slice::from_raw_parts_mut(data, BYTES) }
}

/// The texels of the four rows, without the padding between rows.
fn texels(buffer: This) -> Vec<u8> {
    settled();
    bytes(buffer)
        .chunks(PITCH)
        .flat_map(|row| row[..16].to_vec())
        .collect()
}

/// A texture of known texels, and buffers A and B of other bytes.
fn made() -> Option<[This; 3]> {
    crate::host::host()?;
    let (texture, source) = (resource(true, COPY_DEST), resource(false, 0));
    let (a, b) = (resource(false, 0), resource(false, 0));
    for (i, x) in bytes(source).iter_mut().enumerate() {
        *x = (i % 251) as u8 | 1;
    }
    bytes(a).fill(0);
    bytes(b).fill(0);
    let list = new_list();
    copy_texture(list, &whole(texture), &rows(source));
    submit(&[list]);
    Some([texture, a, b])
}

fn expected() -> Vec<u8> {
    (0..4 * PITCH)
        .filter(|i| i % PITCH < 16)
        .map(|i| (i % 251) as u8 | 1)
        .collect()
}

#[test]
fn a_buffer_copy_in_a_later_submission_reads_what_the_gpu_wrote() {
    // What the two tests below build on: this holds already.
    let Some([texture, a, b]) = made() else {
        return; // no host GPU here
    };
    let first = new_list();
    copy_texture(first, &rows(a), &whole(texture));
    submit(&[first]);
    assert_eq!(texels(a), expected(), "the read back itself");
    let second = new_list();
    copy_buffer(second, b, a);
    submit(&[second]);
    assert_eq!(texels(b), expected());
}

#[test]
fn a_buffer_copy_reads_what_the_gpu_wrote_before_it_in_the_list() {
    let Some([texture, a, b]) = made() else {
        return; // no host GPU here
    };
    let list = new_list();
    copy_texture(list, &rows(a), &whole(texture));
    copy_buffer(list, b, a);
    submit(&[list]);
    assert_eq!(texels(a), expected(), "the read back itself");
    assert_eq!(texels(b), expected(), "the copy after it");
}

#[test]
fn a_buffer_copy_reads_what_an_earlier_list_of_the_submission_wrote() {
    let Some([texture, a, b]) = made() else {
        return; // no host GPU here
    };
    let (first, second) = (new_list(), new_list());
    copy_texture(first, &rows(a), &whole(texture));
    copy_buffer(second, b, a);
    submit(&[first, second]);
    assert_eq!(texels(a), expected(), "the read back itself");
    assert_eq!(texels(b), expected(), "the copy in the second list");
}
