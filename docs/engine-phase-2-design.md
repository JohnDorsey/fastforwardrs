# Phase 2: export through the engine (design)

Written 2026-10-16, before any code of the phase, from reading the code that is there (section 1) and from three experiments (section 1, marked **[V]**). It says **what is built, in which pieces,
how each piece is checked, and what is decided**. The plan it carries out is [phase 2 of the plan of action](plugin-system-plan-of-action.md) (EN-03, EN-10, EN-11); the engine it builds on is
[phase 1](engine-phase-1-design.md) and its [report](engine-phase-1-report.md).

Tags as in phase 1: **[V]** verified by me by reading the code or running something; **[D]** decided by the maintainer; **[P]** my decision, which stands unless the maintainer objects; **[B]** my belief or
estimate, not checked.

## 0. What phase 2 is, in one paragraph

The finished video is made by the same engine that makes the preview. Today the export is one ffmpeg process given a very long filter graph (decode, cut, speed, scale, turn, colour, lay over, join) built by
`media::export`; after this phase ffmpeg only **decodes** the source files (one process for each stretch of a clip, as the preview does already) and **encodes** (one process, given the finished frames as raw video on
its standard input, with the sound made by the same sound filter graph as now). In between, for each frame of the project, the engine draws the picture from the same description the preview uses, converts it
on the card to the planes the encoder takes, and gives them back through a ring of read-back buffers. **Nothing visibly new is promised** (compatibility mode, CM-20): the picture is what the preview shows, and the
sound is what it was. What gets better: animated values are exact at every frame (the export no longer cuts animation into stretches and no longer refuses a project with "too much animation"), the export no longer
depends on how long a filter graph ffmpeg can set up, and the preview and the export are the same picture by construction. A command-line export (no window) is the first user of the engine without the window (EN-11).
The old export code is deleted at the end, with the tests that only tested it.

**Carried over from phase 1:** the benchmark's gate on the integrated graphics is open (49.8 frames to the second against 60). This phase builds what bears on it (a read-back that overlaps the next frame, pictures that are
sent in every frame and reuse the card's memory), the benchmark gets a scene and a mode for them (section 8), and the maintainer is asked to run it again.

## 1. What is there today

* **The export** [V, read 2026-10-16]. `plan_export(project, settings, render_text) -> ExportPlan` (`media/export/plan.rs`): plans the picture (`plan_pictures`) and the sound (`plan_sound`); cuts every section that has
  keyframes into **stretches** over which everything is in one place in whole pixels (`animated_sections.rs`; refuses a project that needs more than 1500 stretches: `ExportPlanError::TooMuchAnimation`); renders every
  text picture and the fast-forward symbols up front as PNG inputs; builds the filter graph text (`filter_graph/{pictures,sound}.rs`). `VideoExport::start` writes the graph and the PNGs to a temporary folder and
  runs **one ffmpeg**: `-i` each source file, `-i` each PNG, `-/filter_complex graph.txt`, `-map`, libx264 (`-preset medium -crf 18`, or `mpeg4 -q:v 2` when ffmpeg has no x264), `-pix_fmt yuv420p`, aac 192k, `+faststart` for
  mp4/mov, `-progress pipe:1`. It writes to `name.exporting.ext` beside the destination and renames when complete; cancelling kills ffmpeg and removes the file; a failure reports the end of ffmpeg's error output.
* **How the filter graph treats the picture** [V, read]: each source is resampled once with `fps=<rate>:round=near`, then for each section `trim=start_frame:end_frame`, `select='not(mod(n,speed))'`, `setpts=N*…`; each layer is
  `scale` to its box, the colour matrix (`ColourMatrix::ffmpeg_filter`), `rotate` (turned layers are made `rgba` with clear corners), `overlay` on a black `color` canvas, sections joined by `concat`. **A video that
  is at the back, fills its box inside the picture and is not turned is the picture itself** (padded with black where it does not reach): the old export keeps its YUV as it is.
