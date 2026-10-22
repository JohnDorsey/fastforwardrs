# Settings, colours and the exported video

What each setting does, how colours are read and mixed, and what the exported video is.

*Where a section says "see *Settings*" or the like, the section is in one of the files that `docs/README.md` lists.*

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

Three more are **global only** (no project has them; they are under a line of their own in the global settings window), for the preview of effects that remember earlier frames (EN-14):

| Setting | What it does | Default |
|---|---|---|
| Generate lead-up on seek | Also the checkbox of the **View** menu. After you jump to another place in the timeline, the frames that lead up to it are made in the background, so that an effect that remembers earlier frames (a shader with a `PERSISTENT` buffer: feedback, trails) shows what the exported video shows there; until it is done the picture is approximate. Off: the preview goes on from the nearest saved checkpoint instead, and says that the picture is approximate. **The export is never affected: it makes every frame in order.** *(The setting is kept and shown. **The checkpoints and the note over the picture work now, whatever the checkbox says**: after a jump the preview goes on from the nearest state it has that is not after the frame before it, and says over the bottom of the picture that it is approximate, until the effect has gone on for 300 frames. The lead-up that this checkbox adds is not built yet.)* | on |
| Checkpoint interval | How far apart, in frames, the preview keeps a copy of the state of such an effect: every 30, 60, 120 or 300 frames. | every 60 frames |
| Checkpoint memory budget | How much memory the copies may take together (256 MiB, 512 MiB, 1, 2 or 4 GiB); the oldest are dropped first. | 1 GiB |

The **project settings** (*Edit > Project settings*) have a checkbox, *Use global setting*, next to each
setting. While it is ticked the project follows the global setting: the setting beside it is greyed out (showing
the global value that is in use) and nothing about it is saved in the project file. Unticking it gives the
project a setting of its own. A new project follows all the global settings.

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
**BT.709 for a picture of more than 1024 across or 576 down and BT.601 for a smaller one**, and the range is limited (full for a `yuvj` file);
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
  are worked out for that frame, and not for a stretch of frames. How loud a clip is, when it has keyframes for that, is worked out again for
  every little piece of its sound (a fiftieth of a second or so) from the keyframes, and a clip or a track that is muted by keyframes for a while is silence for
  that while.
* What has **effects** is made as they say: the colours of a layer are changed on the card by the matrix of its effects at that frame, before the layer is put in
  its place; the effects of a clip are done and then those of its track, and those of its groups (see *Groups*). The picture of a text object is changed the same
  way. An effect that changes nothing (a hue shift of no turn, or of whole turns) is not asked for at all.
* The **colours of the file** are those of the **colour space of exported videos** of the project (see *Settings*), and nothing else: the finished picture is put together in linear light, **encoded** with the curve that the space says into
  half-float numbers, and **only then** turned into the planes of `yuv420p` (BT.709, the limited range, rounded to 8 bits once, at the end: it is not rounded to 8 bits
  first, as the preview's picture is), so that the file's numbers are the preview's when the space is the first one. **The file always says what its colours are**: its
  matrix and its primaries (`bt709`), its transfer (`bt709` for the two spaces that are Rec.709 video, `iec61966-2-1` for sRGB) and its range (`tv`). A video of another matrix (a standard-definition one, say) is converted, in light, to what the file says, so its colours
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
