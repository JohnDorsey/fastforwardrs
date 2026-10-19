# fastforwardrs

A small video editor with one main job: **fast-forward the parts of a video you choose.**

Open a video, cut it into clips, and make any clip play 2x, 4x, 10x... faster; export the result. Fast-forwarded
clips show a fast-forward symbol with their speed (like ▶▶ 4x), in the live preview and in the exported video.
The preview plays the sound too. A project can have several **tracks**, and on them videos, sound files,
**pictures** and **text objects**, which are put together in front-to-back order: a video or a picture (or a
text) can be made smaller, moved (even partly past the edge of the picture) and **turned** on the picture, so
that what is behind it shows. The window is divided into **panels**: the timeline along the bottom, the preview at
the top right and, at the top left, the **properties** of whatever is selected. Most properties (how loud a clip is,
where it is on the picture, how big and how far turned, what a text says and how it looks) can be **animated** with
**keyframes**, which are kept in **racks** in the **drawers** of clips and tracks, where they can be seen and moved on the
timeline. A clip or a track can have **effects** (a **hue shift**, **saturation**, **brightness and contrast**, **exposure**,
**opacity**, a **blur**, a **crop** and a **look-up table** from a `.cube` file), which are put on with a **+** button
in the properties panel and are animated with keyframes like any other
property. Clips, tracks and other groups can be put together in **groups** (**tags**, **clip groups** and **track groups**),
which are drawn on the timeline as outlines in a colour of their own and can have effects too. The preview can be **zoomed**,
in until the pixels of the picture can be seen. Every edit can be undone. **Pictures are put together in linear light**, as light is mixed (a layer that is half see-through,
turned or made smaller takes its share of the light of what is behind it), and **what the colours of each file are** is read from what the file says of them and can be said
by hand (see *Colours*).

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

## Using it

**The menus** are File and Edit.

* **File**, in this order: *New project*, *Save project*, *Save project as*, *Load project*, *Embed and import*, *Link and import*, *Export*.
  * *New project* asks for a video or sound file straight away.
  * *Save project as* asks where to save the project even if it has a file already: the window opens in the folder of
    that file with its name offered, and the file chosen is the project's from then on (the file it came from is left as
    it was). Nothing happens if no place is chosen. (*Save project* asks only when the project has no file yet.)
  * A project is saved as a **`.ffrs` file**, which is what is offered first and what a name with no ending gets, or as a
    plain **`.json`** file (the second kind in the list; see *Project files*). *Load project* opens either. A `.ffrs` file
    can have files **included** in it (see *include a file in the project file* below); a `.json` file cannot.
  * *Embed and import* and *Link and import* each ask for a video, a sound file or a **picture** (PNG, JPEG, BMP, WebP or
    TIFF) and put **all of it on the timeline at the current frame**: on the front-most track that has nothing on it, or on a
    new track in front of all the others. A picture is shown for five seconds (change that in its properties, see below).
    *Link and import* leaves the file where it is: the project remembers the place, and the file is not in the project file.
    *Embed and import* also **puts the file in the project file** (as *Include in project file* does, see below), so that
    the project and its file are one file, which can be taken to another computer. Embedding needs the project's file to be a
    `.ffrs` file: if it is not one (it has none yet, or it is a plain `.json` one) a window says so *before any file is asked
    for*, and *Choose where to save it* asks where to save the project as a `.ffrs` file, saves it there, and then asks
    for the file to embed; *Cancel* (or no place chosen) changes nothing. If the file cannot be put in the project file
    after all (the project file cannot be written), it is on the timeline all the same, linked, and a message says why.
    *New project* asks for a file by *Link and import*.
  * *Export* asks where to save the finished video (`.mp4`, `.mkv` or `.mov`) and shows its progress, with a
    Cancel button. If the project has effects that the program does not have (a project file brought them), it first says
    which, because the video would not have what they do, and asks whether to export anyway (*Export anyway* or *Cancel*).
  * Loading, starting a new project and closing the window ask first if there are changes that have not been
    saved, unless Autosave is on (see *Settings*).
* **Edit**: *Undo* and *Redo*, *Deselect* (with its key, **Esc**, shown beside it; it can be used while a clip is
  selected), *Project settings* and *Global settings*, which open little windows (see *Settings*).

**The window** is divided into three **panels**, each in a lighter grey than the space between them (which is 6
points wide, and goes round every panel too): the **properties panel** at the top left, the **preview** at the top
right, and along the bottom the **timeline panel**, which has, on one line along its top, the Stop / Play / Pause buttons, the
current time next to them (`hh:mm:ss.ff`, where `ff` is the frame within that second) and, filling the rest of the line,
a slider through the whole video, and under that the timeline (a round **+** button, a ruler, and a row for every
track). The properties panel shows the
properties of what is selected, in a box headed with what it is: *Video clip*, *Sound clip*, *Picture clip*, *Text
object* or *Track* (or, for a keyframe or a rack of keyframes, what it is of and when) and asks you to select something
when nothing is. Under the box of a clip that is seen, or of a track, or of a group, are the boxes of its **effects**, with
the **badges** of the groups and the track it takes effects from among them (see *Groups*), and a round **+**
button that puts another effect on. A selected **group** has a box with a thick outline in its colour (see *Groups*). When several things are selected (clips, tracks, keyframes and racks of keyframes, in any
mixture) it lists them inside a yellow outline instead, and has nothing to change: click an entry of the list to select
just that one.

