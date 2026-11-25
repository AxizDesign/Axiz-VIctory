//! A console wave64 compute program on a host of 32-lane waves: a wave reduction inside a
//! per-lane branch, with that branch taken wave-wide ([`crate::structure::structure_wide`])
//! so that the two host waves can exchange their halves inside it. Lowered, validated by
//! `dxil.dll` when present, and run on the host GPU when it takes waves of 32.

use std::collections::HashSet;

use super::host::{Dispatch, Host, Root, View};
use super::{HostShader, lower, validator};
use crate::native::{
    ComputeIo, HwStage, NativeProgram, ResourceType, RootParam, RootType, StageIo, UsedResource,
};
use crate::{ShaderError, io, ir, resource, structure};

const S_ENDPGM: u32 = 0xBF81_0000;

/// 64 threads in one console wave. Each thread t holds the bit `1 << (t >> 1)`; the threads
/// below s0 (a root constant) OR theirs over the wave inside `if (t < s0) { ... }` and store
/// the result to dword 0 of the raw UAV.
/// `skip`: the branch past the body when no thread is below s0: `s_cbranch_execz`, or
/// `s_cbranch_scc0` on the SCC `s_and_saveexec` sets ("some lane is left").
fn program(skip: u32) -> NativeProgram {
    let mut words = vec![
        0x7E0A_0281, // v_mov_b32 v5, 1
        0x2C0C_0081, // v_lshrrev_b32 v6, 1, v0
        0x340E_0B06, // v_lshlrev_b32 v7, v6, v5
        0x7D88_0000, // v_cmp_gt_u32 vcc, s0, v0
        0xBE8C_246A, // s_and_saveexec_b64 s[12:13], vcc
        skip | 23,   // the branch to the restore of EXEC
    ];
    words.extend_from_slice(&super::wave::WAVE_OR_V7_TO_S7);
    words.extend_from_slice(&[
        0x7E00_0207, // v_mov_b32 v0, s7
        0x38 << 26 | 0x1C << 18,
        0x80 << 24 | 2 << 16, // buffer_store_dword v0, off, s[8:11], 0
        0xBEFE_040C, // s_mov_b64 exec, s[12:13]
    ]);
    native(&words)
}

/// A wave64 compute program of 64 threads (one console wave; the thread index in v0) with 4
/// root constants in s0..s3 and a raw UAV's descriptor in s[8:11], then `body`.
fn native(body: &[u32]) -> NativeProgram {
    native_of(body, true, 64)
}

/// The same for a console of 32-lane waves when `wave64` is not set, with `threads` threads.
fn native_of(body: &[u32], wave64: bool, threads: u32) -> NativeProgram {
    let mut words = vec![
        0xBE85_0380, // s_mov_b32 s5, 0
        0x3D << 26 | 0x02 << 18 | 8 << 6 | 2,
        125 << 25, // s_load_dwordx4 s[8:11], s[4:5], 0
    ];
    words.extend_from_slice(body);
    words.push(S_ENDPGM);
    NativeProgram {
        stage: HwStage::Cs,
        wave64,
        vgprs: 16,
        layout: vec![
            RootParam {
                root: RootType::Constants,
                table_offset_dwords: 0,
                sgprs: 4,
                base_register: 0,
                user_data: 0,
                slot: 0,
            },
            RootParam {
                root: RootType::Table,
                table_offset_dwords: 0,
                sgprs: 1,
                base_register: 0,
                user_data: 4,
                slot: 1,
            },
        ],
        used: vec![UsedResource {
            kind: ResourceType::Uav,
            root: RootType::Table,
            table_offset_dwords: 0,
            lower_bound: 0,
            stride: 0,
            dwords: 4,
            user_data: 4,
            slot: 1,
        }],
        io: StageIo::Cs(ComputeIo {
            rsrc2: 5 << 1, // 5 user SGPRs: s0..s4
            threads: [threads, 1, 1],
        }),
        code: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
    }
}

/// `program()` for a host of 32-lane waves, with the branches closing `wide` taken wave-wide.
fn lowered(native: &NativeProgram, p: &ir::Program, wide: &HashSet<usize>) -> Result<HostShader, ShaderError> {
    lowered_on(native, p, wide, 32)
}

/// [`lowered`] for a host whose waves have at most `host_wave` lanes.
fn lowered_on(
    native: &NativeProgram,
    p: &ir::Program,
    wide: &HashSet<usize>,
    host_wave: u32,
) -> Result<HostShader, ShaderError> {
    let st = structure::structure_wide(p, wide).expect("structure");
    let res = resource::track(p, &native.layout, native.stage, Some(&st)).expect("resource::track");
    let io = io::raise(p, native).expect("io::raise");
    let was = lower::TEST_HOST_WAVE.replace(Some(host_wave));
    let lowered = lower::lower(&lower::Parts {
        native,
        p,
        st: &st,
        io: &io,
        res: &res,
        inputs: Vec::new(),
        outputs: Vec::new(),
        prims: Vec::new(),
        mesh_stride: 0,
    });
    lower::TEST_HOST_WAVE.set(was);
    let lowered = lowered?;
    Ok(HostShader {
        stage: HwStage::Cs,
        dxil: lowered.dxil,
        user_data: lowered.user_data,
        samplers: lowered.samplers,
        static_samplers: Vec::new(),
        root_cbvs: lowered.root_cbvs,
        root_srvs: lowered.root_srvs,
        root_uavs: lowered.root_uavs,
        root_indexed_views: res.root_indexed_views,
    })
}

