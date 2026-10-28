# The project file

The `.ffrs` and `.json` project files: their layout, an example, what each part means and how older versions are read.

*Where a section says "see *Settings*" or the like, the section is in one of the files that `docs/README.md` lists.*

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
keyframes kept there. A rack has the `property` (`gain`, `muted`, `position`, `size`, `rotation`, or, for the value of an effect, its id:
`degrees` for the hue shift; **what a text says, its font, its size and its colour are values of its text effect, `text`, `font`, `size` and `colour`**), its `keyframes`, each a `frame` (counted from the clip's or the crate's first frame, or from the
start of the timeline for a track) and a `value`: a number, a pair of numbers (`[across, down]` or `[wide, tall]` as
fractions of the picture), true or false, words, or a colour like `"#FFFFFFFF"`; and, when the keyframes are of
something other than what has the drawer, a `target`: the `track` (its place in the list of tracks, counting from 0) and,
for a clip, the `clip` (its place in the list of that track's clips, counting from 0). What the clip itself says (its
gain, its place; the words of a text are its text effect's) is what the property is while it has no keyframes, and is left as it was when it has some.
A drawer may have several racks of one property of one thing (they say it together, in the order they are in). A video,
a picture, a text object, a clip of a sound file (its effects are those of sound plugins) or a track that has **effects** has `effects`: a list with an entry for each, in the order they are
done in, which is its `effect` (the key of the type of effect: a namespace, a colon and an id, `builtin:hue_shift`), its `version` (the
major and the minor number of the type that it was made with), its `instance` (the number that the effect has in this list, which no
other effect of the list has: there may be several effects of one kind, and this is what tells them apart), its `state` (**only a plugin that has a window has one**: what the plugin saved of
what is not one of its values, in base 64, which only the plugin can read, of any size (a sampler's is tens of megabytes; there was a limit of 16 MiB until 2026-10-26); left out when it saved none, and in a file of version 18 or earlier) and its `values`, an object
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
  "file_format_version": 20,
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
    },
    {
      "key": "builtin:text",
      "version": [1, 0],
      "name": "text",
      "description": "Words, in a font, of a size and a colour",
      "category": "Generators",
      "domain": "picture",
      "params": [
        { "id": "text", "name": "text", "label": "Text", "type": "words", "default": "", "control": "multiline" },
        { "id": "font", "name": "font", "label": "Font", "type": "words", "default": "", "control": "font" },
        {
          "id": "size",
          "name": "font size",
          "label": "Size",
          "type": "number",
          "default": 0.06,
          "range": [0.01, 0.5],
          "unit": "pt",
          "decimals": 1,
          "control": "points_of_the_picture_height"
        },
        { "id": "colour", "name": "colour", "label": "Colour", "type": "colour", "default": "#FFFFFFFF" }
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
          "content": 0,
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
              "effect": 0,
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
              "effect": 1,
              "keyframes": [
                { "frame": 0, "value": 0.0 },
                { "frame": 60, "value": 360.0 }
              ]
            }
          ],
          "effects": [
            { "effect": "builtin:text", "version": [1, 0], "instance": 0, "values": { "text": "Chapter one" } },
            { "effect": "builtin:hue_shift", "version": [1, 0], "instance": 1 }
          ],
          "badges": [ { "of": { "tag": 0 }, "effects_before": 2 } ]
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
crate mutes the track the video is on from frame 420, and the track has a rack of its own for how loud it is. The text object is made of its first effect, the text effect (`content` names it), whose colour has the keyframes of the first rack in its drawer, and it has a hue shift (the effect named 1) that goes a whole turn round in its first 60 frames, with its keyframes in a rack in its drawer; the picture has one that is a quarter of a turn; and the track of the videos has two (named 0 and 1 in its list), of 15 degrees and of 40, which together turn the colours of both of them by 55. The text object and the first video are in a tag, which turns their colours another 30 degrees: in the chain of the text object the badge of the tag comes after its own hue shift, and in that of the video it comes after the badge of its track. The text object and the picture are in a clip group, and both tracks are in a track group.) A text's
`size` is a fraction of the picture's height, which the window shows as points on a picture 1080 tall; an
empty `font` is the program's default font. (A text clip has `content`, the `instance` of its text effect in its `effects`, and the area and turn of its place
on the picture like a generator's; **a text that says nothing of its own is not a file**: what it says, its font, its size and its colour are the `values` of the effect, left out when they are what a new text has.) A video clip may have a `placement` like the picture's
(without one it is fitted inside the picture), and the `area` of a placement is the rectangle **before** it is
turned. It may reach out past the edges of the picture (a `left_fraction_of_frame_width` below 0 starts before the
picture, and a left and a width that add up to more than 1 end after it), but its middle is always on the picture
and it is never bigger than the picture: what is further out than that is moved back in when the file is read. `settings` holds only the settings the project has chosen for
itself, and is left out altogether when the project follows them all. `frame_rate` is the frame rate chosen for
the project, and is left out while the project follows its first video. A source that is included in the project file
(which is an archive then) has `included_as`, the name of its entry, besides the two paths it had when it was included, which
are kept as they were (they are where it is put back); it is left out for a source that is not included. A video or a picture file that the user has said
the colours of (see *Colours*) has `colour`: an object with the parts that were said and no others, `matrix` (`bt601`, `bt709` or `bt2020`), `range` (`limited` or `full`), `primaries` (`rec709`, `bt601_ntsc`, `bt601_pal`, `rec2020`, `display_p3`, `aces_cg`, `aces2065`) and `curve` (`srgb`, `gamma22`, `gamma24`, `gamma26`, `rec709` or `linear`); it is left out when nothing was said (and a sound has none). The project's own `settings` may also have `display_transform`, `working_colour_space` and `output_colour_space` (left out while the project follows the global ones). **Older files are still
read**: version 11 files have no `colour` and no colour settings (the user said nothing, and the project follows the global colour settings: they load as they were); version 9 files have no groups and no badges (nothing in them is in a group), and what the program writes is always version 20 (version 20 added what phase 12 asks of the model, see `engine-phase-12-design.md`: the `sharing` of a track, `{ "picture": ..., "sound": ... }` with each of `per_clip`, `per_run`, `per_track_with_windows` or `per_track_indivisible` and a part that is `per_run` left out (the whole field is left out when both are); the `clip_tails` of a track, a list of `{ "clip": n, "edge": "around" | "into" | "cut", "depth": "behind" | "in_front" }` for the clips (by their place in the track, from 0) whose tail is not what a new clip's is, with a part that is the default left out; the `tail` of a clip group or a track group, `{ "edge": "keep_alive" | "release" | "cut", "depth": ... }`, left out when it is the default and for a tag, which has no edge and is refused if it has one; the `tail` of an effect, `"none"` or `{ "milliseconds": n }`, left out for the automatic one; and the `edge_fade` setting of a project, `"off"`, `"5_ms"`, `"10_ms"`, `"20_ms"` or `"50_ms"`, left out while it follows the global one; a file of version 19 has none of them and is read as one where every track has one copy of its effects for each run of clips and every tail is as a new one is; a file where a clip group with an effect that is switched on takes part of a track whose effects are indivisible, or where two groups would take overlapping windows, is refused with the reason; version 19 added the `state` of an effect, which a plugin's window made it save; version 18 made a text of an effect: a clip of `"kind": "text"` has a `content`, the name in its `effects` of its text effect, `builtin:text`, which makes its picture and has what it says (`text`), its `font`, its `size` and its `colour` for its values, **and has no `text`, `font_family`, `font_size_fraction_of_frame_height` or `colour` of its own**; a file of version 17 or earlier, which has those in the clip, is read as a clip whose text effect is the first of its chain, with the name that comes after the names its other effects have, so that they and the racks that name them stay what they were, and the badges that were before its first effect come after the text effect; the racks of keyframes of its `text`, `font`, `font size` and `colour` are those of the values of that effect; version 15 added the clip of `"kind": "adjustment"`, with its `duration_in_frames` and, like any clip, its `gap_before_frames`, `drawer`, `effects` and `badges`: a clip with no picture of its own whose effects are done to everything behind it, in its own group if it is in one; a file of version 14 has none, and an older program refuses a 15 with the message about a newer file; version 16 added the clip of `"kind": "generator"` (`gap_before_frames`, `duration_in_frames`, `content`: the name, in its `effects`, of the effect that it makes its picture with, `area` and `rotation_degrees` (left out when upright), and like any clip its `drawer`, `effects` and `badges`; a file that names a `content` that the clip does not have is refused), and the `shader` of a snapshot of an effect in `plugins`: `{ "language": "isf", "source": "..." }`, the whole text of the file that runs the effect, so that the project carries it (a shader of another language, or one that says more than these two and its images, is kept as it was and not run; a file of version 15 has none); version 17 added the `images` of a shader that brings images with it (an ISF file's `IMPORTED`): a list of `{ "name": "...", "data": "<base 64>" }`, in the order that the header lists them, each the bytes of a PNG or a JPEG file of at most 4 MiB, at most 6 of them; a shader that brings none has no `images`, and a file of version 16 has none; version 14 added the `plugin_libraries` of a project, see *Plugins and who may run them*; a version 13 file has none; version 13 added to the effects: `category` and, for each value, `scale` and `control` (a number, a choice with its options, a switch, or a resource of a kind) in the snapshot of an effect, `enabled` in an effect, which is left out while it is on, the `resources` list, and a `resource_storage` setting; a version 12 file has none of them and is read as it was); version 10 files (and version 11 as it was while the effects were being given names) call an effect by the name that the program had for it, `hue shift`, have their values in a list, have no `plugins`, and have no `instance` in their effects and no `effect` in their racks (they have at most one effect of each kind: the effects are named in the order they are in, and a rack of the keyframes of the values of an effect is of the effect that has a value of that name);
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
