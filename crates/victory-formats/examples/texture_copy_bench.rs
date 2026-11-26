//! Synthetic CPU copy throughput. No GPU or game files are used.

use std::hint::black_box;
use std::time::Instant;

use victory_formats::texture::{Desc, Dimension, Layout, Swizzle};

fn main() {
    let base = Desc {
        dimension: Dimension::Tex2d,
        width: 1920,
        height: 1080,
        depth: 1,
        mips: 1,
        block: 1,
        element_bytes: 4,
        samples: 1,
        swizzle: Swizzle::SW_64KB_D,
    };
    let cases = [
        ("RGBA8 64KB_D", base, 0, 0),
        (
            "RGBA8 4KB_S",
            Desc {
                swizzle: Swizzle::SW_4KB_S,
                ..base
            },
            0,
            0,
        ),
        (
            "BC7 mip 5",
            Desc {
                width: 4096,
                height: 4096,
                mips: 13,
                block: 4,
                element_bytes: 16,
                ..base
            },
            5,
            0,
        ),
        (
            "RGBA8 volume",
            Desc {
                dimension: Dimension::Tex3d,
                width: 128,
                height: 128,
                depth: 64,
                swizzle: Swizzle::SW_4KB_S,
                ..base
            },
            0,
            0,
        ),
        (
            "R8 linear tight",
            Desc {
                width: 256,
                height: 128,
                element_bytes: 1,
                swizzle: Swizzle::LINEAR,
                ..base
            },
            0,
            0,
        ),
        (
            "R8 linear padded",
            Desc {
                width: 256,
                height: 128,
                element_bytes: 1,
                swizzle: Swizzle::LINEAR,
                ..base
            },
            0,
            16,
        ),
    ];
    for (name, desc, mip, padding) in cases {
        let layout = Layout::new(&desc).unwrap();
        let m = layout.mips[mip as usize];
        let active = m.width as usize * desc.element_bytes as usize;
        let row = active + padding;
        let plane = row * m.height as usize;
        let bytes = active * m.height as usize * m.depth as usize;
        let mut tiled = vec![0x5a; layout.size as usize];
        let mut linear = vec![0; plane * m.depth as usize];
        let calls = (64usize << 20).div_ceil(bytes);
        for detile in [true, false] {
            let mut copy = || {
                if detile {
                    layout.detile(
                        black_box(&tiled),
                        mip,
                        0,
                        black_box(&mut linear),
                        row,
                        plane,
                    )
                } else {
                    layout.tile(
                        black_box(&mut tiled),
                        mip,
                        0,
                        black_box(&linear),
                        row,
                        plane,
                    )
                }
                .unwrap();
            };
            copy();
            let mut samples = [0.0; 7];
            for sample in &mut samples {
                let start = Instant::now();
                for _ in 0..calls {
                    copy();
                }
                *sample = (bytes * calls) as f64 / start.elapsed().as_secs_f64() / 1e9;
            }
            samples.sort_by(f64::total_cmp);
            let op = if detile { "detile" } else { "tile" };
            println!("{name:20} {op:6} {:8.2} GB/s", samples[3]);
        }
    }
}