#[test]
fn a_sum_of_halves_over_the_wave_is_the_consoles_tree_bit_for_bit() {
    // thread t holds the 16-bit float with bits 0x3C00 + 37 t (1.0 and up, with odd
    // mantissas, so that additions round); all of them store the wave's sum
    let mut words = vec![
        0x340C_0085, // v_lshlrev_b32 v6, 5, v0
        0x340E_0082, // v_lshlrev_b32 v7, 2, v0
        0x4A0E_0F06, // v_add_nc_u32 v7, v6, v7
        0x4A0E_0F00, // v_add_nc_u32 v7, v0, v7
        0x4A0E_0EFF,
        0x0000_3C00, // v_add_nc_u32 v7, 0x3c00, v7
    ];
    // (the wave OR's words with v_add_f16 for v_or_b32: opcode 50 for 28)
    words.extend(super::wave::WAVE_OR_V7_TO_S7.iter().map(|&w| if w >> 25 == 28 { w & 0x01FF_FFFF | 50 << 25 } else { w }));
    words.extend_from_slice(&[
        0x7E00_0207, // v_mov_b32 v0, s7
        0x38 << 26 | 0x1C << 18,
        0x80 << 24 | 2 << 16, // buffer_store_dword v0, off, s[8:11], 0
    ]);
    let native = native(&words);
    let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
    assert_eq!(p.blocks.iter().flat_map(|b| &b.insts).filter(|i| i.name() == "v_add_f16").count(), 6);
    let shader = lowered(&native, &p, &HashSet::new()).expect("lower");
    if let Ok(mut v) = validator::Validator::load(validator::Validator::default_path().as_deref()) {
        let mut copy = shader.dxil.clone();
        v.validate(&mut copy).expect("DXIL validation failed");
    }
    // the console's sum: lanes 2k and 2k+1, then pairs, ... each addition rounded to 16 bits
    // (to nearest, ties to even); values here are normal numbers
    let wide = |h: u16| f32::from_bits((u32::from(h >> 10) + 112) << 23 | u32::from(h & 0x3FF) << 13);
    let narrow = |f: f32| {
        let b = f.to_bits();
        let h = ((b >> 23) - 112) << 10 | (b >> 13 & 0x3FF);
        let rest = b & 0x1FFF;
        (h + u32::from(rest > 0x1000 || rest == 0x1000 && h & 1 == 1)) as u16
    };
    let add = |a: u16, b: u16| narrow(wide(a) + wide(b));
    let terms: Vec<u16> = (0..64u16).map(|t| 0x3C00 + 37 * t).collect();
    let mut tree = terms.clone();
    while tree.len() > 1 {
        tree = tree.chunks(2).map(|c| add(c[0], c[1])).collect();
    }
    let in_a_row = terms.iter().copied().reduce(add).unwrap();
    assert_ne!(tree[0], in_a_row, "these terms tell the tree from another order");
    let _one = super::valu_tests::ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(host) = Host::new(false) else {
        eprintln!("no SM 6.6 hardware device; not run");
        return;
    };
    let (root, pso) = match host.compute_pipeline(&shader) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("the hardware device does not take this program (waves of 32?): {e}; not run");
            return;
        }
    };
    let ud = vec![0u32; shader.user_data_dwords().max(1) as usize];
    let out = host
        .dispatch(
            &root,
            &pso,
            &Dispatch {
                roots: vec![(0, Root::Constants(ud))],
                views: &[View::RawUav(1)],
                samplers: &[],
                groups: [1, 1, 1],
            },
        )
        .unwrap_or_else(|e| panic!("dispatch failed: {e}"));
    // (the high half is lane 0's term's: zero)
    assert_eq!(out[0][0], u32::from(tree[0]), "tree {:#x}, in a row {in_a_row:#x}", tree[0]);
    eprintln!("ran on the hardware device: the sum {:#x} of the console's tree", tree[0]);
}

/// The console's 16-bit wave sum where the lanes at or past s0 have left the program (a
/// thread past the end of the data): on the console they add +0, and after each step of the
/// tree they hold their run's partial sum all the same. On a host of 64-lane waves (AMD) the
/// lanes that are here read each partner run's sum from a lane of it that is here.
#[test]
fn a_sum_of_halves_where_lanes_have_left_is_the_consoles_tree_bit_for_bit() {
    let mut words = vec![
        0x7DA8_0000, // v_cmpx_gt_u32 exec, s0, v0: the lanes below s0 stay
        0, // s_cbranch_execz to the end (filled in below)
        0x340C_0085, // v_lshlrev_b32 v6, 5, v0
        0x340E_0082, // v_lshlrev_b32 v7, 2, v0
        0x4A0E_0F06, // v_add_nc_u32 v7, v6, v7
        0x4A0E_0F00, // v_add_nc_u32 v7, v0, v7
        0x4A0E_0EFF,
        0x0000_3C00, // v_add_nc_u32 v7, 0x3c00, v7
    ];
    words.extend(super::wave::WAVE_OR_V7_TO_S7.iter().map(|&w| if w >> 25 == 28 { w & 0x01FF_FFFF | 50 << 25 } else { w }));
    words.extend_from_slice(&[
        0x7E00_0207, // v_mov_b32 v0, s7
        0x38 << 26 | 0x1C << 18,
        0x80 << 24 | 2 << 16, // buffer_store_dword v0, off, s[8:11], 0
    ]);
    // past every word after it, to the program's end
    words[1] = 0xBF88_0000 | (words.len() as u32 - 2);
    let native = native(&words);
    let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
    let shader = lowered_on(&native, &p, &HashSet::new(), 64).expect("lower");
    if let Ok(mut v) = validator::Validator::load(validator::Validator::default_path().as_deref()) {
        let mut copy = shader.dxil.clone();
        v.validate(&mut copy).expect("DXIL validation failed");
    }
    let wide = |h: u16| {
        if h == 0 {
            0.0
        } else {
            f32::from_bits((u32::from(h >> 10) + 112) << 23 | u32::from(h & 0x3FF) << 13)
        }
    };
    let narrow = |f: f32| {
        if f == 0.0 {
            return 0;
        }
        let b = f.to_bits();
        let h = ((b >> 23) - 112) << 10 | (b >> 13 & 0x3FF);
        let rest = b & 0x1FFF;
        (h + u32::from(rest > 0x1000 || rest == 0x1000 && h & 1 == 1)) as u16
    };
    let add = |a: u16, b: u16| narrow(wide(a) + wide(b));
    let _one = super::valu_tests::ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let mut ran = false;
    for stay in [64u32, 63, 40, 33, 32, 17, 5, 1] {
        let terms: Vec<u16> = (0..64u32)
            .map(|t| if t < stay { (0x3C00 + 37 * t) as u16 } else { 0 })
            .collect();
        let mut tree = terms.clone();
        while tree.len() > 1 {
            tree = tree.chunks(2).map(|c| add(c[0], c[1])).collect();
        }
        let Some(got) = on_the_card(&shader, [stay, 0]) else {
            return;
        };
        ran = true;
        // (the high half is lane 0's term's: zero)
        assert_eq!(got, u32::from(tree[0]), "{stay} lanes stay: got {got:#x}, the console's tree {:#x}", tree[0]);
    }
    if ran {
        eprintln!("ran on the hardware device: the console's tree with lanes gone, in every case");
    }
}

