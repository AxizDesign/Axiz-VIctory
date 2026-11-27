# Windows builds

Use the pinned Rust 1.98.1 toolchain for `x86_64-pc-windows-msvc`, the Visual Studio C++ build tools, and a Windows SDK. The verified host uses MSVC tools 14.44.35207 (CL 19.44.35230) and SDK 10.0.26100.0. Cargo resolves the checked-in lockfile.

```powershell
cargo build --release --locked -p victory
cargo build --profile diag --locked -p victory --features diagnostics
cargo test --workspace --locked --no-fail-fast -- --test-threads=1
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo xtask policy
```

See the [architecture](docs/architecture.md) and [asset policy](docs/asset-policy.md) for actual runtime inputs. The CLI smoke checks, self-contained tests and CPU benchmarks use no title files.

| Profile | Local artifact | Source |
| --- | --- | --- |
| Pinned upstream release | `target/baseline/63bcca4/victory.exe` | `63bcca43c6a00a758510398fa43096ce7e4f7df0` |
| Previous candidate release | `target/baseline/0b366c6b/victory.exe` | `7dd9a868959d6389596a1824e04a199c474ab630` |
| Previous candidate diagnostics | `target/diag/victory.exe` | `7dd9a868959d6389596a1824e04a199c474ab630` |
| Blackwell candidate release | `target/validated/01806bde/victory.exe` | `01806bde5bf056557637b7e629f5aedb1ff0dc60` |
| Residency candidate release | `target/validated/887b1c72/victory.exe` | `887b1c72b97eda25664277888d60c99b9e72e22f` |

The local builds dated 2026-11-26 retain matching PDB files. They depend on Windows D3D12 for GPU operation. The diagnostics profile adds instrumentation and should be used separately from release frame-pacing captures.

SHA-256 values for the local executables:

| Artifact | SHA-256 |
| --- | --- |
| Baseline release | `303520a2e0ef216abb5192e0c9426fec2fbf113e6bcead52235f1ed6f032884e` |
| Previous candidate release | `9e4abb9b66d9902c939350a061d8b39728bf7fb798ac6c2b3614327fadffc1e8` |
| Previous candidate diagnostics | `46e45b40ddba98ea0e2ca7677af37e2ef9c91ed44219805a331680c6b74a5cf6` |
| Blackwell candidate release | `8a89b1164c54f987ac805060fdd0fa967834bd59d1287cfeed729a6502805d1e` |
| Residency candidate release | `1f4a4e65b54c854cf909a7734360bea92d049dac4307bc263bb8de3353191725` |

For the hybrid laptop, set `$env:VICTORY_D3D12_NVIDIA='1'` before launching the candidate or running the workspace tests against the RTX.

## Vertex alpha candidate

The Release executable and PDB for the integration candidate are under
`target/validated/f56b50f9/`. The source commit is
`f56b50f9c605b7b9272e17913c8fd9942ef3df2a`. The executable SHA-256 is
`9ce99bbf576520c6bd5dde84293fd1c02483be7f7ee67e30b91c45d4b5621ffd`.
The pinned Vertex Release and the comparison benchmark are archived under
`target/baseline/33f26c4e/`. Build artifacts are local.

```powershell
$env:VICTORY_D3D12_NVIDIA = '1'
cargo build --release --locked -p victory
& ./target/release/victory.exe doctor
cargo test --release --locked --workspace --no-fail-fast -- --test-threads=1
```

## Pipeline scheduling candidate

Source `45049c1b` builds with `cargo build --release --locked -p victory`.
The archived executable and matching PDB are in `target/validated/45049c1b/`.
Executable SHA-256
`7624d956a03303582efb98ac7c445d451c87754c2cadd5dc3f8e71f75a684839`.
The earlier Vertex integration Release remains in `target/validated/f56b50f9/`.

## Streaming stability candidate

Build source `4e0a324d` uses `cargo build --release --locked -p victory`.
The executable and matching PDB are archived at `target/validated/4e0a324d/`.
The executable SHA-256 is
`83ddf8ca33d2c04a5a8e3483aa2f5a97681b74b6cb4af5f28a083252b736f34a`.
The preceding candidate remains at `target/validated/45049c1b/`.

## Mixed-frame latency candidate

Release source `af2be548` builds with `cargo build --release --locked -p victory`.
The executable and matching PDB are under `target/validated/af2be548/`.
Executable SHA-256
`1ac1287f69277b3a09161d72b9d75bf9bb6410b2c7c4f097305bd2b108ebba53`.
The previous `target/validated/4e0a324d/` Release is preserved.
