# CPU streaming candidate

The source baseline is `443af51d74996997b1530ca7d40bb3a387139d2c`.
The archived stability executable is `target/validated/8d4a69f7/victory.exe`.
The new Release is built from `5d7cacd641ff27ffa04a387ce6c005388766b8b6`
with the repository's locked dependencies, MSVC and default features.

## Runtime changes

- `419adcd2` selects and orders only the texture entries that fit the pass
  quota, collecting queue totals in the same traversal. Retries still run
  newest first. Fresh work still alternates newest and oldest. Eligibility,
  write sequence numbers, residency policy and uploads retain their
  existing semantics.
- `5d7cacd6` reuses successful section commits across runtime and streaming
  aliases and maps a proven exact placeholder directly. The hint lasts for
  one mapping call. A rejected replacement falls back to the original
  split-and-map path. Partial replacement and changes spanning several
  views still preserve their surviving mappings. The same remapping guard
  covers all operations. Set `VICTORY_MAP_LEAN=0` to use the original
  split and commit calls.
- The frame log separates the allocator lock wait, wait for readers,
  system unmap/map/commit work, notices to the GPU runtime and bytes mapped
  again outside a changed range. In diagnostic builds the periodic
  address-space walk defaults to every 60 memory lines. Use
  `VICTORY_MEMORY_MAP=6` for the former interval.

## Source review

| Source | Decision |
| --- | --- |
| Belobug and Cool Fox/CGBJ | Directly ported the remapping code from `49d71266b25f8d432cfc86ec3d27c304a5191e66` using the existing section bitmap. The commit records its author as `local <local@localhost>`. The thread-priority and logging changes in that commit were outside the mapping port. |
| Belobug | Ported the diagnostic walk cadence from `fb28d49e959359cb20992e5c5533a647cb408054`, by `local <local@localhost>`. The address-type cache hunk was omitted because reused addresses can change mapping type. |
| fxceless | Ported the remap phase timers from `d8986cd7b8392f85478f5675933c59712519e163`, by fxceless. Timing is collected only when the frame log is enabled. |
| Vertex and other inherited code | The baseline already uses short kernel reads for memory waits and merges range notifications. It also has opt-in parallel texture checks and already parks known unmapped view blocks from `e277b98e6063f9c0ad02f61a7356f783803a73ba`. |
| Upstream VIctory | Marvis's `56f75f8583061826fd7594e4e06f442bc0718f48` works through a different bindless registry. Vertex has a native parallel path that preserves submit and write boundaries. |
| Flamingo | `0cacf878f6c3577b3c5118ec79e8d268797752d0` removes redundant bounds queries in its placeholder splitter. The Axiz path already avoids those queries and checks exact replacements through the existing Windows mapping API. |

The three ports above come from GPL-3.0-or-later forks with the same
component license. The [existing source inventory](pipeline-scheduling-integration.md)
records the pinned Belobug and Cool Fox/CGBJ heads and archive provenance.
The Windows section-commit and exact replacement semantics are documented
by [CreateFileMapping](https://learn.microsoft.com/windows/win32/api/winbase/nf-winbase-createfilemappinga)
and [MapViewOfFile3](https://learn.microsoft.com/windows/win32/api/memoryapi/nf-memoryapi-mapviewoffile3).

## Reproduction

Each CPU fixture measures 120 frames after 12 warm-up frames. The
descriptor fixture stages 15,897 slots, including 13,877 initially
awaiting memory, and changes 372 slots per frame. It compares a checksum
of service order. The remap fixture rotates 372 physical pages through
372 addresses per frame, checking the first and last word at every
address. Compare `VICTORY_MAP_LEAN=0` with `1` and the nested remap
timers with `VICTORY_FRAME_LOG=0` and `1` in fresh processes.

```powershell
cargo test --release --locked -p victory-gpu generated_cpu_streaming_profile -- --ignored --nocapture
cargo test --release --locked -p victory-hle generated_remap_profile -- --ignored --nocapture
$env:VICTORY_D3D12_NVIDIA = '1'
cargo test --release --locked --workspace --no-fail-fast -- --test-threads=1
```

The comparables isolate descriptor and Windows virtual-memory work.
The expanded NVIDIA queue test also checks upload, compute and readback.
They exercise different phases. Their durations cannot be summed to
predict gameplay. The submitted 224.2 ms preparation and 1,597.4 ms GPU
idle overlap. Neither value identifies how much wall time was spent in
runtime scheduling or in the title. Windows Performance Recorder rejected
the CPU trace with `0xc5585011`. System-wide scheduler attribution remains
open.


## Measurements

Windows 11, Intel Core Ultra 9 275HX and GeForce RTX 5070 Ti Laptop
(12,227 MiB reported by the driver, driver 621.14). Values below are
medians of nine fresh processes per mode. Runs alternated order.
P95 and P99 are nearest-rank estimates over the 120 frames of each run.

| Workload | Total before / after | P95 before / after | P99 before / after |
| --- | ---: | ---: | ---: |
| Texture queue, including slot changes and retry transitions | 49.542 / 26.269 ms | 0.458 / 0.254 ms | 0.496 / 0.270 ms |
| Windows remap, frame log disabled | 211.827 / 188.250 ms | 1.807 / 1.620 ms | 1.844 / 1.670 ms |
| Windows remap, frame log enabled | 221.030 / 197.874 ms | 1.909 / 1.687 ms | 1.939 / 1.783 ms |

In the queue fixture the selection and census phase fell from 43.445 to
20.142 ms total. All runs produced service checksum
`6130681562897695922`. With the new remap path the extra frame-log timing
probes cost about 0.080 ms per generated frame. The remap test measures
the probes, with separate controls required for presentation diagnostics,
the periodic address-space walk and full validation. Measured remap-thread
kernel time over the run including warm-up fell from 187.5 to 140.625 ms.
Windows thread CPU accounting is coarse at this duration.

The workspace suite on NVIDIA passed 813 tests, with 17 ignored and zero
failures. Both added ignored benchmarks ran separately. The diagnostics
profile passed `cargo check`. The asset policy passed. The new Release
and PDB are archived under `target/validated/5d7cacd6`, with the previous
`8d4a69f7` Release preserved. The new executable's SHA-256 is
`1A4977ED9EF8615A28A43A0B7A87BD916F9746FE83B0C108269C1757C20CAC7E`.

The runtime's separate 128 MiB streaming test checked upload, shader
writes and readback over 48 submissions. All 15 baseline and 15 candidate
runs passed the output checks. The median sum of eight submit-to-settle
windows was 64.277 ms before and 62.594 ms after. Steady window medians
were 3.837 and 3.830 ms, and both builds peaked at 143 MiB sampled local
usage. This route uses the copy queue for readback. It does not use the
changed remap and descriptor paths, so the timings provide a regression
control.

The separate 120-frame mixed NVIDIA regression ran at 8,242 MiB sampled
local memory. It verified every readback pixel in the generated workload,
with 396.007 ms total frame latency, 0.833 ms median frame time and
0.211 ms median outside timed GPU commands. The mixed tool exercises the
graphics, compute and copy queues under pressure. It does not route
through the changed compatibility paths, so its timing is a platform
control.

The reported 1,683.5 ms frame was not reproduced. The next measurement
needs matching present intervals, per-thread waits and CPU samples, and
queue activity over a sustained streaming burst. The new remap totals can
then separate mapping and lock time from periods when the submission
thread has no work. The measured savings establish synthetic CPU
improvements only.
