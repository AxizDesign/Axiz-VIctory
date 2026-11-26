Third-party notices
===================

Portions of `crates/victory-shader` are a port of TuxBox
(https://github.com/brcly/TuxBox), Copyright (c) 2026 the TuxBox authors,
licensed under the Apache License, Version 2.0
(http://www.apache.org/licenses/LICENSE-2.0).

* `crates/victory-shader/src/emit/lower.rs` (and its tests in
  `src/emit/valu_tests.rs`): the scalar bit searches `s_flbit_i32_b64`,
  `s_ff0_i32_b32`, `s_ff0_i32_b64` and the zero counts `s_bcnt0_i32_b32`,
  `s_bcnt0_i32_b64`, ported from TuxBox commit 1a50eb9 ("shader: restore
  s_flbit_i32_b64 and s_ff0_i32 lowering dropped by a merge") and its
  scalar bit-operation lowering (`tools/shader-compiler/src/hlsl.rs`,
  `tools/shader-compiler/src/hlsl/scalar_bits.rs`).

Marked in the source at each ported site with the upstream commit.

The complete Apache License 2.0 text is in docs/LICENSE-APACHE.txt.

The RB+ swizzle patterns in `crates/victory-formats/src/texture/patterns.rs`
were generated from AMD's Mesa addrlib at
265a2d4f0f842a28e0dfd150637fa91fb70345a2. The file carries AMD's copyright and
MIT identifier. The full text is in docs/LICENSE-AMD-ADDRLIB.txt. The
associated texture layout port credits MiamiGame/XTP commits
00c60fb197434c4987156779466a2ab41d5be2df and
188d895814255c72518c21d96994164072fac688, by Simon (GPL-3.0).
