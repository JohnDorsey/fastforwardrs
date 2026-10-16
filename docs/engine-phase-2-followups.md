<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# After the phase 2 report: the maintainer's answers and what was done with them

Written 2026-10-16. The maintainer read [the report of phase 2](engine-phase-2-report.md), ran the benchmark and the export on their own computer, and sent back answers and a bug. This is what was done, in the usual form. It is **not a phase**: nothing in it changes what phase 2 delivered, and phase 3 ([design](engine-phase-3-design.md)) begins after it.

**[V]** means I ran something and saw it. **[B]** means I believe it from reading the code. Everything that ran, ran on **software adapters** (SF-01), except what the maintainer ran themselves.

## What was said

* **The benchmark** (integrated AMD Radeon, Vulkan, 1080p): 50.3 frames to the second one frame at a time, 44.1 with a new video picture in every frame, 43.6 with two frames on their way; the RTX 3090 459.1. *"Let's change our gate to 720p60 on integrated graphics."*
* **The export of a real project** (768 by 432, 30 a second, 2 minutes 24): a contact sheet of eight frames of the file looks right [V] (layered videos, a turned video, the text "bounce"); `ffprobe` says H.264 High, 4346 frames, `yuv420p`, `tv`/`bt709` tags on all four, AAC 48 kHz stereo, and the audio is 42 microseconds shorter than the video [V]. **The time it took was not said**: it is still the number I cannot measure.
* **A bug:** *every time a project is loaded and saved, with nothing changed, the file grows by more than 30 KB*, and *some of the project does not look like valid JSON*. Two copies of the file were left for me (before and after).
* **Answers:** the edges of saturated colours and the pictures of the export's differences are acceptable (question 3); **the export is to say when it held a picture** (4); the separate `fastforwardrs-export` program is acceptable (5); **the engine preview becomes the default, LADSPA moves earlier, and the window announces a replaced graphics device** (6); phase 3 follows (7).

## What was done

### 1. The project file that grew

**What it was** [V, on the two files]: a `.ffrs` file is a zip, stored uncompressed, and *a save adds to it* (round 10: so that a project with gigabytes of video in it is not written again on every save): a new copy of the project text, a new directory, a new end record. What the file had before is "garbage" that is taken away by writing the file again, which was done only when it was **at least 4 MiB** and at least half of what is in use. For a project that is only text (35 199 bytes of JSON), every save added 35 KB and the file would have taken about 115 saves to be tidied. The maintainer's "before" file had **six** copies of the text in it (206 800 bytes), the "after" file **seven** (242 149): each save had added the whole text again, a directory and an end record, and the last end record is what says which copy is the project. The copies are 32 097, 34 322, 34 422, 35 261, 35 349 and 35 349 bytes long (and the one that the "after" file has added is 35 349 more): the earlier ones are shorter because the project had less in it, and **the newest two are identical**, which is what a save with nothing changed does.

**The JSON** [V]: the entry inside is strictly valid (it parses with no NaN and no duplicate key: 35 199 bytes, 6 sources, 5 tracks, 4 groups); what looked wrong in a text editor was **the file around it**: the zip's binary records, and the text of every earlier save, one after another.

**The fix:** a file with little in use (8 MiB or less: every project that has no file included in it) is written again without its garbage **whenever there is any**, which takes a moment; a bigger one when the garbage is at least half of what is in use (so a project with a video in it is not written again to take away a few kilobytes of text). It happens when the user saves and when the window is closed, as it did, and **never by an autosave**, which still only adds. The maintainer's two files, put through the code that the program uses: **242 149 and 206 800 bytes both come out as 35 349 bytes, and the project reads the same** [V, a throwaway program that was run on copies and deleted].

**Tests:** three new ones at the level of the window (saving five times, loading and saving three times, and autosaves followed by a save and by a close, each of which ends with no garbage and no growth), and the unit test of the rule rewritten on purpose (it said that "a hundred kilobytes of garbage are not worth taking away", and "some megabytes" for the least; it says now what is written above, with the edges of 8 MiB and of half). **Rewritten on purpose in `tidying_tests.rs`:** `saving_leaves_alone_garbage_that_is_not_worth_taking_away` (it asserted the bug: a few dozen bytes left alone; it is now `saving_writes_the_project_file_again_for_any_garbage_when_it_has_little_in_use`) and `closing_writes_the_project_file_again_only_if_it_is_worth_it` (it passed for another reason after the fix, so it was given a premise that is true: a project with two files of 5 MiB in it and the text of the saves as garbage).

### 2. The gate is 720p

