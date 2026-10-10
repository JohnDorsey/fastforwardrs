# fastforwardrs

A small video editor with one job: **fast-forward the parts of a video you choose.**

Open a video, cut its track into clips, drag a slider on any clip to make it play 2x, 4x, 10x... faster, and
export the result. Fast-forwarded clips show a fast-forward symbol with their speed (like ▶▶ 4x), in the live
preview and in the exported video. The preview plays the sound too.

## Status

| | |
|---|---|
| Tested on | **Windows 10** (the first version is only tested there) |
| Written for | Windows, macOS and Linux. The only platform-specific code is the `.exe` suffix when looking for ffmpeg and hiding the console windows of the ffmpeg child processes (both `cfg(windows)`); a couple of unit tests have a Unix and a Windows variant, and only the Windows ones have been run. It has **not** been built or run on macOS or Linux yet. On Linux the window system libraries that `eframe`/`wgpu` need may have to be installed, and so may ALSA's development files (`libasound2-dev` on Debian and Ubuntu), which the sound output (`cpal`) is built with. |
| Sound | The preview's sound goes to the computer's default output device. On the development machine the device opens and takes sound at the speed of real time (checked by a test that plays silence), and the right sound reaches recording stand-ins for the speakers at the right time (see *Testing*). That the sound is **audible** has not been judged by ear. |
| Rust | edition 2024 (Rust 1.85 or newer); developed with 1.99 |
| ffmpeg | developed and tested against ffmpeg/ffprobe **9.0.2**. The code also handles ffmpeg older than 7 (it reads the filter graph from a file with the older option name), but that path is only covered by unit tests of the command line, not by a real old ffmpeg. |

## Running it

fastforwardrs runs the `ffmpeg` and `ffprobe` programs for everything to do with video, so they must be
installed. They are looked for in this order:

1. the folder named by the environment variable `FASTFORWARDRS_FFMPEG_DIRECTORY`,
2. the folder fastforwardrs itself is in,
3. the `PATH`.

If they are missing the window still opens and says so.

```text
cargo run --release                                  # an empty window
cargo run --release -- "C:\path\to\project.json"     # open a project straight away
```

## Using it

**The menus** are File and Edit.

* **File**, in this order: *New project*, *Save project*, *Load project*, *Import*, *Export*.
  * *New project* asks for a video file straight away.
  * *Import* chooses the video for a project that has none, or replaces the one a loaded project can't find.
    (A project holds one video in this version.)
  * *Export* asks where to save the finished video (`.mp4`, `.mkv` or `.mov`) and shows its progress, with a
    Cancel button.
  * Loading, starting a new project and closing the window ask first if there are changes that have not been
    saved, unless Autosave is on (see *Settings*).
* **Edit**: *Project settings* and *Global settings*, which open little windows (see *Settings*).

**The window** is the preview in the middle, and along the bottom the Stop / Play / Pause buttons with the
current time next to them (`hh:mm:ss.ff`, where `ff` is the frame within that second), then the video track
with a ruler above it, then a slider that spans the whole window.

| To... | Do this |
|---|---|
| cut the video track | press **S** (cuts at the playhead) |
| move the playhead | **click or drag in the ruler** above the video track (the small strip with a mark for every second, where the red triangle sits); or press **Left** / **Right** to move it one frame back or forward, paused or playing (holding the key repeats); or drag the slider along the bottom |
| speed up a clip | press the **right mouse button** on the clip and **drag up**: a vertical slider appears (1x to 1000x, whole numbers; the arrow keys or mouse wheel nudge by one). Let go to set it. Drag back to the bottom for normal speed. |
| scroll the track | the mouse wheel over the ruler or the track scrolls it without moving the playhead |
| move or resize the fast-forward symbol | move the pointer over it in the preview (it is shown while the current frame is in a sped-up clip), then drag it, or drag its corner. There is one symbol position and size for the whole project. |

The track is a thick strip of pictures from the video, one for every second of the **finished** video, side by
side with no gaps; each clip has its corners rounded a little. A sped-up clip is narrower on the strip, and the
time shown is the time in the exported video. The red line is the playhead.

**Sound.** While the video plays, its sound plays too, and it is the sound the export will have: every clip's
sound is cut out and sped up with the clip (speeding sound up this way does not make it higher), and sped-up
clips can be muted (see *Settings*). The sound starts a fraction of a second after pressing Play (about 0.07
seconds was measured on the development machine), jumps with the playhead when that is moved while playing,
and stops the moment the video is paused.