* **The old export's output tags and colours** [V, experiment 2026-10-16, a one-frame test video of four flat colours, exported unedited]: the picture passes through **unchanged** for every tag tried (the
  decoded colours of the source and of the exported file are equal, pixel for pixel), and the **tags follow the source**: untagged stays untagged (`color_space`, `color_range` absent), `bt709` + `tv` stays `bt709` + `tv`,
  `smpte170m` + `tv` stays so, and `bt709` + `pc` stays `pc` and makes the pixel format `yuvj420p`. (Primaries and transfer were not in the test videos.) *This is what "the exported files' tags are the same as before"
  has to mean; section 5 says how the new export gets as near to it as an RGB picture can.*
* **The preview's decode** [V, read]: `media::FrameStream` runs one ffmpeg for a stretch of a clip: `-ss` (half a frame early), `-t`, `settb=AVTB,setpts=PTS-(…),fps=<rate>:round=near:start_time=0`, `select`,
  `setpts=N*…`, `scale=W:H:flags=fast_bilinear`, `format=rgba|yuv420p`, rawvideo on a pipe; it is built to deliver **the very frames the export has** ("frame `k` that comes out is the clip's frame `k`"; the tests of
  `media_frame_stream` check the frame numbers it delivers against the arithmetic of the export's graph, for sources of other frame rates, rounded timestamps and variable frame rates, **not against an export**: piece 2.4 does that). It delivers the video **as big as it can be inside the picture, in its own shape** (`delivered_size`).
* **The engine** [V]: `Engine` (thread + messages) with `Upload`, `Forget`, `InstallEffect`, `Render` (the newest request wins, older ones are answered with nothing), one frame at a time, RGBA read back through
  `ReadbackBuffers` (`FrameRenderer::render` records, submits and waits for each frame; the converted picture of an upload is a new texture each time, not from the pool). The preview's `describe_frame` (in
  `application/preview_panel/engine_view.rs`) turns a `PreviewContent` into a `FrameDescription`; the window builds the `PreviewContent` for a frame in `drawing.rs::preview_content`.
* **Layers** [V]: the plan's `Layer` has `placement_at(frame)`, `colour_matrix_at(frame)` and `text_at(frame)`, which work out the keyframes at **any** frame exactly; the export does not use them (it uses the stretches).
* **The tests that hold the export** [V]: `tests/media_export*.rs` (unedited, edited, random edits, tracks, animation, effects, text, symbol, sound; they call `plan_export` and `VideoExport::start` and look at
  the decoded frames: frame numbers in the counter video, boxes of what is not black, colours with a tolerance, durations, pitch), `application_journeys.rs` (whole journeys through the application, among them an export),
  and about 3000 lines of unit tests of the plan and the graph text in `src/media/export/` and `src/composition/`.

## 2. Decisions