/// One `v_add_f16` whose exact result lies between two halves: 1 + 3/1024 (0x3C03) plus 2
/// (0x4000) is 3 + 1.5 steps of that range, which the console rounds up to 0x4202. The
/// host's `f32tof16` may give 0x4201 (toward zero); with `VICTORY_F16_NEAREST=1` the
/// translation gives the console's whatever the host does.
#[test]
fn a_16_bit_sum_rounds_to_nearest_when_asked_to() {
    let words = [
        0x7E0E_02FF,
        0x0000_3C03, // v_mov_b32 v7, 0x3c03
        0x7E0C_02FF,
        0x0000_4000, // v_mov_b32 v6, 0x4000
        0x6400_0D07, // v_add_f16 v0, v7, v6
        0x38 << 26 | 0x1C << 18,
        0x80 << 24 | 2 << 16, // buffer_store_dword v0, off, s[8:11], 0
    ];
    let native = native(&words);
    let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
    assert!(p.blocks.iter().flat_map(|b| &b.insts).any(|i| i.name() == "v_add_f16"));
    let _one = super::valu_tests::ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    for nearest in [false, true] {
        let was = lower::TEST_F16_NEAREST.replace(Some(nearest));
        let shader = lowered(&native, &p, &HashSet::new());
        lower::TEST_F16_NEAREST.set(was);
        let shader = shader.expect("lower");
        if let Ok(mut v) = validator::Validator::load(validator::Validator::default_path().as_deref()) {
            let mut copy = shader.dxil.clone();
            v.validate(&mut copy).expect("DXIL validation failed");
        }
        let Ok(host) = Host::new(false) else {
            eprintln!("no SM 6.6 hardware device; not run");
            return;
        };
        let Ok((root, pso)) = host.compute_pipeline(&shader) else {
            eprintln!("the hardware device does not take this program; not run");
            return;
        };
        let ud = vec![0u32; shader.user_data_dwords().max(1) as usize];
        let out = host
            .dispatch(
                &root,
                &pso,
                &Dispatch {
                    roots: vec![(0, Root::Constants(ud))],
                    views: &[View::RawUav(1)],
                    samplers: &[],
                    groups: [1, 1, 1],
                },
            )
            .unwrap_or_else(|e| panic!("dispatch failed: {e}"));
        let got = out[0][0] & 0xFFFF;
        eprintln!("v_add_f16 0x3c03 + 0x4000 on the hardware device, nearest {nearest}: {got:#x}");
        if nearest {
            assert_eq!(got, 0x4202);
        } else {
            assert!(got == 0x4201 || got == 0x4202, "{got:#x}");
        }
    }
}

#[test]
fn a_wave_or_inside_a_lane_branch_needs_the_branch_wave_wide() {
    for (skip, name) in [(0xBF88_0000, "s_cbranch_execz"), (0xBF84_0000, "s_cbranch_scc0")] {
        a_wave_or_inside(skip, name);
    }
}

fn a_wave_or_inside(skip: u32, name: &str) {
    let native = program(skip);
    let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
    let branch = (0..p.blocks.len())
        .find(|&b| p.blocks[b].insts.last().is_some_and(|i| i.name() == name))
        .expect("the branch");
    // per lane, the two host waves cannot meet inside the branch
    lower::take_needs();
    let per_lane = lowered(&native, &p, &HashSet::new());
    assert!(
        matches!(&per_lane, Err(ShaderError::Unsupported { what, .. }) if *what == "a wave reduction where not every lane runs"),
        "{:?}",
        per_lane.err()
    );
    // ... and the lowering names the block, around which that branch is the one to widen
    let needs = lower::take_needs();
    let st = structure::structure(&p).expect("structure");
    let res = resource::track(&p, &native.layout, native.stage, Some(&st)).expect("resource::track");
    let io = io::raise(&p, &native).expect("io::raise");
    assert_eq!(lower::branches_around(&native, &p, &st, &io, &res, &needs), Some(vec![branch]));
    // (taken, past the body, when every lane's bit is clear)
    assert_eq!(st.wave_any[branch], Some(false));
    // wave-wide, every lane runs the reduction (EXEC stays a guard on what it does there)
    let wide = HashSet::from([branch]);
    let st = structure::structure_wide(&p, &wide).expect("structure");
    let inside = match p.blocks[branch].exit {
        ir::Exit::Branch { not_taken, .. } => not_taken,
        _ => unreachable!(),
    };
    assert!(st.guards[inside].iter().any(Option::is_some));
    let shader = lowered(&native, &p, &wide).expect("lower");
    if let Ok(mut v) = validator::Validator::load(validator::Validator::default_path().as_deref()) {
        let mut copy = shader.dxil.clone();
        v.validate(&mut copy).expect("DXIL validation failed");
    }
    // On a GPU that takes waves of 32 (two host waves for the console's one): the OR is of
    // both halves, whichever half's lanes take the branch.
    let _one = super::valu_tests::ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(host) = Host::new(false) else {
        eprintln!("no SM 6.6 hardware device; not run");
        return;
    };
    let (root, pso) = match host.compute_pipeline(&shader) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("the hardware device does not take this program (waves of 32?): {e}; not run");
            return;
        }
    };
    // (threads below s0 take the branch; thread t gives bit t >> 1)
    for (below, expected) in [(64u32, u32::MAX), (40, 0x000F_FFFF), (33, 0x0001_FFFF), (10, 0x1F), (1, 1)] {
        let mut ud = vec![0u32; shader.user_data_dwords().max(1) as usize];
        for u in &shader.user_data {
            if let (RootType::Constants, 0, 0) = (u.root, u.slot, u.dword) {
                ud[u.index as usize] = below;
            }
        }
        let out = host
            .dispatch(
                &root,
                &pso,
                &Dispatch {
                    roots: vec![(0, Root::Constants(ud))],
                    views: &[View::RawUav(1)],
                    samplers: &[],
                    groups: [1, 1, 1],
                },
            )
            .unwrap_or_else(|e| panic!("dispatch failed: {e}"));
        assert_eq!(out[0][0], expected, "{name}: {below} threads in the branch");
    }
    eprintln!("ran on the hardware device ({name}): the OR of both halves in every case");
}

