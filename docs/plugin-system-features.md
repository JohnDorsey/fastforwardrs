# Plugin system: features list (draft 2: every question of section 19 is answered)

Status: **draft 2**. Draft 1 was written 2026-10-13; round 1 of the questions was answered 2026-10-13 and round 2 on 2026-10-15, so **no [Q] is left** (the list is finished in the sense of the next paragraph, and the plan of action is `plugin-system-plan-of-action.md`). It turns the long-term plan (`long-term-plan-plugins-and-gpu-engine.md`) and the maintainer's
answers of 2026-10-13 into a list of features that the final plan of action can be built from. It is *not* the plan: it says **what
the system must do**, not in what order, or how. Every line that is not yet concrete enough to plan from is marked **[Q]** and has a
matching entry in section 19 (open questions). The list is finished when no **[Q]** is left in a line that the plan depends on.

Background reading: the long-term plan (the data model, the engine options, the ffmpeg question) and the design notes
(`plugin-system-design-notes.md`: colour, node graphs, a render-server process, groups, test machines).

## How to read it

Each feature has an identifier (area letters and a number; the identifiers do not change when the list is edited), a status, a priority
and a sentence that can be checked.

| Status | Meaning |
|---|---|
| **[D]** | decided by the maintainer (the date is given) |
| **[P]** | proposed by me; it stands unless the maintainer objects |
| **[Q]** | not decided: a question in section 19 has to be answered before a plan can rely on it |
| **[S]** | shelved on purpose: written down so that it is not forgotten, and not planned |

| Priority | Meaning |
|---|---|
| **M** | must be there for the plugin system to be called done (the first release of it) |
| **S** | should be there for that release, and can slip |
| **C** | could be there |
| **L** | later: a release after that one; the design must not make it impossible |

