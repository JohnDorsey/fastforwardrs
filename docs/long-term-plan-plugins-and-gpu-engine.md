# Long-term plan (draft): plugins, a wgpu engine, and how much of ffmpeg stays

**Update 2026-10-13:** the maintainer answered the questions of section 11 (the answers are at the end of that section), chose **GPLv3**, shelved
OpenFX and asked for groups; the features that follow from all of it are in `plugin-system-features.md`, and the deeper answers (colour and ACES, a
node-graph model, a render process, groups, test machines, licensing) in `plugin-system-design-notes.md`.

**Update 2026-10-15:** every question of the features list is answered; **the order of work is now `plugin-system-plan-of-action.md`, which replaces section 9 of this document** (kept here for history).
Two things in this document are overtaken: the key of an ISF shader or LUT no longer contains a content hash (the hash is the *version*), and a project file **may contain** a binary plugin, behind a severe prompt
(features list, SF-04 to SF-10). The plugin model is file **format 11** (format 10 was taken by groups).

Status: **abstract draft for discussion**, written 2026-10-12 after round 10. Nothing in it is decided or started, except the four
preparatory steps the maintainer already agreed to (section 9, phase 0). It answers: how should the data model change to take
ISF shaders, `.cube` LUTs, LADSPA audio plugins and eventually OpenFX plugins, and more such things later; what does the move from
ffmpeg filter graphs to a wgpu engine look like; what are the options; how much of ffmpeg can be relied on if it is linked instead of
run as a child process; and can all of it work on Windows, macOS and Linux.

## How to read the evidence

Each factual claim carries a tag, because the sources differ a lot in how far they can be trusted:

* **[V]** verified by me in this repository or on this PC (read from the code, counted with grep, or measured with a throwaway script
  that I ran myself).
* **[M]** measured on this PC (Ryzen 5 7600X with its iGPU, an RTX 3090, Windows 10, wgpu 30.0.1, ffmpeg 9) by a research helper's
  throwaway programs. I did not run them again, except where it says **[M+V]**.
* **[R]** read in documentation, repositories or crate pages by research helpers (all dated 2026-10-12). Model output; plausible, not
  verified. Treat version numbers and dates as "check before relying".
* **[B]** my belief or opinion. Effort figures are all [B]: they are orders of magnitude for one person who knows this code, not
  estimates to plan a calendar with.

Confidence in a recommendation is said in words (high / medium / low) next to it.

---

## 1. The short version

1. **Open up identity first, before any GPU work.** Today the model cannot name an effect or a parameter that the compiler does not
   already know: `EffectKind` and `PropertyId` are closed enums (`PropertyId`: 1250 uses in 79 files; `EffectKind`: 262 uses in 25 files
   [V]). Every plugin family needs the same thing: a *stable string key* for the plugin, a *descriptor* that says which parameters it has,
   and *instances* in the project that hold parameter values by parameter id, never by position. That is a pure refactor that can be done,
   tested and committed with no GPU and no new dependency, and every later step needs it. (High confidence.)
2. **Then build a wgpu engine fed by the existing ffmpeg pipes**, with the CPU path kept as a golden oracle until parity is shown
   (option A in section 5). Do not link ffmpeg yet. (High confidence on the direction; medium on the effort.)
3. **Do `.cube` LUTs first among the plugin kinds**: they are small, they exercise exactly the new parts of the model (a descriptor, a
   parameter, an embedded resource file), and they can ship before the engine exists, through ffmpeg's `lut3d` filter on export. **ISF
   next** (needs the engine), **audio plugins in parallel** on a separate track of work (they need an in-process audio block graph, not
   the GPU), and **OpenFX last, or never** (huge host surface; its GPU suites do not map to wgpu). (Medium-high confidence on the order.)
4. **Linking libav is an optimisation, not a prerequisite**, and the main reason to do it (scrub latency) is smaller than it looks:
   starting ffmpeg costs about 19 ms and one 1080p picture about 60–100 ms [M+V], and most of that is decoding, which linking does not
   remove. The real forks are *licensing* (an LGPL build with hardware encoders only, or a GPL application) and *crash isolation*.
5. **Yes, all three operating systems can work**, with different costs per piece (section 8). wgpu itself is portable (Direct3D 12,
   Vulkan, Metal). What differs is which plugin formats exist on which system, and how an ffmpeg is built and shipped for each.

---

## 2. Where the code is today (what the plan has to start from)

### 2.1 The pipeline [V, from reading the code]

```
Project (model)  ──►  composition: PicturePlan / SoundPlan  ──►  preview:  ffmpeg frame-stream children -> RGBA pictures -> egui
 tracks, clips,        sections of frames in which the same          export:   the same plan -> ffmpeg filter_complex text -> ffmpeg
 racks, effects        layers are seen; per-layer placement,         sound:    the same plan -> ffmpeg audio graph, for the speakers
                       colour matrix, text; per-clip sound            (live) and for the export
```

* The *plan* (`src/composition`) is the part that is already independent of how pictures are made. It is tested frame by frame against
  a plain evaluation, with random projects. That test infrastructure is the most valuable thing the engine work can reuse.
* Two renderers exist for the picture: the preview draws layers in the window (placement, a colour matrix, text drawn on the CPU), and
  the export builds an ffmpeg filter graph from the same plan (`src/media/export/filter_graph/pictures.rs`, 372 lines, plus 867 lines of
  tests; `animated_sections.rs`, 130 lines, whose only reason to exist is that a filter graph cannot change values per frame cheaply, so
  animated values are quantised into stretches; `EffectKind::as_exported` exists for the same reason). A wgpu engine makes all three of
  those disappear, and makes preview and export one renderer, which is the main benefit apart from the plugins.
* Sound is a third, separate graph (`src/media/export/filter_graph/sound.rs`), run by ffmpeg both for live playback and for the export.
* The window is already on wgpu: eframe's default renderer, and wgpu 30.0.1 is in the lock file [V].

### 2.2 Where the model is closed [V]

* `EffectKind` is an enum with a table of rows (`ROWS`), described in the code as "a new effect is a new row". That works for effects
  that are written into the program, and one row exists (hue shift). It cannot hold an effect that a file brings.
