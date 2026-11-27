# Pipeline scheduling and RTX queue validation

## Baseline and provenance

The release starts from the verified Axiz Vertex integration branch
`4a3c3472749a0f561080a0ded7170f25709c2592`. The original Vertex pin is
`33f26c4e49c64bc1135aebd0e8d02507c1417a0e`. This keeps its Rust submission,
copy queues, residency policy, Wave32 path and Axiz texture and shader ports.
See the [first integration record](axiz-alpha-integration.md) for the earlier
comparison. The candidate changes neither render resolution nor shader math.

| Candidate | Source reviewed | Decision |
| --- | --- | --- |
| [Vertex](https://github.com/proneonn/Vertex) | `33f26c4e49c64bc1135aebd0e8d02507c1417a0e`, GPL-3.0-or-later | Live ref matches the established baseline. |
| [victory-all-forks](https://github.com/WillyboPeep/victory-all-forks) | Prior pin `8b8319c0ac550f962784d286053aac77cd3be9dc`, GPL-3.0-or-later | The remote returned repository not found at this check. Vertex already contains the earlier multi-fork integration. |
| [XCL](https://github.com/xcl-dev/xcl) | Live ref `8b957bec6f82495aec940944c6bf47faa02db590`, GPL-2.0-only | Its implementation is excluded from the GPLv3 candidate. |
| [DirectX Graphics Samples](https://github.com/microsoft/DirectX-Graphics-Samples) | `68a2b1e97f71642924e74c7757ca21756ad221b5`, MIT residency starter header | Reviewed `Libraries/D3DX12Residency/d3dx12Residency.h`. Its tracking and submission contract assumes caller-managed residency sets. A wholesale port would duplicate Vertex's existing range, epoch and alias lifetime tracking. |
| [RTX Memory Utility](https://github.com/NVIDIA-RTX/RTXMU) | `3167d8187144d0cedbbe77e60e92a1d6147d9500`, MIT | Reviewed D3D12 acceleration-structure manager and suballocator. The manager owns build and compaction lifetimes. Vertex translates guest shaders and observes guest-owned addresses, so transparently moving structures first needs verified address ownership and remapping. |
| [ShaderMake](https://github.com/NVIDIA-RTX/ShaderMake) | `d7550f0fc69e724461eafbb0462467f2bec1b708`, MIT | It compiles source permutations via external compilers and tracks their input dependencies. Vertex emits DXIL from guest instructions and schedules driver PSO creation at runtime. ShaderMake is applicable to authored HLSL tooling, outside this integration. |
| [Belobug](https://github.com/Belobug/VIctory-Belobug-AMD) | Public head `4eb6cfdf9e61cb112f1f811344ec0dd309eddd6c`, GPL-3.0-or-later | Imported the independent DXGI segment query and adapted the slow pipeline lane shared with Cool Fox. |
| [CGBJ public source mirror](https://github.com/DoubleGhosty/cool_fox) | Archived Git HEAD `f4b48ad766ddbec1a1867022cd42f7645b0dd55e`, GPL-3.0-or-later | The original codec-d remote returned repository not found. The public mirror and previously pinned Git objects supply the reviewed implementation. |

Only two source changes were imported. No vendor C++ source, private launcher,
prebuilt executable, game asset, or access-control component was transferred.

### Attached archives

- **VIctory-Belobug-AMD-a-publier.zip** SHA-256
  `e3057f3e3cd1d057d7af101a1d1f1bc5b1dbc537d5842dbf9363b08dc866ab8d`.
  Byte-for-byte Git blob comparison matched 268 of 269 files to Belobug
  `cd8dfc06ace7ccb91a19431e74bc2d0de2a10145`. Only `scripts/play.ps1`
  differed. The archive has GPLv3 text and a GPL-3.0-or-later manifest.
  The matching commit also carries a PC FidelityFX bridge. That component
  and its SDK and binary dependencies were outside the import.
- **VIctory-CGBJ.zip** SHA-256
  `dd84f3baf75de0baef83a052c42fe29cb7937d9f989954816dd3db10d71aa901`.
  The embedded main reference identifies `f4b48ad7`. Of 250 working files,
  248 matched the pinned public tree by Git blob hash. The two differences
  were `Play-VIctory.cmd` and `scripts/play.ps1`. Its root license and Rust
  manifest agree with the public GPLv3 lineage. Archive launchers, hooks,
  configuration, and object packs were neither executed nor imported.
  The project-wide license is not a license grant for external title files
  or detached binary dependencies.

## Changes

- `da12d38f` ports the slow speculative lane from `a30829c4d245467d06df7aaebddc32f8fd954a82`
  by the recorded author `local`, with the original co-author credit
  retained. The source commit is in both public histories. Known slow
  precache work uses at most a quarter of the worker pool. Draw and dispatch
  demand retains Vertex's existing demand-based ordering and bypasses the
  speculative cap. Measured successful builds write timing hints atomically
  and independently of the binary cache. `VICTORY_PIPELINE_SLOW=0` restores
  the prior scheduling policy. An empty timing history cannot classify a
  previously unseen pipeline.
- `f92c069d` cherry-picks IronMan's
  `0e301a9c84bae3fc0b6eaea1faf4e446ecb87855` from the public Belobug branch
  under GPL-3.0-or-later. It splits local and shared DXGI budget queries.
- `45049c1b` applies the local sample to Vertex's texture allocation, card
  buffer admission and idle residency gates. These gates now use one
  segment query and can still act when the unrelated shared query fails.
  Thresholds, resource lifetimes and rendering behavior retain their
  existing policy. The same commit adds opt-in hardware coverage to the
  generated queue fixture, with explicit NVIDIA and card-backed checks.

The larger Belobug texture prelude (`5d6bd841`) changes how staged textures
cross submission, alias and upload phases. Cool Fox's per-heap bindless
refresh and low-commit staging work (`18bdf793`, `ab75ac3e`) depends on its
pool and descriptor-publication model. Both need a separate integration
with mixed alias, upload and graphics consumers. Existing Vertex structure
translation gaps and the prior-line NVIDIA structured-view stride issue
remain alpha gates.

## RTX queue validation

The opt-in fixture uses a generated 4 MiB write-watched buffer and twelve
submissions, with a compute shader writing to tested offsets. It compares
every dword against the expected host result. Elapsed time covers submission
through settlement and excludes fixture setup and CPU comparison. Process
order is rotated to limit order effects. Use an isolated diagnostics test
process for each queue configuration.

```powershell
$env:VICTORY_D3D12_NVIDIA = '1'
$env:VICTORY_TEST_NVIDIA_QUEUES = '1'
$env:VICTORY_CARD_BUFFERS_MIN_MB = '1'
$env:VICTORY_RELAY = '2'
$env:VICTORY_QUEUE2 = '0' # Repeat in its own process with 1 and 3.
cargo test --profile diag --locked -p victory-gpu --features diagnostics title_bytes_go_up_and_shader_writes_come_back -- --nocapture
```

Doctor reported the RTX 5070 Ti Laptop GPU with 11,944 MiB dedicated memory,
32-lane waves, Shader Model 6.8 and native 16-bit operations. The fixture also
requires an active local DXGI budget sample and hardware-backed allocation.
These workloads report queue correctness and small-batch transfer behavior.
Game frame pacing, upload overlap during streaming, peak VRAM under
sustained pressure and visual parity still require separate measurements.

Nine isolated processes per mode used the same diagnostics build and
fixture. All 27 matched every expected dword.

| D3D12 mode | Median for 12 submissions | Observed range |
| --- | ---: | ---: |
| Direct queue only (`0`) | 11.110 ms | 9.969–14.354 ms |
| Copy-queue readback (`1`) | 23.426 ms | 19.644–25.201 ms |
| Copy-queue upload and readback (`3`) | 23.327 ms | 21.176–27.347 ms |

The initial local DXGI budget was about 10.9 GiB. These measurements favor
the direct queue for this dependency-heavy small workload. The candidate
retains the existing shipping queue policy pending a larger streaming
comparison. The WARP overlap/refetch regressions also passed.

The locked Release workspace run completed with 806 passes, zero failures
and 14 inherited ignores. The prior baseline and previous Release candidate
were retained. The new executable and PDB are archived at
`target/validated/45049c1b/`.
