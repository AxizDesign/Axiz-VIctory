# Development log

## 2026-11-26

- Pinned upstream `63bcca43c6a00a758510398fa43096ce7e4f7df0` and saved a separately named release artifact. See [build instructions](BUILD_INSTRUCTIONS.md).
- Fetched eight GPL workspace forks and compared their source and divergence. Reused brainbudt's safe-Rust texture copy optimization in `38985da7` with attribution to the original commit. See [optimization matrix](OPTIMIZATION_MATRIX.md).
- Added packed linear plane copies and regression coverage in `3d10a2ee`. Added the synthetic copy harness and frame interval analyzer in `e04aea17`. See [benchmarks](BENCHMARKS.md).
- Identified the default Intel adapter on the hybrid laptop and added opt-in NVIDIA selection in `7dd9a868`. The opt-in test checked the actual device LUID. See [NVIDIA compatibility](NVIDIA_COMPATIBILITY.md).
- Both release variants compiled and passed CLI smoke tests. Clippy passed with default and all features. The baseline and NVIDIA-opt-in candidate workspace runs reported the same five failed tests, with 867 and 872 passes respectively in the Cargo logs, including the nested fixture runner. See [known issues](KNOWN_ISSUES.md).

### Blackwell follow-up

- Saved the starting Release executable and PDB under `target/baseline/0b366c6b`.
- Aligned descriptor heap sizes with the translator bound, retained the original assertions, and measured reusable null templates on the RTX. The descriptor work extends `codex/texture-copy`.
- Stacked `codex/blackwell-shaders` on the descriptor branch. It adds constant-dot fixes and bounded memoization, identifies the validation adapter, initializes the mandatory root dword and corrects the gather input view. The compiled source is `01806bde`.
- Verified that all five original problem tests run successfully on the RTX. The remaining structured read failure also reproduced on the preserved baseline with only the test-adapter selector applied. The zero-count root-constant crash reproduced there as well and the current dispatch passes.
- The full Release workspace run with NVIDIA selected recorded 878 passes, one failure and 13 ignored tests, excluding the nested fixture runner. All-features Clippy passed. The single failing shader assertion remained enabled.
- Measured native FP16 in a separate source checkout. See [benchmarks](BENCHMARKS.md) for GPU timestamps, memory calculations and the shader compilation microbenchmark.

### Cool Fox and residency follow-up

- Recovered the licensed Cool Fox snapshot from two public forks and compared its RTX 5090 reports and renderer settings. See the [source and benchmark review](OPTIMIZATION_MATRIX.md#cool-fox-source-and-rtx-claims).
- Adapted the fork's interpolation-analysis shortcut in `41136239` and independently skipped unnecessary texture age sorting in `887b1c72`. Kept the prior Release at `target/validated/01806bde`. The 18 resource tests and five interpolation tests passed. The two synthetic benchmark fixtures ran separately.
- Next review the copy-queue descriptor publication and pool backoff in isolation, using the existing RTX structured-buffer failure and explicit resource lifetime tests as correctness gates. Record budget and residency on the 12 GB GPU before choosing a policy for representative workloads.
