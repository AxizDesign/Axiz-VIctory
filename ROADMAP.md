# Roadmap

1. Retain the pinned upstream release and verify changes with synthetic fixtures and the Windows test suite.
2. Reproduce and isolate the descriptor heap and shader failures on the RTX 5070 Ti Laptop GPU. Track the host heap capacity and shader index range together.
3. Capture comparable baseline and candidate runs on an authorized workload. Use release for FPS, then collect CPU, GPU, residency and copy diagnostics on separate runs to identify the dominant cost.
4. Work toward sustained 5, 10, 15, 20, 25 and 30 FPS on the same representative route. Skip demonstrated thresholds. At 30 FPS, inspect the distribution around 33.333 ms, outliers and sustained stability as well as the mean.

Candidate areas after profiling include texture upload and copy-back, pipeline creation and cache reuse, descriptor lifetime, and CPU/GPU synchronization. Keep each change independently reviewable.
