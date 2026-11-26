# Axiz-VIctory alpha integration

The integration branch uses Vertex as its Rust and D3D12 baseline. Git retains
Vertex's merge history and authors. The prior Axiz line remains available for
comparison at `c9ad62ca`, with Marvisak/VIctory `63bcca43` as its upstream
reference. This branch is the foundation for v0.1.0-alpha.

## Source selection

All entries responded to `git ls-remote --symref` before any source was fetched.
The pins below identify the fetched heads. Performance figures from different
setups or rendering configurations do not establish performance on this laptop.

| Source | Fetched HEAD | License and decision |
| --- | --- | --- |
| [proneonn/Vertex](https://github.com/proneonn/Vertex) | `33f26c4e49c64bc1135aebd0e8d02507c1417a0e` | GPL-3.0-or-later. Selected multi-fork baseline. Its published 6.4–9.9 median FPS comparison on a desktop RTX 5070 uses its own scene, settings and lighting behavior. |
| [DoubleGhosty/cool_fox](https://github.com/DoubleGhosty/cool_fox) | `f4b48ad766ddbec1a1867022cd42f7645b0dd55e` | GPL-3.0-or-later. Overlapping queue and commit-memory work. Published RTX 5090 settings use a Series S profile and lighting overrides. No equivalent evidence for the reported 25 FPS at the target fidelity. |
| [WillyboPeep/victory-all-forks](https://github.com/WillyboPeep/victory-all-forks) | `8b8319c0ac550f962784d286053aac77cd3be9dc` | GPL-3.0-or-later. Useful multi-fork history already represented in Vertex. Candidate defaults also include quality shortcuts. |
| [fxceless/victory-fork](https://github.com/fxceless/victory-fork) | `ceb9e1f7a0180e61961bfde85d69d24e2dd84b39` | GPL-3.0-or-later. Vertex incorporates an earlier merge. Newer late settlement and parallel texture refresh changes overlap with Vertex's relay and worker. |
| [Linox-IDK/LNX-victory](https://github.com/Linox-IDK/LNX-victory) | `c04e6159c1452a24afa73e205a52b31bf0e9d24a` | GPL-3.0-or-later. Its reported performance uses settings including a Series S profile and upscaling. |
| [realblastex/flamingo](https://github.com/realblastex/flamingo) | `89e5d6c3d7029d6e8750ccdae8fbe2297f0f13f1` | GPL-3.0-or-later. Reported settings explicitly skip some pipelines. Its XCL-inspired work documents a distinction between ideas and code. |
| [xcl-dev/xcl](https://github.com/xcl-dev/xcl) | `b49be1836844fe0b0cdb3211dce3cb7fb4e21ac7` | GPL-2.0-only. Reference only. Its source was excluded from the GPLv3 integration. |
| [Marvisak/VIctory](https://github.com/Marvisak/VIctory) | `63bcca43c6a00a758510398fa43096ce7e4f7df0` | GPL-3.0-or-later. Prior Axiz upstream and comparison point. |
| [awesome-gta6-reverse-engineering](https://github.com/Ahmet-Dedeler/awesome-gta6-reverse-engineering) | `6648e173909aa4787568ef43f9c75f4e1ea2b2b1` | Discovery index with no repository license verified. No content copied. |

Vertex shares upstream merge base `ffe310eb2098e52169f4c44db5ffdabf43a72622`
with the newer Marvisak line. Vertex was 382 commits ahead and 479 behind that
line when selected. The original VIctory asset exception for the BCPack tables
still applies to the same tracked blob. The source history adds no new binary
assets. The benchmark, build products and logs stay local under `target`.

## Integrated subsystems

| Source in the preserved Vertex history | Subsystem present in this candidate |
| --- | --- |
| proneonn `d63ad3fe`, `368806f8`, `b577ff3a` and `65f7f236`; myokex, fxceless and soldsawn | Multi-fork GPU backend with direct-queue submission worker, copy queues, range-aware resource synchronization and staged shader readback. |
| fxceless `f85101b6`; Claude `ce304e56`; avt0m4t48 `8654c67a` | Copy-queue readback, fence-based handoff and propagation of copy-engine device loss. The GPU preset also permits copy-queue uploads. |
| proneonn `3e06f25e` from reddxd-byte/ShakiraRED-dt `c2271e5`; soldsawn `85a6ef00`; TG0000 `233a65f4` | Memory-range journal and guards, plus the heap texture upload worker and staging. |
| PastiiJay `b89bbc2f` from Geckomacabre/vx-layer `eee5667d`; proneonn `66f41e05` | Texture readback with console tiling and propagation of writes between aliases. |
| fxceless `465b94f5` | Per-list gathering of scattered indirect records. |
| Vertex shader and GPU backend; tippythetaidum `3562acbf` | Wave32 translation paths, and bit-exact handling of supported 16-bit typed views. These views preserve packed bits. Globally forcing native 16-bit math would change shader semantics. |
| Vertex GPU backend | Per-adapter descriptor limits, tracked in-flight resources and residency checks against DXGI budget. |

These components inherit the GPL-3.0-or-later project license. The small
TuxBox scalar bit-operation port is Apache-2.0 and carries its attribution and
full license text in [NOTICE](../NOTICE.md) and
[LICENSE-APACHE](LICENSE-APACHE.txt). The compatible Apache component remains
under its own notice. The texture-layout port from Simon's MiamiGame/XTP
`00c60fb197434c4987156779466a2ab41d5be2df` and
`188d895814255c72518c21d96994164072fac688` is GPL-3.0. Its AMD Mesa addrlib
patterns come from `265a2d4f0f842a28e0dfd150637fa91fb70345a2` under MIT.
See [the full AMD notice](LICENSE-AMD-ADDRLIB.txt). The lockfile uses crates.io
or workspace dependencies.

## Axiz ports and original work

- The four initial cherry-picks retain their origins in Git. They add synthetic
  texture and frame-interval tools (`9a190fcd` from `e04aea17`), reusable
  null descriptor templates (`232f7a0a` from `6f19b2f4`), required root
  constant initialization (`27177fa7` from `8f53bfde`) and lazy shader
  interpolation lookup (`bc574406` from `41136239`).
- The texture port carries the packed linear plane copy from `3d10a2ee` and
  its padding regression test. Vertex's tiled and XOR paths remain in use.
  The linear bounds checks already cover overflowing row and depth pitches.
- The signed packed-dot lowering and constant driver workaround are from
  KainanP `77434098`, with memoized analysis from Axiz `01806bde`. The
  hardware regression covers each constant class and run-time multiplicands.
- The explicit NVIDIA adapter option from Axiz `7dd9a868` now also applies to
  shader tests. Doctor reports supported wave widths and native 16-bit shader
  capability. The null sampler template uses the D3D12 sampler heap limit
  alongside Vertex's existing negotiated view heap limits.
- The old Axiz eviction sort and static descriptor-cap changes are superseded
  by Vertex's idle-resource traversal and adapter-specific negotiation. The
  old gather fixture belongs to the newer upstream shader layout and was
  left on the Axiz line.
- The pipeline manifest carries the half-view metadata needed to reconstruct
  translated shaders. Existing manifests deserialize with an empty list.
- Alpha presets preserve texture mip levels and disable the ambient-light
  substitute by default. They use blocking texture refresh on remap and
  retain the regular shader-write checks and pipeline wait budget. The ray
  tracing builder is allowed to run when its required pipelines translate.
  Translation and rendering gaps in Vertex can still affect the output.

## Validation

The base and candidate both produced locked Release builds. The candidate
runs `--help`, `--version` and `doctor` on Windows 11. The NVIDIA adapter
reported 11,944 MiB dedicated memory, Shader Model 6.8, 32-lane waves and
native 16-bit shader operations. The [benchmark table](../BENCHMARKS.md#vertex-alpha-comparison-2026-11-26)
covers generated CPU texture copies.

The final locked Release workspace suite at `f56b50f9` recorded 804
passes, zero failures and 14 inherited ignores. An additional diagnostics run on the
accepted source exercised the generated round trip on the WARP direct queue
and with both copy lanes enabled. The copy-queue run recorded 12 uploads and
12 readbacks, with no landing-buffer misses. The separate in-flight write
race test reproduced the intermediate readback and verified the corrected
final bytes. These results establish correctness for the tested cases on
WARP. They provide no hardware queue timings.

The Vertex baseline predates the prior Axiz branch's structured-view stride
fixture, `a_heap_buffer_read_by_element_takes_the_views_stride`. Its NVIDIA
failure remains open on the prior branch. The Vertex pass count is for the
Vertex test inventory and does not establish that the earlier failure is fixed.
Port the generated fixture to Vertex's resource-binding model before using
that case as an alpha release gate.

## Alpha gate

Use generated workloads on the RTX 5070 Ti Laptop GPU before ranking any
additional fork by performance. The next milestone is a repeatable
direct/copy-queue comparison with equivalent output, queue wait times,
shader pipeline stalls and peak DXGI local-memory budget usage. Full visual
parity, reliable cold and warm startup, stability across a sustained
12 GB VRAM workload and a 33.333 ms frame-time target remain open.
