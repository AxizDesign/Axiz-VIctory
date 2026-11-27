# Optimization matrix

For the Vertex alpha branch, see the [integration and provenance matrix](docs/axiz-alpha-integration.md). The historical Axiz matrix follows.

Pinned comparison base is [Marvisak/VIctory at 63bcca4](https://github.com/Marvisak/VIctory/tree/63bcca43c6a00a758510398fa43096ce7e4f7df0). Cargo declares GPL-3.0-or-later for the project. The upstream asset policy records a BCPack table exception outside that grant.

## Fork comparison

Heads were fetched and compared with the pinned base on 2026-11-26. Behind/ahead counts use the merge base and include merges. Reported gameplay results are third-party claims with differing settings and workloads.

| Repository | Head (commit UTC) | Behind/ahead | Source review and reported evidence | NVIDIA applicability and integration |
| --- | --- | ---: | --- | --- |
| [DexNom/VIctory](https://github.com/DexNom/VIctory/tree/64bd58f5fb794216894bc0e33e03a3a64d19c347) | `64bd58f`, Nov 26 01:12 | 160/19 | Separate launcher, warm-up and FSR profiles. BENCHMARK.md reports an indoor 3.52 to 4.05 FPS comparison over 93 matched presents on RX 7900 XTX, with divergent outdoor runs. | Portable copy/readback ideas merit review individually. Preset changes and launch tooling add dependencies. |
| [JMAN730/project-flamingo](https://github.com/JMAN730/project-flamingo/tree/ef535754d4298cea962a9a973a152ddfb6a07a2d) | `ef53575`, Nov 26 07:11 | 479/1138 | Parallel refresh, vectorized copies, upload retirement, and wave32 work across a wide merge queue. README describes RTX 2070 SUPER tests with ~7.3/8 GB VRAM and device loss 30–45 seconds after entering the world, plus a separate RTX 4080 Proton run. | Isolate each mechanism. Memory ordering and lifetimes make a wholesale merge high risk. |
| [Steenax6/VIctory-NVID](https://github.com/Steenax6/VIctory-NVID/tree/d787d905aa594bd1bfeff7643fe1833368235995) | `d787d90`, Nov 26 02:54 | 479/196 | Card-local buffers, texture promotion and residency policies. Switch notes report 78.6 ms in the first room and 77 ms in a 22-minute open-world run on a Windows 11 RTX 4070 Ti 12 GB. | Comparable VRAM class. Streaming policy changes texture quality and is coupled to descriptor/resource state. |
| [fxceless/victory-fork](https://github.com/fxceless/victory-fork/tree/46bf5351707b1697c04e5d871c30973ebaa3e973) | `46bf535`, Nov 26 05:52 | 479/105 | Separate instrumentation for remapping and texture worker. Run 309 includes 30.2 FPS at seven draws per frame, insufficient evidence of open-world performance. | Useful phase instrumentation. Validate thread ownership and queue ordering before taking worker code. |
| [uiliannn/VItoriaBr](https://github.com/uiliannn/VItoriaBr/tree/1361e190b7f46df996f00ea1b540ecf66785127f) | `1361e19`, Nov 26 04:09 | 479/169 | Dynamic descriptor slots, barriers and second present plane. README reports 4–6 FPS on the preset and a separate experimental 22 FPS at 1080p on RTX 5060 Ti 16 GB. | Inspect descriptor provenance and bounds independently of renderer presets. Experimental results lack a comparable local capture. |
| [WillyboPeep/victory-all-forks](https://github.com/WillyboPeep/victory-all-forks/tree/8b8319c0ac550f962784d286053aac77cd3be9dc) | `8b8319c`, Nov 25 09:04 | 479/273 | Integration ledger tracking sources and switches. Reports 4.9 to 5.2 FPS at ~5,600 draws/frame on RTX 4090 24 GB with three switches combined. | Use the ledger to trace individual changes. Combined switch result does not isolate a cause. |
| [trustno1-coder/VIctory-NVIDIA](https://github.com/trustno1-coder/VIctory-NVIDIA/tree/033be27f336e395745667df5fe20160e375209ca) | `033be27`, Nov 26 03:41 | 479/420 | Adapter selection by description, dynamic descriptors and GPU compares. Reports 11 to 21.8 FPS on RTX 5070 12 GB in the prologue using a Series S profile at half scale, custom upscaling and skipped ray walks. | Dynamic descriptor work is the origin of the VItoriaBr variant. The quality changes make the reported speed preset unsuitable as a baseline. |
| [Xeltax/VIcky](https://github.com/Xeltax/VIcky/tree/abafd94aaad6d21c4840478e9f3ffeb09ffa3d6e) | `abafd94`, Nov 25 08:40 | 594/243 | Descends from fxceless, adds heap-cap and slot-distribution diagnostics and adjusts a shader cache key. README reports nine-minute runs from one save at 3.5–4.5 FPS in Series X mode and 5.7–6.0 in Series S mode on RTX 4080 16 GB. | Use slot-distribution instrumentation as reference. Cross-profile FPS changes cannot isolate an optimization. |

The eight fetched heads declare GPL-3.0-or-later in their workspace manifests. Verify asset provenance and any component-specific license before importing. GitHub API returned 404 for the previously discussed `Zettrox/ztrx`. No source or license was inspected for that URL.

## Candidates and validation

| Area | Author and source | Mechanism and expected effect | Dependencies, risks and validation | Status |
| --- | --- | --- | --- | --- |
| CPU, allocation, streaming | brainbudt, [d331047](https://github.com/Marvisak/VIctory/pull/2736) | Derive contiguous swizzle runs, pair rows and visit depth slabs while cache lines are hot. | Safe Rust, works on host CPU independent of adapter. Verify per-element mapping, padding, tails and errors in both directions. Interacts with #2026. | Validated on synthetic copies |
| CPU, streaming | Axiz-VIctory, `3d10a2ee` | Copy packed linear rows in one span per depth slice, reducing per-row copies. | Safe Rust, no dependency. Validate padded rows and per-mip/depth gaps. | Validated on synthetic copies |
| CPU, streaming | 0xkn0wh3r3, [1ec3877](https://github.com/Marvisak/VIctory/pull/2026) | Check bounds once per row, then copy individual runs through pointers. | GPL-compatible Rust, overlapping edits with #2736. Requires verifying the run bounds for unchecked copies. | Rejected for this integration in favor of tested safe slices |
| CPU, threading, command submission | fever308, [b975c652](https://github.com/JMAN730/project-flamingo/commit/b975c652), and fxceless, [d4a528ba](https://github.com/fxceless/victory-fork/commit/d4a528ba) | Parallelize refresh and combine residency queries, or dedicate a texture upload worker. | Windows APIs and current GPU state ownership. Isolate scheduling changes and test concurrent writes, queue waits and GPU hashes. | Under investigation |
| CPU/GPU synchronization, lifetime, allocation | fever308, [694695c2](https://github.com/JMAN730/project-flamingo/commit/694695c2) | Batch retirements, page kept-resource tracking and reuse upload storage. | D3D12 fences and barriers. Exercise multiple queues, resets and retire-on-completion. | Discovered |
| VRAM, rendering, streaming | Steenax6, [83e2ba51](https://github.com/Steenax6/VIctory-NVID/commit/83e2ba51) | Start sampling at the first available level while textures stream in. | Verify fallback, binding clamps and image correctness with authorized workloads. Avoid forced mip dropping in default profiles. | Experimental |
| VRAM, RAM, graphics API overhead | uiliannn, [e40fba44](https://github.com/uiliannn/VItoriaBr/commit/e40fba44) | Dynamic descriptors within a capped heap. | D3D12. Compare with upstream's capacity calculation and shader index clamps. Verify no overlaps, rollover or early release. | Under investigation |
| Stability, error handling | Axiz-VIctory, `7dd9a868` | Opt-in NVIDIA D3D12 adapter selection avoids unintended use of integrated graphics on hybrid systems. | DXGI enumeration and feature-level 12_0. Verified the actual runtime device LUID on the RTX 5070 Ti Laptop GPU. | Validated for adapter selection |
| GPU, shader execution | 0xkn0wh3r3, [47078d7](https://github.com/Marvisak/VIctory/pull/2825) | Emit native half arithmetic on devices that pass the capability and fused-rounding checks. GPL-3.0-or-later source tested in a separate checkout. | Three synthetic RTX chains ran 7.4–27.6× faster and matched per-thread outputs. Preserve output-modifier fallback and cache-key separation. See [benchmarks](BENCHMARKS.md). | Experimental |
| Shader compilation and cache | [Upstream #1844](https://github.com/Marvisak/VIctory/issues/1844) | Investigate whether host wave settings affect cache identity across adapters. | Check current key fields, invalidate only when generation inputs change, and test cold versus warm bytes. | Under investigation |
| Pipeline cache, filesystem I/O | [Upstream architecture](https://github.com/Marvisak/VIctory/blob/63bcca43c6a00a758510398fa43096ce7e4f7df0/docs/architecture.md) | Existing content-addressed shader cache and D3D12 pipeline library. Measure startup cache hits, conversion work and disk reads separately. | Pipeline library is adapter/driver dependent. Compare cold/warm captures with the same inputs. | Discovered |
| Frame pacing, stability, error handling | Axiz-VIctory, `e04aea17` | Offline interval summary exposes budget misses and tail latency. Runtime diagnostics already capture device removal and submission timing. | One exported interval per frame. Separate release capture from diagnostic instrumentation. | Implemented |
| VRAM, RAM, descriptors | pes7, [541a3203](https://github.com/Marvisak/VIctory/pull/2683), and Axiz-VIctory `f5a13995` | Portable one-million-slot bound shared by shaders and host heaps, reserved null slots and pairs. GPL-3.0-or-later. | Original failing assertions pass. Full initialization costs more for small view heaps, while stride and mark buffers shrink. Oversized guest heaps still need remapping. | Validated |
| CPU, D3D12 allocation | Axiz-VIctory `6f19b2f4` | Reuse immutable CPU-only null templates per heap type, with a sampler-sized template. | Distinct destination heaps, initialization serialized once per template. Benchmarked both heap types. | Validated |
| Shader correctness | KainanP, [9ea9e00d](https://github.com/Marvisak/VIctory/pull/2408), GPL-3.0-or-later | Lower constant packed dot operands into wrapping signed byte operations. Dynamic operands still use packed hardware dot. | Original FSPX fixture passes on the Intel and NVIDIA adapters. Added constant equivalence tests with input classes from the PR. | Validated |
| Shader compilation, CPU | Axiz-VIctory `01806bde` | Memoize proven constant values for repeated DXIL subexpressions at the same remaining analysis depth. | Retains the depth bound and evaluation semantics. Compare the same graph before and after. | Validated |
| Shader validation | Bullet18x, [9c6e0942](https://github.com/Marvisak/VIctory/pull/2323), GPL-3.0-or-later | Provide a raw view to a test using byte-addressed gather inputs. | Fixture matches the translated shader contract and retains the per-lane output assertions. | Validated |
| RAM and allocation | 0xKaji, [98c38907](https://github.com/Marvisak/VIctory/pull/2971), GPL-3.0-or-later | Sparse host-slot tables and deferred upload buffers. | Evaluate hot-path lookup cost and lifetime handling after resolving structured views. | Under investigation |
| Shader execution, buffer bindings | Nikita Romanovich, [319749e4](https://github.com/Marvisak/VIctory/pull/2178), GPL-3.0-or-later | Back structured guest buffers with raw host views and explicitly compute byte offsets. | Prototype needs tests for bounds, descriptor format, copy alignment and cache separation. The original failing stride assertion stays enabled. | Under investigation |

## Cool Fox source and RTX claims

GitHub API and Git both returned 404 for `codec-d/cool_fox`. The accessible [UnkwnDev fork](https://github.com/UnkwnDev/cool_fox/tree/f4b48ad766ddbec1a1867022cd42f7645b0dd55e) and [pop1022 fork](https://github.com/pop1022/cool_fox/tree/f4b48ad766ddbec1a1867022cd42f7645b0dd55e) both exposed the same commit authored by codec_d. Its workspace manifest declares GPL-3.0-or-later, with the documented inherited asset exception. This snapshot is six commits ahead of its `0a435665` base in [avatarmanz/VIctory-7900XTX](https://github.com/avatarmanz/VIctory-7900XTX/commit/0a435665), and 414 commits are unique to the mirror relative to Axiz-VIctory `4263a2c8`, with 494 unique to Axiz-VIctory. The s-teamplus fork adds launcher configuration, including a different SDK and disabled texture pooling.

The [fork's journal](https://github.com/UnkwnDev/cool_fox/blob/f4b48ad766ddbec1a1867022cd42f7645b0dd55e/docs/bug-journal.md) describes an RTX 5090 comparison at 64.2 to 54.8 ms median across 15 title-bar intervals per run, with a sampled peak of 17.45 GiB of whole-adapter memory use in the candidate. A separate confirmation reports 54.3 ms median across 120 high-draw intervals. Capture receipts are referenced outside the Git tree. These figures do not establish 25 FPS at comparable fidelity. The [launcher](https://github.com/UnkwnDev/cool_fox/blob/f4b48ad766ddbec1a1867022cd42f7645b0dd55e/Play-VIctory.cmd) selects Series S and overrides ambient lighting parameters.

Cool Fox uses asynchronous submission, a copy queue with deferred descriptor publication, explicit upload pooling, and separate RAM/commit-pressure logic. Axiz-VIctory currently waits before its next eviction pass, stamps texture reads, and gives up idle or least-recently-used resources. Deferred publication and pool backoff from [ab75ac3e](https://github.com/UnkwnDev/cool_fox/commit/ab75ac3e) depend on the fork's broader upload and fence ownership model. Evaluate those separately with byte-preserving eviction and in-flight lifetime tests.

| Source | Disposition |
| --- | --- |
| jaschinski97-ops, [81a8e2e3](https://github.com/UnkwnDev/cool_fox/commit/81a8e2e3), GPL-3.0-or-later | Adapted the skip-unused-interpolation-analysis idea. Axiz-VIctory now builds SSA indexes only when pairing interpolation instructions. |
| pes7, [3729ba04](https://github.com/UnkwnDev/cool_fox/commit/3729ba04), GPL-3.0-or-later | Opcode-name caching was already present in the pinned Axiz-VIctory source. |
| Axiz-VIctory `887b1c72` | Skip sorting active texture ages when the eviction policy only selects idle textures. Preserve idle order and stable least-recently-used ordering under pressure. |
| [Linox-IDK/LNX-victory at c04e6159](https://github.com/Linox-IDK/LNX-victory/tree/c04e6159c1452a24afa73e205a52b31bf0e9d24a), GPL-3.0-or-later | A separate low-VRAM NVIDIA reference. Its headline FPS uses a Series S/FSR profile, so that profile was excluded from this fidelity-preserving pass. |
| [XCL at b49be183](https://github.com/xcl-dev/xcl/tree/b49be1836844fe0b0cdb3211dce3cb7fb4e21ac7), GPL-2.0-only | Separate HLSL-to-DXIL pipeline and offline compute pipelines. Its NVIDIA reference distinguishes 3D gameplay from the menu-inclusive average. Architecture reference only. |

## Pipeline scheduling candidate

See the [pipeline scheduling integration record](docs/pipeline-scheduling-integration.md)
for the Belobug and CGBJ archive hashes, public source pins, license review
and exact authors. The candidate adds slow speculative pipeline scheduling
and a local-only DXGI pressure query for residency decisions.

## Streaming stability

The [streaming integration record](docs/streaming-stability.md) tracks
Cool Fox heap-growth backoff (`ab75ac3e`), the new empty-heap reclamation
policy, and memory-gated speculative PSO helpers. It also records the
deferred Belobug texture upload prelude and Cool Fox bindless changes.

## Exact-key D3D12 pipeline coalescing

`382b5524` adds original Axiz PSO work sharing around Vertex's persistent
D3D12 pipeline library. `af2be548` keeps it opt-in after the single-request
control. See [mixed-frame results](docs/mixed-frame-latency.md). This change
uses no newly imported fork implementation.

## D3D12 runtime stability

- Ported fever308's cached UAV capability query from
  `28f070b5f27af419d96abb91933a176f65d0627d` (GPL-3.0-or-later). The NVIDIA
  fixture confirmed that the old path accepted a typeless texture UAV.
  The fixed path rejects it before descriptor creation and accepts the
  matching supported typed view.
- Ported FictionHub's first-uncompleted breadcrumb formatter and test
  from `787c7b43a7ed8664f4bd92a30b96066ce5eddd88` (GPL-3.0-or-later).
- Axiz changes enable full state and synchronized queue validation when GBV
  is requested and check direct fence completion before publishing results.
  Neither the observed device hang nor the preceding over-limit descriptor
  access has been reproduced with the generated workloads.

## CPU streaming

- Original Axiz change `419adcd2` bounds texture selection and combines
  selection with the queue census. It preserves the existing service order.
- `5d7cacd6` adapts the section commit and placeholder reuse in
  `49d71266b25f8d432cfc86ec3d27c304a5191e66` (Belobug and Cool Fox,
  `local <local@localhost>`, GPL-3.0-or-later).
- The same commit incorporates remap instrumentation from fxceless
  `d8986cd7b8392f85478f5675933c59712519e163` and the diagnostic walk cadence
  from Belobug `fb28d49e959359cb20992e5c5533a647cb408054`
  (`local <local@localhost>`). Both components are GPL-3.0-or-later.
- The [CPU streaming report](docs/cpu-streaming.md) records the benchmarks,
  exact baseline, imported code and remaining profiling gaps.
