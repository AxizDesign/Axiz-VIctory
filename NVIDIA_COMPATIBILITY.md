# NVIDIA compatibility

Local discovery on 2026-11-26 found an NVIDIA GeForce RTX 5070 Ti Laptop GPU with driver 621.14 and 12,227 MiB reported by NVIDIA SMI. NVIDIA documents the laptop SKU as Blackwell with 12 GB GDDR7. DXDiag reported D3D12 DDI 12, feature level up to 12_2 and WDDM 3.2 for the NVIDIA adapter. Vulkaninfo reported Vulkan 1.4.357 for that adapter. The host also exposes Intel integrated graphics, so capture metadata should identify the adapter used.

A local D3D12 API probe opened the RTX explicitly. It returned Shader Model 6.8 (probe ceiling), resource binding tier 3, 32-lane waves, native 16-bit shader operations, raytracing tier 1.2, mesh shader tier 1, and a point-in-time DXGI local-memory budget of 11,176 MiB. NVIDIA SMI reported compute capability 12.0.

The default D3D12 adapter resolved to Intel Graphics in the probe. Set `VICTORY_D3D12_NVIDIA=1` for Axiz-VIctory to try compatible NVIDIA adapters in DXGI order. Mode selection is once per process. The log names the NVIDIA adapter when selected and reports fallback to the system default when none can be used. The diagnostics WARP switch takes precedence. Shader crate hardware tests use their own adapter selection.

The CPU is an x86-64 Intel Core Ultra 9 275HX with 24 cores and 24 logical processors. Windows 11 Pro is build 26300. The OS reports 33,752,997,888 bytes of physical memory. Drive C had about 300 GiB free. The automatically managed pagefile showed a 2 GiB allocation and zero reported use at inspection.

Git 2.56.1 and GitHub CLI 2.105.0 were available, and authenticated GitHub API and HTTPS fetch succeeded. Cargo, rustc, MSVC, the Windows SDK, DXDiag, Vulkaninfo, NVIDIA SMI and Windows Performance Recorder were available. Nsight Graphics and Nsight Systems were not found on PATH or in the standard NVIDIA installation directory checked.

The texture-copy path changes host CPU work. Shader execution, occupancy, register pressure, queues, VRAM residency and pipeline behavior require adapter-specific captures before choosing an NVIDIA profile. The upstream runtime queries D3D12 capabilities in `crates/victory-gpu/src/host.rs`.

[NVIDIA RTX 50 Series Laptop specifications](https://www.nvidia.com/en-gb/geforce/laptops/50-series/)
