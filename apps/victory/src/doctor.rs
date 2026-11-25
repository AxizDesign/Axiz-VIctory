//! `victory doctor`: what this PC has of what a run needs. Reads only; loads
//! no title.

use anyhow::{Result, bail};
use windows::Win32::Foundation::LUID;
use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_12_0;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

/// System DLLs a run uses, and where each comes from. They are looked for in
/// System32 only, as `victory-hle` loads them.
const DLLS: &[(&str, &str)] = &[
    ("d3d12.dll", "Windows 10 or later"),
    ("dxgi.dll", "Windows"),
    ("d3dcompiler_47.dll", "Windows"),
    ("ucrtbase.dll", "Windows"),
    ("xinput1_4.dll", "Windows"),
    ("xaudio2_9.dll", "Windows 10 or later"),
    ("mfplat.dll", "Windows (N editions: install the Media Feature Pack)"),
    ("mfreadwrite.dll", "Windows (N editions: install the Media Feature Pack)"),
    ("vcruntime140.dll", VC_RUNTIME),
    ("vcruntime140_1.dll", VC_RUNTIME),
    ("msvcp140.dll", VC_RUNTIME),
];

const VC_RUNTIME: &str =
    "the Visual C++ x64 runtime: winget install Microsoft.VCRedist.2015+.x64";

/// Commit a run was seen to use is about 24 GB; below this it likely fails.
const COMMIT_GIB: u64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    /// Nothing is drawn, or the run does not start.
    Fail,
    /// Some things may not work.
    Warn,
}

/// What an adapter's device says of itself.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Gpu {
    shader_model: (u32, u32),
    binding_tier: i32,
    mesh: bool,
    wave_lanes: Option<(u32, u32)>,
    int64: bool,
    rovs: bool,
}

/// What an adapter lacks for a run.
fn lacks(gpu: &Gpu) -> Vec<(Level, String)> {
    let mut out = Vec::new();
    let mut add = |level, text: String| out.push((level, text));
    let (major, minor) = gpu.shader_model;
    if gpu.shader_model < (6, 6) {
        add(
            Level::Fail,
            format!(
                "shader model {major}.{minor}, 6.6 is needed: update the graphics driver and \
                 Windows (Windows 10's own Direct3D may stop below 6.6)"
            ),
        );
    }
    if gpu.binding_tier < 3 {
        add(
            Level::Fail,
            format!("resource binding tier {}, 3 is needed", gpu.binding_tier),
        );
    }
    if !gpu.mesh {
        add(
            Level::Fail,
            "no mesh shaders: menus may show, the world is not drawn".into(),
        );
    }
    match gpu.wave_lanes {
        None => add(Level::Fail, "no wave operations".into()),
        // The translator has forms for 64 lanes and for a widest wave of 32.
        Some((_, 32 | 64)) => {}
        Some((min, max)) => add(
            Level::Warn,
            format!("waves of {min} to {max} lanes: some compute programs are not translated"),
        ),
    }
    if !gpu.int64 {
        add(Level::Warn, "no 64-bit integers in shaders".into());
    }
    if !gpu.rovs {
        add(Level::Warn, "no rasterizer-ordered views".into());
    }
    out
}

