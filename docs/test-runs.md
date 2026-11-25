# Test runs

Measured runs, newest last. One PC: RTX 5070 12 GB (32-lane waves, driver's
own), Ryzen 7 8700F (16 threads), 32 GB, Windows 11; Series X profile. One
title, played by hand, so scenes differ between runs: compare the shape, not
the last digit. Numbers come from the layer's own frame line (about one every
five seconds), world frames only (1,000 draws and more).

## 2026-11-25

| Run | Build | Minutes | Frames/s median | best | Frame ms 90 % | 95 % | worst | Stalls of 1 s and more | Errors |
|---|---|---|---|---|---|---|---|---|---|
| 6 | main `ae10a74` | 22.9 | 5.4 | 8.1 | 1,345 | 7,174 | 21,559 | 9 | 0 |
| 8 | soldsawn 9e06958, cold pipeline cache | 18.8 | 4.9 (8.0 once pipelines were made) | 12.5 | 683 | 1,111 | 6,530 | 11 | 0 |
| 9 | the same, `SKIP_WC_COMPARE` off, warm cache | 7.5 | 5.8 | 7.7 | 389 | 528 | 1,168 | 1 | 0 |
| 10 | 8's settings, warm cache, shared copies kept (below) | 10.8 | 6.7 | 10.6 | 496 | 1,054 | 2,733 | 5 | 0 |
| 11 | first unified build (`unify/all-forks` ecade07) | 20.1 | 6.8 | 12.9 | 478 | 1,042 | 4,648 | 9 | 0 |

Run 11's first five minutes shared the processor with builds and tests; its
numbers are low for that part. By three-minute part, frames/s median: 4.8,
5.8, 8.0, 5.5, 5.8, 7.5, 7.8.

First world frame after the start (with the wait for a button in it):
425 s (6), 106 s (8), 75 s (9), 166 s (10), 97 s (11). Pipelines made while
running: about 2,190 (8, cold), 0 (9), 0 at the start and 371 later (11).

### Where a frame goes

| Per frame, median | Run 8 | Run 9 | Run 10 | Run 11 |
|---|---|---|---|---|
| Fetched back from the card | 1.4 MB | 85 MB | 8.9 MB | 8.9 MB |
| Sent up to the card | 18 MB | 85 MB | 24 MB | 24 MB |
| Submitting thread waits for the card | 12 ms | 62 ms | 21 ms | 21 ms |
| Preparing (of it copies) | 68 (51) ms | 67 (47) ms | 75 (64) ms | 73 (58) ms |
| The pass over the view heap | 41 ms | 36 ms | 35 ms | 39 ms |

Neither side is full. A four-second look in run 11: 4.6 of 16 threads busy,
the busiest thread at 56 %; the card at 38 % and 84 W of 250 W at its full
clock. What limits the frame is one step waiting for the next.

### Findings

- **Heads and necks missing (run 8).** Present in runs 9, 10 and 11. Two
  candidates, not yet told apart: shader writes that stay on the card and do
  not reach other copies of the same memory (10 and 11 have a change for it),
  and one-time compute passes dropped while pipelines are made (run 8 alone
  had a cold cache). The control run, warm cache and the change off, is open.
- **Once a frame the submitting thread waits 21 ms (43 at the ninetieth of a
  hundred)** for a title buffer copy that touches memory flights may write.
  A switch that runs such copies on the card is written, not measured.
- **Card memory is full** within two minutes; 7,000 to 12,000 textures wait
  while the largest level of every texture is left out already. 330 GB of
  texture uploads in run 11's twenty minutes.
- **Boats sink.** The title reads a height map its compute passes write; it
  stayed on the card. Ported after run 11 from PastiiJay's klbf fork
  (ebdbcc3, ee7081b); being tested.
- Ray tracing structure builder: off, its stages are dropped (about 220 a
  frame at first, 450 later).
- Open faults seen: tile-shaped yellow halos round light patches while the
  camera moves, dark tiles on water, shade that is close to black, walls
  that come in late after a change of scene, about 35 s of black between two
  scenes.
- Keyboard and mouse carried run 11 for twenty minutes: no pad is needed.

### Tests

Unified tree: 234 of 234 GPU tests, 34 of 34 format tests, 122 and 10 of the
rest pass (5 and 6 are left out by design).
