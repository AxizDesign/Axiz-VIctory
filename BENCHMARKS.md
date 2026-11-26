# Benchmarks

The local host is documented in [NVIDIA compatibility](NVIDIA_COMPATIBILITY.md). Baseline is upstream `63bcca43c6a00a758510398fa43096ce7e4f7df0`, compiled with Rust 1.98.1 and the release profile. The candidate comparison uses `e04aea17e40f0ec984a12e2a1881fe906089095d`.

## Synthetic texture copies

```powershell
cargo run --release --locked -p victory-formats --example texture_copy_bench
```

Use the same example source and toolchain in both checkouts. The harness uses one thread, warms each operation, reuses its buffers, and reports the median of seven samples copying at least 64 MiB of active bytes each. GB/s is decimal. Layout construction and allocation are outside the measurement. Tightly packed R8 uses 256 active bytes per row. The padded case adds 16 bytes per linear row. Repeated buffers exercise warm caches and the host memory subsystem.

The table gives the median of three process runs per build, with the build order varied between runs. Each cell lists detile / tile in GB/s. The intermediate build isolates the imported change.

| Workload | Upstream | Imported change | With packed linear copy |
| --- | ---: | ---: | ---: |
| RGBA8 64KB_D | 16.80 / 10.69 | 19.45 / 16.39 | 18.97 / 15.97 |
| RGBA8 4KB_S | 14.35 / 9.06 | 16.82 / 14.23 | 16.84 / 14.26 |
| BC7 mip 5 | 28.86 / 32.12 | 49.88 / 45.57 | 49.29 / 52.94 |
| RGBA8 volume | 10.46 / 6.81 | 15.67 / 13.63 | 15.67 / 13.64 |
| R8 linear tight | 60.82 / 40.74 | 55.77 / 39.37 | 69.64 / 72.79 |
| R8 linear padded | 47.12 / 40.31 | 46.66 / 39.91 | 46.63 / 39.17 |

Volume tiling reached 2.00× baseline throughput. The packed R8 tiling path reached 1.79× baseline and 1.85× the intermediate build. Padded R8 varied by up to about 3% from baseline in this comparison.

In a separate process-memory sample, both copy benchmarks reached 25.13 MiB peak working set, polled every 15 ms. Neither workload initializes a GPU.

## Gameplay capture

Use one interval per frame in milliseconds, with a single `frametime_ms` header and a decimal point. Export a consistent presentation or display interval from the capture tool throughout the comparison. The input may live outside this repository.

```powershell
cargo xtask frame-stats C:\Benchmarks\capture.csv --stutter-ms 50
```

Average FPS is 1000 divided by mean interval. Median FPS is the inverse median interval. The 1% and 0.1% lows invert the mean of the slowest ceiling(1% or 0.1%) intervals and require at least 100 or 1,000 frames respectively. P95, P99 and P99.9 use nearest rank. A frame misses the 30 FPS budget if it exceeds 1000/30 ms. The stutter count applies the supplied threshold independently.

Keep separate, labeled cold-cache and warm-cache captures. Record the exact branch/SHA, driver, adapter, power and display settings, render settings, route and duration. For each configuration, capture at least three repetitions of the same route and enough frames for the tail metrics. Collect CPU and per-thread utilization, GPU utilization, peak process RAM, peak dedicated VRAM and any crash or device-removal codes alongside each capture. The interval analyzer reads only the CSV. Compare aggregate metrics across equivalent runs and inspect graphics output for differences.

On a hybrid system, confirm from the process or GPU capture that both builds use the same NVIDIA adapter. For the pinned baseline, Windows per-application graphics preference can be used if it places the process on NVIDIA. The candidate can explicitly request the NVIDIA adapter with `VICTORY_D3D12_NVIDIA=1`.

Gameplay and user-reported captures are pending. The synthetic copy example does not measure rendering or GPU utilization.

For an analyzer smoke test, a generated 1,000-interval CSV with 999 intervals of 20 ms and one of 100 ms yielded 49.801 average FPS, 50.000 median FPS, 35.714 and 10.000 for the two lows, and one budget miss and stutter. This fixture only checks arithmetic.

## D3D12 heap creation on RTX 5070 Ti Laptop GPU

```powershell
$env:VICTORY_D3D12_NVIDIA='1'
cargo test --release --locked -p victory-gpu host_heap_creation_throughput -- --ignored --nocapture
```

The Release harness discards one warm-up and times 21 sequential heap creations per case, with destruction outside each timed interval. The earlier candidate was measured with only the benchmark attached. Times are median / P90 in microseconds.

| Title descriptors | Previous candidate | Bounded heap | With reusable null template |
| --- | ---: | ---: | ---: |
| 128 samplers | 893 / 954 | 951 / 1,059 | 355 / 395 |
| 4,096 views | 3,260 / 3,489 | 8,735 / 8,860 | 7,854 / 8,100 |
| 262,144 views | 5,924 / 6,179 | 9,889 / 10,017 | 8,939 / 9,175 |

The bounded heap backs all one million addressable slots and costs more to initialize than the older undersized heap. Reusing the immutable CPU template reduces median initialization 10–63% relative to the bounded heap alone. The new stride and write-mark buffers request 12,000,000 bytes total per view heap, down from 25,165,824 bytes for these title sizes. These are buffer sizes, with process RAM and dedicated VRAM residency still unmeasured.

## Shared constant expressions in the DXIL emitter

The Release benchmark `constant_analysis_of_shared_subexpressions` builds a 15-level expression graph outside the timed interval, checks the result against the CPU formula and reports the median of 21 evaluations. With KainanP's constant-dot lowering as the baseline, caching proven values by instruction and remaining depth reduced the median from **1,001,400 ns** to **2,600 ns** on the Intel Core Ultra 9 275HX. This tests repeated visits to a shared expression graph. It does not estimate a title's total shader or PSO compilation time.

## Experimental native FP16 on Blackwell

Separate checkout at [upstream PR #2825](https://github.com/Marvisak/VIctory/pull/2825), head `47078d7f3cb90335ec6efc921ef8f3f3ce4d178c`, with the NVIDIA test-adapter selector applied. Rust 1.98.1 Release, Windows 11 build 26300, RTX 5070 Ti Laptop GPU, driver 621.14. The PR's `translated_f16_math_costs` harness reports GPU timestamps for synthetic dependent arithmetic chains and compares results per thread.

| Instruction chain | f32 path | Native f16 | Speedup |
| --- | ---: | ---: | ---: |
| Packed f16 FMA | 8.376 ms | 0.630 ms | 13.3× |
| Scalar f16 FMA | 4.636 ms | 0.630 ms | 7.4× |
| f16 multiply + add | 3.682 ms | 0.133 ms | 27.6× |

Each comparison matched all 262,144 threads. Four focused native-path correctness tests passed. This experiment uses the PR's timing protocol and has no rendering or frame-pacing measurements. The shipped candidate keeps its existing f16 path.