| To... | Do this |
|---|---|
| play or pause | press **Space** (or the buttons) |
| move the playhead | **click or drag in the ruler** (the strip with the marks, where the red triangle sits); or press **Left** / **Right** to move it one frame back or forward, paused or playing (holding the key repeats); or press **Ctrl+Left** / **Ctrl+Right** to take it to the nearest place before or after it where a clip begins or ends (of the selected clips if some are selected, and otherwise of any clip on any track; the last place is the last frame); or drag the slider that is on the line of the buttons, right of the current time |
| undo and redo | **Ctrl+Z** undoes the latest edit; **Ctrl+Y** (or **Ctrl+Shift+Z**) does it again; or use the Edit menu. Everything that changes the project is an edit: cutting, deleting, moving, speeds, volumes, tracks (their titles, settings, heights, adding and deleting them), text, effects (putting them on, their values, taking them off, their order), groups (making them, their names and colours, what is in them, their effects, taking them apart), where things are on the picture, importing a file and the fast-forward symbol. Dragging a clip or a slider, or typing a title, is one edit however long it goes on. **Not** edits: the settings of the program and of the project, choosing a missing file again, which drawers are open, which things are selected, and how the preview is zoomed. |
| cut | press **S** to cut the selected clips that are at the playhead there (nothing is cut when none is selected there), or **Shift+S** to cut every clip under the playhead, on every track, selected or not. A cut makes a short **snip** sound (once, however many clips were cut; nothing if nothing was cut) and a quick **flash of white light** at the playhead on each track that was cut, which is at its brightest and biggest the moment it appears and fades away in about a third of a second (0.3 s) |
| select clips | **click** a clip; **Ctrl+click** adds a clip to the selection or takes it away; or **drag a rectangle** from any empty place on the timeline (also from the gaps between tracks) to select every clip it touches, on any track (hold Ctrl to keep what was selected). Clicking an empty part of a track, pressing **Esc** or choosing *Edit > Deselect* selects nothing. Selected clips have a bright outline. |
| select a track | **click its box** (the one at the left of the track, with its title in it): the track is selected, with a bright outline round its box, and its properties are in the properties panel. Selecting a track does not select what is on it, and the keys that cut and delete are about clips only. **Ctrl+click** adds a track to the selection or takes it away. (Clicking the title itself also lets you type in it, which makes the keys the title's.) |
| delete clips | select them and press **Delete**, or **right-click** one of them and choose *Delete clip* (*Delete 3 clips* when three are selected). The same key takes away the selected **keyframes** (see below), and **takes a selected group apart** (what was in it stays: see *Groups*). The clips that touch the end of a deleted video or sound clip move up to close the gap; a gap, a picture or a text object stops that. |
| move clips | **drag** a clip along its track, or **onto another track** (any kind of clip can be on any track): it follows the pointer up and down the rows as well as along them. It cannot go onto another clip (it stops against it) or before the start of the timeline, and a track where it would not fit is not gone onto. **A clip that is dragged into the rectangle of a clip group joins the group** while it is in it (the group is drawn round it as it goes), and is out of it again if it is dragged out before it is let go; where it is let go in the rectangle it is one of the clips of the group, until it is taken out of the group. It stops where being in the group would make the rectangle take in a clip that is not in it, and a clip that is in another clip group does not go into this one. If several are selected they all move together, by the same number of tracks and frames (and stop together). A clip keeps its settings and its place on the picture when it changes track, and lasts as long on the timeline as the speed of the track it is on makes it. Dragging never makes a new track. **A clip dragged to another track is held to the time it started at**, as a magnet holds iron: it can be moved left and right, but when it gets within a few points of that time it snaps back to it, until the button is let go (after that it is wherever it was put). A clip that was dragged a good way along its own track before it was taken across (more than 96 points) has no such time, and a clip that has been on another track is held to its start time on every track, its first included. |
| copy a clip | **right-click** it and choose *Duplicate to the right* or *Duplicate to the left*: a copy is put **right beside it** (it starts where the clip ends, or ends where the clip starts), with a copy of every rack in the clip's drawer, and the copy is selected. There has to be room for it: when there is not (something else is there, or the copy would have to begin before the start of the timeline) the item is greyed out, and says why when the pointer is over it. With several clips selected the menu is about all of them (*Duplicate 3 clips to the right*): each is copied beside itself, and the ones that have no room are left out (the item says so when only some of them have none). What the clip's racks are of is kept, except that what animated the clip animates its copy: a clip that lowers the sound of a big video lowers it again where its copy is. A rack of the clip that is kept in the drawer of another clip is not copied with it. A crate is copied the same way. |
| **include a file in the project file** | **right-click a clip** that is made from a file (a video, a sound or a picture; not a text object) and choose *Include in project file* (*Include 3 files in project file* when the clips that the menu is about are made from three files: right-clicking one of the selected clips is about all of them). The file is then **kept in the project file** (a `.ffrs` file, see *Project files*), and is put there the next time the project is saved: the project and its files then move together in one file, which can be taken to another computer. The two places the file was at (where it was, and that place from the project file's folder) are remembered. **If the project's file is not a `.ffrs` file** (it has none yet, or it is a plain `.json` one) a window says that the project has to be saved as one to go on, and when you choose *Choose where to save it* the save-as window asks where (a `.ffrs` one only; a name that does not end in `.ffrs` gets it) and the project is saved there **with the file included, as one step**: if that cannot be done nothing is included and the project's file is as it was; *Cancel* changes nothing. A file that cannot be found cannot be included (a message says which). **It is not a step that can be undone**, as choosing a file again is not: it is where a file is kept, and not what is done with it (undoing what came before it leaves the file included). **Loading** a project that has files included **reads each where it is in the project file**: nothing is copied or taken out, so the project opens as soon as ffmpeg has looked at the files, however many gigabytes they are, and they take no room twice (the files are stored, not compressed, so ffmpeg is told to read just that stretch of the archive). Only a file that another zip program compressed cannot be read there; that one is taken out into a folder of the computer's cache (`%LOCALAPPDATA%\fastforwardrs\archive-resources` on Windows; once: a file that was taken out before is used again; the cache is never emptied by the program) in the background, and opened from there. A file that cannot be read is a source file that is missing, as one that cannot be found is. **The file is written into the project file at once, and the project is saved with it** (the other changes that were not saved yet go into the file with it): a file of more than a few megabytes is copied in the background, with a window that says how far it has got and a *Cancel* button, and nothing else can be done meanwhile; *Cancel* leaves the project file and the project as they were. |
| **take a file out of the project file** | **right-click a clip** whose file is included and choose *Remove from project file*. The file is **put back where it was when it was included**: where the same file is already, nothing is written (it is that file again); where another file is, that place is not used; where nothing is, the file is written there, if the folder is. Where it was is tried first, and then that place counted from the folder the project file is in now (for a project that has been moved). If there is no place, the file stays in the project file and a message says so. The project file is written without it the next time it is saved (its bytes stay in the file, as garbage, until the file is written again: see *Project files*). |
| change the order of the tracks | **drag a track's title box** (the box at the left of the track, not the words in it) **up or down**: the whole track follows the pointer, and the other tracks move out of its way, quickly, to where the track was taken from, as it passes over them. When it is let go it is put where it was, between the tracks it is over, and the tracks are as they were in every other way (this is one edit, and can be undone). A drag that is more along than up and down is a click, or a selection of the words, as before. The list scrolls if the track is taken to the top or the bottom of it. |
| open a drawer | every clip and every track has a **drawer**, for racks of keyframes. Move the pointer over a clip and a wide, thin **button** comes out **on its bottom edge** (its bottom is the clip's bottom, and its two bottom corners are square); it is there too while the drawer is open, and always when there is something in the drawer. Click it and the drawer slides out under the clip, as wide as the clip and with its outer corners as round as the clip's, in a fraction of a second; click it again to shut it. A drawer opens by **pushing the tracks that are lower in the list down** to make room for itself, and they come back when it shuts; **click the button with Shift held** and the drawer opens **over** them instead (they stay where they are, and what is under the drawer is hidden). Either way, when a drawer opens the timeline **scrolls down as far as it takes** to show all of it (a drawer on the track at the bottom hangs below what was in view); it never scrolls up. The drawer of a **track** is opened with the button on the bottom edge of the track's box, at the left of it, goes all along the track from the start of the timeline, and hangs from where the open drawers of the track's clips end. Which drawers are open is not saved and is not an edit; a stopwatch that makes a keyframe opens the drawer it is in. |
| change a clip's speed, loudness or mute | **select** the clip (a click on it, on the timeline or on the preview): its properties are in the properties panel, which has a speed slider (1x to 1000x, whole numbers), a gain slider (−30 to +30 dB with no change in the middle) and a *Mute* box, and says which file the clip is from (and, if that cannot be found, offers to choose it again). With several clips selected the panel only lists them. (A right-click on a clip selects it too, unless it is one of several that are selected, and opens its menu: see *copy a clip*.) |
| change how long a picture is shown | select its clip: the properties have *Shown for*, in seconds (the picture is shown for as long as there is room before the next clip on its track). |
| change the settings of a whole track | **select the track** (click its box at the left of it): the properties panel has the track's gain, speed and *Mute*, which apply to everything on the track, on top of each clip's own. The speed of a clip is its own speed times its track's, and that product is what the fast-forward symbol shows. |
| rename a track | type in its **title box** (new tracks are called `track 0`, `track 1`, ...) |
| make a track taller or shorter | drag the **boundary** under its title box. A track can be made as short as its title needs, or half as tall again as a new one. A video on a shorter track is drawn with smaller pictures, taken more often. |
| add a track | press the big round **+** above the list of tracks; the new track goes in front of the others, at the top |
| delete a track | **right-click** its title box (or the box's empty part) and choose *Delete track*: the track and everything on it go (the files stay part of the project). The only track cannot be deleted. |
| copy a track | **right-click** its title box and choose *Duplicate above* (the copy goes in front of the track, which is above it on the screen) or *Duplicate below* (behind it). The copy has everything the track has: its loudness, speed, mute switch and height, a copy of each of its clips where they are, and a copy of every rack in its drawer and in the drawers of its clips. What the track or one of its clips animated, the copy of it animates; what they animated that is not on the track is animated by the copy as well. It is titled like the track with *copy* after it (*track 3 copy*, then *track 3 copy 2*), so that the two can be told apart, and it is selected. |
| make a text object | **right-click an empty part of a track**, choose *New* and then *Text object*. It lasts three seconds (or until the next clip) and is selected, so that its properties are in the properties panel. |
| change a text object | select it: its properties have the words, the font, the size in points (on a picture 1080 tall), the colour, for how long it is shown, and where it is (see below) |
| set where something is on the picture with numbers | a video, a picture or a text object has the rows *Position* (where its middle is, as percentages of the picture across and down), *Size* (how wide and tall it is, in percentages of the picture; *Box size* for a text object, whose letters have a size of their own) and *Rotation* (in degrees, clockwise), which show where it is at the playhead, and can be dragged or typed in. It is the same as dragging it on the preview. |
| **animate** a property | **move the pointer over a control** whose property can be animated (how loud a clip or a track is, whether it is muted, where a video, picture or text object is, how big, how far turned, what a text says, its font, the size and the colour of its letters): three little buttons come to the right of it, an **arrow to the left**, a **stopwatch** and an **arrow to the right**. The stopwatch is **grey** when the property has no keyframe at the playhead and **bright orange** when it has: click it to make one (with what the property is there) or, when it is orange, to take it away. The arrows are orange when the property has a keyframe before (or after) the playhead, however far off, and grey when it has not: click one to go to that keyframe. Once a property has keyframes the control shows what it is **at the playhead**, and changing it there (with the control, or by dragging the thing on the preview) makes or changes the keyframe at the playhead; the keyframes at other frames are as they were. **A change of something that has keyframes stops the video if it is playing** (the keyframe is made where it stopped; something that has no keyframes can be changed while the video plays), and a drag, which is one change that goes on, **makes one keyframe**, at the frame the playhead was at when it began, whatever moves the playhead while it goes on (a key that is held, say): it never writes the movement of the hand down as a keyframe at every frame it goes over. What has no keyframes is changed for the whole clip, as it always was. A control of a property that has keyframes cannot be changed where the playhead is not on the clip (there is nowhere to put a keyframe), and is dimmed there. The speed of a clip or a track and for how long something is shown cannot be animated (that would move the keyframes about). The keyframe is made as a **diamond** on a **rack** in the drawer of the clip or the track, which opens to show it (see *racks* below). |
| put an **effect** on a clip, a track or a group | **select** a video, a picture or a text object, or a track, or a group: under its box in the properties panel is a round **+** button (a sound has none, and neither does a keyframe crate: they have no picture to change). Click it and choose the effect from the menu: it has a **field to search** the effects (it has the keyboard when the menu opens; type a part of a name, of what an effect does or of its heading, and **Enter** adds the first that is left) and lists the effects under the headings of their categories (*Colour*, *Light*, *Blur and sharpen*, *Transform*, *Looks*); at the bottom is **Add effect from file...**, which asks for a table of colours (see *Look-up tables*). The effect gets a **box of its own** under the box of the clip, track or group, headed with its name, with the controls of its values (a slider for a number, a list for a choice, a checkbox for a switch), which are animated with the stopwatch like any other property (a choice, a switch and a table are not animated: they hold until they are changed). **Right-click the heading of an effect's box** and choose *Delete effect*: the effect is deleted with the keyframes of its values, in one edit (undo puts both back). An effect can be put on again and again: a second hue shift is an effect of its own, with values of its own, and when there are several of one kind their boxes say which is which (*Hue shift 1*, *Hue shift 2*, counting from the top; one that is alone has no number). **Right-click the heading** also offers *Duplicate effect* (a copy with the same values, put after it; keyframes are not copied) and *Switch off* (and then *Switch on*): an effect that is switched off stays in the chain with its values and keyframes, shows a note that says so, and does nothing to the picture, the preview or the export; it is one edit that undo takes back. (An effect that a project file brought and that the program does not have is a box too, with the name the file gave it, a note that says *Not installed*, and a control for each of its values: it does nothing to the picture, and its values are kept, and saved again as they were. The values of the effects of a group are not animated: they have no stopwatch.) |
| change the order of the effects and badges of a chain | **drag the box of an effect by its heading** (it has grip dots at the top), or **a badge** by any part of it: a line shows where it will go, and it goes there when it is let go of. The effects of the chain are done in the order they are in, from the top to the bottom (see *Effects* and *Groups*). One step of the history. |
| select what a badge, or an item of the list of a group, stands for | **double-click** it. (A click does not: so that a badge can be taken hold of to be moved without selecting what it stands for. The list that is shown when *several* things are selected keeps its single click.) |
| make a **group** | **select two or more clips** and **right-click** one of them: *Create tag* (one clip will do) makes a **tag** of them, wherever they are, and *Create clip group* makes a **clip group** of them, which they have to fit in a rectangle of the timeline that has no other clip in it, or the program says which clip is in the way. A track group is made from the menu of the **box of a track**: *Create track group* makes one of the tracks that are selected (or of the track that was right-clicked, if it is not one of them), which have to be next to each other. The group that is made is **the only thing selected** afterwards. (A group can be selected together with clips, and made one of the members of another group that way.) |
| add clips to a tag | **right-click** a clip (or the selected clips) and choose *Add to tag*, and the tag from the list. (The clip is put last in the tag. Clip groups and track groups are not added to after they are made.) |
| change a group | **select it** (it is selected when it is made; or double-click its badge, or its name in the list of the group it is in): the box at the top left has a **bar** in the colour of the group with its kind, its **name** (type in it) and a **colour** button; under it the list of what is in the group. |
| select a group on the timeline, move it | **put the pointer over a clip group or a tag** (over any clip of it, or the empty parts of its outline) and its **name** shows in a bar in the colour of the group, over the middle of its top edge (for a tag, over the top edge that has the most in view): a tab that is as wide as the name and its dots need, flat where it is on the outline, and going out beyond the sides of a group that is narrower than that. **Click the bar** to select the group (**Ctrl+click** adds it to what is selected or takes it away). **Drag the bar** to move the group and everything in it along the timeline, and down and up to other tracks as many as the pointer goes where there is room for the clips there; it stops against the clips in its way as clips do. Over groups one in another it is the **innermost** group that shows its name, and the name of the group round it is shown when the pointer is over that group and not over one in it. A track group has no name on the timeline. |
| make a clip group wider or narrower | **put the pointer on the left or the right edge of a clip group** (the pointer becomes a double arrow, left and right) and **drag the edge**. The group is **the clips and the clip groups that are inside the frames the edge is let go at**, on the tracks it reaches across: moving an edge out over clips takes them in, and moving it in past clips lets them go (and the rectangle is always the smallest that has what is in the group, so an edge that is let go where there is nothing to take in goes back to fit what is in it). The edge goes along with the pointer where it can be put, and **cannot be put across a clip or a clip group** (part of it inside and part outside), or where the rules of groups would not have it (a clip that is in another clip group, say, or a clip that would make the rectangle take in a clip that is not in the group): over such a place it goes, quickly, to the nearest place where it can be put, and is held there. Nothing is changed until the button is let go, and then it is one edit that can be undone. A group cannot be left with nothing in it. |
| take something out of a group, take a group apart, delete it with what is in it | **right-click an item of the list of a group** and choose *Remove from group* (a clip group cannot lose a clip that would then be inside its rectangle without being in it: only the clips at the edge of the rectangle can be taken out, and a message says so otherwise). **Right-click the bar** of the group: *Disband* deletes the group and keeps everything that was in it, and *Delete all* deletes the group and everything in it, with its clips and tracks (one edit that can be undone, refused when it would delete every track). The **Delete** key takes a selected group apart. |
| select keyframes | **click** a diamond (**Ctrl+click** adds it to what is selected or takes it away), or draw a rectangle round some: the rectangle takes in the diamonds it touches as well as the clips. A selected keyframe has a bright halo. Keyframes can be selected together with clips (anything that has a time), and the properties panel then lists them. A single selected keyframe has a box in the properties panel that says what it is a keyframe of and when. |
| move keyframes | **drag** a diamond, or anything that is selected with it: all of it, clips, keyframes and racks (with their keyframes), goes along the timeline by the same number of frames. A keyframe goes as far as its drawer has frames for (a rack is as long as its drawer: a keyframe cannot be taken out of the clip that has the drawer), and stops short of a keyframe of its rack that is not going with it. A keyframe in the drawer of a clip that is dragged goes with the clip, and keeps its place in it. **Delete** takes away the selected keyframes. |
| move keyframes to other racks | **drag a diamond up or down** onto another rack of **the same property of the same thing** (the racks it can go to are the ones it could be in): it goes into the rack the pointer is over, if it fits there where it is along the timeline, and otherwise stays where it is. It works like dragging a clip to another track: a group of keyframes (selected together, in one rack or in several) goes the same number of racks down or up, in the racks each of them can go to, and the same number of frames, so that they keep their places among one another; if they cannot all fit they go less far until they do, or none moves (and when a rack is selected with them they do not change racks at all: a rack goes whole). **A keyframe dragged to another rack is held to the frame it started at**: it can be moved left and right, but when it gets within a few points (24) of that frame it snaps back to it, until the button is let go. |
| copy keyframes | **right-click** a diamond and choose *Duplicate to the right* or *Duplicate to the left* (it is *Duplicate 3 keyframes to the right* when three are selected): copies are made **in the same racks**, a whole group as a group, **next to the keyframes copied** (as far from them as they are wide from the first to the last, and then a gap of the width of a diamond at the zoom the timeline is at, so that the copies are not on top of the diamonds they are copies of). The copies are selected. Nothing is copied unless **all** the copies have room: in a drawer that has the frames for them, and not on a keyframe of the rack that is there (the item is greyed out, and says why when the pointer is over it, when they do not). |
| select racks | **click** a rack (**Ctrl+click** adds a rack to what is selected or takes it away): a selected rack has a bright outline. Racks can be selected on their own, several at once, and **together with clips and keyframes**; the properties panel lists them all, and a single selected rack has a box that says what its keyframes animate and how many there are. **Delete** takes away the selected racks with their keyframes. (A rack is not selected by a rectangle drawn on the timeline.) |
| merge racks | a drawer can have **several racks of the same property of the same thing**: a rack dragged into a drawer that has one of that property is put in beside it and they are **not** merged. To merge them, select them (two or more, all of the same property of the same thing), **right-click** one and choose *Merge racks*: the keyframes of all of them are put in **one rack**, and the property is what it was at every frame. The rack that is kept is the one in the first drawer (in the order of the timeline) that has room for every keyframe at the frame it is at; a keyframe is never moved to another frame, so the item is greyed out, with the reason, when no drawer has room for all. Where two keyframes are at one frame, the one that counted is kept. |
| move a rack | **drag a rack** (it has six grip dots at its left end). Sideways it takes its keyframes along the timeline, all together. **Up or down**, over a drawer that is open, it goes into that drawer (of a clip or a track): that is how keyframes are kept with the clip they have to go with, even when they animate another. Over the **empty part of a track** it goes into a **keyframe crate** that is made there, as long as its keyframes need (from the first to the last, where they are put down, as far as the empty stretch and the end of the video let them be). A rack is as long as the drawer it is in and never any taller, and it cannot go where its first and last keyframe do not have room to stay as far apart as they are: over such a place the words *not enough space here* are written by the pointer. **Dragged up or down inside a drawer** (or into another one) a rack **changes places with the rack that the pointer goes past**, as it is dragged (in its own drawer as in another), so that it is put where it is let go, among the racks; and the order of the racks of a drawer is the order they are drawn in and, where two keyframes of one property are at the same frame, the later rack counts. A drawer can have several racks of one property of one thing (they are not merged: see *merge racks*). **Racks that are selected together are dragged together**: take hold of any of them and they all go along the timeline by the same number of frames, each whole and in its own drawer, and no further than the one that has the least room can go (so they stay where they are among one another). With **clips** or **keyframes** selected too, all of it goes together, the racks with their keyframes, and no further than whatever is stopped first can go; a rack in the drawer of a clip that goes is carried by the clip and keeps its place in it. A rack that is **alone**, or **several racks of one drawer with nothing else selected**, is taken to other drawers, places and tracks: several together go into another drawer as a block, in the order they were in, and **onto the empty part of a track they make one new crate that holds all of them**; in their own drawer they go along the timeline only, and do not change places in it. Racks that are dragged with other things (clips, keyframes, or racks of other drawers) go along the timeline only, and nothing that is dragged with them is taken to another rack or track. A rack that is not selected is selected by being taken hold of, and dragged alone (**with Ctrl held** it is added to what is selected, and dragged with it). |
| see what a rack is for | **hover over a rack** (or one of its diamonds): a label comes to the **left of the rack** that says what its keyframes animate: the property (*Gain*) and, when the rack is not in the drawer of what it animates, what that is (*Gain of holiday.mp4*, *Muted of track 2*). |
| pick, move, resize and turn something on the preview | **click** a video, a picture or a text object on the preview to select it (or select its clip on the timeline): it shows an outline and a square at each of its four corners. **Drag it** to move it, **past the edge of the picture** if you like (it stops when its middle is at the edge, so that it can always be found again), and with **Shift** held it is **held to the edges of the picture**: when an edge of it (the part of it that sticks out furthest, if it is turned) comes within a few points (12 on the screen) of the edge of the picture on that side, it is moved to be exactly at it, and stays there until the pointer has gone further than that; drag a **corner** to resize it from that corner, out past the edge as well (a video or a picture keeps its shape, text takes whatever shape you drag it to, and nothing can be made bigger than the picture); drag from **just outside a corner** (a faint ring appears) to **turn** it about its middle, clockwise or not, with its angle shown beside the pointer, and with **Shift** held to turn it by 15 degrees at a time. A video that has not been touched fills the picture (or as much of it as its shape lets it). What is past the edge of the picture is not seen, and is cut off in the export, as what is turned and sticks out is. **The outline and the corners of what is selected are drawn, and can be taken hold of, wherever they are over the preview panel**, outside the picture too (when the picture is zoomed out, or does not fill the panel), so that a corner past the edge of the picture can still be dragged; **they stop at the edge of the preview panel and are never drawn over the other panels** (what is in the properties panel and the timeline works as it does without them). What is not selected can only be taken hold of where it is over the picture. |
| zoom the timeline | **Ctrl + the mouse wheel** over the ruler or a track zooms in and out about the pointer (from some minutes across the whole window to a few frames across it, with a mark for every frame). The ruler's marks, and its time labels (`m:ss`), follow the zoom. |
| zoom the timeline up and down | **Ctrl + Alt + the mouse wheel** over the timeline (the ruler, the title boxes or a track) makes every row taller or shorter together, by the same number of times, about the pointer: the part of the rows that is under the pointer stays under it. Rows go up to three times as tall as their tracks say, and down as far as the rows of all the tracks are the least that a row can be (44 points: what the title box needs; for tracks that are all that short there is no zooming out); a zoom that has been turned past what the tracks allow is brought back by the next turn, so that the wheel never has to be turned through a stretch where nothing happens. **It is how the timeline is looked at, like how far it is zoomed along or scrolled**: the tracks keep the heights they have (the boundary under a title box still gives a track the height that is drawn as tall as the pointer makes its row, with the zoom divided out), it is not an edit (nothing to undo or save), and a new project is looked at without it. Pictures on a taller row are bigger (a row has the pictures of its track's height), and the racks of keyframes in the drawers are as tall as they were. It does nothing while a track is being made taller or shorter, or moved about in the list. (A pinch up and down alone on a touch screen does the same.) |
| frame what is selected, or fit the picture | press **F**: with the pointer **over the preview** the picture is made to fit the panel, as the *Fit* button does it; **anywhere else** the timeline is scrolled and zoomed so that what is selected (clips, keyframes and racks of keyframes, together) is in the middle of it and nearly fills its width (a keyframe, or a few frames, are framed with a second round them, and the strips are not scrolled back past the start or the end of the timeline, so what is there is as near the middle as that lets it be). It does nothing when no clip, keyframe or rack is selected. The timeline then stays as it is until the playhead moves, which takes it to the playhead as it always does. |
| zoom the preview | **the mouse wheel over the preview** zooms in and out about the pointer (so does **Ctrl + the wheel**, and a pinch on a touch pad): in until the pixels of the picture can be seen (a pixel of the picture is as much as eight points wide, and the picture is put on the screen as **blocks** when a pixel of it is two pixels of the screen wide or more, so that they can be told apart, and blended into one another otherwise), and out until the picture is **half the size it is when it fits**. **Shift + the wheel** moves the picture across and **Alt + the wheel** up and down; the **middle button** held down takes hold of the picture and drags it. The picture cannot be moved off the panel: when it is bigger than the panel it covers all of it, and when it is smaller it stays in the middle. When the picture is **not** fitting a button, ***Fit***, comes at the **bottom right** of the preview, and puts the picture back; it is not there while the picture fits. The zoom is only for looking: it is not saved and is not an edit, and it is put back to fitting when another project is opened. What can be taken hold of on the preview is where it is seen. |
| scroll the timeline | the mouse wheel over the ruler or a track scrolls it along without moving the playhead; **Alt + the mouse wheel** (or the thin scroll bar at the right, with more than three tracks) moves the rows up and down |
| go back and forward through the views of the timeline | the **Back** and **Forward** buttons of a mouse (the two that many mice have under the thumb), with the pointer anywhere in the window (not while a dialog is open). **Back goes to the view the timeline had before the latest thing that was done to it**, and again to the one before that, and **Forward** goes on again. A view is how far the timeline is zoomed and scrolled along, and how far its rows are zoomed and scrolled up and down. Each **thing done to it is one step**: turns of the wheel that scroll it (along, or the rows with Alt) are one step for as long as they come less than 0.4 seconds after one another, and a pause longer than that ends it; zooming (along with Ctrl, up and down with Ctrl and Alt) is a step for as long as the wheel is turned, and zooming and scrolling are steps of their own even when one comes straight after the other; and every press of **F** that frames what is selected and changes the view is a step. A turn of the wheel that changes nothing (the timeline is at its start already) is none. **What the timeline does by itself is not a step**: following the current frame while the video plays or the arrow keys move it, scrolling to show a drawer that is opening, and scrolling the rows while a track is dragged to the edge of them. The latest 100 steps are kept. A new step after going back **forgets the views that were gone back from**, as the buttons of a browser do. It is only how the timeline is looked at: it is not an edit (nothing to undo or save), and loading a project starts a history of its own. |
| move or resize the fast-forward symbol | move the pointer over it in the preview (it is shown while the video that is in front is sped up), then drag it, or drag its corner. There is one symbol position and size for the whole project. |

None of the keys works while a text field is being typed in (the title of a track, the words of a text object) or
while a menu or a drop-down list is open: they are its.

**Tracks.** The tracks are drawn in front-to-back order: tracks higher in the list are in front of those below.
A track is a row of clips that may have empty time between them (and never two clips at the same time), and a
clip may be a piece of a **video**, a piece of a **sound file**, a **picture** or a **text object**, in any
mixture. A video clip is drawn as a row of pictures from the video, one for every second of the **finished**
video, side by side with no gaps (or more often, when you zoom in or make the track shorter); a sound clip is
drawn as the **waveform** of its sound, dark blue on pale violet; a picture clip is violet, with the picture once
and the name of its file; a text clip is teal, with what it says; a clip whose file cannot be found is dull red
with the file's name. Every clip has its corners rounded (the pictures and the waveform are cut off at them too), and a
raised look, lit from the top left: its left edge and its top are lit and its right edge and its bottom are in shadow (the
left and right edges a good deal more thickly than the top and bottom), so that dark clips can be told apart where they
meet. **The light goes smoothly round the corners**: at the top right and the bottom left corner, where a lit edge turns
into one in shadow, a line fades from light through nothing to dark, with no sudden change, and the top left corner, which
faces the light, is where the highlight is brightest (1.45 times as bright as along the edges). A sped-up clip is narrower, and the time shown is the time in the exported video. The red line is the
playhead.

**What is on the screen at a frame** is made of **layers**, the one on the back-most track first. Every video,
picture and text object that is there at the frame is a layer, in the place on the picture that it was given
(the default place of a video or a picture is the biggest it can be inside the picture, in the middle, in its
own shape), and turned as far as it was turned. A layer may reach out past the edge of the picture, with its
middle always on the picture: what is past the edge is cut off. Where a layer does not cover the picture, what is
behind it shows, down to black. The fast-forward symbol goes over the front-most video (if that is sped up), under whatever
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

**Changing a length and the keyframes in its drawers.** Making a clip shorter (the time a picture or a text object is
shown for), playing it or its track faster or slower, moving it to a track of another speed, and choosing another frame
rate all change how many frames the drawer of a clip has, and so where its keyframes can be. **No keyframe is lost
while there is a frame for each one**: the keyframes go to the frames nearest to where they were, and **the empty space
between keyframes is used up first** (keyframes that would come to the same frame are put on the frames next to one
another, round where they wanted to be) before any is dropped. When there are more keyframes than frames, the program
**asks first**: *Lose 3 keyframes?*, with *Lose them* and *Cancel*, which leaves everything as it was. If it goes ahead,
what is lost when something is made shorter is the **latest** keyframes, and when it is sped up or slowed down it is
keyframes chosen **evenly** through the rack (the first and the last stay).

**The first video decides the picture, and starts the frame rate.** The size of the finished picture is the size
of the first video, and a project follows that video's frame rate too, until it is given one of its own (*Edit >
Project settings > Frame rate*). Until a project has a video it is 30 frames per second and 1920×1080. Choosing
another frame rate counts everything on the timeline again, so that every clip lasts as long as it did and starts
when it did, to the nearest frame (a clip is a whole number of frames, so a clip of a few frames can come to one
more or less, and the pieces of a video that was cut into several stay together as far as they can). Undoing it
puts everything back exactly.

**Videos of another frame rate than the project's can be added.** Every video is made to run at the rate of the
project, in the preview and in the export: each frame of the project shows the picture of the video that is nearest
in time to it (a picture that is exactly half way between two frames is in the later), so a video of 25 pictures
a second in a project of 30 has a picture on screen for two frames now and then, and lasts as long in time as it
did (3 seconds are 90 frames); a video of 60 in a project of 24 has pictures that are never shown. The first video
of a project that already has clips on its tracks does not change the rate they were laid out at. The preview
chooses the pictures the way the export does wherever it starts (this is tested with the real ffmpeg, for
videos of 12, 24, 25 and 30 pictures a second in projects of 24 and 30).

**Keyframes.** A property that has keyframes is what they say: **between** two keyframes it changes evenly from the one's value to the other's (how loud in decibels, where and how big in fractions of the picture, how far turned in degrees, the letter size, and each of the red, green, blue and opacity of a colour), and a yes or no (mute) and words stay what they were until the next keyframe is reached. **Before the first** keyframe it is what the first says, and **after the last** what the last says. A turn is **the angle it is given, however many turns that is**: 0 degrees at one keyframe and 720 at the next make the picture go round twice between them (the field for how far something is turned has no limit: type the angle, or drag the number, which is a degree for each point the pointer goes, and a tenth of that with Shift held; turning something on the preview by going round and round it turns it as many times as the pointer goes round). Between two keyframes it goes the way the numbers say, so 170 degrees and then -170 is 340 degrees back, and not 20 on. The keyframes of a clip are **in the clip**, counted from its first frame, and go with it wherever it is moved, to another track too. **Cutting** a clip keeps the curve as it was on both sides (a keyframe is put at the cut if the curve goes on through it), **changing the speed** of a clip stretches its keyframes with it, and making a clip shorter (a picture's or a text object's time) pulls in the keyframes that would be past its end, without losing any while there is a frame for each of them (see *Changing a length and the keyframes in its drawers*). A track's keyframes are counted from the start of the timeline. Taking away the last keyframe of a property leaves it as that keyframe said, not animated. They are made and gone to with the buttons beside the controls (and by changing what is animated), and are seen and moved on the timeline, in racks (see below).

**Racks, drawers and crates.** A **rack** holds the keyframes of one property of one clip or track, so that they can be moved as a group, and it is a dark, grainy tray with an orange diamond for each keyframe. Racks are kept in **drawers**: every clip and every track has one, and the keyframes that a stopwatch makes go into a rack in the drawer of what they are of (the rack is made if the drawer has none for that property). But a rack can be moved to any other drawer, so keyframes that lower the sound of a big video can be kept in the drawer of a short clip, and go wherever that clip goes: copy the clip and put the copies at different times, and each lowers the sound of the big video where it is. What a property of something is, when it has keyframes, is what **all** the racks that are of it say together, wherever they are: their keyframes are put at the frames of the timeline they are at (the frames of a rack are counted from the start of the clip whose drawer it is in, or from the start of the timeline for the drawer of a track) and make one curve. Where two are at the same frame the one in the rack that comes later counts (the tracks from the front; of each track the drawer of the track first, and then the drawers of its clips, in order). A keyframe may be at a time when what it is of is not there; it still counts for it. A rack is as long as the drawer it is in. A drawer can hold **several racks of one property of one thing**: they are not merged (merging is asked for, from the menu of the selected racks), and say the property together as all racks do; the racks of a drawer are in an order, which is changed by dragging them. **Keyframe crates** are for racks that have no clip to be kept with: a crate is a stretch of a track that is nothing but a drawer that is always open (it has no drawer of its own, and does nothing to the picture or the sound), shown as a dark box like a clip with the racks in it, one above another, in the middle of it (a crate with more racks than fit makes itself taller, like a drawer, below the track, and lies over the rows under it: it has no button to open it, so there is no Shift to hold, and it never pushes them down as a drawer does). **A crate lasts exactly as long as the keyframes in it need**: from the frame of the first of them, in whichever of its racks, to the frame of the last (a crate with one keyframe lasts one frame). It is **shorter** when keyframes are taken out of it or moved in it, and **longer**, at either end, when a rack that is put in it, or a keyframe or a rack that is dragged in it, has keyframes beyond it: where there is room for that, which is the empty frames round it, as far as the clips on either side of it, the start of the timeline and the end of the video (a crate does not make the video longer); where there is not, the drag stops there and says *not enough space here*. Nobody sets how long a crate lasts, and the frames of its racks stay at the same frames of the timeline when it begins earlier or later. A crate is moved, copied and deleted like a clip, and goes when its last rack goes (or the last of the racks that are of something that is deleted). Taking away the last keyframe of a rack takes the rack away. **Cutting** a clip cuts what is in its drawer too, and the racks kept in other drawers that are of the clip are copied for the second half, so that what the clip does is the same on both sides of the cut.

**Effects.** An **effect** is something that is done to the picture of a clip, or of a track (to everything that is seen on it), on its way to the finished picture. A video, a picture and a text object can have effects, and so can a track; a sound and a keyframe crate cannot. A clip, a track or a group has its effects in a **chain**, and may have several of one kind (two hue shifts, each with values of its own): the effects are done in the order they are in, from the first to the last, and a new one is put last. The chain is shown in the properties panel as the boxes of the effects with **badges** among them, which say where the effects of something else are done to it (see *Groups*), and **the order is changed by dragging**. The effects of a clip are done in the order of its chain, with those of its **track** done at the place of the grey badge of the track (which is last in the chain until it is moved: so to begin with it is the effects of the clip, and then those of its track) and those of each **tag** it is in at the place of the badge of the tag; and after those, the effects of the **clip groups** it is inside, the nearest first, and then those of the **track groups** its track is in. What an effect does is set by its **values**, which are properties like any other: each is changed with a control in the box of the effect, has the stopwatch and the arrows of keyframes, and its keyframes are in racks in the drawers, named by the property (*Hue shift*, and when there are several effects of one kind, which of them: *Hue shift 2*). An effect that is put on has the values it begins with (a hue shift of nothing, which changes no colour); taking it off takes its keyframes with it. The **hue shift** turns the colours of the picture round the wheel of colours by the number of **degrees** it has (a third of the way round is 120): red goes to green and then to blue, as the *hue-rotate* of CSS does it, which keeps how bright a colour is as far as it can; a whole number of turns changes nothing, and how clear a pixel is is not changed. The number is not limited (the slider goes from half a turn back to half a turn on, and a number typed in goes further): between two keyframes the colours go round as many times as the numbers say. The picture of a clip is changed after it is put where it is (so a clip that is turned, or only partly on the picture, has all of its picture changed) and before it is turned, and what is changed in the preview is the same as what is exported. The pictures on the **timeline** are the pictures of the file as they are, whatever the effects are.

*The effects there are.* Every effect has a value of nothing-changed that it begins with (a **default**: the effect does nothing until a value is changed), and its range ends do what they say.

| Effect | Values | What it does |
|---|---|---|
| **Hue shift** | *Turn*, in degrees (-180 to 180 on the slider, any number typed) | as described above |
| **Saturation** | *Amount*, 0 to 2 (1 changes nothing) | 0 makes every colour the grey it is as bright as; 2 doubles how colourful each colour is (a colour that this takes past what a screen shows is cut off) |
| **Brightness and contrast** | *Brightness*, -1 to 1 (0); *Contrast*, 0 to 2 (1) | contrast stretches the colours away from the middle grey, and brightness adds to them, on the numbers of the screen, as the controls of a monitor do |
| **Exposure** | *Amount*, in **stops**, -5 to 5 (0) | multiplies the light by 2 to the power of the stops: +1 is twice the light, -1 half, **in linear light**, as a camera's exposure is |
| **Opacity** | *Amount*, 0 to 1 (1) | makes the picture of the clip, or of the track, see-through by that much (the light of what is behind shows through) |
| **Blur** | *Radius*, 0 to 100 points (0) | a Gaussian blur **of light**. The radius is in **points of a picture 1080 high**, and is scaled with the size of the finished picture, so that the preview and the export of any size blur the same share of the picture |
| **Crop** | *Left*, *Top*, *Right*, *Bottom*, each 0 to 1 of the picture (0), and *Feather*, 0 to 0.5 (0) | what is cut off is clear; the feather softens the new edges |
| **Look-up table** | *Table*, *Intensity* 0 to 1 (1), *Input space*, *Output space*, *Interpolation* | see *Look-up tables* |

The colour effects (hue shift, saturation, brightness and contrast) are done on the numbers of the screen, and exposure, opacity, blur and crop in linear light (a look-up table in the space it says); the program converts between the two **once** where effects of the two kinds are next to each other, not at every effect. Effects that are next to each other and are done on the colours alone are **fused** into one matrix, so a chain of them costs one pass of the card. An effect that does nothing at the frame (a value at its default, or an effect that is switched off) is not asked for at all. A clip group or a track group offers only the effects that give the same picture when they are done to each member by itself (hue shift, saturation, brightness and contrast, exposure, look-up table); **opacity, blur and crop are not offered to them** until the program can do a group's effects to the picture it makes together (a tag offers every effect).

**At most 64 effects** can be on one clip, track or group; adding or copying the 65th is refused with a message, and a project file with more on one thing is refused when it is opened.

**Look-up tables.** The **look-up table** effect reads a `.cube` file (the table that colourists' programs write; 1-D and 3-D tables, and the variant of DaVinci Resolve is read but not written). A file is **read as a table before anything is changed**: one that is not a table is refused with the number of the line that is wrong and the project is left as it was. A 3-D table of **more than 65 points a side** is refused for now (it says so). A 3-D table is applied with **tetrahedral** interpolation by default (or **trilinear**), and a 1-D table is applied to each of the three colours by itself. The *Input space* and the *Output space* say what the numbers of the table mean: the **codes of a screen (sRGB)**, which is what a table made for pictures wants and is the default, **Rec.709 video (gamma 2.4)**, or **linear light (the working space)**; the program converts to the input space, looks the colour up, and converts back from the output space. *Intensity* mixes the result with the picture as it was, in light. Five tables of the program's own are in the folder `assets/luts` (*cool*, *faded*, *monochrome*, *punchy*, *warm*: made by a formula, `tools/make_sample_luts.py`, and given to the public domain, CC0); choose one with *Add effect from file...*.

**Where the table is kept can be changed afterwards.** Right-click the heading of the box of a look-up table effect: *Include table in project file* puts a linked table inside the `.ffrs` file (if the project is not saved as one, the program asks to save it as one first, as it does for a video), and *Take table out of project file* puts an included table back where it came from, if nothing is there or the same file is (another file at that place is never overwritten: then the next place is tried, and if there is none the table stays in the file and a message says so). Where the project has several tables that could be moved, the menu also offers *Include all N tables in project file* or *Take all N tables out of project file*. Like the same actions for a video this is not an edit that undo takes back.

**The table is kept in the project** (it is a *resource* of the project: the same table used by several effects is kept once) as the setting *Files that effects use (tables of colours)* says: **embedded** in the `.ffrs` file (the default; the table is then in the file and travels with it, and a table cannot be larger than 64 MiB), **linked** (the path of the file is kept, and the table is read from there when the project is opened) or **asked about** each time a table is added. A project that is saved as plain `.json` has no room to embed anything, so the table is linked, and the program says so once. A table that is **not there** when a project is opened (a linked file that was moved) is shown in the box of the effect (*The project does not have this table.*), does nothing to the picture, and **the export asks about it**: its prompt lists the missing tables, as it lists the effects that are not installed.

**Groups.** Things can be put together in a **group**, which has a name and a colour and can have effects of its own. There are three kinds:

* A **tag** holds clips (from any tracks, with whatever is between them), and clip groups, track groups and other tags (a group cannot be in itself, however far round). A clip or a group can be in any number of tags. The effects of a tag are done **to each member by itself**, at the place in the member's chain where it has the badge of the tag: a hue shift on a tag turns the colours of each clip in it.
* A **clip group** holds clips and other clip groups that fit in **one rectangle of the timeline**: so many frames across and so many tracks down, the smallest rectangle that has all of them in it, which is the rectangle that is drawn round it. **No clip that is not in the group may be inside that rectangle**, whatever is done: a clip that is in no clip group and is dragged into the rectangle (or put there by an edit that moves clips) **joins the group**, and one that cannot (it would make the rectangle take in a clip that is not in it, or it is in another clip group) stops against the rectangle, as it stops against another clip; and an edit that would make the rectangle take in another clip (a copy, a speed, a length, a frame rate, a track moved, a new text object, a keyframe crate) is not made, and a message says which clip is in the way. Clips are also taken in and let go of by **moving the left or the right edge of the group** (see *make a clip group wider or narrower*). A clip or a group is directly in one clip group at most.
* A **track group** holds tracks and other track groups that are **next to each other** in the list of tracks, and so everything that is on them, from the start of the timeline to the end. A track is directly in one track group at most; a track that is not in it cannot be moved in between its tracks, and one that is cannot be moved out of it. A clip group can be inside a track group (all of it, or none), but a track group cannot be in a clip group.

*The outlines.* A group is drawn on the timeline as a line in its colour (2 points thick, and 3 while the group is selected), over the clips and under the playhead: a **rounded rectangle** round a clip group; the same, across the whole timeline and reaching left over the **boxes of the titles** of its tracks, for a track group; and for a tag **one loop**, as of yarn, that goes round all of its members and the thin invisible **spokes** that join them (straight between members that are in the same rows or the same columns, and bent once between those that are not), rounded off where it turns. A group that has groups in it is drawn **4 points further out** than the groups in it, for each level, so that it goes round their outlines. The outlines themselves are not clicked; the **names** of clip groups and tags are shown over them while the pointer is over the group, and are clicked and dragged (see *select a group on the timeline*), and the left and right edges of a clip group are taken hold of to move them (see *make a clip group wider or narrower*).

*The chain and its badges.* In the properties panel, a clip, a track or a group has the boxes of its effects in the order they are done in, and among them a **badge** for each tag it is in: a bar in the colour of the tag, saying *Tag: name*, at the place where the effects of the tag are done to it. A clip on a track that has an effect that applies to it has a **grey badge of the track** too (*Track: name (1 effect)*) where the effects of the track are done to it; it is not there when the track has none that applies to the clip (a sound, say). The badges, like the boxes of the effects, are **dragged** to another place in the chain; **double-clicking** a badge selects what it stands for (the tag, or the track). Last in the chain, apart, and not to be moved, is a badge of the **clip group or track group** that the thing is directly in: the effects of that group are done after all the rest.

*The effects of a clip group or a track group are a placeholder for now.* The effects of a clip group or a track group are meant to be done to the picture that its members make **together**. The preview and the export cannot make that picture yet (a program that can draw some layers into a picture of their own, change it, and then draw it over the rest is planned: see `docs/long-term-plan-plugins-and-gpu-engine.md`), so until they can the effects of such a group are done to **the picture of each member by itself**, as if each member had been given the effects. Where the members are not see-through (a video, a picture with nothing clear in it) that makes the same picture. **Where they are see-through** (the soft edges of text, of a picture that is turned, or of one with clear places) **and are over other pictures**, a colour that the effect pushes past its brightest or its darkest comes out different from what the effect would have made of the picture as a whole: by a few levels of the 255 of a colour for a pair of layers that was measured (14), and by up to about 100 for vivid colours chosen to make it as bad as it can be (tests of the program work these differences out). Because of that, only effects that give the same picture when they are done to each member are offered to a clip group or a track group (all there are now do: the hue shift); one that would not, a blur, say, will not be until the program can do it right. The same is done in the preview and in the export. (A tag does not have this: it is meant to do its effects to each member.)

Groups do nothing to the **sound** (their effects may, later). Deleting a clip or a track takes it out of its groups, and a group with nothing left in it goes. A clip that is **cut** makes two clips in the same groups, with the badges in the same places; a **copy** of a clip or a track is in the groups of what it is a copy of. Every change to a group (its name, its colour, what is in it, its effects, the order of its chain) is one edit that can be undone, and so is making and deleting one. A group is **named in the project file by where it is** (see *Project files*), as the keyframes name the clips they are of.

**Things that can be taken hold of and dragged have grip dots**: little bumps, shaded and not painted, lit from the upper left like braille. There are six (two rows of three) at the top of the box of a track, ten (two rows of five) in the middle of the top edge of a clip, and six (three rows of two) at the left end of a rack of keyframes. The box of a track is rounded at its two left corners and square at the two that face the strip, so that the box and the strip meet with nothing notched between them.

**Sound.** While the video plays, its sound plays too, and it is the sound the export will have: every clip's
sound is cut out, sped up with the clip, and mixed with the other tracks' (see *Settings* for what sped-up sound
is like). The sound starts a fraction of a second after pressing Play (about 0.07 seconds was measured on the
development machine), jumps with the playhead when that is moved while playing, and stops the moment the video
is paused. Changing the volume of one clip or track while playing does not interrupt the sound of the others.

*How the sound is made (phase 7).* The sound is made by **the sound engine** (`sound_engine`): a graph that works a block at a time and is the same for the preview and the export.
Each clip has an ffmpeg of its own that cuts it out of its file and sets its speed (with the very filters the one big filter graph of ffmpeg used to have for it, which is where
`atempo` stays); the engine makes each clip **exactly as long as its place on the timeline**, gives it its **volume at every sample** (between two keyframes it goes evenly in
decibels), puts the clips of a track one after another **at the sample each frame is at** (worked out from the frame, so that 29.97 frames a second does not drift), and adds the
tracks. The ffmpegs of the clips are started about two seconds before they are needed (at most twelve at a time). If the speakers want another kind of sound than stereo at 48000
a second, the sound goes through one more ffmpeg that changes it, as it always did. **The old filter graph is still there**: set the environment variable
`FASTFORWARDRS_SOUND=ffmpeg` and the preview and the export use it instead (it is what the engine is tested against: `tests/sound_engine_null_test.rs`). **Measured against it
[V]:** with constant volumes the two are the same sound to one rounding of a float, in 48 made-up projects with sped-up clips, clips too long and too short for their place, and
every setting for sped-up sound; where a volume **changes**, the old graph changes it in steps of 4096 samples (about 85 milliseconds) and the engine at every sample, so the
engine is the one that follows the curve; and at 29.97 frames a second the old graph's clips are up to a sample later for each item before them, which the engine's are not. A
project of 200 short clips is made at about nine times the speed it is heard, with the first sound after about half a second. Nothing has been **listened to** yet: whether it
sounds right is for an ear.

If a loaded project's file can't be found or opened, a warning appears in the middle of the window listing each
such file and saying it must be chosen again. The rest of the program stays usable and none of the edits are
discarded; choosing the file (the warning's button, or the *Choose the file again* button in the box of a clip
that is made from it) puts it back. A replacement must be the same kind of file; a video may have another frame
rate than the original (the project does not change its own: the clips are in frames of the project, and the
replacement is made to run at its rate).

## Settings

| Setting | What it does | Default |
|---|---|---|
| Autosave | Saves the project to its file by itself two seconds after the last change. Only for a project that already has a file: the first save always asks where. With it on, closing the window or loading another project do not ask about unsaved changes: they save. | off |
| Sound of sped-up clips | A drop-down list for what the sound of a clip that plays faster than normal is like, in the preview and in the exported video: **Mute** (silence of the same length), **Maintain pitch** (faster, but not higher) or **Increase pitch** (like a tape wound on: higher, the more the faster; a clip faster than 1000x is silent) | Maintain pitch |
| Display (how the screen shows colours) | The curve that the numbers of the preview are made with, for the screen: **sRGB**, **Gamma 2.2** or **Gamma 2.4 (BT.1886)**. Only the preview follows it; the exported video follows *Colour space of exported videos*. (The primaries of the screen are taken to be Rec.709's; the program does not read the computer's colour profile.) | sRGB |
| Working colour space | What the pictures of the project are put together in, as light: **Rec.709 (linear)** or **ACEScg** (the wide primaries of ACES, which keep the mixing of strongly coloured light truer). The preview and the export both follow it. | Rec.709 (linear) |
| Files that effects use (tables of colours) | How a table of colours that an effect uses is kept: **Embed in the project file** (inside a `.ffrs` file; a plain `.json` file links it instead and says so once), **Link to the file** (the path is kept) or **Ask each time**. The project's own setting beats the global one. | Embed in the project file |
| Colour space of exported videos | What the numbers of the exported video are: **Rec.709, as the preview shows it** (the picture is encoded as the display setting encodes the preview, so that the file's numbers are the preview's), **Rec.709, BT.1886 (gamma 2.4)** or **sRGB**. The file says which (see *Exported video*). | Rec.709, as the preview shows it |

The **global settings** (*Edit > Global settings*) are the same for every project. They are kept in a file in
the operating system's folder for programs' settings (`%APPDATA%\fastforwardrs\settings.json` on Windows), which
is written as soon as one is changed.

The **project settings** (*Edit > Project settings*) have a checkbox, *Use global setting*, next to each
setting. While it is ticked the project follows the global setting: the setting beside it is greyed out (showing
the global value that is in use) and nothing about it is saved in the project file. Unticking it gives the
project a setting of its own. A new project follows all the global settings.

## Project files

A project file is one of two kinds. A **`.json`** file is pretty JSON, meant to be read (and, if needed, edited). A
**`.ffrs`** file is a **zip file** that has the same JSON in it (as `project.fastforwardrs.json`) and, for each file the user
chose to **include**, that file (as `resources/<number>-<name>`, `<number>` being the file's number in the project, so that two
files of one name are two entries). **Nothing in a `.ffrs` file is compressed**: videos, sounds and pictures are compressed
already, so it would take time and give nothing, and as they are stored they are in the file as they are (any program that opens
zip files can take them out; this was checked with Python's `zipfile`, which reads the archive, finds every file whole and finds
the files stored). An archive that another zip program has changed, and compressed (Deflate) while doing it, is read all the same; what this program writes is always stored. A file is told to be an archive by what it begins with, and not by its name. A `.ffrs` is the recommended kind,
for it can do all that the other can and more; which kind a project is saved as is told by the ending of the name (`.ffrs`, in any
case of letters; any other ending is plain JSON, and in the editor a name with no ending gets `.ffrs`). A project that is saved as the other kind, and a plain file that says a file is included in
it, are refused: **a `.json` file cannot have files included**.

**Saving a `.ffrs` file adds to it; it does not write it again.** The files in it, which may be gigabytes, are not copied for
a save, and ffmpeg reads them where they are in the archive (on Windows a file that a program has open cannot be replaced, so
a save that wrote the archive anew could not be made while a video is playing from it). A save writes the new text of the project
after what is in the file, then the list of what is in the file (a zip file's directory, which is read from the end of the file),
then the end record that makes it count; the directory is flushed to the disk before the end record is, so that a save that is
cut off (the power goes) leaves the file as it was: the end record that was last whole is the one that is believed, and what was
begun after it is ignored, and cut off the next time the file is added to. What a save leaves behind (the text it replaces, and the
bytes of a file that was taken out of the file) is **garbage** that the file keeps, which is counted. An autosave does no more than
this, so it takes as long for a project with ten gigabytes of video in it as for a small one. **The file is written again without
its garbage** only when that is worth doing (**any garbage at all, when what is in use is 8 MiB or less, which is every project that has no file included in it: the file is then only the text of the project, and a save does not make it longer**; otherwise when the garbage is at least half of what is in use), and only at a
moment when nothing has the file open: after the user saves it (*Save project*) when no file that is included is read in place (the
file then has little in it, so that takes no time), and when the window is closed, after everything that is reading the project
file has been let go of: if that is a great deal to copy the window stays open, with a window that says how far it has got and a
*Cancel* button (giving up leaves the file as it was, and the window closes), and then it closes. If it cannot be done (a file in
the project file has been damaged: each is checked against its CRC-32 as it is copied) a message says that the file is as it was,
and the window closes when it has been read. *Save project as* writes a new file, which has no garbage. A file that is a zip file
but was cut off in the middle of a save cannot be read by other zip programs until the next save of this program cuts the end off
(Python's `zipfile` only looks in the last 64 KiB for the directory).

Outside an archive the videos, sounds and pictures themselves are not inside
the file: each is linked by **both** an absolute path and a path relative to the project file, and the relative
one is the backup when a project and its files are moved together. The file format is versioned, and the file is
written to a temporary file first and then renamed into place, so a failed save cannot damage an existing
project (an archive is written whole the same way, and flushed to the disk before it replaces the old one). Tracks are listed front-most first; a clip has `gap_before_frames` (left out when 0), and gain, mute, a
track's height, and the place and turn of a clip are left out when they have their normal values. Where
something is on the picture is always **fractions of the picture's width and height** (and the turn is in
degrees, clockwise), so it comes out in the same place whatever size the picture is shown or exported at. A clip, a
track or a keyframe crate that has **racks of keyframes** has a `drawer`: a list with a rack for each property that has
keyframes kept there. A rack has the `property` (`gain`, `muted`, `position`, `size`, `rotation`, `text`, `font`,
`font size`, `colour`, or, for the value of an effect, its id: `degrees`), its `keyframes`, each a `frame` (counted from the clip's or the crate's first frame, or from the
start of the timeline for a track) and a `value`: a number, a pair of numbers (`[across, down]` or `[wide, tall]` as
fractions of the picture), true or false, words, or a colour like `"#FFFFFFFF"`; and, when the keyframes are of
something other than what has the drawer, a `target`: the `track` (its place in the list of tracks, counting from 0) and,
for a clip, the `clip` (its place in the list of that track's clips, counting from 0). What the clip itself says (its
gain, its place, its words) is what the property is while it has no keyframes, and is left as it was when it has some.
A drawer may have several racks of one property of one thing (they say it together, in the order they are in). A video,
a picture, a text object or a track that has **effects** has `effects`: a list with an entry for each, in the order they are
done in, which is its `effect` (the key of the type of effect: a namespace, a colon and an id, `builtin:hue_shift`), its `version` (the
major and the minor number of the type that it was made with), its `instance` (the number that the effect has in this list, which no
other effect of the list has: there may be several effects of one kind, and this is what tells them apart) and its `values`, an object
with a value for each of the values of the effect that is not what it begins with, **by the id of the value** (for the hue shift, `degrees`,
the turn); they are what the effect is while its values have no keyframes (the keyframes of an effect's values are racks like any other,
with the id of the value for the `property`, and the `effect` that they are of: the `instance` of that effect in the list of the clip or the
track that the keyframes are of). **What an effect is** is in `plugins`, a list that a project that has effects has: a snapshot of the
descriptor of each type of effect that it uses (once for each key and version): the `key`, the `version`, the `name`, the `description`,
whether it works on the `picture` or the `sound`, whether it `changes_each_pixel_by_itself`, and its `params`, each with an `id`, a `name`
(what its keyframes are called), a `label` (what its control says), a `type` (`number`, `pair`, `colour`, `flag` or `words`), what it
begins as (`default`) and, for a number, the `range` of its control, its `unit` and its `decimals`. So a project can be read, shown and
edited without its effects being installed. An effect with the same key and major number as one that the program has, whose snapshot has no
value that the program's has not, **is** the program's; any other is an effect that the program does not have: it is kept with all its values
and keyframes, does nothing to a picture or to the export, and is saved again as it was. **Whatever an effect, a type of effect or one of its
values has that this version does not know is kept and written back as it was.** A file with keyframes for what the target does not
have (the values of an effect that it does not have are not its properties, and nor are those of an `effect` that is not in its
list), a target that is not in the file, or a value of the wrong kind, is refused with the rack named, and so is an effect that
there is not (one that has no snapshot in `plugins` and is not the program's, or that says a version that has none), two effects of one
list with the same `instance` (or some with an `instance` and some without), one on a sound or a crate, a snapshot that cannot be used
(a key that is not a namespace, a colon and an id, two values with one id, a range that is not from the least to the most) or two of
one key and version, and values that are not of an id that the effect has, or not the right kind, with the clip or the track named.

**Resources** (the tables of colours that look-up table effects use) are listed in `resources`, a list that a project that uses some has: for each, its `name` (the name the effect's value says), its `kind` (`cube`), its `size` in bytes and its `crc32` (for whoever reads the file; the program trusts the zip file's own check), and where its bytes are kept. A resource that is **included** is an entry of the archive named `project-resources/<crc32>-<name>` (so a table that is changed is another entry and one that is not changed is not written again), and a resource that is **linked** has an absolute path and a path relative to the folder of the project file, as a source file does. Only the resources that an effect uses are saved. The bytes are read when the project is opened; a resource that cannot be read (the linked file is gone, or it is more than 64 MiB) is kept as *missing*: the effects that use it stay, it does nothing to the picture, and it is saved again as it was. Two tables with the same bytes are one resource. The value of a look-up table effect that names the table is the **name** of the resource.

A project that has **groups** has `groups`: a list with an entry for each, in the order they were made in (a group is named in the
rest of the file by its place in this list, counted from 0), which is its `kind` (`tag`, `clip_group` or `track_group`), its `name`,
its `colour` (`"#RRGGBB"`), its `members` and, when it has some, its `effects` (as a clip's) and its `badges`. A member is a clip, by the
place of its track in the list of tracks and its own place on that track, counting from 0 (`{ "clip": { "track": 0, "clip": 2 } }`), a
track (`{ "track": 1 }`), or a group, by its place in the list of groups (`{ "group": 3 }`). A clip or a group that is in tags has
`badges`: an entry for each badge in its chain of effects, which is its `of` (`{ "tag": 0 }`, the place of the tag in the list of groups,
or `"track"`, the track the clip is on) and `effects_before`, how many of the effects of the chain come before it (left out when none do:
of badges that have as many before them, the first in the list is the first in the chain). The badge of the track of a clip that is at the end
of its chain, where it is until it is moved, is not written. A file whose groups break the rules (a member that is not in the file, one
that a group of that kind cannot have, one that is in two clip groups or two track groups, a clip that is inside the rectangle of a clip
group and is not in it, tracks of a track group that are not next to each other, a badge of a tag that the clip is not in, or a clip that
is in a tag and has no badge of it) is refused with the group named.

```json
{
  "file_format": "fastforwardrs-project",
  "file_format_version": 13,
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
  "plugins": [
    {
      "key": "builtin:hue_shift",
      "version": [1, 0],
      "name": "hue shift",
      "description": "Turns the colours round the wheel of colours",
      "domain": "picture",
      "changes_each_pixel_by_itself": true,
      "params": [
        {
          "id": "degrees",
          "name": "hue shift",
          "label": "Turn",
          "type": "number",
          "default": 0.0,
          "range": [-180.0, 180.0],
          "unit": "°",
          "decimals": 1
        }
      ]
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
          "rotation_degrees": -12.5,
          "drawer": [
            {
              "property": "colour",
              "keyframes": [
                { "frame": 0, "value": "#FFFFFF00" },
                { "frame": 15, "value": "#FFFFFFFF" }
              ]
            },
            {
              "property": "gain",
              "target": { "track": 1, "clip": 0 },
              "keyframes": [
                { "frame": 10, "value": 0.0 },
                { "frame": 30, "value": -12.0 }
              ]
            },
            {
              "property": "degrees",
              "effect": 0,
              "keyframes": [
                { "frame": 0, "value": 0.0 },
                { "frame": 60, "value": 360.0 }
              ]
            }
          ],
          "effects": [ { "effect": "builtin:hue_shift", "version": [1, 0], "instance": 0 } ],
          "badges": [ { "of": { "tag": 0 }, "effects_before": 1 } ]
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
          },
          "effects": [ { "effect": "builtin:hue_shift", "version": [1, 0], "instance": 0, "values": { "degrees": 90.0 } } ]
        },
        {
          "kind": "crate",
          "gap_before_frames": 30,
          "duration_in_frames": 61,
          "drawer": [
            {
              "property": "muted",
              "target": { "track": 1 },
              "keyframes": [
                { "frame": 0, "value": false },
                { "frame": 60, "value": true }
              ]
            }
          ]
        }
      ]
    },
    {
      "title": "track 0",
      "gain_decibels": -3.0,
      "speed_factor": 1,
      "muted": false,
      "drawer": [ { "property": "gain", "keyframes": [ { "frame": 0, "value": -3.0 }, { "frame": 600, "value": 0.0 } ] } ],
      "effects": [ { "effect": "builtin:hue_shift", "version": [1, 0], "instance": 0, "values": { "degrees": 15.0 } }, { "effect": "builtin:hue_shift", "version": [1, 0], "instance": 1, "values": { "degrees": 40.0 } } ],
      "clips": [
        { "kind": "video", "source_identifier": 1, "first_source_frame": 0, "end_source_frame": 300, "speed_factor": 1, "drawer": [ { "property": "position", "keyframes": [ { "frame": 0, "value": [0.5, 0.5] }, { "frame": 120, "value": [0.25, 0.5] } ] } ], "badges": [ { "of": "track" }, { "of": { "tag": 0 } } ] },
        { "kind": "video", "source_identifier": 1, "first_source_frame": 300, "end_source_frame": 600, "speed_factor": 4, "muted": true },
        { "kind": "audio", "gap_before_frames": 30, "source_identifier": 2, "first_source_frame": 0, "end_source_frame": 150, "speed_factor": 1, "gain_decibels": 6.0 }
      ]
    }
  ],
  "groups": [
    {
      "kind": "tag",
      "name": "intro",
      "colour": "#4BD254",
      "members": [ { "clip": { "track": 0, "clip": 0 } }, { "clip": { "track": 1, "clip": 0 } } ],
      "effects": [ { "effect": "builtin:hue_shift", "version": [1, 0], "instance": 0, "values": { "degrees": 30.0 } } ]
    },
    {
      "kind": "clip_group",
      "name": "titles",
      "colour": "#647DD8",
      "members": [ { "clip": { "track": 0, "clip": 0 } }, { "clip": { "track": 0, "clip": 1 } } ]
    },
    {
      "kind": "track_group",
      "name": "everything",
      "colour": "#D064D8",
      "members": [ { "track": 0 }, { "track": 1 } ]
    }
  ],
  "speed_indicator": {
    "left_fraction_of_frame_width": 0.03,
    "top_fraction_of_frame_height": 0.04,
    "height_fraction_of_frame_height": 0.08
  },
  "settings": {
    "sped_up_clip_sound": "mute"
  },
  "frame_rate": "30/1"
}
```

(The clips of the last track and its drawer are shown on one line each here to save room; the program writes one
value to a line. A test loads this very example, so it cannot go out of date without the tests saying so. In it the
text object lowers the loudness of the first video while it is shown, with a rack kept in its own drawer, a keyframe
crate mutes the track the video is on from frame 420, and the track has a rack of its own for how loud it is. The text object has a hue shift that goes a whole turn round in its first 60 frames, with its keyframes in a rack in its drawer; the picture has one that is a quarter of a turn; and the track of the videos has two (named 0 and 1 in its list), of 15 degrees and of 40, which together turn the colours of both of them by 55. The text object and the first video are in a tag, which turns their colours another 30 degrees: in the chain of the text object the badge of the tag comes after its own hue shift, and in that of the video it comes after the badge of its track. The text object and the picture are in a clip group, and both tracks are in a track group.) A text object's
font size is a fraction of the picture's height, which the window shows as points on a picture 1080 tall; an
empty `font_family` is the program's default font. A video clip may have a `placement` like the picture's
(without one it is fitted inside the picture), and the `area` of a placement is the rectangle **before** it is
turned. It may reach out past the edges of the picture (a `left_fraction_of_frame_width` below 0 starts before the
picture, and a left and a width that add up to more than 1 end after it), but its middle is always on the picture
and it is never bigger than the picture: what is further out than that is moved back in when the file is read. `settings` holds only the settings the project has chosen for
itself, and is left out altogether when the project follows them all. `frame_rate` is the frame rate chosen for
the project, and is left out while the project follows its first video. A source that is included in the project file
(which is an archive then) has `included_as`, the name of its entry, besides the two paths it had when it was included, which
are kept as they were (they are where it is put back); it is left out for a source that is not included. A video or a picture file that the user has said
the colours of (see *Colours*) has `colour`: an object with the parts that were said and no others, `matrix` (`bt601`, `bt709` or `bt2020`), `range` (`limited` or `full`), `primaries` (`rec709`, `bt601_ntsc`, `bt601_pal`, `rec2020`, `display_p3`, `aces_cg`, `aces2065`) and `curve` (`srgb`, `gamma22`, `gamma24`, `gamma26`, `rec709` or `linear`); it is left out when nothing was said (and a sound has none). The project's own `settings` may also have `display_transform`, `working_colour_space` and `output_colour_space` (left out while the project follows the global ones). **Older files are still
read**: version 11 files have no `colour` and no colour settings (the user said nothing, and the project follows the global colour settings: they load as they were); version 9 files have no groups and no badges (nothing in them is in a group), and what the program writes is always version 14 (version 14 added the `plugin_libraries` of a project, see *Plugins and who may run them*; a version 13 file has none; version 13 added to the effects: `category` and, for each value, `scale` and `control` (a number, a choice with its options, a switch, or a resource of a kind) in the snapshot of an effect, `enabled` in an effect, which is left out while it is on, the `resources` list, and a `resource_storage` setting; a version 12 file has none of them and is read as it was); version 10 files (and version 11 as it was while the effects were being given names) call an effect by the name that the program had for it, `hue shift`, have their values in a list, have no `plugins`, and have no `instance` in their effects and no `effect` in their racks (they have at most one effect of each kind: the effects are named in the order they are in, and a rack of the keyframes of the values of an effect is of the effect that has a value of that name);
version 8 files have no `included_as` (nothing in them is included); version 7
files have no effects (nothing in them has one); version 6
files have an `animation` object in a clip or a track where version 7 has a rack in its drawer (it is read as a rack of
the clip's or track's own keyframes, and has no crates or racks of other things); version 5
files have no keyframes (nothing in them is animated); version 4
files have no frame rate of their own (the project follows its first video, and all its videos run at that rate);
version 3 files have no heights, pictures, places or turns (every track is of the normal height, every video is
fitted inside the picture and every text is upright); version 2 files (one video and its clips) become a project with
one track called `track 0`, and version 1 files (from before there were settings) follow all the settings. The
old yes-or-no setting *mute sped-up clips* is read as *Mute* or *Maintain pitch*.

## Plugins and who may run them

*Phase 7 is under way: **a sound plugin can be put on a clip and keyframed, and it is heard** in the preview and in the export (piece 7.6; **it is waiting for your ears**: nothing here can hear). What is built is everything that decides whether one may be run (below), the part that can load a library at all (a crate of its own, so that the rest of the program can go on forbidding `unsafe` code), a way of finding what is installed that cannot take the editor down with it (*Finding the plugins that are installed*), the plugins as effects (*Sound plugins as effects*), and running them in the sound (*Running plugins in the sound*).*

**A plugin is a program.** A sound plugin is a file of native code (a `.dll`, `.so` or `.dylib`) that the computer loads and runs with all the rights of whoever runs the editor, and **nothing checks what it does**. A project file can name one or carry one, so a project from somebody else could run anything. These are the rules:

* A project lists its **plugin libraries** (`plugin_libraries` in the file, format 14). A library is **linked** (a file somewhere, named by a path like a video; the path it was at and the same place from the folder of the project file are tried) or **embedded** (inside the `.ffrs` file, with one **build** for each kind of computer: a file of native code runs on one operating system and one processor, so `windows-x86_64` and `linux-x86_64` are builds of one library). Only a `.ffrs` file can have a library inside it. At most 64 libraries, 16 builds of one, 256 MiB for a build and 256 MiB for all the builds of a project (a project that says more is refused when it is opened, before any of it is read).
* When a project is **opened**, each library that has not been answered for is **asked about**, one after another, in a window that blocks everything else (nothing about plugins is an always-visible control). Plugins that are **files on this computer** (linked) are asked about **once for each folder** they are in (*Run plugins from this folder?*, with the libraries that are wanted named), and a *yes* is remembered for **the folder** (in `plugin_approvals.json` beside the settings file): what is in the folder, or comes to be, is not asked about again, and a file that is changed is run as it is. (This is the answer to *I don't want a question for every plugin I installed*; it trusts the folder as a whole, so approve only folders whose contents you would run.) A folder is asked about when a project that has plugins on its clips is opened, when the scan finds a plugin that a project needed, when the sound first comes to a plugin nobody has answered for, and before an export; **a *no* holds for as long as the project is open**, and the sound goes through those plugins unchanged. A plugin **inside the project file** gets a severe question **every time the project is opened** (never remembered, not even after a crash): *This project contains a program: «name», for windows-x86_64. It runs with all your rights and nothing checks what it does. Load it only if you trust where this project came from.* **Don't load** is the default (Escape and closing the window are *no* too), and **Load it** cannot be clicked: it has to be **held down for five seconds**, with the mouse on it or with the Enter key, while it fills; letting go starts it over; and the Space bar does nothing.
* An embedded library with **no build for this computer** is not asked about: a message says which kinds it has. A linked one that is not where the project says is said so too.
* **Declining is safe.** A plugin that is not said yes to is a plugin that is not there: what uses it passes the sound through unchanged and keeps its settings, as any effect the program does not have does, and the project keeps the library and is saved with it as it was.
* Before an embedded plugin may be loaded it is **written out to a folder of its own** in the temporary files, and **checked against the SHA-256 that the project recorded for it** (a build that was changed or damaged is never run); the folder is removed when the program ends, and the folders that a run which was killed left behind are removed by a later one when they are a day old.
* **The command line cannot ask**, so `fastforwardrs-export` **refuses** a project that needs a plugin inside the project file, or a plugin in a folder that has not been approved, lists them, and exports nothing, unless it is given `--allow-plugins`. `--plugin-approvals <file>` reads the approvals from another file than the editor's. It looks for the plugins that the clips need (the folders in the file `plugin_search.json` beside the settings file, which `--settings` moves, and the usual places) with the same scan program the editor uses, and only when the project has effects that are not installed.

**Finding the plugins that are installed.** Plugins are looked for in the folders that a person adds, in the folders of the environment variable `LADSPA_PATH`, and in the usual ones (`/usr/lib/ladspa`, `/usr/local/lib/ladspa` and `~/.ladspa` on Linux and macOS; **Windows has none**: add the folder of the program that has them, such as the `plugins\ladspa` folder of LMMS). **A library is never loaded by the editor to find out what is in it.** The editor starts a program of its own, `fastforwardrs-plugin-scan <library file>`, which loads the library, reads what each plugin says of itself (its ports and the hints about them: range, default, toggle, whole number, logarithmic, a bound that is a fraction of the sample rate), then **makes each plugin and runs it once on 512 moments of silence**, and writes what it found. A library that crashes that program, never returns (it is stopped after **5 seconds**), says nothing, says something that is not the answer, or is not a library the system can load (an empty file, a damaged one, one that needs a library that is not there) is **failed, with the reason and what the scan was doing when it ended**, and the editor goes on. A plugin that cannot be made or run is listed with the reason, and the others of its library are not lost. What the scans found, failures too, is kept in `plugin_scan_cache.json` beside the settings and **is not tried again until the library's size or time of change is different** (so a library that crashes the scanner does not do it at every start; a scan of another version of the program is not believed). A **breadcrumb** (`plugins_in_use.json`) is meant for the time when plugins run in the editor itself: a library is written in it, and the file is flushed to the disk, before the first time it is used, and the file is removed when the editor ends well; one that is found at the next start says which library was in use when the editor ended badly, and the one that was started last is failed like the others. The scan program silences the crash windows of the system (on Windows) and core files (on Linux and macOS) for itself.

**The host.** `crates/ladspa-host` is **the only part of the program with `unsafe` code in it** (the editor keeps `#![forbid(unsafe_code)]`): it loads a library, reads the descriptors, makes instances and runs them, with a safe interface (an instance connects its ports to memory it owns or to buffers that are lent for one call, never leaves a plugin holding a pointer to memory that has gone, activates at the first block, deactivates and cleans up when it is dropped, and keeps the library loaded for as long as it exists). Every `unsafe` block says why it is sound and a test of the crate counts them (25 places; the list is in `docs/engine-phase-7-design.md`). The facts of the interface that are easy to get wrong are tested: `unsigned long` is 32 bits on Windows and 64 on Linux, so those fields are `c_ulong`; the places of the fields of the descriptor; the flags; the arithmetic of the defaults of the hints. `crates/ladspa-test-plugins` is a plugin library written for the tests, in Rust and from `ladspa.h` and not with the host's types (so the two agree only where the header makes them): a gain, a filter that remembers, a stereo swap, a toggle and a whole number and a meter, a plugin with no ports, a generator, mono to stereo, stereo to mono, and one that counts what the host does to it; and it can be asked to crash, never return, print noise, say nothing, give nothing or lie about how many ports it has. It is public domain (CC0).

**Sound plugins as effects.** *Edit, Sound plugins...* opens a small window (it is not a control that is always there) where a person says where plugins are: the **folders to look for plugins in** (those of `LADSPA_PATH` and the usual ones are looked in too, and shown), the **folders with libraries that plugins need** (on Windows a plugin often needs a DLL that is not beside it: LMMS keeps `libfftw3f-3.dll` two folders above its plugins), and a checkbox, **on unless it is turned off**, to look in **the folders above each plugin library** too when the library it needs is not in the folders just named (nearest first, never the top of the disk). It says what the scan found (each library and how many plugins it has; the ones that failed, with the reason; the plugins that cannot be effects, with the reason), and has *Scan again*. Where to look is kept in `plugin_search.json` beside the settings. The scan runs in the background, at the start of the editor and whenever the places change, and never holds up the window; **a scan that failed for want of a library is tried again when the places to look for libraries change** (the others, a crash or a library that never returns, are not).
* **Each plugin that has an audio input and an audio output is an effect** (one in and one out, two and two, one and two, two and one) that can be put on **a sound or a video** (a video has a soundtrack), on a track and on a tag. A plugin with no audio input is a generator and is not an effect yet; its reason is in the window. **The menu of the plus button lists them** under the heading of their maker, found by the search field by name and by maker.
* **Each control input of a plugin is a value of the effect**, with the range its hint says (a bound that is a fraction of the sample rate is made into the value at 48000 a second), the default its hint says, a logarithmic slider where the hint says so, a switch for a toggle, a whole number for an integer; a value with a range that has only one end is a field to drag. A control output (a meter) is not a value. Values are keyframed like any other. Its key is `ladspa:` the library file's name without its ending, a colon and the label.
* **A project keeps what a plugin effect is made of**: the snapshot of the effect, its values and its keyframes are in the file. Where the plugin is not installed (or not found yet: the scan is in the background) the effect is **kept whole**, says *Not installed*, and is written back as it was; when the plugin is found it works, without opening the project again.
* **What the plan of the sound has:** each clip's plan carries its sound effects with their values and keyframes (and the frame it begins at), for the sound engine to do.

**Running plugins in the sound (piece 7.6).** The preview and the export ask the same code (`plugin_host`) to make the effect of a plugin, and the graph of the sound engine puts it in the clip's **insertions**.
* **Where in the sound:** the effects of a clip are done to its sound **before its volume**, one after another in the order of its chain. The effects of a **track** (and of a tag) are done to the sound of **each clip** where their badge is in the clip's chain, and not to the track once its clips are together: for an effect with no memory that is the same sound; for a filter or a reverb a clip ends where the effect's memory of it ends, and the next clip begins with the effect as it was made. (Making the effect of a track once, after its clips are together, waits for the groups of tracks.)
* **Channel layouts:** a plugin with **one input and one output** is made twice, one copy for each channel, with the same values; **two and two** is made once; **one and two** is given the mix of the two channels and its two outputs are the channels; **two and one** is given both channels and its output is both.
* **Values are exact at the frame the keyframes say:** a value that has keyframes is looked at **every 32 samples** (two thirds of a thousandth of a second) at the frame the first of them is in, and one that has not is given once and kept. A value is **kept to the range the plugin says** (a person can type one past its slider; the plugin is not given it). Playing from the middle of a clip gives the same samples as playing through to it, to the sample, where the plugin has no memory (tested with a gain that rises).
* **A plugin that remembers** (a filter, a reverb) is made again when the sound is made again; a seek makes the sound again, so it begins as the plugin begins. (A pre-roll, running the plugin for a while before the place a seek lands on so that its memory is warm, is not done.)
* **What a plugin gives that is not a number** (NaN, infinity) is **silence** in its place; a plugin that fails to run is left out for the rest of that play and a note says so.
* **A plugin that is too slow** (it takes longer to make a block of sound than the block lasts, for two seconds of sound in a row) is **left out of the preview** for the rest of that play, the sound going through it unchanged, and a message says so, once. **The export waits for it and never leaves one out.**
* **The same sound:** the preview and the export make the same samples (tested to the sample with the real ffmpeg and the test plugins, with and without keyframes), and a clip with no plugin on it is made by the host exactly as it is made without one.
* **Who may be run, in the sound:** a plugin in a folder nobody has said yes to is **not run**: the sound goes through it unchanged and the editor asks about the folder (above); when the answer is *yes* the sound is made again with the plugin in it, without anything else being done.
* **The libraries that plugins need** are looked for where *Sound plugins...* says (the folders named, then the folders above the plugin library if the box is checked, which it is unless turned off) when the editor loads a plugin to run it, as the scan does. On Windows this is done by putting those folders in front of the search path of the process before the library is loaded; the other systems read their search path when the program starts, so a library that needs one that is not beside it is found by the scan (which is started with the folders) and may not be by the editor: *[B] not tried on Linux or macOS*.
* **The breadcrumb is written now:** a library is written in `plugins_in_use.json` and flushed before the first time the editor loads it, and the file is taken away when the window is closed; one found at the next start fails the library it was using (the last one started) in the scan cache and says so in a message, and the library is not loaded again until the file changes.
* **Plugins run in the editor's own process** (the host is `crates/ladspa-host`): a plugin that crashes takes the editor with it, and the breadcrumb says, the next time, which library it was. That is the price of running them at the speed of the sound; the scan, which tries every plugin once in a program of its own, is what makes it rare.
* **A real set:** `tests/sound_plugins_lmms_set.rs` (ignored; run it on your computer with `FASTFORWARDRS_LADSPA_FOLDER` set to a folder of LADSPA libraries) puts **every plugin that can be an effect on a clip, each in a process of its own**, makes a second of noise with it by the export and by the preview, and reports for each whether the two made the same sound, whether it is still a sound, whether it changed it, how loud it is, and what the plugin said; a plugin that crashes or hangs takes only its own process.

**Not built yet (the rest of phase 7):** a plugin that is **inside a project file** is asked about (the severe question) but its effects run only when the same plugin is also installed from a folder (nothing makes an installed plugin of an embedded library yet); generators (plugins with no audio input); the effects of a **group** of tracks; a pre-roll for plugins with memory.

## Colours

**What a picture is made of is light, and the program mixes light.** Every picture that is put on the frame (a video, a picture file, a text, the fast-forward symbol) is first
turned from the numbers of its file into **linear light** (by the curve that goes with the file), in the primaries of the **working colour space** (Rec.709 or ACEScg: see *Settings*).
Scaling, turning, a layer that is half see-through, and the soft edge of a letter all mix light, as a lens and a screen do: **a white line on black that is made thinner than a
pixel is brighter, not darker; half of white over black is the number 0.735 of an sRGB picture, not 0.5**. At the end the light is turned into the numbers of the screen (the
*display* setting) for the preview, or of the file (the *colour space of exported videos*) for the export, in half-float numbers, and only the last step rounds them to bytes. **What
is not mixed does not change**: a picture shown at its own size and opaque comes out with the numbers it had (all 256 codes are held to it by a test), and so does a clip that is
only cut or moved. The hue shift is done on the numbers of the screen (sRGB codes, Rec.709 primaries) wherever the colours are put together, so that it looks as it always did.

**What the colours of a file are.** A video says what its colours are (ffprobe's `color_space`, `color_range`, `color_primaries`, `color_transfer` and `chroma_location`), and the program
reads it as a player does: the **matrix** (BT.601, BT.709 or BT.2020) and the **range** (limited, 16 to 235, or full) as the file says; where the file **says nothing** of the matrix, it is
**BT.709 for a picture of more than 1024 across or 576 down and BT.601 for a smaller one** (it was BT.601 for every size until version 12), and the range is limited (full for a `yuvj` file);
the **primaries** as the file says (Rec.709 where it says nothing); and the **curve**: a file that says `bt709` (or any other curve of the video standards of standard dynamic range) or
says nothing is read as **the curve that a screen shows it with, sRGB's** (nearly every such file is a recording of a screen, or made to be seen on one, and every player shows it so; the
camera curve of BT.709 would lift every shadow, and is never chosen from a tag: only by hand). **High dynamic range** (PQ, HLG) is **not converted**: such a file is read as sRGB, and says so.
A picture file, a text and the symbol are sRGB. How a video's pictures are made into colours is one function for the preview and the export (the matrix and range are given to ffmpeg for the
forms that it makes colours of, so that it does not guess BT.601), and where the colour differences of a 4:2:0 video sit among the pixels (`left`, `center`, `topleft`; left where the file does
not say) decides how they are blended back into one for every pixel: **in linear light the edge between two colours of a video is smooth, where it was a block of four pixels**.

**Saying it by hand.** Right-click a clip made from a video or a picture file and choose **Colour space of this file...**: a window with a drop-down list for the *matrix*, the *range*, the
*primaries* and the *transfer function* of a video (a picture file has the primaries and the transfer function). Each starts at **Automatic**, with what that is for this file written in it, and
under the lists is **everything that was assumed, in words**. A choice is applied at once and is an edit that can be undone (choices in a row on one file are one step); the preview and the
export read the file as chosen, and the project file keeps it (see *Project files*). (The old drawing of the preview, which is only used when it is asked for, ignores all of this, as it always did.)

**What can be seen to be different** from before colours were made like this (the pictures and numbers are in `docs/engine-comparison-phase-3`, and the list in `docs/engine-phase-3-report.md`):
what is scaled, turned or made see-through is mixed as light (edges and fine detail are brighter; soft text edges are the coverage of light); an untagged high-definition video has other reds and
greens (BT.709 instead of BT.601); the edge between two colours of a video is smooth; the exported file says its matrix, primaries and transfer where it used to say nothing for a video that
said nothing. Nothing else changes.

## Exported video

* Every picture is **exact**: a clip at speed N keeps source frames `first`, `first+N`, `first+2N`, ... and the
  export contains precisely the frames the project describes, at the project's constant frame rate. This is
  checked picture by picture against the project, on MP4 and MKV, at 29.97 fps, and from a variable-frame-rate
  source (see *Testing*). Empty time is black.
* The picture of each frame is **made by the picture engine, on the graphics card, from the very description that the preview is made from**
  (`frame_scene`, one function for both): the layers from the back to the front over black, the fast-forward symbol over the front-most video, each layer where its
  place says (turned, scaled, cut off at the edge of the picture) with the colours its effects make. The video is read by ffmpeg, one process for each *run* of a
  clip's video (the stretch of the timeline over which the clip is seen, however many captions or pictures come and go over it), through the preview's own
  frame streams, so the same frames are chosen; it comes at the size the layer needs (ffmpeg's own scaling, as before), and in planes where the file is stored so
  that the card can take it as it is. The engine draws the frame, makes the planes of a `yuv420p` picture of it on the card, and hands them back; one more ffmpeg is
  given them as raw video on its standard input and encodes them with the sound. A few frames are asked for ahead, so that the decoders, the card and the encoder
  all have something to do at once. The export is made on the graphics adapter that the window is drawn with (see *Exporting without a window* for the other
  way).
* What has **keyframes** is made **exactly at every frame**: where a layer is, how big it is, how far it is turned, how a text looks and how the colours are changed
  are worked out for that frame, and not for a stretch of frames. (The export used to cut an animation into the stretches over which nothing changes, snap a place to
  a whole pixel and a turn to a tenth of a degree, and refuse an animation that needed more than 1500 stretches; none of that is so any more, and a video that is in
  another place in each of 1560 frames is exported, each where its keyframes say.) How loud a clip is, when it has keyframes for that, is worked out again for
  every little piece of its sound (a fiftieth of a second or so) from the keyframes, and a clip or a track that is muted by keyframes for a while is silence for
  that while.
* What has **effects** is made as they say: the colours of a layer are changed on the card by the matrix of its effects at that frame, before the layer is put in
  its place; the effects of a clip are done and then those of its track, and those of its groups (see *Groups*). The picture of a text object is changed the same
  way. An effect that changes nothing (a hue shift of no turn, or of whole turns) is not asked for at all.
* The **colours of the file** are those of the **colour space of exported videos** of the project (see *Settings*), and nothing else (until version 12 of the
  program they followed the first video of the project): the finished picture is put together in linear light, **encoded** with the curve that the space says into
  half-float numbers, and **only then** turned into the planes of `yuv420p` (BT.709, the limited range, rounded to 8 bits once, at the end: it is not rounded to 8 bits
  first, as the preview's picture is), so that the file's numbers are the preview's when the space is the first one. **The file always says what its colours are**: its
  matrix and its primaries (`bt709`), its transfer (`bt709` for the two spaces that are Rec.709 video, `iec61966-2-1` for sRGB) and its range (`tv`), where it used to say
  nothing when the first video said nothing. A video of another matrix (a standard-definition one, say) is converted, in light, to what the file says, so its colours
  are its own and the file is not tagged with the matrix of another.
* The sound is every track's sound mixed, each clip sped up with its picture (see *Settings*), and it is as long as
  the picture. It is **made first, by the sound engine, into a file in the temporary folder** (32-bit floats, 48000 a second, stereo: about 23 MB a minute, removed when
  the export ends), which the encoder then reads as its second input; the sound of a long project takes its own time before the picture begins to be encoded, and nothing
  shows progress while it is made. (`FASTFORWARDRS_SOUND=ffmpeg` has the encoder make it with the old filter graph, as it was.) Where a sped-up clip is so short that its sound would be under a tenth of a second, that stretch is
  silent instead.
* H.264 (`libx264`, falling back to `mpeg4` if your ffmpeg has no libx264) and AAC. `.mp4` and `.mov` files are
  written for streaming (`+faststart`). The finished video is no bigger than the first video's picture, with an even number of pixels across and down.
* If a project was pointed at a file **shorter** than the one it was edited on, all the edits are kept and the
  timeline keeps its length (so that things on other tracks stay where they are): where a video has no pictures
  any more it is not there (what is behind it shows, or black) and where a sound has no sound any more it is
  silent, and the message at the end of the export says how many frames of the project are missing the picture of
  a video.
* The video is written to `name.exporting.ext` next to the destination and renamed when it is complete, so
  cancelling or failing never leaves a half-written file or damages an existing one. Exporting over a source
  file is refused. What goes wrong after the export has started (a file that cannot be read, a graphics adapter that is
  not there or is lost, ffmpeg that stops, a mistake of the program itself) ends it with a message that says what it was; a frame that is made of more
  pictures than the picture memory of the engine (512 MiB on the card) has room for ends it too.
* A video file that has **fewer pictures than the project was told** (it was changed behind the project's back) does not fail the export at the end of it: the
  last picture it has is held for the rest of the clip, **and the message at the end says how many pictures were held** (the command line says it as a warning, before
  it says that the video was saved).
* The picture is not exactly what the old way of exporting (ffmpeg's filters) made: the same pictures are there, in the same places, to within a pixel and a
  few levels at the edges of things that are turned or scaled (the new way is the one that is exact: see `docs/engine-phase-2-design.md`, section 9).

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

## Licence

fastforwardrs is free software, licensed under the **GNU General Public License, version 3 or (at your option) any later version** (`GPL-3.0-or-later`); the text is in
[`LICENSE`](LICENSE). The libraries it is built from are under MIT, Apache-2.0, Zlib, BSD, ISC, MPL-2.0 and similar licences, all of which can be used in a GPLv3 program (the list
is made from `Cargo.lock` by `cargo metadata`; the egui fonts also carry the SIL Open Font Licence and the Ubuntu Font Licence, and are separate files in that crate). It
runs the programs `ffmpeg` and `ffprobe` as separate processes. Whether plugins that the program loads (sound plugins, shaders, colour tables) have to be GPL themselves is
a question that the copyright holder has settled in a written clarification, [`LICENSE-PLUGIN-CLARIFICATION`](LICENSE-PLUGIN-CLARIFICATION) (adopted 2026-10-15): a plugin that works with the program only through a published plugin interface is not, merely by being loaded, a work based on it. Every source file carries an SPDX header, and `reuse lint` (REUSE 3.3) passes; the licence texts are in `LICENSES/`, and `REUSE.toml` says what covers the other files.