* `PropertyId` is a `Copy + Ord` enum of ten variants. It is the key of the keyframes (`Animation: BTreeMap<PropertyId, Keyframes>`),
  of racks (`Rack { target, property, keyframes }`), of the selection, of the undo commands and of the drawers. An effect's values are
  "properties like any other", so the hue shift is a `PropertyId` variant and `PropertyId::effect()` finds its owner by searching the
  table. Adding a plugin parameter means adding an enum variant, so plugins are impossible.
* `Effects` allows **at most one effect of each kind** on a clip or track, and `Effect.values` is a positional `Vec<PropertyValue>`
  indexed by the position of the property in the kind's list. Position is the one thing a plugin's parameter list must never be
  identified by (every host that did so paid for it; section 3.5).
* `PropertyValue` has five kinds (number, pair, colour, flag, words). Colours are 8-bit. There is no choice, integer, trigger,
  resource or float-colour value.
* The project file is format 9, a JSON text that names properties and effects by their table names (`"hue shift"`), which is already
  a stable-string scheme in spirit, only a closed one.
* One model fact makes a lot easier: **tracks are layers** (front to back) and the plan already knows, for each layer, what is behind
  it. That is what transitions and blend modes need (section 3.4).

### 2.3 What ffmpeg does for the program today [V]

Probe (ffprobe), picture streams, sound streams, thumbnails, waveforms, still-picture decoding, the whole export (filter graph, encode,
mux), and, since this round, reading media *in place* inside a `.ffrs` archive through the `subfile` protocol. All as child processes.
It is a deliberately thin interface: arguments in, raw bytes out.

---

## 3. The data model

### 3.1 What every plugin family needs, whatever its format

Looking at ISF, `.cube`, LADSPA, CLAP, OpenFX, frei0r, and at how hosts that survived handled them [R], the common requirements are the
same five things, so the model should be built around them and not around any one format:

1. **A stable key for the plugin**, a string with a namespace: `builtin:hue_shift`, `isf:<name>@<content hash>`, `lut:<content hash>`,
   `ladspa:<UniqueID>/<label>`, `clap:<reverse-uri id>`, `ofx:<identifier>`. Never a file path (it moves) and never an index.
2. **A descriptor**: the parameters the plugin has, each with a stable string id, a value type, a range, a default, whether it can be
   animated, a label and a group. Descriptors come from the plugin, but the project keeps a **snapshot of the last one seen**.
3. **Instances** in the project: an instance id, the plugin key, the version it was made with, whether it is on, and the parameter values
   *keyed by parameter id*, with the keyframes owned by the host (the program), not by the plugin.
4. **Resources**: files a plugin needs (a `.cube` table, an ISF shader source, an image an ISF shader imports), which travel with the
   project exactly as source media can (linked, or embedded in the `.ffrs`).
