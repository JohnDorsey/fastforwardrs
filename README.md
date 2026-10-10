# fastforwardrs

A small video editor with one job: **fast-forward the parts of a video you choose.**

Open a video, cut its track into clips, drag a slider on any clip to make it play 2x, 4x, 10x... faster, and
export the result. Fast-forwarded clips show a fast-forward symbol with their speed (like ▶▶ 4x), in the live
preview and in the exported video.

## Status

| | |
|---|---|
| Tested on | **Windows 10** (the first version is only tested there) |
| Written for | Windows, macOS and Linux. The only platform-specific code is the `.exe` suffix when looking for ffmpeg and hiding the console windows of the ffmpeg child processes (both `cfg(windows)`); a couple of unit tests have a Unix and a Windows variant, and only the Windows ones have been run. It has **not** been built or run on macOS or Linux yet; on Linux the window system libraries that `eframe`/`wgpu` need may have to be installed. |
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

**The File menu** (the only menu) has, in this order: *Load project*, *Save project*, *New project*, *Import*,
*Export*.

* *New project* asks for a video file straight away.
* *Import* chooses the video for a project that has none, or replaces the one a loaded project can't find.
  (A project holds one video in this version.)
* *Export* asks where to save the finished video (`.mp4`, `.mkv` or `.mov`) and shows its progress, with a
  Cancel button.
* There is no autosave. Loading, starting a new project and closing the window ask first if there are changes
  that have not been saved.

**The window** is the preview in the middle, and along the bottom the Pause / Play / Stop buttons with the
current time next to them (`hh:mm:ss.ff`, where `ff` is the frame within that second), then the video track,
then a slider that spans the whole window.

| To... | Do this |
|---|---|
| cut the video track | press **Space** (cuts at the current frame), or **left-click** the track (cuts where you clicked) |
| speed up a clip | press the **right mouse button** on the clip and **drag up**: a vertical slider appears (1x to 1000x, whole numbers; the arrow keys or mouse wheel nudge by one). Let go to set it. Drag back to the bottom for normal speed. |
| move through the video | drag the slider along the bottom, which covers the whole timeline. The mouse wheel scrolls the track without moving the current frame. |
| move or resize the fast-forward symbol | move the pointer over it in the preview (it is shown while the current frame is in a sped-up clip), then drag it, or drag its corner. There is one symbol position and size for the whole project. |

The track is a thick strip of pictures from the video, one for every second of the **finished** video, side by
side with no gaps. A sped-up clip is narrower on the strip, and the time shown is the time in the exported
video.

If a loaded project's video can't be found or opened, a warning appears in the middle of the window saying it
must be chosen again. The rest of the program stays usable and none of the edits are discarded; choosing the
video (the warning's button, or *Import*) puts it back. A replacement must have the same frame rate as the
original, because clips are stored as frame numbers.

## Project files

Pretty JSON, meant to be read (and, if needed, edited). The video itself is not inside the file: it is linked by
**both** an absolute path and a path relative to the project file, and the relative one is the backup when a
project and its video are moved together. The file format is versioned, and the file is written to a temporary
file first and then renamed into place, so a failed save cannot damage an existing project.

```json
{
  "file_format": "fastforwardrs-project",
  "file_format_version": 1,
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
  }
}
```

Several `source_videos` are allowed by the format; the editor uses one.

## Exported video

* Every picture is **exact**: a clip at speed N keeps source frames `first`, `first+N`, `first+2N`, ... and the
  export contains precisely the frames the project describes, at the source's constant frame rate. This is
  checked picture by picture against the project, on MP4 and MKV, at 29.97 fps, and from a variable-frame-rate
  source (see *Testing*).
* The sound is sped up with the picture and stays as long as it. Where a sped-up clip is so short that its sound
  would be under a tenth of a second, that stretch is silent instead.
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
| `project` | What a project *is*: source video, clips, speeds, and all the timeline arithmetic, in whole frames (`SourceFrameIndex`, `TimelineFrameIndex`, `SpeedFactor`...). Pure logic: no files, processes or drawing. |
| `project_file` | Saving and loading the JSON. Separate document types keep the file format independent of the in-memory model. |
| `media` | Everything that runs ffmpeg: finding it, probing a video, the stream of preview pictures, the thumbnails, and the export (planning, the filter graph, running and cancelling). |
| `speed_indicator` | Draws the fast-forward symbol and decides where it goes, for the preview **and** the export, so they cannot disagree. |
| `playback` | The clock, which picture the preview needs for a frame, and the `PictureSupplier` that keeps decoders running ahead of the clock. |
| `application` | The egui window. Decisions (where tiles go, what a slider position means, how an edit moves the current frame) are plain functions with tests; the drawing code is thin. |

Some choices that are not obvious from the code:

* **Whole frames everywhere.** Times are never floating-point seconds inside the model, so there is no drift and a
  cut is always on a frame.
* **ffmpeg as a child process, not libav bindings.** Building stays simple on every system and users can update
  ffmpeg themselves. All of it is behind `media`, so it could be replaced.
* **The preview and the export select frames the same way** (same frame-rate normalisation, same
  `select='not(mod(n,N))'`), which is what lets the preview be trusted as a picture of the export.
* **Large projects.** The export's filter graph is given to ffmpeg in a file, so a project with hundreds of clips
  does not hit command-line length limits.
* **`FastForwardApplication`'s "programmatic interface"** (`programmatic_interface.rs`) does what the File menu
  does without its file dialogs and lets you look at the state. The window uses the same code; it is also how the
  end-to-end tests drive the real program.

## Testing

```text
cargo test                      # everything below
cargo clippy --all-targets -- -D warnings
```

| Suite | What it covers |
|---|---|
| unit tests (in `src/`) | the model, timeline geometry, project file, filter graph text, symbol drawing, playback decisions... and **window interaction tests** that send simulated mouse and keyboard input to the real window (cutting, the speed slider, the symbol, dialogs, the missing-video warning, the menu) |
| `tests/media_*.rs` | the real ffmpeg: probing, decoding, thumbnails, and exports checked **picture by picture** against an independent calculation of what the project must contain |
| `tests/application_journeys.rs` | whole journeys through the real program with a real video: import, edit with the mouse, save, load into a fresh editor, lose the video and choose it again, export |
| `tests/application_playback.rs` | playing: the preview keeps up with the clock, through a change of speed and a jump, and stops on the last picture. Timing-dependent, so it has its own file, which cargo runs on its own. |
| `tests/application_screenshots.rs` | not checks: with `FASTFORWARDRS_SCREENSHOT_FOLDER` set, `cargo test --test application_screenshots -- --ignored` draws the window in several states to PNG files |

The tests that use ffmpeg make their own small test videos (each frame carries its own frame number as a strip of
black and white blocks, so a test can read which frame it received) in `target/test_fixtures`. They are skipped
with a message when ffmpeg is missing; set `FASTFORWARDRS_REQUIRE_FFMPEG=1` to make that a failure instead.

## Known limits

* One source video per project (the file format allows more).
* The preview is silent. Sound is in the export.
* A variable-frame-rate source is exported at a constant frame rate (each picture slot gets the source picture
  nearest in time), and its preview can be a frame off at the start of a clip.
* The speed slider offers 1x to 1000x; a project file may hold a larger factor and the editor will keep it.
* There is no undo, and cuts cannot be joined again.
