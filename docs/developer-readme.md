# Developer readme

Where the program was tested, how it is built (the layers and what each is for), how it is tested, and its known limits.

*This file holds sections that used to be in the README, as they were written, so that the README could be short. Where a section says "see *Settings*" or the like, the section is in one of the files that `docs/README.md` lists.*

## Status

| | |
|---|---|
| Tested on | **Windows 10**, and **Ubuntu 26.04 in WSL 2** (since 2026-10-15: the library's tests all pass there, and the integration tests, with ffmpeg 9: see the *ffmpeg* row). macOS: by the CI of GitHub only (see the next row), and nobody has looked at the window on a Mac. |
| Written for | Windows, macOS and Linux. The only platform-specific code is the `.exe` suffix when looking for ffmpeg and hiding the console windows of the ffmpeg child processes (both `cfg(windows)`); a couple of unit tests have a Unix and a Windows variant, and only the Windows ones have been run. The CI of GitHub (`.github/workflows/ci.yml`) builds it and runs the checks on Windows, Ubuntu and macOS (a Mac of the runner, with ffmpeg from Homebrew); on the Mac the headless tests pass apart from the ones that are reported as warnings by design, and the tests of the picture engine and of the export skip there, because the runner has no software graphics adapter (one step runs them on the Metal device of the runner, with failures reported as warnings; they pass there, which is a virtual machine's graphics and not a real Mac's); on Linux it builds and runs the tests, and `tools/linux/run_in_wsl.sh` runs a command on a copy of the project in WSL. On Linux the window system libraries that `eframe`/`wgpu` need may have to be installed, and so may ALSA's development files (`libasound2-dev` on Debian and Ubuntu), which the sound output (`cpal`) is built with. Text is drawn with the computer's own fonts, which `cosmic-text` finds on each system. |
| Sound | The preview's sound goes to the computer's default output device. On the development machine the device opens and takes sound at the speed of real time (checked by a test that plays silence), and the right sound reaches recording stand-ins for the speakers at the right time (see *Testing*). That the sound is **audible** has not been judged by ear. |
| Rust | edition 2024 (Rust 1.88 or newer, which is when `let` chains and `as_chunks_mut`, that the code uses, became stable); developed with 1.99 |
| ffmpeg | **ffmpeg and ffprobe 9 or newer are required** (developed and tested against **9.0.2** on Windows and on Linux, with the GPL build of n9.0.2 from BtbN). An older one is not used, and the message that says so is shown with the places that were searched. The reason: **ffmpeg 8.0.1 (Ubuntu 26.04's) does not give the same pictures**: for a file whose sound starts before its picture (an `mkv` made by ffmpeg has the sound at -0.021 s), `-ss 0` there moves the picture's timestamps by that much, so the preview would show the first picture twice and every picture one frame late; ffmpeg 9.0.2 does not (checked with `framemd5` on the same file with both). Two integration tests fail with ffmpeg 8 for that reason. |

## Running it

fastforwardrs runs the `ffmpeg` and `ffprobe` programs for everything to do with video, so they must be
installed. They are looked for in this order:

1. the folder named by the environment variable `FASTFORWARDRS_FFMPEG_DIRECTORY`,
2. the folder fastforwardrs itself is in,
3. the `PATH`.

If they are missing the window still opens and says so.

```text
cargo run --release                                  # an empty window
cargo run --release -- "C:\path\to\project.ffrs"     # open a project straight away (a .ffrs or a .json one)
cargo run --release --bin fastforwardrs-export -- "C:\path\to\project.ffrs" "C:\path\to\video.mp4"   # export with no window
```

**Exporting without a window.** `fastforwardrs-export [--adapter best|software] [--settings <file>] [--allow-plugins] [--plugin-approvals <file>] <project file> <video file>` makes the finished video of a project
(a `.json` project, or a `.ffrs` one, which can have its videos in it) the way the *Export* menu item does, with no window, and tells its progress on the error stream,
a line for each per cent. The picture is made on the best graphics adapter the computer has (`--adapter software` asks for a software one, which is slow, for a computer
with no graphics card); the global settings are the editor's own unless `--settings` names another file. The exit status is 0 when the video was made, 1 when the export
failed (the reason is said) and 2 when the command line was not understood. It is a program of its own, and not an option of the editor, because the editor is a windowed
program on Windows, which has no console to write to. Anything that begins with a dash is taken for an option (a file whose name begins with one can be given as `./-name`).

**The picture engine.** The preview is made by a picture engine on the graphics card (design: `docs/engine-phase-1-design.md`), which is how it is made unless
`FASTFORWARDRS_PREVIEW=old` is set before the program is started (**the engine has been the default since 2026-10-16**; before that it was started by
`FASTFORWARDRS_PREVIEW=engine`, which still works, and says so in a message if the window is not drawn by wgpu): the preview is one
picture that the engine made of all the layers, and the window still draws the outlines, the handles and the places the mouse can take hold of itself. While a project plays and the card cannot keep up, the engine makes its frames smaller (a half, then a quarter) instead of late, and makes them whole again when the picture has been still for a moment. If the engine
cannot start, or stops, the program says so and makes the preview the old way from then on. If the graphics card is reset, or does not finish a frame in two seconds, the engine makes itself a new graphics device and goes on (a picture may be late), **and a message says so** (it waits for a question that is being asked, and is put under a message that is showing); if that is needed again within a minute and again, it stops, as above. The old way (the window's own drawing, one texture for each layer) is what the preview falls back to,
and is what `FASTFORWARDRS_PREVIEW=old` asks for. The export is made by a picture engine of its own, whatever this setting says (see *Exported video*). How fast the engine is on this computer's graphics card is measured by `cargo run --release --example engine_benchmark` (it says which adapters there are with `-- --list`);
it makes frames of four layers with two blurs, a colour matrix and a lookup on them, and says the frames to the second and what each kind of pass costs. **The gate is 60 frames to the second at 720p** (the size that the preview is made at) on the integrated graphics; it was 1080p until 2026-10-16, and `-- --size 1920x1080` measures that, as a figure and not as the gate.

## How it is built

The code is a library plus a thin `main`, in a Cargo workspace with two small crates beside it (`crates/ladspa-host`, the only code with `unsafe` in it, and `crates/ladspa-test-plugins`, a plugin library for the tests). It is layered, and each layer only depends on the ones below it:

| Module | What is in it |
|---|---|
| `project` | What a project *is*: its source files, its tracks (`project::tracks`: a `Track` is a row of clips with gaps, a speed, a gain, a height and a mute switch, and holds all the arithmetic of which clip is at which frame, what moves when a length changes and whether clips may be moved; a clip's `ClipId` is the track it was first made on and a number, so that it keeps its id when it is moved to another track, which `Project::move_clips` does for several clips at once or not at all), the speed and gain types, text objects, pictures, **placements** (`placement.rs`: a rectangle on the picture as fractions, with its middle on the picture and free to reach past its edges, an angle, how a corner drags and what a point is on), the fit of a video inside a picture, settings, and **animation** (`project::animation`: the table of properties that can be animated and the kind of value each has, the keyframes of one property, and the animation of a clip or a track, which is worked out from the racks). The keyframes are kept in **racks** (`project::tracks::rack`) in **drawers** (`drawer`) of the clips, the tracks and the keyframe crates (a crate is a kind of clip: `crate_clip.rs`); `project/racks.rs` puts together what all the racks that are of one thing say, `project/animating.rs` is the project's way of asking what a property is at a frame and of making a keyframe or changing a property at a frame, `project/rack_editing.rs` moves keyframes and racks about (`rack_merging.rs` merges racks and `keyframe_copying.rs` copies keyframes), `project/duplicating.rs` copies a clip or a track with what is in its drawers, and **effects** are data in `project/effects.rs` (an `EffectKind` is a small value that is a reference to an `EffectDescriptor`: the key of the type of effect, its version, what it is called and does, what it works on, and its values, each with an id, a name, a label, what it begins as and how its control looks; the program has some in itself, in `project/effects/built_in.rs`, with what each does, and a project file brings others, which are kept and do nothing; each effect in a chain is named by an `EffectInstanceId`, one number in that chain, and a value of an effect is the property `PropertyId::EffectValue { effect, instance, slot }`, which is what lets a chain have several effects of one kind, each with a name of its own in its chain) with `project/effect_editing.rs` putting them on and taking them off. A value of an effect is a `PropertyId` like any other, and `PropertyId::is_had_by_a_clip` and `is_had_by_a_track` are the one rule for whether a clip or a track has a property (the keyframes of an effect's value only where it is on). Pure logic: no files, processes or drawing. In whole frames (`SourceFrameIndex`, `TimelineFrameIndex`, `SpeedFactor`...). |
| `colour_effects` | The arithmetic of the effects that change the colours of a picture: a `ColourMatrix` (the hue rotation of CSS is one) and what it does to the pixels of a picture. No other part of the program is in it. |
| `composition` | Works out from a project what the finished video is: the **picture plan** (for each stretch of the timeline, the layers that are seen: their video, picture or text, where each is, its effects, and its keyframes, from which the preview works out where it is at a frame and how the colours are changed) and the **sound plan** (for each track, which clips are heard when, and how loud, which for a clip with keyframes is a loudness that changes; a clip muted by keyframes is cut into the stretches that are heard). Plain logic. The live preview and the export both use these plans, which is what makes them agree. |
| `project_file` | Saving and loading the project JSON, including the older versions, and as a `.ffrs` archive that can have files included in it (`archive`: a zip file of stored entries, with the names of the entries checked before they are believed, that is written whole to a temporary file and renamed into place when it is saved as another file, and added to (`layout`: its own reader and writer of the zip directory, with zip64, that finds the last whole end record and cuts off a save that was cut off) when the project's own file is saved, so that the files in it stay where they are and are read there; `WritePlan` works out what is to be written first, so that a copy of a great deal can be done in the background and stopped). Separate document types keep the file format independent of the in-memory model. |
| `settings_file` | The same for the global settings file, which is more forgiving: a missing setting gets its default and an unknown one is ignored. (Both files are written by one small `atomic_file` module that never leaves a file half-written.) |
| `media` | Everything that runs ffmpeg: finding it, probing a video, sound or picture file, the streams of preview pictures and preview sound, the decoding of a picture file, the thumbnails (at several levels of detail), the waveforms, and what is left of the export in it: the filter graph of the sound (the finished video's and the preview's alike) and the choice of the video encoder. The streams share one small skeleton (`StreamingTool`). |
| `rgba_picture`, `text_rendering` | A picture made by the program (as pixels), and the drawing of a text object into one, with `cosmic-text` (fonts, wrapping, fallback fonts for other alphabets and emoji). The fonts of the computer take a moment to load, which is done once on another thread. The text is drawn upright in its area: turning it is done when it is laid over. |
| `speed_indicator` | Draws the fast-forward symbol and decides where it goes, for the preview **and** the export, so they cannot disagree. |
| `frame_scene` | What a frame of the finished picture shows, and what the picture engine is asked to draw for it: `PreviewContent` (the layers that are seen, each with where it is, its colours and what its picture is made of) `layers_of_a_section` and `symbol_of_a_section` (a section of the picture plan as the layers and the symbol of a frame, as they are at that frame), and `describe_frame`, which turns it into the engine's `FrameDescription`. **The window and the export both use all of it**, which is what makes them the same picture. |
| `engine` | The picture engine: a thread of its own with its own graphics device (`wgpu`) that draws a frame from a description (layers of pictures with effects), keeps the pictures that it is sent on the card (made into **linear light**, premultiplied, in half floats, when they are sent), and for the export makes the planes of a `yuv420p` picture of the frame (from the finished frame in half floats, rounding once) and reads them back, frame after frame in order with the next recorded while the one before is read. It guards against a card that does not answer (see *The picture engine*). |
| `video_export` | The export of the finished video: `plan_export` (what has to happen: the runs of video, the files, the sound, the size, and the output colour space), `PicturesOfTheRuns` (one ffmpeg decode for each run, in order), `FrameDescriber` (each frame as the preview describes it), `output_colour` (the matrix and the tags the file gets from the output colour space), the `Encoder` (ffmpeg given raw frames on its standard input) and `VideoExport`, which runs it all on a thread and reports progress. |
| `colour_space` | Colours as numbers (phase 3): the table of **primaries** (Rec.709/sRGB, the two of BT.601, Rec.2020, Display P3, ACEScg and ACES2065-1: their chromaticities and whites), the **curves** between codes and light (sRGB, gammas 2.2, 2.4 and 2.6, the camera curve of Rec.709, linear), the matrices between sets of primaries (through X, Y and Z, with the Bradford adaptation where the whites differ), the **matrices** and **ranges** of video, and where the colour differences of a video sit. It is the reference: written in `f64`, held to published numbers (IEC 61966-2-1, BT.709, BT.2087, the ACES matrices) and by round trips, and the graphics card's version of it (`engine/shaders/colour.wgsl`) is held to it. |
| `sound_engine` | The sound engine (phase 7): the block graph that makes the sound of the preview and of the export from a `SoundGraphSpecification` and a feed for each clip (`SoundGraph`, `GainCurve`, `sample_at_frame`, `LookAheadFeeds`), with places for the effects of clips and of tracks (`Insertions`) and the insertions that ask a `PluginRunner` for the plugin effects of a clip (`PluginInsertions`). No ffmpeg, no thread and no `unsafe` in it. |
| `plugin_scan` | Finding the plugins that are installed without loading one in the editor (phase 7): the folders to look in, where the libraries that plugin libraries need are looked for, the scan program and the words it and the editor use for each other, how its end is understood (a crash, a library that never returns, a wrong answer are each a `ScanFailure` with the reason), the cache of scans that is not believed once a file changes (or, for a library that failed for want of another, once the places to look for libraries change), the plugins made into types of effect (`effects`: the key, the values from the ports, the channel layouts), what a person said of where to look (`settings`) and the breadcrumb of the libraries in use. It has no `unsafe` code: the loading is in `crates/ladspa-host`. |
| `plugin_host` | Running plugins in the sound (phase 7, piece 7.6): `PluginHost`, the one place that loads a library into the editor (only from a folder that was said yes to, after the breadcrumb says which library is about to be used, checking that it is the plugin that was scanned), and `PluginEffect`, a plugin as an effect of a clip (the channel layouts, the values at the frame the keyframes say, the plugin that is too slow, the sound that is not a number). |
| `plugin_trust` | Who may run a plugin (phase 7): the approvals of the folders that linked plugins are in, the severe question and the control that has to be held for five seconds (`HoldToConfirm`), the writing out of an embedded plugin with its hash checked, and `load_library`, the one way a library may be loaded, which refuses unless the person said yes. It loads nothing itself and has no `unsafe`. |
| `digest` | SHA-256, for the plugins (the CRC-32 of the resources only tells versions apart). |
| `playback` | The clock, which pictures the preview needs for a frame (one for each video that is seen), the `PreviewPictures` that keeps a `PictureSupplier` for each track (decoders running ahead of the clock), and the `AudioSupplier` that keeps the sound in step with it and feeds the speakers (through `cpal`, opened by `output_stream`, which the **sound effects** use too: the snip of a cut and the sounds of drawers are made by the program (`sound_effects`: `snip` and `drawer` are the sounds, and `synthesis` is what they are made from, noise and filters and the shaping of an end) and play through a stream of their own, so that they are heard while paused and never disturb the sound of the preview). |
| `application` | The egui window. Decisions (where tiles and waveform bars go, how far a clip may be moved, what a slider position means, how an edit moves the current frame, which keys do what (`keyboard`: a table of rows, key and modifiers and where the pointer is, with a test that checks the order of the rows, because egui matches a key that has more modifiers held than a row asks for), at which frame an edit of what has keyframes is made (`gesture_frame`: a gesture keeps to the frame it began at), where a layer's handles are and what a point on the preview is on, which settings the windows show) are plain functions or small tables with tests; the drawing code is thin. The timeline is a directory of small files (ruler, a track's header, its strip, the clips, the playhead, dragging clips about, resizing a track, the rows (where each track is drawn, and `track_reordering`, which moves them out of the way of a track whose title box is being dragged), the drawers and what is in them (`drawers`: their buttons, their height and how a crate is laid out; `racks` and `rack_look`: the racks and their diamonds, and the labels of them; `grip_dots`: the shaded bumps that say that something can be dragged), `item_dragging` and `rack_dragging` (where the pointer takes clips and keyframes, and racks: what is dragged together is chosen in `what_is_dragged` and `drag_of`), `clip_menu` (the menu of a clip), the flash of a cut, `rows` (`RowsLayout`, which says how tall the rows are going to be, so that the timeline panel can be made that tall before it is drawn: see `prepare`), scrolling and zooming, and the `StripView` that turns a place along the strips into a place on the screen and back, so that no drawing code writes that formula out for itself), the preview too (the layers, the widgets that take the mouse, what is under a point, the symbol, and how it is zoomed: `view.rs` is the arithmetic of the zoom and `zooming.rs` the wheel and the *Fit* button), and the properties panel (a file for each kind of thing it shows, `effects.rs` for the boxes of the effects and the **+** button that puts one on, which is the round `plus_button` the timeline has for tracks too), the history of edits and the selection are modules of their own, and the application itself is a set of files by what they are about (flows for source files, the project file, the files that are included in it and taken out again, export, dialogs, and one place that carries out every command), and `archive_cache` (where the files that are taken out of project archives are kept). |

Some choices that are not obvious from the code:

* **Whole frames everywhere.** Times are never floating-point seconds inside the model, so there is no drift and a
  cut is always on a frame.
* **Where things are on the picture is fractions, and angles are worked out in pixels.** The fractions of the
  width and of the height are not the same length, so nothing about an angle (where a turned corner is, what is
  under a point, how a turned rectangle is resized with the corner across staying where it is) can be done in
  them; `placement.rs` does it in the pixels of a picture of a given size, which the preview (drawn at the same
  shape) shares with the export.
* **Keyframes are a table of properties, kept in racks.** `PropertyId` lists what can be animated and what
  kind of value each has (a number, a pair, a colour, yes or no, words), a `Rack` holds the keyframes of one of them of one
  clip or track, wherever the rack is kept, and the project puts together what the racks of one thing say into an
  `Animation` (once, for the plans of the picture and the sound); what is shown in the panel, saved in the file, hashed to tell whether the sound
  changed and made by the stopwatch is driven from that table, so a property that is added is a row and not a new
  feature everywhere. A change of an animated property is made by **`FastForwardApplication`** (`animating.rs`): the
  commands that change a gain, a place or a text object still say the whole new value, and what is different from what
  the property is at the playhead is worked out there and made a keyframe there, so the preview, the panel and the
  numbers all change things the same way. The places that have to list the properties by hand (the rows of the panel, the
  keyframes tried in the file's tests, the properties that make up a place and a text object, the ones that are heard) are
  each checked against the table by a test, so that a property that is added to the table and left out of one of them
  fails a test and is not a control that quietly does nothing.
* **Ripple rules live in one place.** What moves when a clip changes length or goes is decided by `Track` alone
  (media clips that touch follow; a gap stops it; pictures and text stay), and is tested against a simple model by
  random edits. So are the rules for moving clips about (they never overlap, they move together, they stop at the
  start).
* **Undo is a list of copies of the project.** The project is small plain data, so every edit keeps the project as
  it was before (`application::history`); edits of one gesture (a slider, a drag, typing) have the same key and
  are joined into one step while the pointer is down or for about a second. Every `Command` says whether it can
  change the project (a `match` with no catch-all, so a command that is added must be put on one side), and the
  commands of one frame are one step.
* **ffmpeg as a child process, not libav bindings.** Building stays simple on every system and users can update
  ffmpeg themselves. All of it is behind `media`, so it could be replaced.
* **The preview is the export.** The picture plan and the sound plan are made once, in `composition`; the picture of a frame is described by one function
  (`frame_scene::describe_frame`) for the preview and for the export, and the export draws it with the picture engine (the preview does too when it is asked to);
  the sound graph the preview plays is the one the export has in it. The export reads its video through the preview's own frame streams, so the same frames
  are chosen. The two agree because they are made from the same description, and tests check the export's pixels where turned and scaled things must be. The sound is started from the playhead and kept in step with the clock by skipping a
  little sound or holding it back with silence, and started over when the playhead jumps or the edit changes
  (when it changes, the new sound is started beside the old and crossfaded into it, so that editing a volume
  does not make a gap). A clip is laid on silence of its exact length rather than padded, because ffmpeg's `apad`
  gives frames without timestamps when it pads nothing (a clip that lies wholly past the end of a shorter file).
* **Commands.** Drawing code never changes the project: it pushes a `Command` (select, delete, move, set a clip's
  speed, put a clip somewhere on the picture, make a text object...) and one function afterwards carries out
  each. That one place also keeps the playhead on the picture it was showing when an edit moves clips along the
  timeline.
* **A few widgets take every press on the preview, and none of them knows what the press is about.** egui
  decides who gets a press at the start of a frame, from the widgets of the frame before. A preview whose layers
  each felt the mouse only while they were what the pointer was over would give a click that came in the same frame
  as the move that brought the pointer there (a tap on a touch screen, a very quick click) to whatever the pointer
  was over a frame earlier, or to nothing. So the preview has one widget over all its room, and `grab_at` works out
  from where a press was what it takes hold of: the handles and the places to turn from of the selected layer, then
  the layers from the front (each as far as it really is, turned as it is, and only over the picture, but for the
  selected one), with the fast-forward symbol between the front-most video and what is in front of it. The box of
  the selected layer (its handles and the places to turn it from) can reach out of the picture, so each of those has a
  widget of its own too, made after the panel's, which makes it the one on top where it is: they only take the press
  to the same place. (They are made in the panel's own Ui, so they stop where it does: over the other panels, what is
  in them takes the press. What is drawn of the box is cut off at the edge of the panel the same way.)
  Only the left button picks and drags. The test window gives every event a frame of its own, as egui's test
  harness does, so the tests that need more than one event in a frame say so (`Window::all_in_one_frame`).
* **Native file dialogs are behind a trait** (`FileChooser`), and the editor in tests has one that panics when
  asked, so a test that reaches a dialog fails at once instead of hanging on a window nobody can click.
* **Settings are a table.** `project::SETTINGS` lists every setting once (a checkbox or a drop-down list); both
  settings windows are built from it, and tests go through it to check that every setting survives both files.
  Adding a setting is a field, a row, and a line in each file's document type (see the notes at the top of
  `project/settings.rs`).
* **Effects are descriptors.** An `EffectKind` is a small value that can be copied, compared and put in order, and that stands for an
  `EffectDescriptor` (`project/effects/descriptor.rs`): the key (`builtin:hue_shift`), the version, the name, what it does in a few
  words, what it works on (pictures or sounds), whether it changes each pixel by itself (which says whether a clip group or a track
  group can have it), and its values: each has an id (what a file calls it for ever), a name (what its keyframes are called), a label
  (what its control says), what it begins as, and the range, unit and decimals of its control. It is made by being *interned*: a
  descriptor that is made into a kind is kept for the rest of the run, and the same descriptor made into a kind twice is the same
  kind, which is how a kind can be part of the key of a keyframe (`PropertyId::EffectValue`) without a list of the effects there are
  being at hand. The program has some effects in itself (`EffectKind::BUILT_IN`; each a descriptor, and in `project/effects/built_in.rs`
  what it does: how it changes the colours of a picture from its values); a
  project file brings the others, as snapshots of their descriptors. The **+** button's menu, the box of an effect and the stopwatches
  of its values, the file (`plugins` and `effects`), the picture plan, the preview (the colours of the pictures put on the graphics
  card) and the export (the same colours, at every frame) are all driven from the descriptors, and a test goes
  through each: a new effect of the program is a descriptor, a constant, and a row of what it does, not a change in the code that is
  the same for all of them. (Controls for values that are not numbers are said in words for now; they come with the effect surface of
  the plan. An effect that does something other than change colours, such as one that needs the sound or other pictures, adds a way
  of changing what it changes to what the program does for it, which is the one thing that does not exist yet.) **A descriptor of the
  program is never changed without its version being changed** (a test has the hue shift as it was in version 1.0), because a project
  file keeps a snapshot of it. An effect that the program does not have (a file brought it) is kept with its values and keyframes,
  does nothing, and is saved again as it was.
* **Keyframes are not lost by accident.** What changes how many frames a drawer has (a length, a speed, the frame rate)
  puts the keyframes on the frames nearest to where they were, with the empty space between them used up first
  (`project/tracks/keyframe_fitting.rs`: the nearest way to give each keyframe a frame of its own, the same as the least
  total move, which was checked against every small case worked out by brute force), and when that is not enough the
  program finds out how many would be lost by making the change on a copy of the project and asks (`losing_keyframes.rs`).
* **The preview and the export select frames the same way**: the export reads its video through the preview's own frame streams (same frame-rate
  normalisation, same `select='not(mod(n,N))'`), which is what lets the preview be trusted as a picture of the export.
* **Large projects.** The sound graph (the export's and the preview's) is given to ffmpeg in a file, so a project with hundreds of clips
  does not hit command-line length limits. The picture of the export is not a graph at all: it is made frame by frame. The picture plan is made in a time
  that grows in step with the number of clips.
* **The state of the editor** (`FastForwardApplication`) is in six groups (`fast_forward_application/state.rs`), by what each is about: `document` (the project, where it is
  kept, the history, what is unsaved), `playback` (the clock and what makes the preview's pictures and sound), `sources` (what is known of the files the project is made from),
  `jobs` (the export, a save that runs in the background), `interface` (the selection, the views of the panels, the dialog that is up, closing) and `environment` (what the
  computer gives it: ffmpeg, the file dialogs, the settings file, fonts). What belongs to one group is a method of it (`sources.is_available`, `playback.reading`); the
  work that needs several is a method of the application, in the file of what it is about.
* **`FastForwardApplication`'s "programmatic interface"** (`programmatic_interface.rs`) does what the File menu
  does without its file dialogs and lets you look at the state, and chooses where sound goes (the program makes
  no sound until it is told, so tests never make any noise). The window uses the same code; it is also how the
  end-to-end tests drive the real program.

## Testing

```text
cargo test --workspace         # everything below
cargo clippy --workspace --all-targets -- -D warnings
```

| Suite | What it covers |
|---|---|
| unit tests (in `src/`) | the model (including the rules for what moves when a clip changes and for moving clips, checked against a simple model with random edits), placements (corner dragging checked with random drags), the plans in `composition` (checked frame by frame against a plain evaluation, with random projects that have videos of other shapes, places and turns), timeline geometry, zoom, thumbnail levels and waveform bars, both files and the older versions of the project file, the filter graph text of the sound, symbol drawing, text drawing (with the computer's fonts), the clock, the picture and sound suppliers (with fake decoders and speakers), the history... and **window interaction tests** that send simulated mouse and keyboard input to the real window: selecting (with Ctrl and a rectangle), moving, deleting and cutting clips; undo and redo; zooming, scrolling and resizing tracks; tracks, their titles, the round button and deleting one; the panels and the properties panel (what it shows for what is selected, what its controls do, the list of several things), selecting tracks, dragging clips to other tracks; copying clips and tracks from their menus; the drawers (their buttons, how they open, pushing the tracks below down or, with Shift, lying over them, and how the timeline scrolls to show them), the racks and diamonds in them and their labels, selecting and dragging keyframes alone and with clips, dragging racks between drawers and into crates, with the words *not enough space here*; text objects from the menu; effects (the **+** button and its menu, the box and the slider of the hue shift and its stopwatch, taking it off and undoing that, what the plan, the preview's colours and the export's filter and stretches say at every frame of random projects that have effects, and the files of them), **groups** (the rules about them, checked after tens of thousands of random edits of every kind: nothing but what is in a clip group is ever inside its rectangle, whatever is made, moved, copied, cut, sped up, retimed or deleted, and an edit that is refused changes nothing; the outlines, whose shape is checked on random sets of rectangles for being simple, going round clockwise and having every clip inside; the menus that make groups, the box of a group with its list and its bar, the chain of effects with its badges dragged and double-clicked, drags that stop against rectangles, the messages when an edit is refused, the deletes and the **Delete** key, undo; the file of them, and the older versions), the keyframes that are lost only after a question (the fitting of keyframes is checked against every small case, worked out by trying all the ways there are), racks that are selected, reordered and merged, keyframes that are copied and dragged to other racks, the zoom of the preview (the wheel, the middle button, the *Fit* button, pixels shown as blocks) and the edges of the picture that something is held to; picking, moving (past the edge of the picture too), resizing and turning videos, pictures and text on the preview, by corners that are outside the picture and over the preview (also with a press that comes in the same frame as the move of the pointer, as a tap does), and that corners over the other panels are not taken hold of and are not drawn there; the keys; dialogs; the menus and settings windows; autosave |
| `tests/media_*.rs` | the real ffmpeg: probing videos, sound files and pictures, decoding, thumbnails (checked to be the picture on screen at the moment they stand for), waveforms, exports checked **picture by picture** against an independent calculation of what the project must contain (and tone by tone for the sound of several tracks, with gain and speed), **pixel by pixel for what is scaled, moved and turned** (a video at half size, a video turned a quarter, text turned so that its red left half goes to the top, something cut off at the picture's edge, a video, a picture and a text put partly past the edge, a picture from a file that is turned, comes later in the timeline, or lies over a video that is cut in pieces), and the sound stream (right length and format, from any playhead position, sample for sample the same as the sound from the start, and the same loudness second by second as the export) |
| `tests/application_journeys.rs` | whole journeys through the real program with a real video: import, edit with the mouse and keyboard, undo, save, load into a fresh editor, lose a file and choose it again, import a sound file or a picture, make a text object, animate a video with the stopwatch and the numbers, export (and check the picture and the sound of the result) |
| `tests/video_export_*.rs`, `tests/command_line_export.rs` | the export made by the picture engine, with the real ffmpeg and the engine on a software graphics adapter: the pictures of each run of video are the frames the project says (as RGBA and as planes, at the size of the picture and smaller, from videos of other frame rates, hidden for a while, shorter than the project was told); the encoder (frames in order and none lost, the colours the file says, the sound as long as the picture, stopping); and the whole export (every frame, cancelling and dropping leave nothing, **cancelling works when ffmpeg is stuck and not reading the frames** (on Linux and Mac, with a named pipe), a file that cannot be read or written fails with its name or ffmpeg's words, a finished video that cannot be moved into place and a mistake of the program in the middle of the export are failures that say so, a project of one frame, two different files for two sources, the destination cannot be a source). The command line is the real program: a plain project, a project file with its video in it when the video is gone, a missing video, an export that fails after it has started, a command line that is not understood. |
| `tests/media_export_animation.rs` | exports with the real ffmpeg of clips that have keyframes, checked picture by picture: a video that moves (where its edges are at every frame), one that is turned (each picture is the same as the one made with no keyframes by turning the video that far), one that gets smaller, and text that moves and changes colour over a video that goes on unbroken. The sound of keyframes for how loud and whether silent is checked in `tests/media_audio_stream.rs`: a ramp, a ramp started in the middle, a silent stretch with the sound after it from the right place, and the preview's sound against the export's |
| `tests/media_export_effects.rs` | exports with the real ffmpeg of clips and tracks that have a hue shift, checked colour by colour: a flat colour comes out as the colour the arithmetic of the effect says, whether the effect is the clip's or the track's, or both, for a video, a picture file and a text object, for the piece of a video that has the effect and not the piece next to it, with keyframes a little more turned at each frame, and for groups of each kind (a tag turns the clips in it and no other; a clip group and a track group turn the clips in them, which is the placeholder described in *Groups*; and the effects of a clip, its track, a tag, a clip group and a track group are all done) |
| `tests/application_playback.rs` | playing: the preview keeps up with the clock, through a change of speed and a jump, and stops on the last picture. Timing-dependent, so it has its own file, which cargo runs on its own. |
| `tests/application_sound.rs` | the preview's sound in the real program, with recording stand-ins for the speakers: a test video whose pitch changes every second shows that the right sound is heard at the right time, through a change of speed and a jump; that it starts quickly, stops on pause, and is muted or raised in pitch when asked; that changing a volume while playing leaves no gap |
| `crates/ladspa-host/` (unit tests and `tests/test_plugins.rs`) | the host of plugins: the facts of the interface and of the hints; a descriptor built by hand with its lists missing, wrong or too long; the real test library loaded, described, made and run (every plugin, the controls, the channel layouts, the filter that remembers, a reset, an instance that outlives the library value, an instance moved to another thread); what the host does to a plugin (it counts: activation at the first block, deactivation, clean-up, ports connected before activation, nothing left connected to a buffer that was lent); and files that cannot be loaded (nothing, noise, a library that needs one that is not there, a system library that is not a LADSPA one). |
| `tests/plugin_scan.rs` | the scan with the real scan program and the test library asked to misbehave: a good library, a crash while described, made or run, a library that never returns (stopped at the time limit), one that ends the program or answers wrongly, noise on the standard output, no plugins, a descriptor that is not usable, a million ports, plugins without end, a plugin that gives nothing; that a failure is kept and not tried again, by the same scanner and by another run of the editor, until the file changes; that a bad library costs the others nothing; and that a scan program that cannot be run is not the library's fault. |
| `tests/sound_plugins_journey.rs` | sound plugins in the real editor with the real scan program: the folder is added, the scan finds the plugins in the background, one is put on a clip from the menu, the project is saved, loaded where the plugin is not installed (the effect is kept whole and says so; saved again it is what it was) and where it is (it works again); **an export from the editor has the plugin's sound** (measured on the file) when the folder is approved, and says which plugins were not run when it is not. |
| `src/plugin_host/tests.rs` | plugins as effects of a clip, with the real test plugins: each channel layout (one and one made twice, two and two, one and two, two and one), a plugin that remembers gives the same sound in blocks of any size and each channel has a memory of its own, a reset forgets, a value is kept to its range, a flag and a whole number, a value with keyframes is what the keyframes say at the frame each 32 samples are in, sound that is not a number is silence; the whole graph with a gain plugin gives the arithmetic exactly for blocks of any size (and is done before the volume), **a seek gives the same samples as playing through** with a value that rises, the effects are asked for in the order of the chain; a plugin that is too slow is left out of the preview after the patience it is given and never from the export; the host: a folder nobody has answered for is wanted once (with its libraries and plugins) and the sound is unchanged, an approved one is run (the library is loaded once), a declined one is not asked again until another project is opened, a library that cannot be loaded or is not the plugin that was scanned is said once, the breadcrumb is written before the library is loaded. |
| `tests/sound_plugins_in_the_sound.rs` | the sound of plugins with the real ffmpeg and the test plugins: the export and the preview make the same samples and they are the arithmetic of the plugin; a value with keyframes is what they say; a folder nobody said yes to leaves the sound as it was and is asked about, and when it is approved the plugin is in the sound; a plugin that cannot be loaded is said once, in the export and in the preview; **a plugin that is too slow is left out of the preview and said, and the export waits for it**; the starter of the editor makes the sound with its plugins. |
| `src/application/fast_forward_application/sound_plugin_folder_tests.rs` | the folders that plugins are in, in the editor: one question for a folder naming its libraries when a project is opened, two folders two questions, a plugin not on a clip or not installed is not asked about, a yes approves the folder for the host and writes it down, a no holds until another project is opened, the sound coming to a plugin before anybody answered asks once, a scan that finds a plugin a project needed asks, an export waits for the questions and goes on, the plugins an export would not run are listed with the reason, a note is told once while the sound plays, where libraries are looked for reaches the host, the breadcrumb is taken away when the window is closed, and a breadcrumb that a run left fails the library and says so. |
| `tests/sound_plugins_lmms_set.rs` | **ignored; a report, not a check:** every plugin of a folder of LADSPA libraries on a clip, each in a process of its own, made by the export and by the preview (see *Running plugins in the sound*). |
| `tests/application_screenshots.rs` | not checks: with `FASTFORWARDRS_SCREENSHOT_FOLDER` set, `cargo test --test application_screenshots -- --ignored` draws the window in several states to PNG files (tracks, a waveform, text, pictures, turned and resized videos, things past the edge of the picture, zoom, the properties panel, the menus, a missing file, a cut flashing, a track being dragged, drawers, racks of keyframes, racks being dragged and crates, the menus that copy clips and tracks, the buttons of the keyframes, selected racks and the menus of racks and keyframes, the question before keyframes are lost, the preview zoomed in and out and to its pixels, the **+** button and the hue shift...) |

The tests that use ffmpeg make their own small test videos (each frame carries its own frame number as a strip of
black and white blocks, so a test can read which frame it received), tone files and pictures in
`target/test_fixtures` or a scratch folder. They are skipped with a message when ffmpeg is missing; set
`FASTFORWARDRS_REQUIRE_FFMPEG=1` to make that a failure instead. One test opens the computer's real sound output
and plays silence through it; it is skipped on a computer with none. The text tests are skipped on a computer
with no fonts at all. Nothing has been tried with a real touch screen: the taps are simulated.

The tests of the picture engine, and all the tests of the export, make their pictures on a **software graphics adapter** and on nothing else (WARP on Windows, lavapipe on
Linux), so that nothing a test does is done on a real graphics card (a shader that does not finish must never freeze a developer's screen). Where there is none (a Mac)
they say so and do nothing; `FASTFORWARDRS_REQUIRE_SOFTWARE_ADAPTER=1` makes that a failure instead (the CI sets it on Windows and Ubuntu). `FASTFORWARDRS_TESTS_MAY_USE_ANY_ADAPTER=1`
lets them use the adapter that a computer has where it has no software one: the CI sets it in one step, on the Mac of the runner (a virtual machine that is set aside for the
purpose), where a failure is a warning; **it is not for a developer's computer**.

The tests themselves are checked with **planted mistakes**: one deliberate mistake at a time (a `<` that becomes `<=`, a condition that is left
out...) is put in a copy of the code, and the tests must notice it; a mistake that none of them notices is a gap, and is closed with a test (or
is one that changes nothing a user could see). `tools/mutation_testing/` is what plants them and runs the tests (see its `README.md`): it stops
at the first test that fails, tries several mistakes at once, counts a test as noticing a mistake only if it fails again when it is run by
itself (a test of the real speakers fails by itself when the computer is busy), and checks itself with two controls before it begins and with a
final run of the tests that noticed; `py -m unittest discover -s tools/mutation_testing` tests the runner itself.
`tools/ci/run_tests_and_report.sh` is the helper that every test step of the CI goes through: it writes what failed, and what ran (the result line of each program of
tests, and how many tests said that they were skipped, where the run shows that), as annotations of the job, which can be read without signing in to GitHub;
`py -m unittest discover -s tools/ci` tests it (they need `bash`).

## Known limits

* **Colours** (see *Colours*): **high dynamic range** (PQ, HLG) is not converted (read as sRGB, with a note in the window of the file), and nor are camera logs; the program does not read the **computer's colour profile** (the display is sRGB, 2.2 or 2.4 by the setting) and does not dither (a smooth dark gradient can show steps); ffmpeg **scales a video before the program sees it**, in the numbers of the file and not in light (a video made much smaller has its edges averaged in codes); the matrix and range chosen for a video that is stored as 4:2:2, 4:4:4 or in 10 bits are given to ffmpeg, which makes its colours itself and blends their colour differences its own way (**native planes for those forms are not made**); and **effects of other people's** (shaders loaded from files, which the program does not yet have a way to ask for) would get the numbers of the working space and not the numbers of the screen (when they exist, phase 4 gives them the domain they declare).
* **The effects of a clip group or a track group are a placeholder for now**: they are done to the picture of each member by itself, which makes the same picture as doing them to the whole only where the members are not see-through (see *Groups*); at the see-through edges of layers that are over others it can be quite different. A tag is not affected. Groups have no effect on sound, are not clicked or dragged on the timeline, and have no name written on it.
* Cut clips cannot be joined again. There is no way to trim a clip by dragging its edge (cut it and delete the
  part instead). A clip dragged to another track is never put on a new track: the track has to be there.
* Undo covers what is edited in the project, not the settings, and not choosing a missing file again. It goes
  back at most 200 steps, and only as far as the program has been open (the history is not saved with the
  project).
* The finished picture is the size of the first video.
* The export needs a graphics adapter: the one the window is drawn with, or with `fastforwardrs-export` the best the computer has or (slow) a software one. A computer that has none of either cannot export.
* `fastforwardrs-export` cannot be stopped with Ctrl+C cleanly: ffmpeg is stopped with it, and the half-written `name.exporting.ext` is left (the window's *Cancel* removes it).
* The engine keeps 512 MiB of pictures on the card. A frame that is made of more than that (many large pictures at once) ends the export with a message.
* Something on the picture can be put partly past its edge, but its middle stays on the picture, and it cannot be
  made bigger than the picture (so a video cannot be zoomed in past it). What is past the edge is not seen, and is
  cut off in the export. Only what is selected can be taken hold of where it is not over the picture: by its corners
  and the places to turn it from, wherever they are over the preview panel (not over the other panels: what is in them
  is what is taken hold of there), and by anywhere in its outline over the preview. A video or picture thinner
  than a fiftieth of the picture is drawn a fiftieth wide.
* The properties panel changes one thing at a time: with several selected it only lists them (and a group of
  several things cannot be made yet).
* A picture file with clear places is drawn with them clear, but a picture never hides what is behind it, so what
  is behind a picture that covers the whole picture is still decoded and exported. A photograph that is turned
  on its side by a note in the file (EXIF) is shown the way ffmpeg shows it, which is not tested.
* A GIF file is read as a video, as ffprobe says it is one: an animated one plays at its own frame rate (and is
  made to run at the project's, like any video) and a still one lasts a single frame. It is not offered with the
  picture files.
* A file that is chosen again in place of a missing one must be the same kind of file; if its shape is another,
  the picture is stretched into the place of the old one.
* A text object has one font, size and colour for all of its words, and is drawn from the top left of its area,
  wrapped to its width and cut off at its bottom edge.
* Speeding sound up with *Maintain pitch* does not keep very short clips (under a tenth of a second of sped-up
  sound), and *Increase pitch* is silent beyond 1000x.
* Autosave needs the project to have a file already. For a `.ffrs` file it adds the new text of the project to the file and
  copies nothing else (see *Project files*), so it is as quick with a great deal of video included as with none. An autosave
  never writes the file again without its garbage, and does not save while a save is going on in the background.
* A variable-frame-rate source is exported at a constant frame rate (each picture slot gets the source picture
  nearest in time), and its preview can be a frame off at the start of a clip. The same goes for a file whose
  pictures are not exactly where its frame rate says they are, in ticks too coarse to say (a thousandth of a second
  or more): where two pictures are equally near a frame, the preview can choose the other one than the export.
* The speed sliders offer 1x to 1000x; a project file may hold a larger factor and the editor will keep it.
* Waveforms are made for sound *files*; a video's own sound is not drawn.
* **Files included in a `.ffrs` project file**: each is read where it is in the archive (a stretch of the file, by ffmpeg's
  `subfile` input: checked with real ffmpeg for the probe, the pictures, the sound, the thumbnails, the waveform, a picture file
  and an export, with names that have commas, quotes, accents and Japanese letters in them), so nothing is copied to open one and
  nothing takes room twice. **Including a file copies it into the archive**, which takes as long as copying the file (and doubles
  the room it takes until the original is deleted): a copy of more than 8 MiB is done in the background with a window that
  says how far it has got, and nothing else can be done until it is done or given up. Taking a file out of the project file
  copies it back (a copy of a file of several gigabytes is done without a window, and the window waits for it: this is the one place
  where the window stops for a copy). Thumbnails, waveforms and the like are not kept in the archive, and are made again when a
  project is loaded. A file that another zip program compressed is taken out into the cache the first time it is needed; the program
  never takes anything out of the cache. **The program that reads the archive in place must find the file at the same place
  all the time**: the file is written again (without its garbage) only when nothing is reading it; and on Windows a program that
  has the file open (another one) makes that fail, which a message says. Files of 4 GiB and more are written the way the zip
  format has for them (zip64): tried with a file of 4 GiB and a little over (it is written and read back whole, and Python's
  `zipfile` reads it and checks its CRC). The test that does it is ignored by default because it writes about 13 GB:
  `cargo test --lib a_file_of_more_than_four_gigabytes -- --ignored`. A file that is taken out of the project file is put back
  only where nothing is (or where the same file is already), and never over another file: a project that has been moved to
  another computer may have nowhere to put it, which is said. Including and taking out are not steps of the history (like
  choosing a file again). A project file that has files included cannot be saved as plain JSON, and a message says so.
* **Keyframes**: keyframes can be copied beside themselves (*Duplicate to the right* and *to the left*) but not copied
  and pasted elsewhere, and racks cannot be copied at all (a clip or a track is copied with the racks in its
  drawers, and the racks that are of it but kept in the drawer of another clip are not copied with it); racks are not
  selected by a rectangle, only by clicking them; racks that are selected together (with each other, or with clips or
  keyframes) are dragged **along the timeline only**: a rack that is dragged with other things is not taken to another
  drawer or another place in its drawer, and keyframes that are dragged with a rack stay in their racks (take hold of a rack
  that is alone to put it elsewhere);
  merging racks keeps every keyframe at its frame, so a merge for which no drawer has room for all of them is not offered
  (and a keyframe of a rack that is not merged, at the frame of one that is, can come to count where it did not);
  the value of a keyframe is changed with the controls of what it is of, with the playhead put on it (the diamond only moves it in time).
  **Losing keyframes**: the question is asked about a change that is made by a command (a speed, a length, the frame rate,
  moving clips to a track of another speed); while a clip is being *dragged* to a track of another speed it is asked
  as soon as the drag would lose some, which can be in the middle of the drag.
  A crate lasts as long as its keyframes need, and nobody sets how long (a file of an older version, in which a crate lasts
  longer than its keyframes, is read as a crate that is as long as they need, and one with no keyframes in it is not read
  as one: it is not in the project, nor in the groups it was in). Speed and the time something is shown for cannot be
  animated, so a clip's length never depends on keyframes.
  What is turned is turned as many whole turns as the angles of its keyframes say, and a whole number of turns looks
  the same as none (it is exported as none: see *Exporting keyframes*). A text object's keyframes for its words and its font change them all at
  once at the keyframe (words are not made of anything that can change evenly).
* **Exporting keyframes**: where something is, how big it is and how far it is turned (as far as its keyframes say: going round once or twice more is seen in
  the pictures between, and a whole number of turns looks the same as none) are worked out for each frame of the video, exactly: not in whole pixels, and with no
  limit to how many frames differ (see *Exported video*). How loud something is when it has keyframes is made in small pieces of sound, not sample by sample.
* **Undo of keyframes**: a change that makes a keyframe is one step with the changes of the same property at the same
  frame that follow it; at another frame it is a step of its own, however soon after.
* **Drawers** have no setting to turn them off; whether they are open is not saved. A drawer pushes the tracks behind the
  track it is on down while it is open, unless it was opened with Shift held, when it lies over them and hides what is under
  it (which one it is is kept until it has shut). The timeline scrolls down to show a drawer that is opening, but never up,
  and does not follow a drawer as it grows when keyframes are put in it. A copy of a clip or a track is put right beside
  what it is a copy of (to put it elsewhere, drag it), and its title is the original's with *copy* after it. The snip of a cut
  is made by the program (no sound file), plays through the computer's default output device (opened the first time it
  is needed, so nothing is opened by a program that never cuts) and has no setting to turn it off either; that it is
  pleasant to hear has not been judged by ear. The flash is drawn for about a third of a second (0.3 s) and is not saved.
  **A drawer makes a quiet, wooden sound as it slides open and another as it slides shut** (also when it opens because a
  keyframe was put in it): a rumble of wood on wood that fades as the drawer arrives and ends in a soft knock, and one
  that swells as it shuts and ends in a lower, heavier thunk. They are made by the program like the snip, play through the
  same speakers, are about as long as the sliding (a little over a quarter of a second, a third for the thunk), less than
  half as loud as the snip, and have nothing high in them to squeak and no pitch in the rumble to creak. What was measured
  of them (their lengths, loudness, how little is high or very low in them, that the rumble is noise and not a note,
  that the thunk is lower than the knock) is in the tests; that they sound like wood and are pleasant has **not** been
  judged by ear.
  **While a drawer slides, nothing wobbles**: the timeline panel is made as tall as the rows will be in the frame, and not
  as they were in the frame before (a panel is, in egui, as tall as its content was), so the track under a drawer that is
  opening stays where it is on the screen (it used to jump down in the first frame by about a quarter of how far the
  drawer opens, and creep back over the ten frames that followed), and the row of the buttons and the slider, which is the
  top of the panel, goes up with it in step; the same goes for a drawer that shuts and for a track that is added or taken
  away.
* The controls that have the buttons for keyframes are a little narrower than the others, to leave them room.
* **Effects**: a clip, a track or a group may have several effects of one kind (and one on the clip and one on its track are
  both done), and they are done in the order of the chain, which is changed by dragging. The effects it has are the eight of the table in *Effects*. They are the same in the preview and in the
  export (the preview is a smaller picture, so a blur or a table is a little different at the pixels, not in the picture). Opacity, blur and crop are not offered to a clip group or a track group (see *Groups*).
  Values of a table, of a choice and of a switch are not animated. **A blur on the card of the development machine has not been timed** (it was measured on software
  adapters only: a blur is two passes whose cost grows with the radius). The pictures on the
  **timeline** (the thumbnails of videos and the pictures of picture files) are the pictures as they are, with no effect on
  them; the preview has the effects, including the thumbnail that stands in for the picture of a video for a moment while
  the playhead is moved about. Greys have no hue and are not changed by a hue shift, and a colour that a turn takes past
  what a screen can show is cut off at the least or the most there is (the formula is the one of CSS, which only keeps how
  bright a colour is within that).
* **The zoom of the preview** shows the pictures the preview has, which are made at no more than 1280 by 720 pixels (a video's, and a
  picture file's): in a project whose picture is bigger the pixels that are seen when it is zoomed in are those of the preview's
  pictures, not of the exported ones. (Text is drawn at the size of the finished picture.) The zoom is not saved with the project.
