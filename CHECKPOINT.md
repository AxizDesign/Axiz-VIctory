# Continuation checkpoint

Resume on `codex/residency-efficiency`. The verified stack is [#1 texture copy and descriptor heaps](https://github.com/AxizDesign/Axiz-VIctory/pull/1) at `c5c10302`, [#2 Blackwell shader fixes](https://github.com/AxizDesign/Axiz-VIctory/pull/2) at `4263a2c8`, and [#3 shader I/O and residency efficiency](https://github.com/AxizDesign/Axiz-VIctory/pull/3) with implementation `887b1c72` and prior documentation `ed65e23f`. All three are open drafts. GitHub reported no check runs at this checkpoint.

## Verified local state

- Latest Release is `target/validated/887b1c72/victory.exe`. It reports `victory 0.1.0`. SHA-256 is `1f4a4e65b54c854cf909a7734360bea92d049dac4307bc263bb8de3353191725`.
- Preserved Release builds are `target/validated/01806bde/victory.exe`, `target/baseline/0b366c6b/victory.exe`, and `target/baseline/63bcca4/victory.exe`. Their hashes match [the build record](BUILD_INSTRUCTIONS.md).
- Existing logs confirm 18 resource and five interpolation tests passed for the latest code. The earlier full Blackwell Release run had 878 passes, 13 ignored tests, and one inherited structured-buffer stride failure. Synthetic benchmark results are in [BENCHMARKS.md](BENCHMARKS.md). Gameplay frame times on the target machine are still unmeasured.

## Next high-impact candidates

1. **Fix the RTX structured-buffer stride failure first.** Start with `a_heap_buffer_read_by_element_takes_the_views_stride` under `VICTORY_D3D12_NVIDIA=1`. Review [upstream #2178](https://github.com/Marvisak/VIctory/pull/2178) for raw-backed views. Check logical stride, typed and writable views, bounds, placement alignment, write tracking, and shader cache identity. Keep the existing assertion.
2. **Reduce upload stalls within the RTX's 12 GB VRAM budget.** Isolate Cool Fox's deferred descriptor publication and allocation backoff, using the [pinned source review](OPTIMIZATION_MATRIX.md#cool-fox-source-and-rtx-claims). First verify with a synthetic fence-and-readback test that completed uploads publish the original bytes and in-flight resources survive until completion. Measure DXGI budget, upload stalls, and frame-time tails independently.
3. **Evaluate optional native FP16 or pipeline warm-up after correctness.** Keep native FP16 behind an experimental switch with fused-rounding checks, output-modifier fallback, and isolated cache keys. Choose between this and pipeline warm-up from a representative profile.

Keep resolution, effects, mip selection, lighting, and shader accuracy fixed when comparing builds. Use authorized workloads or generated fixtures. The reported Cool Fox 25 FPS remains unverified at comparable fidelity. See [KNOWN_ISSUES.md](KNOWN_ISSUES.md) for blockers and [ROADMAP.md](ROADMAP.md) for longer-term gates.

To reproduce the known stride failure from the workspace root:

```powershell
$env:VICTORY_D3D12_NVIDIA='1'
cargo test --release --locked -p victory-shader --features host --lib a_heap_buffer_read_by_element_takes_the_views_stride -- --nocapture
```

Confirm that the test prints the RTX 5070 Ti Laptop GPU as its hardware adapter.
