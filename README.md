# fastforwardrs

A small video editor with one main job: **fast-forward the parts of a video you choose.**

Open a video, cut it into clips, and make any clip play 2x, 4x, 10x... faster; export the result. Fast-forwarded
clips show a fast-forward symbol with their speed (like ▶▶ 4x), in the live preview and in the exported video.
The preview plays the sound too. A project can have several **tracks**, and on them videos, sound files and
**text objects**, which are put together in front-to-back order.

## Status

| | |
|---|---|
| Tested on | **Windows 10** (the first version is only tested there) |
| Written for | Windows, macOS and Linux. The only platform-specific code is the `.exe` suffix when looking for ffmpeg and hiding the console windows of the ffmpeg child processes (both `cfg(windows)`); a couple of unit tests have a Unix and a Windows variant, and only the Windows ones have been run. It has **not** been built or run on macOS or Linux yet. On Linux the window system libraries that `eframe`/`wgpu` need may have to be installed, and so may ALSA's development files (`libasound2-dev` on Debian and Ubuntu), which the sound output (`cpal`) is built with. Text is drawn with the computer's own fonts, which `cosmic-text` finds on each system. |
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
  * *New project* asks for a video or sound file straight away.
  * *Import* asks for a video or a sound file and puts **all of it on the timeline at the current frame**: on the
    front-most track that has nothing on it, or on a new track in front of all the others.
  * *Export* asks where to save the finished video (`.mp4`, `.mkv` or `.mov`) and shows its progress, with a
    Cancel button.
  * Loading, starting a new project and closing the window ask first if there are changes that have not been
    saved, unless Autosave is on (see *Settings*).
* **Edit**: *Deselect* (with its key, **Esc**, shown beside it; it can be used while a clip is selected),
  *Project settings* and *Global settings*, which open little windows (see *Settings*).

**The window** is the preview in the middle, and along the bottom the Stop / Play / Pause buttons with the
current time next to them (`hh:mm:ss.ff`, where `ff` is the frame within that second), then the timeline (a ruler,
and a row for every track), then a slider that spans the whole window.

| To... | Do this |
|---|---|
| play or pause | press **Space** (or the buttons) |
| move the playhead | **click or drag in the ruler** (the small strip with a mark for every second, where the red triangle sits); or press **Left** / **Right** to move it one frame back or forward, paused or playing (holding the key repeats); or drag the slider along the bottom |
| cut | press **S**: the selected clip is cut at the playhead if it is there, and if no clip is selected (or the selected one is somewhere else) every clip under the playhead, on every track, is cut |
| select a clip | **click** it. Clicking an empty part of a track, pressing **Esc** or choosing *Edit > Deselect* selects nothing. A selected clip has a bright outline. |
| delete a clip | select it and press **Delete** (or **Backspace**). The clips that touch the end of a deleted video or sound clip move up to close the gap; a gap, or a text object, stops that. |
| change a clip's speed, loudness or mute | **right-click** the clip: a box opens right above it, with a pointer at the clip. It has a speed slider (1x to 1000x, whole numbers), a gain slider (−30 to +30 dB with no change in the middle) and a *Mute* box. Click anywhere else, or press Esc, to close it. |
| change the settings of a whole track | press the **gear** in the box at the left of the track. The box that opens above it has the track's gain, speed and *Mute*: they apply to everything on the track, on top of each clip's own. The speed of a clip is its own speed times its track's, and that product is what the fast-forward symbol shows. |
| rename a track | type in its **title box** (new tracks are called `track 0`, `track 1`, ...) |
| add a track | press the big round **+** under the list of tracks; the new track goes in front of the others, at the top |
| make a text object | **right-click an empty part of a track**, choose *New* and then *Text object*. It lasts three seconds (or until the next clip), is selected, and its window opens. |
| change a text object | its window (it opens when the object is made, and again on a double click or right click on its clip, or a double click on it in the preview) has the words, the font, the size in points (on a picture 1080 tall), the colour and for how long it is shown |
| move or resize a text object | select it (click its clip, or click it in the preview): it shows a square at each of its four corners on the preview. Drag a **corner** to resize it from that corner, or drag the text itself to move it. |
| scroll the timeline | the mouse wheel over the ruler or a track scrolls it along without moving the playhead; with more than three tracks, the thin scroll bar at the right moves the rows up and down |
| move or resize the fast-forward symbol | move the pointer over it in the preview (it is shown while what is on screen is sped up), then drag it, or drag its corner. There is one symbol position and size for the whole project. |