| # | Decision | Why | Tag |
|---|---|---|---|
| X1 | **ffmpeg decodes and encodes; the engine composes.** One decode process for each *run* of a video clip (the stretch of the timeline over which one clip is seen, across the sections that other clips cut it into), one encode process for the whole video. | The decode is the preview's (`FrameStream`), so the export has the frames the preview has; the encoder reads raw video on a pipe. A run is not restarted at a section boundary: a long video with a hundred captions over it is two hundred sections and one stream. | [P] (plan: "piped to ffmpeg as raw video") |
| X2 | **The preview and the export describe a frame with the same function.** `PreviewContent` and its parts, `describe_frame`, `HowToMakeIt`, `PictureSupply` and `SentPictures` move from `application/preview_panel` to a new module `frame_scene`, below the application; the preview panel re-exports them under the names it has, and **its tests do not change**. The export builds a `PreviewContent` for each frame from the plan (as `drawing.rs` does for the window) and calls `describe_frame`. | EN-03 by construction: two describers could differ; one cannot. A command-line export must not need the window's code. | [P] |
| X3 | **The export's frames are the preview's 8-bit frames, converted.** The engine draws the frame to the 8-bit RGBA texture it already makes (`output` pass), and a **YUV pass reads that texture** and writes the three planes. Preview and export are therefore the same RGBA8 frame at the same size, and the planes are a function of it (tested against a CPU reference within 1 level). | "Bit-identical on one machine" becomes a test of the RGBA8 stage (identical bytes through both roads) plus a test of the conversion; rounding twice costs at most a level. | [P] |
| X4 | **Planes are the encoder's layout and 4:2:0 with a 2×2 box average for the colour** (colour siting: centred; H.264's default is left-sited, a half pixel of colour shift, **[B]** not visible). Limited range. | Simple, exact for the pictures that came as planes (replicated chroma averaged back is itself). | [P] |
| X5 | **The matrix and the tags of the output follow the first video of the project** (section 5): the matrix the engine used to make colours of it, limited range, and the tags that video states (matrix, primaries, transfer; range `tv`). A project without video, or a first video that states nothing, is BT.601 limited and **untagged**, as the old export left a video that stated nothing. | The nearest an RGB picture gets to the old export keeping the source's YUV and tags. Phase 3 replaces it by an output colour space the user chooses (CM-05). | [P], for the maintainer's eye |
| X6 | **The video is delivered to the engine as big as the layer needs it, made by ffmpeg with its default (bicubic) scale**: for a layer whose box does not change, exactly its box in pixels; for one that does, the largest box it has in the run (capped). For a video that fills the picture that is the picture's size, which is what the old export did, so full-picture video is 1 to 1 in the engine. | Quality: the engine samples with a bilinear filter and no mipmaps, which is good to about 2 to 1 and aliases beyond; the old export scaled with swscale's bicubic whatever the ratio. | [P] |
| X7 | **Every frame is made, in order** (`RenderForExport`; the preview's `Render` stays "newest wins"), **two frames in flight**, answered by `ExportFrame` with the planes. The engine's watchdog and device replacement apply as they do for the preview. A frame that fails, or a device that is replaced, **ends the export with a message** in this phase (retrying the frames in flight is a later refinement, section 10). | EN-10; the export must not drop frames and must not stall on the graphics card for ever. | [P] |
| X8 | **The pictures the engine is sent for a frame are forgotten as soon as no later frame uses them**, and the textures of forgotten pictures go back to the pool instead of being freed (streaming video would otherwise allocate a texture of 16.6 MB at 1080p for every frame). | The carried-over gate (section 8) and any long export. | [P] |
| X9 | **Text is drawn on the export's own thread when it is wanted, and the drawn pictures are kept while they are in use** (not made up front for every way the text will ever look). | The old export made a picture for each look up front; with every frame exact, an animated text can look different at every frame, and 600 pictures of 8 MB do not fit in memory. | [P] |
| X10 | **A video that ends before its run does holds its last picture** for the frames it lacks (and says so in the end message); one that delivers no picture at all fails the export with ffmpeg's words. | A truncated or oddly counted last frame must not fail an hour's export at minute 59. The plan already leaves out the frames it knows a file cannot supply. | [P] |
| X11 | **`TooMuchAnimation` goes**, and with it `animated_sections.rs` and the quantising of turns to a tenth of a degree. | The reason for them was ffmpeg's graph setup time. | [P] (plan: "no longer quantised") |
| X12 | **The adapter of the export is a setting of the environment**: the window's adapter in the program, the best one the computer has in the command-line export, a software adapter in the tests (so that no test runs on the maintainer's card: SF-01). | The same rule as the preview. | [P] |
| X13 | **The command-line export is `fastforwardrs --export <output> <project>`** (a `.ffrs` or `.json` file), no window, progress on the error stream, exit code 0 on success. | EN-11. Last piece of the phase: the library entry it calls is what the application's export calls. | [P] |

## 3. The pieces and where they live

```
application/                 frame_scene/ (new)                 video_export/ (new)                       engine/
  preview_panel/               PreviewContent & parts             plan.rs     ExportPlan (sections, runs,    request: RenderForExport
    engine_view.rs (re-exports)  describe_frame, HowToMakeIt        sources, sound, colour, tags)           event:   ExportFrame
    mod.rs (re-exports)          PictureSupply, SentPictures      runs.rs     one decode stream per run      frames: begin / finish, ring
  fast_forward_application/                                       frames.rs   the PreviewContent of frame f  yuv pass (rgba8 -> planes)
    export_flow.rs  --------------------------------------------> runner.rs   VideoExport (thread, engine,    pool for converted pictures
    drawing.rs (preview_content)                                              encoder, progress, cancel)
main.rs  --export  -------------------------------------------->  command line, encoder arguments, tags
media/export/ keeps: the sound graph (used by the live preview too), progress parsing, VideoEncoder
```

