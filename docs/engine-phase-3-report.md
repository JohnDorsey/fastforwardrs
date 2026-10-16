<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Phase 3 report: linear light and colour management

Written 2026-10-16, at the end of phase 3 of [the plan of action](plugin-system-plan-of-action.md). How the phase was designed is in [its design](engine-phase-3-design.md) (decisions C1 to C18, pieces 3.1 to 3.8, and in section 12 where each piece
stands); this is the report in the usual form: **done, what can be seen, decisions, tests rewritten on purpose, limits, mistakes, verified versus believed**, and then **what I need from you**.

**[V]** means I ran something and saw it. **[B]** means I believe it from reading the code. Everything that was run was run on **software adapters** (WARP on Windows 10, lavapipe on Ubuntu in the CI, and the software
renderers of the other jobs there), because of rule SF-01: no shader of the engine, and none of the export, has run on a real graphics card.

**The phase is not called done until you have looked at the list of visible differences in section 2** (decision CM-20: that is the gate). Everything else is built and checked.

## 1. Done

The pictures of a project are now **put together in linear light**, and **what the colours of each file are** is read from the file, one way for the preview and the export, and can be said by hand.

| piece | what | commit |
|---|---|---|
| 3.1 | `colour_space`: the table of primaries (Rec.709/sRGB, the two of BT.601, Rec.2020, Display P3, ACEScg, ACES2065-1), the curves (sRGB, gammas 2.2, 2.4 and 2.6, the camera curve of Rec.709, linear), the matrices between sets of primaries (through X, Y, Z, with the Bradford adaptation), in `f64`, held to published numbers and by round trips | `545f86d` |
| 3.2 | the same in WGSL, `WorkingSpace` and `FrameColour` in the engine; the compatibility mode stays **inside the engine** as the oracle's mode | `545f86d` |
| 3.3 | pictures made light when they are sent (premultiplied in light), the video's planes into light, the hue shift in the display-referred domain, the output pass as the display transform; the three colour settings; the window and the export ask for linear light | `545f86d` |
| 3.4 | what a file's tags mean: one function (`colour_of_a_picture_of_size_with`), untagged high definition is BT.709 | `417ecb0` |
| 3.5 | the export's planes are made from the **encoded half-float frame** (rounded to 8 bits once); the file always says its matrix, primaries, transfer and range, from the output colour space | `417ecb0` |
| 3.6 | the user's word about a file's colours: the model (`InputColourOverride` on `SourceMedia`), the window (*Colour space of this file...* in the menu of a clip), the preview and the export use it, **project file format 12** with a kept fixture of format 11 | `9417116` |
| 3.7 | ffmpeg is **told** the matrix and the range for every form it makes colours of (and a stored form of full range, `yuvj`, is read as full); the colour differences of 4:2:0 video are **blended smoothly** in linear light, where the file says they sit; the pictures and the numbers of the visible differences | `98c6110` |
| 3.8 | the planted mistakes and what they found, the documents, this report | the commit of this report |

### The gate of the plan, line by line

| the gate | where it stands |
|---|---|
| the round-trip tests and the tests against published numbers (CM-16), "50 % white over black is 0.5 in linear light, 0.735 in sRGB code" | **Met [V].** Round trips for every curve and every pair of primaries; the sRGB matrix of IEC 61966-2-1, BT.709 to BT.2020 (BT.2087), the two ACES matrices (tolerances 1e-4 for AP1 and 3e-4 for AP0, said where they are used); the sRGB knee constants meet to 3e-8 and the test says so. The card's curves are within 4e-7 of the reference on WARP, and the test bound is 1e-5, a thirtieth of a level. "Half of white over black is the code 0.735" is a test of the engine. |
| **a written list of every visible difference with before/after pictures, reviewed by the maintainer** | **The list and the pictures are in section 2 and `docs/engine-comparison-phase-3/`. The review is yours.** |
| *Delivers:* the colour handling that was asked for as "important" | built; the part of CM-17 that was about native planes of 4:2:2, 4:4:4 and 10 bits is **not** (section 3, D1). |