None of the keys works while a text field is being typed in or while one of the boxes or windows that have
controls of their own (a clip's box, a track's box, the window of a text object) is open: they are the box's.

**Tracks.** The tracks are drawn in front-to-back order: tracks higher in the list are in front of those below.
A track is a row of clips that may have empty time between them, and a clip may be a piece of a **video**, a piece
of a **sound file** or a **text object**, in any mixture. A video clip is drawn as a row of pictures from the
video, one for every second of the **finished** video, side by side with no gaps; a sound clip is drawn as the
**waveform** of its sound, dark blue on pale violet; a text clip is teal, with what it says; a clip whose file
cannot be found is dull red with the file's name. Every clip has its corners rounded. A sped-up clip is narrower,
and the time shown is the time in the exported video. The red line is the playhead.

What is on the screen at a frame is: the **video of the front-most track that has one there** (it fills the
picture; videos of another shape than the first video's are fitted inside it with black beside them), with the
**text objects on the tracks in front of it** over it, and the fast-forward symbol over that; where no track has a
video the picture is black. What is heard is the **sound of every track mixed together**: each clip's sound at
its own loudness and its track's.

**Changing a length.** A clip that comes from a file gets shorter when it is sped up and longer when it is slowed
down, and the clips that *touch* its end move along with it so that no gap opens or closes. An empty stretch stops
the movement, and **text objects stay where they are** (and never move other clips). A text object lasts as many
seconds as its window says, whatever the speed of its track.

**The first video decides the picture.** The frame rate of the project is the frame rate of its first video, and
the size of the finished picture is that video's size. Until a project has a video it is 30 frames per second and
1920×1080. All the videos of a project must have the same frame rate (sound files can be anything), because clips
are stored as frame numbers.

**Sound.** While the video plays, its sound plays too, and it is the sound the export will have: every clip's
sound is cut out, sped up with the clip, and mixed with the other tracks' (see *Settings* for what sped-up sound
is like). The sound starts a fraction of a second after pressing Play (about 0.07 seconds was measured on the
development machine), jumps with the playhead when that is moved while playing, and stops the moment the video
is paused.

If a loaded project's file can't be found or opened, a warning appears in the middle of the window listing each
such file and saying it must be chosen again. The rest of the program stays usable and none of the edits are
discarded; choosing the file (the warning's button, or the *Choose the file again* button in the box of a clip
that is made from it) puts it back. A replacement must be the same kind of file, and a video must have the same
frame rate as the original.

## Settings

| Setting | What it does | Default |
|---|---|---|
| Autosave | Saves the project to its file by itself two seconds after the last change. Only for a project that already has a file: the first save always asks where. With it on, closing the window or loading another project do not ask about unsaved changes: they save. | off |
| Sound of sped-up clips | A drop-down list for what the sound of a clip that plays faster than normal is like, in the preview and in the exported video: **Mute** (silence of the same length), **Maintain pitch** (faster, but not higher) or **Increase pitch** (like a tape wound on: higher, the more the faster; a clip faster than 1000x is silent) | Maintain pitch |

The **global settings** (*Edit > Global settings*) are the same for every project. They are kept in a file in
the operating system's folder for programs' settings (`%APPDATA%\fastforwardrs\settings.json` on Windows), which
is written as soon as one is changed.

The **project settings** (*Edit > Project settings*) have a checkbox, *Use global setting*, next to each
setting. While it is ticked the project follows the global setting: the setting beside it is greyed out (showing
the global value that is in use) and nothing about it is saved in the project file. Unticking it gives the
project a setting of its own. A new project follows all the global settings.

## Project files

