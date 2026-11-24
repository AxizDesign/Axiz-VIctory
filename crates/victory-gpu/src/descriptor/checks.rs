//! What the host accepts in a view, checked before one is made: view
//! creation returns nothing, and a description the host refuses removes the
//! device. The console is laxer, so views are brought into range where that
//! keeps their meaning and refused (left null) where it does not.

use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use crate::resource::{SRGB, family};

/// A constant buffer view holds at most 4,096 vectors of 16 bytes.
const CONSTANT_MOST: u32 = D3D12_REQ_CONSTANT_BUFFER_ELEMENT_COUNT * 16;
/// A typed buffer view holds at most 2^27 elements.
const TEXELS_MOST: u32 = 1 << D3D12_REQ_BUFFER_RESOURCE_TEXEL_COUNT_2_TO_EXP;

/// The bytes of a title constant buffer of `size` bytes that a view shows.
pub(crate) fn constant_bytes(size: u32) -> u32 {
    size.min(CONSTANT_MOST)
}

/// A constant buffer view of `size` bytes at `gpu`, in whole 256 bytes; the
/// null view of no bytes or no address.
pub(super) fn constant_desc(gpu: u64, size: u32) -> D3D12_CONSTANT_BUFFER_VIEW_DESC {
    if gpu == 0 || size == 0 {
        return D3D12_CONSTANT_BUFFER_VIEW_DESC::default();
    }
    D3D12_CONSTANT_BUFFER_VIEW_DESC {
        BufferLocation: gpu,
        SizeInBytes: constant_bytes(size).next_multiple_of(256),
    }
}

/// Whether the host has typed buffer views (`unordered`: unordered access
/// ones) of `count` elements in `format`.
pub(super) fn typed_buffer(format: DXGI_FORMAT, count: u32, unordered: bool) -> bool {
    // The console's own formats, some of whose numbers the PC gave to others.
    if count > TEXELS_MOST || matches!(format.0, 116..=120 | 189..) || format.0 <= 0 {
        return false;
    }
    let mut needed = D3D12_FORMAT_SUPPORT1_BUFFER.0;
    if unordered {
        needed |= D3D12_FORMAT_SUPPORT1_TYPED_UNORDERED_ACCESS_VIEW.0;
    }
    support(format) & needed == needed
}

/// The host's `D3D12_FORMAT_SUPPORT1` bits for `format`, asked once each.
fn support(format: DXGI_FORMAT) -> i32 {
    use std::sync::OnceLock;
    static ASKED: [OnceLock<i32>; 256] = [const { OnceLock::new() }; 256];
    let Some(asked) = usize::try_from(format.0).ok().and_then(|f| ASKED.get(f)) else {
        return 0;
    };
    *asked.get_or_init(|| {
        let Some(host) = crate::host::host() else {
            return 0;
        };
        let mut support = D3D12_FEATURE_DATA_FORMAT_SUPPORT {
            Format: format,
            ..Default::default()
        };
        // SAFETY: a plain query into our own structure.
        let known = unsafe {
            host.device.CheckFeatureSupport(
                D3D12_FEATURE_FORMAT_SUPPORT,
                (&raw mut support).cast(),
                size_of_val(&support) as u32,
            )
        };
        if known.is_ok() { support.Support1.0 } else { 0 }
    })
}

