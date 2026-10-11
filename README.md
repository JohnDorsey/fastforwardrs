# fastforwardrs

A small video editor with one main job: **fast-forward the parts of a video you choose.**

Open a video, cut it into clips, and make any clip play 2x, 4x, 10x... faster; export the result. Fast-forwarded
clips show a fast-forward symbol with their speed (like ▶▶ 4x), in the live preview and in the exported video.
The preview plays the sound too. A project can have several **tracks**, and on them videos, sound files,
**pictures** and **text objects**, which are put together in front-to-back order: a video or a picture (or a
text) can be made smaller, moved and **turned** on the picture, so that what is behind it shows. Every edit can
be undone.

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
  * *Import* asks for a video, a sound file or a **picture** (PNG, JPEG, BMP, WebP or TIFF) and puts **all of
    it on the timeline at the current frame**: on the front-most track that has nothing on it, or on a new track in
    front of all the others. A picture is shown for five seconds (change that in its box, see below).
  * *Export* asks where to save the finished video (`.mp4`, `.mkv` or `.mov`) and shows its progress, with a
    Cancel button.
  * Loading, starting a new project and closing the window ask first if there are changes that have not been
    saved, unless Autosave is on (see *Settings*).
* **Edit**: *Undo* and *Redo*, *Deselect* (with its key, **Esc**, shown beside it; it can be used while a clip is
  selected), *Project settings* and *Global settings*, which open little windows (see *Settings*).

**The window** is the preview in the middle, and along the bottom the Stop / Play / Pause buttons with the
current time next to them (`hh:mm:ss.ff`, where `ff` is the frame within that second), then the timeline (a round
**+** button, a ruler, and a row for every track), then a slider that spans the whole window.