pub fn run() -> Result<()> {
    let mut failed = false;
    println!("System");
    if std::is_x86_feature_detected!("avx2") {
        println!("  ok    CPU has AVX2");
    } else {
        println!("  FAIL  this CPU has no AVX2");
        failed = true;
    }
    match commit_gib() {
        Some(free) if free < COMMIT_GIB => println!(
            "  WARN  {free} GiB of commit free, about {COMMIT_GIB} or more is wanted: close \
             other programs or enlarge the paging file"
        ),
        Some(free) => println!("  ok    {free} GiB of commit free"),
        None => {}
    }
    let system = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let system = std::path::Path::new(&system).join("System32");
    for (dll, source) in DLLS {
        if !system.join(dll).is_file() {
            println!("  FAIL  {dll} is missing; it comes with {source}");
            failed = true;
        }
    }
    if cfg!(not(feature = "diagnostics")) {
        println!(
            "  note  this build ignores VICTORY_SHADER_LOOP_LIMIT (only builds with the \
             diagnostics feature read it; start.ps1 makes one)"
        );
    }

    failed |= !adapters()?;

    println!("Folders");
    let cfg = victory_core::Config::load()?;
    for (what, folder) in [
        ("cache", cfg.cache_subdir("shaders")),
        ("data", cfg.data_subdir("")),
    ] {
        match folder {
            Ok(path) => println!("  ok    {what}: {}", path.display()),
            Err(e) => {
                println!("  FAIL  {what}: {e:#}");
                failed = true;
            }
        }
    }

    if failed {
        bail!("this PC lacks something a run needs (the FAIL lines above)");
    }
    println!("Nothing needed is missing.");
    Ok(())
}

/// Lists the adapters; whether the one Windows gives by default (the one
/// a run uses when none is chosen) has what is needed.
fn adapters() -> Result<bool> {
    println!("Graphics");
    let default = device(None).ok().map(|d| {
        // SAFETY: a plain query.
        unsafe { d.GetAdapterLuid() }
    });
    let same = |a: LUID, b: LUID| (a.LowPart, a.HighPart) == (b.LowPart, b.HighPart);
    let (mut default_good, mut other_good) = (false, None);
    // SAFETY: plain DXGI enumeration.
    unsafe {
        let factory: IDXGIFactory6 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
        for i in 0.. {
            let Ok(adapter) = factory.EnumAdapterByGpuPreference::<IDXGIAdapter1>(
                i,
                DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
            ) else {
                break;
            };
            let d = adapter.GetDesc1()?;
            if d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let end = d.Description.iter().position(|&c| c == 0);
            let name =
                String::from_utf16_lossy(&d.Description[..end.unwrap_or(d.Description.len())]);
            let is_default = default.is_some_and(|l| same(l, d.AdapterLuid));
            println!(
                "  {name}, {} MiB{}",
                d.DedicatedVideoMemory >> 20,
                if is_default { " (Windows' default)" } else { "" }
            );
            let device = match device(Some(&adapter)) {
                Ok(device) => device,
                Err(e) => {
                    println!("    FAIL  no Direct3D 12 device at feature level 12.0: {}", e.message());
                    continue;
                }
            };
            let lacks = lacks(&read(&device));
            for (level, text) in &lacks {
                let level = if *level == Level::Fail { "FAIL" } else { "WARN" };
                println!("    {level}  {text}");
            }
            if lacks.iter().all(|(level, _)| *level != Level::Fail) {
                println!("    ok    can draw");
                if is_default {
                    default_good = true;
                } else {
                    other_good.get_or_insert(name);
                }
            }
        }
    }
    if let (false, Some(name)) = (default_good, &other_good) {
        println!(
            "  FAIL  Windows' default is not {name}, which can draw: choose High performance for \
             victory.exe in Settings > System > Display > Graphics"
        );
    } else if !default_good {
        println!("  FAIL  no graphics card here can draw");
    }
    Ok(default_good)
}

fn device(adapter: Option<&IDXGIAdapter1>) -> windows::core::Result<ID3D12Device> {
    let mut device: Option<ID3D12Device> = None;
    // SAFETY: device creation on a live adapter, or the default one.
    unsafe {
        match adapter {
            Some(a) => D3D12CreateDevice(a, D3D_FEATURE_LEVEL_12_0, &mut device)?,
            None => D3D12CreateDevice(None, D3D_FEATURE_LEVEL_12_0, &mut device)?,
        }
    }
    device.ok_or_else(windows::core::Error::empty)
}

/// One feature structure of `device`; `None` where the query fails.
fn feature<T: Default>(device: &ID3D12Device, which: D3D12_FEATURE) -> Option<T> {
    let mut data = T::default();
    // SAFETY: each caller names the feature whose structure `T` is.
    unsafe { device.CheckFeatureSupport(which, (&raw mut data).cast(), size_of::<T>() as u32) }
        .ok()
        .map(|()| data)
}

