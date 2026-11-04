# Floating UI Scorecard

## Checkpoint CP0 (Milestone M0)

Scores: 5 = Same as prototype / spec; 4 = within 2 px or detail only visible at full crop; 3 = visible difference; 2 = wrong; 1 = broken; 0 = missing.
Gate: every cell ≥ 4, every motion row 5.

### Parts

Claude's score decides. "Found" is the state Gemini reported; "Now" is after Claude's fixes on this checkpoint.

| Part | Gemini | Found | Now | What Claude checked |
|---|:---:|:---:|:---:|---|
| Float surface, border, radius 16 | 5 | 5 | 5 | `floats_*.png` at full size; tests read the values |
| Float shadow | 5 | 5 in code, unverifiable in renders | 5 | The goldens had no blur (`debugDisableShadows`). With `withRealShadows`, the plain float is within 1/255 of Chromium's render of the prototype's CSS at the sampled points below it, dark and light |
| Notch at 52 | 5 | 5 | 5 | Golden beside `reference/text-pane-closed_wide-1280_{dark,light}.png` at 3×: edges and joints land on the prototype's |
| Notch at 26, 0 | 5 | 5 | 5 | Golden at full size; joints 13 at 26, plain float at 0; `notch_path_*` tests |
| `UnreadBadge` | 5 | 5 | 5 | Prototype `.bdg` measured: 18 × 18, 11/600 |
| `MentionBadge` | 5 | **3** | 5 | Was a fixed 18 circle, and its test pinned that. Prototype `.bdg` with `@`: 21 × 18, padding 5, radius 9. Now the same pill, width pinned within 1 px of 21 in a `Row` |
| Avatar initials | 5 | 4 | 5 | The formula was up to 0.6 off (avatar 40: 14.1 against 13.5). Now the prototype's table, the formula only for other sizes. Spec §5.19 corrected: the "within 0.5" was Claude's mistake |
| Presence ring and place | 5 | 4 | 5 | Ring 2 was right, but the disc sat at 0. Prototype `.pres` measured: 12 disc at +2. Now the disc is at +2 |

### Motion (every spring against §4.3)

Claude recomputed the curves in a separate script, then ran the real `SpringValue` frame by frame in a throwaway test.

| Row | Spec settle | Recomputed | Gemini | Claude | Worst error against §4.3 |
|---|---|---|:---:|:---:|---|
| `pane` | 342 ms | 342 ms | 5 | 5 | open side 0.43; close notch 0.10; People → Music 0.12 |
| `paneOut` | 244 ms | 244 ms | 5 | 5 | close side 0.17; notch with its 50 ms delay 0.06; voice card 0.09 |
| `pop` | 177 ms | 177 ms | 5 | 5 | menu scale, opacity and offset under 0.005; peak 1.0015 |
| `tile` | 295 ms | 295 ms | 5 | 5 | 90 % at 170 ms; peak 1.0004 |
| `move` | 366 ms | 366 ms | 5 | 5 | 90 % at 186 ms; no overshoot |
| Plain timings | §4.2 | formula | 5 | 5 | Pane button and voice control rows within 0.04 on `easeIn` 130 and `easeOut` 100. In the code: press 80, exit 130, fade 120, turn 140 |

Every spring keeps running until just before its settle time and has stopped just after it.

### Verdict

Every cell ≥ 4 and every motion row 5 once the fixes are in. **Passes, subject to the pull request's checks on the fixes. The owner releases M1.**

## Checkpoint CP1 (Milestone M1)

Scores: 5 = Same as prototype / spec (measured within 1 px); 4 = within 2 px or detail only visible at full crop; 3 = visible difference; 2 = wrong; 1 = broken; 0 = missing.
Gate: every cell ≥ 4, motion row 5.

### Parts by Breakpoint

All 5s are measured within 1 px of `reference/measure.json` and the prototype via `compare.py` (237 checked, 0 off by > 1 px) and `floating_frames_test.dart`.

| Part | 1440 | 1280 | 1180 | 1100 | 980 | 940 | 759 | Crop / Verification |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|---|
| Title bar (Linux, Windows, macOS) | 5 | 5 | 5 | 5 | 5 | 5 | 5 | `titlebar_1280.png`: height 40, flush top, caption height 32 on Windows; `window_chrome_test.dart` |
| Title crumb | 5 | 5 | 5 | 5 | 5 | 5 | 5 | `titlebar_1280.png`: centered, capped at 56 % width, ellipsis on overflow; `title_crumb_test.dart` |
| History arrows | 5 | 5 | 5 | 5 | 5 | 5 | 5 | `titlebar_1280.png`: 16 px icons, 8 px inset, disabled styling; `title_crumb_test.dart` |
| Ground (`rail` between floats) | 5 | 5 | 5 | 5 | 5 | 5 | 5 | `left_column_1280.png`: `colors.rail` under floats, 8 px margins; `floating_frames_test.dart` |
| Rectangles (`rail`, `left`, `head`, `me`, `main`, `composer`) | 5 | 5 | 5 | 5 | 5 | 5 | 5 | `compare.py`: 237 rectangles checked, 0 off by > 1 px; `bottom_row_1280.png` |
| Drawers (right drawer @1100, left drawer @759) | — | — | — | 5 | 5 | 5 | 5 | `drawer_right_1100.png` (`828,40,264,752`), `drawer_left_759.png` (`78,40,285,652`) |

### Motion

| Row | Spec settle | Recomputed | Measured (40 ms) | Measured (120 ms) | Gemini | Score | Numbers & verification |
|---|---|---|---|---|:---:|:---:|---|
| Drawers (`pane`) | 342 ms | 342 ms | x = 602.9 (err 0.0 px) | x = 747.0 (err 0.0 px) | 5 | 5 | `drawers_open_on_the_springs`: progress 0.2266 at 40 ms, 0.7500 at 120 ms against `OcSpring.pane` |

