<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Phase 6 report: the ISF host

Written 2026-10-22, at the end of phase 6 of [the plan of action](plugin-system-plan-of-action.md). How it was designed is in [the design of the phase](engine-phase-6-design.md) (decisions J1 to J9), [the design of passes, buffers that last and imported images](engine-phase-6-4-design.md)
(K1 to K15, and section 6 on EN-14) and [the design of the rest](engine-phase-6-6-design.md) (the plugin manager, the bundles, transitions, the checkpoints and the lead-up).
**[V]** means I ran something and saw it; **[B]** means I believe it from reading the code. Everything that ran on a graphics adapter ran on **software adapters** (WARP on Windows 10, lavapipe on Ubuntu); **nothing of this phase has run on a real graphics card** (SF-01).

**The one piece of the plan that is not done is 6.7, the golden corpus, and it is yours to do**: it is a comparison with the ISF Editor on Windows, which I cannot run (section 6).

## 1. Done

| piece | what | commit |
|---|---|---|
| 6.1 to 6.3 | **the ISF host**: a shader file (`.fs`, `.isf`) is an effect (filters of one picture), kept in the project (file format 16): the reader, the lenient JSON, the rewriting step, naga to WGSL on a thread that survives a panic, a refusal with the line and column of the file | `09f8e7e` |
| 6.5 | **generators**: a clip whose first effect is a shader that reads no picture; **nothing is built in** (the solid colour of phase 5 was taken out and comes back as a few lines of shader) | `e8729f7` |
| 6.4.1 | shaders with **passes**: buffers, sizes by expression, `PASSINDEX`, `RENDERSIZE` | `88d9981` |
| 6.4.2 | **buffers that last** (`PERSISTENT`): states kept by the engine, exact in the export | `e0be7c5` |
| 6.4.4 | **images of a shader's own** (`IMPORTED`), kept inside the shader in the project (file format 17) | `c38ce58` |
| 6.4.3a | the **View** menu with the checkbox *Generate lead-up on seek*, and the settings of the checkpoint interval and memory | `f2e68a6` |
| found by the corpus | a point reaches the shader as the file wrote it, not scaled; the corpus test | `da9d097`, `f22ea05` |
| 6.8 | **the plugin manager** (the sound plugins window, renamed) finds shaders and installs the ones that run | `384de40`, `7eac64d` |
| 6.9 | **the Bundles tab fetches ISF shaders**: 218 of Vidvox's that are licence-compatible and that the program runs (132 filters, 25 generators, 61 transitions), each checked against a checksum kept in the program | `7eac64d`, `8a3083a` |
| 6.6 | **transitions**: one clip over another, the transition on the clip on top, its `progress` animated | `8a3083a` |
| 6.4.3b-1 | **EN-14, the checkpoints**: after a jump the preview goes on from the nearest copy it kept of the state of an effect that depends on earlier frames, and says over the picture that it is approximate | `7371d6f` |
| 6.4.3b-2 | **EN-14, the lead-up**: with the checkbox on, a picture that stays for a quarter of a second gets the frames before it made in the background and is exact | `1d435ea` |
| 6.7 | the golden corpus and the shaders in the box | **not done** (section 6) |

## 2. The gate, line by line

| the gate (plan of action, phase 6) | where it stands |
|---|---|
| the **golden corpus** (10 to 20 shaders compared by hand with the ISF Editor, then frozen) passes | **Not met: the comparison is yours (6.7).** What there is instead **[V]**: a run over the 218 shaders of the manifest (`FASTFORWARDRS_ISF_CORPUS=<folder> cargo test --lib isf_corpus -- --ignored --nocapture`): each draws without a mistake, and each transition shows what is behind at progress 0 and the clip at 1 (61 of 62; *Crazy Parametric Fun* divides by a number that has no default). It found two real bugs (a point was scaled; naga dropped the initialiser of a global that is not a constant), both fixed, each with a test. |
| the deliberate-hang test passes on a software adapter and in WSL | **Unchanged and passing on WARP [V]** (the guard was not touched). WSL: not run in this phase. |
| a feedback effect behaves as EN-14 says on seek | **Met [V], both modes**: the checkpoints (checkbox off) and the lead-up (on). A window with a real engine and a shader that counts the frames: after a jump the picture shows two tenths of red and says it is approximate; with the lead-up it becomes nine tenths, which is what playing from the start shows, and the line goes. |
| filters, generators and transitions are told apart by their inputs | **Met [V]** (`isf::shape_of`). |
| every plugin pass runs under the watchdog | **Met [V]**: the guard, the loop limits and the device replacement are those of phase 1; a lead-up whose frame does not finish is answered as any frame is (the device is replaced, and the lead-up stops). |

