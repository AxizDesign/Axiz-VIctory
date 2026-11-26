# Roadmap

1. Retain the pinned upstream release and verify changes with synthetic fixtures and the Windows test suite.
2. Resolve the remaining structured buffer read on RTX 5070 Ti Laptop GPU. Test raw-backed views against the existing stride oracle, including typed and writable views, overlaps, alignment and cache invalidation.
3. Capture comparable baseline and candidate runs on an authorized workload. Use release for FPS, then collect CPU, GPU, residency and copy diagnostics on separate runs to identify the dominant cost.
4. Work toward sustained 5, 10, 15, 20, 25 and 30 FPS on the same representative route. Skip demonstrated thresholds. At 30 FPS, inspect the distribution around 33.333 ms, outliers and sustained stability as well as the mean.

Candidate areas after profiling include texture upload and copy-back, pipeline creation and cache reuse, descriptor lifetime, and CPU/GPU synchronization. Keep each change independently reviewable.

Gate native FP16 behind an experimental option before measuring it in a representative workload. Retain the fused-rounding and output-modifier checks and compare independent shader and pipeline cache results.