Pretty JSON, meant to be read (and, if needed, edited). The videos and sound files themselves are not inside the
file: each is linked by **both** an absolute path and a path relative to the project file, and the relative one
is the backup when a project and its files are moved together. The file format is versioned, and the file is
written to a temporary file first and then renamed into place, so a failed save cannot damage an existing
project. Tracks are listed front-most first; a clip has `gap_before_frames` (left out when 0), and gain and mute
are left out when they have their normal values.

```json
{
  "file_format": "fastforwardrs-project",
  "file_format_version": 3,
  "sources": [
    {
      "kind": "video",
      "identifier": 1,
      "absolute_path": "C:\\videos\\holiday.mp4",
      "relative_path": "holiday.mp4",
      "frame_rate": "30/1",
      "total_frame_count": 1200,
      "display_width_in_pixels": 1280,
      "display_height_in_pixels": 720,
      "has_audio": true
    },
    {
      "kind": "audio",
      "identifier": 2,
      "absolute_path": "C:\\music\\song.mp3",
      "relative_path": "../music/song.mp3",
      "duration_in_microseconds": 10000000
    }
  ],
  "tracks": [
    {
      "title": "track 1",
      "gain_decibels": 0.0,
      "speed_factor": 1,
      "muted": false,
      "clips": [
        {
          "kind": "text",
          "gap_before_frames": 60,
          "duration_in_frames": 90,
          "text": "Chapter one",
          "font_family": "",
          "font_size_fraction_of_frame_height": 0.06,
          "colour": "#FFFFFFFF",
          "area": {
            "left_fraction_of_frame_width": 0.1,
            "top_fraction_of_frame_height": 0.74,
            "width_fraction_of_frame_width": 0.8,
            "height_fraction_of_frame_height": 0.16
          }
        }
      ]
    },
    {
      "title": "track 0",
      "gain_decibels": -3.0,
      "speed_factor": 1,
      "muted": false,
      "clips": [
        { "kind": "video", "source_identifier": 1, "first_source_frame": 0, "end_source_frame": 300, "speed_factor": 1 },
        { "kind": "video", "source_identifier": 1, "first_source_frame": 300, "end_source_frame": 600, "speed_factor": 4, "muted": true },
        { "kind": "audio", "gap_before_frames": 30, "source_identifier": 2, "first_source_frame": 0, "end_source_frame": 150, "speed_factor": 1, "gain_decibels": 6.0 }
      ]
    }
  ],
  "speed_indicator": {
    "left_fraction_of_frame_width": 0.03,
    "top_fraction_of_frame_height": 0.04,
    "height_fraction_of_frame_height": 0.08
  },
  "settings": {
    "sped_up_clip_sound": "mute"
  }
}
```

(The clips are shown on one line here to save room; the program writes one value to a line.) A text object's
font size is a fraction of the picture's height, which the window shows as points on a picture 1080 tall; an
empty `font_family` is the program's default font. `settings` holds only the settings the project has chosen for
itself, and is left out altogether when the project follows them all. **Older files are still read**: version 2
files (one video and its clips) become a project with one track called `track 0`, and version 1 files (from
before there were settings) follow all the settings. The old yes-or-no setting *mute sped-up clips* is read as
*Mute* or *Maintain pitch*.

## Exported video

* Every picture is **exact**: a clip at speed N keeps source frames `first`, `first+N`, `first+2N`, ... and the
  export contains precisely the frames the project describes, at the project's constant frame rate. This is
  checked picture by picture against the project, on MP4 and MKV, at 29.97 fps, and from a variable-frame-rate
  source (see *Testing*). Empty time is black.
* The picture of each frame is made the way the preview shows it: the video that is in front, then the
  fast-forward symbol, then the text objects from the back to the front. Text is drawn by the same code for the
  preview and for the export, at the size of the finished picture.
* The sound is every track's sound mixed, each clip sped up with its picture (see *Settings*), and it is as long as
  the picture. Where a sped-up clip is so short that its sound would be under a tenth of a second, that stretch is
  silent instead.
* H.264 (`libx264`, falling back to `mpeg4` if your ffmpeg has no libx264) and AAC. `.mp4` and `.mov` files are
  written for streaming (`+faststart`).
* If a project was pointed at a file **shorter** than the one it was edited on, all the edits are kept and the
  timeline keeps its length (so that things on other tracks stay where they are): where a video has no pictures
  any more the picture is black and where a sound has no sound any more it is silent, and the message at the
  end of the export says how many frames of the project are black.