/// Runs `shader` on the hardware device with the root constants `s0`, `s1` and gives dword 0
/// of the UAV; None (and a line) without a device that takes it.
fn on_the_card(shader: &HostShader, constants: [u32; 2]) -> Option<u32> {
    let Ok(host) = Host::new(false) else {
        eprintln!("no SM 6.6 hardware device; not run");
        return None;
    };
    let Ok((root, pso)) = host.compute_pipeline(shader) else {
        eprintln!("the hardware device does not take this program; not run");
        return None;
    };
    let mut ud = vec![0u32; shader.user_data_dwords().max(1) as usize];
    for u in &shader.user_data {
        if let (RootType::Constants, 0, d @ 0..=1) = (u.root, u.slot, u.dword) {
            ud[u.index as usize] = constants[d as usize];
        }
    }
    let out = host
        .dispatch(
            &root,
            &pso,
            &Dispatch {
                roots: vec![(0, Root::Constants(ud))],
                views: &[View::RawUav(1)],
                samplers: &[],
                groups: [1, 1, 1],
            },
        )
        .unwrap_or_else(|e| panic!("dispatch failed: {e}"));
    Some(out[0][0])
}

/// A 32-lane console wave. A register holds a lane mask on one path (the lanes below s0)
/// and a number on the other, and is read as a number after the join, as the trace programs
/// keep their lanes' masks and their node numbers on one stack
/// (`deferred_tiled:CS_LaserLightShadowRaytracing`, s46 at 0x104, 0x75c and 0x188). The
/// mask arrives as the ballot of its lanes.
#[test]
fn a_lane_mask_joined_with_a_number_is_read_as_the_number_it_is() {
    let words = [
        0x7D88_0000, // v_cmp_gt_u32 vcc_lo, s0, v0
        0xBE94_036A, // s_mov_b32 s20, vcc_lo
        0xBF06_8001, // s_cmp_eq_u32 s1, 0
        0xBF85_0001, // s_cbranch_scc1 over the next
        0xBE94_0385, // s_mov_b32 s20, 5
        0x7E00_0214, // v_mov_b32 v0, s20
        0x38 << 26 | 0x1C << 18,
        0x80 << 24 | 2 << 16, // buffer_store_dword v0, off, s[8:11], 0
    ];
    let native = native_of(&words, false, 32);
    let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
    let shader = lowered(&native, &p, &HashSet::new()).expect("lower");
    if let Ok(mut v) = validator::Validator::load(validator::Validator::default_path().as_deref()) {
        let mut copy = shader.dxil.clone();
        v.validate(&mut copy).expect("DXIL validation failed");
    }
    let _one = super::valu_tests::ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    for (below, other, expected) in [(32u32, 0u32, u32::MAX), (10, 0, 0x3FF), (0, 0, 0), (10, 1, 5)] {
        let Some(got) = on_the_card(&shader, [below, other]) else { return };
        assert_eq!(got, expected, "{below} lanes in the mask, s1 = {other}");
    }
    eprintln!("ran on the hardware device: the mask's bits, or the number");
}

/// A 32-lane console wave. `if (t < s0) { ... SCC = (s1 == 7) }` and a branch on SCC after
/// the join: the skip brings the SCC of `s_and_saveexec` ("some lane is left") along
/// (`rtdirectionalshadows:CS_RTShadowTrace32HQ_TiledFarOnly`, 0x360-0x39c). On the console
/// the skip is taken by the wave, so with some lane inside every lane sees the inner test.
/// A lane that skipped on its own would see "some lane is left" instead: refused per lane,
/// right with the branch taken wave-wide. Lane 31 stores what it saw.
#[test]
fn scc_of_a_mask_operation_joined_with_a_scalar_test_needs_the_skip_wave_wide() {
    let words = [
        0x7D88_0000, // v_cmp_gt_u32 vcc_lo, s0, v0
        0xBE8D_3C6A, // s_and_saveexec_b32 s13, vcc_lo
        0xBF88_0001, // s_cbranch_execz over the next
        0xBF06_8701, // s_cmp_eq_u32 s1, 7
        0xBEFE_030D, // s_mov_b32 exec_lo, s13
        0xBF85_0002, // s_cbranch_scc1 to "seven"
        0x7E0A_0281, // v_mov_b32 v5, 1
        0xBF82_0001, // s_branch over the next
        0x7E0A_0282, // seven: v_mov_b32 v5, 2
        0x7DA4_009F, // v_cmpx_eq_u32 exec_lo, 31, v0
        0x38 << 26 | 0x1C << 18,
        0x80 << 24 | 2 << 16 | 5 << 8, // buffer_store_dword v5, off, s[8:11], 0
    ];
    let native = native_of(&words, false, 32);
    let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
    let branch = (0..p.blocks.len())
        .find(|&b| p.blocks[b].insts.last().is_some_and(|i| i.name() == "s_cbranch_execz"))
        .expect("the skip");
    lower::take_needs();
    let per_lane = lowered(&native, &p, &HashSet::new());
    assert!(
        matches!(&per_lane, Err(ShaderError::Unsupported { what, .. }) if *what == "a lane mask read as data"),
        "{:?}",
        per_lane.err()
    );
    let st = structure::structure(&p).expect("structure");
    let res = resource::track(&p, &native.layout, native.stage, Some(&st)).expect("resource::track");
    let io = io::raise(&p, &native).expect("io::raise");
    assert_eq!(lower::branches_around(&native, &p, &st, &io, &res, &lower::take_needs()), Some(vec![branch]));
    let shader = lowered(&native, &p, &HashSet::from([branch])).expect("lower");
    if let Ok(mut v) = validator::Validator::load(validator::Validator::default_path().as_deref()) {
        let mut copy = shader.dxil.clone();
        v.validate(&mut copy).expect("DXIL validation failed");
    }
    let _one = super::valu_tests::ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    // (lanes below s0 inside, s1) -> what lane 31 stores: 2 if SCC was set at the branch
    for (below, s1, expected) in [(0u32, 7u32, 1u32), (10, 7, 2), (10, 3, 1), (32, 3, 1), (32, 7, 2)] {
        let Some(got) = on_the_card(&shader, [below, s1]) else { return };
        assert_eq!(got, expected, "{below} lanes inside, s1 = {s1}");
    }
    eprintln!("ran on the hardware device: the inner test's SCC for every lane once some lane is inside");
}