/// Whether the host views a typed texture in another format of its family
/// (`CastingFullyTypedFormatSupported`).
fn casts() -> bool {
    static CASTS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CASTS.get_or_init(|| {
        let Some(host) = crate::host::host() else {
            return false;
        };
        let mut options = D3D12_FEATURE_DATA_D3D12_OPTIONS3::default();
        // SAFETY: a plain query into our own structure.
        let known = unsafe {
            host.device.CheckFeatureSupport(
                D3D12_FEATURE_D3D12_OPTIONS3,
                (&raw mut options).cast(),
                size_of_val(&options) as u32,
            )
        };
        let casts = known.is_ok() && options.CastingFullyTypedFormatSupported.as_bool();
        tracing::info!(casts, "views of typed textures in other formats of their family");
        casts
    })
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Class {
    Float,
    Unorm,
    Snorm,
    Integer,
}

/// What a format's numbers are; `None` for typeless formats and those whose
/// casts are not used here (depth, shared exponent, half-float blocks).
fn class(format: u32) -> Option<Class> {
    Some(match format {
        2 | 6 | 10 | 16 | 34 | 41 | 54 => Class::Float,
        11 | 24 | 28 | 29 | 35 | 49 | 56 | 61 | 71 | 72 | 74 | 75 | 77 | 78 | 80 | 83 | 87 | 88
        | 91 | 93 | 98 | 99 => Class::Unorm,
        13 | 31 | 37 | 51 | 58 | 63 | 81 | 84 => Class::Snorm,
        3 | 4 | 7 | 8 | 12 | 14 | 17 | 18 | 25 | 30 | 32 | 36 | 38 | 42 | 43 | 50 | 52 | 57
        | 59 | 62 | 64 => Class::Integer,
        _ => return None,
    })
}

/// Whether a typed texture of format `own` may be viewed as `view`: the same
/// family, not between normalised signed and unsigned numbers, nor between
/// floats and others.
pub(super) fn castable(own: u32, view: u32) -> bool {
    rule(own, view) && casts()
}

fn rule(own: u32, view: u32) -> bool {
    let (Some(a), Some(b)) = (class(own), class(view)) else {
        return false;
    };
    family(own) == family(view)
        && (a == Class::Float) == (b == Class::Float)
        && !matches!((a, b), (Class::Unorm, Class::Snorm) | (Class::Snorm, Class::Unorm))
}

/// Whether the host format `own` is typeless (every family that has one has
/// it first and at least one format after it).
fn typeless(own: u32) -> bool {
    family(own) == own && family(own + 1) == own
}

/// The format in which shaders read a typeless texture of family `own` that
/// the title views in `format` (0: names none), and the plane it is in.
/// Depth formats are not shader formats on the host, the colour format of
/// the same bits is.
fn shader_format(own: u32, format: u32) -> Option<(u32, Option<u32>)> {
    Some(match (own, format) {
        (39, 0 | 39 | 40) => (41, None),
        (53, 0 | 53 | 55) => (56, None),
        (19, 0 | 19 | 20 | 21) => (21, Some(0)),
        (44, 0 | 44 | 45 | 46) => (46, Some(0)),
        (19, 22) => (22, Some(1)),
        (44, 47) => (47, Some(1)),
        // Typeless on the host for an sRGB format of the title's
        // (`crate::resource`): with no description, that one.
        (_, 0) => (SRGB.iter().find(|&&(srgb, _)| family(srgb) == own)?.0, None),
        // Another typeless name is no view format.
        (_, f) if typeless(f) => return None,
        (_, f) => (f, None),
    })
}

/// How many of `n` levels (or slices) from `first` a texture of `count` has
/// (-1: the rest); `None` if none.
fn within(first: u32, n: u32, count: u32) -> Option<u32> {
    (first < count && n > 0).then(|| n.min(count - first))
}

/// A whole-texture shader view of `d` in `format`.
fn whole_shader_view(d: &D3D12_RESOURCE_DESC, format: u32) -> D3D12_SHADER_RESOURCE_VIEW_DESC {
    use D3D12_SHADER_RESOURCE_VIEW_DESC_0 as Union;
    let layers = u32::from(d.DepthOrArraySize);
    let (mips, ms) = (u32::from(d.MipLevels), d.SampleDesc.Count > 1);
    let (dimension, view) = match (d.Dimension, ms, layers > 1) {
        (D3D12_RESOURCE_DIMENSION_TEXTURE1D, _, false) => (
            D3D12_SRV_DIMENSION_TEXTURE1D,
            Union {
                Texture1D: D3D12_TEX1D_SRV {
                    MipLevels: mips,
                    ..Default::default()
                },
            },
        ),
        (D3D12_RESOURCE_DIMENSION_TEXTURE1D, _, true) => (
            D3D12_SRV_DIMENSION_TEXTURE1DARRAY,
            Union {
                Texture1DArray: D3D12_TEX1D_ARRAY_SRV {
                    MipLevels: mips,
                    ArraySize: layers,
                    ..Default::default()
                },
            },
        ),
        (D3D12_RESOURCE_DIMENSION_TEXTURE3D, ..) => (
            D3D12_SRV_DIMENSION_TEXTURE3D,
            Union {
                Texture3D: D3D12_TEX3D_SRV {
                    MipLevels: mips,
                    ..Default::default()
                },
            },
        ),
        (_, true, false) => (D3D12_SRV_DIMENSION_TEXTURE2DMS, Union::default()),
        (_, true, true) => (
            D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY,
            Union {
                Texture2DMSArray: D3D12_TEX2DMS_ARRAY_SRV {
                    FirstArraySlice: 0,
                    ArraySize: layers,
                },
            },
        ),
        (_, false, false) => (
            D3D12_SRV_DIMENSION_TEXTURE2D,
            Union {
                Texture2D: D3D12_TEX2D_SRV {
                    MipLevels: mips,
                    ..Default::default()
                },
            },
        ),
        (_, false, true) => (
            D3D12_SRV_DIMENSION_TEXTURE2DARRAY,
            Union {
                Texture2DArray: D3D12_TEX2D_ARRAY_SRV {
                    MipLevels: mips,
                    ArraySize: layers,
                    ..Default::default()
                },
            },
        ),
    };
    D3D12_SHADER_RESOURCE_VIEW_DESC {
        Format: DXGI_FORMAT(format as i32),
        ViewDimension: dimension,
        Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
        Anonymous: view,
    }
}

/// Spells out the format (and plane) of a shader view of a typeless texture;
/// a view without a description gets one. False: the host has no such view.
pub(super) fn name_shader_format(
    d: &D3D12_RESOURCE_DESC,
    desc: &mut Option<D3D12_SHADER_RESOURCE_VIEW_DESC>,
) -> bool {
    let own = d.Format.0 as u32;
    if !typeless(own) {
        return true;
    }
    let asked = desc.as_ref().map_or(0, |v| v.Format.0 as u32);
    if asked != 0 && family(asked) != own {
        return true; // (`fits` refuses such a view)
    }
    let Some((format, plane)) = shader_format(own, asked) else {
        return false;
    };
    let view = desc.get_or_insert_with(|| whole_shader_view(d, format));
    view.Format = DXGI_FORMAT(format as i32);
    if let Some(plane) = plane {
        match view.ViewDimension {
            D3D12_SRV_DIMENSION_TEXTURE2D => {
                // SAFETY: the member the dimension names.
                let mut t = unsafe { view.Anonymous.Texture2D };
                t.PlaneSlice = plane;
                view.Anonymous.Texture2D = t;
            }
            D3D12_SRV_DIMENSION_TEXTURE2DARRAY => {
                // SAFETY: as above.
                let mut t = unsafe { view.Anonymous.Texture2DArray };
                t.PlaneSlice = plane;
                view.Anonymous.Texture2DArray = t;
            }
            _ => {}
        }
    }
    true
}

/// Brings a shader view's mips and slices into `d`; false if it names
/// another kind of texture or none of its subresources.
pub(super) fn shader_view_fits(
    d: &D3D12_RESOURCE_DESC,
    v: &mut D3D12_SHADER_RESOURCE_VIEW_DESC,
) -> bool {
    let (mips, layers) = (u32::from(d.MipLevels), u32::from(d.DepthOrArraySize));
    let one = d.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE1D;
    let two = d.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE2D;
    let three = d.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D;
    let ms = d.SampleDesc.Count > 1;
    // SAFETY: each the union member its dimension names; plain numbers.
    let fit = unsafe {
        let a = &mut v.Anonymous;
        match v.ViewDimension {
            D3D12_SRV_DIMENSION_TEXTURE1D if one => {
                let t = &mut a.Texture1D;
                within(t.MostDetailedMip, t.MipLevels, mips).map(|n| t.MipLevels = n)
            }
            D3D12_SRV_DIMENSION_TEXTURE1DARRAY if one => {
                let t = &mut a.Texture1DArray;
                within(t.MostDetailedMip, t.MipLevels, mips)
                    .zip(within(t.FirstArraySlice, t.ArraySize, layers))
                    .map(|(n, s)| (t.MipLevels, t.ArraySize) = (n, s))
            }
            D3D12_SRV_DIMENSION_TEXTURE2D if two && !ms => {
                let t = &mut a.Texture2D;
                within(t.MostDetailedMip, t.MipLevels, mips).map(|n| t.MipLevels = n)
            }
            D3D12_SRV_DIMENSION_TEXTURE2DARRAY if two && !ms => {
                let t = &mut a.Texture2DArray;
                within(t.MostDetailedMip, t.MipLevels, mips)
                    .zip(within(t.FirstArraySlice, t.ArraySize, layers))
                    .map(|(n, s)| (t.MipLevels, t.ArraySize) = (n, s))
            }
            D3D12_SRV_DIMENSION_TEXTURE2DMS if two && ms => Some(()),
            D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY if two && ms => {
                let t = &mut a.Texture2DMSArray;
                within(t.FirstArraySlice, t.ArraySize, layers).map(|s| t.ArraySize = s)
            }
            D3D12_SRV_DIMENSION_TEXTURE3D if three => {
                let t = &mut a.Texture3D;
                within(t.MostDetailedMip, t.MipLevels, mips).map(|n| t.MipLevels = n)
            }
            D3D12_SRV_DIMENSION_TEXTURECUBE if two && !ms && layers >= 6 => {
                let t = &mut a.TextureCube;
                within(t.MostDetailedMip, t.MipLevels, mips).map(|n| t.MipLevels = n)
            }
            D3D12_SRV_DIMENSION_TEXTURECUBEARRAY if two && !ms => {
                let t = &mut a.TextureCubeArray;
                let cubes = layers.saturating_sub(t.First2DArrayFace) / 6;
                within(t.MostDetailedMip, t.MipLevels, mips)
                    .zip(within(0, t.NumCubes, cubes))
                    .map(|(n, s)| (t.MipLevels, t.NumCubes) = (n, s))
            }
            _ => None,
        }
    };
    fit.is_some()
}

/// [`shader_view_fits`] for an unordered access view.
pub(super) fn unordered_view_fits(
    d: &D3D12_RESOURCE_DESC,
    v: &mut D3D12_UNORDERED_ACCESS_VIEW_DESC,
) -> bool {
    let (mips, layers) = (u32::from(d.MipLevels), u32::from(d.DepthOrArraySize));
    let one = d.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE1D;
    let two = d.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE2D && d.SampleDesc.Count <= 1;
    let three = d.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D;
    // SAFETY: each the union member its dimension names; plain numbers.
    let fit = unsafe {
        let a = &mut v.Anonymous;
        match v.ViewDimension {
            D3D12_UAV_DIMENSION_TEXTURE1D if one => (a.Texture1D.MipSlice < mips).then_some(()),
            D3D12_UAV_DIMENSION_TEXTURE1DARRAY if one && a.Texture1DArray.MipSlice < mips => {
                let t = &mut a.Texture1DArray;
                within(t.FirstArraySlice, t.ArraySize, layers).map(|s| t.ArraySize = s)
            }
            D3D12_UAV_DIMENSION_TEXTURE2D if two => (a.Texture2D.MipSlice < mips).then_some(()),
            D3D12_UAV_DIMENSION_TEXTURE2DARRAY if two && a.Texture2DArray.MipSlice < mips => {
                let t = &mut a.Texture2DArray;
                within(t.FirstArraySlice, t.ArraySize, layers).map(|s| t.ArraySize = s)
            }
            D3D12_UAV_DIMENSION_TEXTURE3D if three && a.Texture3D.MipSlice < mips => {
                let t = &mut a.Texture3D;
                let depth = (layers >> t.MipSlice).max(1);
                within(t.FirstWSlice, t.WSize, depth).map(|s| t.WSize = s)
            }
            _ => None,
        }
    };
    fit.is_some()
}

/// A view of all of target `d` (its first mip) in `format`, as the words of
/// a render target view or (`depth`) a depth stencil view.
pub(super) fn whole_target(d: &D3D12_RESOURCE_DESC, format: u32, depth: bool) -> [u32; 6] {
    let layers = u32::from(d.DepthOrArraySize);
    let ms = d.SampleDesc.Count > 1;
    // Render target dimensions; depth ones are one less, without 3D, and
    // have flags before the rest.
    let (dimension, [a, b, c]) = match (d.Dimension, ms, layers > 1) {
        (D3D12_RESOURCE_DIMENSION_TEXTURE1D, _, false) => (2, [0, 0, 0]),
        (D3D12_RESOURCE_DIMENSION_TEXTURE1D, _, true) => (3, [0, 0, layers]),
        (D3D12_RESOURCE_DIMENSION_TEXTURE3D, ..) => (8, [0, 0, u32::MAX]),
        (_, false, false) => (4, [0, 0, 0]),
        (_, false, true) => (5, [0, 0, layers]),
        (_, true, false) => (6, [0, 0, 0]),
        (_, true, true) => (7, [0, layers, 0]),
    };
    if depth {
        [format, dimension - 1, 0, a, b, c]
    } else {
        [format, dimension, a, b, c, 0]
    }
}

#[cfg(test)]
#[path = "checks_tests.rs"]
mod tests;
