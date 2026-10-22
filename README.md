# fastforwardrs

A small video editor with one main job: **fast-forward the parts of a video you choose.** Open a video, cut it into clips, make any clip play 2x, 4x, 10x... faster, and export the result. It also mixes several tracks of videos, sound files, pictures and text, with keyframes, effects, groups and sound plugins.

## Install and run

**You need**

* **ffmpeg and ffprobe, version 9 or newer** (the GPL build). An older one is refused with a message that lists where it looked: ffmpeg 8 shifts the pictures of a file whose sound starts before its picture, so every frame would be off by one. They are looked for in the folder named by `FASTFORWARDRS_FFMPEG_DIRECTORY`, then the folder of the program, then the `PATH`. If they are missing the window still opens and says so.
* **Rust 1.88 or newer** to build it (edition 2024).
* **`curl`**, only for the *Bundles* tab (below).
* **A graphics adapter** for the preview and the export (a software one works, slowly: see the export command below).
* **On Linux**, the window system libraries that `eframe`/`wgpu` need, and ALSA's development files (`libasound2-dev` on Debian and Ubuntu) for the sound.

Tested on Windows 10 and Ubuntu 26.04 (WSL 2). macOS is built and tested by the CI only, and nobody has looked at the window on a Mac.

```text
cargo build --release --bins                          # builds the editor, the export program and the plugin scan program together
cargo run --release                                   # an empty window
cargo run --release -- "C:\path\to\project.ffrs"      # open a project (.ffrs or .json)
cargo run --release --bin fastforwardrs-export -- "project.ffrs" "video.mp4"   # export with no window
```

