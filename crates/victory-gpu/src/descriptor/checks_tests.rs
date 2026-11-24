use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;

use super::*;

fn texture(
    dimension: D3D12_RESOURCE_DIMENSION,
    format: i32,
    layers: u16,
    mips: u16,
    samples: u32,
) -> D3D12_RESOURCE_DESC {
    D3D12_RESOURCE_DESC {
        Dimension: dimension,
        Width: 64,
        Height: 64,
        DepthOrArraySize: layers,
        MipLevels: mips,
        Format: DXGI_FORMAT(format),
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: samples,
            Quality: 0,
        },
        ..Default::default()
    }
}
const D2: D3D12_RESOURCE_DIMENSION = D3D12_RESOURCE_DIMENSION_TEXTURE2D;

fn committed(d: &D3D12_RESOURCE_DESC) -> ID3D12Resource {
    let host = crate::host::host().unwrap();
    let heap = D3D12_HEAP_PROPERTIES {
        Type: D3D12_HEAP_TYPE_DEFAULT,
        ..Default::default()
    };
    let mut made: Option<ID3D12Resource> = None;
    // SAFETY: plain creation.
    unsafe {
        host.device
            .CreateCommittedResource(
                &heap,
                D3D12_HEAP_FLAG_NONE,
                d,
                D3D12_RESOURCE_STATE_COMMON,
                None,
                &mut made,
            )
            .unwrap();
    }
    made.unwrap()
}

fn alive() -> bool {
    // SAFETY: a plain device query.
    unsafe { crate::host::host().unwrap().device.GetDeviceRemovedReason() }.is_ok()
}

#[test]
fn constant_views_stay_in_what_the_host_takes() {
    let bytes = |gpu, size| {
        let d = constant_desc(gpu, size);
        (d.BufferLocation, d.SizeInBytes)
    };
    assert_eq!(bytes(0x1000, 4), (0x1000, 256));
    assert_eq!(bytes(0x1000, 65_536), (0x1000, 65_536));
    assert_eq!(bytes(0x1000, 4 << 20), (0x1000, 65_536));
    assert_eq!(bytes(0x1000, 0), (0, 0));
    assert_eq!(bytes(0, 256), (0, 0));
}

#[test]
fn casts_stay_inside_a_family_and_a_kind_of_number() {
    assert!(rule(28, 29) && rule(29, 28) && rule(98, 99) && rule(28, 30));
    assert!(rule(42, 43) && !rule(41, 42) && !rule(28, 31));
    assert!(!rule(28, 87), "another family");
    assert!(!rule(27, 28) && !rule(40, 41) && !rule(95, 96));
}

#[test]
fn depth_views_name_the_colour_format_and_the_plane() {
    let d = texture(D2, 44, 1, 1, 1);
    for (asked, format, plane) in [(45, 46, 0), (46, 46, 0), (47, 47, 1)] {
        let mut v = whole_shader_view(&d, asked);
        v.Anonymous.Texture2D = D3D12_TEX2D_SRV {
            MipLevels: 1,
            PlaneSlice: 7,
            ..Default::default()
        };
        let mut v = Some(v);
        assert!(name_shader_format(&d, &mut v));
        let v = v.unwrap();
        // SAFETY: a 2D view.
        let got = unsafe { v.Anonymous.Texture2D.PlaneSlice };
        assert_eq!((v.Format.0, got), (format, plane));
    }
    // Without a description: one of the whole texture.
    let mut none = None;
    assert!(name_shader_format(&texture(D2, 39, 4, 3, 1), &mut none));
    let v = none.unwrap();
    assert_eq!(
        (v.Format.0, v.ViewDimension),
        (41, D3D12_SRV_DIMENSION_TEXTURE2DARRAY)
    );
    // Of a typed texture nothing changes.
    let mut none = None;
    assert!(name_shader_format(&texture(D2, 28, 1, 1, 1), &mut none) && none.is_none());
}

#[test]
fn views_are_cut_to_the_texture_or_refused() {
    let d = texture(D2, 28, 2, 4, 1);
    let view = |first, n| D3D12_SHADER_RESOURCE_VIEW_DESC {
        ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
        Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
            Texture2D: D3D12_TEX2D_SRV {
                MostDetailedMip: first,
                MipLevels: n,
                ..Default::default()
            },
        },
        ..Default::default()
    };
    for (first, n, left) in [
        (0, u32::MAX, Some(4)),
        (3, 9, Some(1)),
        (4, 1, None),
        (0, 0, None),
    ] {
        let mut v = view(first, n);
        let ok = shader_view_fits(&d, &mut v);
        // SAFETY: a 2D view.
        assert_eq!(ok.then_some(unsafe { v.Anonymous.Texture2D.MipLevels }), left);
    }
    // Another dimension, samples, too few layers for a cube.
    let mut v = view(0, 1);
    v.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE3D;
    assert!(!shader_view_fits(&d, &mut v));
    assert!(!shader_view_fits(&texture(D2, 28, 1, 1, 4), &mut view(0, 1)));
    v.ViewDimension = D3D12_SRV_DIMENSION_TEXTURECUBE;
    assert!(!shader_view_fits(&d, &mut v));
    assert!(shader_view_fits(&texture(D2, 28, 6, 4, 1), &mut v));
}