/// Wave64 compute groups whose size is not a multiple of 32 (e.g. 48 = 32 + 16,
/// and 16 < 32) lowered to 32-lane host waves: every console thread index
/// `0..threads` must execute exactly once per group, with no skipped threads,
/// duplicate wraps, or out-of-bounds indices.
#[test]
fn partial_group_wave64_on_wave32_assigns_every_thread_once() {
    let words = [
        // ds_swizzle_b32 v2, v0 offset:0x80E4 (triggers console_lanes -> halves)
        (0x36 << 26) | (0x35 << 18) | 0x80E4,
        2 << 24,
        0x3402_0082, // v_lshlrev_b32 v1, 2, v0 (byte offset = thread_id.x * 4)
        0x7E06_0281, // v_mov_b32 v3, 1
        // buffer_atomic_add v3, v1, s[8:11], 0 offen
        (0x38 << 26) | (0x32 << 18) | (1 << 12),
        (0x80 << 24) | (2 << 16) | (3 << 8) | 1,
    ];
    let _one = super::valu_tests::ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    for threads in [16u32, 48u32] {
        let native = native_of(&words, true, threads);
        let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
        let shader = lowered(&native, &p, &HashSet::new()).expect("lower");
        if let Ok(mut v) =
            validator::Validator::load(validator::Validator::default_path().as_deref())
        {
            let mut copy = shader.dxil.clone();
            v.validate(&mut copy)
                .unwrap_or_else(|e| panic!("DXIL validation failed for {threads} threads: {e}"));
        }
        let Ok(host) = Host::new(false) else {
            continue;
        };
        let Ok((root, pso)) = host.compute_pipeline(&shader) else {
            continue;
        };
        let groups = 64u32;
        let ud = vec![0u32; shader.user_data_dwords().max(1) as usize];
        let out = host
            .dispatch(
                &root,
                &pso,
                &Dispatch {
                    roots: vec![(0, Root::Constants(ud))],
                    views: &[View::RawUav(64)],
                    samplers: &[],
                    groups: [groups, 1, 1],
                },
            )
            .unwrap_or_else(|e| panic!("dispatch failed ({threads} threads): {e}"));
        for slot in 0..threads as usize {
            assert_eq!(
                out[0][slot], groups,
                "threads={threads}: slot {slot} got {} (expected {groups})",
                out[0][slot]
            );
        }
        for slot in threads as usize..64 {
            assert_eq!(
                out[0][slot], 0,
                "threads={threads}: out-of-range slot {slot} was written ({})",
                out[0][slot]
            );
        }
    }
}

/// A count over the wave behind a branch on a float condition no thread differs in: the code
/// LLVM's AMD back end makes of `if (k > 0.5) n = <the wave's lanes above 40>` with `k` a root
/// constant. There is no scalar float compare, so the condition is a vector compare into an
/// SGPR pair, and each lane takes the branch by its own bit. All take it alike: the count's
/// block is one every thread of the group reaches, and the two host waves meet there
/// ([`lower::group_bits`]). The same branch on a compare of the thread's own number does part
/// the lanes, and stops as before.
#[test]
fn a_branch_on_a_compare_of_root_constants_parts_no_lanes() {
    let program = |compare: [u32; 2]| {
        let mut words = compare.to_vec();
        words.extend_from_slice(&[
            0x87EA_047E, // s_and_b64 vcc, exec, s[4:5]
            0xBF87_0003, // s_cbranch_vccnz past the count
            0x3602_00BF, // v_and_b32 v1, 63, v0
            0x7D82_02A8, // v_cmp_lt_u32 vcc, 40, v1
            0xBE81_106A, // s_bcnt1_i32_b64 s1, vcc
            0x7E00_0201, // v_mov_b32 v0, s1
            0x38 << 26 | 0x1C << 18,
            0x80 << 24 | 2 << 16, // buffer_store_dword v0, off, s[8:11], 0
        ]);
        native(&words)
    };
    let native = program([0xD40B_0004, 0x0201_E000]); // v_cmp_ngt_f32 s[4:5], s0, 0.5
    let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
    let names: Vec<&str> = p.blocks.iter().flat_map(|b| &b.insts).map(|i| i.name()).collect();
    assert!(names.contains(&"v_cmp_ngt_f32") && names.contains(&"s_bcnt1_i32_b64"), "{names:?}");
    let shader = lowered(&native, &p, &HashSet::new()).expect("lower");
    if let Ok(mut v) = validator::Validator::load(validator::Validator::default_path().as_deref()) {
        let mut copy = shader.dxil.clone();
        v.validate(&mut copy).expect("DXIL validation failed");
    }
    let own = program([0xD40B_0004, 0x0201_E100]); // v_cmp_ngt_f32 s[4:5], v0, 0.5
    let q = ir::build(&own.code, own.wave64, own.vgprs).expect("ir::build");
    lower::take_needs();
    let stopped = lowered(&own, &q, &HashSet::new());
    lower::take_needs();
    assert!(
        matches!(&stopped, Err(ShaderError::Unsupported { what, .. }) if lower::needs_every_lane(what)),
        "{:?}",
        stopped.err()
    );
    let _one = super::valu_tests::ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    // (s0 as a float, s1) -> what every lane stores: the lanes above 40 if s0 > 0.5, else s1
    for (level, s1, expected) in [(1.0f32, 7u32, 23u32), (0.25, 7, 7), (0.75, 9, 23)] {
        let Some(got) = on_the_card(&shader, [level.to_bits(), s1]) else { return };
        assert_eq!(got, expected, "s0 = {level}, s1 = {s1}");
    }
    eprintln!("ran on the hardware device: one count over the console wave behind a branch all lanes take alike");
}

/// Whether a form of the program needs the two host waves to meet where not every lane runs.
fn stops(native: &NativeProgram) -> bool {
    let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
    lower::take_needs();
    let r = lowered(native, &p, &HashSet::new());
    lower::take_needs();
    match r {
        Ok(shader) => {
            if let Ok(mut v) = validator::Validator::load(validator::Validator::default_path().as_deref()) {
                let mut copy = shader.dxil.clone();
                v.validate(&mut copy).expect("DXIL validation failed");
            }
            false
        }
        Err(ShaderError::Unsupported { what, .. }) if lower::needs_every_lane(what) => true,
        Err(e) => panic!("stopped for another reason: {e:?}"),
    }
}