* The video is written to `name.exporting.ext` next to the destination and renamed when it is complete, so
  cancelling or failing never leaves a half-written file or damages an existing one. Exporting over a source
  file is refused.

## How it is built

The code is a library plus a thin `main`. It is layered, and each layer only depends on the ones below it:

| Module | What is in it |
|---|---|
| `project` | What a project *is*: its source files, its tracks (`project::tracks`: a `Track` is a row of clips with gaps, a speed, a gain and a mute switch, and holds all the arithmetic of which clip is at which frame and what moves when a length changes), the speed and gain types, text objects, settings. Pure logic: no files, processes or drawing. In whole frames (`SourceFrameIndex`, `TimelineFrameIndex`, `SpeedFactor`...). |
| `composition` | Works out from a project what the finished video is: the **picture plan** (for each stretch of the timeline, which video is seen and which text objects are over it) and the **sound plan** (for each track, which clips are heard when, and how loud). Plain logic. The live preview and the export both use these plans, which is what makes them agree. |
| `project_file` | Saving and loading the project JSON, including the older versions. Separate document types keep the file format independent of the in-memory model. |
| `settings_file` | The same for the global settings file, which is more forgiving: a missing setting gets its default and an unknown one is ignored. (Both files are written by one small `atomic_file` module that never leaves a file half-written.) |
| `media` | Everything that runs ffmpeg: finding it, probing a video or sound file, the streams of preview pictures and preview sound, the thumbnails, the waveforms, and the export (planning, the filter graph in a picture half and a sound half, running and cancelling). The streams share one small skeleton (`StreamingTool`). |
| `rgba_picture`, `text_rendering` | A picture made by the program (as pixels), and the drawing of a text object into one, with `cosmic-text` (fonts, wrapping, fallback fonts for other alphabets and emoji). The fonts of the computer take a moment to load, which is done once on another thread. |
| `speed_indicator` | Draws the fast-forward symbol and decides where it goes, for the preview **and** the export, so they cannot disagree. |
| `playback` | The clock, which picture the preview needs for a frame, the `PictureSupplier` that keeps decoders running ahead of the clock, and the `AudioSupplier` that keeps the sound in step with it and feeds the speakers (through `cpal`). |
| `application` | The egui window. Decisions (where tiles and waveform bars go, what a slider position means, how an edit moves the current frame, which keys do what, which settings the windows show) are plain functions or small tables with tests; the drawing code is thin. The timeline is a directory of small files (ruler, a track's header, its strip, the clips, the playhead, the boxes of settings, and the `StripView` that turns a place along the strips into a place on the screen and back, so that no drawing code writes that formula out for itself), the preview too (the symbol and the text objects), and the application itself is a set of files by what they are about (flows for source files, the project file, export, dialogs, and one place that carries out every command). |

Some choices that are not obvious from the code:

* **Whole frames everywhere.** Times are never floating-point seconds inside the model, so there is no drift and a
  cut is always on a frame.
* **Ripple rules live in one place.** What moves when a clip changes length or goes is decided by `Track` alone
  (media clips that touch follow; a gap stops it; text stays), and is tested against a simple model by random
  edits.
* **ffmpeg as a child process, not libav bindings.** Building stays simple on every system and users can update
  ffmpeg themselves. All of it is behind `media`, so it could be replaced.
* **The preview is the export.** The picture plan and the sound plan are made once, in `composition`, and both
  turn them into pictures and sound (live, or through one ffmpeg filter graph); the sound graph the preview plays
  is the one the export has in it. The sound is started from the playhead and kept in step with the clock by
  skipping a little sound or holding it back with silence, and started over when the playhead jumps or the edit
  changes. A clip is laid on silence of its exact length rather than padded, because ffmpeg's `apad` gives frames
  without timestamps when it pads nothing (a clip that lies wholly past the end of a shorter file).
* **Commands.** Drawing code never changes the project: it pushes a `Command` (select, delete, set a clip's speed,
  make a text object...) and one function afterwards carries out each. That one place also keeps the playhead on
  the picture it was showing when an edit moves clips along the timeline.
* **Settings are a table.** `project::SETTINGS` lists every setting once (a checkbox or a drop-down list); both
  settings windows are built from it, and tests go through it to check that every setting survives both files.
  Adding a setting is a field, a row, and a line in each file's document type (see the notes at the top of
  `project/settings.rs`).