fn read(device: &ID3D12Device) -> Gpu {
    let base: D3D12_FEATURE_DATA_D3D12_OPTIONS =
        feature(device, D3D12_FEATURE_D3D12_OPTIONS).unwrap_or_default();
    let waves: D3D12_FEATURE_DATA_D3D12_OPTIONS1 =
        feature(device, D3D12_FEATURE_D3D12_OPTIONS1).unwrap_or_default();
    let mesh: D3D12_FEATURE_DATA_D3D12_OPTIONS7 =
        feature(device, D3D12_FEATURE_D3D12_OPTIONS7).unwrap_or_default();
    Gpu {
        shader_model: shader_model(device),
        binding_tier: base.ResourceBindingTier.0,
        mesh: mesh.MeshShaderTier != D3D12_MESH_SHADER_TIER_NOT_SUPPORTED,
        wave_lanes: waves
            .WaveOps
            .as_bool()
            .then_some((waves.WaveLaneCountMin, waves.WaveLaneCountMax)),
        int64: waves.Int64ShaderOps.as_bool(),
        rovs: base.ROVsSupported.as_bool(),
    }
}

/// The highest shader model `device` takes, as (major, minor).
fn shader_model(device: &ID3D12Device) -> (u32, u32) {
    // The query fails for a model the runtime does not know: ask downwards.
    for minor in (0..=9).rev() {
        let mut data = D3D12_FEATURE_DATA_SHADER_MODEL {
            HighestShaderModel: D3D_SHADER_MODEL(0x60 + minor),
        };
        // SAFETY: a feature query with its own structure.
        let asked = unsafe {
            device.CheckFeatureSupport(
                D3D12_FEATURE_SHADER_MODEL,
                (&raw mut data).cast(),
                size_of_val(&data) as u32,
            )
        };
        if asked.is_ok() {
            let model = data.HighestShaderModel.0 as u32;
            return (model >> 4, model & 0xf);
        }
    }
    (5, 1)
}

/// Commit still free, in GiB.
fn commit_gib() -> Option<u64> {
    let mut status = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: the structure's length is set.
    unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
    Some(status.ullAvailPageFile >> 30)
}

#[cfg(test)]
mod tests {
    use super::{Gpu, Level, lacks};

    fn good() -> Gpu {
        Gpu {
            shader_model: (6, 6),
            binding_tier: 3,
            mesh: true,
            wave_lanes: Some((32, 32)),
            int64: true,
            rovs: true,
        }
    }

    #[test]
    fn a_card_with_everything_lacks_nothing() {
        assert_eq!(lacks(&good()), []);
        let amd = Gpu {
            wave_lanes: Some((32, 64)),
            shader_model: (6, 8),
            ..good()
        };
        assert_eq!(lacks(&amd), []);
    }

    #[test]
    fn what_stops_drawing_fails() {
        for gpu in [
            Gpu { shader_model: (6, 5), ..good() },
            Gpu { shader_model: (5, 1), ..good() },
            Gpu { binding_tier: 2, ..good() },
            Gpu { mesh: false, ..good() },
            Gpu { wave_lanes: None, ..good() },
        ] {
            let lacks = lacks(&gpu);
            assert_eq!(lacks.len(), 1, "{gpu:?}");
            assert_eq!(lacks[0].0, Level::Fail, "{gpu:?}");
        }
    }

    #[test]
    fn what_may_still_draw_warns() {
        for gpu in [
            Gpu { wave_lanes: Some((8, 16)), ..good() },
            Gpu { int64: false, ..good() },
            Gpu { rovs: false, ..good() },
        ] {
            let lacks = lacks(&gpu);
            assert_eq!(lacks.len(), 1, "{gpu:?}");
            assert_eq!(lacks[0].0, Level::Warn, "{gpu:?}");
        }
    }
}