/// `v_mov_b32 v0, s<n>` and the store of v0.
fn store_sgpr(n: u32) -> [u32; 3] {
    [0x7E00_0200 | n, 0x38 << 26 | 0x1C << 18, 0x80 << 24 | 2 << 16]
}

#[test]
fn a_loop_lanes_leave_at_different_times_is_no_place_to_meet() {
    let after = |body: &[u32]| {
        let mut words = vec![0x7E02_0280]; // v_mov_b32 v1, 0
        words.extend_from_slice(body);
        words.push(0xBEFE_04C1); // s_mov_b64 exec, -1
        words.extend(store_sgpr(6));
        native(&words)
    };
    // One block, closed by the branch the lanes leave by.
    let own = [
        0x4A02_0281, // v_add_nc_u32 v1, 1, v1
        0x7D82_0101, // v_cmp_lt_u32 vcc, v1, v0
        0xBE86_106A, // s_bcnt1_i32_b64 s6, vcc
        0x87FE_6A7E, // s_and_b64 exec, exec, vcc
        0xBF89_FFFB, // s_cbranch_execnz to the top
    ];
    assert!(stops(&after(&own)), "a count in the block that closes the loop");
    // The test and the count at the top, the way back from the bottom.
    let top = [
        0x7D82_0101, // v_cmp_lt_u32 vcc, v1, v0
        0xBE86_106A, // s_bcnt1_i32_b64 s6, vcc
        0x87FE_6A7E, // s_and_b64 exec, exec, vcc
        0xBF88_0002, // s_cbranch_execz past the loop
        0x4A02_0281, // v_add_nc_u32 v1, 1, v1
        0xBF82_FFFA, // s_branch to the top
    ];
    assert!(stops(&after(&top)), "a count in the block that opens the loop");
    // A loop all lanes leave together, round a branch that parts them: they have met again
    // by the block that closes it.
    let together = [
        0xBE86_0380, // s_mov_b32 s6, 0
        0xBE87_0380, // s_mov_b32 s7, 0
        // the top
        0x7D82_02A8, // v_cmp_lt_u32 vcc, 40, v1
        0xBE8C_246A, // s_and_saveexec_b64 s[12:13], vcc
        0xBF88_0001, // s_cbranch_execz
        0x4A02_0281, // v_add_nc_u32 v1, 1, v1
        0xBEFE_040C, // s_mov_b64 exec, s[12:13]
        0x7D82_0101, // v_cmp_lt_u32 vcc, v1, v0
        0xBE86_106A, // s_bcnt1_i32_b64 s6, vcc
        0x8007_8107, // s_add_u32 s7, s7, 1
        0xBF0A_8407, // s_cmp_lt_u32 s7, 4
        0xBF85_FFF6, // s_cbranch_scc1 to the top
    ];
    assert!(!stops(&after(&together)), "a count where the lanes have met again");
}

#[test]
fn several_console_waves_meet_only_behind_what_their_group_shares() {
    // The count behind a scalar branch, of threads above 40 in their wave.
    let count = [
        0x3602_00BF, // v_and_b32 v1, 63, v0
        0x7D82_02A8, // v_cmp_lt_u32 vcc, 40, v1
        0xBE82_106A, // s_bcnt1_i32_b64 s2, vcc
    ];
    let behind = |condition: &[u32], threads: u32| {
        let mut words = condition.to_vec();
        words.push(0xBF84_0003); // s_cbranch_scc0 past the count
        words.extend(count);
        words.extend(store_sgpr(2));
        native_of(&words, true, threads)
    };
    // The wave's own count of something: two waves of a group may go different ways.
    let wave = [
        0x7D88_0000, // v_cmp_gt_u32 vcc, s0, v0
        0xBE81_106A, // s_bcnt1_i32_b64 s1, vcc
        0xBF08_8801, // s_cmp_gt_u32 s1, 8
    ];
    assert!(stops(&behind(&wave, 128)), "a wave's own count decides");
    assert!(!stops(&behind(&wave, 64)), "one wave is the group");
    let constant = [0xBF08_8800]; // s_cmp_gt_u32 s0, 8
    assert!(!stops(&behind(&constant, 128)), "a root constant decides");
    // A scalar counter, joined by a phi at the loop's top.
    let mut counted = vec![0xBE86_0380]; // s_mov_b32 s6, 0
    counted.extend(count);
    counted.extend([
        0x8006_8106, // s_add_u32 s6, s6, 1
        0xBF0A_8406, // s_cmp_lt_u32 s6, 4
        0xBF85_FFFA, // s_cbranch_scc1 to the count
    ]);
    counted.extend(store_sgpr(2));
    assert!(!stops(&native_of(&counted, true, 128)), "a scalar counter decides");
}

#[test]
fn what_a_group_shares_is_not_what_a_wave_or_a_moment_gives() {
    // `load` into s6, `v_mov_b32 v1, s6`, `v_cmp_ne_u32 vcc, 5, v1`: whether the group shares
    // that bit.
    let shared = |load: &[u32]| {
        let mut words = load.to_vec();
        words.extend([0x7E02_0206, 0x7D8A_0285]);
        words.extend([0x7E04_0280, 0x0204_0281]); // v2 = 0, then 1 where VCC is set
        let native = native(&words);
        let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
        let st = structure::structure_wide(&p, &HashSet::new()).expect("structure");
        let res = resource::track(&p, &native.layout, native.stage, Some(&st)).expect("track");
        let io = io::raise(&p, &native).expect("io::raise");
        let bit = p
            .blocks
            .iter()
            .flat_map(|b| &b.insts)
            .find(|i| i.name() == "v_cmp_ne_u32")
            .expect("the compare")
            .writes[0]
            .1;
        lower::group_bits(&p, &st, &io, &res).contains(&bit)
    };
    let smem = |op: u32, base: u32, more: u32| [0x3D << 26 | op << 18 | 6 << 6 | base | more, 125 << 25];
    assert!(shared(&smem(8, 4, 0)), "s_buffer_load_dword");
    assert!(!shared(&smem(8, 4, 1 << 16)), "s_buffer_load_dword glc");
    let mut refreshed = smem(0x20, 0, 0).to_vec();
    refreshed[0] &= !(0x7F << 6);
    refreshed.extend(smem(8, 4, 0));
    assert!(!shared(&refreshed), "s_buffer_load_dword in a program with s_dcache_inv");
    assert!(!shared(&smem(0x24, 0, 0)), "s_memtime");
    assert!(!shared(&smem(0x2A, 0, 0)), "s_get_waveid_in_workgroup");
}