| To... | Do this |
|---|---|
| play or pause | press **Space** (or the buttons) |
| move the playhead | **click or drag in the ruler** (the strip with the marks, where the red triangle sits); or press **Left** / **Right** to move it one frame back or forward, paused or playing (holding the key repeats); or drag the slider along the bottom |
| undo and redo | **Ctrl+Z** undoes the latest edit; **Ctrl+Y** (or **Ctrl+Shift+Z**) does it again; or use the Edit menu. Everything that changes the project is an edit: cutting, deleting, moving, speeds, volumes, tracks (their titles, settings, heights, adding and deleting them), text, where things are on the picture, importing a file and the fast-forward symbol. Dragging a clip or a slider, or typing a title, is one edit however long it goes on. **Not** edits: the settings of the program and of the project, and choosing a missing file again. |
| cut | press **S**: the selected clips that are at the playhead are cut there; if none is, every clip under the playhead, on every track, is cut |
| select clips | **click** a clip; **Ctrl+click** adds a clip to the selection or takes it away; or **drag a rectangle** from any empty place on the timeline (also from the gaps between tracks) to select every clip it touches, on any track (hold Ctrl to keep what was selected). Clicking an empty part of a track, pressing **Esc** or choosing *Edit > Deselect* selects nothing. Selected clips have a bright outline. |
| delete clips | select them and press **Delete** (or **Backspace**). The clips that touch the end of a deleted video or sound clip move up to close the gap; a gap, a picture or a text object stops that. |
| move clips | **drag** a clip along its track. It cannot go onto another clip (it stops against it) or before the start of the timeline. If several are selected they all move together (and stop together). Clips cannot be moved onto another track. |
| change a clip's speed, loudness or mute | **right-click** the clip: a box opens right above it, with a pointer at the clip. It has a speed slider (1x to 1000x, whole numbers), a gain slider (−30 to +30 dB with no change in the middle) and a *Mute* box. If several clips are selected and it is one of them, what is changed is changed for all of them. Click anywhere else, or press Esc, to close it. |
| change how long a picture is shown | **right-click** its clip: the box has *Shown for*, in seconds (the picture is shown for as long as there is room before the next clip on its track). |
| change the settings of a whole track | press the **gear** in the box at the left of the track. The box that opens above it has the track's gain, speed and *Mute*: they apply to everything on the track, on top of each clip's own. The speed of a clip is its own speed times its track's, and that product is what the fast-forward symbol shows. |
| rename a track | type in its **title box** (new tracks are called `track 0`, `track 1`, ...) |
| make a track taller or shorter | drag the **boundary** under its title box. A track can be made as short as its title and gear need, or half as tall again as a new one. A video on a shorter track is drawn with smaller pictures, taken more often. |
| add a track | press the big round **+** above the list of tracks; the new track goes in front of the others, at the top |
| delete a track | **right-click** its title box (or the box's empty part) and choose *Delete track*: the track and everything on it go (the files stay part of the project). The only track cannot be deleted. |
| make a text object | **right-click an empty part of a track**, choose *New* and then *Text object*. It lasts three seconds (or until the next clip), is selected, and its window opens. |
| change a text object | its window (it opens when the object is made, and again on a double click or right click on its clip, or a double click on it in the preview) has the words, the font, the size in points (on a picture 1080 tall), the colour and for how long it is shown |
| pick, move, resize and turn something on the preview | **click** a video, a picture or a text object on the preview to select it (or select its clip on the timeline): it shows an outline and a square at each of its four corners. **Drag it** to move it; drag a **corner** to resize it from that corner (a video or a picture keeps its shape, text takes whatever shape you drag it to); drag from **just outside a corner** (a faint ring appears) to **turn** it about its middle, clockwise or not, with its angle shown beside the pointer, and with **Shift** held to turn it by 15 degrees at a time. A video that has not been touched fills the picture (or as much of it as its shape lets it). What is turned is cut off at the edge of the picture. |
| zoom the timeline | **Ctrl + the mouse wheel** over the ruler or a track zooms in and out about the pointer (from some minutes across the whole window to a few frames across it, with a mark for every frame). The ruler's marks, and its time labels (`m:ss`), follow the zoom. |
| scroll the timeline | the mouse wheel over the ruler or a track scrolls it along without moving the playhead; **Alt + the mouse wheel** (or the thin scroll bar at the right, with more than three tracks) moves the rows up and down |
| move or resize the fast-forward symbol | move the pointer over it in the preview (it is shown while the video that is in front is sped up), then drag it, or drag its corner. There is one symbol position and size for the whole project. |

None of the keys works while a text field is being typed in or while one of the boxes or windows that have
controls of their own (a clip's box, a track's box, the window of a text object) is open: they are the box's.

**Tracks.** The tracks are drawn in front-to-back order: tracks higher in the list are in front of those below.
A track is a row of clips that may have empty time between them (and never two clips at the same time), and a
clip may be a piece of a **video**, a piece of a **sound file**, a **picture** or a **text object**, in any
mixture. A video clip is drawn as a row of pictures from the video, one for every second of the **finished**
video, side by side with no gaps (or more often, when you zoom in or make the track shorter); a sound clip is
drawn as the **waveform** of its sound, dark blue on pale violet; a picture clip is violet, with the picture once
and the name of its file; a text clip is teal, with what it says; a clip whose file cannot be found is dull red
with the file's name. Every clip has its corners rounded and its top and left edges lit, so that dark clips can be
told apart. A sped-up clip is narrower, and the time shown is the time in the exported video. The red line is the
playhead.

**What is on the screen at a frame** is made of **layers**, the one on the back-most track first. Every video,
picture and text object that is there at the frame is a layer, in the place on the picture that it was given
(the default place of a video or a picture is the biggest it can be inside the picture, in the middle, in its
own shape), and turned as far as it was turned. Where a layer does not cover the picture, what is behind it
shows, down to black. The fast-forward symbol goes over the front-most video (if that is sped up), under whatever
is in front of that. Two details that are decisions rather than accidents:

* A video that fills the picture and is not turned hides everything behind it, so nothing behind it is read or
  drawn. A **picture** never counts as hiding what is behind it (a picture file may have clear places), and nor
  does a text object.
* A video that has no picture to show (its file is missing, or is shorter than its clip) is not there at all for
  those frames: what is behind it shows. (It used to leave the picture black, because a video always filled the
  picture.)

What is heard is the **sound of every track mixed together**: each clip's sound at its own loudness and its
track's.

**Changing a length.** A clip that comes from a file gets shorter when it is sped up and longer when it is slowed
down, and the clips that *touch* its end move along with it so that no gap opens or closes. An empty stretch stops
the movement, and **pictures and text objects stay where they are** (and never move other clips). A picture or a
text object lasts as many seconds as its box or window says, whatever the speed of its track.

**The first video decides the picture.** The frame rate of the project is the frame rate of its first video, and
the size of the finished picture is that video's size. Until a project has a video it is 30 frames per second and
1920×1080. All the videos of a project must have the same frame rate (sound files and pictures can be anything),
because clips are stored as frame numbers.

**Sound.** While the video plays, its sound plays too, and it is the sound the export will have: every clip's
sound is cut out, sped up with the clip, and mixed with the other tracks' (see *Settings* for what sped-up sound
is like). The sound starts a fraction of a second after pressing Play (about 0.07 seconds was measured on the
development machine), jumps with the playhead when that is moved while playing, and stops the moment the video
is paused. Changing the volume of one clip or track while playing does not interrupt the sound of the others.

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

