//! End-to-end tests of image stores: synthetic GFX10 programs lowered to
//! DXIL, validated by `dxil.dll` when present, and run on the host GPU and
//! WARP, the stored texels read back.

use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use super::host::{Dispatch, Host, Root, View};
use super::{HostShader, lower, validator};
use crate::native::{
    ComputeIo, HwStage, NativeProgram, ResourceType, RootParam, RootType, StageIo, UsedResource,
};
use crate::{io, ir, resource, structure};

const S_ENDPGM: u32 = 0xBF81_0000;

/// [`build_with`] for an image view at heap index 0, its descriptor in
/// `s[4:11]` (`v1`: 0).
fn build(body: &[u32]) -> Result<HostShader, &'static str> {
    let load = [
        0xF40C_0100,
        0xFA00_0000, // s_load_dwordx8 s[4:11], s[0:1], null
        0x7E02_0280, // v_mov_b32 v1, 0
    ];
    build_with(
        &[&load, body].concat(),
        vec![used(ResourceType::Uav, 0, 0, 8)],
    )
}

/// What the program reads of the table at s0: a view `entry` heap slots in.
pub(super) fn used(kind: ResourceType, entry: u32, stride: u32, dwords: u8) -> UsedResource {
    UsedResource {
        kind,
        root: RootType::Table,
        table_offset_dwords: 8 * entry,
        lower_bound: 0,
        stride,
        dwords,
        user_data: 0,
        slot: 0,
    }
}

/// Lowers a wave32 compute program of 32 threads (`v0`: the thread,
/// `s[0:1]`: the table of views, heap index 0 on); or why it is refused.
pub(super) fn build_with(
    body: &[u32],
    used: Vec<UsedResource>,
) -> Result<HostShader, &'static str> {
    let mut words = vec![
        0xB001_0000, // s_movk_i32 s1, 0
    ];
    words.extend_from_slice(body);
    words.push(S_ENDPGM);
    let native = NativeProgram {
        stage: HwStage::Cs,
        wave64: false,
        vgprs: 32,
        layout: vec![RootParam {
            root: RootType::Table,
            table_offset_dwords: 0,
            sgprs: 1,
            base_register: 0,
            user_data: 0,
            slot: 0,
        }],
        used,
        io: StageIo::Cs(ComputeIo {
            rsrc2: 1 << 1,
            threads: [32, 1, 1],
        }),
        code: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
    };
    let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
    let st = structure::structure(&p).expect("structure");
    let res = resource::track(&p, &native.layout, native.stage, Some(&st)).expect("track");
    let io = io::raise(&p, &native).expect("io::raise");
    let lowered = lower::lower(&lower::Parts {
        native: &native,
        p: &p,
        st: &st,
        io: &io,
        res: &res,
        inputs: Vec::new(),
        outputs: Vec::new(),
        prims: Vec::new(),
        mesh_stride: 0,
    })
    .map_err(|e| match e {
        crate::ShaderError::Unsupported { what, .. } => what,
        e => panic!("{e}"),
    })?;
    let shader = HostShader {
        stage: HwStage::Cs,
        dxil: lowered.dxil,
        user_data: lowered.user_data,
        samplers: lowered.samplers,
        static_samplers: Vec::new(),
        root_cbvs: lowered.root_cbvs,
        root_srvs: lowered.root_srvs,
        root_uavs: lowered.root_uavs,
        root_indexed_views: Vec::new(),
    };
    if let Ok(mut v) = validator::Validator::load(validator::Validator::default_path().as_deref()) {
        v.validate(&mut shader.dxil.clone())
            .expect("DXIL validation failed");
    } else {
        eprintln!("no dxil.dll; validation skipped");
    }
    Ok(shader)
}

/// The dwords of the 32 texels the program leaves in a 32x1 texture.
fn run(shader: &HostShader, format: DXGI_FORMAT, texel: u32) -> Vec<(&'static str, Vec<u32>)> {
    run_with(shader, &[View::FormatUav(32, 1, format, texel)])
}

/// What the program leaves in the first unordered access view of `views`.
pub(super) fn run_with(shader: &HostShader, views: &[View]) -> Vec<(&'static str, Vec<u32>)> {
    let mut out = Vec::new();
    let _one = super::valu_tests::ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    for (label, warp) in [("hardware", false), ("warp", true)] {
        let Ok(host) = Host::new(warp) else {
            eprintln!("no SM 6.6 {label} device; skipped");
            continue;
        };
        let (root, pso) = host
            .compute_pipeline(shader)
            .unwrap_or_else(|e| panic!("{label}: {e}"));
        let ud = vec![0u32; shader.user_data_dwords().max(1) as usize];
        let words = host
            .dispatch(
                &root,
                &pso,
                &Dispatch {
                    roots: vec![(0, Root::Constants(ud))],
                    views,
                    samplers: &[] as &[D3D12_SAMPLER_DESC],
                    groups: [1, 1, 1],
                },
            )
            .unwrap_or_else(|e| panic!("{label}: {e}"));
        out.push((label, words[0].clone()));
    }
    out
}

