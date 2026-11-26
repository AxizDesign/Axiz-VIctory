# NVIDIA compatibility

Local discovery on 2026-11-26 found an NVIDIA GeForce RTX 5070 Ti Laptop GPU with driver 621.14 and 12,227 MiB reported by NVIDIA SMI. NVIDIA documents the laptop SKU as Blackwell with 12 GB GDDR7. DXDiag reported D3D12 DDI 12, feature level up to 12_2 and WDDM 3.2 for the NVIDIA adapter. Vulkaninfo reported Vulkan 1.4.357 for that adapter. The host also exposes Intel integrated graphics, so capture metadata should identify the adapter used.

A local D3D12 API probe opened the RTX explicitly. It returned Shader Model 6.8 (probe ceiling), resource binding tier 3, 32-lane waves, native 16-bit shader operations, raytracing tier 1.2, mesh shader tier 1, and a point-in-time DXGI local-memory budget of 11,176 MiB. NVIDIA SMI reported compute capability 12.0.

The default D3D12 adapter resolved to Intel Graphics in the probe. Set `VICTORY_D3D12_NVIDIA=1` for Axiz-VIctory to try compatible NVIDIA adapters in DXGI order. The runtime log names the adapter when selected and reports fallback to the system default when none can be used. The diagnostics WARP switch takes precedence. Shader validation now honors the same NVIDIA preference and prints the actual hardware adapter in tests. The RTX returned true for both typed-resource and descriptor-heap 64-bit atomics. Intel returned false for the typed-resource capability.

The CPU is an x86-64 Intel Core Ultra 9 275HX with 24 cores and 24 logical processors. Windows 11 Pro is build 26300. The OS reports 33,752,997,888 bytes of physical memory. Drive C had about 300 GiB free. The automatically managed pagefile showed a 2 GiB allocation and zero reported use at inspection.

Git 2.56.1 and GitHub CLI 2.105.0 were available, and authenticated GitHub API and HTTPS fetch succeeded. Cargo, rustc, MSVC, the Windows SDK, DXDiag, Vulkaninfo, NVIDIA SMI and Windows Performance Recorder were available. Nsight Graphics and Nsight Systems were not found on PATH or in the standard NVIDIA installation directory checked.

Wave32 is the only wave size reported by the RTX. Shader Model 6.6 validates required wave sizes when creating a pipeline, so changing the guest wave width requires checking cross-lane and mask semantics. Native FP16 also needs a runtime fused-rounding probe, correct fallbacks for output modifiers and distinct translation cache keys. The external experiment uses those checks. See [benchmarks](BENCHMARKS.md) for measured compute-chain results. [Wave size specification](https://microsoft.github.io/DirectX-Specs/d3d/HLSL_SM_6_6_WaveSize.html) · [Typed and heap atomics specification](https://microsoft.github.io/DirectX-Specs/d3d/HLSL_SM_6_6_Int64_and_Float_Atomics.html)

Occupancy, register pressure, queue stalls, VRAM residency and pipeline behavior still require representative captures. The runtime queries D3D12 capabilities in `crates/victory-gpu/src/host.rs`.

[NVIDIA RTX 50 Series Laptop specifications](https://www.nvidia.com/en-gb/geforce/laptops/50-series/)