## 3. Tests rewritten on purpose

* Seven tests of 6.6 that said *"a transition cannot be run yet"* (the unusable example is now a shader that reads a mask) and one of the rewriting step (`the_shaders_own_main_is_renamed...`, for the globals that are set up in a function before `main`).
* The tests of the preview that read what it asked of the engine (`what_was_asked...`) have an arm for the rules of the checkpoints (E1). I rewrote no other test of an earlier phase, and none to make it pass.

## 4. Numbers

* **Tests:** the library 5265 passed, 0 failed, 10 ignored; clippy `-D warnings` on every target, `cargo fmt --check` and `cargo doc -D warnings` are clean; the integration programs that draw (`media_export_effects` 41, `application_journeys` 32, `sound_plugins_journey` 4) pass, and the new `lead_up_with_a_real_video` (3, the real ffmpeg).
* **Planted mistakes:** 6.6: 44, all caught; E1 (the checkpoints): 46, all caught after two tests were added; E2 (the lead-up): 97, of which 95 were noticed at first and the other two (a describer that was not told how to describe its frames, hidden by a fixture whose sizes happened to agree with the plan's; and a lead-up that the window could not make that it did not say) were closed with tests and are noticed.
* **The lead-up on a processor** (WARP, release build): 300 frames of 1280 by 720 with one effect that has a state take **26.7 s** (89 ms a frame); the pixels taken back are at most a tenth of it; **I did not measure a real card**, and believe it is limited there by decoding (about a second and a half for 300 frames).

## 5. Limits

* **Seeks while the project plays** have a **chase** since the report was written (2026-10-22, at the maintainer's request: design in `engine-phase-6-6-design.md`, E3): the frames from before the jump on to the end of the project are made for states of their own, and the preview's take them over when they have caught up with the playhead. It is verified on the software adapter only (a jump while the project plays at five frames a second is made exact while it plays); on a card whose frames leave no time to spare it never catches up, and is given up after 20 seconds.
* **A transition is not run on a clip inside a group that has effects of its own** (what is behind the clip is then only what is in the group); the box says so and the effect is counted as not run.
* The old drawing of the window (the fallback of a computer with no usable graphics adapter) leaves out what the engine does: blur, crop, the effects of groups, and every shader.
* Shaders that the program cannot run yet are listed as *cannot be used* with the reason: one that has a function with the name of one of GLSL's own (naga refuses it), one that is not UTF-8, one that reads more than 7 pictures, and *Crazy Parametric Fun*. Each is a small piece of work if you meet one that matters.
* **No generator is built in** and **nothing is shipped in the box**: the shaders come from the Bundles tab, after a question.
* **I never used a real graphics card, and never ran the real ffmpeg, the engine, an effect with a state and a video in one window**: each part has a test, and the window gathers what the lead-up needs with a video in planes in a test with made-up streams. **The lead-up of an effect on a group** (its state is as big as the frame) has no test of its own.

## 6. What I need from you

* **6.7, the golden corpus.** If you say go, I make the pictures of 15 shaders (five filters, five generators, five transitions of the manifest, at fixed values) and the list of what to set in the ISF Editor, so that you compare them with it; what you accept is frozen as expected pictures and becomes a test.
* **Look at it in the window**, with a real card: a transition (put a clip on a track above another, put a transition from the Bundles tab on the top one, animate its `progress`), a feedback shader after a seek (the line over the picture, then exact after a moment), and the View menu's checkbox.
* ~~Say whether seeks while the project plays should have a lead-up~~ answered: yes, built (E3).
* The next piece in the order of the plan is **phase 9 (text objects become generators, file format 12)**, size M-L; I suggest **high effort**, with a design first. (The plugin manager of phase 8 exists as *Edit > Plugin manager...*, which finds sound plugins and, since 6.8, shaders; I have not compared it line by line with phase 8 of the plan.)