* **The preview and the export select frames the same way** (same frame-rate normalisation, same
  `select='not(mod(n,N))'`), which is what lets the preview be trusted as a picture of the export.
* **Large projects.** The export's filter graph is given to ffmpeg in a file, so a project with hundreds of clips
  does not hit command-line length limits. The preview's sound graph is too. The picture plan is made in a time
  that grows in step with the number of clips (6000 clips take under 2 milliseconds in a release build).
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
| unit tests (in `src/`) | the model (including the rules for what moves when a clip changes, checked against a simple model with random edits), the plans in `composition` (checked frame by frame), timeline geometry and waveform bars, both files and the older versions of the project file, filter graph text, symbol drawing, text drawing (with the computer's fonts), the clock, the picture and sound suppliers (with fake decoders and speakers)... and **window interaction tests** that send simulated mouse and keyboard input to the real window: selecting, deselecting, deleting and cutting; tracks, their titles and the round button; the boxes above clips and tracks; text objects from the menu, their window and their corners on the preview; the keys; dialogs; the menus and settings windows; autosave |
| `tests/media_*.rs` | the real ffmpeg: probing videos and sound files, decoding, thumbnails, waveforms, exports checked **picture by picture** against an independent calculation of what the project must contain (and tone by tone for the sound of several tracks, with gain and speed), and the sound stream (right length and format, from any playhead position, sample for sample the same as the sound from the start, and the same loudness second by second as the export) |
| `tests/application_journeys.rs` | whole journeys through the real program with a real video: import, edit with the mouse and keyboard, save, load into a fresh editor, lose a file and choose it again, import a sound file, make a text object, export (and check the picture and the sound of the result) |
| `tests/application_playback.rs` | playing: the preview keeps up with the clock, through a change of speed and a jump, and stops on the last picture. Timing-dependent, so it has its own file, which cargo runs on its own. |
| `tests/application_sound.rs` | the preview's sound in the real program, with recording stand-ins for the speakers: a test video whose pitch changes every second shows that the right sound is heard at the right time, through a change of speed and a jump; that it starts quickly, stops on pause, and is muted or raised in pitch when asked |
| `tests/application_screenshots.rs` | not checks: with `FASTFORWARDRS_SCREENSHOT_FOLDER` set, `cargo test --test application_screenshots -- --ignored` draws the window in several states to PNG files (tracks, a waveform, text, the boxes of settings, the menu, a missing file...) |

The tests that use ffmpeg make their own small test videos (each frame carries its own frame number as a strip of
black and white blocks, so a test can read which frame it received) and tone files in `target/test_fixtures`.
They are skipped with a message when ffmpeg is missing; set `FASTFORWARDRS_REQUIRE_FFMPEG=1` to make that a
failure instead. One test opens the computer's real sound output and plays silence through it; it is skipped on a
computer with none. The text tests are skipped on a computer with no fonts at all.

## Known limits

* Clips cannot be moved along the timeline or between tracks (only cut, sped up, deleted, and for text objects
  made again somewhere else), and cut clips cannot be joined again. There is no undo.
* All the videos of a project must run at the same frame rate, and the finished picture is the size of the first
  video.
* A text object has one font, size and colour for all of its words, and is drawn from the top left of its area,
  wrapped to its width and cut off at its bottom edge.
* Speeding sound up with *Maintain pitch* does not keep very short clips (under a tenth of a second of sped-up
  sound), and *Increase pitch* is silent beyond 1000x.
* Autosave needs the project to have a file already.
* A variable-frame-rate source is exported at a constant frame rate (each picture slot gets the source picture
  nearest in time), and its preview can be a frame off at the start of a clip.
* The speed sliders offer 1x to 1000x; a project file may hold a larger factor and the editor will keep it.
* Waveforms are made for sound *files*; a video's own sound is not drawn.
