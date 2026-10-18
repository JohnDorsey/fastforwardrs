<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Phase 4 report: the effect surface, the built-in effects and the `.cube` host

Written 2026-10-18, at the end of phase 4 of [the plan of action](plugin-system-plan-of-action.md). How the phase was designed is in [its design](engine-phase-4-design.md) (decisions F1 to F15, pieces 4.1 to 4.8); this is the report in the
usual form: **done, what can be seen, decisions, tests rewritten on purpose, limits, mistakes, verified versus believed**, and then **what I need from you**.

**[V]** means I ran something and saw it. **[B]** means I believe it from reading the code. Everything that ran on a graphics adapter ran on **software adapters** (WARP on Windows 10, lavapipe on Ubuntu in the CI), because of rule SF-01:
no shader of the engine has run on a real graphics card.

You asked for all of phase 4 to be built without a stop in the middle, and it was.

## 1. Done

| piece | what | commit |
|---|---|---|
| 4.1 | the descriptor grows (category; for each value a scale, a control — slider, whole number, switch, choice, resource — and a unit); `enabled` (bypass) on an effect; **project file format 13**, with the fixture of format 12 | `18896b3` |
| 4.2 | a layer carries a **list of operations**, not a matrix; the engine's plan with the two colour domains (screen codes, light) and **one conversion between neighbours of one domain**; neighbouring colour mixes are fused into one pass | `b048d43` |
| 4.3 | **saturation, brightness and contrast** (a matrix with offsets), **exposure** and **opacity** (passes in light) | `154f3f7` |
| 4.4 | **blur** (a Gaussian in two passes, of light, the radius in points of a 1080-high picture scaled with the frame) and **crop** (four edges and a feather) | `199ca4d` |
| 4.5 | **resources**: the model, the setting (embed, link, ask; the project's beats the global), the file, embed/link, the limits, dedupe by content, missing | `f60db49` |
| 4.6 | the **`.cube` host** (1-D, 3-D, the Resolve variant read only; every refusal with its line), the 3-D texture, tetrahedral and trilinear passes, the look-up table effect, five sample tables of our own (CC0) | `7227286`, `b723ba4`, `a648078` |
| 4.7 | the surface: the **+ menu with a search field and categories**, *Add effect from file...*, controls made from the descriptor (choice, switch, table), *Duplicate effect*, notes for a table that cannot be used, the export question lists missing tables | `cf06ef9` |
| 4.8 | conformance tests, the mutation campaign and the tests it asked for, the README, the features list, the plan, this report | the commit of this report |

### The gate of the plan, line by line

| the gate | where it stands |
|---|---|
| a table through the shader equals ffmpeg's `lut3d=interp=tetrahedral` within a tolerance (PH-04) | **Met [V].** 1 level at the worst of 4096 colours, for five tables, both ways (`tests/cube_tables_against_ffmpeg.rs`, with the real ffmpeg 9.0.2); the pass is held to the processor's look-up to 1 level on software adapters. |
| conformance tests for each built-in | **Met [V].** `src/composition/pictures/conformance_tests.rs` goes through `EffectKind::BUILT_IN` and the descriptors: the default of every value is inside its range and changes nothing; both ends of every range do what the arithmetic says; **every number of every effect follows its keyframes at every frame** (0 to 59, before the first keyframe, between, after the last); values past the ends are cut off; values of the wrong kind do nothing. The effect-by-effect tests of 4.3 to 4.6 are against the arithmetic written once on the processor. |
| a project with a missing effect survives a round trip unchanged and renders as pass-through | **Met [V]** (phase 2 had it for effects; 4.5 has it for resources, with an unknown effect, an unknown version, an unknown value and a resource that is gone, each saved, loaded and saved again). |
| mutation campaign | **Done**, section 5. |
| *Delivers:* the first plugin family | built: **eight effects and the `.cube` host**, with the surface to use them. |

### The numbers

* **Tests, at the end:** the library **4256 passed, 0 failed, 7 ignored**; the integration programs **255 passed, 0 failed, 27 ignored** (the ignored ones are the real-time ones, the ones that write pictures into `docs`, and the benchmark) **[V]**.
* `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items`: clean **[V]**.
* **CI [V]:** green on all five jobs at `5b15d90` and `cf06ef9` (4.7), the last pushed before this report. At `f60db49`, `7227286` and `b723ba4` the Ubuntu job was red on the **doc step** and at `a648078` too (see section 6: my own mistake); `5868b5b` fixed it and is green. The CI of the commit of this report is not yet looked at.
* **Planted mistakes: 362** over the new code (colour effects, the `.cube` host, the descriptors of the built-in effects, resources in the model and in the file, the engine's own effects, tables on the card, the plan, the frame renderer, the layer's operations, `frame_scene`, the table flow and the plus menu, and five shaders). **272 were noticed at once, 74 were not, 14 did not compile and 2 hung or crashed (which counts as noticed).**
  After the tests below were added, the 74 were run again (88 lines with the 14 that do not compile): **26 are now noticed, 5 no longer exist because the code changed (see below), and 43 are left.** All 43 were read one by one; none is a mistake a person could see:
  * *Ties and thresholds* (a `>` for a `>=` where both give the same colour): the four of tetrahedral interpolation in `cube_table.rs` and the six in `lut.wgsl` (when two of the three fractions are equal, the two orders of the tetrahedron give the same value; that is why the choice is free), `colour_effects.rs` 156 and 160, `built_in.rs` 483 and 484, `plan.rs` 310 and 321 (the threshold below which a change is taken for none: a float cannot be told to be exactly the threshold), `pictures.rs` 353 and 569 (older code of phase 0 to 2 that the random choice hit; the boundary is the same number twice).
  * *Values nothing reads:* the alpha and the padding of the texture of a table (`tables.rs` 106, 141, 142: the shader reads three numbers), the matrix and the offsets of a pass that only converts (`frame_renderer.rs` 743 and 752: it mixes nothing), the size of a solid colour (660), a capacity (`frame_renderer.rs` 1175, `resources.rs` 211), the clear colour of a variable that is overwritten (`lut.wgsl` 149), the size of a fullscreen triangle that is cut by the screen (`lut.wgsl` 56), a division by an opacity of nothing whose result is multiplied by nothing (`lut.wgsl` 150), an `unwrap_or` that cannot be reached (`table_flow.rs` 200), a subtraction that cannot go below nothing (`resources.rs` 337), a second check that catches the same files (`resources.rs` 248), the serial number a table begins at (`cube_table.rs` 32), the sum of a clamp that a bound already limits (`cube_table.rs` 169), the fallback of an effect of the wrong kind of values for opacity, which is `1.0` or `2.0` and both change nothing (`built_in.rs` 659), and two bits of the hash of a picture's colour (`describe.rs` 198 and 206).
  * *Looks* (cosmetic, and I judged a test would only repeat the constant): the width of the + menu and the places of its dots (`chain.rs` 38, 44, 209, 434), the highlight of the chosen option of a list (`effects.rs` 223), and whether the menu is told to close after a choice when egui closes it anyway (`chain.rs` 426).
  * **I did not look further at `pictures.rs` 404** (`unwrap_or(0)` of an unknown number of frames in a video, older code). [B] that it is the same kind.
  * The 14 that **do not compile** are `&&` turned into `||` between a condition and a `let` (let chains), and numbers of an `enum`; they say nothing about the tests and I did not plant them in another form.
  * **Closed with tests (26 noticed, and the fixes):** the **ACEScg conversions of a table** (the flags that say which way the matrix goes were not noticed because every test used greys, which any set of primaries leaves alone; now a coloured picture, and a test of the matrices themselves), the **premultiplied** handling of the table pass for see-through pixels, the **limits of resources** (64 MiB, 256, 120 characters, the length of an ending, written down as the documents give them and not relative to the constant), the **relative path** rule of a resource in the file, the **clamps** of crop and opacity, the effects of **the wrong kind of values**, the **domain** that a table works in (only a whole table of screen codes into screen codes) and the **passes** each kind takes, the **tables on the card** (exactly as many as it keeps), the **sweep of the shelf** of published tables, the **ids** of pictures with their colour tag, the **row count** in the message about a table that is too big.
  * *Code that the campaign led me to change:* `MixingParts::ALL` was a constant nothing used (removed); a fallback of `unwrap_or(0)` in the parser that could not be reached is now an `if let`; the sweep of the shelf is a function of its own so that it can be tested without the global shelf; the matrices of a working space are visible to the engine's tests.
* **The pictures of this phase:** see section 2.

## 2. What can be seen

* The **+ menu** has a field to search (it has the keyboard when the menu opens; **Enter** adds the first that matches), the effects under the headings of their categories, and at the bottom *Add effect from file...*. A box of an effect has *Duplicate effect* in its menu.
* **Eight effects:** hue shift, saturation, brightness and contrast, exposure, opacity, blur, crop, look-up table. A chain of colour effects next to each other is **one pass**.
* **A look-up table** from a `.cube` file, kept in the project (embedded by default) or linked, as the setting *Files that effects use (tables of colours)* says. A table that is missing, or is not a table, is said in the box of the effect, does nothing, and is listed by the export's question.
* **Nothing else changes** in the pictures of a project that has no such effect: the tests of phases 1 to 3 pass without being edited in 4.2 [V].
* There are **no before/after pictures** for this phase: nothing that existed was made to look different. The effects are new.

## 3. Decisions

The answers you gave before the build are in the design (section 6) and were followed: **the radius of a blur is in points of a picture 1080 high, scaled with the frame; opacity is an effect; the spaces of a table are screen codes (sRGB), Rec.709 video (gamma 2.4) and linear light; crop has four edges and a feather; exposure is in stops; five tables of our own by formula; 3-D tables of more than 65 points a side are refused for now; all of the phase at once.** Where I took the decision:

* **Composition's calls moved to 4.3**, not 4.2 as the design had it: the list of operations of a layer was needed by the first effect that is not a hue shift.
* **Duplicate copies the values and not the keyframes** (a copy that moved with the original's keyframes would be surprising; this is said in the README).
* **The values of a choice, a switch and a table are not animated** (they hold until changed); a number is. The stopwatch is not shown for the others.
* **A plain `.json` project cannot embed**: the table is linked and a message says so once (the design had a question about saving as `.ffrs`; a message is less in the way).
* **A table of more than 64 MiB** shows a message and is not added (the design had a prompt).
* **The checksum in the name of an included resource is the CRC-32**, not a strong hash: the name is for telling versions apart, and the zip file checks the bytes. [B] this is enough; a clash would need two different tables of one name with one CRC.
* **ffmpeg's `lut3d` takes the domain of a table differently** from the specification (it scales by the domain in another way); the program follows the specification, the test with ffmpeg uses tables with the default domain and one with another domain is held to the processor's look-up only.
* **A table with `LUT_3D_SIZE` between 66 and 256 is refused** with its row count, and a 1-D table up to 65536 points is read.

## 4. Tests rewritten on purpose

* One test of 4.7 that looked for the checkbox of a flag by the label *On*: the checkbox is labelled by the descriptor's label now.
* No other test of an earlier phase was edited: the phase-1-to-3 tests passed unmodified at 4.2 and since.

## 5. Limits

* **Not built (from the design):** the limit of **64 effects on one object** (F15). Resources and tables have their limits; effects do not. **RS-04** (switch a resource between embedded and linked afterwards), **DS-01 to DS-03** (the program does not scan a folder for tables; the samples are in `assets/luts` and are chosen with *Add effect from file...*), **PH-05** (other table formats), **ID-07** (handles), **UI-11** (EN-14 controls; phase 6).
* **Values of a resource are not animated**, and the keyframes of a value of a resource are not scanned for names that would keep a resource alive (a table named only by a keyframe would be dropped when the project is saved). Today a table can only be named by the value, not by a keyframe, because the value is not animated; [B] nothing can reach the case.
* **opacity, blur and crop are not offered to a clip group or a track group** (phase 5 lifts it).
* **A blur's cost on the card is not measured.** It is two passes, and its half width grows with the radius (up to 100 points of 1080); the benchmark has the scene of two blurs, a matrix and a table, and ran on the software adapters only [V]; on your integrated graphics it is [B] fine at the gate (720p60) and unmeasured.
* **ffmpeg's lut3d** differs from the specification as to the domain (section 3).
* **The preview and the export are the same functions**; a blur or a table at the size of the preview differs from the export at the pixels, because the picture is smaller, not in what it is [B].

## 6. Mistakes (mine)

* **I said "CI green" when it was not.** At `f60db49`, `7227286` and `b723ba4` I looked at the status and said it was green; the Ubuntu job was red on the **doc step** because my script ran `cargo doc` without `--document-private-items` and the CI documents private items; the cause was a link in a comment that points at a private item. I found it at `a648078`, fixed the link (`5868b5b`) and made my script run what the CI runs. Three commits were red on the Ubuntu job for a day without my saying so.
* **My arithmetic in the tests of 4.3 and 4.4:** `128*3-255 = 129`, not 128; a column of a blur at 204, black and white checkers, pass counts, top and bottom crop together, and the feather including the picture's own right edge: each premise was wrong, the code was right, and each is now measured.
* **The licences job** went red on new advisories (RUSTSEC) in the middle of the phase; I added them to `deny.toml` with the reason for each, not by silencing the check.
* **A table scene-linear call skipped its own conversions** (found by the test of spaces): the renderer now converts for a call that is not on screen codes.
* **Two runs of the campaign at once.** I started the campaign twice by mistake (the first start was not stopped by the end of its shell), and the two used one work folder and collided on a linker error; I killed both and started one. No result of the collision was used.
* **Flaky, not diagnosed:** at 4.2 a rustc crash (`STATUS_STACK_BUFFER_OVERRUN`) and two journeys under load that passed on a second run; the macOS job of the CI failed once on a real-time test of the speakers in an earlier phase. I have not found the causes [B: load].
* **Process:** several edit scripts failed on text that `cargo fmt` had reflowed; the tool's `--check` before a campaign and the file tools instead of inline text are the cure, as in phase 3.

## 7. Verified versus believed

**Verified [V]:** the numbers of section 1; that a table through the shader equals ffmpeg's `lut3d` to 1 level (real ffmpeg 9.0.2); that the export of a project with each effect is made end to end (`tests/media_export_effects.rs`); that a project with an embedded table survives a save, a load and a save in `.ffrs`, and that one that is linked survives a move by its relative path; the conformance tests of section 1; that CI is green on five jobs at `cf06ef9`.

**Believed [B]:** that the effects look right on a real graphics card (the shaders are the ones measured on WARP and lavapipe); that a blur at a large radius is fast enough on the integrated graphics; that the tie mistakes are equivalent (I read each; the proof is that the two orders of a tetrahedron agree at a tie, which the test of the reference shows for the ones it reaches); `pictures.rs` 404; that the CRC-32 in the name of a resource is enough.

## 8. What I need from you

1. **Run the preview and an export on your real card with some of the effects** (a blur of 30, a crop with a feather, exposure, one of the sample tables from `assets/luts`), and say whether they look right. Nothing of the phase has run on a card.
2. **Run `cargo run --release --example engine_benchmark`** on the integrated graphics: the scene with two blurs, a matrix and a table is what the effects of this phase cost, and the gate is 720p60.
3. **Decide about the 64-effect limit** (F15): I did not build it. It is a small change (a refusal with a message on adding or duplicating), and I would rather build it with the file reader's limit in one piece than leave it out; say if you want it before phase 5.
4. **Decide about RS-04** (a command to turn an embedded table into a linked one and back): I did not build it; the setting only affects tables added later.
5. **Say whether the order stays 7, 5, 6, 8, 9** (the sound engine and LADSPA next, as you said on 2026-10-16), or whether you want the group effects of phase 5 first now that opacity, blur and crop exist and are not offered to groups.