5. **A missing-plugin policy**: if the plugin is not installed on this computer, the instance stays in the project with all its values
   and keyframes, renders as "no effect", says so, and is saved back unchanged. (Kdenlive drops such effects: "removed from this
   project". Blender keeps unknown nodes as generic properties. Natron stores plugin id, version and knobs by name. [R]) Losing a user's
   work because a plugin is not installed on a laptop is the worst failure this design can have.

### 3.2 Three ways to model it

| | A. Keep the closed table, add rows | B. Descriptor-driven instances (recommended) | C. Full node graph |
|---|---|---|---|
| What it is | `EffectKind::ALL` grows; each new built-in is a row | `PluginDescriptor` + `EffectInstance`, built-ins are descriptors too | a DAG of nodes with typed ports; clips are nodes |
| Plugins from files | impossible | yes | yes |
| Built-in effects | easy, compile-time checked | as easy once the hue shift is a descriptor | as easy |
| Effort to get there | none | M: a refactor of about 80 files, almost all mechanical | XL: changes the timeline's model |
| Risk | dead end for everything in this document | the refactor itself (see 3.6 for how to make it safe) | a rewrite of the editing model for a feature (multi-input effects) the stack model mostly gets through `backdrop` inputs (3.4) |
| When to choose it | if plugins are never going to happen | **now** | only if keyers, mattes and multi-source composites become the product |

My recommendation is **B**, with a stack per clip and per track (the shape the program has) and *typed input slots* in the descriptor
so that a later step towards C (an effect that reads another clip's picture) does not need a new model. High confidence.

### 3.3 The types (sketch, not code to paste)

```rust
/// "builtin:hue_shift", "lut:9f2c…", "isf:Color Wheel@a41b…", "ladspa:1053/amp_mono", "clap:com.example.reverb", "ofx:net.sf.openfx.Merge"
struct PluginKey(Arc<str>);

struct PluginDescriptor {
    key: PluginKey,
    version: (u32, u32),                 // OFX-style: same major = compatible, take the highest minor
    domain: Domain,                      // Picture | Sound
    role: Role,                          // Filter | Generator | Transition | Retimer (picture); Insert | Generator (sound)
    inputs: Vec<InputSlot>,              // Source (the clip), Backdrop (everything behind), Aux(n) later
    params: Vec<ParamDesc>,
    resources: Vec<ResourceSlot>,        // "this plugin needs a .cube table", "an image called …"
    needs_state_across_frames: bool,     // ISF PERSISTENT buffers, reverbs: affects seeking (section 6.4)
    signature: Signature,                // what the backend must offer to run it (a shader, a CLAP plugin, …)
}

struct ParamDesc {
    id: String,                          // stable, unique in the plugin; NEVER an index. Opaque to the model.
    label: String, group: Option<String>,
    value: ParamType,                    // Number{min,max,step,default,scale} | Int | Flag | Choice{ids,labels} | Pair{..}
                                         // | Colour{float rgba} | Trigger | Text | Resource(kind)
    animatable: bool,
    default_interpolation: Interpolation, // Linear | Hold (Choice, Flag, Trigger) ; per-key choice can come later
    since_version: (u32, u32),
}

struct EffectInstance {
    instance: EffectInstanceId,          // u32, per project, like ClipId and RackId
    plugin: PluginKey, version_seen: (u32, u32), enabled: bool,
    values: BTreeMap<ParamId, ParamValue>,   // saved by id; in memory also addressable by slot (below)
    extra: serde_json::Value,            // an opaque blob a backend may need (a plugin's saved state); preserved unread
}

/// What a rack, a keyframe, an undo step or a drawer points at.
enum ParamAddress { Intrinsic(IntrinsicProperty), Effect { instance: EffectInstanceId, param: ParamSlot } }
```

**The one trick that keeps the refactor small** (medium-high confidence, needs a spike to confirm): `PropertyId` is `Copy + Ord` and is
used as a map key in 79 files. Keep that. Make it
`PropertyId::{Gain, Muted, Position, …(the intrinsic ones…), EffectParam { instance: EffectInstanceId, slot: ParamSlot }}` where `slot`
is a small number that indexes the *instance's own* parameter table (the order of the descriptor snapshot the instance was made with).
Slots are stable *inside a project in memory*; the **file stores parameter ids**, and slots are re-derived when a file is read. So the
identity that is stored never depends on an index, while the identity that is passed around the code stays `Copy`, `Ord` and cheap.
The alternative (an `Arc<str>` id inside `PropertyId`) loses `Copy` and touches every one of the 1250 uses.

The intrinsic properties (position, size, rotation, gain, mute, the text object's text, font, size and colour) stay a closed enum on
purpose: they are the model's own concepts, no plugin adds to them.

Where descriptors live: **in the project, once per key and version** (`Project.plugins`), not in each instance, so that the model stays
pure (no registry needed to validate a value's type, as `Effect::with_values` does today) and a clone for undo stays cheap (`Arc`).
The *live* descriptor comes from the host registry in the application; when both exist and differ, the program reports and migrates
(3.5); when only the snapshot exists (plugin missing), the snapshot is enough to show sliders, ranges and keyframes.

### 3.4 Roles that need more than "a filter on a clip"

* **Generators** (an ISF source shader, a CLAP instrument later): a clip with no media. Text objects are already this idea in
  special form. Do **not** unify text objects into generators in the first rounds (a lot of churn for no new capability); allow a
  *generated clip* kind whose content is an effect instance with no `Source` input. (Medium confidence that unifying later is worth it.)
* **Transitions** (68 of the 327 shaders in the main ISF corpus are transitions [R]): an effect with inputs `startImage`, `endImage` and a
  `progress` number. In this model that needs no new object: a transition is an effect **on the front clip with a `Backdrop` input**,
  where `Source` is the clip and `Backdrop` is what is composited behind it, and `progress` is a keyframed parameter over the overlap.
  Tracks being layers is what makes that work. The editing gesture ("drop a transition on the cut between two clips on one track")
  would be a convenience that creates two clips on two tracks overlapping; clips cannot overlap on one track today and I would keep it
  so. (Medium confidence; the alternative, a first-class transition object on a cut with handles, is a bigger change to the timeline.)
* **Blend modes, keyers, glass effects**: the same `Backdrop` slot.
* **Sound effects**: the same instance type, `domain: Sound`; each clip and track has one ordered list of instances, and the picture and
  sound compositions each take the ones of their domain.
* **State across frames** (ISF `PERSISTENT` buffers, feedback, reverb tails): see 6.4; it is a property of the descriptor because it
  changes how seeking and export work for that clip, and the plan must know.
* **Not plugins, but they will come as model concepts**: masks and shapes with keyframed paths, tracking data, time remapping beyond
  constant speed, colour management settings per project. None of these is blocked by the descriptor model and none is solved by it.

### 3.5 Missing, changed and migrated plugins

* **Missing**: keep the instance; values and keyframes are kept verbatim, typed by the snapshot; render as pass-through; show a
  badge; save unchanged; when the plugin appears (same key), it just rebinds. [B/R: this is what Ardour's placeholders and OFX's
  persistence rules both do.]
* **Version changed**: OFX's rule is a good default: same major version, use the highest minor; a parameter that exists on only one side
  is not an error (the other side keeps its default, or the saved value is kept as `extra`). Different major: treat as missing and say
  so. A per-plugin **alias table** (`old id -> new id`) handles renames; MLT needed such a file for frei0r because it identified
  parameters by index [R], which is the cost this design avoids.
* **Changed descriptor** (a parameter's range moved): clamp on use, keep the stored value, say so once.
* **Keyframes belong to the host and are typed**: `ParamType` decides the interpolation (numbers and colours lerp, choices and flags
  hold). A plugin never sees keyframes; it is given the value at the frame. (OpenFX lets plugins ask for the value at a time, which is
  the same contract. [R])

### 3.6 Resources: reuse what this round built

A `.cube` table and an ISF shader (and the images an ISF shader imports) are *files the project depends on*, exactly like source
media. So they get the same link:

```json
"resources": [
  { "id": 4, "kind": "lut3d", "link": { "absolute_path": "C:/luts/film.cube", "relative_path": "luts/film.cube", "included_as": "resources/4-film.cube" },
    "content_hash": "9f2c…" }
]
```

* `SourceMediaLink` already has `absolute_path`, `relative_path` and `included_as`, and the program already has *Embed* and *Link*
  (this round's "Embed and import", "Link and import", "Include in project file"). A resource list reuses all of it. LUTs and shaders are
  small, so they are read into memory (`extract_entry`), not read in place.
* **Key the plugin by content hash** (`lut:<hash>`, `isf:<name>@<hash>`), so that two clips using one LUT are one resource, a changed
  file is a different plugin version, and an embedded copy is trusted only if its hash matches.
* A plugin that is a *file the user chose* (an ISF shader or a LUT) is thereby also **missing-proof**: embed it, and the project carries
  the shader with it. That is better than anything the commercial hosts do and costs nearly nothing here. (Medium confidence that the
  UX of "embed" per resource, rather than a project-wide switch, is right: ask the maintainer.)

### 3.7 File format: version 10, and how to get there safely

```json
"plugins": [ { "key": "builtin:hue_shift", "version": [1, 0], "domain": "picture", "role": "filter",
               "params": [ { "id": "degrees", "label": "Hue shift", "type": "number", "min": -360, "max": 360, "default": 0, "animatable": true } ] } ],
"clips": [ { "…": "…", "effects": [
    { "instance": 3, "plugin": "builtin:hue_shift", "version_seen": [1, 0], "enabled": true, "values": { "degrees": 30.0 } } ] } ],
"racks": [ { "id": 7, "target": { "clip": 12 }, "param": { "instance": 3, "id": "degrees" }, "keyframes": [ … ] } ]
```

* **Reading version 9** maps `"hue shift"` to `builtin:hue_shift` with parameter `degrees`; **writing is always the newest**. The
  program already reads every older version (the README's *Project files* section lists them); one more is routine.
* **An unknown field is preserved** (the structure has `extra` bags at the instance and plugin level), so a file written by a newer
  program survives a round trip through an older one. This is the single most valuable compatibility property; add it in the same step.
* **The refactor is made safe by tests that exist today**: the project model has about 12 000 lines of tests [V] (effect editing, racks,
  animation, duplication, undo), the plan and the export are tested against plain evaluations with random projects, and there are 29
  real-ffmpeg journeys [V]. The rule from the previous rounds applies: lock behaviour with the existing tests, change the *type* under
  them, and keep them green and unmodified wherever the behaviour is meant to stay (an effect is on a clip; its value animates; undo
  restores it). The tests that must change on purpose are the ones that assert "at most one of each kind" (instances are now unique by id,
  and two hue shifts are legal) and the file-format ones.

---

## 4. What changes, what stays (by layer)

| Layer | Today | After |
|---|---|---|
| model (`project`) | closed `EffectKind` / `PropertyId`; one effect per kind; positional values | descriptors in the project; instances by id; `PropertyId::EffectParam`; intrinsic properties stay an enum |
| keyframes, racks, undo, drawers, selection | key on `PropertyId` | unchanged in structure (the key type is the same shape) |
| file format | v9, table names | v10 with `plugins`, `instances`, `resources`; v9 is read |
| plan (`composition`) | layers, sections, colour matrix per layer | layers with an ordered list of **evaluated effect calls** (`instance`, `plugin`, `(param id, value)`); sections no longer have to split where an animated value crosses a quantisation step |
| preview | draws layers with egui, a colour matrix, CPU text | asks the engine for the frame's texture; egui shows it (section 5.3) |
| export (picture) | ffmpeg filter graph text | engine frames read back and piped to ffmpeg as raw video (`animated_sections.rs`, `filter_graph/pictures.rs`, `as_exported` deleted once parity is shown) |
| sound | ffmpeg audio graph | the same at first; later an in-process block graph (section 7.3) |
| source media | ffmpeg children; `.ffrs` in place | unchanged until section 6 says otherwise |

---

## 5. The engine: options and what a wgpu engine looks like

### 5.1 Options

| | Description | Effort | Risk | Notes |
|---|---|---|---|---|
| **A** | wgpu compositor/effects engine; pictures still come from ffmpeg child processes (yuv420p / nv12 pipes, converted to RGB **on the GPU**); export reads back and pipes raw video to ffmpeg for encode and mux | M | low | **recommended**. 4K yuv420p through a pipe runs at 80 fps; RGBA conversion by ffmpeg drops it to 13.6 fps, so conversion must move to the GPU |
| B | A plus in-process libav (own decode threads, hardware decode import where it works) | M-L on top of A | medium | the best performance; do it only if A's decode path is the measured bottleneck |
| C | GStreamer with its GPU elements | L | high | its GL/Direct3D/Vulkan memory is not a wgpu device, and it is built for playback, not editing |
| D | A separate render-server process | XL | high | the only design that truly survives a GPU hang, at the price of a copy through shared memory and a lot of plumbing |
| E | CPU compositor stays; only plugin passes run on the GPU | S-M | medium | two renderers that cannot give identical results, and a 4K round trip per effect (about 3 ms on a fast GPU, 7 ms on an integrated one); a mixed CPU/GPU graph "negates the speedup" [R] |

Recommendation: **A, then B only on evidence.** E is the tempting shortcut and the one I would argue against hardest: it adds plugins
fast and makes the preview/export mismatch permanent. (High confidence.)

### 5.2 What the engine is

* **One render graph per frame**, built from the plan's section: per layer `decode -> upload -> convert -> transform (placement) ->
  effect stack -> composite`; then track effects; then the output texture. Pooled textures keyed by (width, height, format),
  reference-counted, ping-ponged; persistent per-instance textures for effects that keep state. This is the structure the Blender
  compositor uses, and the shape the Smelter project documents (read-only reference: its licence forbids embedding) [R].
* **Working format `Rgba16Float`**: filterable, renderable and blendable on all backends in the guaranteed set; `Rgba32Float` filtering
  is not available on every Apple GPU [R]. **Gamma-encoded first** (my recommendation, medium confidence): the current preview and the
  ffmpeg export both apply colour matrices to gamma-encoded RGB, so a gamma-encoded engine can be checked against them and against
  the CPU oracle with a small tolerance. Linear-light compositing is more correct for blends and blurs but changes the look of every
  existing project; offer it later as a project setting, with a version number so old projects keep their look.
* **Colour conversion on the GPU**: planes uploaded as R8 + RG8 (or `R16Uint`/`Rg16Uint` for 10-bit, portable) and converted in a
  shader, as Smelter does. `EXTERNAL_TEXTURE` and NV12 texture formats exist only on some backends [R]; do not build on them.
* **egui integration**: render on a worker thread into a texture, show it with `register_native_texture`, a latest-wins queue between
  them. eframe exposes the device and queue (`wgpu_render_state`). (An egui paint callback inside the window's own pass is the
  alternative; it ties rendering to the UI frame, which is wrong for a render that may take longer than a frame.)
* **Export**: the same engine, frames read back (`copy_texture_to_buffer`, a ring of three buffers, convert to a YUV layout *before*
  readback: 4K RGBA16F readback is 7–9 ms, an NV12-style layout 1.0 ms on the fast GPU [M]) and written to ffmpeg's stdin as raw video;
  audio is mixed separately and muxed. A bounded channel paces the encoder.
* **Determinism**: the same adapter gives identical bytes run to run; on one GPU, Vulkan and DX12 were byte-identical [M]; other vendors
  differed by at most 1/255 in a test scene [M], and WGSL does not promise more. So "preview equals export" is guaranteed on one
  machine and is "within one level" across machines. State that in the documentation, and test with a tolerance.
* **User shaders can hang the GPU**: infinite loops compile without complaint, and the default loop bounding prevents undefined
  behaviour, not long running time [R]. Plan: run plugin passes under a watchdog, treat device loss as recoverable (recreate the device,
  disable the offending instance, say which), and **never test this on the maintainer's GPU** (the helper rightly did not).

### 5.3 What it costs to run [M]

Four 4K layers with two blurs and a LUT (a "typical stack", on full-frame noise, the worst case for caches): about 1 ms on the RTX 3090,
25–28 ms on the integrated GPU, 2.8 s on the software rasteriser (which is what the test machines will have, so test with small
frames). Upload of a 4K RGBA frame: 2.3–2.6 ms on the 3090, 3.9 ms integrated. So 4K is comfortable on a discrete GPU and borderline on an
integrated one; 1080p on an integrated GPU is comfortable. Intel and Apple GPUs were not measured.

### 5.4 Testing the engine

* **Golden oracle**: keep the CPU compositor and the ffmpeg export path alive through the migration; random projects (the tests already
  generate them) are rendered by both and compared with a tolerance and a pixel-count budget. Delete the old path only when the gate
  below is met.
* **CI**: egui_kittest already prefers CPU adapters; wgpu's own CI installs WARP (Windows) and Mesa lavapipe (Linux) [R]. Software
  rendering is slow, so engine tests use small frames.
* **Shader errors** are testable: bad WGSL gives an error scope and line/column information, not a panic [R].

---

## 6. How much of ffmpeg can be relied on if it is linked

### 6.1 What stays in ffmpeg either way

Demuxing, decoding, muxing and encoding (the codec libraries are the reason ffmpeg is in the program at all), `swresample`, and for now
the audio filters (`atempo`, `amix`, `apad`, `aresample`). Nothing in section 5 replaces the codecs. What the engine replaces is *only*
video `libavfilter`: trimming and `setpts` are timeline logic the plan already has; scale, pad, rotate, `colorchannelmixer`, overlay
and text are render passes. [B; R for the filter list]

### 6.2 Linking: what you get and what it costs

| Question | Answer |
|---|---|
| Rust crates | `ffmpeg-next` 9.0.0 and `ffmpeg-the-third` 6.0.0 are current for FFmpeg 9 and are used in practice; neither wraps hardware decode contexts or custom I/O safely (about 60 lines of `unsafe` sys code are needed); `rsmpeg` has the widest API but is stalled on FFmpeg 8; all use bindgen, so libclang is a build dependency. The crate's major version must match the FFmpeg major (FFmpeg 9 bumped all seven library majors). [R] |
| What does linking gain? | No process start (19 ms **[M+V]**), no pipe copies, frame-accurate seeking inside one open demuxer, hardware decode import, decode threads you control. |
| What does it **not** gain? | Decoding time. One 1080p picture from a GOP-250 file takes 60–100 ms by spawning ffmpeg, and 291 ms when it has to decode from the start, **[M+V]** (a throwaway script, `seek_bench.py`, run for this document): most of that is decoding the frames between the keyframe and the target, which an in-process decoder also has to do. The saving from linking is the ~20 ms start plus open, not a different order of magnitude. |
| Reading inside a `.ffrs` | The `subfile` protocol worked in every ffmpeg use this round **[V]**; libavformat takes the same URL string in-process (believed; not tried), or a custom `AVIOContext` with read and seek (seeking is required for mp4 files whose index is at the end). Only *stored* zip entries can be windowed, which is what the program writes. |
| Licensing | **The real fork.** A proprietary application needs an **LGPL** build, dynamically linked, with no `--enable-gpl`/`--enable-nonfree`, shipped with its source and configure line; that rules out `libx264`/`libx265` and leaves the platform's hardware encoders (NVENC, AMF, QSV, VideoToolbox, VAAPI, MediaFoundation) plus a baseline-only `openh264`. A GPL application can link anything. Running ffmpeg as a **separate process** is generally treated as "mere aggregation" (the GPL FAQ), which is why the program can ship or use a GPL ffmpeg today. Not legal advice; ask a lawyer before shipping. [R] |
| The dev machine | The installed ffmpeg is a GPL full build (gyan.dev). Homebrew's on macOS is GPL as well. Neither can be redistributed with a proprietary program. [R] |
| Crash isolation | A decoder crash takes the editor down if it is in-process. Today's child process is isolation for free. If libav is linked, link it into a **worker binary** that a supervisor respawns, with frames passed through shared memory (`memmap2`); the engine and the UI stay in the main process. [R/B] |
| Hardware decode into wgpu | No turnkey zero-copy exists (`gpu-video` 0.4 does Vulkan Video H.264 decode on wgpu 29 for Linux and Windows; nothing for macOS, HEVC, AV1) [R]. Realistic baseline: software decode, upload NV12 planes, convert on the GPU (4K NV12 is 12.4 MB a frame, about 0.75 GB/s at 60 fps: arithmetic, not measured). Hardware decoders also constrain GOP seeking, which matters for frame-accurate editing [B]. |
| Build and ship | Windows (MSVC): LLVM plus `FFMPEG_DIR`, or vcpkg (opt-in GPL components); BtbN publishes `lgpl-shared` and `gpl-shared` builds. macOS: your own LGPL build with VideoToolbox, bundled dylibs, codesigned and notarised. Linux: `-dev` packages and pkg-config; AppImage clashes with the host's libva; Flatpak offers an ffmpeg extension. [R] |
| Alternatives | GStreamer (playback-oriented, heavy to bundle, poor fit for an editor), libmpv (a player), platform APIs (three code paths), pure Rust (no production H.264/H.265 decoder found; `symphonia` for audio decode only). [R] |

### 6.3 Recommendation

**Keep ffmpeg as a child process through phases 1 to 3** (section 9), as a dumb decode/encode pipe: raw pixels out for the engine,
raw pixels in for the encode. It costs ~20 ms per start, it isolates crashes for free, it keeps the licence question out of the code,
and it keeps today's tests valid. Make the decision about linking **on measurements of the real editor** after the engine exists:
scrub latency against a budget, and 4K playback against the pipe limit (80 fps for yuv420p [M+V], so the pipe is not the first
bottleneck). If it is needed, link libav into a separate worker (option B), not into the UI process. (Medium confidence. The
reasoning is that linking is a one-way door on licensing and packaging for a gain that, measured, is mostly in the start cost.)

---

## 7. The plugin families, one by one

| | Format | Maps to | Order | Effort [B] | Windows | macOS | Linux |
|---|---|---|---|---|---|---|---|
| 1 | `.cube` LUT | built-in descriptor `lut:<hash>` with a `Resource` parameter and an `intensity` | first | S | yes | yes | yes |
| 2 | ISF shader | descriptor from the JSON header; WGSL from the GLSL source | second | M-L | yes | yes | yes |
| 3 | audio: LADSPA, then CLAP | `domain: Sound` descriptors; in-process block graph | in parallel | M, then M | LADSPA binaries rare; CLAP yes | LADSPA almost none; CLAP yes | both |
| 4 | OpenFX | CPU image path through a host written against the OFX C API | last, or never | XL | yes | yes | yes |

### 7.1 `.cube` LUTs (high confidence this is the right first one)

* Format: Adobe's cube specification (2013; the original URL is gone, mirrors exist): `LUT_3D_SIZE` up to 256, or `LUT_1D_SIZE` up to
  65536, with `DOMAIN_MIN/MAX`; red varies fastest; a reader "should" use tetrahedral interpolation for 3D tables. The Resolve variant
  adds a 1D shaper and non-standard input ranges. Own parser: about 150 lines; crates exist but are small and old. [R]
* **Ships before the engine**: ffmpeg's `lut3d` (tetrahedral by default) in the export graph, and a small 3D-lookup in the preview.
  That makes it the cheapest possible proof of the descriptor, resource and file-format steps, with a user-visible result. [B]
* On the GPU: a `Rgba16Float` 3D texture (filterable without features) with a tetrahedral lookup written as four `textureLoad` taps;
  hardware trilinear is not tetrahedral and gives slightly different colours. Clamp to the domain; keep float intermediates. [R]
* **A LUT assumes the encoding it was built for** (a log-to-Rec.709 LUT expects log footage). With a gamma-encoded working space
  the MVP applies it as ffmpeg does; a per-instance *input encoding* choice is the honest long-term answer. [R/B]

### 7.2 ISF shaders (medium confidence on effort; the risks are specific)

* **Compilation**: ISF is GLSL with a JSON header. A small source *shim* (rewrite to `#version 450`, a uniform block for the inputs,
  separate texture and sampler, `gl_FragColor` and varying rewrites) followed by `naga`'s GLSL front end, validation and WGSL output
  compiled **320 of 327 shaders (97.9 %)** of the current VIDVOX corpus in about 0.5 s in total **[M]**. That is *compile success
  only*: no pixel was compared with a reference host, and open naga issues cover silent miscompiles (`&&`/`||` not short-circuiting, which
  95 of the 327 shaders use; array index arithmetic; deep nesting crashes) [R]. So the plan needs a **golden-image corpus test**, with
  a reference host (the ISF Editor) producing the expected pictures, before ISF is promised to anyone. Run naga under `catch_unwind` in
  a worker thread.
* **No usable Rust ISF renderer exists** (`isf` 0.1.0 is parse-only and from 2020; `nannou_isf` is naive; `tweak_shader` is a design
  reference) [R]: write the header parser (tolerant: duplicate keys and an integer `ISFVSN` occur in the wild), the pass runtime
  (`TARGET`, `PERSISTENT`, `FLOAT`, `WIDTH`/`HEIGHT` expressions: a tiny evaluator) and the shim ourselves.
* **Parameter mapping**: `float -> Number`, `bool -> Flag`, `long -> Choice`, `color -> Colour`, `point2D -> Pair`, `event -> Trigger`,
  `image -> an input slot`; `audio`/`audioFFT` deferred. [R]
* **Time**: `TIME` is clip-local, `FRAMEINDEX` clip-local, never the wall clock; ISF is y-up, so flip consistently. [R]
* **Persistent buffers are the riskiest part** (15 % of the corpus): a persistent effect needs the previous frame's output, so a seek
  must re-simulate from the start of the clip or a checkpoint. Export is sequential and deterministic. Say so in the descriptor and
  show it in the UI ("this effect depends on earlier frames"). [R/B]
* **Licensing is mixed**: all 68 transitions are converted from the `gl-transitions` collection under individual licences; some shaders
  are non-commercial; the community site indexes about 10 500 shaders with no visible per-shader licence. **Bundle only shaders you can
  show a licence for** (the original VIDVOX ones), and let users load their own. [R]
* Reading them from a project: ISF source is a resource (3.6), embedded if the user wishes, keyed by content hash.

### 7.3 Audio plugins (medium confidence)

* **LADSPA** is a trivial C ABI (`ladspa_descriptor`, ports, `run`), no state or latency API, parameters read once per block. A host is
  about 300 lines over `libloading` [R]. **Its binaries exist almost only on Linux**; Audacity lists Windows and Linux, Ardour's docs
  Linux and Mac with few plugins, Homebrew has only an SDK [R]. So "LADSPA first" is worth doing as a *proving ground* for the audio
  graph, and **CLAP is the format real users get on all three systems** (`clack-host` 0.2.0; LV2 and VST3 hosts in Rust are young) [R].
* **Do not use ffmpeg's `ladspa` filter**: it hard-codes `.so`, the `:` separator and `$HOME`, and Homebrew's ffmpeg is built without
  it [R]. Host plugins **in-process, in one Rust block graph** that both live playback (`cpal`) and export call. That also removes the
  current duplication (one ffmpeg graph for the speakers, one for the file) and is the way to sample-accurate parameter changes: split
  blocks at keyframe change points or every 32–64 samples, smooth gain-like parameters, reset and pre-roll on seek (`activate` on a LADSPA
  plugin re-initialises it).
* **Order of work**: (1) lock today's ffmpeg mix with tests; (2) build the Rust graph and a null test against the old export (the
  output must match what ffmpeg produced, within a tolerance, for mixes of gains, mutes and speed); (3) descriptor, built-in gain,
  placeholder and serialisation; (4) LADSPA; (5) CLAP; (6) plugin *scanning* in a child process (a crashing plugin must not take the
  editor down at start-up). Time stretching (`atempo`) stays in ffmpeg until a replacement is judged by ear and by a re-baselined test
  (`signalsmith-stretch` is MIT; `rubberband` is GPL). [R/B]
* Missing plugin: pass-through placeholder that re-saves everything. (Ardour does this.)

### 7.4 OpenFX (medium-low confidence; effort is the issue)

* An ASWF project with a BSD-3 API; version 1.5.1 current [R]. Hosts: Nuke, Resolve, Natron, Vegas, Flame. **A host needs**: the
  Property, ImageEffect, Parameter, Memory, MultiThread and Message suites at minimum; a long list of host properties; clip
  preferences, regions of definition, time handling, threading flags; parameters are keyframed *by the host*, so this matches 3.5. Parts of the C API are variadic, so
  a small C shim is needed from Rust. A one-frame-filter host is about 6000 lines; one that runs the free `openfx-misc` set properly is
  10–20 thousand lines and two to four months of work [R/B]. A reference host exists (`openfx-runner`, MIT, Rust, 5.9k lines, filters
  only) [R].
* **Its GPU suites do not map to wgpu** (they are OpenGL, CUDA, Metal and OpenCL), so the realistic host is the **CPU path**: read back,
  run the plugin, upload. A 1080p float RGBA frame is 33 MB each way. Track where each image lives so that consecutive CPU effects pay
  one transfer. [R]
* The free plugin sets are GPL (`openfx-misc` and others): a proprietary host should not bundle them. Commercial plugins (BorisFX,
  Neat Video and others) are the real prize and also the real support burden. [R/B]
* **Isolation**: no mainstream OFX host is known to sandbox plugins [R]. Make the plugin trait *message-shaped* (no raw pointers
  across it) so an out-of-process host stays possible later: CPU shared memory is easy at 8–33 MB a frame; sharing GPU textures with a
  helper has no safe wgpu API. [R/B]
* **My opinion**: do not start OpenFX until ISF, LUTs and audio plugins have shipped and users ask for specific commercial plugins.
  It is the one family whose cost is not proportional to its first visible result. (Medium confidence.)

### 7.5 Later families (the model should not block them)

frei0r (C ABI, 8-bit, parameters by index only: wrap with an alias table), OpenColorIO transforms (BSD-3; likely the right route for
colour transforms and log LUTs once the colour-management question is decided), WASM plugins (they would ship a WGSL shader and a
descriptor, not call the GPU themselves), the program's own plugin format (a WGSL file plus a descriptor, the simplest of all, and a good
first format for built-in effects). All of these are *descriptors plus a backend that can run them*, which is the point of 3.1.

---

## 8. Windows, macOS, Linux

| | Windows | macOS | Linux |
|---|---|---|---|
| wgpu backend | Direct3D 12 (or Vulkan) | Metal | Vulkan |
| today's ffmpeg child | works (this machine) | needs an ffmpeg on `PATH` or bundled | same |
| shipping ffmpeg | BtbN `lgpl-shared`/`gpl-shared`; or own build | own LGPL build with VideoToolbox, notarised; Homebrew's is GPL | distro packages or bundled; Flatpak extension |
| hardware decode (option B) | D3D11VA/D3D12VA, NVDEC, QSV, AMF; Vulkan Video | VideoToolbox | VAAPI, NVDEC; Vulkan Video |
| `.cube`, ISF | same code | same | same |
| LADSPA | few binaries | almost none | the home of it |
| CLAP | yes | yes | yes |
| OpenFX | `C:\Program Files\Common Files\OFX\Plugins` | `/Library/OFX/Plugins` | `/usr/OFX/Plugins` |
| CI | WARP software adapter | Mac runner (Metal) | Mesa lavapipe |
| measured here | yes | **no** | **no** |

* Everything on the macOS and Linux columns is **[R] or [B]**: this PC is the only machine anything was measured on. The wgpu parts
  (Metal, Vulkan) are well-trodden by eframe users; the unknowns are ffmpeg packaging, hardware-decode behaviour and Apple GPU limits
  (for example filtering of 32-bit float textures).
* The program already runs on all three in principle (eframe, cpal, rfd, `std::path`); this round's Windows-specific facts (an open file
  cannot be renamed over; `subfile` paths with commas) were measured on Windows only, and the code was written to not depend on them
  (the safe design works where renaming over an open file would succeed, too).
* A real porting check is cheap and should happen **before** the engine is large: build and run the existing test suite on macOS and
  Linux once, with a CI matrix. That shows what is Windows-shaped in the code today. (Not done; I have no access to those systems.)

---

## 9. A phased path with gates

Sizes are [B]. A gate is a condition that must hold before the next phase starts; none of them needs the maintainer's attention except
to read the result.

| Phase | What | Size | Gate |
|---|---|---|---|
| **0** (agreed) | open up effect and property identity (the model refactor of section 3, without plugins yet: the hue shift becomes `builtin:hue_shift`); the mutation-testing tooling into the repository (cross-platform); split `FastForwardApplication`'s state into sub-structs; `.ffrs` reading in place (**done** in round 10) | M, M, M | all existing tests green and unmodified except the ones that must change on purpose; file format v10 round-trips v9 files |
| **S** | spikes (days each, throwaway code): (S1) one ISF shader compiled and drawn; (S2) a deliberate hang and device-loss recovery, on a machine that is not the maintainer's; (S3) one `.cube` table in a shader versus `lut3d`; (S4) seek latency of the real editor, spawn versus a prototype in-process open; (S5) the existing suite on macOS and Linux | S | each spike says what it found; S2 decides how plugin passes are guarded |
| **1** | engine skeleton, option A: render today's plan on the GPU (placement, hue, text as an uploaded texture); the preview shows engine textures; **the CPU path stays** as the oracle | M-L | 1080p60 for a typical stack on the integrated GPU (about 6 ms measured by scaling [M/B]); random-project comparison within tolerance; a bad shader gives an error, never a hang; CI green on software adapters |
| **2** | export through the engine (readback, raw video into ffmpeg); then delete `filter_graph/pictures.rs`, `animated_sections.rs`, `as_exported` and their tests | M | export equals preview on one machine; the real-ffmpeg journeys pass unchanged |
| **3a** | `.cube` as a descriptor with an embeddable resource (ffmpeg `lut3d` on export before phase 2 if it is wanted sooner) | S | tetrahedral shader equals `lut3d=interp=tetrahedral` within a tolerance |
| **3b** | ISF: parser, shim, pass runtime, golden corpus | M-L | the golden corpus passes for the shaders that are bundled; persistent effects behave on seek |
| **4** | audio block graph, LADSPA, then CLAP, scanning in a child process | M + M | the Rust mix equals the old ffmpeg mix; no plugin crash takes the editor down |
| **5** | option B only if S4 and phase 1's measurements demand it: a libav worker process, with its own licence decision | M-L | measured gain against the budget |
| **6** | OpenFX CPU host, if asked for | XL | a named plugin that users need runs |

Dependencies: 0 before everything; 1 needs 0; 2 needs 1; 3a can precede 1; 3b needs 1; 4 is independent of 1–3 (it needs 0);
5 needs the measurements of 1; 6 needs 1 and 0.

---

## 10. Risks, and what would change the plan

* **Licensing intent** is the most important unknown. A proprietary or closed-source distribution rules out the shipped GPL ffmpeg, the
  free OpenFX sets, and `libx264`; a GPL distribution removes most of those worries. Everything in section 6 branches on it.
* **The refactor of identity touches 79 files.** Mitigation: it is mechanical, the tests are dense, and the `ParamSlot` design keeps the
  key type `Copy`; if the spike shows the slot scheme is awkward in some file, the fallback is `Arc<str>` ids behind a type alias, which
  costs more touching but nothing structural.
* **Shader hangs and driver bugs** are real (a documented crash exists for a Direct3D 12 to 11 shared-texture path on recent NVIDIA
  drivers [R]); the watchdog and the "disable the instance" behaviour need to exist before user shaders are accepted.
* **Cross-vendor drift** (at most 1/255 in tests [M]): decide now that "preview equals export" means "on one machine, bit for bit;
  across machines, within a tolerance", and write that down.
* **ISF correctness**: compile rate is not correctness. Budget for a visual comparison corpus before any promise.
* **Solo-maintainer load**: five families is a lot. The order in section 9 is chosen so that each phase ends with something a user can
  use, and so that any phase after 1 can be cancelled without wasting the earlier ones.
* **What would change the recommendation**: a hard requirement for 4K scrubbing on integrated GPUs (favours B earlier); a decision to
  go closed-source (forces the LGPL path and hardware encoders only); evidence from S4 that process start dominates scrubbing (it did
  not in the one measurement made); a user base that wants specific OpenFX plugins (moves phase 6 up).

---

## 11. Questions for the maintainer

1. **Distribution**: will this be open source (GPL is acceptable), source-available, or closed? It decides whether ffmpeg can be bundled,
   which encoders can be offered, and whether OpenFX's free plugin sets can ship.
2. **Performance goals**: is 1080p60 preview on an integrated GPU the target, or 4K on a discrete GPU, or both? Is a *rendered* preview
   (the first seconds computed in the background, then played) acceptable for heavy stacks, as opposed to always-live?
3. **Colour**: do you expect log or HDR footage, and do you want linear-light compositing? It changes the working space and what every
   LUT means.
4. **Which plugin family matters to you first** (I assumed `.cube` then ISF)? And is *embedding* a shader or LUT in the project file
   by default what you want (3.6), or only on request?
5. **Transitions**: is the "overlapping clips on two tracks, with the transition on the front clip" model acceptable (3.4), or do you want
   a transition to be an object on a cut?
6. **Platforms**: which of macOS and Linux do you need to support and test, and is a CI machine available for them?
7. **Text objects**: should they eventually be generator effects (one idea for everything generated), or stay a model concept of their
   own?

### Answers (the maintainer, 2026-10-13)

1. **Distribution:** GPLv3. (This removes the LGPL-or-GPL fork of section 6: a GPL ffmpeg build may be used and shipped, with its source offered; see
   the design notes, section 6.)
2. **Performance goals:** acceptable for now.
3. **Colour:** linear-light compositing is an important feature; wants more colour spaces and asks whether ACES AP0 can be supported (design notes,
   section 1).
4. **First plugin family:** `.cube` first, then ISF; embedding a shader is the default, configurable in the project settings and the global settings.
5. **Transitions:** the model of 3.4 is acceptable.
6. **Platforms:** Windows, macOS and Linux eventually; no Mac for the far future; a Linux VM in VirtualBox, or WSL, for Linux testing (design notes, section 5).
7. **Text objects:** should eventually be generator effects.

OpenFX is shelved (section 7.4 and phase 6 are kept for reference only). Groups (clip groups and track groups with effects of their own) are wanted.

---

## Appendix A. What I verified for this document, and how

* Counts of uses of `PropertyId` (1250 in 79 files) and `EffectKind` (262 in 25 files), line counts of the files named in section 2,
  the shape of `Effects`, `Effect`, `PropertyId`, `Rack` and `Animation`: by grep and by reading the code on 2026-10-12.
* ffmpeg through a pipe, 4K H.264 (a synthetic clip, so real footage is slower): yuv420p 82.6 fps, nv12 79.3 fps, rgba 13.6 fps,
  rgba64le 3.6 fps; 1080p: yuv420p 403 fps, rgba 163 fps. Same as the helper's numbers. `pipe_bench.py`.
* ffmpeg start-up and single-picture cost: process start 19 ms; one 1080p picture, GOP 250, fast seek: 60 ms (at the start of the file)
  to 101 ms (20 s in); decode-from-start seek 291 ms; one 4K60 picture 2 s in: 266 ms (which includes decoding up to 120 frames of the
  GOP, so it is not the cost of starting). `seek_bench.py`. A synthetic clip, one machine.
* This round's facts about Windows and `.ffrs` (an archive that ffmpeg is reading can be appended to; it cannot be renamed over or
  deleted; confirmed again by a test that fails with "Access is denied" when the readers are not let go of before the file is
  written again).

## Appendix B. What is reported but not verified

The wgpu upload, readback, stack and compile-time measurements and the ISF compile rate are **[M]**: made by helpers' throwaway
programs on this PC, not re-run by me. Everything about crate versions and dates, OpenFX, CLAP, LADSPA, licensing, ISF licensing, the
ffmpeg build and shipping notes, and the other platforms is **[R]** from web pages and repositories read by helpers on 2026-10-12.
The helpers' own *not verified* lists are worth keeping in mind: pixel parity of ISF with a reference host; the compile rate on the
10 500 community shaders; zero-copy hardware decode into wgpu in a shipped crate; a real GPU hang; Metal, Intel and Apple numbers;
macOS and Linux behaviour of everything; the exact date of FFmpeg 9.0; whether any OpenFX host sandboxes crashes; and whether GPL
plugins can be hosted by a proprietary program (a legal question).
