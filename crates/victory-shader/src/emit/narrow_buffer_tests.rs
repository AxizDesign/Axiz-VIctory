//! End-to-end tests of byte and 16-bit buffer accesses by index, in buffers
//! whose elements are no whole number of dwords (`StructuredBuffer` of
//! `uint16_t`, say): synthetic GFX10 programs on the host GPU and WARP.

use super::host::{Stride, View};
use super::image_store_tests::{build_with, run_with, used};
use crate::native::ResourceType::{Srv, Uav};

/// `s[4:7]`: the view at heap index 0, `s[8:11]`: the one at 1.
const LOADS: [u32; 4] = [
    0xF408_0100,
    0xFA00_0000, // s_load_dwordx4 s[4:7], s[0:1], null
    0xF408_0200,
    0xFA00_0020, // s_load_dwordx4 s[8:11], s[0:1], 0x20
];

/// `v2` to dword `v0` of the raw buffer at heap index 1.
const OUT: [u32; 3] = [
    0x3406_0082, // v_lshlrev_b32 v3, 2, v0
    0xE070_1000,
    0x8002_0203, // buffer_store_dword v2, v3, s[8:11], 0 offen
];

fn dwords(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

#[test]
fn elements_of_odd_sizes_load_by_index() {
    // 256 bytes, each different.
    let bytes: Vec<u8> = (0..=255u8)
        .map(|b| b.wrapping_mul(167).wrapping_add(91))
        .collect();
    let data = dwords(&bytes);
    let byte = |at: usize| u32::from(bytes[at]);
    let half = |at: usize| byte(at) | byte(at + 1) << 8;
    // (instruction, its offset, bytes, signed)
    for (op, offset, size, signed) in [
        (0xE020_2000u32, 1u32, 1, false), // buffer_load_ubyte v2, v0, s[4:7], 0 idxen
        (0xE024_2000, 0, 1, true),        // buffer_load_sbyte
        (0xE028_2000, 0, 2, false),       // buffer_load_ushort
        (0xE02C_2000, 0, 2, true),        // buffer_load_sshort
        (0xE028_2000, 3, 2, false),       // across dwords in some lanes
    ] {
        for stride in [2u32, 3, 6] {
            let body = [&LOADS[..], &[op | offset, 0x8001_0200], &OUT].concat();
            let shader = build_with(&body, vec![used(Srv, 0, stride, 4), used(Uav, 1, 0, 4)])
                .unwrap_or_else(|e| panic!("stride {stride}, {op:#x}: {e}"));
            let views = [
                View::Buffer {
                    data: &data,
                    stride: Stride::Raw,
                },
                View::RawUav(32),
            ];
            let want: Vec<u32> = (0..32usize)
                .map(|t| {
                    let at = t * stride as usize + offset as usize;
                    match (size, signed) {
                        (1, false) => byte(at),
                        (1, true) => bytes[at] as i8 as u32,
                        (_, false) => half(at),
                        (_, true) => half(at) as u16 as i16 as u32,
                    }
                })
                .collect();
            for (label, got) in run_with(&shader, &views) {
                assert_eq!(got, want, "{label}: stride {stride}, {op:#x}+{offset}");
            }
        }
    }
}

#[test]
fn elements_of_odd_sizes_store_by_index_beside_each_other() {
    // Lanes share dwords: none may lose another's bytes.
    let values = [
        0x4A04_00FF,
        0x0000_A100, // v_add_nc_u32 v2, 0xa100, v0
    ];
    for (op, stride, size) in [
        (0xE068_2000u32, 2u32, 2usize), // buffer_store_short v2, v0, s[4:7], 0 idxen
        (0xE068_2000, 3, 2),            // across dwords in some lanes
        (0xE060_2000, 3, 1),            // buffer_store_byte
    ] {
        let body = [&LOADS[..2], &values, &[op, 0x8001_0200]].concat();
        let shader = build_with(&body, vec![used(Uav, 0, stride, 4)])
            .unwrap_or_else(|e| panic!("stride {stride}, {op:#x}: {e}"));
        let mut want = vec![0u8; 128];
        for t in 0..32u32 {
            let v = (0xA100 + t).to_le_bytes();
            let at = (t * stride) as usize;
            want[at..at + size].copy_from_slice(&v[..size]);
        }
        let want = dwords(&want);
        for (label, got) in run_with(&shader, &[View::RawUav(32)]) {
            assert_eq!(got, want, "{label}: stride {stride}, {op:#x}");
        }
    }
}
