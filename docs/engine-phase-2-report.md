# Phase 2 report: the export through the engine

Written 2026-10-16, at the end of phase 2 of [the plan of action](plugin-system-plan-of-action.md). How each piece was built, with the numbers, is in [the design of the phase](engine-phase-2-design.md) (sections 9 to 11); this is the report
in the usual form: **done, decisions, tests rewritten on purpose, limits, mistakes, verified versus believed**, and then **what I need from you**.

**[V]** means I ran something and saw it. **[B]** means I believe it from reading the code. Everything that was run was run on **software adapters** (WARP on Windows 10, lavapipe on Ubuntu 26.04 in WSL, and the same two on CI), because of rule SF-01:
no code of the engine, and none of the export, has run on a real graphics card.

## 1. Done

**The finished video is made by the picture engine.** ffmpeg decodes the files (one process for each *run* of a clip's video, started a second and a half before it is wanted, read in order) and encodes (raw frames on its standard input, with the sound made by the same filter graph as before). Between
them, for each frame of the project, the engine draws the picture from the very description that the preview is made from, makes the planes of a `yuv420p` picture of it on the card, and hands them back, with up to two frames in flight in the engine and four asked ahead, so that the decoders, the card and the encoder work at once.
The old export (ffmpeg's filter graph for the picture, the cutting of animation into stretches, the refusal of "too much animation") is **deleted**, with what only it used.

| piece | what | commit |
|---|---|---|
| 2.1 | `frame_scene`: the preview and the export describe a frame with the same functions | `b072888` |
| 2.2, 2.3 | the planes of `yuv420p` on the card; ordered frames with the next one recorded while the one before is read back; the card's textures reused; the benchmark's new scene and table | `aec5d5a` |
| 2.4 | the plan of an export, the runs of video, their pictures in order | `ca12705` |
| 2.5 | the runner, the frame describer, the encoder, the colours of the file | `d478f82` |
| 2.6 | the window's export on the engine, the command line `fastforwardrs-export` | `4e3331a` |
| 2.7 | the comparison with the old export on 13 projects, and the old export deleted | `1280da6` |
| 2.8 | what planting mistakes found, one function for what a frame shows, a panic ends an export with a message, the macOS step of the CI, the documents | the commit of this report |

### The gate of the plan, line by line

| the gate | where it stands |
|---|---|
| **Preview and export are bit-identical on one machine** (EN-03) | **Met in the way that can be shown, not in the way that is meant, [V] and [B].** [V] The window and the export build what a frame shows with the same functions (`layers_of_a_section`, `symbol_of_a_section`, `describe_frame`) and the engine draws it with the same passes; the export's planes are the preview's 8-bit frame converted, within **one level** of the plain arithmetic on 40 scenes of every kind and three matrices and ranges. [B] That the 8-bit frame itself is **byte for byte** the same in both is not read back in planes mode, so no test says it; it follows from its being one render with a different read-back. |
| …and **within a stated tolerance across machines** | **Measured for two software rasterisers only [V]:** the same 40 scenes on WARP and on lavapipe: 98.5 % of the values are identical, the rest differ by 1 level except 84 of 670 288 values (80 by 2, 3 by 3, 1 by 4); the planes by 2 at most. **Real cards of other makers: not measured (SF-01).** |
| **The real-ffmpeg journeys pass unchanged** | **Yes [V], with edits that are mechanical and one replacement (section 4).** The 96 export tests of the old suite (`media_export*`, `media_in_place`, `media_audio_stream`) pass on the new export with their assertions as they were (one premise changed: see section 4); the 29 journeys pass on Windows and on Linux. |
| **The exported files' tags are the same as before** | **For an untagged video, `bt709` and `smpte170m` [V]** (the file says what the video said, and nothing when it said nothing; the colours are within 3 levels of the video's own). **Not the same, on purpose:** a video of the full range is squeezed into the limited one and the file says `tv` (the old export kept the full range and said `yuvj420p`), and the tags follow the *first video* wherever it is laid (the old export's depended on whether the first video filled the picture). The rule is tested as a table; **a full-range file was not exported end to end [B]**. |
| *Delivers:* **animated values are exact at every frame**, and no refusal | **Yes [V]:** a journey exports 1560 frames that are each in another place, each where its keyframes say; the old export snapped to whole pixels (picture in design section 9). |
| *Carried over from phase 1:* the 1080p60 gate on the integrated graphics | **Still open.** Built what bears on it (frames that overlap; pictures that reuse the card's textures); the benchmark has the scene and the table. On the software adapter, at 640 by 360, the whole stack makes 8.7 frames to the second with its pictures on the card and 5.8 when every frame is given new pictures of video, with or without two frames on their way (5.9): a processor does not do two things better than one, so **this says nothing of the gate**. **I need you to run it again (section 8).** |

### The numbers

* **Tests, at the end:** Windows **3907 passed, 0 failed, 7 ignored**; Ubuntu in WSL **3906 passed, 0 failed, 7 ignored** (one test is for Windows only); every program of integration tests passes on both (29 journeys, the 96 export tests of the old suite, and the new `video_export_*` (28) and `command_line_export` (6): `video_export_runs` has 15 on Linux, with the test of an export whose ffmpeg is stuck, which needs a named pipe). **Phase 2 added 189 library tests and 34 integration tests and deleted 128: net +95** [V].
* `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items`: clean **[V]**. `reuse lint`: compliant, 530 of 530 files **[V]**.
* **The mutation campaign:** 57 planted mistakes in 2.2 to 2.4 (the engine's planes and ordered frames, and `pictures.rs`; all noticed in the end, one being no mistake) and **274 in 2.8** over the rest of the new code: **201 noticed at once by the unit tests, 61 more by the programs with the real ffmpeg or after tests were written for them, 12 not noticed**: 2 are code that was removed, 3 can only be noticed on Linux (and fail there, checked by hand), 7 change nothing that a test could see. The list, with the reason for each, is in design section 11.3.
* **The comparison with the old export** (13 projects, every frame of both decoded and compared): frames the same, a video that fills the picture within 3 levels, differences at edge pixels; reproduced from a copy of the old commit [V]. **WARP and lavapipe on the same 40 scenes: within 4 levels, 98.5 % of the values exactly; the planes within 2.**
* **The benchmark** (release build): on the software adapter (WARP) at 640 by 360, 30 frames of each scene, the whole stack makes **8.7 frames to the second** with its pictures already on the card and **5.8** when every frame is given new pictures of video (5.9 with two frames on their way: a processor does not do two things better than one). The new rows are the ones that matter for the gate and are for you to run (section 8).
* **CI:** green on all five jobs at 2.7 (`1280da6`, 1416 s: about twice the 777 s of before the phase, because the export tests run on software rendering). The commit of this report is read when it has run.

## 2. What can be seen (the list of visible differences)

The old export and the new one were compared once on **13 projects**, every frame of both decoded and compared value by value (design section 9, with the pictures in `docs/engine-comparison-phase-2/`; **I reproduced the table from a copy of the old commit and every number came out the same**):

* **Frames: the same frames** (equal counts; no block-sized difference anywhere).
* **A video that fills the picture, colours:** 3 levels at most; flat colours exact. The hue shift is **nearer to the arithmetic** than before (1 level off against 2 on the orange that was tried).
* **Edges** of everything that is turned, scaled or put in a smaller place: differences of up to 255 **at the edge pixels** (a pixel is in the layer or not), in 0.5 to 1.6 % of the values over 4 levels; inside the layers and elsewhere the pictures agree. Inside a turned video the new export is the video's own colour, and the old export's turn left a tint of a level or two.
* **The symbol's and the text's edges** differ by up to 91 levels at the edge pixels of the letters.
* **One row:** the old export put a video that was at half the size one row too high; the new one is where the arithmetic puts it.
* **Animation:** the old export's box moved in whole pixels per stretch; the new one's is exact (1 pixel at the outline).
* **On purpose:** no refusal of "too much animation"; the tags as above.

**For your eye: question 3.**

## 3. Decisions

**Mine, which stand unless you object** (the numbers are those of the design's table, section 2; what changed is in section 11.2 of it):

* **X13 changed: the command line is `fastforwardrs-export`, a program of its own**, and not `fastforwardrs --export`. The release build of the editor is a windowed program on Windows, which has no console to write to; a console program next to it can write its progress and its status.
* **One function for what a frame shows (X2, finished in 2.8).** In 2.1 only `describe_frame` was shared; the window and the export each turned a section of the plan into layers with about twenty lines of their own. They are one function now, with only what really differs passed in. I am telling you because I wrote in 2.1 that the two were "the same picture by construction", which was true of the second half only.
* **A mistake of the program in the export (a panic) ends it with a message** (`the export stopped because of a mistake in the program: …`), as every other ending does. Without it the window would have waited for an end that never came. (Found while reading the code for the planted mistakes, not by a failure.)
* **Anything that begins with a dash on the command line is an option**, so that `-adapter` is told to be one and is not taken for the project file.
* **A destination that is not there yet is not "the same file as a source"** (the old check had a branch for that, which could never be true).
* **X10 only half:** the last picture is held when a file has fewer than the project was told, and counted, **but nothing tells the user** yet. **Question 4.**
* **Not built, on purpose:** not making a frame again when it is the same as the one before (a title card of ten seconds is made 300 times). It is an optimisation that nothing needed to be correct; it waits for a measurement on a real card.
* **A step in the CI that runs the tests of the engine and of the export on the Mac's Metal device**, with `FASTFORWARDRS_TESTS_MAY_USE_ANY_ADAPTER=1` set in that step and nowhere else, and failures as warnings (it is a virtual machine that is set aside for tests, which is what SF-01 allows). **Its first result is not known** (it needs a push to run).

**Yours, as before:** the shape of the export (decode and encode in ffmpeg, compose in the engine); the old export deleted with its tests at the end of the phase.

## 4. Tests rewritten on purpose

**Deleted with the old export (2.7), 128 tests** [V: counted in `git diff 4e3331a 1280da6`]: the old plan of the picture export and its stretches (69: `plan/tests.rs` 38, `plan/effects_tests.rs` 16, `plan/animation_tests.rs` 15), the text of the picture filter graph (44), the command line of the old ffmpeg run (7), what `composition` kept only for the stretches (4), the old filter graph's own file (2), and the `ffmpeg_filter` of the colour matrix (2). **Kept** (they are the checks of the plan against plain evaluation, which the engine's tests still use): the rest of `composition`, `effects_tests.rs` included.

**Changed so that they still compile or still mean the same (every assertion as it was):**

* The 96 export tests of `media_export*.rs`, `media_in_place.rs` and `media_audio_stream.rs`: the calls are the new ones (`plan_export`, `VideoExport::start` with its options). **One premise changed:** the test that a project of 150 clips has a graph that cannot fit on a command line asserts it of the **sound** graph now (the picture has no graph).
* `tests/application_journeys.rs`: 18 journeys use the macro that skips them where there is no graphics adapter and make the application export on a software adapter. **One was replaced:** the one that checked the refusal "too much animation" is now one that exports 1560 frames, each in another place.
* `tests/support/mod.rs` and `tests/journey/mod.rs`: the helpers export with the new export.
* Test videos are named by their whole specification (two specifications once shared a name, and macOS CI found it).
* 2.8: `frames/tests.rs` gets one import (`LayerContent` was imported by `frames.rs` and no longer is), `oracle_tests.rs` has two helpers taken out of code that was there (the scene that a number makes up, and the description of a scene on a renderer), the weak test of a dropped encoder is made strong (below).

## 5. Limits

* **Nothing of the export has run on a real graphics card** (SF-01), and **`fastforwardrs-export --adapter best` has never been run by anyone**: the choice of the best adapter is a function of the list (tested), and making a device on a real card is yours to try.
* **How fast an export is on a real card is not known.** On the software adapter, at 320 by 180, the new export of the 13 comparison projects took 0.6 to 2.3 s where the old one took 0.1 to 1.3 s (the slowest of the old was the animated project, which it cut into stretches); most of the new one's time is starting the software device, and none of it says anything of a card.
* **The export needs a graphics adapter.** A computer with none cannot export (Windows and Linux with lavapipe have a software one; `--adapter software` asks for it).
* **A lost graphics device ends the export** (with a message). The engine can replace its device; the export does not use that yet.
* **The export does not tell that it held a picture** (X10), and **does not skip frames that repeat**.
* **The 8-bit frame of the export is not read back**, so "bit-identical" is shown through the planes (section 1).
* **Colour siting** of the 4:2:0 planes is the centre; H.264's default is the left. **[B]** not visible; not looked for.
* **A video of the full range, or one that is not in planes, is not exported end to end in a test**; the rule for it is a tested table.
* **`fastforwardrs-export` cannot be stopped cleanly with Ctrl+C** (ffmpeg is stopped with it and the half-written file is left); the window's Cancel removes it.
* **CI takes twice as long** as before the phase (about 23 to 27 minutes against 13): the export tests run on software rendering. The time is the Windows job's integration step.
* **macOS:** the export and the engine tests skip on the CI's Mac (no software adapter), apart from the new step above.

## 6. Mistakes

Mine, found, and corrected.

1. **Concurrent adapter enumeration crashed lavapipe** (a segmentation fault in two test programs that ran their tests in parallel on Linux): asking whether a software adapter exists, from one thread, while another thread was drawing. Found on Linux in 2.5, **a hypothesis (the previous device being left behind) was disproved by a diagnostic before the cause was found**; the question is asked once now, under the lock that serialises exports.
2. **Two test videos had one name** (the file for one specification was made by whichever test program ran first, and a Mac that skips the tests that export made the one without sound for the probe test): found by **macOS CI**, mended in 2.6 (a name made of the whole specification).
3. **`LazyTextRenderer::is_loading` stayed true for ever** after the fonts were loaded (the old export's wait had hidden it). Found when a journey hung for 271 seconds; mended with a regression test (checked to fail on the old code).
4. **I wrote that the preview and the export were "the same picture by construction" in 2.1, when only `describe_frame` was shared.** The two builders of what a frame shows were found to be parallel copies when I came to write the claim into the gate; they are one function now (2.8).
5. **A panic in the export's thread would have left the window waiting for ever.** Found by reading the code while writing planted mistakes; mended and tested.
6. **A test of mine could not see what it was written for** (found by a planted mistake): `an_encoder_that_is_dropped_without_being_finished_does_not_stay_running` passed with the code that stops ffmpeg removed, because ffmpeg ends by itself when its input closes. It now gives ffmpeg an hour of silence to encode, so that only the drop can end it.
7. **Wrong premises of tests** (each fixed by correcting the test, with an assertion of the premise added where it was wanted): a hidden video makes two runs; a start at frame 100 starts only the run that is seen; a 25 frames a second project is made by the first video; an `u8` that overflowed in a made-up gradient; the plan hides a video behind a covering one; a font size is a fraction of the picture's height (my 20 and 80 were both cut to the largest).
8. **Two scripts of mine garbled files** (a heredoc with an apostrophe, and a trim of a patch script that duplicated it); each was caught before it was run on the tree or by the tests, and the rule in my notes (write such scripts with the file tool) stands.
9. **Redundant guards in `find_the_runs`** (two conditions that each made the other unnecessary) and **a dead branch in `is_the_same_file`**: found by planted mistakes that nothing could notice; the code is simpler now, and tests with hand-made plans say what each of the remaining conditions is for.
10. **The first version of the test of cancelling an export whose ffmpeg is stuck hung when the cancel did not work, instead of failing.** Dropping an export waits for its thread, which was blocked in a write to a ffmpeg that waits for a named pipe for ever; planted on the Linux copy, the test did not end for 17 minutes. It lets ffmpeg go (it opens the pipe for reading and writing at once and shuts it) before it asserts, so that a failure is a failure, in 33 seconds.
11. **A mistake of mine in how I checked that test on Linux**: for a while the unmodified test appeared to fail 8 times out of 8, and then to pass 8 out of 8. The reason was the copy on Linux: `rsync` gives the files the old times of the Windows files, and cargo does not build again what is not newer than the build it has, so a binary with a planted mistake in it went on being run after the file had been put back. Found because the unmodified code could not both fail and pass; the script touches the files after every change now. (The suites that were run on Linux were not affected: files that were edited are always newer than the last build.)

## 7. Verified versus believed

**Verified [V]:** everything in the numbers section above; that the comparison can be made again from the old commit and gives the same numbers; that each new test that closes a gap notices its mistake when it is planted again (the second tries of the campaigns say so for the ones Windows can run, and the three that need a named pipe were planted by hand on Linux: each fails there, and the test passes without them);
that a panic in the export's thread ends the export with a message (and that the test hangs when the mending is taken out); that `fastforwardrs-export` exports a plain project and a project file with its video in it when the video is gone, tells a line for each per cent, says why it failed and uses the exit statuses 0, 1 and 2; that cancelling and dropping an export leave nothing (also when ffmpeg is
stuck, on Linux); that the window and the export call one function for what a frame shows; that the planes are the preview's frame converted; that nothing of this ran on a real graphics card.

**Believed [B]:** that the 8-bit frame is **byte for byte** the same in the two (it is one render with a different read-back; no test reads it back in planes mode); that a real card behaves as the software adapters do (speed, a lost device, edge pixels of turned pictures); that `--adapter best` picks the card you expect (the ranking is a tested function of a list; making a device on a real card is not tested);
what the Metal step of the CI will say; that the colour siting of 4:2:0 is not visible; that a video of the full range or one that is not in planes comes out right end to end (the rule is a tested table; no such file was exported in a test); that the 7 mistakes that nothing noticed change nothing a test could see (my reading, with the reason for each); that `fastforwardrs-export` leaves the half-written file when it is stopped with Ctrl+C (from the code).

## 8. What I need from you

1. **Run the benchmark again on the integrated graphics and send me what it prints** (the gate that is still open). It now has a scene with a new video picture in every frame and a second table with two frames overlapping; the last line says which figure the gate was judged by.
   ```text
   cargo run --release --example engine_benchmark
   ```
   (`-- --adapter 0` for the RTX 3090, `-- --list` says which adapters there are.)
2. **Export a real project with the window's File menu, and then with the command line on the real card, and tell me the time and whether it looks right.** The window uses the adapter it is drawn with; the command line uses the best the computer has:
   ```text
   cargo run --release --bin fastforwardrs-export -- "C:\path\to\project.ffrs" "C:\path\to\video.mp4"
   ```
   Please also say **how long** it took for how many frames, which is the number I cannot measure (SF-01).
3. **Look at the pictures of the differences** (`docs/engine-comparison-phase-2/`, five pictures of three panels: the old export, the new one, and what differs made eight times as bright) **and say whether the edges are acceptable.** The cause is the same as in phase 1: the engine samples with a bilinear filter, ffmpeg's `scale` and `rotate` did something else.
4. **Should the export say when it held a picture** (a file with fewer pictures than the project was told)? Today it holds the last picture and counts them and says nothing. If yes: in the message at the end, as the missing frames are?
5. **Is `fastforwardrs-export` (a program of its own) what you wanted for the command line**, rather than an option of the editor?
6. **Still open from phase 1:** the preview with the engine (`FASTFORWARDRS_PREVIEW=engine`) and whether it becomes the default (E8); E2 and E3; should sound (LADSPA) move earlier; should the window say when the engine replaced its device.
7. **The prompt effort for phase 3** (linear light and colour management, a separate and reviewed change): **xhigh**, please.