/// What each lane stores: integers whose bits are no ordinary floats (a
/// denormal and a signalling NaN among them).
const VALUES: &[u32] = &[
    0x4A04_00FF,
    0x8000_0001, // v_add_nc_u32 v2, 0x80000001, v0
    0x7E06_02A2, // v_mov_b32 v3, 34
    0x7E08_02FF,
    0x7FA0_0001, // v_mov_b32 v4, 0x7fa00001
    0x7E0A_0281, // v_mov_b32 v5, 1
];

/// The `image_store` of `v2..` at `v0, v1` with a data mask (and 16-bit data).
fn image_store(dmask: u32, d16: bool) -> [u32; 2] {
    [
        0xF020_1008 | dmask << 8,
        0x0001_0200 | if d16 { 0x8000_0000 } else { 0 },
    ]
}

/// Runs [`VALUES`] stored with `dmask` in a four-channel dword format; every
/// lane's texel must be `want` of its thread.
fn stores(dmask: u32, format: DXGI_FORMAT, want: impl Fn(u32) -> [u32; 4]) {
    let shader = build(&[VALUES, &image_store(dmask, false)[..]].concat()).expect("translates");
    for (label, words) in run(&shader, format, 16) {
        let want: Vec<u32> = (0..32).flat_map(&want).collect();
        assert_eq!(words, want, "{label}, mask {dmask:#x}");
    }
}

#[test]
fn integer_formats_get_the_stored_bits() {
    // The stores are emitted as floats whatever the view's format is, which
    // D3D12 does not define for integer views; the adapters tested keep the
    // bits. This says so if one does not.
    for format in [DXGI_FORMAT_R32G32B32A32_UINT, DXGI_FORMAT_R32G32B32A32_SINT] {
        stores(0xF, format, |t| [0x8000_0001 + t, 34, 0x7FA0_0001, 1]);
    }
}

#[test]
fn channels_the_mask_leaves_out_are_stored_as_zero() {
    // The data of the channels named comes from consecutive registers; LLVM
    // drops trailing zeros of a store from its mask on this GPU. Every
    // texel is overwritten with ones first, so zeros are stored ones.
    let ones = [
        0x7E14_02C1, // v_mov_b32 v10, -1
        0x7E16_02C1, // v_mov_b32 v11, -1
        0x7E18_02C1, // v_mov_b32 v12, -1
        0x7E1A_02C1, // v_mov_b32 v13, -1
        0xF020_1F08,
        0x0001_0A00, // image_store v[10:13], v[0:1], s[4:11] dmask:0xf
    ];
    for dmask in 1..15u32 {
        let body = [VALUES, &ones, &image_store(dmask, false)].concat();
        let shader = build(&body).unwrap_or_else(|e| panic!("mask {dmask:#x}: {e}"));
        for (label, words) in run(&shader, DXGI_FORMAT_R32G32B32A32_UINT, 16) {
            let want: Vec<u32> = (0..32u32)
                .flat_map(|t| {
                    let mut data = [0x8000_0001 + t, 34, 0x7FA0_0001, 1].into_iter();
                    let mut texel = [0; 4];
                    for (ch, x) in texel.iter_mut().enumerate() {
                        if dmask >> ch & 1 != 0 {
                            *x = data.next().unwrap();
                        }
                    }
                    texel
                })
                .collect();
            assert_eq!(words, want, "{label}, mask {dmask:#x}");
        }
    }
}

#[test]
fn half_channels_the_mask_leaves_out_are_stored_as_zero() {
    // v2: 1.0 and 2.0, v3: 0.5 and -1.0, as 16-bit floats, low half first.
    let halves = [
        0x7E04_02FF,
        0x4000_3C00, // v_mov_b32 v2, 0x40003c00
        0x7E06_02FF,
        0xBC00_3800, // v_mov_b32 v3, 0xbc003800
    ];
    for count in 1..=4usize {
        let body = [&halves[..], &image_store((1 << count) - 1, true)].concat();
        let shader = build(&body).unwrap_or_else(|e| panic!("{count} channels: {e}"));
        for (label, words) in run(&shader, DXGI_FORMAT_R32G32B32A32_FLOAT, 16) {
            let mut texel = [0.0f32; 4];
            texel[..count].copy_from_slice(&[1.0, 2.0, 0.5, -1.0][..count]);
            let want = texel.map(f32::to_bits).repeat(32);
            assert_eq!(words, want, "{label}, {count} channels");
        }
    }
    // With 16-bit data only the mask's bit count is said to matter; a mask
    // that is not its low bits is left refused.
    assert_eq!(
        build(&[&halves[..], &image_store(0b1010, true)].concat()).err(),
        Some("a 16-bit image store of some channels")
    );
}