**Status after phase 4 (2026-10-18; [V] means run, on the software adapters and the real ffmpeg 9.0.2; the report is `docs/engine-phase-4-report.md`).**
**Built:** SC-04 (the word is *effect* everywhere in the window), ID-01 to ID-06 and ID-08 (the descriptor has a category, and each value a scale, a control and a unit; the kinds of control are slider, whole number,
switch, choice and resource, and a number, a pair, a colour, a flag and words are the kinds of value; all eight built-in effects are descriptors, with snapshots in the file, and an effect that is not installed is kept whole),
MD-01 to MD-10 (the file is format 13), RS-01 to RS-03, RS-05 to RS-08 (resources, included or linked by a setting of the project or the global one, embedded as the default; two with one content are one resource; one
that is gone is *missing* and the effect passes the picture; read at open; 64 MiB and 256 resources and the checks of names), EF-01 and EF-07 (hue shift, saturation, brightness and contrast, exposure, opacity,
blur, crop, look-up table), UI-01 to UI-07 (the + menu with a search field and categories, controls made from the descriptor, the badge and note of a missing effect, bypass, drag to reorder, *Duplicate effect*, *Add effect
from file...*, the embedding setting in both settings windows), PH-01 to PH-04 and PH-20 (the `.cube` reader, with the Resolve variant read only; tetrahedral and trilinear interpolation; intensity, input space, output space;
held to ffmpeg's `lut3d` to 1 level at the worst of 4096 colours; the built-in effects run as passes of the engine), MG-02, LG-02 and LG-08 (five sample tables of our own, CC0, made by a formula), Q-12 (the export asks first
and lists the effects and the tables that are missing).
**Also built, 2026-10-18:** RS-04 (*Include table in project file* and *Take table out of project file* in the menu of the box of a look-up table effect, and for all tables at once) and the rest of SF-11 (64 effects on one clip, track or group: refused when added or copied, and in a file).
**Partly:** SF-03 (names of resources are checked as the names of media are), DS-01 to DS-03
(the sample tables are in the program's `assets/luts` and are chosen with *Add effect from file...*; **no folder is scanned** and there is no list of what was found: that waits for the plugin manager).
**Not built:** RS-09 and SF-02 (they are about shaders: phase 6), ID-07 (handles on the picture), UI-11 (the controls of EN-14: phase 6),
PH-05 (other table formats).

**Status after phase 7 (2026-10-19; [V] means run, on Windows and on Ubuntu in WSL with the real ffmpeg 9; the report is `docs/engine-phase-7-report.md`, the design `docs/engine-phase-7-design.md`).**
**Built:** AU-01 and AU-03 (LADSPA plugins are sound effects on clips, tracks and tags; the preview and the export make their sound with one block graph in the program, with an ffmpeg for each clip), AU-04 (a value with
keyframes is looked at every 32 samples; [V] the arithmetic is exact), AU-07 (the four channel layouts), AU-08 (a plugin that is not installed, not approved, not loadable, too slow in the preview or broken passes the sound through and keeps its
settings), AU-09 (the null test: the new graph is held to the old filter graph, which stays as the oracle; and the preview and the export make the same samples), AU-12 ([V] the 119 libraries of LMMS scan, and the 211 plugins that can be
effects were each put on a clip, played and exported in a process of their own), DS-04 (the scan is a program of its own, with a time limit, a cache and a breadcrumb), UI-12, SF-04, SF-06 to SF-10 (the severe question with the hold,
declining is safe, builds per platform, the cache folder with a hash check, the command line refuses), PL-04 (the endings of the libraries and the usual folders), project file format 14 (the plugin libraries).
**Changed by the maintainer (2026-10-19): SF-05 and SF-09 are by folder** (see SF-05).
**Partly:** AU-05 (a seek makes the sound again so a plugin begins as it begins; **no pre-roll**), DS-01 to DS-03 and DS-05 (folders to scan are a setting, with *Sound plugins...*; the list of what was found is in that window; **no
way to disable one type yet**: the plugin manager, phase 8), SC-01 (LADSPA is built; CLAP plugins are **found, offered as effects and run in the preview and the export with sample-accurate values and compensated latency**, pieces 7.8.1 to 7.8.4 of `engine-phase-7-clap-design.md`; the pre-roll of a tail is 7.8.5, built: a clip is given sound from before it for the plugins that remember, CLAP only).
**Not built:** AU-02 (CLAP), AU-06 (latency), AU-10 (generators, a master bus), effects of a clip group or a track group in sound. (The effects of an embedded plugin are built since 2026-10-21: AU-13.)

Words used with one meaning: an **effect type** is what a plugin or a built-in provides (its descriptor); an **effect instance** is one
use of it on a clip, a track or a group; a **host** is the part of the program that can run effect types of one family (the built-in
host, the LUT host, the ISF host, the LADSPA host, the CLAP host); a **resource** is a file an effect needs (a `.cube` table, a shader
source, an image).

---

## 1. Scope (SC)

* **SC-01 [D] M** The plugin families, in the order they are built: built-in effects; `.cube` LUTs; ISF shaders; LADSPA audio plugins;
  CLAP audio plugins. (2026-10-13: ".cube first, then ISF"; LADSPA was in the original request; CLAP is my addition, see AU-02.)
* **SC-02 [S]** OpenFX plugins are shelved (2026-10-13). Nothing in the model may *depend* on them; nothing is planned for them. (What
  the model keeps possible is in section 9 of the plan.)
* **SC-03 [D] M** The program is licensed GPLv3 (2026-10-13). Consequences are collected in LG-01 to LG-08.
* **SC-04 [D] M** (2026-10-15, Q-01) The user-facing word is **effect** (for pictures and sound alike); **plugin** is used only for what comes from a file
  or a library. A `.cube` table is a *resource* of a built-in effect, not a plugin.
* **SC-05 [D] L** Text objects become generator effects, eventually (2026-10-13). When is Q-17.
* **SC-06 [D] M** Clip groups and track groups, with effects of their own, are wanted (2026-10-13). Their place in the order is argued in
  the design notes, section 4; their features are in section 13.
* **SC-07 [P]** Not in scope of this list: masks and shapes with keyframed paths, motion tracking, time remapping beyond constant speed,
  scripting or expressions for parameters, a node-editor interface, VST3 and LV2 hosting, WASM plugins. Each may be asked for later; the
  model must not make them impossible. (Q-30, 2026-10-15: **no node editor yet, but both image-input parameters and a node editor must stay
  possible**: ID-10, EN-15.)

---

## 2. Effect types and descriptors (ID)

* **ID-01 [P] M** An effect type has a **key**: a string `kind:stable-id` with a namespace (`builtin:hue_shift`, `lut3d:...`,
  `isf:<name>`, `ladspa:<UniqueID>/<label>`, `clap:<id>`). The key never contains a file path and never an index. *Note: this changes
  one thing in the long-term plan: there the key of an ISF shader or a LUT contained a hash of its content. That makes every edit of a
  shader a different effect type, which contradicts ID-04. The hash is the **version**, not the key. See Q-08.*
* **ID-02 [P] M** An effect type has a **descriptor**: name, description, category, domain (picture or sound), role (filter, generator,
  transition, or for sound: insert, generator), input slots, parameters, resource slots, a statement of what it needs across frames, and
  the **colour domain** it works in (CM-10).
* **ID-03 [P] M** A **parameter** has a string id that is stable and unique in its effect type; a label; a group; a type (ID-05); a default;
  a range and step where they mean something; whether it can be animated; and the version of the effect type it appeared in.
* **ID-04 [P] M** Versions: an effect type has a version (major, minor). Same major means compatible: the instance uses the newest
  minor. A different major makes the instance behave as if the effect type were missing (ID-08) and says so. An alias table can say that
  a parameter id was renamed.
* **ID-05 [P] M** Parameter types, at least: number (with range, step, and a linear or logarithmic scale), integer, flag, choice (by id,
  with labels), pair (two numbers: point, size), colour (float RGBA, with a colour space: Q-06), text, resource (a file of a kind), and
  trigger (an event). Needed by ISF: float, bool, long, color, point2D, event, image. Needed by LADSPA and CLAP: number with scales, toggle,
  integer, choice. *Needs a decision:* font as a type (for text generators): Q-17.
* **ID-06 [P] M** The built-in effects are descriptors like any other (the hue shift becomes `builtin:hue_shift`, parameter `degrees`).
  There is no code path that is only for built-ins, except the host that runs them.
* **ID-07 [P] S** The descriptor of an effect type can say which of its parameters are **on-canvas handles** (a point on the preview that
  can be dragged). Q-20.
* **ID-08 [P] M** An effect type that is not installed is **missing**: its instances keep everything (section 3, MD-07).
* **ID-10 [D] M** (2026-10-15, Q-30) **Image-input parameters stay possible.** A descriptor's input slots (ID-02) can be *image inputs that are not the clip's
  own picture* (ISF `image` inputs; a matte, a displacement map, another layer's picture). The first release need not offer an interface that
  fills one from another layer, but the descriptor, the instance values and the engine's passes must be able to say "this input is that
  layer's picture" without a change of the file format's shape. This is what covers most of what a node editor is wanted for.
* **ID-09 [P] M** Each host turns what it reads (an ISF file header, a CLAP plugin's parameter list, a LADSPA port list) into
  descriptors, and reports why it could not (a parse error with a line number; a plugin that crashed while being scanned).

---

## 3. Instances and the project model (MD)

* **MD-01 [P] M** An **effect instance** has an instance id (unique in the project, like clip and rack ids), the key and version of its
  effect type, an enabled flag (bypass), parameter values keyed by parameter id, and an opaque `extra` value that is saved and not read.
* **MD-02 [P] M** A clip, a track, a clip group and a track group each have an **ordered list** of effect instances. Several instances of
  one effect type on one object are allowed (today there is at most one of each kind).
* **MD-03 [P] M** Picture effects and sound effects are in the same list and are applied by domain.
* **MD-04 [P] M** What identifies a parameter inside the program is *(instance id, parameter slot)*, so that keyframe racks, undo, the
  selection and drawers keep working with a `Copy` key; what is **saved** is the instance id and the parameter id (a string). Slots are
  worked out when a project is read. (The "slot trick" of the plan, section 3.3; to be confirmed by a spike before the plan relies on
  it: see QA-07.)
* **MD-05 [P] M** The project keeps **a snapshot of each descriptor it uses** (once per key and version), so that it can be read, shown and
  edited without the plugin being installed, and so that a project's model needs no registry.
* **MD-06 [P] M** The keyframes of a parameter belong to the project, are typed by the parameter's type, and are the program's own
  (racks, drawers, stopwatch buttons). A plugin is only given the value at the frame.
* **MD-07 [P] M** **Missing or changed effect types**: the instance is kept with all values and keyframes, is saved back unchanged,
  shows a badge, and renders as a pass-through. When the effect type is found again (same key, compatible version) it binds again. A
  changed range clamps on use and keeps the stored value. What an *export* does with a missing effect: Q-12.
* **MD-08 [P] M** Effect instances are copied with their clips and tracks (duplicate, copy keyframes), and a duplicate gets new instance
  ids.
* **MD-09 [P] M** Adding, removing, reordering, enabling and editing an effect instance and its values are steps of the history (undo
  and redo), like today's effect edits. Putting a resource in the project file is not one (as including a media file is not).
* **MD-10 [P] M** The project file (format 10) saves `plugins` (descriptor snapshots), instances with values by parameter id, racks that
  point at `(instance, parameter id)`, and `resources`. Format 9 is read (hue shift becomes `builtin:hue_shift`). An unknown field is
  kept and written back (so a newer program's file survives an older one).
* **MD-11 [D] M** (2026-10-15, Q-06) The space of a colour parameter is **always the working space** (so none is stored with the value), and colours are
  **interpolated in linear light**. A value that is in another space (an input space of a source) says so in its own field.
* **MD-12 [P] C** An instance can have a user-given name and a colour label.

---

## 4. Resources (RS)

* **RS-01 [P] M** A **resource** is a file an effect needs: a `.cube` table, an ISF shader's source, an image an ISF shader imports. It
  has an id, a kind, a link (absolute path, relative path, `included_as` entry name: the same shape as a source media link), and a content
  hash.
* **RS-02 [D] M** **Embedding is the default** for resources (2026-10-13): a shader or a LUT that is added to a project is put in the
  project file as `resources/<id>-<name>`.
* **RS-03 [D] M** The default is **configurable in the project settings and in the global settings** (2026-10-13). *Open details:*
  what the values are (embed / link / ask), which wins (the project's, I propose), whether the setting changes resources already in the
  project (I propose: no, only new ones), and what happens when the project's file is plain JSON, which cannot embed anything (Q-03).
* **RS-04 [P] M** Each resource can be switched between embedded and linked afterwards with the same two actions media has ("Include in
  project file", "Remove from project file"), and the project can say so for all of them at once.
* **RS-05 [P] M** Two instances that use one LUT use one resource (dedupe by content hash).
* **RS-06 [P] M** A resource that is missing (a linked file that is gone) makes its instances behave as missing effects (MD-07) and is
  listed in the same warning window as missing source files, with "choose the file again".
* **RS-07 [D] M** A resource is read into memory when the project is loaded; resources are small. (A 65-point 3D table is about 6 MB of
  text.) (2026-10-15, Q-26) **A limit on the size of one resource that is loaded into memory** (a table, a shader source, an image, a font): **64 MiB by
  default**, a number in the **global settings**. A bigger resource is not refused: the program **asks** ("this file is 300 MB; load it anyway?").
  The limit protects against a project that exhausts memory by accident or on purpose (a frozen load, a swollen autosave). It does not apply to
  media included in a project (read in place, never into memory), and it is not the limit of an embedded plugin binary (SF-10).
* **RS-08 [P] M** A resource in an embedded project is only ever found by its entry name inside the project file: an ISF shader that
  imports `texture.png` finds a resource of the project, never a path on the computer. (Safety: SF-03.)
* **RS-09 [P] S** When the project file has an embedded copy of a shader and the library has a different version of the same shader, the
  **embedded copy wins** (the project renders as it did), and the user is offered "update to the library version". (Q-08.)

---

## 5. Colour (CM)

*Maintainer, 2026-10-13: linear-light compositing is an important feature; wants to know about more colour spaces; asks whether ACES AP0
can be supported. The design notes answer that at length; here is what the system must do.*

* **CM-01 [D] M** Pictures are composited in **linear light** in a floating-point working format (`Rgba16Float`) (2026-10-13).
* **CM-02 [P] M** The **working colour space** is a project setting: its primaries and its (linear) encoding. At least linear
  sRGB/Rec.709 primaries (the default for new projects); proposed to be offered from the start: linear Rec.2020, and **ACEScg (AP1)**.
  Which exactly: Q-04.
* **CM-03 [P] M** Every source has an **input colour space** (primaries, transfer function, range, matrix for YUV): detected from the
  file's tags where there are some, overridable per source (a drop-down of known spaces), with an assumption for untagged files that is
  said in the interface (BT.709 for HD, BT.601 for SD: Q-05).
* **CM-04 [P] M** The picture on the preview is converted from the working space to the **display** by an output transform: at least
  sRGB display. Which display spaces: Q-04. *The window cannot take linear `Rgba16Float` directly* (egui expects gamma-encoded 8-bit and
  `register_native_texture` needs `Rgba8Unorm`): the engine ends with its own pass (display transform, then the display's encoding) into an
  8-bit texture that egui shows.
* **CM-05 [P] M** An export converts the working space to the **output colour space** the user chooses (at least Rec.709 with the
  BT.1886 transfer function, which is what the encoders' YUV is tagged as), and writes the right tags (primaries, transfer, matrix, range)
  into the file.
* **CM-06 [D] S** ACES: **AP0 (ACES2065-1)** is supported as an **interchange** space (a source's input space; an output space) and, as I propose,
  **not as the working space** (it has an imaginary blue primary: verified; ACES itself recommends AP1/**ACEScg** for compositing). ACES
  **view transforms** (the "ACES look": the 2.0 output transform is an algorithm that needs small hue tables, or can be baked to a 3-D table;
  OpenColorIO can bake it) are a **later** step that goes through the LUT host. What "support" means for the first release: Q-04 (defaults accepted
  2026-10-13). **EXR/DPX files are not in the first release** (Q-18, 2026-10-15: "not yet"); the working format and the input-space table are made so that
  they can be added (an image-sequence clip kind, 16-bit float upload, colour space and alpha handling) without rework.
* **CM-07 [P] S** Known transfer functions to start with: sRGB, BT.709, BT.1886, gamma 2.2/2.4/2.6, linear; later PQ and HLG (HDR) and
  camera log curves (Q-04).
* **CM-08 [P] M** The conversions are **fixed-function**: a 1-D curve and a 3x3 matrix per step, written once in Rust (a reference
  implementation, tested against published values) and once in WGSL (checked against the Rust one). Creative or view transforms that are
  not a curve and a matrix use a 3-D table.
* **CM-09 [P] M** Colour and its precision: no 8-bit stage between the source and the output in the picture path.
* **CM-10 [P] M** An effect type declares the **colour domain** it works in: *scene-linear*, *display-referred* (encoded with the
  sRGB curve, which is how most ISF shaders and today's hue shift are written), or *log* (named). The engine converts around the effect
  when the working space is not its domain. An instance can override it. (This is the main addition that linear-light needs: without
  it, every existing effect changes its look. See the design notes, section 1.)
* **CM-11 [P] M** **`.cube` tables** say in which space they expect their input and what space they produce (the format does not):
  two choices on the instance, default *display-referred in, display-referred out*.
* **CM-12 [D] M** Projects made before linear light (format 9) are **converted** (decided 2026-10-13, Q-05 option b): no legacy flag and no second mode; they
  load with the new colour handling and look slightly different (edges of scaled pictures, blends, and untagged HD video, which is read as BT.709 and not BT.601). The
  release notes say so.
* **CM-13 [P] L** HDR: PQ and HLG sources, an HDR-capable display, HDR export. The working format already holds it; the interface for it
  is later.
* **CM-14 [P] M** Colour **settings** live in: working space and input defaults in the *project* settings; the display transform in the
  *global* settings (it describes the monitor), with a project override only if asked for. (Q-04.)
* **CM-15 [P] S** A colour space list is data (a table of primaries, white point, transfer), so adding one is a row, not code.
* **CM-16 [P] M** A **colour-managed test**: every conversion pair has a round-trip test and a test against published numbers; the
  linear-light compositing of two known colours (50 % of white over black is 0.5 in linear light, 0.735 in sRGB code) is a test.
* **CM-17 [P] M** **YUV to RGB happens on the GPU**: ffmpeg decodes and does nothing to the colours (raw frames in their native pixel format, 8-bit
  or 10-bit), the engine reads the file's tags (ffprobe: primaries, transfer function, matrix, range; a missing tag means unspecified) and
  converts with the matrix and range that apply, or with the assumption of CM-03. **Verified today: ffmpeg converts untagged video with the BT.601
  limited-range matrix at every size, 4K included**, so today's preview and export show untagged HD material with the wrong matrix; the assumption
  chosen here changes the look of such files (Q-05).
* **CM-18 [P] S** Camera log curves as input spaces (ARRI LogC3 and LogC4, Sony S-Log3, Panasonic V-Log, RED Log3G10, Canon Log 3, Blackmagic Film Gen5):
  each is a published formula and a matrix (specifications or open implementations exist; nothing found suggests a licence problem), written as
  a row of the colour space table (CM-15). Which ones are wanted: Q-04.
* **CM-20 [P] M** **Parity before linear light.** The engine is first run in a **display-referred compatibility mode** that reproduces today's pipeline (so that
  parity with the oracle can be *proved*, EN-02); linear light is then switched on as **a separate, reviewed change** whose visible differences are listed
  (edges of scaled pictures, blends, blurs). Otherwise the port and the new look arrive together and neither can be checked.
  *(Phase 3, 2026-10-16: **status of the colour entries** [V on software adapters, WARP and lavapipe; the report has the tests]. **Built:** CM-01 (half-float premultiplied linear light, the working format
  of the engine), CM-02 and CM-14 (the working colour space, the display transform and the output colour space are rows of the settings table, global with a project override; Rec.709 linear and
  ACEScg), CM-03 (a file's tags and what is assumed where it says nothing, in one function; said in words in the window of the file, and changeable there), CM-04 (the display transform: sRGB,
  gamma 2.2, gamma 2.4), CM-05 (the output space and the tags of the file: matrix, primaries, transfer and range, always), CM-07 (sRGB, gammas, the Rec.709 camera curve, linear), CM-08 and CM-15 (a
  table of primaries and the conversions made from it, in Rust and in WGSL, the one held to the other), CM-09 (no 8-bit stage between the sources and the output: the export's planes are made from
  the half-float frame), CM-12 (old projects are converted: format 12, a fixture of format 11), CM-16 (round trips; published numbers: IEC 61966-2-1, BT.709, BT.2087, the ACES matrices), CM-17
  (the planes of 4:2:0 are turned into colours on the card, smoothly in linear light; **4:2:2, 4:4:4 and 10-bit are made into colours by ffmpeg, told the matrix and the range, not by the card**:
  the part of CM-17 not done), CM-20 (the compatibility mode is the "before" of the pictures in `docs/engine-comparison-phase-3`; the review is the gate). **Partly:** CM-10 (the hue shift is
  display-referred and runs in the right domain; a foreign effect would be given working-space numbers, and the conversion around it waits for phase 4). **Not built:** CM-06 (AP0 as an input or
  output space: the primaries are in the table, but there is no way to choose them yet), CM-11 (`.cube`: phase 4), CM-13 (HDR), CM-18 (camera logs), CM-19.)*
* **CM-19 [P] L** OpenColorIO as an **optional** tool (to read an OCIO config or to bake a view transform to a table): a C++ library with no WGSL output and no
  established Rust binding, so it is not on the path of the first release. **Later** (Q-35, 2026-10-15): the colour space table (CM-15) is data and baked
  tables are the common output, so nothing blocks it.

---

## 6. The engine (EN)

* **EN-01 [P] M** One renderer for the picture, on wgpu, used for **both the preview and the export** (the long-term plan's option A).
  ffmpeg stays as the decode and encode pipe. *(Phase 1, 2026-10-16 [V]: the **preview** can be made by the engine, as an option for now (`FASTFORWARDRS_PREVIEW=engine`). Phase 2, 2026-10-16 [V]: the **export** is made by the engine, always, from the same description of a frame
  (`frame_scene`: `layers_of_a_section`, `symbol_of_a_section`, `describe_frame`); ffmpeg only decodes the files and encodes the finished frames, and the old export (ffmpeg's filter graph for the picture) is deleted. Report: `engine-phase-2-report.md`.)*
* **EN-02 [P] M** Until parity is shown, the old path (the egui layers and the ffmpeg filter graph) stays as the **oracle**. Parity
  means: random projects compared frame by frame within a tolerance (one level on one machine, a stated larger one across vendors).
  *(Phase 1 [V], on the software adapters: the real old drawing against the engine on 400 random scenes, 2 levels at most everywhere but on the edges of layers; the one difference that can be seen, the hard
  edges of colours at some scales of a video, and the pictures that show them, are in `engine-phase-1-design.md`, sections 9 and 11, and `docs/engine-comparison/`.)*
* **EN-03 [P] M** Preview and export are **bit-identical on one machine** and within a tolerance across machines (documented).
  *(Phase 2, 2026-10-16: the window and the export make what a frame shows with the same functions and the engine draws it with the same passes, whichever of them asked. **[V]** The planes that the export is given are the preview's 8-bit frame converted, to within one level of the plain
  arithmetic, on 40 scenes made up from numbers (turned, translucent, solid, partly off the frame) in three of the six matrices and ranges. **[B]** That the 8-bit frame is byte for byte the same in the two modes follows from its being one render with a different read-back, and no test reads it back in planes mode. **Across machines, [V] for two
  software rasterisers only:** the same 40 scenes on WARP (Windows) and lavapipe (Linux): 98.5 % of the values are identical, the rest differ by one level except 84 of 670 288 values (80 by two levels, 3 by three, 1 by four), and the planes by two at most. A real card of another maker has not been measured (SF-01).)*
* **EN-04 [P] M** A render of a frame is a **graph of passes** built from the plan: decode -> upload -> colour conversion -> placement ->
  effect stack -> composite -> (group effects) -> track effects -> output transform. Textures come from a pool.
* **EN-05 [P] M** A **bad shader never hangs the program**: compile errors give a message with a line; a pass that takes too long, or a
  lost device, disables that instance and says which; the device is recreated. *(Spike S2, 2026-10-15: see plugin-system-spike-findings.md. Recommended guard: loop caps injected in the
  rewriting step, the engine's own device and thread, a watchdog with a timeout and recreation, a first-use probe at small size; on a software adapter the abandoned work cannot be
  cancelled and keeps using the CPU, so a render process stays an open option. Waiting for the maintainer's answer.)*
  *(Phase 1.6, 2026-10-16 [V]: built as recommended except the first-use probe (phase 6): loop caps in every shader that is given to the engine, the engine's own device and thread, a watchdog with a timeout and recreation, the effects of the frame that did not finish switched off for the run, three recreations within a minute stop the engine. Verified on the software adapters, **not** on a real card (SF-01):
  `engine-phase-1-design.md` section 6 says what was measured.)*
* **EN-06 [P] M** An effect that depends on earlier frames (ISF persistent buffers, feedback) is marked so in its descriptor; a seek
  re-simulates from the clip's start or a checkpoint; the export is sequential. The interface says "this effect depends on earlier
  frames". **Decided 2026-10-15 (Q-21): see EN-14.**
* **EN-14 [D] M** (2026-10-15, Q-21) **Preview of effects that depend on earlier frames** has two modes, a **global setting** (it describes the machine, not the
  project; **the export is never affected**: it renders every frame in order and is exact):
  *Bounded* (the default): the preview keeps **checkpoints** of the effect's state and, after a seek, simulates forward from the nearest earlier checkpoint
  (or the clip's start) for **at most 300 frames** (fixed); if none is that near, it shows the nearest state, marked "depends on earlier frames,
  approximate", until playback has caught up. *Warm-up only*: a seek resets the effect and it re-converges while playing (cheapest, least faithful).
  Two more global settings tune *Bounded*: the **checkpoint interval** (default every 60 frames) and the **checkpoint memory budget** (default 1 GiB; the
  oldest checkpoints are dropped when it is exceeded). They are not project settings and are not saved in a project.
* **EN-15 [D] M** (2026-10-15, Q-30) The engine's **graph of passes stays the compile target** of the effect stacks (EN-04), so that a node-graph model or
  a node editor can be built on top later without changing the engine (design notes, section 2.4). Nothing for it is built now.
* **EN-07 [D] M** Performance goals (2026-10-13: "acceptable for now"): a typical stack (four layers, two blurs and a LUT) plays at
  **720p60 on the integrated GPU** (**changed from 1080p60 by the maintainer on 2026-10-16**, after the measurements below, at 1080p, gave 44 to 50 frames to the second; 720p is the size that the preview is made at) and at **4K on a discrete GPU**. These are measured by a benchmark that is part of the repository,
  not by feel. (What happens when the machine cannot keep up: Q-13.)
  *(Phase 1.7 [V]: the benchmark is in the repository (`cargo run --release --example engine_benchmark`, QA-05). Measured by the maintainer on 2026-10-16 at 1080p: **integrated graphics (AMD Radeon, Vulkan): 49.8 frames to the second for the whole stack, so the goal is NOT yet made** (20.09 ms against 16.67; the estimates of what closes it are in `engine-phase-1-design.md`, section 10); RTX 3090 (Vulkan): 459.1. The 4K goal has not been run.)*
  *(2026-10-16 [V], the maintainer's run on the integrated graphics at 1080p, after phase 2: **50.3** frames to the second one at a time, **44.1** with a new video picture in every frame, **43.6** with two frames on their way; RTX 3090 459.1 one at a time. The benchmark's default size and the line that says whether the gate is made are 720p now; whether 720p makes 60 is not measured yet [B]: about 100 follows from the pixels, and the maintainer is asked to run it.)*
  *(Phase 2 [V]: the benchmark has a scene in which every frame is given new pictures of video, and a second table with two frames on their way; on the software adapter (WARP) at 640 by 360, 30 frames of each scene, the whole stack makes **8.7 frames to the second** with its pictures already on the card and **5.8** when every frame is given new pictures of video (5.9 with two frames on their way: a processor does not do two things better than one), which says nothing of the gate. The maintainer is asked to run it again on the integrated graphics.)*
* **EN-08 [D] S** (Q-13, default accepted 2026-10-15) Preview quality is adaptive: at full speed on a slow machine the engine renders at a lower resolution rather than
  dropping frames, and a still frame (paused, scrubbing stopped) is rendered at full resolution. *(Phase 1.7 [V]: built (`engine/quality.rs`, used by the preview): whole, half and a quarter of the size; the numbers
  are the design's first guesses, to be tuned with the benchmark on a real card.)*
* **EN-09 [P] S** Rendered frames are cached (the frame at the playhead, and frames around it) and the cache is dropped when an edit
  changes what they show. *(Phase 1 [V]: the engine keeps the frames it made, by what the description of a frame is made of (32 frames or 256 MiB), and clears them when a picture or an effect that they depend on is replaced or the graphics
  device is; the preview counts the frames that came from it.)*
* **EN-10 [P] M** Export reads frames back from the GPU in a pipelined way and converts to the encoder's YUV on the GPU first.
  *(Phase 2, 2026-10-16 [V] on the software adapters: the frame is converted to the planes of `yuv420p` on the card (a 2 by 2 average for the colour, limited range, the matrix of the first video) and read back with two frames in flight in the engine and up to four asked ahead, so that the decoders, the card and the encoder all work at once.
  Not measured on a real card (SF-01).)*
* **EN-11 [P] C** A render of a project can be started with no window (a command-line export), which also gives the engine tests a way
  to run. (It is what option D, a render process, would also need: Q-31.)
  *(Phase 2, 2026-10-16 [V]: `fastforwardrs-export [--adapter best|software] [--settings FILE] PROJECT VIDEO`, a program of its own because the editor is a windowed program on Windows; exit status 0, 1 or 2; the progress on the error stream. It reads `.json` and `.ffrs` projects, including a video that is inside the project file. **`--adapter best` has never been run by me** (SF-01); the tests use `--adapter software`.)*
* **EN-12 [P] M** The engine's interface is **message-shaped**, so that a render process stays possible later (the design notes, section 3): the
  scene is an **immutable, serialisable value** (what to render, parameter values, times, sizes; resources referred to by content hash; no
  UI types, no pointers into the program's data); the entry is `request a frame(scene snapshot, time) -> frame`; GPU resources are addressed by
  id; there is a path that **rebuilds all GPU state from the scene** (needed anyway after a lost device); the engine runs on a thread of its own
  behind channels first. A command-line export (EN-11) is the first user of that shape.
* **EN-13 [S]** OpenFX's CPU path is not designed for now.

---

## 7. Kinds of effect (EF)

* **EF-01 [P] M** **Filters**: take the picture of the clip (or of the track, or of a group), give a picture.
* **EF-02 [P] M** **Generators** *(2026-10-21, maintainer: **no built-in generator, not even a solid colour**; **built 2026-10-22 as a clip whose first effect is an ISF shader that reads no picture**, phase 6.5; the text generator is not)*: take no picture; give one (ISF source shaders; later the text generator). A generator clip has no media,
  has a duration the user sets (like a picture clip, **5 seconds by default, and it can be dragged longer**: Q-11, 2026-10-15), and its content is an effect instance.
* **EF-03 [D] M** **Transitions** are effects with a **backdrop** input on the front clip (2026-10-13: acceptable): the clip is the
  "end" picture, what is composited behind it is the "start" picture, and `progress` is a number. (Q-09: where `progress` comes from.)
* **EF-04 [P] S** Other users of the backdrop slot: blend modes, glass and blur-behind effects, adjustment-style effects.
* **EF-05 [P] M** **Sound effects** (inserts on a clip or a track; generators later): in section 12.
* **EF-06 [D] L** The **text object** becomes a generator effect with the parameters it has today (text, font, size, colour), as a
  CPU-drawn picture uploaded to the GPU; the **text generator** is therefore a "CPU image producer", a second kind of host next to the
  shader hosts (Q-17).
* **EF-07 [P] S** A minimum set of **built-in effects** in the box, since a plugin system with one built-in effect is hard to judge and
  to test: Q-10 asks which.
* **EF-08 [P] C** Presets: a saved set of parameter values for an effect type.

---

## 8. Animation of parameters (AN)

* **AN-01 [P] M** Every parameter that can be animated can have keyframes in a rack, with the stopwatch button, in the drawers, exactly
  as today's properties.
* **AN-02 [P] M** Interpolation follows the type: numbers, pairs and colours are interpolated linearly (colours in the working space,
  in linear light: MD-11), choices, flags and text hold, triggers fire once at their frame.
* **AN-03 [P] C** Per-keyframe interpolation (hold, linear, ease); today there is one rule per type.
* **AN-04 [P] M** Editing an animated value pauses playback (as today).
* **AN-05 [D] L** (2026-10-15, Q-22) Audio-reactive parameters (ISF `audio` and `audioFFT` inputs, about 8 of 327 shaders in the corpus): **not in the first
  release**; such an input reads as silence, and the shader says so.
* **AN-06 [P] M** A transition's `progress` and similar "over the length of the effect" values: Q-09.
* **AN-07 [P] L** Linking one parameter to another, expressions: not now (SC-07).

---

## 9. Interface (UI)

*The rule from the earlier rounds applies: **no always-visible control is added unless it is asked for.** Everything below that is not a
menu item or a window opened from one is marked.*

* **UI-01 [P] M** An effect is added with the **+ button** that exists (the menu of effect types), which gets **categories and a search
  field** because the list will have hundreds of entries. (A new control inside an existing menu; accepted 2026-10-15, Q-14.)
* **UI-02 [P] M** The **properties panel** shows an instance's parameters with controls made from the descriptor: sliders, number fields,
  pairs, colour pickers, choice boxes, toggles, a button for a trigger, a file chooser for a resource; grouped as the descriptor says;
  each with the stopwatch.
* **UI-03 [P] M** A missing effect shows a badge and its saved parameters (from the snapshot), not an empty box; a failed one shows its error.
* **UI-04 [P] M** An instance can be disabled (bypass), reordered (drag), duplicated, removed, from the box's menu.
* **UI-05 [P] M** "Add effect from file..." in the same menu: choose a `.cube` or an ISF file; it becomes a resource and an instance
  (embedded or linked by the setting).
* **UI-06 [D] M** The **embedding default** is a setting in the project settings window and in the global settings window (RS-03).
* **UI-07 [P] S** The **colour settings** (working space, input space of a source, display transform, output colour space of an export)
  are in the project settings, the clip's properties and the export window: no new always-visible controls.
* **UI-08 [D] L** On-canvas handles for point parameters (ID-07), drawn by the preview's transform-box code: **later** (Q-20, 2026-10-15).
* **UI-09 [P] C** A window that lists the effect types that were found and those that failed, with the reason (the plugin manager).
* **UI-11 [D] M** (2026-10-15) The **global settings window** gets the controls of EN-14 (mode, checkpoint interval, checkpoint memory budget), the
  resource size limit of RS-07, and the folders of DS-02. No project setting is added for any of them.
* **UI-12 [D] M** (2026-10-15) **The prompts about plugins** (SF-05 to SF-09) are modal windows, shown only when a project that needs them is opened or a plugin
  is added: no always-visible control.
* **UI-10 [P] M** All new texts follow the long-descriptive-name and plain-wording conventions of the existing windows.

---

## 10. Finding and managing effect types (DS)

* **DS-01 [D] M** Where effect types come from: **(a)** library folders scanned at start (a folder per family, per operating system), **(b)**
  files added to a project ("Add effect from file...", UI-05), **(c)** both. (c) was chosen (2026-10-13, Q-02).
* **DS-02 [P] M** The folders: a program folder (what is shipped), a user folder (`%APPDATA%`, `~/.config`, the usual place per system),
  and folders the user adds in the global settings.
* **DS-03 [P] M** Scanning happens in the background and never blocks the window; the result is cached and refreshed when a folder
  changes (or by a button in the plugin manager).
* **DS-04 [P] M** Scanning a **binary plugin** (LADSPA, CLAP) happens in a **child process**, so that a plugin that crashes while it
  loads does not take the editor down; one that crashed is listed as "failed" and is not tried again until it changes.
* **DS-05 [P] S** An effect type can be disabled in the manager (it is then treated as missing).
* **DS-06 [P] M** Duplicate keys (two folders with the same shader name): the user folder wins over the program folder; the manager
  shows both.

---

## 11. Picture hosts (PH)

### 11.1 The `.cube` LUT host

* **PH-01 [P] M** Reads Adobe `.cube` 1D and 3D tables with `TITLE`, `DOMAIN_MIN/MAX`, `LUT_1D_SIZE` (2 to 65536), `LUT_3D_SIZE` (2 to
  256); says what is wrong with a file that it refuses (with the line). **The Resolve variant** (a 1-D shaper followed by a 3-D table, with its own input-range
  keywords) **is read, and never written** (Q-23, 2026-10-15); the shaper is applied as a first pass.
* **PH-02 [P] M** Applies a 3-D table on the GPU with **tetrahedral** interpolation; a 1-D table with linear interpolation; the input is
  clamped to the domain and the output is not clamped to 0 to 1.
* **PH-03 [P] M** Parameters: *intensity* (a mix with the input, 0 to 1), *input space*, *output space* (CM-11), *interpolation*
  (tetrahedral or trilinear).
* **PH-04 [P] S** Matches ffmpeg's `lut3d` tetrahedral within a tolerance (the test of the plan, phase 3a).
* **PH-05 [P] S** Other table formats later (Hald CLUT image, `.3dl`, `.csp`): L.

### 11.2 The ISF host

* **PH-10 [P] M** Reads ISF 2.0 files: the JSON header (tolerantly: duplicate keys and an integer `ISFVSN` occur) and the GLSL source;
  turns the header into a descriptor (ID-02) with the parameter mapping float -> number, bool -> flag, long -> choice, color -> colour,
  point2D -> pair, event -> trigger, image -> input slot.
* **PH-11 [P] M** Compiles the source through a small rewriting step and naga into WGSL, in a worker thread that survives a panic, and
  validates it. Compile errors are shown with line and column.
* **PH-12 [P] M** Multi-pass shaders (`PASSES` with `TARGET`, `WIDTH` and `HEIGHT` expressions) and persistent buffers (EN-06); `IMG_*`
  macros; `TIME` is clip-local time and `FRAMEINDEX` clip-local frame number, never the wall clock; the y-axis flip.
* **PH-13 [P] M** Filters, generators and transitions are told apart by their inputs (`inputImage`; none; `startImage`, `endImage`,
  `progress`) (EF-01 to EF-03).
* **PH-14 [P] M** Imported images (`IMPORTED`) are resources (RS-08).
* **PH-15 [P] M** A **golden corpus** of ISF shaders with expected pictures from a reference host guards correctness; the compile rate
  alone is not accepted as correctness. **The reference host is the ISF Editor on Windows, run by hand, for 10 to 20 shaders** (Q-24, 2026-10-15).
* **PH-16 [D] M** What ships in the box (Q-25, 2026-10-15): **gl-transitions and the clearly MIT original Vidvox shaders, our own LUTs, nothing else**; the
  licence policy of LG-02 decides anything beyond that.
* **PH-17 [P] L** `audio` and `audioFFT` inputs (AN-05).

### 11.3 Built-in host

* **PH-20 [P] M** Runs the built-in effects (hue shift, and the others of EF-07) as WGSL passes; each is a descriptor plus a shader.
* **PH-21 [P] C** The same descriptor-plus-WGSL form is offered to users as the program's own plugin format (no ISF shim needed).

---

## 12. Sound hosts (AU)

* **AU-01 [D] M** LADSPA plugins can be used as sound effects on clips and tracks (the original request), **before** CLAP (2026-10-13).
* **AU-12 [D] M** The first test set of real plugins is the **LADSPA folder of the maintainer's LMMS 1.2.2 install** (Windows builds: the swh, TAP, CMT and
  CAPS collections, Calf and others; 119 DLLs). **Verified by me (a read-only byte check): all 119 are 64-bit x64 and contain `ladspa_descriptor`**, so they can be
  loaded by the 64-bit program; whether each needs other DLLs of the install (the C runtime) is untested. Most of these collections are GPL (not
  checked file by file): the program **does not bundle them**, it finds them through a folder the user adds (DS-02) or the `LADSPA_PATH` convention. A
  zero-byte or damaged DLL in a folder is a test case for DS-04 (a plugin that fails to load is listed as failed, never fatal).
* **AU-13 [D] M** (2026-10-20) **Built 2026-10-21 for plugins inside a project file** (see `plugins-and-trust.md`: written out after the severe question, scanned in the background, installed for as long as the project is open; the command line does it with `--allow-plugins`). It was: an effect of a plugin works only if the same plugin is **installed from a folder** that was scanned (an embedded plugin is asked about, but nothing makes an installed plugin of it); that was temporary, for LADSPA and for CLAP. **At some point both are to be usable without installing them**: a plugin that comes inside a project file, or with the program, is found, scanned and run without a folder to add. (Bundling plugins with the program needs their licences to allow it: **searched 2026-10-20, see `bundleable-clap-plugins.md`**: permissive ones fit LG-02, the copyleft ones (Dragonfly Reverb, most of DPF-Plugins, Surge XT Effects, LSP) need a decision to ship their binaries with the source offer, as ffmpeg does; **built the same day: `bundled-plugins/manifest.json` and `tools/bundled_plugins/`; the folder `plugins` beside the program is looked in last and trusted; no release has been made, so nothing is distributed yet.**)
* **AU-02 [P] S** CLAP plugins as well, because LADSPA binaries are almost only found on Linux; CLAP is the format users on all three
  systems have. **Decided 2026-10-13: LADSPA first, then CLAP** (Q-27).
* **AU-03 [P] M** Sound plugins run **in the program**, in one block graph that the speakers and the export both call (not through
  ffmpeg's `ladspa` filter, which is not available on every system). This replaces today's two ffmpeg graphs for sound, with the
  old one kept as the oracle until the mixes match.
* **AU-04 [P] M** Parameters are changed **sample-accurately**: blocks are split at keyframe changes or every 32 to 64 samples; gain-like
  parameters are smoothed.
* **AU-05 [P] M** A seek **resets** the plugins and runs a pre-roll so that tails and filters are in a sane state.
* **AU-06 [P] S** Plugins that report latency are compensated (CLAP can; LADSPA has no standard way).
* **AU-07 [P] M** Mono plugins are instantiated once per channel; channel layouts are handled and said.
* **AU-08 [P] M** A missing sound plugin passes the sound through unchanged and keeps its settings (MD-07).
* **AU-09 [P] M** Preview and export give the same sound on one machine (null test against each other).
* **AU-10 [D] L** Sound generators; a master bus with effects: **later** (Q-28, 2026-10-15). Adding it later is one more node at the end of the shared
  block graph (AU-03); its place in the window would be a new control that needs the maintainer's approval first.
* **AU-11 [D] M** (2026-10-15, Q-29) Time stretching (the "maintain pitch" behaviour of sped-up clips) stays ffmpeg's `atempo` until a replacement has been
  judged by ear and the tests re-baselined.

---

## 13. Groups (GR)

***Superseded 2026-10-13:** groups are specified by the maintainer in `groups-feature-spec.md` (three kinds: scattered group = tag, contiguous group = clip group, track group), and are built **before** the rest of the effects system. Where this section disagrees with that document, that document wins; GR-03 and GR-05 are answered by it, GR-09 and GR-10 stay.*

***Built 2026-10-13** (details in `groups-feature-spec.md`, section 7): the three kinds of group, their rules, the properties panel, the outlines, the file format (version 10). **GR-02 and GR-10 were a placeholder until 2026-10-21 (phase 5): the effects of a clip group or a track group are done to the picture that the members make together now, a group with no effect that does anything is pass-through, and every picture effect can be on a group** (`engine-phase-5-design.md`; the old drawing of the window leaves a group's effects out). A tag is exact (its effects are done to each member).*

* **GR-01 [D] M** There are **track groups** and **clip groups**, and each can have effects of its own (2026-10-13).
* **GR-02 [P] M** The effects of a group apply to the **composite of what is in the group** (not to what is behind it).
* **GR-03 [D] M** (superseded by the three kinds of `groups-feature-spec.md`; kept for history) A **clip group** is (a) a **compound clip**: an opaque clip that has a timeline of its own with tracks and clips, which can
  be opened, or (b) a **selection group** (moves and is cut together, and can take an effect that applies to the composite of exactly its
  clips). Prior art: Kdenlive's groups are (b) and its nested "sequences" are (a); Premiere, Resolve, Final Cut and After Effects have (a). (a) is the
  largest piece of the group work by far. Q-15.
* **GR-04 [P] M** A **track group** contains tracks and behaves as one layer; it can be collapsed, muted, soloed (if there is solo),
  and has a sound **sub-mix** with a gain.
* **GR-05 [P] M** Groups can be nested; a depth limit is allowed (Q-15).
* **GR-06 [P] M** A group's effect instances are ordinary instances (MD-01), animated with the same racks.
* **GR-07 [P] M** Grouping and ungrouping are steps of the history and do not change what the picture and sound are (a test: ungroup
  gives the same render).
* **GR-08 [P] S** The model of groups is built on the descriptor-driven effect model, never on the closed enum (the order: design
  notes, section 4).
* **GR-09 [D] S** **Adjustment clips** *(built 2026-10-21, phase 5.3)*: a clip with no picture of its own whose effects apply to the composite of everything behind it, for the
  length of the clip. Under the accepted transition model (EF-03) it costs almost nothing (a filter whose input is the backdrop), it is in
  Blender, MLT, Resolve and After Effects, and it is the cheapest first use of the backdrop mechanism. Not asked for by the maintainer; accepted when proposed (2026-10-13, Q-32), to be built after the engine.
* **GR-10 [P] M** A group is **isolated** (its children are composited together first, then the group's effects and blend mode apply) or
  **pass-through** (its children are composited straight into what is behind). The model has the flag from the start; the renderer treats a
  group with no effects and no blend mode as pass-through (free). **Decided 2026-10-15 (Q-33): the choice is hidden.** A group is isolated if it has
  effects, an opacity below 100 % or a blend mode, and is pass-through otherwise. A user-visible "isolate" property can be added later without a change
  of the file format.

---

## 14. Safety (SF)

* **SF-01 [P] M** A shader that does not finish cannot freeze the window (EN-05). It is run under a watchdog, and it is **never
  tested on the maintainer's own GPU**; the test is on a software adapter or a machine set aside for it.
* **SF-02 [P] M** A project from elsewhere that embeds shaders can do no more than a shader can do: draw, and waste time (EN-05). It
  cannot read files, because the resource lookup is by entry name only (RS-08).
* **SF-03 [P] M** Names of embedded resources are checked as today's are (no path parts, no control characters, a length limit).
* **SF-04 [D] M** A binary plugin (CLAP, LADSPA) is **trusted native code** that runs with the program's rights and is not sandboxed. *Decided 2026-10-15
  (Q-26), replacing the earlier proposal that a project could only name such a plugin:* **a project file may name a plugin (linked) and may contain one
  (embedded)**, and each has its own prompt (SF-05, SF-06). Data (an ISF shader, a `.cube` table, an image) is not native code and has **no prompt**: the
  worst a shader can do is waste time on the GPU, which EN-05 guards against.
* **SF-05 [D] M** **A linked plugin** (named by a path) gets a **warning prompt** the first time it is needed, **remembered per plugin by its path and a
  hash of the file**: if the file changes, the program asks again. **Changed by the maintainer, 2026-10-19: one question for each new folder a plugin or a library comes from, remembered for the folder**; what is in the folder, or comes to be, is not asked about
  again, and a file that changes is not asked about again. (Not for a plugin inside a project file: SF-06 is unchanged.)
* **SF-06 [D] M** **An embedded plugin** is **never loaded without a concise, severe prompt**, shown **every time the project is opened** (never
  remembered, also not after a crash recovery of an autosave), with two buttons: *Don't load* (the default) and *Load it*. *Load it* can only be accepted by
  **holding it down for 5 seconds with the mouse, or holding Enter (never Space), while the button visibly fills up**; letting go resets it. The draft
  text: *"This project contains a program: «name», for «operating system». It runs with all your rights and nothing checks what it does. Load it only if you
  trust where this project came from."*
* **SF-07 [D] M** **Declining is safe**: the instance is a missing effect (MD-07): it passes the picture or the sound through, keeps its settings, shows
  a badge that says why, and is saved back unchanged.
* **SF-08 [D] M** An embedded plugin can carry **several builds, one per platform** (a binary only works on one operating system and processor). The
  prompt names the build that would be loaded; a project with no build for this platform shows the plugin as unavailable and says which platforms it has.
* **SF-09 [D] M** **Command-line export** (EN-11) cannot show a prompt: it **refuses** a project that needs an embedded plugin, or a linked plugin that
  is not approved, and lists them by name, unless it is given an explicit flag (`--allow-plugins`).
* **SF-10 [D] M** (2026-10-15) A native library cannot be loaded straight out of a zip, so an embedded plugin is **extracted to a cache folder** after the prompt, checked
  against its recorded hash, loaded, and its file removed when the program ends. (Antivirus software may complain; the manager says so.) Its size has a
  limit of its own, **256 MiB** (confirmed 2026-10-15), separate from RS-07, because plugin binaries are larger than resources.
* **SF-11 [P] S** Limits: the size of one resource, the number of instances on one object, the number of passes of one shader, the size
  of one intermediate texture.

---

## 15. Compatibility and migration (MG)

* **MG-01 [P] M** Format 9 projects open in the new program and look **nearly** as before: the differences are the ones CM-12 lists, which the maintainer accepted (2026-10-13).
* **MG-02 [P] M** A project saved by the new program is not expected to open in the old one, and says so clearly (the "newer file
  format" message exists).
* **MG-03 [P] M** Every change of the saved form has a version and a reader for the old one, and a fixture file of each version in the
  tests.
* **MG-04 [P] M** The refactor that opens up effect identity (phase 0) is done **behind the existing tests**: the behaviour tests of
  effects, racks, undo and the plan stay and pass unchanged, and only the tests that assert "at most one effect of each kind" and the
  file-format tests change on purpose.

---

## 16. Platforms (PL)

* **PL-01 [D] M** Windows first (2026-10-13). The program already runs here; every feature must work here.
* **PL-02 [D] S** Linux eventually (2026-10-13), tested in WSL or a virtual machine (design notes, section 5). **WSL2 with Ubuntu is installed (2026-10-15)**,
  and the GitHub repository is public (hosted CI is free). "Supported" means (Q-19, default accepted 2026-10-13): Linux: the tests pass; macOS: it builds in CI.
* **PL-03 [D] L** macOS far in the future (2026-10-13): no Mac to test on, and a macOS virtual machine on other hardware is not permitted. Nothing in the
  code may assume Windows; the cheap check is a **hosted runner** (GitHub's `macos-latest`, free for a public repository) that builds and runs the headless
  tests; GPU behaviour there stays unmeasured (Q-19).
* **PL-04 [P] M** Per platform, the plugin folders and binary suffixes are the usual ones (`.dll`/`.so`/`.dylib`; CLAP `.clap`).
* **PL-05 [P] M** No feature may assume a particular GPU vendor; the software adapter must run everything (slowly).

---

## 17. Testing and quality (QA)

* **QA-01 [P] M** Each host has **conformance tests** with small known inputs and expected outputs (a table of 8 pixels through a LUT;
  a trivial shader; a gain plugin with a sine).
* **QA-02 [P] M** The engine is compared with the oracle on random projects (EN-02), with a tolerance and a budget of differing pixels.
* **QA-03 [P] M** The **golden corpus** for ISF (PH-15).
* **QA-04 [P] M** The mutation campaigns of earlier rounds continue; the **tooling moves into the repository** and works on every
  platform (agreed 2026-10-12): see the list of agreed preparatory work in the plan, phase 0.
* **QA-05 [P] M** Performance benchmarks (EN-07) are in the repository and can be run on any machine.
* **QA-06 [P] M** The whole suite runs on **Linux** (WSL2 or a VM; Linux uses a software Vulkan adapter, which is what the tests want) and on **hosted CI
  for Windows, Ubuntu and macOS**; picture comparisons have per-system tolerances. **WSL2 and VirtualBox on one machine slow each other down**: choose one
  (WSL2 was chosen and is installed, 2026-10-15). Q-19.
* **QA-07 [P] M** A **spike** (a day or two, throwaway) confirms the parameter-slot design (MD-04) against the real code before the plan
  relies on it, and another confirms one ISF shader through naga to a picture, and one deliberate hang on a software adapter.

---

## 18. Legal and licensing (LG)

*A technical survey, not legal advice: design notes, section 6.*

* **LG-01 [D] M** The program is **GPLv3** (2026-10-13). Whether "or later": Q-34 (I propose `GPL-3.0-or-later`, which is what the FSF recommends and
  what the dependencies already force at least version 3 for).
* **LG-02 [P] M** Shaders, LUTs and other content that **ships with the program** are under **MIT, BSD, ISC, Apache-2.0, CC0, Zlib or CC BY**, or are made
  by us under GPLv3, with the licence written in the file's own header or next to it, and a **manifest** (name, author, licence, URL). Files
  with no licence are rejected; anything non-commercial or share-alike with non-commercial terms is not bundled (**Boxinator** and the
  Shadertoy-derived shaders of the ISF collection are out; **gl-transitions** are in, with notices). The user can load any shader or table from disk;
  that is not bundling. PH-16, Q-25.
  **Amended 2026-10-20 (the maintainer's decision, for what ships with the program, plugin binaries included):** content under **GPL and LGPL licences that are compatible with
  GPLv3** may be bundled as well (GPL-3.0, GPL-3.0-or-later, GPL-2.0-or-later, LGPL-3.0, LGPL-2.1-or-later; **not GPL-2.0-only**, which is not compatible with GPLv3), **on the
  conditions of LG-03**: the licence text next to it, the copyright notices kept, nothing altered or renamed, the **corresponding source of that exact version** offered with
  the release, and a line in the manifest (name, author, licence, URL, version, where the source is kept). **AGPL is not bundled** (the maintainer's decision), nor
  anything non-commercial, share-alike with non-commercial terms, or without a licence.
* **LG-03 [P] M** ffmpeg: a **GPL build (with `--enable-version3`) may be used and shipped** (it is GPLv3-or-later; **verified for the build on this PC**), never a
  `--enable-nonfree` build; **the corresponding source of the ffmpeg we ship is offered** (hosted by us).
* **LG-04 [P] M** Hosting plugins that are not GPL (CLAP, LADSPA, LV2, VST3): the formats are all licensed compatibly. Whether the project adds an **explicit
  statement that plugins loaded through a third-party API are not derivative works** (Ardour's "Plugin Clarification"), which only the
  copyright holder can add and which is easy for a sole author now: Q-34.
* **LG-05 [P] S** The plugin API of the program's own format (PH-21) is documented with a GPLv3-compatible example.
* **LG-06 [D] M** (done 2026-10-15) The mechanics, as a task of phase 0: a root `LICENSE`; `license = "..."` in `Cargo.toml` (missing today); an SPDX line in each source
  file (checkable with `reuse lint`); a `THIRD_PARTY` notices file made by `cargo-about` or checked by `cargo-deny`; the ffmpeg source offer; the manifest
  of LG-02.
* **LG-07 [P] S** Check the one dependency whose licence is not on the FSF's list: the **Ubuntu Font Licence** of egui's default fonts (ship those
  fonts as separate files with their notices, or choose others).
* **LG-08 [P] S** Our own LUTs for the sample set (identity, warm, cool, teal-orange) are made by us and released under GPLv3 (the licence-clear alternative to
  taking someone's pack); a pack under **CC BY 4.0** (Q-DDL, reported) is possible once its licence file is read.

---

## 19. Open questions

*Round 1 (the ones the data model, the first phases and the file format depend on) was asked in the chat on 2026-10-13. **A1** = answered
by accepting my default (2026-10-13); **A3** = round 2, answered 2026-10-15 ("default" there means my default was accepted). **Every question is answered.***

| # | Round | Question | My default / the answer |
|---|---|---|---|
| Q-01 | A3 (default accepted) | Terminology: **effect** (what the user sees), **effect type**, **effect instance**, **host**, **resource**; **plugin** only for what comes from a file or library. | yes |
| Q-02 | A1 (default accepted) | Where do effect types come from: **library folders** scanned at start, **files added to the project**, or both? | both |
| Q-03 | A1 (default accepted) | Embedding setting: values *embed / link / ask*; project beats global; changes only resources added later; with a **plain-JSON project file** (which cannot embed) it falls back to link with a note, or asks once to save as `.ffrs`? | embed/link/ask; project beats global; new ones only; ask once to convert |
| Q-04 | A1 (default accepted) | Colour spaces for the **first release**: working (which), source inputs (which, including log cameras, PQ/HLG, ACES2065-1), display, export outputs; and is the ACES **view** transform (the look) wanted, or only the space conversions? | working: linear Rec.709 primaries and ACEScg; inputs: Rec.709/sRGB, Rec.2020, ACES2065-1 (as numbers; no EXR files yet); display sRGB; export Rec.709; logs and HDR later; view transform later through a baked table |
| Q-05 | A2 | Projects made before linear light, and the untagged-video assumption | **answered 2026-10-13: option (b): old projects are CONVERTED (no legacy flag); they get the new colour handling and look slightly different; new and old alike assume untagged HD is BT.709, SD is BT.601** |
| Q-06 | A3 (default accepted) | Colour parameters: their space is always the working space, and colours are interpolated in linear light. | yes |
| Q-07 | A1 (default accepted) | The **hue shift** (and other existing effects) run in which domain: display-referred (keeps today's look) or linear? | display-referred; new effects declare their own |
| Q-08 | A1 (default accepted) | File-based effect types: **key = the name, version = a hash of the content**; the **embedded copy wins** over a library copy of the same name, with an "update to the library version" offer. | yes |
| Q-09 | A1 (default accepted) | Transitions: where does `progress` come from: a keyframed number only, or **derived from a start and end frame on the effect** (with a keyframed override)? | derived from a range, with an override |
| Q-10 | A1 (default accepted) | Which **built-in effects** ship in the first release besides hue shift? | brightness/contrast, saturation, exposure, blur, crop, opacity, LUT |
| Q-11 | A3 (default accepted) | A generator clip's default length (5 s like a picture) and whether it can be dragged longer. | 5 s, extendable |
| Q-12 | A1 (default accepted) | Export when an effect is missing: refuse, warn and go ahead, or ask? | refuse, listing what is missing, with an "export anyway" button |
| Q-13 | A3 (default accepted) | When the machine cannot keep up in the preview: lower resolution, drop frames, or both? | lower resolution first, then drop |
| Q-14 | A3 (default accepted) | The + menu grows to hundreds of entries: **categories and a search field** inside that menu are acceptable (a new control in an existing menu)? | yes |
| Q-15 | A2 | Clip groups | **answered 2026-10-13 by the three-kind specification in `groups-feature-spec.md` (tag, clip group, track group)** |
| Q-16 | A3 (confirmed) | Groups as organisation only (no effects) before the engine exists: acceptable? | yes (done in round 11) |
| Q-17 | A1 (default accepted) | Text objects become generators: in the same file-format change as the plugin model (v10) or a later one (v11)? Font as a parameter type. | later (v11), once the plugin model has settled |
| Q-18 | A3 | EXR/DPX image or sequence import and export in the first release? | **answered 2026-10-15: not yet** (not in the first release) |
| Q-19 | A1 (default accepted); **closed 2026-10-15** | Platforms: install **WSL2** now (admin + restart; I can then run the Linux tests myself), or a VirtualBox VM (you create it; cannot be combined with WSL2 without slowdown), or neither yet? Is the GitHub repository public (free hosted CI for Windows/Ubuntu/macOS)? What does "supported" mean per platform? | WSL2 (installed); public repo with CI (it is public); Linux: tests pass, macOS: builds in CI |
| Q-20 | A3 (default accepted) | On-canvas handles for point parameters in the first release? | later |
| Q-21 | A3 | Persistent (feedback) effects: re-simulate up to how many frames on a seek before using the last cached state? | **answered 2026-10-15: two global modes, Bounded (300 frames, fixed) and Warm-up only, plus global checkpoint interval (60 frames) and memory budget (1 GiB); none are project settings; the export is never affected: EN-14** |
| Q-22 | A3 (default accepted) | Audio-reactive shaders: ever? | not in the first release |
| Q-23 | A3 | The Resolve `.cube` variant (1D shaper plus 3D, non-standard input range): needed? | **answered 2026-10-15: yes, read only** |
| Q-24 | A3 (default accepted) | Reference host for the ISF golden corpus: the ISF Editor on Windows, by hand, 10 to 20 shaders? | yes |
| Q-25 | A3 (default accepted) | What ISF and LUT content ships (LG-02)? | gl-transitions and the clearly MIT original Vidvox shaders, our own LUTs, nothing else |
| Q-26 | A3 | Resource size limit; ask before loading a project that names a binary plugin? | **answered 2026-10-15: 64 MiB per memory-loaded resource, in the global settings, asking when exceeded; a project may name AND contain plugins; a linked plugin gets a warning prompt (per plugin, path and hash); an embedded one gets a severe prompt every time the project is opened, accepted by holding "Load it" for 5 s (mouse, or Enter, never Space); SF-04 to SF-10** |
| Q-27 | A1 | Which audio plugins first? | **answered 2026-10-13: LADSPA before CLAP; the LADSPA plugins of the maintainer's LMMS 1.2.2 install are the first test set** |
| Q-28 | A3 | A master bus with effects? | **answered 2026-10-15: later** |
| Q-29 | A3 (default accepted) | Keep ffmpeg's `atempo` for sped-up sound for now? | yes |
| Q-30 | A3 | Will you ever want a **node-editor** interface (design notes, section 2)? | **answered 2026-10-15: no node editor yet, but image-input parameters and a node editor must both stay possible: ID-10, EN-15** |
| Q-31 | A3 (default accepted) | A render process (design notes, section 3): keep it possible only through message-shaped interfaces, or build it? | possible only; a command-line export first |
| Q-32 | A1 (default accepted) | **Adjustment clips** (an effect over everything behind a clip for its length): wanted? | yes, after the engine |
| Q-33 | A3 (default accepted) | Isolated / pass-through for groups visible to the user? | a hidden default (isolated if it has effects, opacity below 100 % or a blend mode) |
| Q-34 | A1 (default accepted) | **GPL-3.0-or-later or GPL-3.0-only**, and an Ardour-style **plugin clarification** (plugins loaded through a third-party API are not derivative works)? Now, while you are the only copyright holder? | or-later; yes |
| Q-35 | A3 | OpenColorIO as an optional baker/reader later? | **answered 2026-10-15: later** |
| Q-36 | A3 | Export formats: 10-bit and other codecs now that a GPL ffmpeg may ship (x264, x265)? Hardware encoders? | **answered 2026-10-15: later; keep today's formats** |
| Q-37 | A1 (default accepted) | The plan's idea of shipping **LUTs through ffmpeg's `lut3d` before the engine exists** conflicts with the colour-space features (CM-10, CM-11): drop it (LUTs wait for the engine) or ship a limited version (display-referred in and out) first? | LUTs wait for the engine |

---

## 20. What this list does not say yet

The order, the sizes and the gates are in the plan of action, **`plugin-system-plan-of-action.md`** (written 2026-10-15, now that section 19 has no
open question). It also lists what was decided *about* the plan, and what is still a proposal of mine.

## 21. Decision log

| Date | Decision | By |
|---|---|---|
| 2026-10-12 | `.ffrs` reads stored entries in place; the program's two import actions are "Embed and import" and "Link and import" | maintainer |
| 2026-10-12 | the first four opinions of round 9 are agreed: open up effect identity, mutation tooling into the repository, split `FastForwardApplication`'s state, `.ffrs` v2 | maintainer |
| 2026-10-13 | licence GPLv3 | maintainer |
| 2026-10-13 | the performance goals of the plan are acceptable for now | maintainer |
| 2026-10-13 | linear-light compositing is an important feature; ACES AP0 asked about | maintainer |
| 2026-10-13 | `.cube` first, then ISF; embedding a shader is the default, configurable in the project settings and the global settings | maintainer |
| 2026-10-13 | the transition model (an effect with a backdrop input on the front clip) is acceptable | maintainer |
| 2026-10-13 | Windows first; Linux eventually (WSL or a VirtualBox VM); macOS far away | maintainer |
| 2026-10-13 | text objects should eventually be generator effects | maintainer |
| 2026-10-13 | OpenFX is shelved | maintainer |
| 2026-10-13 | clip groups and track groups with their own effects are wanted | maintainer |
| 2026-10-13 | Q-05: old projects are converted (option b), no legacy flag | maintainer |
| 2026-10-13 | **groups are built before the effects-system work**; three kinds specified (tag / clip group / track group): `groups-feature-spec.md` | maintainer |
| 2026-10-13 | round 1 answered by accepting my defaults, except Q-05 and Q-15 (still open) and Q-27 (LADSPA before CLAP; LMMS plugins as the test set; `ladspa-customwave` is not part of this project) | maintainer |
| 2026-10-15 | WSL2 (Ubuntu) is installed and the GitHub repository is public (Q-19 closed) | maintainer |
| 2026-10-15 | round 2 answered: my defaults for Q-01, 06, 11, 13, 14, 20, 22, 24, 25, 29, 31, 33; Q-16 confirmed; Q-23 yes (read only); Q-18, Q-28, Q-35, Q-36 later | maintainer |
| 2026-10-15 | Q-21: preview of effects that depend on earlier frames: global settings (mode, checkpoint interval, memory budget), never project settings, never the export (EN-14) | maintainer |
| 2026-10-15 | 256 MiB cap for an embedded plugin binary (SF-10); the plugin clarification wording is drafted by me and decided by the maintainer (Q-34); the CI workflow is created by me and committed by the maintainer; Ubuntu is registered in WSL | maintainer |
| 2026-10-15 | copyright notices carry the maintainer's legal name, John Dorsey; the plugin clarification, variant A, is adopted (`LICENSE-PLUGIN-CLARIFICATION`, Q-34, LG-04); `reuse`, `cargo-deny` and `cargo-about` may be installed | maintainer |
| 2026-10-15 | Q-30: no node editor yet; image-input parameters and a node editor stay possible (ID-10, EN-15) | maintainer |
| 2026-10-15 | Q-26: size limit 64 MiB (asks when exceeded, global setting); **projects may contain plugins** (reverses "never contain"); linked plugin: warning prompt per plugin; embedded plugin: severe prompt every time, accepted by holding "Load it" for 5 s (mouse, or Enter, not Space); declining is safe; several platform builds allowed; command-line export refuses without `--allow-plugins` | maintainer |

## 22. Gaps and inconsistencies found while writing this list

Recorded so that they are not found twice; each points at where it was resolved or is asked.

1. **An effect key that contained a content hash** (the long-term plan) contradicts versioning: every edit of a shader would be a new effect type.
   Resolved in ID-01 (the hash is the version); the project-versus-library rule is Q-08.
2. **"Embedding is the default"** cannot hold for a project saved as plain JSON, which cannot embed anything: Q-03.
3. **Linear light versus existing effects**: the hue shift and nearly all ISF shaders are written for display-referred values and would change look in a
   linear engine: CM-10, Q-07.
4. **Today's colour handling is an accident**: untagged video of any size is converted as BT.601 (verified), so choosing a correct assumption
   changes existing projects: CM-17, CM-12, Q-05.
5. **LUTs through ffmpeg before the engine** (the plan's phase 3a) conflicts with colour-space-aware LUTs: Q-37.
6. **Text objects as generators** need a second kind of host (CPU-drawn pictures), a font parameter type and a migration: EF-06, ID-05, Q-17.
7. **LADSPA was asked for first among audio plugins, but its binaries are rare on Windows**, the development system: AU-02, Q-27.
8. **The performance goals are stated for an integrated GPU that has not been measured with the real engine**: EN-07 makes them a benchmark in the
   repository, not a promise.
9. **Groups were asked for as "clip groups" without saying whether a group is a nested timeline or a selection**: GR-03, Q-15.
10. **The transition model needs a rule for `progress`** (the user does not keyframe it by hand for every transition): EF-03, Q-09.
11. **"Support Linux" and "support macOS" were not defined** (build, tests, release files): Q-19.
12. **Plugins and the licence**: hosting non-GPL plugins in a GPL program is a debated case that the copyright holder can settle in one
    sentence, which is easy now and hard later: LG-04, Q-34.
13. **"A project file can name a plugin and can never contain one"** (SF-04 of draft 1) was **reversed by the maintainer on 2026-10-15** (Q-26): embedded
    plugins are allowed, behind a severe prompt (SF-06). Everything that relied on the old rule is rewritten: SF-02 (shaders are data and have no prompt),
    RS-08, DS-04, and the plan's section on risks.
14. **A native library cannot be loaded from inside a zip**, so embedding a plugin needs an extract-and-hash step (SF-10).
15. **A feedback effect's preview can differ from its export** after a distant seek in *Bounded* mode and always in *Warm-up only* mode; EN-03 (preview and
    export agree on one machine) holds **for effects that do not depend on earlier frames, and for these only when the preview has caught up**. EN-14 says so
    in the interface.