If a loaded project's video can't be found or opened, a warning appears in the middle of the window saying it
must be chosen again. The rest of the program stays usable and none of the edits are discarded; choosing the
video (the warning's button, or *Import*) puts it back. A replacement must have the same frame rate as the
original, because clips are stored as frame numbers.

## Settings

| Setting | What it does | Default |
|---|---|---|
| Autosave | Saves the project to its file by itself two seconds after the last change. Only for a project that already has a file: the first save always asks where. With it on, closing the window or loading another project do not ask about unsaved changes: they save. | off |
| Mute sped-up clips | The sound of sped-up clips is left out, in the preview and in the exported video (silence of the same length). | off |

The **global settings** (*Edit > Global settings*) are the same for every project. They are kept in a file in
the operating system's folder for programs' settings (`%APPDATA%\fastforwardrs\settings.json` on Windows), which
is written as soon as one is changed.

The **project settings** (*Edit > Project settings*) have a checkbox, *Use global setting*, next to each
setting. While it is ticked the project follows the global setting: the setting beside it is greyed out (showing
the global value that is in use) and nothing about it is saved in the project file. Unticking it gives the
project a setting of its own. A new project follows all the global settings.

## Project files

Pretty JSON, meant to be read (and, if needed, edited). The video itself is not inside the file: it is linked by
**both** an absolute path and a path relative to the project file, and the relative one is the backup when a
project and its video are moved together. The file format is versioned, and the file is written to a temporary
file first and then renamed into place, so a failed save cannot damage an existing project.

```json
{
  "file_format": "fastforwardrs-project",
  "file_format_version": 2,
  "source_videos": [
    {
      "identifier": 1,
      "absolute_path": "C:\\videos\\holiday.mp4",
      "relative_path": "holiday.mp4",
      "frame_rate": "30/1",
      "total_frame_count": 1200,
      "display_width_in_pixels": 1280,
      "display_height_in_pixels": 720,
      "has_audio": true
    }
  ],
  "video_track": {
    "clips": [
      { "source_video_identifier": 1, "first_source_frame": 0, "end_source_frame": 300, "speed_factor": 1 },
      { "source_video_identifier": 1, "first_source_frame": 300, "end_source_frame": 600, "speed_factor": 4 }
    ]
  },
  "speed_indicator": {
    "left_fraction_of_frame_width": 0.03,
    "top_fraction_of_frame_height": 0.04,
    "height_fraction_of_frame_height": 0.08
  },
  "settings": {
    "mute_sped_up_clips": true
  }
}
```

Several `source_videos` are allowed by the format; the editor uses one. `settings` holds only the settings the
project has chosen for itself, and is left out altogether when the project follows them all. Version 1 files
(from before there were settings) are read as following all of them.

## Exported video

* Every picture is **exact**: a clip at speed N keeps source frames `first`, `first+N`, `first+2N`, ... and the
  export contains precisely the frames the project describes, at the source's constant frame rate. This is
  checked picture by picture against the project, on MP4 and MKV, at 29.97 fps, and from a variable-frame-rate
  source (see *Testing*).
* The sound is sped up with the picture and stays as long as it. Where a sped-up clip is so short that its sound
  would be under a tenth of a second, that stretch is silent instead. With *Mute sped-up clips* on, every sped-up
  clip is silent.
* H.264 (`libx264`, falling back to `mpeg4` if your ffmpeg has no libx264) and AAC. `.mp4` and `.mov` files are
  written for streaming (`+faststart`).
* If a project was pointed at a video **shorter** than the one it was edited on, all the edits are kept, but a
  clip reaching past the end of the new video is cut short (or left out if none of it exists), the sound ends
  with the picture, and the message at the end of the export says how many frames of the project are not in it.
* The video is written to `name.exporting.ext` next to the destination and renamed when it is complete, so
  cancelling or failing never leaves a half-written file or damages an existing one. Exporting over the source
  video is refused.

## How it is built

The code is a library plus a thin `main`. It is layered, and each layer only depends on the ones below it:

| Module | What is in it |
|---|---|
| `project` | What a project *is*: source video, clips, speeds, settings, and all the timeline arithmetic, in whole frames (`SourceFrameIndex`, `TimelineFrameIndex`, `SpeedFactor`...). Pure logic: no files, processes or drawing. |
| `project_file` | Saving and loading the project JSON. Separate document types keep the file format independent of the in-memory model. |
| `settings_file` | The same for the global settings file, which is more forgiving: a missing setting gets its default and an unknown one is ignored. (Both files are written by one small `atomic_file` module that never leaves a file half-written.) |
| `media` | Everything that runs ffmpeg: finding it, probing a video, the streams of preview pictures and preview sound, the thumbnails, and the export (planning, the filter graph, running and cancelling). The three streams share one small skeleton (`StreamingTool`). |
| `speed_indicator` | Draws the fast-forward symbol and decides where it goes, for the preview **and** the export, so they cannot disagree. |
| `playback` | The clock, which picture the preview needs for a frame, the `PictureSupplier` that keeps decoders running ahead of the clock, and the `AudioSupplier` that keeps the sound in step with it and feeds the speakers (through `cpal`). |
| `application` | The egui window. Decisions (where tiles go, what a slider position means, how an edit moves the current frame, which settings the windows show) are plain functions or small tables with tests; the drawing code is thin. The timeline is a directory of small files (ruler, clips, playhead, speed pop-up). |

Some choices that are not obvious from the code:

* **Whole frames everywhere.** Times are never floating-point seconds inside the model, so there is no drift and a
  cut is always on a frame.
* **ffmpeg as a child process, not libav bindings.** Building stays simple on every system and users can update
  ffmpeg themselves. All of it is behind `media`, so it could be replaced.
* **The preview's sound is the export's sound.** Both are made by the very same filter graph (the export's graph
  has the sound graph in it), so what is heard while playing is what ends up in the file. The sound is started
  from the playhead and kept in step with the clock by skipping a little sound or holding it back with silence,
  and started over when the playhead jumps or the edit changes.
* **Settings are a table.** `project::BOOLEAN_SETTINGS` lists every setting once; both settings windows are built
  from it, and tests go through it to check that every setting survives both files. Adding a setting is a field,
  a row, and a line in each file's document type (see the notes at the top of `project/settings.rs`).
* **The preview and the export select frames the same way** (same frame-rate normalisation, same
  `select='not(mod(n,N))'`), which is what lets the preview be trusted as a picture of the export.
* **Large projects.** The export's filter graph is given to ffmpeg in a file, so a project with hundreds of clips
  does not hit command-line length limits. The preview's sound graph is too.
* **`FastForwardApplication`'s "programmatic interface"** (`programmatic_interface.rs`) does what the File menu
  does without its file dialogs and lets you look at the state, and chooses where sound goes (the program makes
  no sound until it is told, so tests never make any noise). The window uses the same code; it is also how the
  end-to-end tests drive the real program.

## Testing

```text
cargo test                      # everything below
cargo clippy --all-targets -- -D warnings
```

| Suite | What it covers |
|---|---|
| unit tests (in `src/`) | the model, timeline geometry, both files, filter graph text, symbol drawing, the clock, the picture and sound suppliers (with fake decoders and speakers)... and **window interaction tests** that send simulated mouse and keyboard input to the real window (cutting with S, the arrow keys, the ruler, the speed slider, the symbol, dialogs, the missing-video warning, the menus and settings windows, autosave) |
| `tests/media_*.rs` | the real ffmpeg: probing, decoding, thumbnails, exports checked **picture by picture** against an independent calculation of what the project must contain, and the sound stream (right length and format, from any playhead position, sample for sample the same as the sound from the start, and the same loudness second by second as the export) |
| `tests/application_journeys.rs` | whole journeys through the real program with a real video: import, edit with the mouse and keyboard, save, load into a fresh editor, lose the video and choose it again, export |
| `tests/application_playback.rs` | playing: the preview keeps up with the clock, through a change of speed and a jump, and stops on the last picture. Timing-dependent, so it has its own file, which cargo runs on its own. |
| `tests/application_sound.rs` | the preview's sound in the real program, with recording stand-ins for the speakers: a test video whose pitch changes every second shows that the right sound is heard at the right time, through a change of speed and a jump; that it starts quickly, stops on pause, and is muted when asked |
| `tests/application_screenshots.rs` | not checks: with `FASTFORWARDRS_SCREENSHOT_FOLDER` set, `cargo test --test application_screenshots -- --ignored` draws the window in several states to PNG files |

The tests that use ffmpeg make their own small test videos (each frame carries its own frame number as a strip of
black and white blocks, so a test can read which frame it received) in `target/test_fixtures`. They are skipped
with a message when ffmpeg is missing; set `FASTFORWARDRS_REQUIRE_FFMPEG=1` to make that a failure instead.
One test opens the computer's real sound output and plays silence through it; it is skipped on a computer with
none.

## Known limits

* One source video per project (the file format allows more).
* Speeding sound up keeps its pitch (as the export does), so a sped-up clip talks fast but not squeaky.
* Autosave needs the project to have a file already.
* A variable-frame-rate source is exported at a constant frame rate (each picture slot gets the source picture
  nearest in time), and its preview can be a frame off at the start of a clip.
* The speed slider offers 1x to 1000x; a project file may hold a larger factor and the editor will keep it.
* There is no undo, and cuts cannot be joined again.