/// A target's own view has the target's dimension, samples and layers, on
/// the real device too.
#[test]
fn whole_targets_have_the_targets_dimension() {
    let words = |layers, samples, depth| {
        let (typeless, format) = if depth { (44, 45) } else { (27, 29) };
        whole_target(&texture(D2, typeless, layers, 1, samples), format, depth)
    };
    assert_eq!(words(1, 1, false), [29, 4, 0, 0, 0, 0]);
    assert_eq!(words(3, 1, false), [29, 5, 0, 0, 3, 0]);
    assert_eq!(words(1, 4, true), [45, 5, 0, 0, 0, 0]);
    assert_eq!(words(3, 1, true), [45, 4, 0, 0, 0, 3]);
    let Some(host) = crate::host::host() else {
        return; // no host GPU
    };
    for (samples, layers) in [(1, 1), (1, 3), (4, 1), (4, 3)] {
        let mut d = texture(D2, 27, layers, 1, samples);
        d.Flags = D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET;
        let target = committed(&d);
        let words = whole_target(&d, 28, false);
        // SAFETY: six words in the view description's layout.
        let view = unsafe {
            (&raw const words)
                .cast::<D3D12_RENDER_TARGET_VIEW_DESC>()
                .read_unaligned()
        };
        host.rtv(Some(&target), Some(&view));
        assert!(alive(), "target: {samples} samples, {layers} layers");

        let mut d = texture(D2, 44, layers, 1, samples);
        d.Flags = D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL;
        let depth = committed(&d);
        let words = whole_target(&d, 45, true);
        // SAFETY: as above.
        let view = unsafe {
            (&raw const words)
                .cast::<D3D12_DEPTH_STENCIL_VIEW_DESC>()
                .read_unaligned()
        };
        host.dsv(&depth, Some(&view));
        assert!(alive(), "depth: {samples} samples, {layers} layers");
    }
}

/// Views the rules let through are views the device takes: a typed texture
/// in its sRGB and integer forms, and typed buffers in every format said to
/// have them.
#[test]
fn what_is_let_through_the_device_takes() {
    let Some(host) = crate::host::host() else {
        return; // no host GPU
    };
    // SAFETY: a heap of our own.
    let views: ID3D12DescriptorHeap = unsafe {
        host.device
            .CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                NumDescriptors: 1,
                ..Default::default()
            })
            .unwrap()
    };
    // SAFETY: a live heap.
    let at = unsafe { views.GetCPUDescriptorHandleForHeapStart() };
    let d = texture(D2, 28, 1, 1, 1);
    let typed = committed(&d);
    for format in (0..128).filter(|&f| castable(28, f)) {
        let desc = whole_shader_view(&d, format);
        // SAFETY: a live texture and a slot of our heap.
        unsafe {
            host.device
                .CreateShaderResourceView(&typed, Some(&desc), at)
        };
        assert!(alive(), "28 as {format}");
    }
    let mut bytes = texture(D3D12_RESOURCE_DIMENSION_BUFFER, 0, 1, 1, 1);
    (bytes.Width, bytes.Height) = (4096, 1);
    bytes.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    bytes.Flags = D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
    let buffer = committed(&bytes);
    for format in (1..200).map(DXGI_FORMAT) {
        if typed_buffer(format, 4, false) {
            let desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
                Format: format,
                ViewDimension: D3D12_SRV_DIMENSION_BUFFER,
                Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
                Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                    Buffer: D3D12_BUFFER_SRV {
                        NumElements: 4,
                        ..Default::default()
                    },
                },
            };
            // SAFETY: a live buffer and a slot of our heap.
            unsafe {
                host.device
                    .CreateShaderResourceView(&buffer, Some(&desc), at)
            };
        }
        if typed_buffer(format, 4, true) {
            let desc = D3D12_UNORDERED_ACCESS_VIEW_DESC {
                Format: format,
                ViewDimension: D3D12_UAV_DIMENSION_BUFFER,
                Anonymous: D3D12_UNORDERED_ACCESS_VIEW_DESC_0 {
                    Buffer: D3D12_BUFFER_UAV {
                        NumElements: 4,
                        ..Default::default()
                    },
                },
            };
            // SAFETY: as above.
            unsafe {
                host.device.CreateUnorderedAccessView(
                    &buffer,
                    None::<&ID3D12Resource>,
                    Some(&desc),
                    at,
                )
            };
        }
        assert!(alive(), "a buffer as {}", format.0);
    }
    assert!(typed_buffer(DXGI_FORMAT_R32_UINT, 4, true));
    assert!(!typed_buffer(DXGI_FORMAT_D32_FLOAT, 4, false));
    assert!(!typed_buffer(DXGI_FORMAT_R32_UINT, (1 << 27) + 1, false));
}
