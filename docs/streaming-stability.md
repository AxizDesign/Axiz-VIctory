# Streaming and frame-time stability

The starting code and runtime baseline was `e1a7bfed92960ee81bb5751126d4ccb2408bc3a8`.
Commit `eaaaa65a` adds the same generated measurement fixture used for baseline
comparison. The prior Release and diagnostic binaries are kept locally.

## Integrated work

- `4e0a324d` adapts codec_d's refused heap growth backoff from
  `ab75ac3e02ba9fe5b45297c6254bafe5660a1a68` (GPL-3.0-or-later).
  Each placed-texture heap alignment class waits 500 ms after a rejected
  heap creation. Existing ranges are searched first. The original
  committed-allocation fallback is still available.
- The same commit trims completely empty retained texture heaps when raw DXGI
  local usage reaches 90% of its budget. Scans are limited to four a second.
  Returns through the existing retirement mechanism settle first. Trimming
  preserves live or retired-but-referenced textures and keeps the existing
  idle-age policy on the live-byte estimate.
- `de0e649d` adds two opt-in low-priority workers for known-slow speculative
  PSOs on machines with at least 16 visible logical processors. They check for
  at least 8 GiB of free physical RAM and 12 GiB of available commit before
  claiming work. The original workers, priority order, and speculative cap
  remain available for demand. Enable the experiment with
  `VICTORY_PIPELINE_HELPERS=1`.

Previously reviewed Belobug `5d6bd841` moves texture copies into a submission
prelude. Its pool and alias-state contract differs from Vertex. It remains
the main upload-order candidate once mixed read/write and alias workloads
can verify the port. Cool Fox `18bdf793` ties speculative staging and
bindless publication to its own heap pass. This branch takes the independent
allocator backoff from its successor. Cross-heap publication and alternate
guard-journal execution remain deferred.

## Generated target workloads

The marked hardware test runs on an RTX 5070 Ti Laptop GPU. It uses 128 MiB
of generated, write-watched memory, 48 compute submissions, and 16 changed
64 KiB regions per submission. It settles every six submissions and checks
every dword of the buffer. Timings are CPU submit-to-settle intervals.
The first interval includes the initial population of the card copy.
VRAM sampling measures local process usage after each settlement.

```powershell
$env:VICTORY_D3D12_NVIDIA = '1'
$env:VICTORY_TEST_NVIDIA_QUEUES = '1'
$env:VICTORY_TEST_STREAMING = '1'
$env:VICTORY_CARD_BUFFERS_MIN_MB = '1'
$env:VICTORY_RELAY = '2'
$env:VICTORY_QUEUE2 = '1'
cargo test --profile diag --locked -p victory-gpu --features diagnostics title_bytes_go_up_and_shader_writes_come_back -- --nocapture
```

The driver PSO benchmark created 64 generated graphics pipelines through
`victory-pso bench --adapter nvidia --only pipelines --rounds 15`
(driver 32.0.16.2114). Individual cold creates took a median **18.698 ms**
with **20.625 ms p95**. Recreation from cached blobs took **0.265 ms**
median and **0.284 ms p95**. For a freshly generated set of 64 pipelines,
one, four and 24 threads took **1,248.150 ms**, **363.885 ms**, and
**155.055 ms**, respectively. These driver runs are independent of the
runtime scheduler and use generated pipelines rather than title shaders.

The production queue replay measured **800 to 400 virtual ticks** to
complete all speculative work with two helpers. The urgent job remained
at **one tick**. The helper stays disabled by default because the replay
does not measure CPU interference, driver locks, or real memory contention.

## Rendering issues to verify next

- Texture pop-in or flicker when a source upload crosses multiple recorded
  lists. Reproduce the ordering with two submissions and aliased staging.
- Missing or incorrect indirect lighting and black water patches,
  especially where RaytracingResolve or structure-building shaders do not
  translate on Wave32.
- Incorrect structured-buffer reads at nontrivial strides on NVIDIA. Port
  the prior-line fixture to Vertex and verify exact values before closing it.
- Previously reported blocky or missing cutscene geometry and smooth clouds.
  These still need generated feature-level reproducers or authorized
  equivalent-output captures before changing rendering behavior.

## Paired streaming results

Six fresh processes per build and queue mode, rotating queue mode and
alternating build order. Each process contributed one cold window and seven
warm windows. Values use nearest-rank percentiles. All 36 runs matched the
expected bytes. The local DXGI budget was 11,176 MiB.

| Build | Queue mode | Cold p50, ms | Warm p50, ms | Warm p95, ms | Peak sampled local, MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| Baseline | Direct | 28.443 | 4.093 | 6.363 | 143 |
| Baseline | Copy readback | 38.212 | 3.882 | 6.237 | 145 |
| Baseline | Copy both ways | 39.665 | 4.365 | 7.686 | 162 |
| Candidate | Direct | 30.565 | 4.111 | 7.077 | 143 |
| Candidate | Copy readback | 38.910 | 4.064 | 4.556 | 145 |
| Candidate | Copy both ways | 39.861 | 4.076 | 7.634 | 157 |

Copy readback had the lowest candidate warm tail and the direct queue
settled the first candidate window fastest. With both copy lanes the direct
queue was instructed to wait for 47 uploads in each candidate process.
The first upload exceeded the landing pool, triggering direct fallback.
These results keep the current readback-only default. The lower warm tail
in this sample does not isolate a production code speedup. The workload
uses too little VRAM to exercise high-pressure heap reclamation.

In the separate generated `victory-pso` upload and sync groups, uploading
64 MiB took 1.334 ms direct and 1.325 ms on the copy queue. Readback took
1.801 ms direct and 1.783 ms via copy. Batching 64 empty direct lists took
0.557 ms versus 1.031 ms in separate submissions. These measurements
isolate transfer and submission costs. They do not quantify simultaneous
GPU-engine occupancy.

The final locked Release workspace run passed 807 tests, with zero failures
and 14 inherited ignores. The WARP overlap and refetch regressions passed.
The hardware smoke check detected Shader Model 6.8, 32-lane waves and native
16-bit support. Gameplay frame time and fidelity remain unverified.
