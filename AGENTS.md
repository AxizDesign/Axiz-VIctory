# Axiz-VIctory development

- Preserve the upstream [asset policy](docs/asset-policy.md). Use generated fixtures and keep captures, caches and proprietary files outside the repository.
- Record the baseline, build configuration, workload and actual measurements before claiming a performance change. Keep gameplay, external reports and synthetic measurements distinct.
- Check the target D3D12 adapter and supported capabilities before introducing a hardware-specific path. Keep rendering and synchronization semantics covered by regression tests.
- Record imported code's author, source SHA and license in [the optimization matrix](OPTIMIZATION_MATRIX.md). Prefer small, attributable commits and retain the pinned upstream reference.
- See [build instructions](BUILD_INSTRUCTIONS.md), [benchmarks](BENCHMARKS.md) and [known issues](KNOWN_ISSUES.md) for validation.
