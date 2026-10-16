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
timeline. A clip or a track can have **effects** (so far, a **hue shift** that turns the colours round the wheel of
colours), which are put on with a **+** button in the properties panel and are animated with keyframes like any other
property. Clips, tracks and other groups can be put together in **groups** (**tags**, **clip groups** and **track groups**),
which are drawn on the timeline as outlines in a colour of their own and can have effects too. The preview can be **zoomed**,
in until the pixels of the picture can be seen. Every edit can be undone.

## Status

| | |
|---|---|
| Tested on | **Windows 10**, and **Ubuntu 26.04 in WSL 2** (since 2026-10-15: the library's tests all pass there, and the integration tests, with ffmpeg 9: see the *ffmpeg* row). macOS: never. |
| Written for | Windows, macOS and Linux. The only platform-specific code is the `.exe` suffix when looking for ffmpeg and hiding the console windows of the ffmpeg child processes (both `cfg(windows)`); a couple of unit tests have a Unix and a Windows variant, and only the Windows ones have been run. It has **not** been built or run on macOS yet; on Linux it builds and runs the tests (`tools/linux/run_in_wsl.sh` runs a command on a copy of the project in WSL, and `.github/workflows/ci.yml` is the CI that is to do it for Windows, Ubuntu and macOS, written but not yet run on GitHub). On Linux the window system libraries that `eframe`/`wgpu` need may have to be installed, and so may ALSA's development files (`libasound2-dev` on Debian and Ubuntu), which the sound output (`cpal`) is built with. Text is drawn with the computer's own fonts, which `cosmic-text` finds on each system. |
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
```

**The picture engine (experimental).** The preview is drawn by the window itself, one texture for each layer. A new way of making it, by a picture engine on the
graphics card (design: `docs/engine-phase-1-design.md`), is tried by setting `FASTFORWARDRS_PREVIEW=engine` before the program is started: the preview is then one
picture that the engine made of all the layers, and the window still draws the outlines, the handles and the places the mouse can take hold of itself. While a project plays and the card cannot keep up, the engine makes its frames smaller (a half, then a quarter) instead of late, and makes them whole again when the picture has been still for a moment. If the engine
cannot start, or stops, the program says so and makes the preview the old way from then on. If the graphics card is reset, or does not finish a frame in two seconds, the engine makes itself a new graphics device and goes on (a picture may be late); if that is needed again within a minute and again, it stops, as above. The old way (`FASTFORWARDRS_PREVIEW=simple`, or no setting) is what the preview is
made by until the engine has been tried on more graphics cards. The export does not use the engine. How fast the engine is on this computer's graphics card is measured by `cargo run --release --example engine_benchmark` (it says which adapters there are with `-- --list`);
it makes frames of four layers with two blurs, a colour matrix and a lookup on them, and says the frames to the second and what each kind of pass costs.

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
| put an **effect** on a clip, a track or a group | **select** a video, a picture or a text object, or a track, or a group: under its box in the properties panel is a round **+** button (a sound has none, and neither does a keyframe crate: they have no picture to change). Click it and choose the effect from the menu (so far: **Hue shift**). The effect gets a **box of its own** under the box of the clip, track or group, headed with its name, with the controls of its values (for the hue shift, *Turn*, in degrees), which are animated with the stopwatch like any other property. **Right-click the heading of an effect's box** and choose *Delete effect*: the effect is deleted with the keyframes of its values, in one edit (undo puts both back). An effect can be put on again and again: a second hue shift is an effect of its own, with values of its own, and when there are several of one kind their boxes say which is which (*Hue shift 1*, *Hue shift 2*, counting from the top; one that is alone has no number). (An effect that a project file brought and that the program does not have is a box too, with the name the file gave it, a note that says *Not installed*, and a control for each of its values: it does nothing to the picture, and its values are kept, and saved again as they were. The values of the effects of a group are not animated: they have no stopwatch.) |
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
its garbage** only when that is worth doing (the garbage is at least 4 MiB and at least half of what is in use), and only at a
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
  "file_format_version": 11,
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
are kept as they were (they are where it is put back); it is left out for a source that is not included. **Older files are still
read**: version 9 files have no groups and no badges (nothing in them is in a group), and what the program writes is always version 11; version 10 files (and version 11 as it was while the effects were being given names) call an effect by the name that the program had for it, `hue shift`, have their values in a list, have no `plugins`, and have no `instance` in their effects and no `effect` in their racks (they have at most one effect of each kind: the effects are named in the order they are in, and a rack of the keyframes of the values of an effect is of the effect that has a value of that name);
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

## Exported video

* Every picture is **exact**: a clip at speed N keeps source frames `first`, `first+N`, `first+2N`, ... and the
  export contains precisely the frames the project describes, at the project's constant frame rate. This is
  checked picture by picture against the project, on MP4 and MKV, at 29.97 fps, and from a variable-frame-rate
  source (see *Testing*). Empty time is black.
* The picture of each frame is made the way the preview shows it: the layers from the back to the front over
  black, the fast-forward symbol over the front-most video. Each video or picture is scaled to where it is on the
  picture (after the pictures a sped-up clip does not keep are dropped, so those are never scaled), and what is
  turned is made into a picture with clear corners before it is laid over. What reaches out past the edge of the
  picture is cut off by being laid over it (a video at the back that does is laid over black, and not put on it by
  padding). Text is drawn by the same code for the preview and for the export, at the size of the finished picture,
  and turned the same way.
* What has **keyframes** is made as they say, frame by frame, the way the preview shows it. A stretch of the picture
  in which a layer has keyframes is cut into the stretches over which what is made of it does not change: where it is
  in whole pixels, how far it is turned (in steps of a tenth of a degree, for what keyframes turn), and for a text
  object what it looks like (a picture is made of it for each way it looks, and one picture however far it moves). A
  slow change is a few stretches and a fast one a stretch for each frame, and each stretch costs ffmpeg time to set up
  (measured: about 7 seconds for 1000, 13 for 1500, 26 for 2000 and over a minute for 3000), so **an animation that
  would need more than 1500 of them is refused** with a message that says so (make what changes change more slowly,
  for a shorter time, or cut the clip into shorter ones). How loud a clip is, when it has keyframes for that, is
  worked out again for every little piece of its sound (a fiftieth of a second or so) from the keyframes, and a clip
  or a track that is muted by keyframes for a while is silence for that while.
* What has **effects** is made as they say. The colours of a video or a picture are changed (`colorchannelmixer`, with
  the nine numbers of the matrix of the effect, found by the same arithmetic as the preview's) right after it is scaled
  to its place, and before it is turned or put on the picture; the picture of a text object is changed the same way
  before it is laid over; the effects of a clip are done and then those of its track. An effect that changes nothing
  (a hue shift of no turn, or of whole turns) is no filter at all. A value of an effect that has keyframes is exported
  **to the nearest whole degree** (a hue turned less than that is not seen), so that a slow turn is a few stretches of
  the picture, not a stretch for each frame; they count towards the 1500 stretches above like moves and colour changes
  of text do.
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
| `project` | What a project *is*: its source files, its tracks (`project::tracks`: a `Track` is a row of clips with gaps, a speed, a gain, a height and a mute switch, and holds all the arithmetic of which clip is at which frame, what moves when a length changes and whether clips may be moved; a clip's `ClipId` is the track it was first made on and a number, so that it keeps its id when it is moved to another track, which `Project::move_clips` does for several clips at once or not at all), the speed and gain types, text objects, pictures, **placements** (`placement.rs`: a rectangle on the picture as fractions, with its middle on the picture and free to reach past its edges, an angle, how a corner drags and what a point is on), the fit of a video inside a picture, settings, and **animation** (`project::animation`: the table of properties that can be animated and the kind of value each has, the keyframes of one property, and the animation of a clip or a track, which is worked out from the racks). The keyframes are kept in **racks** (`project::tracks::rack`) in **drawers** (`drawer`) of the clips, the tracks and the keyframe crates (a crate is a kind of clip: `crate_clip.rs`); `project/racks.rs` puts together what all the racks that are of one thing say, `project/animating.rs` is the project's way of asking what a property is at a frame and of making a keyframe or changing a property at a frame, `project/rack_editing.rs` moves keyframes and racks about (`rack_merging.rs` merges racks and `keyframe_copying.rs` copies keyframes), `project/duplicating.rs` copies a clip or a track with what is in its drawers, and **effects** are data in `project/effects.rs` (an `EffectKind` is a small value that is a reference to an `EffectDescriptor`: the key of the type of effect, its version, what it is called and does, what it works on, and its values, each with an id, a name, a label, what it begins as and how its control looks; the program has some in itself, in `project/effects/built_in.rs`, with what each does, and a project file brings others, which are kept and do nothing; each effect in a chain is named by an `EffectInstanceId`, one number in that chain, and a value of an effect is the property `PropertyId::EffectValue { effect, instance, slot }`, which is what lets a chain have several effects of one kind, each with a name of its own in its chain) with `project/effect_editing.rs` putting them on and taking them off. A value of an effect is a `PropertyId` like any other, and `PropertyId::is_had_by_a_clip` and `is_had_by_a_track` are the one rule for whether a clip or a track has a property (the keyframes of an effect's value only where it is on). Pure logic: no files, processes or drawing. In whole frames (`SourceFrameIndex`, `TimelineFrameIndex`, `SpeedFactor`...). |
| `colour_effects` | The arithmetic of the effects that change the colours of a picture: a `ColourMatrix` (the hue rotation of CSS is one), what it does to the pixels of a picture and the `colorchannelmixer` filter that asks ffmpeg to do the same, so that the preview and the export agree. No other part of the program is in it. |
| `composition` | Works out from a project what the finished video is: the **picture plan** (for each stretch of the timeline, the layers that are seen: their video, picture or text, where each is, its effects, and its keyframes, from which the preview works out where it is at a frame and how the colours are changed) and the **sound plan** (for each track, which clips are heard when, and how loud, which for a clip with keyframes is a loudness that changes; a clip muted by keyframes is cut into the stretches that are heard). Plain logic. The live preview and the export both use these plans, which is what makes them agree. |
| `project_file` | Saving and loading the project JSON, including the older versions, and as a `.ffrs` archive that can have files included in it (`archive`: a zip file of stored entries, with the names of the entries checked before they are believed, that is written whole to a temporary file and renamed into place when it is saved as another file, and added to (`layout`: its own reader and writer of the zip directory, with zip64, that finds the last whole end record and cuts off a save that was cut off) when the project's own file is saved, so that the files in it stay where they are and are read there; `WritePlan` works out what is to be written first, so that a copy of a great deal can be done in the background and stopped). Separate document types keep the file format independent of the in-memory model. |
| `settings_file` | The same for the global settings file, which is more forgiving: a missing setting gets its default and an unknown one is ignored. (Both files are written by one small `atomic_file` module that never leaves a file half-written.) |
| `media` | Everything that runs ffmpeg: finding it, probing a video, sound or picture file, the streams of preview pictures and preview sound, the decoding of a picture file, the thumbnails (at several levels of detail), the waveforms, and the export (planning, the filter graph in a picture half and a sound half, running and cancelling). The streams share one small skeleton (`StreamingTool`). |
| `rgba_picture`, `text_rendering` | A picture made by the program (as pixels), and the drawing of a text object into one, with `cosmic-text` (fonts, wrapping, fallback fonts for other alphabets and emoji). The fonts of the computer take a moment to load, which is done once on another thread. The text is drawn upright in its area: turning it is done when it is laid over. |
| `speed_indicator` | Draws the fast-forward symbol and decides where it goes, for the preview **and** the export, so they cannot disagree. |
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
  what it does: how it changes the colours of a picture from its values, and how its values are rounded when they are exported); a
  project file brings the others, as snapshots of their descriptors. The **+** button's menu, the box of an effect and the stopwatches
  of its values, the file (`plugins` and `effects`), the picture plan, the preview (the colours of the pictures put on the graphics
  card) and the export (the filter and the stretches of an animated value) are all driven from the descriptors, and a test goes
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
* **The preview and the export select frames the same way** (same frame-rate normalisation, same
  `select='not(mod(n,N))'`), which is what lets the preview be trusted as a picture of the export.
* **Large projects.** The export's filter graph is given to ffmpeg in a file, so a project with hundreds of clips
  does not hit command-line length limits. The preview's sound graph is too. The picture plan is made in a time
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
cargo test                      # everything below
cargo clippy --all-targets -- -D warnings
```

| Suite | What it covers |
|---|---|
| unit tests (in `src/`) | the model (including the rules for what moves when a clip changes and for moving clips, checked against a simple model with random edits), placements (corner dragging checked with random drags), the plans in `composition` (checked frame by frame against a plain evaluation, with random projects that have videos of other shapes, places and turns), timeline geometry, zoom, thumbnail levels and waveform bars, both files and the older versions of the project file, filter graph text, symbol drawing, text drawing (with the computer's fonts), the clock, the picture and sound suppliers (with fake decoders and speakers), the history... and **window interaction tests** that send simulated mouse and keyboard input to the real window: selecting (with Ctrl and a rectangle), moving, deleting and cutting clips; undo and redo; zooming, scrolling and resizing tracks; tracks, their titles, the round button and deleting one; the panels and the properties panel (what it shows for what is selected, what its controls do, the list of several things), selecting tracks, dragging clips to other tracks; copying clips and tracks from their menus; the drawers (their buttons, how they open, pushing the tracks below down or, with Shift, lying over them, and how the timeline scrolls to show them), the racks and diamonds in them and their labels, selecting and dragging keyframes alone and with clips, dragging racks between drawers and into crates, with the words *not enough space here*; text objects from the menu; effects (the **+** button and its menu, the box and the slider of the hue shift and its stopwatch, taking it off and undoing that, what the plan, the preview's colours and the export's filter and stretches say at every frame of random projects that have effects, and the files of them), **groups** (the rules about them, checked after tens of thousands of random edits of every kind: nothing but what is in a clip group is ever inside its rectangle, whatever is made, moved, copied, cut, sped up, retimed or deleted, and an edit that is refused changes nothing; the outlines, whose shape is checked on random sets of rectangles for being simple, going round clockwise and having every clip inside; the menus that make groups, the box of a group with its list and its bar, the chain of effects with its badges dragged and double-clicked, drags that stop against rectangles, the messages when an edit is refused, the deletes and the **Delete** key, undo; the file of them, and the older versions), the keyframes that are lost only after a question (the fitting of keyframes is checked against every small case, worked out by trying all the ways there are), racks that are selected, reordered and merged, keyframes that are copied and dragged to other racks, the zoom of the preview (the wheel, the middle button, the *Fit* button, pixels shown as blocks) and the edges of the picture that something is held to; picking, moving (past the edge of the picture too), resizing and turning videos, pictures and text on the preview, by corners that are outside the picture and over the preview (also with a press that comes in the same frame as the move of the pointer, as a tap does), and that corners over the other panels are not taken hold of and are not drawn there; the keys; dialogs; the menus and settings windows; autosave |
| `tests/media_*.rs` | the real ffmpeg: probing videos, sound files and pictures, decoding, thumbnails (checked to be the picture on screen at the moment they stand for), waveforms, exports checked **picture by picture** against an independent calculation of what the project must contain (and tone by tone for the sound of several tracks, with gain and speed), **pixel by pixel for what is scaled, moved and turned** (a video at half size, a video turned a quarter, text turned so that its red left half goes to the top, something cut off at the picture's edge, a video, a picture and a text put partly past the edge, a picture from a file that is turned, comes later in the timeline, or lies over a video that is cut in pieces), and the sound stream (right length and format, from any playhead position, sample for sample the same as the sound from the start, and the same loudness second by second as the export) |
| `tests/application_journeys.rs` | whole journeys through the real program with a real video: import, edit with the mouse and keyboard, undo, save, load into a fresh editor, lose a file and choose it again, import a sound file or a picture, make a text object, animate a video with the stopwatch and the numbers, export (and check the picture and the sound of the result) |
| `tests/media_export_animation.rs` | exports with the real ffmpeg of clips that have keyframes, checked picture by picture: a video that moves (where its edges are at every frame), one that is turned (each picture is the same as the one made with no keyframes by turning the video that far), one that gets smaller, and text that moves and changes colour over a video that goes on unbroken. The sound of keyframes for how loud and whether silent is checked in `tests/media_audio_stream.rs`: a ramp, a ramp started in the middle, a silent stretch with the sound after it from the right place, and the preview's sound against the export's |
| `tests/media_export_effects.rs` | exports with the real ffmpeg of clips and tracks that have a hue shift, checked colour by colour: a flat colour comes out as the colour the arithmetic of the effect says, whether the effect is the clip's or the track's, or both, for a video, a picture file and a text object, for the piece of a video that has the effect and not the piece next to it, with keyframes a little more turned at each frame, and for groups of each kind (a tag turns the clips in it and no other; a clip group and a track group turn the clips in them, which is the placeholder described in *Groups*; and the effects of a clip, its track, a tag, a clip group and a track group are all done) |
| `tests/application_playback.rs` | playing: the preview keeps up with the clock, through a change of speed and a jump, and stops on the last picture. Timing-dependent, so it has its own file, which cargo runs on its own. |
| `tests/application_sound.rs` | the preview's sound in the real program, with recording stand-ins for the speakers: a test video whose pitch changes every second shows that the right sound is heard at the right time, through a change of speed and a jump; that it starts quickly, stops on pause, and is muted or raised in pitch when asked; that changing a volume while playing leaves no gap |
| `tests/application_screenshots.rs` | not checks: with `FASTFORWARDRS_SCREENSHOT_FOLDER` set, `cargo test --test application_screenshots -- --ignored` draws the window in several states to PNG files (tracks, a waveform, text, pictures, turned and resized videos, things past the edge of the picture, zoom, the properties panel, the menus, a missing file, a cut flashing, a track being dragged, drawers, racks of keyframes, racks being dragged and crates, the menus that copy clips and tracks, the buttons of the keyframes, selected racks and the menus of racks and keyframes, the question before keyframes are lost, the preview zoomed in and out and to its pixels, the **+** button and the hue shift...) |

The tests that use ffmpeg make their own small test videos (each frame carries its own frame number as a strip of
black and white blocks, so a test can read which frame it received), tone files and pictures in
`target/test_fixtures` or a scratch folder. They are skipped with a message when ffmpeg is missing; set
`FASTFORWARDRS_REQUIRE_FFMPEG=1` to make that a failure instead. One test opens the computer's real sound output
and plays silence through it; it is skipped on a computer with none. The text tests are skipped on a computer
with no fonts at all. Nothing has been tried with a real touch screen: the taps are simulated.

The tests themselves are checked with **planted mistakes**: one deliberate mistake at a time (a `<` that becomes `<=`, a condition that is left
out...) is put in a copy of the code, and the tests must notice it; a mistake that none of them notices is a gap, and is closed with a test (or
is one that changes nothing a user could see). `tools/mutation_testing/` is what plants them and runs the tests (see its `README.md`): it stops
at the first test that fails, tries several mistakes at once, counts a test as noticing a mistake only if it fails again when it is run by
itself (a test of the real speakers fails by itself when the computer is busy), and checks itself with two controls before it begins and with a
final run of the tests that noticed; `py -m unittest discover -s tools/mutation_testing` tests the runner itself.

## Known limits

* **The effects of a clip group or a track group are a placeholder for now**: they are done to the picture of each member by itself, which makes the same picture as doing them to the whole only where the members are not see-through (see *Groups*); at the see-through edges of layers that are over others it can be quite different. A tag is not affected. Groups have no effect on sound, are not clicked or dragged on the timeline, and have no name written on it.
* Cut clips cannot be joined again. There is no way to trim a clip by dragging its edge (cut it and delete the
  part instead). A clip dragged to another track is never put on a new track: the track has to be there.
* Undo covers what is edited in the project, not the settings, and not choosing a missing file again. It goes
  back at most 200 steps, and only as far as the program has been open (the history is not saved with the
  project).
* The finished picture is the size of the first video.
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
* **Exporting keyframes**: where something is is made in whole pixels, and a turn in steps of a tenth of a degree (as the
  angle between -180 and 180 that it looks like: going round once or twice more is seen in the pictures between, and a
  whole number of turns is exported as no turn at all), as its keyframes say; an animation that makes more than 1500 different stretches of the picture (a fast move or a fast
  change of colour that goes on for more than 50 seconds, say) is refused, for the reason given under *Exported
  video*. How loud something is when it has keyframes is made in small pieces of sound, not sample by sample.
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
  both done), and they are done in the order of the chain, which is changed by dragging. The only effect that the program has is the hue shift, which changes colours and nothing else
  (not how clear a pixel is, and not where anything is), and is exported to the nearest whole degree. The pictures on the
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