Pretty JSON, meant to be read (and, if needed, edited). The videos, sounds and pictures themselves are not inside
the file: each is linked by **both** an absolute path and a path relative to the project file, and the relative
one is the backup when a project and its files are moved together. The file format is versioned, and the file is
written to a temporary file first and then renamed into place, so a failed save cannot damage an existing
project. Tracks are listed front-most first; a clip has `gap_before_frames` (left out when 0), and gain, mute, a
track's height, and the place and turn of a clip are left out when they have their normal values. Where
something is on the picture is always **fractions of the picture's width and height** (and the turn is in
degrees, clockwise), so it comes out in the same place whatever size the picture is shown or exported at.

```json
{
  "file_format": "fastforwardrs-project",
  "file_format_version": 4,
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
    },
    {
      "kind": "image",
      "identifier": 3,
      "absolute_path": "C:\\pictures\\logo.png",
      "relative_path": "../pictures/logo.png",
      "display_width_in_pixels": 800,
      "display_height_in_pixels": 400
    }
  ],
  "tracks": [
    {
      "title": "track 1",
      "gain_decibels": 0.0,
      "speed_factor": 1,
      "muted": false,
      "height_in_points": 80.0,
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
          },
          "rotation_degrees": -12.5
        },
        {
          "kind": "image",
          "gap_before_frames": 30,
          "source_identifier": 3,
          "duration_in_frames": 150,
          "placement": {
            "area": {
              "left_fraction_of_frame_width": 0.6,
              "top_fraction_of_frame_height": 0.05,
              "width_fraction_of_frame_width": 0.3,
              "height_fraction_of_frame_height": 0.3
            },
            "rotation_degrees": 20.0
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

(The clips of the last track are shown on one line each here to save room; the program writes one value to a
line. A test loads this very example, so it cannot go out of date without the tests saying so.) A text object's
font size is a fraction of the picture's height, which the window shows as points on a picture 1080 tall; an
empty `font_family` is the program's default font. A video clip may have a `placement` like the picture's
(without one it is fitted inside the picture), and the `area` of a placement is the rectangle **before** it is
turned, which is always inside the picture. `settings` holds only the settings the project has chosen for
itself, and is left out altogether when the project follows them all. **Older files are still read**: version 3
files have no heights, pictures, places or turns (every track is of the normal height, every video is fitted
inside the picture and every text is upright); version 2 files (one video and its clips) become a project with
one track called `track 0`, and version 1 files (from before there were settings) follow all the settings. The
old yes-or-no setting *mute sped-up clips* is read as *Mute* or *Maintain pitch*.

## Exported video

* Every picture is **exact**: a clip at speed N keeps source frames `first`, `first+N`, `first+2N`, ... and the
  export contains precisely the frames the project describes, at the project's constant frame rate. This is
  checked picture by picture against the project, on MP4 and MKV, at 29.97 fps, and from a variable-frame-rate
  source (see *Testing*). Empty time is black.
* The picture of each frame is made the way the preview shows it: the layers from the back to the front over
  black, the fast-forward symbol over the front-most video. Each video or picture is scaled to where it is on the
  picture (after the pictures a sped-up clip does not keep are dropped, so those are never scaled), and what is
  turned is made into a picture with clear corners before it is laid over. Text is drawn by the same code for the
  preview and for the export, at the size of the finished picture, and turned the same way.
* The sound is every track's sound mixed, each clip sped up with its picture (see *Settings*), and it is as long as
  the picture. Where a sped-up clip is so short that its sound would be under a tenth of a second, that stretch is
  silent instead.
* H.264 (`libx264`, falling back to `mpeg4` if your ffmpeg has no libx264) and AAC. `.mp4` and `.mov` files are
  written for streaming (`+faststart`).
* If a project was pointed at a file **shorter** than the one it was edited on, all the edits are kept and the
  timeline keeps its length (so that things on other tracks stay where they are): where a video has no pictures
  any more it is not there (what is behind it shows, or black) and where a sound has no sound any more it is
  silent, and the message at the end of the export says how many frames of the project are missing the picture of
  a video.
* The video is written to `name.exporting.ext` next to the destination and renamed when it is complete, so
  cancelling or failing never leaves a half-written file or damages an existing one. Exporting over a source
  file is refused.

## How it is built

The code is a library plus a thin `main`. It is layered, and each layer only depends on the ones below it:

| Module | What is in it |
|---|---|
| `project` | What a project *is*: its source files, its tracks (`project::tracks`: a `Track` is a row of clips with gaps, a speed, a gain, a height and a mute switch, and holds all the arithmetic of which clip is at which frame, what moves when a length changes and whether clips may be moved), the speed and gain types, text objects, pictures, **placements** (`placement.rs`: a rectangle on the picture as fractions, an angle, how a corner drags and what a point is on), the fit of a video inside a picture, settings. Pure logic: no files, processes or drawing. In whole frames (`SourceFrameIndex`, `TimelineFrameIndex`, `SpeedFactor`...). |
| `composition` | Works out from a project what the finished video is: the **picture plan** (for each stretch of the timeline, the layers that are seen: their video, picture or text, and where each is) and the **sound plan** (for each track, which clips are heard when, and how loud). Plain logic. The live preview and the export both use these plans, which is what makes them agree. |
| `project_file` | Saving and loading the project JSON, including the older versions. Separate document types keep the file format independent of the in-memory model. |
| `settings_file` | The same for the global settings file, which is more forgiving: a missing setting gets its default and an unknown one is ignored. (Both files are written by one small `atomic_file` module that never leaves a file half-written.) |
| `media` | Everything that runs ffmpeg: finding it, probing a video, sound or picture file, the streams of preview pictures and preview sound, the decoding of a picture file, the thumbnails (at several levels of detail), the waveforms, and the export (planning, the filter graph in a picture half and a sound half, running and cancelling). The streams share one small skeleton (`StreamingTool`). |
| `rgba_picture`, `text_rendering` | A picture made by the program (as pixels), and the drawing of a text object into one, with `cosmic-text` (fonts, wrapping, fallback fonts for other alphabets and emoji). The fonts of the computer take a moment to load, which is done once on another thread. The text is drawn upright in its area: turning it is done when it is laid over. |
| `speed_indicator` | Draws the fast-forward symbol and decides where it goes, for the preview **and** the export, so they cannot disagree. |
| `playback` | The clock, which pictures the preview needs for a frame (one for each video that is seen), the `PreviewPictures` that keeps a `PictureSupplier` for each track (decoders running ahead of the clock), and the `AudioSupplier` that keeps the sound in step with it and feeds the speakers (through `cpal`). |
| `application` | The egui window. Decisions (where tiles and waveform bars go, how far a clip may be moved, what a slider position means, how an edit moves the current frame, which keys do what, where a layer's handles are and what a point on the preview is on, which settings the windows show) are plain functions or small tables with tests; the drawing code is thin. The timeline is a directory of small files (ruler, a track's header, its strip, the clips, the playhead, the boxes of settings, resizing a track, scrolling and zooming, and the `StripView` that turns a place along the strips into a place on the screen and back, so that no drawing code writes that formula out for itself), the preview too (the layers and their handles, the symbol), the history of edits and the selection are modules of their own, and the application itself is a set of files by what they are about (flows for source files, the project file, export, dialogs, and one place that carries out every command). |

Some choices that are not obvious from the code:

* **Whole frames everywhere.** Times are never floating-point seconds inside the model, so there is no drift and a
  cut is always on a frame.
* **Where things are on the picture is fractions, and angles are worked out in pixels.** The fractions of the
  width and of the height are not the same length, so nothing about an angle (where a turned corner is, what is
  under a point, how a turned rectangle is resized with the corner across staying where it is) can be done in
  them; `placement.rs` does it in the pixels of a picture of a given size, which the preview (drawn at the same
  shape) shares with the export.
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
* **The preview is the export.** The picture plan and the sound plan are made once, in `composition`, and both
  turn them into pictures and sound (live, or through one ffmpeg filter graph); the sound graph the preview plays
  is the one the export has in it. The preview draws the layers itself (as textured, turned rectangles) and the
  export lays them over each other with `scale`, `rotate` and `overlay`; the two agree because the places come
  from the same plan and the same placement arithmetic, and tests check the export's pixels where turned and
  scaled things must be. The sound is started from the playhead and kept in step with the clock by skipping a
  little sound or holding it back with silence, and started over when the playhead jumps or the edit changes
  (when it changes, the new sound is started beside the old and crossfaded into it, so that editing a volume
  does not make a gap). A clip is laid on silence of its exact length rather than padded, because ffmpeg's `apad`
  gives frames without timestamps when it pads nothing (a clip that lies wholly past the end of a shorter file).
* **Commands.** Drawing code never changes the project: it pushes a `Command` (select, delete, move, set a clip's
  speed, put a clip somewhere on the picture, make a text object...) and one function afterwards carries out
  each. That one place also keeps the playhead on the picture it was showing when an edit moves clips along the
  timeline.
* **One widget takes every press on the preview.** egui decides who gets a press at the start of a frame, from
  the widgets of the frame before. A preview whose layers each felt the mouse only while they were what the pointer
  was over would give a click that came in the same frame as the move that brought the pointer there (a tap on a
  touch screen, a very quick click) to whatever the pointer was over a frame earlier, or to nothing. So the preview
  has a single widget over all its room, and `grab_at` works out from where a press was what it takes hold of: the
  handles and the places to turn from of the selected layer, then the layers from the front (each as far as it
  really is, turned as it is), with the fast-forward symbol between the front-most video and what is in front of
  it. Only the left button picks and drags. The test window gives every event a frame of its own, as egui's test
  harness does, so the tests that need more than one event in a frame say so (`Window::all_in_one_frame`).
* **Native file dialogs are behind a trait** (`FileChooser`), and the editor in tests has one that panics when
  asked, so a test that reaches a dialog fails at once instead of hanging on a window nobody can click.
* **Settings are a table.** `project::SETTINGS` lists every setting once (a checkbox or a drop-down list); both
  settings windows are built from it, and tests go through it to check that every setting survives both files.
  Adding a setting is a field, a row, and a line in each file's document type (see the notes at the top of
  `project/settings.rs`).
* **The preview and the export select frames the same way** (same frame-rate normalisation, same
  `select='not(mod(n,N))'`), which is what lets the preview be trusted as a picture of the export.
* **Large projects.** The export's filter graph is given to ffmpeg in a file, so a project with hundreds of clips
  does not hit command-line length limits. The preview's sound graph is too. The picture plan is made in a time
  that grows in step with the number of clips.
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
| unit tests (in `src/`) | the model (including the rules for what moves when a clip changes and for moving clips, checked against a simple model with random edits), placements (corner dragging checked with random drags), the plans in `composition` (checked frame by frame against a plain evaluation, with random projects that have videos of other shapes, places and turns), timeline geometry, zoom, thumbnail levels and waveform bars, both files and the older versions of the project file, filter graph text, symbol drawing, text drawing (with the computer's fonts), the clock, the picture and sound suppliers (with fake decoders and speakers), the history... and **window interaction tests** that send simulated mouse and keyboard input to the real window: selecting (with Ctrl and a rectangle), moving, deleting and cutting clips; undo and redo; zooming, scrolling and resizing tracks; tracks, their titles, the round button and deleting one; the boxes above clips and tracks; text objects from the menu, their window; picking, moving, resizing and turning videos, pictures and text on the preview (also with a press that comes in the same frame as the move of the pointer, as a tap does); the keys; dialogs; the menus and settings windows; autosave |
| `tests/media_*.rs` | the real ffmpeg: probing videos, sound files and pictures, decoding, thumbnails (checked to be the picture on screen at the moment they stand for), waveforms, exports checked **picture by picture** against an independent calculation of what the project must contain (and tone by tone for the sound of several tracks, with gain and speed), **pixel by pixel for what is scaled, moved and turned** (a video at half size, a video turned a quarter, text turned so that its red left half goes to the top, something cut off at the picture's edge, a picture from a file that is turned, comes later in the timeline, or lies over a video that is cut in pieces), and the sound stream (right length and format, from any playhead position, sample for sample the same as the sound from the start, and the same loudness second by second as the export) |
| `tests/application_journeys.rs` | whole journeys through the real program with a real video: import, edit with the mouse and keyboard, undo, save, load into a fresh editor, lose a file and choose it again, import a sound file or a picture, make a text object, export (and check the picture and the sound of the result) |
| `tests/application_playback.rs` | playing: the preview keeps up with the clock, through a change of speed and a jump, and stops on the last picture. Timing-dependent, so it has its own file, which cargo runs on its own. |
| `tests/application_sound.rs` | the preview's sound in the real program, with recording stand-ins for the speakers: a test video whose pitch changes every second shows that the right sound is heard at the right time, through a change of speed and a jump; that it starts quickly, stops on pause, and is muted or raised in pitch when asked; that changing a volume while playing leaves no gap |
| `tests/application_screenshots.rs` | not checks: with `FASTFORWARDRS_SCREENSHOT_FOLDER` set, `cargo test --test application_screenshots -- --ignored` draws the window in several states to PNG files (tracks, a waveform, text, pictures, turned and resized videos, zoom, the boxes of settings, the menu, a missing file...) |

The tests that use ffmpeg make their own small test videos (each frame carries its own frame number as a strip of
black and white blocks, so a test can read which frame it received), tone files and pictures in
`target/test_fixtures` or a scratch folder. They are skipped with a message when ffmpeg is missing; set
`FASTFORWARDRS_REQUIRE_FFMPEG=1` to make that a failure instead. One test opens the computer's real sound output
and plays silence through it; it is skipped on a computer with none. The text tests are skipped on a computer
with no fonts at all. Nothing has been tried with a real touch screen: the taps are simulated.

## Known limits

* Clips can be moved along their track, but not onto another track, and cut clips cannot be joined again. There
  is no way to trim a clip by dragging its edge (cut it and delete the part instead).
* Undo covers what is edited in the project, not the settings, and not choosing a missing file again. It goes
  back at most 200 steps, and only as far as the program has been open (the history is not saved with the
  project).
* All the videos of a project must run at the same frame rate, and the finished picture is the size of the first
  video.
* Where something is on the picture keeps its rectangle (before it is turned) inside the picture; what is turned
  can stick out at the corners, and is cut off there. A corner of something that is turned so far that the corner
  is outside the preview cannot be grabbed (turn it back, or move it). A video or picture thinner than a fiftieth
  of the picture is drawn a fiftieth wide.
* A picture file with clear places is drawn with them clear, but a picture never hides what is behind it, so what
  is behind a picture that covers the whole picture is still decoded and exported. A photograph that is turned
  on its side by a note in the file (EXIF) is shown the way ffmpeg shows it, which is not tested.
* A GIF file is read as a video, as ffprobe says it is one: an animated one plays at its own frame rate (which the
  project then has to share) and a still one lasts a single frame. It is not offered with the picture files.
* A file that is chosen again in place of a missing one must be the same kind of file; if its shape is another,
  the picture is stretched into the place of the old one.
* A text object has one font, size and colour for all of its words, and is drawn from the top left of its area,
  wrapped to its width and cut off at its bottom edge.
* Speeding sound up with *Maintain pitch* does not keep very short clips (under a tenth of a second of sped-up
  sound), and *Increase pitch* is silent beyond 1000x.
* Autosave needs the project to have a file already.
* A variable-frame-rate source is exported at a constant frame rate (each picture slot gets the source picture
  nearest in time), and its preview can be a frame off at the start of a clip.
* The speed sliders offer 1x to 1000x; a project file may hold a larger factor and the editor will keep it.
* Waveforms are made for sound *files*; a video's own sound is not drawn.