#[test]
fn a_read_after_a_cache_invalidation_is_a_read_of_its_own() {
    // The same dword twice, with what `between` is between, and the sum stored.
    let reads = |between: &[u32]| {
        let load = |v: u32| [0x38 << 26 | 0x0C << 18, 0x80 << 24 | 2 << 16 | v << 8];
        let mut words = load(1).to_vec();
        words.extend_from_slice(between);
        words.extend(load(2));
        words.push(0x4A00_0501); // v_add_nc_u32 v0, v1, v2
        words.extend([0x38 << 26 | 0x1C << 18, 0x80 << 24 | 2 << 16]);
        let native = native(&words);
        let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
        let shader = lowered(&native, &p, &HashSet::new()).expect("lower");
        let census = super::census::take(&shader.dxil).expect("counted");
        let count = |kind: &str| {
            let at = super::census::KINDS.iter().position(|&k| k == kind).unwrap();
            census.counts[at]
        };
        if let Ok(mut v) = validator::Validator::load(validator::Validator::default_path().as_deref()) {
            let mut copy = shader.dxil.clone();
            v.validate(&mut copy).expect("DXIL validation failed");
        }
        (count("buffer_load"), count("barrier"))
    };
    assert_eq!(reads(&[]), (1, 0), "nothing between: one read serves both");
    assert_eq!(reads(&[0xE1C4_0000, 0]), (2, 1), "buffer_gl0_inv");
    assert_eq!(reads(&[0xE1C8_0000, 0]), (2, 1), "buffer_gl1_inv");
}

/// A wave OR (so that the two host waves exchange), then `body` behind a branch each lane takes
/// by its own number (`t < s0`), in a group that writes each thread's number to its dword of
/// shared memory first. `meet`: the group's barrier between the writes and the branch.
fn behind_a_lane_branch(meet: bool, body: &[u32]) -> NativeProgram {
    let mut words = vec![
        0x3412_0082, // v_lshlrev_b32 v9, 2, v0
        0xD834_0000,
        0x0000_0009, // ds_write_b32 v9, v0
    ];
    if meet {
        words.push(0xBF8A_0000); // s_barrier
    }
    words.push(0x7E0E_0300); // v_mov_b32 v7, v0
    words.extend_from_slice(&super::wave::WAVE_OR_V7_TO_S7);
    words.extend_from_slice(&[
        0x7D88_0000,                     // v_cmp_gt_u32 vcc, s0, v0
        0xBE8C_246A,                     // s_and_saveexec_b64 s[12:13], vcc
        0xBF88_0000 | body.len() as u32, // s_cbranch_execz past the body
    ]);
    words.extend_from_slice(body);
    words.push(0xBEFE_040C); // s_mov_b64 exec, s[12:13]
    let mut native = native(&words);
    let StageIo::Cs(cs) = &mut native.io else { unreachable!() };
    cs.rsrc2 |= 1 << 15; // 512 bytes of shared memory
    native
}

/// `v8 = shared[s1]`, the first lane's of it, stored to dword 0 of the UAV.
const FIRST_LANE_OF_A_SHARED_READ: [u32; 7] = [
    0x7E0A_0201, // v_mov_b32 v5, s1
    0xD8D8_0000,
    0x0800_0005, // ds_read_b32 v8, v5
    0x7E0C_0508, // v_readfirstlane_b32 s6, v8
    0x7E00_0206, // v_mov_b32 v0, s6
    0x38 << 26 | 0x1C << 18,
    0x80 << 24 | 2 << 16, // buffer_store_dword v0, off, s[8:11], 0
];

/// Where lanes have parted, the two host waves cannot meet to ask for the console's first lane.
/// They need not when every lane reads the same dword of shared memory: the group meets round a
/// branch that only reads it, also where the program itself has no barrier.
#[test]
fn the_first_lane_of_one_shared_dword_needs_no_meeting() {
    for meet in [true, false] {
        let native = behind_a_lane_branch(meet, &FIRST_LANE_OF_A_SHARED_READ);
        let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
        let shader = lowered(&native, &p, &HashSet::new()).expect("lower");
        if let Ok(mut v) = validator::Validator::load(validator::Validator::default_path().as_deref()) {
            let mut copy = shader.dxil.clone();
            v.validate(&mut copy).expect("DXIL validation failed");
        }
        let _one = super::valu_tests::ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        for (below, thread) in [(50u32, 40u32), (20, 40), (64, 3), (1, 63)] {
            let Some(got) = on_the_card(&shader, [below, thread * 4]) else { return };
            assert_eq!(got, thread, "barrier {meet}: the threads below {below} read thread {thread}'s dword");
        }
    }
}

/// Whether the first-lane read of `native` is one the two host waves would have to meet for.
fn first_lane_refused(native: &NativeProgram) -> bool {
    let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
    lower::take_needs();
    let r = lowered(native, &p, &HashSet::new());
    lower::take_needs();
    match r {
        Ok(_) => false,
        Err(ShaderError::Unsupported { what, .. }) if lower::needs_every_lane(what) => true,
        Err(e) => panic!("stopped for another reason: {e:?}"),
    }
}

#[test]
fn each_lanes_own_shared_dword_is_not_one_value() {
    let mut own = FIRST_LANE_OF_A_SHARED_READ.to_vec();
    own[0] = 0x7E0A_0309; // v_mov_b32 v5, v9
    assert!(first_lane_refused(&behind_a_lane_branch(true, &own)));
}

