# Mixed-frame pipeline latency

The candidate starts at `d7a7a178` on the integrated Vertex backend. The
first full profile used the generated benchmark from `a1cc8346`. It found
a median **8.96 ms** program-and-pipeline creation burst, versus **0.69 ms**
for a 32 MiB streaming upload. The long frames came from pipeline work.

## Runtime change

`382b5524` coalesces overlapping D3D12 PSO requests at the existing
persistent pipeline-library entry point. The key is the existing hash
of the serialized root signature, shader bytes, and pipeline state.
One caller loads or creates the PSO. Other callers for that key receive
the same COM object when it is ready. Unrelated keys can build concurrently.
Failure leaves the work retryable, and only callers hold strong references.
The driver library's on-disk format and asynchronous store remain in use.
The old policy remains the default.

No guest draw, descriptor, transfer, resource barrier, or shader arithmetic
changed. The older speculative compile helpers remain opt-in. The benchmark
uses the same Rust coalescer and models the cold-miss case by requesting
the PSO directly from the driver. The runtime still uses its persistent
library for warm lookups.

## Reproduction

```powershell
cargo build --release --locked -p victory-pso
$env:VICTORY_PSO_COALESCE = '1' # Repeat with 0 for the baseline.
$env:VICTORY_MIXED_SALT = '543307'
& ./target/release/victory-pso.exe bench --adapter nvidia --only mixed
```

The workload has 120 offscreen frames. Every frame streams 32 MiB over the
copy queue, dispatches a shader through a 12-byte structured SRV, draws to a
128 by 128 RGBA8 target, and reads back and checks every pixel. Four recording
workers request the same new graphics description every fourth frame.
Set `VICTORY_MIXED_PEERS=1` for the single-request control. The test holds
generated resident allocations up to 8 GiB and leaves at least one quarter
of reported free DXGI local budget unused before workload allocations.
Frame timing includes submission and readback completion. The CPU pixel
comparison and memory sampling follow each timed frame.

Six fresh processes per mode alternated order and used different shader
salts to reduce warm-driver-cache carryover. Below are medians across
processes. Frame P95 and P99 use nearest rank in each 120-frame run.

| Metric | Baseline | Coalesced |
| --- | ---: | ---: |
| Total over 120 frames | 447.204 ms | 357.161 ms |
| Frame median | 0.948 ms | 0.920 ms |
| Frame P95 | 11.329 ms | 8.522 ms |
| Frame P99 | 13.175 ms | 10.033 ms |
| PSO burst median | 9.527 ms | 7.040 ms |
| Frame P95 outside GPU timestamps | 10.614 ms | 7.861 ms |
| D3D12 graphics PSO creates | 120 | 30 |
| Peak sampled DXGI local usage | 8,242 MiB | 8,242 MiB |

The elapsed time outside the instrumented queues estimates how long this
workload has no timed GPU commands executing. It is not whole-device idle
on a shared desktop. The test measures the benefit of overlapping requests.
It does not establish the frequency of duplicate PSOs in gameplay or
full-budget eviction performance.

Single-request controls ran six pairs cold and six pairs with the same
prewarmed shader variants. Cold median total moved from **397.592 ms** to
**424.075 ms**, P95 from **9.902 ms** to **10.565 ms**, and P99 from
**11.604 ms** to **12.274 ms**. Warm median total was **179.564 ms** versus
**177.244 ms**. Warm P95 was **3.041 ms** versus **3.071 ms** and P99 was
**3.672 ms** versus **3.868 ms**. The control cannot separate small changes
from driver and desktop variability. The final candidate therefore keeps
coalescing opt-in via `VICTORY_PSO_COALESCE=1` (`af2be548`).

## Correctness and open issues

The mixed benchmark checked 16,384 output pixels per frame on the RTX and
ran a smaller equivalent test on WARP. Unit tests cover retry and distinct
keys proceeding during a blocked same-key build. The NVIDIA debug layer
was unavailable on this Windows installation because Graphics Tools is
missing. The ordinary NVIDIA and WARP device runs both passed.

The retained structured-buffer fixture reproduced the older Axiz NVIDIA
translation defect. It returned 101, 104, 107 and eventually zeros where
the expected dwords begin 200, 201, 202. The mixed benchmark's native
D3D12 descriptor reads correct values; that does not close the translated
heap-indexed issue.

A second generated fixture, `6262f55a`, reproduced stale texture aliases
on the RTX without title data. Two format interpretations of the same
guest memory were uploaded and confirmed clean. Consumers were recorded,
the guest bytes changed from 0x31 to 0x62, then the lists were submitted.
Both returned 0x31. The next newly recorded consumers returned 0x62.
The fixture asserts the intended result and is kept as an explicit
known-failing diagnostic repro.

```powershell
$env:VICTORY_D3D12_NVIDIA = '1'
cargo test --profile diag --locked -p victory-gpu --features diagnostics two_pre_recorded_texture_views_see_the_new_guest_bytes -- --ignored --nocapture
```

The two rendering defects are independent of the PSO gate. Rework the
submission-time upload policy only after the alias reproducer and mixed
output pass together. Before changing the translated descriptor path,
port the prior-line NVIDIA stride fixture to Vertex's bindings. The
mixed workload did not test presentation or resolve Wave32 lighting and
other existing rendering gaps.

The locked Release workspace suite passed 809 tests with zero failures.
Fourteen inherited ignores and the explicit aliasing reproducer account
for the 15 ignored tests. The prior Release executable's SHA-256 matched
its published build record.
