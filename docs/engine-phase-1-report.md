# Phase 1 report: the engine skeleton in compatibility mode

Written 2026-10-16, at the end of phase 1 of [the plan of action](plugin-system-plan-of-action.md). How each piece was built, with the numbers, is in [the design of the phase](engine-phase-1-design.md); this is the report
in the usual form: **done, decisions, tests rewritten on purpose, limits, mistakes, verified versus believed**, and then **what I need from you**.

**[V]** means I ran something and saw it. **[B]** means I believe it from reading the code. Everything that was run was run on **software adapters** (WARP on Windows 10, lavapipe on Ubuntu 26.04 in WSL, and the same two on CI), because of rule SF-01:
no code of the engine has run on a real graphics card.

## 1. Done

The preview can be made by a picture engine on the graphics card. It runs on a thread of its own with a graphics device of its own, is given a description of the frame (layers, four corners each, effect calls) and pictures by number, and gives back the finished
frame as pixels, which the window shows as one texture. It reproduces today's pictures: tests compare the two drawings on random scenes and hold them to a stated tolerance. It is **off unless `FASTFORWARDRS_PREVIEW=engine` is set** (decision E8, below); the old drawing is what the preview is made by otherwise, and what it falls back to if the engine stops.

| piece | what | where |
|---|---|---|
| 1.1 | the skeleton: device, texture pool, the one buffer for the numbers of every pass of a frame, read-back, caches, the thread and its messages, the plan of a frame as a pure function, black frames | `src/engine/` |
| 1.2 | pictures into the engine, **place** and **composite**, and the oracle: the same triangles through egui's own renderer | `engine/pictures.rs`, `passes`, `shaders/*.wgsl` |
| 1.3 | **video in planes**: ffmpeg gives YUV, and the card makes colours of it (no conversion in ffmpeg, and the matrix and the range are the video's own tags) | `media/source_probe.rs`, `frame_stream.rs`, `engine/yuv.rs` |
| 1.4 | the **colour matrix** effect worked in half floats (no 8-bit step), `describe_frame` (what the window shows becomes a description), and the gate of EN-02: the real old drawing against the engine on random scenes | `engine/passes`, `application/preview_panel/engine_view.rs`, `engine_parity_tests.rs` |
| 1.5 | the window: `EnginePreview`, the shown texture, all the aids the old drawing has (outlines, handles, the symbol's places to take hold of), the fallback | `application/preview_panel/engine_preview.rs` |
| 1.6 | **the guard** (EN-05): a limit on every loop of a shader that others wrote, errors with line and column, a watchdog that gives a frame that does not finish a new graphics device, effects of strangers switched off for the run, three replacements within a minute stop the engine | `engine/guard.rs`, `effect_shaders.rs`, `replacement.rs`, `worker.rs` |
| 1.7 | **adaptive quality** (smaller frames when the card cannot keep up, whole ones when the picture is still), frames that come from the engine's cache are counted, the **benchmark**, the nine pictures of what differs, the mutation campaign | `engine/quality.rs`, `benchmark.rs`, `examples/engine_benchmark.rs`, `docs/engine-comparison/` |

### The gate of the plan, line by line

| the gate | where it stands |
|---|---|
| Random projects drawn by the engine and by the oracle agree within a stated tolerance and a pixel budget | **Met [V]** on WARP and on lavapipe. **120 random scenes on every run of the tests** and **400** in the longer run that the numbers were measured with (both pass on both systems at the end of the phase): every pixel within **2 levels of 255** except on the edges of layers, where **at most 2 pixels in a frame of 320 by 180** differ more, each within 0.05 of a pixel of an edge (which of two layers a pixel shows when the edge goes through its middle). Text, the symbol and video in planes have tests of their own. |
| A typical stack (four layers, two blurs and a lookup) plays at **1080p60 on the integrated graphics**, measured by a benchmark in the repository | **The benchmark is built and run [V]; the measurement the gate asks for is not made, and it is yours** (SF-01). On the software adapter of this computer the whole stack makes **1.0 frame to the second at 1080p and 8.2 at 640 by 360**, which is a processor and says nothing about a card. |
| A deliberately bad shader gives an error and never a hang | **Met [V] on the software adapters.** A mistake in a shader says **where** (line and column of the shader's own text). A loop that never ends is cut at 100 000 passes and the frame is made in 14 to 19 milliseconds (8 by 8 pixels). A shader that is only very slow is caught by the watchdog **in a process of its own** that is killed by its id if it is not done: the frame was given up on and the device replaced 2.25 seconds after it was asked for (limit 2 s) on WARP, and the next frame took 4.4 ms on the new device. **Not tried on a real card [B]**: what a real card's own reset does, and in what order, is the one thing the guard could not be shown on. |
| CI is green on software adapters | **Met [V]** at the last commit that CI has read before this report (`e6ea639`): Windows, Ubuntu and macOS (which has no software adapter: its engine tests say so and are skipped), the licences, the tools. |

### The numbers

* **Library tests, at the end:** Windows **3846 passed, 0 failed, 7 ignored**; Ubuntu in WSL **3845 passed, 0 failed, 7 ignored** (one test is for Windows only). `src/engine` has 220 of them, the window's use of the engine 62 more (51 in the preview panel, 11 of a whole window with an engine); the 7 ignored ones are
  the longer parity run, the writer of the pictures, the benchmark of the gate (minutes), the two children of the hang tests, a 4 GB archive and the writer of the old indicator previews. Integration tests: the ones that need ffmpeg 9 ran on CI (green); a new one that writes the pictures of the edges of colours is ignored.
* `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items`: clean **[V]**. `reuse lint`: not installed on this computer; **green on CI** at the last commit, and this report adds only files under `docs/`, which `REUSE.toml` covers.
* The engine and preview tests were run **ten times in a row** (339 tests each) without a failure **[V]**.
* **The mutation campaign:** 229 planted mistakes in the engine and its two files of the window, **167 noticed**, 50 not, 12 that did not compile. Of the 50, **27 were real gaps and now have tests**, 23 can change nothing anyone could see (section 10 of the design lists them). **Two of the gaps are worth telling** (section 6, point 4).
* **The benchmark** (software adapter, release build; the table of the seven scenes is in section 10 of the design): the whole stack 1.0 frame to the second at 1080p, 8.2 at 640 by 360; a black frame (output and read-back alone) 4.3 ms at 1080p; one picture of video sent to the card, 313 ms at 1080p. **A processor made these.**

## 2. What can be seen (the list of visible differences)

All of it, with the pictures, is in section 11 of the design and in `docs/engine-comparison/` (nine pictures of three panels: the old drawing, the engine, and what is different, made sixteen times as bright). In short:

* **Almost nothing**: colour effects, translucent edges, text, the symbol and turned pictures are within **1 or 2 levels of 255** of what the old drawing makes (pictures 01 to 05). The old drawing's rectangle for a picture that has not come has a soft edge and the engine's is hard (one pixel).
* **One thing that can be seen, at some sizes of video only**: where a video is shown at **two thirds** of its size (or any size that is not a half or a whole number of times), the **edges of saturated colours** (red against green, a coloured word on a plain background) are harder in the engine than in ffmpeg's picture:
  up to **111 levels** in single pixels at the hardest stripes there are, **24 on average** over the whole picture of stripes, and 3109 of 9216 pixels more than 10 levels apart (picture 08). Smooth colours differ by up to 4 levels, 2.5 on average (09). The reason: ffmpeg scales the colour planes of the video in more bits and with its own filter; the engine is given the planes ffmpeg made at that scale.
  **For your eye. This is question 3 below.**
* A bias of about a level in smooth colours (ffmpeg's whole-number arithmetic cuts off, the engine rounds).
* When zoomed in far enough to see pixels, the blocks are pixels of the finished picture and not of each layer's texels; the symbol is drawn at the resolution of the frame, not of the screen; one window frame more of delay at most between a picture arriving and being shown.

## 3. Decisions

**Mine, which stand unless you object** (the numbers are those of the design's table of decisions, section 2):

* **E8: the engine is not the default.** It is started by `FASTFORWARDRS_PREVIEW=engine` and by nothing else. A preview that was wrong on a card nobody has tried it on would not fall back (only an engine that *stops* does). When you have looked, the default is one line (`start_the_picture_engine_if_it_is_asked_for` in `fast_forward_application/mod.rs`). **For your decision.**
* **E2: the effects of a clip run before it is placed**, on the clip's own picture (the plan's list had placement first). It is what today's hue shift does, and a small clip does not make an effect work on the whole canvas. **E3: the hue shifts of a layer are one fused colour-matrix call** (one matrix, one clamp), as today. **For your confirmation.**
* **The engine's own shaders do not go through the guard in debug builds** (the plan said they would, to exercise the road): debug and release would then run different shader text. A test sends each of them through the guard and checks that nothing is added; another **runs the engine's own colour-matrix shader as a shader that was given to it** and gets the same picture, pixel for pixel.
* **When a frame does not finish, every shader effect that was in it is switched off for the run**, not one: the thread cannot tell which pass hung. A false alarm costs an effect until the program is started again; a shader that hangs the card, switched on again, would be the same hang and then the third replacement, which stops the engine.
* **The governor counts frames in a row**, not a moving average, and does not count frames that come from the engine's cache or that come while the clock is stopped (a project that loops over a few frames would otherwise climb to a size the card cannot keep up with). The numbers (80 % and 35 % of a frame, 2 late frames, 10 comfortable ones, 120 ms before a still picture is made whole) are **first guesses**.
* **The gate of EN-02 is at the level of `PreviewContent`**: the level at which both drawings are given the same thing. Random *projects* would add only the conversion from a project to `PreviewContent`, which the old drawing's own tests and the window tests cover.
* **A device that was replaced is not announced by the window** (it is a hiccup, and it is recorded as the last problem of the preview), and which effects were switched off has nowhere to be shown until there are effects of strangers in projects (phase 6).

**Yours, as before:** the guard as recommended, with the render process kept open and not built (accepted 2026-10-15, and built as accepted); the engine on its **own device** (the engine's device is made from the window's adapter, which is why it cannot be started when the window is not drawn by wgpu).

## 4. Tests rewritten on purpose

**None.** Six files of old tests were touched **so that they still compile**, because a decoded frame is now a picture or planes (`.into()` where one is made, `.rgba()` where its pixels are read) or because a name is imported from somewhere else: `zoomed_pictures_tests.rs`, `textures/tests.rs`, `playback/supplier/tests.rs`,
`tests/journey/mod.rs`, `tests/media_frame_stream.rs`, `tests/media_in_place.rs`. No number they check, and nothing about how they check it, is changed **[V: read from `git diff` against the commit before the phase]**. All the old window tests pass as they were.

## 5. Limits

* **Nothing of this has run on a real graphics card** (SF-01). The things that could differ there and that no software adapter shows: the order of the operating system's reset and the watchdog, what edge pixels of turned pictures do on each vendor's rasteriser (the same triangles give the same coverage by the rules; **[B]**), the speed of everything.
* **The export does not use the engine** (phase 2). The preview and the export are therefore two renderers for now, which is the plan.
* **The cost of the round trip** (read back, cast, upload) on a real card is **not known**: guessed at 2 to 4 ms at 1080p; the benchmark's read-back will say.
* **Shader effects are a first form** (`effect_shaders.rs`: a fragment shader, a uniform of at most 64 numbers, the layer's picture and a sampler); the host of phase 6 gives them more. The **first-use probe** (a shader drawn small before it is drawn big) is phase 6's, and a render process is not built (the interface leaves it possible).
* A video that is not 8-bit `yuv420p` with a matrix and a range the engine knows (`yuvj420p`, 10-bit, 4:2:2, 4:4:4) is decoded to RGBA as it always was; phase 3 takes them.
* **Real-time tests** (`application_playback`, `application_sound`) are still warnings on CI, as set in phase 0.

## 6. Mistakes

Mine, found, and corrected; the first two before they could be committed, the others by CI or by the campaign.

1. **In the first version of the device replacement I kept a handle to the old device on the engine's thread.** Letting go of the **last** handle of a device whose card is still running a draw **never returns** (found by an experiment: 60 seconds were watched), so the engine's thread could hang after telling the window that it had replaced the device. The old renderer is now the only owner and is let go of on a thread named for it, and there is a test of the hand-over. *What I could not establish:* which handle was the last in the first version (my first version's own test passed both times it was run).
2. **My first idea for the colour planes of a video was a blend** (what a bilinear scale would make of them), and it was off from ffmpeg's picture by up to 82 levels; replicating the chroma as ffmpeg does is within 3 at one to one and two to one. Found by measuring against ffmpeg in piece 1.3 and decided there (section 9 of the design).
3. **A test of mine (1.6) raced on Windows CI**: it took a late duplicate answer for the new one. Found by CI at `ec625f6`, mended at `e6ea639` (the helper now waits for the answer it asked for), and stressed under load before the push.
4. **Two of my own tests could not see what they were written for**, found by the campaign:
   * the test of the benchmark's blur ran both of its passes, so a pass that made **the opposite of every colour** was undone by the other (half floats keep numbers below nothing between passes). Each pass is tested alone now.
   * the test that dropping the engine **waits** for its thread looked at what the thread had done right after the drop: when the thread was fast enough, the mistake passed (it was noticed in 5 runs of 6). It now makes the thread slow to say that it stopped, so that it does not depend on who is faster.
5. **Six of the 29 mistakes that survived the second pass looked unobservable when I first read the list**, three of them ones that I had not marked for a second look. Planting each again by hand showed they were not. The 23 that remain are listed in the design with the reason for each; **that they change nothing is my reading of the code, not a proof.**

## 7. Verified versus believed

**Verified [V]:** everything in the numbers section above; the pictures of the differences (written by the tests that write them); that each of the 27 gaps of the campaign is noticed now (the first 21 by a second pass of the campaign, the last six by planting each again by hand and running the tests); that the hang is caught in a process of its own on WARP (2.25 s) and on lavapipe (4.04 s for a limit of 4 s);
that a destroyed device says nothing until it is asked, accepts pictures and fails every frame at once (WARP); that a runaway draw on a software adapter goes on using the processor until the program ends (so the replaced device's words say so when the adapter is one).

**Believed [B]:** that a real card behaves as the software adapters do about the guard (its reset may come before or after the watchdog, and other programs that use the card may be affected: SF-01 is exactly why this was not tried); that the 23 unnoticed mistakes cannot be seen; that hardware bilinear sampling agrees with egui's four taps on a real card as it does on WARP (to 1 level);
that VP9 and AV1 files tagged full range arrive as natives like the one that was tried (an `rawvideo` in Matroska); that the numbers of the governor are right (they are guesses).

## 8. What I need from you

1. **Run the benchmark on the integrated graphics and send me what it prints.** The last line says whether the gate (60 frames to the second at 1080p) is made. With no adapter named, it takes the integrated graphics if there is one.
   ```text
   cargo run --release --example engine_benchmark -- --list
   cargo run --release --example engine_benchmark
   ```
   This computer lists an NVIDIA RTX 3090 as adapters 0 (Vulkan) and 2 (Direct3D 12) and an AMD Radeon integrated graphics as 1 (Vulkan) and 3 (Direct3D 12). **The window's own adapter is probably the RTX 3090** (eframe asks for the high-performance one **[B]**), and the engine is made from the window's adapter, so the run that tells how the preview will go
   is `-- --adapter 0` or `-- --adapter 2`; the integrated one is the gate. Both, if you will; a run at 4K is `-- --adapter 0 --size 3840x2160`.
2. **Look at the preview with the engine**, on a real card, with a few of your projects, and decide whether it becomes the default. In PowerShell:
   ```text
   $env:FASTFORWARDRS_PREVIEW = "engine"; cargo run --release -- "C:\path\to\project.ffrs"
   ```
   What to look at: that it looks like the old preview (start it without the variable and compare); text and turned pictures; the symbol; what happens while a project plays on a slow machine (the picture goes to a half and then a quarter of its size, and comes back whole when it is still); and that a window whose engine has stopped (it stops only if the card has had to be replaced three times within a minute) goes on with the old drawing and says so.
3. **Is the difference at the edges of colours acceptable?** (Section 2, picture 08.) The other way is to have ffmpeg give the planes at the video's own size and let the card scale the picture: the edges would then be the card's at every scale and no longer ffmpeg's at any, and four times as many bytes would go to the card for a video shown at half its size.
4. **Confirm E2 and E3** (effects before placement; hue shifts fused into one call).
5. **Should sound (LADSPA) move earlier**, to straight after phase 4? (Still open from before; it costs the other phases nothing.)
6. **Should the window say when the engine replaced its device** (a message, or only a line in a log)? Today it says nothing.

## 9. What comes next

**Phase 2: export through the engine** (size M). The same engine renders the frames, converts them to the encoder's YUV on the card, reads them back through a ring of buffers and pipes them to ffmpeg as raw video; a command-line export (no window) is the first user of the message-shaped interface; then the old export code is deleted, with its tests kept as the engine's oracle.
Its gate is that **preview and export are bit-identical on one machine**. **Prompt effort for it: high.**