**Exporting without a window:** `fastforwardrs-export [--adapter best|software] [--settings <file>] [--allow-plugins] [--plugin-approvals <file>] <project> <video>` makes the video the way *File > Export* does and reports progress on the error stream. `--adapter software` is for a computer with no graphics card. The exit status is 0 (made), 1 (failed, with the reason) or 2 (the command line was not understood). It cannot be stopped cleanly with Ctrl+C: ffmpeg stops and a half-written `name.exporting.ext` is left (the window's Cancel removes it). For plugins, see below.

**Environment variables**

| Variable | Does |
|---|---|
| `FASTFORWARDRS_FFMPEG_DIRECTORY` | where to look for ffmpeg and ffprobe first |
| `FASTFORWARDRS_PREVIEW=old` | the preview is drawn by the window itself instead of the picture engine (it also takes over if the engine stops) |
| `FASTFORWARDRS_SOUND=ffmpeg` | the old filter-graph sound instead of the sound engine, for the preview and the export |
| `LADSPA_PATH`, `CLAP_PATH` | more folders to look for sound plugins in |
| `FFRS_BUNDLED_PLUGINS` | the folder of the plugins that came with the program, if it is not a folder called `plugins` beside it |

## Getting started

1. *File > New project* asks for a video or sound file. The first video sets the size of the finished picture and the frame rate of the project (*Edit > Project settings* changes the rate).
2. **Space** plays and pauses; click or drag in the ruler, or use **Left/Right**, to move the playhead.
3. **S** cuts the selected clips at the playhead (**Shift+S** cuts every clip under it). There is no trimming by dragging an edge: cut, then delete the part you do not want (select it, press **Delete**).
4. Select a clip: its properties (speed 1x to 1000x, gain, mute) are at the top left. A sped-up clip shows a fast-forward symbol with its speed in the preview and in the exported video.
5. *File > Embed and import* or *Link and import* adds another video, sound or picture at the playhead; right-click an empty part of a track for a text object.
6. *File > Export* writes an `.mp4`, `.mkv` or `.mov`.

Save as a **`.ffrs`** file if you want the project and its media to be one file (see *Project files* below).

## Keys and mouse that are not on the screen

| | |
|---|---|
| Space | play / pause |
| Left / Right | one frame back / forward (repeats when held) |
| Ctrl+Left / Ctrl+Right | the nearest place before / after the playhead where a clip begins or ends (of the selected clips, or of any clip) |
| S / Shift+S | cut the selected clips at the playhead / every clip at the playhead |
| Delete | delete the selected clips or keyframes (a deleted video or sound clip closes the gap if clips touch its end); a selected group is taken apart, what was in it stays |
| Ctrl+Z, Ctrl+Y (or Ctrl+Shift+Z) | undo, redo |
| Esc | select nothing |
| F | over the preview: fit the picture; elsewhere: scroll and zoom the timeline to what is selected |
| Ctrl + wheel | zoom the timeline about the pointer (over the preview: zoom the preview) |
| Ctrl + Alt + wheel | make every track taller or shorter together |
| Alt + wheel | scroll the tracks up and down |
| mouse Back / Forward buttons | back and forward through the views of the timeline |
| Shift + click on a drawer's button | open the drawer over the tracks below instead of pushing them down |
| right-click | clips, tracks, groups, keyframes and the timeline's empty places have menus: duplicate, include a file, make a group, delete, colour space of a file... |

None of the keys works while a text field or a menu is open.

## How it works, where it differs from what you might expect

**Tracks and clips**

* Tracks higher in the list are in front. Any kind of clip can be on any track; two clips never share a time on a track.
* **Speeding a clip up makes it shorter**, and the clips that *touch* its end move along so no gap opens or closes. A gap, a picture or a text object stops that: **pictures and text objects never move, and never move others.**
* A clip's speed is its own speed times its track's; that product is what the fast-forward symbol shows.
* Cut clips cannot be joined again. A clip dragged to another track is held to the time it began at (it snaps back near it) until you let go; dragging never makes a new track.
* **The first video decides the picture size and, until you choose otherwise, the frame rate.** A video of another rate is made to run at the project's: each frame shows the picture nearest in time, so a 25 fps video in a 30 fps project repeats a picture now and then and keeps its length.
* A layer may reach past the edge of the picture, but its middle stays on the picture and it cannot be made bigger than the picture. A video that fills the picture hides what is behind it; a picture or a text object never counts as hiding anything.
* A video with no picture to show for some frames (the file is missing, or shorter than its clip) is simply not there for them.
* A text object has one font, size and colour for all its words. A GIF is a video.

**Keyframes**

* Every clip and track has a **drawer** (the button on its bottom edge). Keyframes live in **racks** in drawers; a rack animates one property. Hover a control to see its stopwatch, which makes the first keyframe. Between two keyframes a value changes evenly (loudness in decibels, positions as fractions of the picture, angles in degrees; a turn counts whole turns).
* Changing a clip's length or speed, its track's speed or the frame rate moves its keyframes to the nearest frames and uses up the empty space first. **If there are more keyframes than frames, the program asks first** (*Lose 3 keyframes?*); shortening loses the latest, speeding up loses evenly spread ones.
* Speed and the time a picture is shown for cannot be animated, so a clip's length never depends on keyframes. Choices, switches and tables are not animated either.

**Colours**

* Pictures are **mixed as light** (linear light): a half see-through layer, a scaled or turned picture and soft text edges blend as light does, so a white line thinner than a pixel is brighter, not darker. A picture shown at its own size, opaque, keeps its numbers exactly.
* Each file's colours are read from what the file says. A video that says nothing is BT.709 above 1024x576 and BT.601 below; the curve is read as sRGB. **High dynamic range (PQ, HLG) and camera logs are not converted** (read as sRGB, with a note). Right-click a clip and choose *Colour space of this file...* to say it by hand.
* The preview follows the *Display* setting; the export follows *Colour space of exported videos* and always tags the file with it. The computer's colour profile is not read.

**Effects and groups**

* Eight effects: hue shift, saturation, brightness and contrast, exposure, opacity, blur, crop and a look-up table (`.cube`). At most 64 on one clip, track or group. Hue shift, saturation and brightness/contrast work on the numbers of the screen; exposure, opacity, blur and crop work in linear light.
* **Shaders:** an effect can be an ISF file (`.fs` or `.isf`), added with *Add effect from file...* like a `.cube` table. Its text is kept in the project, so the project runs it anywhere, and there is no question to ask about it (a shader is data, not code). **Filters of one picture and generators only so far** (no passes of their own, no images of their own); transitions are to come. A **generator** is a shader that reads no picture: *New*, then *Generator from file...*, on an empty part of a track, makes a **generator clip** that it makes the picture of (5 seconds, and it can be moved, resized, turned, given effects and animated like a picture); what it makes is its first effect, which cannot be taken off. There is no built-in generator. It works on the numbers of the screen, `TIME` is the time since its clip began, and a point is in points of a picture 1080 high. Nothing is shipped in the box.
* A clip's effects are done first, then its track's, then its groups'. Drag a box by its heading to reorder.
* **Groups** (tags, clip groups, track groups) are outlines on the timeline. An **adjustment clip** (right-click an empty part of a track, *New*, *Adjustment clip*) does its effects to everything behind it for as long as it lasts. **The effects of a clip group or a track group are done to the picture that its members make together** (a blur of a group is a blur of the whole; a group that has no effect that does anything is not a layer of its own). The effects of a tag are done to each member. The old drawing of the window (the fallback of a computer with no usable graphics adapter) leaves a group's effects out, as it leaves out a blur. Groups do nothing to sound.
* The timeline's thumbnails and waveforms show the files as they are, without effects.

**Project files**

* **`.ffrs`** is a zip that can hold the media; **`.json`** is plain text that cannot. *Link and import* leaves a file where it is; *Embed and import* and *Include in project file* put it inside the `.ffrs`. An included file is read in place: opening is instant however large it is. Including copies the file (a copy over 8 MiB runs in the background with a progress window); it **is not an undoable step**, nor is taking a file out again.
* A project with included files cannot be saved as plain `.json`. Bytes of a file taken out stay in the `.ffrs` as garbage until it is rewritten.
* A file that cannot be found is listed in a warning when the project opens; nothing is discarded. Choose it again from the warning or from the clip's properties (the replacement must be the same kind of file).
* **Undo** goes back 200 steps and only as far as the program has been open. Settings, choosing a missing file again, selection, open drawers and zoom are not undoable.
* Autosave (off by default) saves two seconds after the last change, only for a project that already has a file; with it on, closing and loading do not ask about unsaved changes.
* Older project files are read as they were written.

**Sound**

* The preview plays the same sound the export makes: every clip's sound, cut, sped up with the clip, mixed with the other tracks'. It starts a fraction of a second (about 0.07 s measured) after Play. A video's own sound has no waveform on the timeline.
* *Sound of sped-up clips* (Settings): mute, maintain pitch (default) or increase pitch. Sped-up sound shorter than a tenth of a second is silent; *increase pitch* is silent beyond 1000x.
* Cutting makes a short snip sound and a white flash on the cut track; drawers make a wooden sound as they slide. They have no setting to turn them off.

**Exporting**

* The export contains exactly the frames the project describes at a constant frame rate (a variable-frame-rate source is conformed; empty time is black). The picture is no bigger than the first video's, with an even number of pixels. H.264 and AAC (`mpeg4` if your ffmpeg has no libx264); `.mp4` and `.mov` are written for streaming.
* The sound is made first into a temporary file (about 23 MB a minute), with no progress shown, before the picture begins to encode.
* The video is written to `name.exporting.ext` and renamed when complete, so a cancelled or failed export never damages an existing file; exporting over a source file is refused.
* A file with fewer pictures than the project was told is held on its last picture, and the message at the end says how many frames were held. An effect the program does not have is listed before the export asks whether to go on.
* A frame made of more pictures than the engine's 512 MiB of picture memory ends the export with a message. The export needs a graphics adapter.
* While a project plays and the card cannot keep up, the preview is drawn smaller instead of late. Its zoom shows pictures made at no more than 1280x720, not the exported ones.

## Sound plugins

CLAP and LADSPA plugins with an audio input and output can be effects on a clip, a track or a tag (the **+** button in the properties panel, under *CLAP* and *LADSPA*), keyframed like any value. There is no window of a plugin: its parameters are what the project keeps.

* Trust is asked **once for each folder** of plugins and remembered (a plugin inside a project file is asked about every time the project is opened). A plugin that is not trusted is not run: the sound goes through unchanged and the project keeps its settings.
* **The command line cannot ask:** `fastforwardrs-export` refuses a project that needs a plugin from a folder nobody has approved, and says which, unless it is given `--allow-plugins`. `--plugin-approvals <file>` reads approvals from another file.
* An effect whose plugin does not run (folder not trusted, not installed, failed to load, too slow) is a **placeholder that says why**, and keeps its settings.
* Plugins run inside the editor: **one that crashes takes the editor with it**, and the next start says which library it was. A plugin too slow for two seconds of sound is left out of the preview (never the export).
* They are found by a separate **scan program** (`fastforwardrs-plugin-scan`), never by the editor, in the folders you add under *Edit > Sound plugins...*, in `LADSPA_PATH`, `CLAP_PATH` and the usual places of each system. After rebuilding, **build everything again** (`cargo build --release --bins`): the editor refuses a scan program built from other code.
* The scan also checks whether a CLAP plugin's sound at its start depends on memory it never wrote (a bug some plugins have: a roar, a warble or silence at random). The window and a note while the sound plays say so; it cannot be fixed from here.
* **The *Bundles* tab** of *Edit > Sound plugins...* downloads six free reverbs (Dragonfly Hall, Plate and Room, MVerb, MaGigaverb and Surge XT Effects; about 110 MB on Linux, 145 MB on Windows, 270 MB on macOS) from their authors' release pages, after a question, and checks every file against a checksum that is kept in the program. They are kept in `fastforwardrs/plugins` in your data folder (`%LOCALAPPDATA%` on Windows) and are trusted without the folder question. It needs the `curl` program (Windows 10 and later, macOS and most Linux systems have it) and has builds for Windows, Linux and macOS (the macOS ones are universal; the CI has fetched and scanned them on a Mac, and nobody has heard them there). A folder called `plugins` beside the program is trusted the same way (`py tools/bundled_plugins/fetch_bundled_plugins.py --out <folder>/plugins` makes one, for a release).

## Licence

GNU General Public License, version 3 or later: [`LICENSE`](LICENSE). Plugins that work with the program only through a published plugin interface are separate works, as [`LICENSE-PLUGIN-CLARIFICATION`](LICENSE-PLUGIN-CLARIFICATION) says. It runs `ffmpeg` and `ffprobe` as separate programs. Licences of what it is built from: [`THIRD_PARTY.md`](THIRD_PARTY.md); what a release must carry: [`docs/distribution-notices.md`](docs/distribution-notices.md).

## More

| For | Read |
|---|---|
| every control and the rules behind it | [`docs/using-the-editor-in-detail.md`](docs/using-the-editor-in-detail.md) |
| settings, colours, the exported video | [`docs/settings-colours-and-export.md`](docs/settings-colours-and-export.md) |
| the project file's layout, with an example | [`docs/project-file-format.md`](docs/project-file-format.md) |
| plugins: trust, scanning, how they run, bundling | [`docs/plugins-and-trust.md`](docs/plugins-and-trust.md) |
| building, layers, tests, known limits | [`docs/developer-readme.md`](docs/developer-readme.md) |
| every other document | [`docs/README.md`](docs/README.md) |
