# Development log

## 2026-11-26

- Pinned upstream `63bcca43c6a00a758510398fa43096ce7e4f7df0` and saved a separately named release artifact. See [build instructions](BUILD_INSTRUCTIONS.md).
- Fetched eight GPL workspace forks and compared their source and divergence. Reused brainbudt's safe-Rust texture copy optimization in `38985da7` with attribution to the original commit. See [optimization matrix](OPTIMIZATION_MATRIX.md).
- Added packed linear plane copies and regression coverage in `3d10a2ee`. Added the synthetic copy harness and frame interval analyzer in `e04aea17`. See [benchmarks](BENCHMARKS.md).
- Identified the default Intel adapter on the hybrid laptop and added opt-in NVIDIA selection in `7dd9a868`. The opt-in test checked the actual device LUID. See [NVIDIA compatibility](NVIDIA_COMPATIBILITY.md).
- Both release variants compiled and passed CLI smoke tests. Clippy passed with default and all features. The baseline and NVIDIA-opt-in candidate workspace runs reported the same five failed tests, with 867 and 872 passes respectively in the Cargo logs, including the nested fixture runner. See [known issues](KNOWN_ISSUES.md).

Work branch is `codex/texture-copy`. The compiled source commit is `7dd9a868`. The following documentation commit records the baseline and validation. The next engineering milestone is to isolate descriptor heap sizing and shader test failures on the target GPU.