### The numbers

* **Tests, at the end:** the library **4041 passed, 0 failed, 7 ignored**; the integration programs **245 passed, 0 failed, 27 ignored** (the ignored ones are the real-time ones, the ones that write pictures into `docs`, and the benchmark) **[V]**.
* `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`: clean **[V]**. `reuse lint` is run by the CI (it is not installed here): green at each push.
* **CI [V]:** green on all five jobs at `417ecb0`, `9417116` and `98c6110`. At `545f86d` the macOS job failed once, on two tests of the real speakers (`the_real_speakers_...in_real_time`), which no change of this phase touches; they passed at every run after. I believe it is the runner's audio device and I have not proved it [B].
* **Planted mistakes: 274** in 3.8, over the colour spaces (73), the engine in linear light (77) and the wiring, the settings, the file and the window (124). **259 were noticed by the tests at once, 12 were not, and 3 did not compile** (they were planted again in another form: two were noticed, one was not until a test was added), so 13 were looked at, and for each the cause is below: seven led to a new test or a change of
  the code, three change nothing, and one was noticed as soon as it was aimed at the program of tests that has the test (the tool runs only the library's tests unless a mistake is aimed).
  * *Closed with a test or a change:* the **ownership of the sRGB knee** (`<=` against `<` at the knee, and the knee's own value, three mistakes: they differ by 3e-8, so only the exact arithmetic of each piece can tell which is used: now a test of exactly that); the **cut-off of the encode pass** (a green of Display P3 is a green above 1 in Rec.709, and the half-float frame, unlike the 8-bit one, does not cut it off by being stored: **the pass cut off, and nothing tested that it did**; now a test of exactly that); the **names of the curves in the shader** (the constants `CURVE_...` were in the file and not used: the `switch` has them now); the **export's readings** of its videos (they were made in the middle of a long function, with nothing to test: now `readings_of_the_videos`, with two tests); a **choice on another file** being one step of the history with the choice on the first (the key of the gesture did not say which file: now it does); the **window sending its choice for the first file** whatever file it was about (only a test with a picture file shows it); and the **tags and numbers of the output space**, which only the default had been looked at (a journey exports a grey with each of the three spaces and reads back the tags and the numbers).
  * *Mistakes that change nothing:* the **threshold of the Rec.709 curve in the shader** (rounded beta: the two pieces meet there, so which one is used there is unobservable); a **clamp in the colour mixing shader** (the curve that follows cuts off the same way); and **an all-clear pixel** being divided by a number that is not 0 (its colours are 0 whatever).
  * The tool runs only the library's tests unless a mistake is aimed at a program of tests (the export's mistakes are aimed at `video_export_runs`).
* **Comparison pictures:** nine scenes, made twice by the engine on the software adapter (section 2); `cargo test --test linear_light_comparison -- --ignored --nocapture` writes them again and prints the table.

## 2. What can be seen (the list of visible differences)

The same scene is made **before** (the engine in its compatibility mode, which is the numbers that the preview and the export had until this phase: the tests of the engine against the old drawing hold it to them) and **after**
(linear light, a video's colours read as phase 3 reads them). Each picture in `docs/engine-comparison-phase-3/` has three panels: before, after, and the difference made eight times as strong, in grey. The numbers are over the three
colour numbers of every pixel of the scene.

| # | scene | the largest difference | the mean | values changed |
|---|---|---|---|---|
| 01 | a grid of white lines one pixel wide, turned 20 degrees and made 0.6 times as big | 73 | 5.6 | 12.4 % |
| 02 | a checkerboard of squares of two pixels, made 0.7 times as big | 74 | 38.2 | 73.3 % |
| 03 | half-clear white over black, half-clear red over blue, half-clear grey over white | 60 | 37.7 | 88.9 % |
| 04 | a white spot with a soft edge (the edge of a letter) over a dark picture | 57 | 2.8 | 7.4 % |
| 05 | a black spot with a soft edge over a light picture | 66 | 3.2 | 7.4 % |
| 06 | the planes of a video that says nothing, 1920 across: BT.601 (before) and BT.709 (after) | 10 | 3.1 | 67 % |
| 07 | the edge between two colours of a video: copied to four pixels (before), blended (after) | 82 | 0.6 | 2.8 % |
| 08 | a smooth gradient shown at its own size | **0** | 0 | **0 %** |
| 09 | the same gradient made half as big | 1 | 0.05 | 4.6 % |

What each one says:

1. **Scaled and turned pictures** (01, 02): where light is averaged, the lines and the squares that are made smaller are **brighter** than they were (a white line on black thinner than a pixel is the light that is on it, not the average of the numbers). That is what a lens, a screen and every program that works in linear light do; it
   is also the largest change that you will see. A photograph made smaller changes very little (09: 1 level), because the pixels that are mixed are nearly alike.
2. **See-through layers** (03): half of white over black is the code 188 and not 128 (0.735 of the code), and half of red over blue is a lighter purple. A layer at half its opacity is **lighter than it was** wherever it is over something darker.
3. **The soft edge of text** (04, 05): the antialiased edge is the coverage of the light, so **light text on a dark picture looks bolder and dark text on a light picture looks thinner** than it did. (The scenes use a made-up spot with a soft edge so that they do not depend on the fonts of a computer; the text of the program is made the same way.)
4. **Untagged high-definition video** (06): a video that says nothing of its matrix is read as BT.709 when it is more than 1024 across or 576 down (it was BT.601 at every size): its reds and greens are other by up to 10 levels, which is the difference between a picture that looks right and one that looks a little off. Standard definition is as it was.
5. **The edge between two colours of a video** (07): the colour differences of a video that is stored as 4:2:0 are blended in linear light, so a hard edge of colour is smooth where it was a block of four pixels (82 levels at the edge pixels, and nothing elsewhere).
6. **The file says what it is**: the exported file says its matrix, primaries, transfer and range (`bt709`, `bt709`, `bt709` or `iec61966-2-1`, `tv`), where it said nothing when the first video said nothing. The numbers are the preview's with the default settings.
7. **Nothing else (08):** a picture that is only shown at its own size comes out with the numbers it had (all 256 codes are held to it by a test), and so does a clip that is only cut or moved, and a hue shift on a flat colour. A sRGB picture put through the whole path comes out within 1 level.

**For your eye: questions 1 to 3 in section 8.**

## 3. Decisions

The design (section 2 of it) has C1 to C18; these are the ones where what was built is not what was written, or where I chose something that you should know about.

* **D1. Native planes of 4:2:2, 4:4:4 and 10-bit video are not built.** The plan said that phase 3 would give the engine those forms as they are, instead of having ffmpeg make colours of them. I **did the part that makes the colours right**: ffmpeg is told
  the matrix and the range that the program reads the file with (`scale=...:in_color_matrix=...:in_range=...`), through the one function that the preview and the export both ask (`playback::reading_of_a_video`), so that an untagged high-definition 4:4:4 or 10-bit video is BT.709 like a 4:2:0 one (measured: the guess that this
  replaces is 5.8 levels away on average, and the told reading exact). What native planes would add is **how the colour differences are blended** (ffmpeg's own for those forms), for a kind of file that a screen recording is not, and it is a large change: a layout for each form in the stream, the upload and the shader. **It is yours to say whether it is wanted.** I believe it is not worth its size now [B].
* **D2. The window of a file's colours applies a choice at once** (like the settings windows) and does not have an *OK* button; choices in a row on one file are one step of the history, so one undo takes them back. The design said *OK*.
* **D3. The matrix enum moved** from `media` (where it was `NativeMatrix`) to `colour_space` (`VideoMatrix`), because the project model now has one in what the user says of a file, and the project must not depend on `media`. Every user was renamed; the compiler found all of them.
* **D4. A `yuvj` file says its range by its pixel format** (`RangeIsFullBecauseOfThePixelFormat`) when it says nothing. **Measured:** ffprobe says `pc` of every real `yuvj` file I could make (MJPEG, H.264), and ffmpeg takes a `yuvj` file as full range **whatever it is told** (told limited, it still makes the full-range picture), so the rule changes no picture today; it is there so that what the window says of such a file is true, and a test keeps the measured behaviour.
* **D5. The foreign effects are given the numbers of the working space** (C10 said they should be given display-referred codes, as the effects of shader files are written for). There is no foreign effect yet that a person can load (phase 4 makes the first), so nothing is wrong today; the conversion around them belongs to the phase that makes them. **The hue shift is done in the right domain.**
* **D6. The export's planes are BT.709 whatever the first video was.** A video of another matrix (a standard-definition one) is converted in light, and the file says BT.709: **its colours are its own and the file is not tagged with the matrix of another**. This changes the planes (not the pictures) of an export of such a video.
* **D7. Smooth chroma is in linear light only.** The compatibility mode copies the colour differences, as ffmpeg does, so that the tests of the engine against the old drawing pass as they were written.

The decisions of design section 11 stand as written (C6, C8, C9, C16); section 8 says again what I need.

## 4. Tests rewritten on purpose

Each of these asserted a number or a shape that this phase changes on purpose, and has a comment in the code that says so and why. **None was weakened**: each asserts what it asserted, about the new thing.

* `engine::yuv` layout test (the struct has the curve, the matrix to the working space and, now, a vector for how the colour differences are made one for each pixel); `engine::effect_tests` colour matrix layout (the two domain matrices and the curve after the mixing matrix); `engine::foreign_effect_tests`
  stand-in (the struct got them); `engine::linear_light_tests::a_video_picture_in_planes...` (the colour differences are the same all over the picture, three pairs of them: in linear light they are blended, which the test of the blend tests).
* `media::source_probe` and `playback::supplier` tests about what a video that says nothing is (BT.709 at 1920 by 1080, and BT.601 at 64 by 48 as it was).
* `tests/media_export_effects.rs`, the half-clear text: the colour is mixed as light is mixed, so the number is the one that the arithmetic of the standards gives (the test computes it and says so).
* `video_export::output_colour` tests (the rules follow the output space, not the first video); in `plan` tests the assertions about `first_video()` and the test of it, which is gone, because nothing asks which video is first any more.
* `project_file`: the version is 12 in three tests that said 11 (`groups_tests`, `included_files_tests`, the documented JSON shape); `tests/video_export_runs.rs`: the file says `bt709` whatever the first video said.
* Earlier in the phase (the follow-ups after phase 2): the rule for when a project file is written again without its garbage (one unit test and two of the tests of tidying), the settings tests, the `frames` tests that now run in the compatibility mode on purpose.
* `tests/media_yuv_delivery.rs`: the test that said that telling ffmpeg a range changes a `yuvj` picture was restated as what was measured (D4).

## 5. Limits

* **HDR is not converted** (PQ and HLG are read as sRGB, and the window of the file says so); camera logs are not there; the display is sRGB, 2.2 or 2.4 by the setting, **the program does not read the computer's colour profile**; there is no dithering.
* **ffmpeg scales a video before the program sees it**, in the numbers of the file (in codes): a video made much smaller has its edges averaged in codes. Moving the scaling into the engine means sending full-size pictures (4K is 12 MB a picture in planes); not done.
* **The old drawing** (the window's fallback when the engine stops, and `FASTFORWARDRS_PREVIEW=old`) ignores every colour setting, as it always mixed codes.
* **4:2:2, 4:4:4 and 10-bit video** are made into colours by ffmpeg (D1). The colour differences of those are blended its way.
* **AP0 (ACES2065-1) as a place to choose** is not offered: it is in the table of primaries, and nothing lets a person choose it as an input or an output yet (CM-06's second half).
* The export on a **real** graphics card is not checked (SF-01); the 1080p/720p benchmark gate is yours to run.

## 6. Mistakes (mine)

* **Test premises that were wrong, found by running them:** my chroma tests assumed that the blue of a colour moves in a straight line with the colour difference, which it does not once it is cut off at 0 or 255 (the values were out of range); my test of an untagged high-definition video said that the two matrices differ by 20 levels, and they differ by 12 (the premise was a mean, which is 5.6, in the end);
  my test of `yuvj` said that telling ffmpeg the wrong range washes colours out, and ffmpeg does not do it (D4). In each case the code was right and the premise was not, and each premise is now measured and written down.
* **A comparison that could not tell the difference:** the first scene of a checkerboard was made at exactly half the size, where every sample falls between texels that are alike, so it changed nothing (0 levels). The scene is at 0.7 now.
* **A redundant constant:** the curve numbers in the shader were named constants that nothing used (the `switch` used literals): a planted mistake in them was not noticed, which is how I found it. The `switch` uses them now.
* **A cut-off that nothing tested.** The pass that makes the encoded frame cuts off what is out of range, which for the half-float frame is what keeps a wide-gamut colour from making a brightness above what the file's range can hold (the 8-bit frame of the preview is cut off by being stored). I had written the cut-off and not a test of it until a planted mistake in the pass survived: it is a test now (a Display P3 green).
* **Process:** several of my edit scripts failed on text that `cargo fmt` had reflowed; I now check the text of a change with the tool's `--check` before a campaign and write scripts with the file tool and not inline. (No harm to the code: each failure stopped before it wrote.)
* **Not mine, but noted:** the CI's macOS job failed once on two tests of the real speakers (section 1).

## 7. Verified versus believed

**Verified [V]** (run, on software adapters; the real ffmpeg 9.0.2 and ffprobe): the numbers of the section above; that the file says what the output space says and has its numbers (a journey exports a grey with each of the three spaces: `128`, `135`, `128`, as the arithmetic says); that an untagged high-definition video is read as BT.709
in the export in all of 4:2:0, 4:2:2, 4:4:4 and 10 bits (a mean 0.5 to 1.3 levels from the same planes saying BT.709, where BT.601 would be 5.6); that ffmpeg told the matrix and the range is exactly the file that says them (0 levels); that a project file of format 11 loads and is saved as the same text with the new version line;
that CI is green on five jobs at `417ecb0`, `9417116`, `98c6110`.

**Believed [B]:** that the preview looks right on a real graphics card (the shaders are the ones that were measured on WARP and lavapipe, and I expect the cards to follow, to a level of rounding, but it is for you to look); that the cost of the half-float passes on the integrated graphics is small (the benchmark is yours to run: the gate is 720p60 and the old measurements
were at 1080p); that the macOS speaker failure was the runner's; that native planes for 4:2:2/4:4:4/10-bit are not worth building now.

## 8. What I need from you

1. **Look at the pictures in `docs/engine-comparison-phase-3/` and the list in section 2, and say whether each difference is acceptable.** The largest, and the one most likely to be noticed, is that *things made smaller, turned, or half see-through are lighter than they were* (01, 02, 03), and that *light text is bolder on a dark picture* (04). If you want
   the old mixing back for something, say which: the engine can mix in codes where it is asked to (the compatibility mode is still there inside it), and a setting is a few lines.
2. **Whether native planes for 4:2:2, 4:4:4 and 10-bit video are wanted** (D1), or whether ffmpeg's conversion, told the matrix, is enough.
3. **The four decisions at the end of the design** (C6: a file tagged BT.709 is read with the curve a screen shows it, not the camera's; C8: three display transforms and no profile reading; C9: the default output space is "as the preview shows it", tagged `bt709`; C16: the project file is format 12, so an older program refuses it).
4. **Run `cargo run --release --example engine_benchmark`** on the integrated graphics (the gate is 720p60 now), and **an export and the preview on a real card**: nothing of the colours has run on one.
5. If you can, **tell me what the macOS speakers do** (or just that nobody cares): the CI failed there once, on a real-time test.