`FRAME_SIZE_OF_THE_GATE` is 1280 by 720 (the size the preview is made at); the benchmark's default size, its line "the gate is 60 at 720p on the integrated graphics: MADE / NOT MADE" and its test say so; `--size 1920x1080` still measures 1080p, as a figure that says "not the gate". **Whether 720p makes 60 is not measured [B]**: nearly all the work of a frame is per pixel and 720p has 44 % of the pixels of 1080p, so the whole stack would take about 9 to 10 ms, which is 100 frames to the second; the maintainer is asked to run `cargo run --release --example engine_benchmark` once more, which measures 720p by default now.

### 3. The export says when it held a picture (X10)

`VideoExport::pictures_held()` is how many pictures of videos the export made by holding the last picture that ffmpeg gave of a video (a shared counter, updated as the frames are described), and `words_about_held_pictures` is what is said of it. **The window's message at the end of an export** says *"210 pictures of videos were not there: ffmpeg gave fewer pictures of a video file than the project was told it has (the file is not the one the project was made from, or it is damaged), so the last picture it gave stands in for each of them"*, and **the command line says it as a `warning:` line before it says that the video was saved** (and still ends with status 0: the video is made, all of its frames). **Tests:** the counter on a file of 90 pictures that the project was told has 300 (**210 held, and the exported file has all 300 frames, the last 210 the same**), the command line's order of lines, the window's message through the real window (a journey), the words (singular and plural), and a normal export holding none.

### 4. The engine preview is the default; a replaced device is announced

* `FASTFORWARDRS_PREVIEW=old` asks for the window's own drawing; unset (or anything else) gives the engine; `engine` still works and, if the window is not drawn by wgpu, says so in a message (the others are not told: they asked for nothing). The decision is two small pure functions (`which_drawing_is_asked_for`, `what_to_do_about_the_engine`) with tables of cases, because the function that needs a real window could not be tested. The old drawing stays as the oracle and the fallback.
* **A replaced graphics device is told to the user**: *"The picture engine's graphics device had to be replaced: …. The engine made itself a new one and goes on; a picture may have been late. Nothing of the project is changed."*, with the effects that are switched off for the rest of the run if there are any. The message **waits for a question that is being asked** (it never takes the place of a "discard changes?" or an "export anyway?") and **goes under a message that is showing**. Every replacement is told, in the order they came.
* **A stale sentence mended:** the message that the engine had stopped said *"the export does not use the picture engine"*, which has been false since phase 2; it says now that an export starts a picture engine of its own.

### 5. Found on the way: the CI helper counted what it could not count (`e809773`)

My notice of "what ran" said `lines that say SKIPPED: 0` for every step, but cargo shows what a test that passes printed only with `--nocapture`, which only the Metal step uses: on the Mac, where the engine's tests *do* skip in the main steps, it read as "nothing skipped". The helper says "not counted" now where it cannot count, and has 12 tests of its own (run by the `tools` job; 21 planted mistakes in it are all noticed). CI of `e809773` is green on all five jobs [V].

## The numbers

* **Library tests: 3921 passed, 0 failed, 7 ignored** (Windows, WARP); every program of integration tests passes (the journeys 30, with a new one for the held pictures; `video_export_runs` 15 (16 on Linux); `command_line_export` 7); `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items`: clean; `reuse lint` compliant [V].
* **A planted-mistakes campaign over the new code: 34 mistakes, all noticed in the end.** 32 were noticed the first time; **one was a real gap** (a second replacement of the device could have lost the first one's message: nothing fed the handler two events; the handler is a function of its own now, with a test that feeds it two), and one did not compile and was written again. `tools/mutation_testing` needs a **short work folder on Windows** (`--work-folder C:/Users/.../Temp/mwf`): under the long scratchpad path the linker failed with LNK1104.

## Verified versus believed

**Verified:** the cause of the growth (the files' structure was read: the offsets of the seven entries, and that the newest begins where the older file ends); the fix on the maintainer's own two files; that the JSON is strictly valid; the held pictures end to end with the real ffmpeg; the whole suite.
**Believed:** that 720p makes 60 frames to the second on the integrated graphics (an estimate from pixels); that the engine as the default preview looks right on their cards (it is theirs to look at: nothing here ran on a real card); that the message about a replaced device reads well (it was never seen in a real window).

## What is asked of the maintainer

1. Run `cargo run --release --example engine_benchmark` again (720p is the default now) and send what it prints.
2. Use the program as it is (the engine preview is the default now), and say whether anything looks or feels wrong; `FASTFORWARDRS_PREVIEW=old` gives the old drawing back.
3. How long did the export take for how many frames? (Still the number I cannot measure.)
