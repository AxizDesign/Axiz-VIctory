# VIctory (fork)

Runs an Xbox Series X|S title natively on Windows, from files you supply. This
is a work-in-progress fork of [VIctory](https://github.com/Marvisak/VIctory).

**There are no game files, code or assets here.** You need your own copy of the
game. This project is not affiliated with or endorsed by the game's developer
or publisher.

> **Not playable yet.** The title boots, loads and reaches its world, at a few
> frames per second and with missing graphics. Read [What works](#what-works)
> before you try it.

## Contents

- [Quick start](#quick-start)
- [What works](#what-works)
- [Known problems](#known-problems)
- [Make it run better](#make-it-run-better)
- [When something goes wrong](#when-something-goes-wrong)
- [Controls](#controls)
- [For developers](#for-developers)
- [Debugging](#debugging)
- [Credits and license](#credits-and-license)

## Quick start

### What you need

| | |
|---|---|
| Windows | 10 or 11, 64-bit |
| CPU | AVX2 |
| Graphics card | Direct3D 12 with shader model 6.6, resource binding tier 3 and mesh shaders; 12 GB or more is better |
| Memory | 32 GB. The title uses about 20 GB, so close other programs |
| Disk | an SSD, with room for the shader cache (several GB) |

### Getting started

1. Get the code: `git clone https://github.com/proneonn/victory-unified`
2. Double-click `Launcher.cmd`. It finds your copy's executable, checks this
   PC, installs what is missing and starts the title.

Or the same in PowerShell:

```powershell
.\start.ps1                                  # check the tools, build, check this PC
.\start.ps1 -Install                         # the same, installing what is missing
.\start.ps1 -GameExe <the title's exe>       # the same, then run your own copy
.\start.ps1 -GameExe <the exe> -Prebuild     # prepare shaders ahead of a run
```

If PowerShell refuses scripts:
`powershell -ExecutionPolicy Bypass -File start.ps1`

`victory doctor` (part of `start.ps1`) names what a PC lacks. Every FAIL line is
a reason for a black picture or a run that does not start.

### What to expect on a first run

- Shaders are translated and compiled for your card. The first load is much
  slower than later ones, and parts of the picture come in late.
- **A black screen near the start may be waiting for a button.** Press A, Enter
  or Space.
- **After a skipped cutscene the screen can stay black for several minutes**
  while the next scene loads.
- Single frames of 5 to 20 seconds happen when a scene arrives. Wait, and do not
  hold or drag the window's title bar: the title ends itself when it gets no
  frames for about 30 seconds.
- Close the title with the X or Alt+F4.

## What works

State on 2026-11-25, Series X profile, on one PC (RTX 5070 12 GB, 32 GB RAM).
Other PCs will differ.

| Stage | State |
|---|---|
| Boot, menus | works, 30 fps |
| Loading into the world | works. It crashed at the end of loading (`0x80070057`) until `e5383ce` |
| In the world | renders with faults, about 2 to 5 fps in busy scenes and 8 to 12 in light ones |
| Cutscenes | start; can freeze, see below |
| Skipping the opening cutscene | works, after a long black load |
| Sound | works at first; can break after a long freeze |
| Ray-traced light | off: 3 of the structure builder's 53 programs do not translate for cards with 32-lane waves (NVIDIA) |
| Series S profile | boots; not tried further |

## Known problems

| What you see | What is known |
|---|---|
| Low frame rate | The CPU side. The pass that brings textures up to date took 80 to 500 ms a frame; it is now held to a share of the frame (`VICTORY_TEXTURE_SHARE`). Not measured in a run yet |
| Textures missing, or uploaded over and over | A 12 GB card is full in the world. One 24-minute run uploaded 20.5 GB of textures with about 3.4 GB alive |
| Freezes of 5 to 20 seconds | The title sends no work during them. Cause not known; memory pressure is likely part of it |
| Title closes by itself after a freeze | Its own watchdog, after about 30 seconds without a frame |
| Square blocks on characters, wrong light | 12 compute programs need 64-lane waves and are dropped on NVIDIA; 2 more do not translate. The blocks are believed to come from these |
| Black skin, metal-looking clothes, bare ground | Textures or materials missing. Cause not known |
| Characters frozen in a cutscene, sound broken | Seen after a long freeze. Two faults in the sound code that could do this are fixed; not confirmed in a run yet |
| Shadowed areas black | Ray-traced light is off (above) |
| Access violations in the title's own code | Reported by others on builds from before `350a7c3`. Cause not known |

## Make it run better

1. **Close other programs**, browsers first. With little memory free, freezes
   get longer.
2. **Run twice.** The second start uses the shaders and pipelines of the first.
3. **Try the faster submission settings:** `.\start.ps1 -GameExe <exe> -Tuned`
4. **Change one setting at a time**, set before the start
   (`$env:NAME = 'value'`):

| Variable | Effect |
|---|---|
| `VICTORY_TEXTURE_SHARE=33` | the share of each frame, in percent, the texture pass may take. Lower: smoother, textures come in later. `0`: no limit, as before |
| `VICTORY_TEXTURE_STAGE_MS=250` | the most that pass may take in any case, in ms (three times that with a large backlog) |
| `VICTORY_TEXTURE_BUDGET_MB=<n>` | at most this much card memory for those textures: less pressure on memory, blurrier picture |
| `VICTORY_CARD_BUFFERS_MB`, `VICTORY_CARD_WINDOWS_MB` | how much of the title's memory is copied to the card (`VICTORY_CARD_WINDOWS_MB=0`: no windows) |
| `VICTORY_SHADER_OPT=0` | no clean-up of translated shaders: try it if you see black blocks. Everything is translated again |
| `VICTORY_BVH_BUILDER=1` | asks for ray tracing. It stays off where programs are missing, and the log says which |

More switches are described in comments where the code reads them.

## When something goes wrong

1. **Update first**, then build again: `git pull`, then `.\start.ps1`.
2. **Look at `crashes.log`** in the data folder
   (`%LOCALAPPDATA%\victory\data` by default).
3. **If it names a host GPU failure**, run once more with the debug layer, which
   names the command. It needs the Windows feature "Graphics Tools":

   ```powershell
   $env:VICTORY_D3D12_DEBUG = '1'
   .\start.ps1 -GameExe <the title's exe>
   ```

4. **If `crashes.log` reports Windows error 1455**, Windows ran out of memory to
   promise: close programs, or make the paging file bigger.

In a report, please say: your graphics card, driver version and memory; the
commit (`git rev-parse --short HEAD`); what you saw and when; and the end of
`crashes.log`. Leave out paths with your name, and **do not attach game files,
screenshots or shader dumps** ([asset policy](docs/asset-policy.md)).

## Controls

XInput pads work. Without one, keyboard and mouse stand in while the window has
focus:

| Pad | Keyboard and mouse |
|-----|--------------------|
| left trigger (aim), right trigger (fire) | right click, left click (or U, O) |
| right stick | mouse (or I J K L) |
| left stick | W A S D |
| A | left Shift, Enter |
| B | R, Backspace |
| X | Space, X |
| Y | F, Y |
| left / right shoulder | Tab / Q (or the mouse's back / forward buttons) |
| left / right stick click | left Ctrl, Z / C, H, middle click |
| D-pad up, down, left, right | arrows (or 1, 2, 3, 4); E is right too |
| Menu, View | Esc, P / V |

Click in the window to give it the mouse (that click presses nothing); left Alt
or switching windows gives it back. `VICTORY_MOUSE_SENSITIVITY=1.5` turns
faster (1 by default).

## For developers

Rust is pinned in `rust-toolchain.toml`. The tests need no game data.

```powershell
cargo xtask install-hooks                # asset-policy check on every commit
copy victory.example.toml victory.toml   # then set your paths (outside the repo)
cargo test --workspace
cargo run -p victory -- config           # check your paths
cargo build --release -p victory
target\release\victory.exe run <game dir>\<the title's exe> --execute
```

The window's caption shows the frame rate. Saves, settings and `crashes.log`
are in the data folder, derived data in the cache folder.

### Commands

```
victory inspect <exe|dll> [--imports] [--exports]
victory census [paths...] [--missing] [--strict]
victory lib-exports <import.lib> [--list]
victory shader-info [paths...] [--verbose] [--disasm]
victory translate [paths...] [--validate] [--pipelines] [--list] [--jobs N]
victory translate [paths...] --cache [--pipelines [--used-pipelines]] [--jobs N]
victory run <exe> [--execute] [--unimplemented trap|zero] [--strict]
            [--console series-x|series-s] [-- <title arguments>]
victory config
victory doctor
```

`translate --cache` fills the shader cache ahead of a run; with `--pipelines`
also the pipelines, which belong to one card and driver, and with
`--used-pipelines` only those earlier runs asked for:

```powershell
target\release\victory.exe translate --cache --pipelines <game dir>\shaders.rpf <game dir>\<the title's exe> <game dir>\XboxBvhBuilder.dll <game dir>\amd_fidelityfx_upscaler_xbox.dll
```

Why a program is dropped on a card with 32-lane waves (the hash is in the log):

```powershell
$env:VICTORY_HOST_MAX_WAVE = '32'; $env:VICTORY_WAVE_WHY = '1'
$env:VICTORY_TRANSLATE_ONLY = '<hash>'
victory translate --list <game dir>\<the title's exe> <game dir>\shaders.rpf
```

### More to read

| | |
|---|---|
| [Architecture](docs/architecture.md) | how it works |
| [What this fork changed](docs/fork-changes.md) | the list, with numbers |
| [Ray tracing](docs/fork-ray-tracing.md) | what is there and what is missing |
| [`victory-pso`](docs/victory-pso.md) | pipelines without a title |
| [Upstream status](docs/upstream-status.md) | subsystem by subsystem, 2026-11-23 |
| [Asset policy](docs/asset-policy.md) | what must never be committed |

## Debugging

A release build has no debugging aids. `start.ps1` makes the build that has
them; by hand:

```powershell
cargo build --profile diag -p victory --features diagnostics   # target\diag\victory.exe
```

| Variable | Effect |
|----------|--------|
| `VICTORY_LOG=<target>=debug,...` | debug and trace output, for example `info,victory_hle::modules::audio=debug` |
| `VICTORY_STACKS=20,60` | every thread's stack at those seconds |
| `VICTORY_WATCH=<rva>,...` | logs a title u32 when it changes |
| `VICTORY_KEYS=30:Enter,35:X` | presses keys at those seconds, focus or not (keys as in the pad table above) |
| `VICTORY_CRASH_SAVE=<rva>:<len>,...` | saves title memory to the cache folder on a crash |
| `VICTORY_CAPTURE=<dir>` | saves presented frames (powers of two, or `VICTORY_CAPTURE_FRAMES=<list>`) |
| `VICTORY_DUMP_TARGETS=1` | with the two above, saves every render target of those frames |
| `VICTORY_D3D12_DEBUG=1` | host debug layer |
| `VICTORY_D3D12_GBV=1` | with the above, GPU-based validation (bad descriptor use reported, not faulted; slow) |
| `VICTORY_D3D12_DRED=1` | on a GPU failure, the allocation hit and the commands in flight; for a list that does not close, its commands |
| `VICTORY_PIPELINE_WAIT_MS=24` | reduce the per-frame wait for background pipeline creation from the default of 500 ms. A draw can be omitted until its pipeline is ready |
| `VICTORY_SKIP_PIPELINES=<hash>,...` | omit draws and dispatches, including indirect ones, whose pipeline contains a listed 16-digit shader hash |
| `VICTORY_GPU_TIMING=1` | with `VICTORY_D3D12_DRED=1`, each list of a submission run and timed on its own; slow lists logged with their slowest commands |
| `VICTORY_D3D12_WARP=1` | host device on the software adapter (slow; cannot reset the driver) |
| `VICTORY_SHADER_LOOP_LIMIT=<n>` | every translated shader loop stops after `n` iterations per lane, instead of 256 in pixel, vertex and mesh shaders and 4,096 in compute (own shader cache entries: everything is translated again; a low `n` cuts genuine compute loops short); if a lower `n` makes world frames much faster, loops hitting the limit are what makes them slow |
| `VICTORY_AUDIO_CAPTURE=<file.wav>` | records what the title plays (our process only) |
| `VICTORY_XMA_DUMP=<dir>` | saves the first 32 XMA voices' context and input |

Every build appends crashes and host GPU failures to `crashes.log` in the
title's data folder.

## Credits and license

- **[VIctory](https://github.com/Marvisak/VIctory)** by Marvisak and its
  contributors is the base of everything here: the loader, the runtime
  services, the Direct3D 12 back end and the shader recompiler. Its history is
  kept.
- **[fxceless/victory-fork](https://github.com/fxceless/victory-fork)** and its
  contributors: many changes here come from there, each credited in its commit.
- **[XCL](https://github.com/ivall/xcl)** by ivall and its contributors
  (GPL-2.0-only) was used as a reference for how the same title behaves under
  another layer. Fixes inspired by it were written anew in VIctory's code.
- Developed with [Claude Code](https://claude.com/claude-code).

GPL-3.0-or-later. See [LICENSE](LICENSE) and [NOTICE.md](NOTICE.md).