`video_export` is above `frame_scene`, `playback`, `composition`, `media` and `engine`; nothing below it depends on it. The application's `export_flow.rs` becomes a thin caller, as the command line is.

### 3.1 The run of a video

A **run** is one clip's video over the sections it is in: the source, the first source frame, how many source frames (the sum over its sections), the speed, the first and last timeline frame, and the size it is
delivered at (X6). It is found by walking the sections: a video layer whose picture comes right after the one before it (`runs_on`) belongs to the open run of its clip. Each run is one `FrameStreamRequest`
(`delivery: Yuv420p` when `VideoStreamFormat::as_native_yuv420p` says the source can be had in planes, `Rgba` otherwise; sizes and the new `how_to_scale` as in X6). At most a few runs are open at once: a run is
started a second or so before its first frame (so that ffmpeg's start-up and seek are not waited for) and dropped (ffmpeg killed) when it ends.

### 3.2 One frame of the export

1. The frame `f`: the section it is in, the layers of the section at `f` (`placement_at`, `colour_matrix_at`, `text_at`), the picture each video layer has at `f` (the next one of its run), the decoded image
   of each image layer (decoded once, at the size that fits the picture), the symbol if the front video is accelerated.
2. A `PreviewContent` of these, described by `describe_frame` with a supply that makes the picture of a text or a symbol when it is wanted and sends pictures to the engine.
3. `RenderForExport` for the frame; the pictures no later frame uses are `Forget`-ed.
4. The engine answers in order with the planes; they are written to the encoder's standard input; the progress the encoder reports (`-progress`) is what the application shows.

A frame whose description is the same as the one before it (a title card, a still picture over black) is not made again: its planes are written once more.

## 4. The engine side

* **`RenderForExport { serial, description, colour: YuvColour }`** and **`ExportFrame { serial, frame: Arc<ExportedFrame>, statistics }`** (`ExportedFrame`: width, height, one `Vec<u8>` of the planes one after the other, Y then U then V, which is
  exactly the `yuv420p` that ffmpeg reads as raw video). Not dropped, not collapsed, answered in the order asked.
* **`FrameRenderer` splits a frame in two**: `begin_frame(description, output)` records and submits and returns what is pending; `finish_frame(pending, deadline)` waits and takes the bytes. `render` is the two
  one after the other (the preview and every test of the engine go on as they are). The worker begins the next ordered frame before it finishes the one before (two in flight) and finishes what is left at the
  end of the batch of requests it took, so latency is bounded.
* **The YUV pass** (`shaders/rgb_to_yuv.wgsl`): the 8-bit frame is read with `textureLoad`; the luma pass writes an `R8Unorm` texture of the frame's size; the chroma pass writes two `R8Unorm` textures of half the size
  (two colour attachments of one pass), each pixel the average of the four colours of its square converted to a colour difference. The matrix and range come from the same `YuvColour` that the other direction uses
  (`YuvColour::conversion` and the new `YuvColour::forward_conversion`, derived from the constants Kr and Kb of the standard, and tested to be inverses). The three textures are copied into read-back buffers (rows padded
  to 256 bytes, taken out of the padding when read) and put one after the other.
* **Pictures come from the pool**: the texture a video picture is converted into goes back to the `TexturePool` when the picture is forgotten, and the plane textures already do; the arena of the numbers of the conversion
  is reused. (Measured in the benchmark's new scene, section 8.)
* **No frame cache for the export** (`most_cached_frames` is 0 in its engine): the export asks for a frame once.

## 5. Colour, tags and the encoder

The encoder is given `-f rawvideo -pixel_format yuv420p -video_size WxH -framerate N/D -i pipe:0`, then the source files that the sound is made from as further inputs, the sound graph as before
(`build_audio_graph`, its inputs shifted by one), `-map 0:v` and `-map [finished_audio]`, the same encoder and sound options as before, and `-progress pipe:1`. The standard input is the pipe, so `-nostdin` is dropped (ffmpeg
does not read the keyboard from an input that is `pipe:`).

**Tags** are written with a `setparams` filter on the video (`-vf`), and not with `-colorspace` as an output option (which in ffmpeg 9 converts: the comment in `tests/support/yuv_videos.rs`). The first video of the plan
(the back-most video layer of the first section that has one) says what they are: its `VideoStreamFormat`.

| the first video | the matrix the engine used for it | the output |
|---|---|---|
| states nothing (untagged) | BT.601 (the engine's compatibility rule) | BT.601, limited, **no tags** |
| `bt709` / `smpte170m` / `bt470bg` / `bt2020nc`, range stated or not | that matrix | that matrix, limited, tagged with the matrix, `range=tv`, and the primaries and the transfer if it states them |
| `pc` (full) range | its matrix | limited, tagged `tv` (the old export kept the full range and called the format `yuvj420p`; the picture is the same) |
| not in planes (10-bit, 4:2:2, RGB...) | what ffmpeg made of it | the matrix its tags name (601 if none), as above |
| none (text and pictures only) | – | BT.601, limited, no tags |

Why not copy the old export exactly: it kept the *YUV* of a video that filled the picture and converted everything else with swscale's defaults, so its tags and its colours depended on how the project was laid out (a
video that did not fill the picture was laid over a black canvas, which states nothing). One rule that is the same for every layout is the better choice, and it gives the old numbers back in the case that matters: a
video of one matrix, filling the picture, is converted to RGB with its matrix and back with the same one, and its decoded colours are the old export's within a level (tested, section 7).

## 6. Pieces and the gate of each

I stop after each piece at the point where the tree is green, so that the work can be committed (by the maintainer, or by me when the only reason is to get CI results). The tests of a piece are written before the
code they test, and the old tests stay as they are unless the piece says otherwise.

| Piece | What | Gate |
|---|---|---|
| **2.1** | `frame_scene`: the types and `describe_frame` move; the preview re-exports | **all existing tests unmodified**, clippy, fmt, doc; the symbol's place is worked out with `speed_indicator::place_indicator` and a test says it is the number `SymbolOnScreen::of` gives |
| **2.2** | the YUV pass in the engine and the CPU reference | against the reference within 1 level on random pictures, extremes (black, white, the six colours), odd and even sizes, three matrices, two ranges; forward and inverse matrices are inverses; published numbers (white is 235/128/128 in limited range) |
| **2.3** | `begin_frame`/`finish_frame`, the ring, `RenderForExport` in the handle and the worker, the pool for converted pictures, the benchmark's scene and mode | frames come in order and none is dropped (many requests at once), two in flight (a test that counts), the planes of a frame equal what `render` + the CPU reference give, failures end the stream with a message, a device replacement is reported; the preview's tests untouched |
| **2.4** | `video_export` plan and runs: the sections to runs, the streams, the pictures of a frame | against the counter video with real ffmpeg: the pictures of a run are the frames `expected_source_frames` says, for random edits, speeds, overlapping clips, a video shorter than its clip |
| **2.5** | the runner and the encoder: `plan_export` and `VideoExport` in their new form, the sound, tags, progress, cancel, errors | the tests of `tests/media_export*.rs` ported (their calls changed, their assertions as they are), the tags table above, cancelling leaves nothing, a failing source fails with ffmpeg's words |
| **2.6** | the application (`export_flow.rs`, the adapter of the environment) and the command line | `application_journeys` unchanged, a test of `--export` end to end, the application's menu and messages as they were |
| **2.7** | compare the old export with the new on a corpus of projects (once, with the old code still there), then delete the old code and the tests that tested only it | the comparison's numbers are in this document (section 9); the deleted tests are listed; what compared the plan with plain evaluation is kept and moved |
| **2.8** | docs, README, features list, the benchmark run on the software adapter, the mutation campaign over the new code, the report | the usual end-of-phase list |

## 7. How it is checked

1. **No-GPU tests**: runs (sections to runs, source frames of a run, what a run needs), the encoder's arguments (text), the tags table, the plan's errors, the sizes of delivery (X6), `forward_conversion`
   against the standards' published numbers, the CPU reference of the YUV pass.
2. **Engine tests on the software adapters** (WARP, lavapipe; skipped with a reason where there is none): the YUV pass against the reference; ordered rendering; the ring; the pool.
3. **Tests with real ffmpeg** (the existing ones, ported): everything `tests/media_export*.rs` checks. *Where an assertion would have to change, it is a finding: the cause is said and the maintainer sees it in the
   report; an assertion is not loosened to pass.*
4. **Preview and export are the same picture** (EN-03): the same project, the same decoded pictures, rendered by the preview's road (`Render`, RGBA8) and the export's (`RenderForExport` with a test hook that also gives the RGBA8
   stage): **identical bytes**; and the planes against the reference from those bytes.
5. **The tags** of an exported file for each row of the table in section 5, by ffprobe, and the round trip of a video of one matrix within a level of the old export's decoded colours.
6. **The end of the phase**: full suite, clippy, rustdoc, `reuse lint`, a mutation campaign over the new code, the README and the features list, and the report.

## 8. The benchmark, for the gate that is open

The benchmark (`engine::benchmark`, `examples/engine_benchmark.rs`) gets, in this phase and with the software adapter as its only test (SF-01):

* **a scene in which every frame sends a new picture of video** (1080p, in planes) before the frame is made, and the same stack as the whole stack: the cost of the picture that a playing video has and the
  benchmark did not have;
* **a mode in which frames overlap** (`--in-flight 2`): the time per frame when the next frame is recorded before the last one is read back, which is what the preview and the export can do with the ring;
* the last line still says whether the gate is made, **by the figure of the mode and scene that a real playing project is**, and says which it went by.

The maintainer is asked to run it again on the integrated graphics when the phase is done (the report says so). The stand-in blur and lookup are not changed (changing them after seeing the figure would be
choosing the question by the answer); the estimates of what a blur with bilinear reads and a real 3-D table would take off are in the design of phase 1, section 10, and are phase 4's to prove.

## 9. Differences expected, to be measured in 2.7 and shown

The old export and the new one are compared on a corpus of projects (unedited, edited, tracks, overlays, rotation, scale, colour, text, symbol, animation) with the old code still present. Expected, **[B]**:

1. **Colours** of a video that fills the picture: within a level of the old export (section 5). Of layers composed with others: the engine blends premultiplied in half floats and rounds once; the old export blended 8-bit YUV
   with ffmpeg's `overlay`. Up to 2 levels, more at translucent edges **[B]**.
2. **Edges of turned and scaled layers**: bilinear in the engine against bicubic scaling and the `rotate` filter's interpolation in ffmpeg: a difference at edges and fine detail, **[B]** a few levels at most.
3. **Chroma** of a video not at its own size: the accepted difference of phase 1 (the planes are the ones ffmpeg made at the delivery size), and, since the delivery size is the layer's box (X6), mostly the same
   ffmpeg scaling the old export did.
4. **Animation** is exact at every frame (the old export snapped a position to a whole pixel and a turn to a tenth of a degree per stretch).
5. **Tags** as in section 5.
6. **Bigger changes of behaviour, on purpose**: no "too much animation" refusal; a video that ends early holds its last picture (X10).

## 10. What I do not know yet [B]

* Whether the macOS runners of GitHub have a graphics adapter the engine can use (Metal on a virtual GPU). The engine tests there are skipped today because they ask for a *software* adapter. If there is one, the
  export tests run there on it (a runner's, not the maintainer's: SF-01 allows it) with an environment variable that CI sets; if not, the export tests are skipped on macOS with a reason and the coverage of the
  export there is lost. A CI step will list the adapters (as an annotation) with the first push of the phase.
* How fast the decode, the engine and the encoder together are on the maintainer's machine; the figure that matters is frames to the second of an export of a typical project, which the report asks for.
* Whether an export that loses its graphics device should go on (the frames in flight are asked for again, the pictures sent again) instead of ending. Phase 1 made the engine able to replace the device; the export
  driver does not use it yet (X7). A long export on a flaky driver would want it.
* Whether the `fps=round=near` resample on the whole source (the old export) and on a stretch read from the middle (the stream) always agree for variable-frame-rate sources; the preview's tests say so for the
  sources they use, and 2.4 repeats them against the export's frame numbers for the cases of the existing tests (25, 24 and 12 frames to the second sources in a project of 30).