/// The structure builder's scans: 512 threads, and each wave sums what earlier waves left in
/// shared memory in a loop that it leaves with all its lanes (the test is on the wave's number),
/// reading the running total's first lane inside. Other waves have left the loop by then, so the
/// group cannot meet there; the total is one value over the wave.
#[test]
fn a_total_carried_round_a_loop_the_wave_leaves_together_is_one_value() {
    let mut words = vec![
        0x3412_0082, // v_lshlrev_b32 v9, 2, v0
        0xD834_0000,
        0x0000_0009, // ds_write_b32 v9, v0
        0xBF8A_0000, // s_barrier
        0x7E0E_0300, // v_mov_b32 v7, v0
    ];
    words.extend_from_slice(&super::wave::WAVE_OR_V7_TO_S7);
    words.extend_from_slice(&[
        0x2C14_0086, // v_lshrrev_b32 v10, 6, v0
        0x7E10_0280, // v_mov_b32 v8, 0
        0xBE86_0380, // s_mov_b32 s6, 0
        0xBE8E_0380, // s_mov_b32 s14, 0
        // the top
        0x7D82_1406, // v_cmp_lt_u32 vcc, s6, v10
        0xBF86_0008, // s_cbranch_vccz past the loop
        0x8F07_8206, // s_lshl_b32 s7, s6, 2
        0x7E0A_0207, // v_mov_b32 v5, s7
        0xD8D8_0000,
        0x0B00_0005, // ds_read_b32 v11, v5
        0x4A10_110B, // v_add_nc_u32 v8, v11, v8
        0x7E1C_0508, // v_readfirstlane_b32 s14, v8
        0x8006_8106, // s_add_u32 s6, s6, 1
        0xBF82_FFF6, // s_branch to the top
        // past it
        0x7DA4_0001, // v_cmpx_eq_u32 exec, s1, v0
        0x7E00_020E, // v_mov_b32 v0, s14
        0x38 << 26 | 0x1C << 18,
        0x80 << 24 | 2 << 16, // buffer_store_dword v0, off, s[8:11], 0
    ]);
    // (ds_read's result register and the add's source: v11)
    let at = words.iter().position(|&w| w == 0x4A10_110B).expect("the add");
    words[at] = 0x4A10_1000 | 0x10B;
    let mut native = native_of(&words, true, 512);
    let StageIo::Cs(cs) = &mut native.io else { unreachable!() };
    cs.rsrc2 |= 4 << 15; // 2 KiB of shared memory
    let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
    let shader = lowered(&native, &p, &HashSet::new()).expect("lower");
    if let Ok(mut v) = validator::Validator::load(validator::Validator::default_path().as_deref()) {
        let mut copy = shader.dxil.clone();
        v.validate(&mut copy).expect("DXIL validation failed");
    }
    let _one = super::valu_tests::ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    for thread in [0u32, 40, 64, 127, 200, 300, 511] {
        let Some(got) = on_the_card(&shader, [0, thread]) else { return };
        let wave = thread / 64;
        assert_eq!(got, (0..wave).sum::<u32>(), "thread {thread}: the numbers below {wave}");
    }
    eprintln!("ran on the hardware device: a total over the waves below, from a loop");
}

/// A value made of scalars in a loop each lane leaves in its own round is each lane's last
/// round's behind it.
#[test]
fn what_lanes_take_out_of_a_loop_in_different_rounds_is_not_one_value() {
    let after = |leave: u32| {
        let mut words = vec![0x7E0E_0300]; // v_mov_b32 v7, v0
        words.extend_from_slice(&super::wave::WAVE_OR_V7_TO_S7);
        words.extend_from_slice(&[
            0xBE86_0380, // s_mov_b32 s6, 0
            // the top
            0x7E10_0206, // v_mov_b32 v8, s6
            leave,
            0xBF86_0002, // s_cbranch_vccz past the loop
            0x8006_8106, // s_add_u32 s6, s6, 1
            0xBF82_FFFB, // s_branch to the top
            // past it, behind a branch each lane takes by its own number
            0x7D88_0000, // v_cmp_gt_u32 vcc, s0, v0
            0xBE8C_246A, // s_and_saveexec_b64 s[12:13], vcc
            0xBF88_0005, // s_cbranch_execz
            0x7E1C_0508, // v_readfirstlane_b32 s14, v8
            0x7E00_020E, // v_mov_b32 v0, s14
            0x38 << 26 | 0x1C << 18,
            0x80 << 24 | 2 << 16, // buffer_store_dword v0, off, s[8:11], 0
            0xBF80_0000, // s_nop
            0xBEFE_040C, // s_mov_b64 exec, s[12:13]
        ]);
        native(&words)
    };
    // v_cmp_lt_u32 vcc, s6, v0: by the lane's own number
    assert!(first_lane_refused(&after(0x7D82_0006)));
}

/// What an instruction reads without naming it makes its result each lane's own too: the
/// accumulator of `v_fmac_f32`, the VCC of `v_cndmask_b32`.
#[test]
fn values_made_of_implicit_reads_are_each_lanes_own() {
    let refused = |make: &[u32]| {
        let mut body = make.to_vec();
        body.extend_from_slice(&[
            0x7E0C_0508, // v_readfirstlane_b32 s6, v8
            0x7E00_0206, // v_mov_b32 v0, s6
            0x38 << 26 | 0x1C << 18,
            0x80 << 24 | 2 << 16, // buffer_store_dword v0, off, s[8:11], 0
        ]);
        let native = behind_a_lane_branch(true, &body);
        let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
        lower::take_needs();
        let r = lowered(&native, &p, &HashSet::new());
        lower::take_needs();
        match r {
            Ok(_) => false,
            Err(ShaderError::Unsupported { what, .. }) if lower::needs_every_lane(what) => true,
            Err(e) => panic!("stopped for another reason: {e:?}"),
        }
    };
    assert!(!refused(&[0x7E10_02F4]), "v_mov_b32 v8, 2.0 is one value");
    assert!(
        refused(&[
            0x7E10_0D00, // v_cvt_f32_u32 v8, v0
            0x7E12_02F4, // v_mov_b32 v9, 2.0
            0x5610_12F2, // v_fmac_f32 v8, 1.0, v9
        ]),
        "v_fmac_f32 adds to each lane's own"
    );
    assert!(
        refused(&[
            0x7E12_02F4, // v_mov_b32 v9, 2.0
            0x7D82_00A0, // v_cmp_lt_u32 vcc, 32, v0
            0x0210_12F2, // v_cndmask_b32 v8, 1.0, v9, vcc
        ]),
        "v_cndmask_b32 picks by each lane's own bit"
    );
}
