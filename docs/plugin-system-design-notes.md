# Plugin system: design notes (colour, node graphs, a render process, groups, test machines, licensing)

Status: **draft**, written 2026-10-13 as the companion of `plugin-system-features.md`. The maintainer's answers to the long-term plan
(GPLv3; linear-light compositing; ".cube first, then ISF"; Windows first, Linux through WSL or a VM, macOS far away; text objects eventually
become generators; OpenFX shelved) and four questions are what this document answers:

1. Linear light, more colour spaces, and "can we support ACES AP0?" (section 1)
2. What data model C (a node graph) would allow, and what it costs (section 2)
3. What engine option D (a render-server process) would allow, and what it costs (section 3)
4. Whether groups come before or after plugin support (section 4)
5. Which test machines to use for Linux and macOS (section 5), and what GPLv3 means for the content and the libraries (section 6)

Evidence tags as in the long-term plan: **[V]** verified by me on this PC or in this repository; **[M]** measured here by a helper's throwaway program;
**[R]** reported by research helpers from web pages (model output, plausible, not verified); **[B]** my belief or opinion. Section 7 lists
what I ran myself.

---

## 1. Colour: linear light, more colour spaces, and ACES AP0

### 1.1 The short answers

* **Linear-light compositing is cheap to get right in the engine and expensive for everything built before it.** The engine does it by
  holding pictures in `Rgba16Float` in a *linear* working space: blending, scaling, blurring and placing are then physically sensible. What
  costs is that **every effect that was written for display-referred (gamma-encoded) pictures changes its look** when it is given linear
  values, and that includes today's hue shift and nearly every ISF shader.
  The fix is not to rewrite them: it is to let an effect type *say which colour domain it works in* and to have the engine convert around it
  (feature CM-10).
* **Yes, ACES AP0 can be supported, and I would support it as an interchange space and not as the working space.** I checked the arithmetic
  below with a throwaway script. AP0 is the space that holds *every* visible colour (it is what ACES2065-1 EXR files are in), which is why
  it is used for exchange and archives; for compositing the ACES documents recommend AP1 (ACEScg), whose primaries are real colours.
* More colour spaces cost little **if they are data**: a table of primaries and white point, a table of transfer functions, and two
  conversions built from them (a curve and a 3x3 matrix). What costs is *interpretation* (which space is this file in?) and *output* (what
  do I tag the export as?), not the arithmetic.

### 1.2 What I verified for this section (Python, `scratchpad/gpu_research/aces_check.py`, 2026-10-13)

| Check | Result |
|---|---|
| AP0 to AP1 matrix built from the published primaries and white (ACES white, xy 0.32168, 0.33767) | equals the published matrix to 6 decimals |
| sRGB (linear, D65) to AP0, with a Bradford adaptation from D65 to the ACES white | equals the published matrix within 0.0002 |
| sRGB (linear) to ACEScg (AP1) | equals the published matrix within 0.00005 |
| The three primaries of sRGB, Rec.709, Display P3 and Rec.2020 inside AP0 and AP1 | all inside both (components >= -0.0006: the tiny negatives are the red primaries lying *on* the edge of AP0 and my rounding of their coordinates) |
| AP0's blue primary | chromaticity y = -0.0770: **not a real colour** (every visible colour has y > 0); AP0 contains every spectral colour all the same (the smallest component for 460, 490, 520, 540, 620 nm is positive) |
| The same spectral colours in AP1 | **negative** for violet-blue, cyan and green (-1.14 at 460 nm): ACEScg cannot represent the most saturated visible colours; sRGB is worse (-1.67, -2.15) |
| ACEScg's red primary in AP0 | a hair negative (-0.0055): AP1 is not strictly inside AP0 |
| Half-float precision for linear values | 0.05 % to 0.1 % of the value at every level from sRGB code 1 up. Storing the same linear light in **8 bits** would map sRGB codes 1 to 5 to zero and code 20 to 2: linear light needs a float format |

**Not verified:** the exact xy of the spectral colours (typed from memory; the signs and the order of magnitude are what matter), the
curves of the ACES log encodings and of the camera log curves (nothing below depends on their numbers), and ACES 2.0's output
transform.

### 1.3 What "support a colour space" has to mean in this program

A colour space has: **primaries** (three xy points) and a **white point**, a **transfer function** (the curve between the code values
of a file and linear light), and for YUV video a **matrix** and a **range**. The program needs these at four places, and they are
separate features:

1. **Interpreting a source** (input): what are this file's primaries, transfer function, matrix and range? ffprobe reports the tags
   (`color_primaries`, `color_transfer`, `color_space`, `color_range`) when the file has them; many files do not.
2. **The working space**: what the engine's `Rgba16Float` numbers mean.
3. **Showing it** (display): what the monitor expects (today: sRGB-ish, 8-bit).
4. **Delivering it** (export): what the file is tagged as and encoded in.

Between them the program needs only **conversions**: input to working (decode the transfer function, multiply by a matrix), working to
display (multiply by a matrix, encode), working to output. All three are `curve, 3x3, curve`. A **view transform** (ACES's "look" with
tone mapping and gamut mapping; a film emulation) is *not* of that form: it is a 3-D table or an algorithm, and it is a creative choice more
than a conversion.

### 1.4 Design: three layers

1. **A colour space table** (data): name, primaries, white, transfer function id, with Rec.709/sRGB, Rec.2020, Display P3, ACEScg (AP1),
   ACES2065-1 (AP0) to begin with; adding Adobe RGB, Rec.601 and camera spaces is a row each.
2. **Fixed-function conversions**, written once in Rust (the reference, tested against the published numbers and against round trips) and once in
   WGSL (tested against the Rust one on a software adapter). Transfer functions: sRGB, BT.709, BT.1886, gamma 2.2/2.4/2.6, linear; ACEScc
   and ACEScct; later PQ and HLG; later the log curves of cameras (each is a published formula).
3. **3-D tables** for everything that is not that form (view transforms, creative LUTs): the `.cube` host (PH-01 to PH-04) is also the
   engine of ACES view transforms if those are baked to tables. **OpenColorIO** (BSD-3, compatible with GPLv3) is a way to *make* those
   tables from an ACES config, offline or in the program; whether it is worth a C++ dependency is a question for later (Q-04), not for the
   first release.

### 1.5 The colour domain of an effect (the one new idea)

Most effects, including today's hue shift and nearly all ISF shaders, assume values that are *display-referred* (sRGB-encoded 0 to 1).
In a linear working space they would see different numbers and look different (a colour matrix applied to linear values is not the
same as the same matrix applied to encoded values; a gamma-encoded shader's `mix()` mixes differently).

* An effect type **declares its domain**: *scene-linear*, *display-referred (sRGB)*, or *log (which)*. The engine inserts the conversion
  *before* and *after* the effect (a 1-D curve each, which is cheap and which an adjacent pair of effects with the same domain avoids).
* An instance can override the declaration (a user who wants hue shift in linear light).
* **Built-ins**: the hue shift declares *display-referred*, so the look of existing projects is **unchanged** by the move to a linear
  engine (CM-12, MG-01); new effects (blur, exposure, blend) declare *scene-linear*, because that is what they are correct in.
* **LUTs** need a declared *input* and *output* space on the instance (CM-11), because a `.cube` file says nothing about them.
* **Compositing itself** (the "over" operation, scaling, rotation, blurs) is done in the working space: that is what linear light is for.

### 1.6 Interpretation, assumptions and tags

* ffmpeg gives YUV; the **conversion to RGB moves from ffmpeg to the GPU** (the plan needs that for speed anyway: 4K YUV through a pipe is 80
  fps, RGBA is 13.6 fps) and uses the source's matrix and range. For a file with no tags the program must **assume**; ffmpeg's own
  default for untagged video is not a safe thing to inherit. I propose BT.709 for HD and BT.601 for SD, stated in the source's properties, and
  overridable. (The research helper's digest on what ffmpeg assumes, and how to ask it for raw YUV with the tags, is in section 6.)
* **Export** converts to the output space and **writes the tags**. Today's exports are untagged; tagging them is a visible
  improvement (players then agree with the editor).
* 10-bit and HDR sources are read as 10-bit (`yuv420p10le`) or 16-bit planes and converted on the GPU.

### 1.7 What this does to what exists

* The preview and the export stop being two separate colour pipelines (preview: ffmpeg RGB then egui; export: ffmpeg filters in
  YUV/RGB), and become one, which is a correctness gain by itself.
* **Existing projects**: the hue shift's look is kept by its declared domain; the *placement* of layers (scaling, rotation) and any *blending*
  happens in linear light and **will look slightly different at the edges of scaled pictures** (linear-light scaling is brighter on
  high-contrast edges). A project colour version (`legacy`) can keep the old behaviour; whether that is worth carrying is Q-05.
* The tests that compare the old export with the new engine will have to say **which look** they compare against: the oracle path stays in
  display-referred space and the engine is run in a mode that reproduces it (working space = display-referred, a documented
  "compatibility" working space), so that parity is provable *before* linear light is turned on. Turning it on is then a visible,
  reviewed change and not a hidden side effect of the port. **I propose this order** (parity first, linear second).

### 1.8 What the research added (helpers, 2026-10-13; [R] unless it says verified)

* **Verified by me today** (`untagged_matrix_check.py`, this PC's ffmpeg 9.0.2): for a video with **no colour tags**, ffmpeg converts YUV to
  RGB with the **BT.601 limited-range matrix at every size, 4K included** (a sample (Y, Cb, Cr) = (100, 150, 200) comes out (213, 31, 142),
  the textbook BT.601 value; BT.709 would give (227, 55, 144), which is what a `bt709` tag or `scale=in_color_matrix=bt709` gives). **So
  today's preview and export treat every untagged HD video as BT.601**, which is slightly wrong for HD material (reds and greens are
  shifted). The new engine must *choose* an assumption, and choosing the conventional one (BT.709 for HD) **changes the look of existing
  untagged HD projects**: this is a second reason, besides linear light, for a compatibility decision (Q-05).
* **Raw frames keep their tags out of band.** `ffmpeg -f rawvideo` with no `-pix_fmt` gives the decoded frames **untouched in their native
  pixel format** (a 10-bit file stays 10-bit; verified by the helper on a 16x16 test), and `ffprobe -show_streams` reports `pix_fmt`,
  `color_range`, `color_space`, `color_transfer`, `color_primaries` (a missing key means "unspecified"). So the plan is: **ffmpeg decodes
  and does nothing to the colours; the GPU converts** from YUV using the file's tags or our assumption. (A trap found by the helper:
  on this ffmpeg, `-colorspace bt709` as an *output* option **converts** the pixels instead of only tagging them; use the `setparams`
  filter to tag.)
* **OpenColorIO** is 2.6.0 (2026-10-01), BSD-3, so it is compatible with GPLv3. It has built-in ACES configs (ACES 1.3 since 2.2, ACES 2.0 since 2.5)
  and the ACES 2.0 output transforms as built-in operations. **It has no WGSL output** (GLSL, HLSL, MSL and others), there is **no established
  Rust binding** (hobby repositories only), and its API is C++. Baking a transform into a 3-D table (with a shaper) is standard practice,
  so **the `.cube` host is also the way to run any ACES view transform** (65-point tables are typical). Whether to take the C++ dependency is
  a later question: the first release needs only the fixed-function conversions.
* **The ACES 2.0 output transform is an algorithm, not a table**: a colour appearance model (JMh), a tone scale, chroma compression, gamut
  compression, then conversion to the display's primaries; it needs a few small precomputed tables (360 entries along the hue). It could be
  ported to a shader with those tables computed on the CPU, or baked to a 3-D table. Both are "later"; **no WGSL port was found**.
* **Camera log curves** (ARRI LogC3 and LogC4, Sony S-Log3, Panasonic V-Log, RED Log3G10, Canon Log 3, Blackmagic Film Gen5) have public
  specifications or open implementations (OCIO's built-ins are BSD-3, the ACES input-transform repository is Apache-2.0); nothing found
  suggests an agreement is needed to implement them (not legal advice; ARRI's PDF could not be read). Each is one formula and one matrix.
* **Showing linear Rgba16Float in the window needs our own pass.** egui-wgpu picks a *non*-sRGB 8-bit surface and assumes gamma-encoded
  values; `register_native_texture` needs `Rgba8Unorm`. So the preview does: working space -> view/display transform -> sRGB encode
  -> an `Rgba8Unorm` texture -> egui. (wgpu 30 added `SurfaceConfiguration::color_space`, so a real HDR display path exists in wgpu, but eframe
  does not expose it: **HDR display needs a patched or custom painter**, which is why CM-13 is "later".)

---

## 2. Data model C: a node graph

### 2.1 What it is

In **model B** (the plan's recommendation) the project is what it is today: tracks (layers), clips on them, an ordered list of effect
instances per object, keyframes in racks. In **model C** the project is a **directed acyclic graph of nodes**. A node has typed input
ports, typed output ports and parameters; edges connect an output to an input. Node types would be: a *source* (a media file), a *generator*,
a *filter effect* (one picture in, one out), a *merge* (several pictures in, one out, with a blend mode), a *transform*, a *time* node (offset,
retime), a *group* (a node that contains a graph, with some inputs and outputs exposed) and an *output*. The timeline is then **one node
type** (a sequence node that holds tracks and clips and gives one picture and one sound), or only a **view** of part of the graph.

### 2.2 What it would allow, concretely, in this program

| Capability | Under model B | Under model C |
|---|---|---|
| An effect that needs **two pictures** (a luma key, a displacement, a matte, a transition between two *named* clips) | one extra input slot, `Backdrop` (what is behind); anything else needs a new kind of slot | any picture in the graph, by wire |
| The **same source decoded once** and used by two branches | not possible: a clip is used once | natural: one node, two wires |
| A **reusable look** (a set of effects) used by many clips, edited once | copies of instances, edited one by one | an instance of one subgraph |
| **Masks and trackers** | new model concepts plus slots | nodes whose outputs are wired into effects |
| An effect applied to **part of a stack** (tracks 2 to 4) | a track group (section 4) | a subgraph |
| **Caching and invalidation** per intermediate result | the engine derives it from the plan | the graph *is* the plan: dirty flags flow along edges |
| **Parameter linking** and expressions | not planned (SC-07) | a wire from one parameter to another |
| A **render with several outputs** (the preview, a scope, an export of one branch) | one output | several output nodes |
| **Scripting and batch** (a template project rendered with different media) | edit the project file | edit the graph |

### 2.3 What it costs (here, not in general)

* **The program is built around the timeline.** Every window (timeline, drawers, properties, selection, the racks, the history) assumes
  tracks, clips and their time. A model whose primary form is a graph needs either a *second* interface (a node editor: a large piece of
  work, a new paradigm for the user, new selection, new undo granularity) or a timeline that is a projection of the graph (every edit of
  the timeline turns into graph edits, which is how products that have both do it, and each has a long list of cases where the two views
  disagree).
* **About 12 000 lines of tests of the project model and 29 journeys** state the timeline's rules (clips do not overlap, a drag keeps a
  start time, keyframes move with their clip...). They would all be rewritten as rules about a graph, or kept as rules about a view of it.
* **Time.** In a timeline every clip has one time (its place, speed and trim). In a graph a *time node* turns the time of everything
  behind it into another time; two wires into one merge may come through different time nodes. Caching and keyframe ownership then need an
  answer to "at which time is this node evaluated", per path. Resolve and Nuke have answers; they are part of what makes them hard.
* **Serialization and migration** of arbitrary graphs (cycles, dangling wires, type changes between plugin versions) are a long tail.
* **Undo**: today a step clones the whole project (`record_change(project_before)`), which is fine for tracks and clips and is not a
  design for a graph of thousands of nodes.
* **Risk of node spaghetti** for the simple edits that this program is built for (cut, move, speed, a colour effect).

### 2.4 My recommendation: C as a compile target, not as the model (medium-high confidence)

Keep **B as the authoring model**, as the plan says, and make the engine's **render graph** a real DAG that is *compiled from* the
project (the plan: tracks, groups, effect stacks, backdrop slots) for each section of frames (feature EN-04). That gives most of what
model C is wanted for **inside the engine**: sharing of intermediate results, a texture pool with reference counts, pruning of what is
not visible, caching with invalidation by instance and by input, group effects as subgraphs rendered to an offscreen target, several
output taps (the preview and a scope). What it does **not** give is a user-built graph: free wiring, a node editor, linked
parameters. It keeps a door open: the descriptor's **input slots** (`Source`, `Backdrop`, `Aux(n)`) are *bindings*, and a binding that
defaults to "the previous stage" can later be set to "the output of that other object", which is the first step towards a graph in the
model without a rewrite.

The conditions under which I would change my mind: keyers, mattes and masks become central (they are multi-input and want wires); or
a user-facing "look" or "template" feature is wanted (a subgraph that is reused); or scripting becomes a goal. None of them is on the
list today (question Q-30 asks whether a node editor is ever wanted).

Rough sizes [B]: the compile-target graph is part of the engine work (the plan's phase 1); a model-level graph with a node editor is XL
(several months) and touches every window.

### 2.5 Prior art for model C (helpers, [R])

* **Olive** (the open-source editor) rewrote itself around nodes for 0.2: tracks, clips, groups and time are node types. Its repository was
  created in 2017; the last push is 2024-12-05; every release is flagged a pre-release; the README says "alpha ... highly unstable" and the
  site says "Olive will return". I found no post-mortem; my reading is that **everything-is-a-node plus a full rewrite means years to parity and a
  format break**, which is the schedule risk of model C for a solo maintainer.
* **Nuke and Natron** are graph-native: demand-driven tile rendering, a cache per node, every parameter animatable or driven by an
  expression, and a headless renderer that is the same engine without the window. Natron's own to-do list shows where graphs hurt: caches
  that ignore their limits, renders that silently stall, a command-line render that renders nothing, serialization without round-trip checks.
* **Houdini's image graph (Copernicus)** recooks on every frame change; image graphs are not built for scrubbing video.
* **MLT** (behind Kdenlive and Shotcut) is the best documented *timeline over a graph*: a network of producers, filters, transitions and
  consumers, pulled lazily; the timeline is playlists (a track) in a multitrack in a tractor, with filters and transitions in a "field".
  Its documented limits are an 8-bit YUV bias, seek cost and single-threaded services. Your tracks-as-layers plus effect stacks already
  map onto it.
* **Smelter** (Software Mansion, Rust and wgpu): the user-facing description is a **component tree** (view, input stream, text, shader,
  tiles, rescaler, transitions), not a DAG; it is sent to a separate Rust wgpu server. This is the closest match to "describe a tree,
  compile it internally".
* **DaVinci Resolve** has a graph in its Fusion and colour pages and a timeline on its edit page, and its forums show the seam:
  edit-page transforms are not applied inside a one-clip Fusion composition.

---

## 3. Engine option D: a render-server process

### 3.1 What it is

A second program (`fastforwardrs --render-server`, or a second executable) that **owns the wgpu device** and does **all** the GPU work:
it turns a *scene description* (what to render, with which parameter values, at which time and size; the resources by hash) into pictures,
and runs the untrusted parts (user shaders, and as helpers, binary plugins). The editor process draws the windows and sends messages. Frames
come back through shared memory (or, later and riskily, shared GPU handles).

### 3.2 What it allows

* **A GPU hang or a driver crash does not take the editor down.** The render process is restarted; the instance that caused it is disabled
  and named. (Care: on Windows, a timeout-detection reset acts on the *adapter*; whether other processes' devices survive it is
  driver-dependent [B/unverified], so the editor still has to survive a lost device of its own. What D guarantees is that the process
  that was running the shader is not the one that holds your project.)
* **Sandboxing.** The render process can run with reduced rights (no files except the ones it is given, no network); a shader or a
  binary plugin in it can do less harm. This is the strongest answer to "a project from the internet embeds a shader" (SF-02).
* **Background export without freezing the editor**: the export runs in a render process from a *snapshot* of the project while the user
  goes on editing (today an export blocks editing; the window shows a progress dialog).
* **A command-line and batch interface for free** (`export project.ffrs out.mp4`), and a way to run engine tests with no window.
* **Several workers**: frame-parallel export on the CPU-bound parts (decode, readback, encode), several exports, or later a farm.
* **Plugin families as helpers**: binary audio plugins and any future hosts in processes of their own, with the same supervision.
* **A clean seam for testing**: the protocol is data; tests send scenes and compare frames, without any window.

### 3.3 What it costs

* **Frame transfer.** For the preview, every frame is *rendered, read back, copied through shared memory and uploaded again* into the
  editor's device. Measured on this PC by a research helper (4K): readback 3.0 ms (RTX 3090) or 7.2 ms (integrated GPU) for RGBA8, upload 2.3 to
  3.9 ms: **about 5 to 11 ms added per 4K frame, a frame of latency, and a quarter of that at 1080p**. Zero-copy sharing of GPU textures
  between processes has no safe wgpu API, goes through `unsafe` hal calls, and an NVIDIA driver crash with a Direct3D 12 to 11 shared texture
  path is documented for 2026 drivers [R]. So D's preview is *slower than in-process* by design; its export is not (export reads back
  anyway).
* **Complexity**: a protocol with versions (the two processes can be different builds), supervision, restart and resend of state, errors
  that cross a process boundary, two wgpu devices and two sets of textures in memory, packaging a second executable, and harder debugging.
* **State**: persistent effects, caches and resources live in the server; a restart loses them and the editor must rebuild.
* **Estimated cost** [B]: M to L on top of option A (a message schema, a server main loop, shared-memory frames, a supervisor): a few
  thousand lines and a long tail of lifecycle cases. The part that *cannot* be added cheaply later is the **shape of the engine's
  interface**: if the engine takes pointers into the program's data, D means rewriting the interface; if it takes a **self-contained
  scene description**, D is a transport added under it.

### 3.4 Recommendation (medium-high confidence)

**Do not build D now, and do not make it impossible.** Make the engine's interface *message-shaped* from the first day (EN-12): a `Scene`
that is plain serialisable data, resources referred to by content hash, results as plain frames; run it **in-process on a thread** first.
Then the three things D is best at can be added as separate, small decisions: a **command-line export** (needs only the message-shaped
interface), a **background export** in a child process (the first use of the render process, with no preview latency problem), and, if
shaders from the internet make it necessary, **sandboxing of the whole render process**. Under GPLv3 the old question of "aggregation"
no longer separates the processes for licensing purposes; the reasons for D are only robustness and security. For a solo maintainer, the
cheap part is the interface; the expensive part is the lifecycle.

### 3.5 Prior art for a render process (helpers, [R]) and what the numbers say

* **Chromium's GPU process** exists for security (the sandboxed renderer cannot reach the 3-D API) and robustness (a driver crash spares the
  browser). Frames and commands go through shared memory (a command-buffer ring, shared memory for big resources, "mailboxes" to share
  textures); a **watchdog terminates a hung GPU thread**; crashes are forgiven up to a limit and then it **falls back** (Vulkan, then GL, then
  a software rasteriser, then compositor-only). On Windows the GPU process is only low-integrity: the process that talks to the driver is
  the *weakly* sandboxed one.
* **Firefox** does the same: after a device reset or a driver crash it restarts the GPU process and **resends the display list, which is a
  self-contained blob**. Its WebGPU runs wgpu in that process, with serialized messages and shared memory, and Windows shared textures through
  a Direct3D 11 handle plus a fence.
* **After Effects** went from multi-process rendering to in-process threads (RAM-heavy workers); **Premiere and Blender's Cycles** GPU
  crashes are reported as whole-application crashes; **Nuke and Natron** render headless with the same engine; **mpv and libplacebo** are in-process libraries.
* **Numbers from a helper, CPU only (GPU readback not measured):** a 4K RGBA8 frame is 33.2 MB; a plain memory copy takes 1.3 ms
  (26 GB/s); writing into shared memory, signalling and copying out takes 3.0 ms; an anonymous pipe takes 26 ms (1.3 GB/s), and 60 frames a second at
  4K needs 2.0 GB/s. **Never stream frames over pipes.** At 1080p the shared-memory path is 0.65 ms. Together with the readback and upload
  figures of the earlier research (3 to 7 ms and 2 to 4 ms at 4K), a process split costs **roughly 6 to 12 ms per 4K frame and 2 to 3 ms at
  1080p**, plus a frame of latency, unless GPU textures are shared directly.
* **Sharing GPU textures** between processes has no public wgpu API: only `unsafe` hal imports (Vulkan from a Direct3D 11 shared handle on Windows,
  a DMA-BUF file descriptor on Linux for single-plane formats, merged in April 2026; Direct3D 12 only from a raw resource; Metal open). Exporting a
  shareable texture and its fences is up to us, in raw Vulkan or Direct3D calls.
* **Rust tooling** that exists: `ipc-channel` (serde, Windows tested, shared-memory blobs), `iceoryx2` (zero-copy publish/subscribe on three
  systems, MIT or Apache), `set_device_lost_callback` in wgpu.
* **The helper's list of what is cheap to keep possible** (and what I adopt, EN-12): the scene is an immutable, serialisable value so that a restarted
  engine can be sent everything; the engine's entry is `request_frame(scene snapshot, time) -> frame`, with GPU resources addressed by id and no
  UI types in it; a path that **rebuilds all GPU state from the scene** (needed anyway for device loss); a headless export binary that
  shares the engine crate; the engine on a thread of its own behind channels first. **Expensive:** zero-copy frame sharing; real plugin
  sandboxing (a looping shader can still trigger an operating-system-wide timeout reset: the helper's reasoning, not checked); crash
  recovery in the interface; egui handles drawn over a surface that another process owns.

---

## 4. Groups: before or after plugin support?

### 4.1 The answer

**After the descriptor-driven effect model (phase 0), before the plugin families are *finished*, and in stages:**

1. **Phase 0 makes the effect list generic over its owner** (a clip, a track, and later a group): without that, a group's effect list would be built on a
   closed enum (`EffectKind`, at most one of each kind) and rewritten. This is the one hard prerequisite.
2. **The structure of groups** (a tree of tracks with a mute, a gain and an isolated or pass-through flag; selection groups of clips) can be built **in
   parallel with the engine skeleton**, with the old renderer flattening groups into layers and **no effects on them yet**. It does not touch effects, so
   it can also come *first* if you want to organise big projects soon.
3. **Effects on groups, and adjustment clips** come when the engine can render a group to an offscreen texture (the plan's phase 1): the
   mechanism is "an effect host whose input is a texture", which is the same one that transitions and backdrop effects need.
4. **Sound sub-mixes and sound effects on track groups** come with the in-process audio graph (phase 4).
5. **Compound clips** (a nested timeline inside a clip) come last, **if** they are wanted: they are the biggest piece (Q-15).
6. The plugin **families** (LUT, ISF, sound) are tested **in every scope** (clip, track, group) from the moment their scope exists: the tests are
   written once, parameterised by scope.

Do **not** implement group effects in today's ffmpeg filter graph, which the plan removes. Medium-high confidence; the reasons follow.

### 4.2 Why that order

* **Groups multiply the places an effect can live.** Today an effect lives on a clip or a track. With groups it also lives on a clip
  group and a track group. If the effect list is still a closed enum (`EffectKind`: at most one of each kind), the group work builds more
  code on the thing phase 0 replaces. After phase 0, "an object has an ordered list of effect instances" is one concept and a group is
  one more object that has one.
* **A group's effect needs an offscreen picture**: render the children, composite them, *then* apply the effect to the result. That is
  what the engine's pass graph does naturally (EN-04) and what the ffmpeg filter graph would need a new, nested construction for, in code
  that is about to be deleted (the plan, phase 2). So the *effects on groups* belong after the engine's skeleton, and the *structure* of
  groups does not need the engine at all (the old renderer can flatten a group into layers; mute and gain of a group are arithmetic).
* **Group scopes decided before the plugin *families* are finished** (LUT, ISF, sound), because each family should be tested **in every
  scope** (clip, track, clip group, track group); adding a scope after a family is done means revisiting it. (The LUT host can be built before
  group effects exist, as long as its tests take the scope as a parameter.)
* **The transition model depends on groups.** A transition is an effect on the front clip with a backdrop input; the backdrop of a clip
  *inside a group* is the composite of what is behind it *inside the group*. That rule has to be decided with groups, not after.
* **What would argue for groups first**: if the maintainer wants to organise large projects *now* and the effects can wait. Then the
  structure stage (2) alone can come first, at no cost to the rest, because it does not touch effects.

### 4.3 What a group is (the two decisions the structure needs)

* **Track group**: a track that contains tracks. In this program tracks are already compositing layers (front to back), so a track
  group is the natural unit: *its children are composited, then its effects apply, then the result is a layer of its parent*. It also
  has a sound sub-mix (gain, mute, effects). This is the simpler of the two and I would build it first.
* **Clip group**: two different things share this name, and the maintainer's wording ("clip groups ... which can have their own effects
  applied") does not say which is meant:
  1. a **compound clip** (nested sequence): an opaque clip whose content is a timeline of its own (tracks and clips, with its own time), which
     can be opened and edited and which behaves like one clip outside (moved, cut, speeded, with effects); and
  2. a **selection group**: clips that move and are cut together on their tracks, that can also take an effect which applies to their
     composite.
  The first is far more powerful and far bigger (a nested timeline view, time inside and outside, media handles, "ungroup" that must give the same
  render). The second fits the current model (a set of clip ids) and the effect applies to the composite of exactly those clips over
  what is behind them (which needs the backdrop idea anyway). **This is question Q-15**, and the answer decides the size of the whole
  group feature.
* **Nesting** is allowed in both, with a depth limit for safety.
* **A third thing that the prior art offers, and that you did not ask for: the adjustment clip.** A clip with no picture of its own whose
  effects apply to the **composite of everything behind it** (Blender's adjustment strip, MLT's "FX Cut", Resolve's adjustment clip,
  After Effects' adjustment layer). Under the transition model you accepted (an effect on the front clip with a **backdrop** input) it costs almost
  nothing: it is a filter effect whose `Source` is the backdrop. It is the cheapest way to exercise the backdrop mechanism, it gives "one
  effect over several tracks for a stretch of time", and the helper's ranking put it **first** of the three structures. It is not selective
  (it affects every track behind it), which is what groups add. Whether it is wanted is Q-32.
* **Isolated or pass-through.** A group is *isolated* if its children are composited together first, then the group's effects and blend
  mode apply to that result (what "effects on a group" means); it is *pass-through* if the children are composited straight into whatever is
  behind the group (what Photoshop, GIMP and Krita offer as a second mode). Pass-through needs no offscreen target and means that a group with
  no effects costs nothing. The model needs the flag from the start (a group with no effects and no blend mode can be treated as
  pass-through by the renderer, which is a free optimisation); whether the *user* sees it is Q-33.

### 4.4 What the model needs, whichever is chosen

* A **tree**: a track has an optional parent group; or a group object has children. Ids as for clips and racks.
* The **effect list** of a group is the same type as the list of a clip (after phase 0).
* **Keyframes** of a group (gain, mute, its effects' values) are counted from the start of its parent's time, as a track's are.
* **Time** inside a compound clip: its own timeline starting at zero; outside, a clip with a speed and a trim, like a media clip.
* **Undo**: grouping, ungrouping, adding a child, are steps; **a test that ungrouping does not change the render** is a feature (GR-07).

### 4.5 Prior art for groups (helpers, [R]; several vendor manuals could not be fetched, so some of this is from summaries)

| Product | What a group is | Effects on it | Time inside |
|---|---|---|---|
| Blender (meta strip) | a container with its own strip stack, nestable, "UnMeta" restores | yes; an adjustment strip inside sees only the meta's contents | offset only, no speed |
| Premiere (nested sequence) | a clip that live-references another sequence; no un-nest (copy and paste) | as one clip | own time |
| After Effects (precomp) | own time; "leave attributes in" or "move them into" decides where keyframes live | an adjustment layer inside affects only the precomp; blend modes cross the boundary only with "collapse transformations" | own time |
| Resolve (compound clip) | a container; retiming it retimes everything inside; "decompose in place" loses the parent's grade | yes | retimed as a whole |
| Final Cut (compound clip) | contents shared by all instances; audio "roles" act as buses | yes | own time |
| **Kdenlive** | a **group** is only a *selection group* (Ctrl+G) with an option to propagate effect edits; a **sequence** (23.04+) is a nested timeline by reference | track effects apply to that track's clips only | sequences share one project profile |
| Shotcut | track and output filters; no compound clips (the maintainer said in 2025 he leaves them to Kdenlive) | track filters | none |
| MLT "FX Cut" | an **adjustment clip**: its own picture is discarded, its filters run on the composite of the lower tracks | yes | none |
| Nuke, Natron | a group is a subgraph with Input and Output nodes and exposed parameters | yes | none |

Recurring decisions and pitfalls: **isolation versus pass-through** (an isolated group is rendered to an offscreen target before its effects;
a pass-through group is not: Krita, GIMP and Photoshop offer both); **nested time and who owns the keyframes** (a rule that works: *keyframes
belong to the object whose timeline they are written in*; a group with no offset or speed avoids the whole problem); the **ungroup round
trip** (Premiere has none, Resolve loses the parent's grade, the Pitivi design says to disable ungroup while the group has effects); **effects that need the
backdrop** see only the nest they are in; and **audio**, where a track group maps onto a submix bus. I found no project that is documented as
having built groups first and regretted it; the two precedents that point towards "the effect model first" are Olive (its effect hooks
were hard-coded, so it was rewritten) and Kdenlive (it had to refactor per-timeline state before nesting).

---

## 5. Test machines: WSL2, VirtualBox, hosted CI

### 5.1 What I found on this PC (read-only checks, 2026-10-13)

* **WSL is not installed** (`wsl --status` prints only the usage text of the stub), **VirtualBox is not installed**, Windows 10 build 19045,
  AMD Ryzen 5 7600X, 32 GB of memory, **hardware virtualization is enabled in the firmware**, 385 GB free on `C:`.
* The repository's remote is `https://github.com/JohnDorsey/fastforwardrs.git`, so **GitHub Actions** is available.
* I installed nothing. Installing WSL needs an administrator PowerShell and a restart; I will not do it without being told to.

### 5.2 The options (helpers, [R]; none tried here)

| | WSL2 | VirtualBox VM | GitHub Actions |
|---|---|---|---|
| Linux, headless tests (`cargo test`, kittest on a software adapter) | **yes**; I can run it myself with `wsl.exe -d <distro> -e bash -lc "..."` | yes, if I can reach it (SSH with a forwarded port) | yes, on every push |
| A real window of the program | yes, through WSLg (Windows 10 build 19044 or later; needs the vendor GPU driver) | yes, in the VM's desktop | no |
| GPU | **no Vulkan GPU adapter in stock Ubuntu** (the Mesa driver for it, `dzn`, is not in Ubuntu's package; NVIDIA's WSL driver has no Vulkan ICD): wgpu gets the **software adapter (lavapipe)**, which is also what the tests want | VMSVGA gives OpenGL only: lavapipe again | software only |
| Audio | WSLg's PulseAudio server (cpal needs a small `~/.asoundrc` bridge or its Pulse backend) | an emulated sound card that Linux sees as ALSA | none |
| LADSPA plugins to test the host with | `apt install swh-plugins ladspa-sdk cmt` etc. (on Windows the first set is the maintainer's LMMS install: 119 x64 DLLs, verified by a byte check) | same | same |
| ffmpeg | Ubuntu's is a GPL build with x264 and x265 (version 8.0.1 in Ubuntu 26.04; this PC has 9.0.2, so flags can differ) | same | same |
| Cost and setup | one admin command and a restart; the repository and `target/` should live in the Linux filesystem, not on `/mnt/c` | create the VM, install Ubuntu, set up SSH | a workflow file; **free for public repositories** |
| Interaction with the others | **WSL2 and VirtualBox 7 on one machine make VirtualBox fall back to the Hyper-V API and run slowly** ("snail mode"); Microsoft calls the combination unsupported | the same conflict | none |
| macOS | no | no (**running macOS in a VM on non-Apple hardware is not permitted**, from memory) | **yes**: `macos-latest` (Apple Silicon); Metal in those virtual machines is unreliable ("GPU passthrough impossible" officially; some software has made it work), so build and headless tests are realistic, GPU tests are not guaranteed |

### 5.3 Recommendation (medium confidence)

* **Use WSL2 for Linux** if the maintainer agrees to install it: I can drive it directly, it is quick, it has the right test adapter,
  and it makes the program's Linux build visible on the machine that already has the code. **Choose it *or* VirtualBox, not both**: they
  slow each other. A VirtualBox VM is worth it only for things WSL does not show: a full desktop session, Flatpak and AppImage
  packaging, different window managers.
* **Add hosted CI on all three systems** (Windows, Ubuntu, macOS), which is the only practical macOS check and, for a public repository, free. It
  runs the build and the headless tests, with per-system tolerances for any picture comparisons.
* **No wgpu test should depend on a particular GPU**: the software adapter must run everything (PL-05).
* **Real GPU behaviour on Linux and macOS stays unmeasured** until a person with that hardware looks; the plan must say so.

### 5.4 Linux packaging, when it comes (helper opinion, medium)

**Flatpak first** (the runtime brings Mesa, Vulkan, Wayland, X11 and audio access; we ship our own GPL ffmpeg as a module), **AppImage second**
(it cannot carry GPU drivers, so it needs the host's Vulkan driver, and older systems need `libfuse2`), distribution packages last. eframe's
defaults include both Wayland and X11; the helper found reports of Wayland trouble with wgpu (a "surface already has a syncobj attached" error,
scale-factor bugs), and WSLg runs Weston with XWayland, so both can be tried there.

---

## 6. What GPLv3 means here

*A technical survey by research helpers (reading licence files, headers and the FSF's own pages), not legal advice. A lawyer should read
the result before anything is released; the notes say what the sources say.*

### 6.1 Shaders and LUTs that could ship with the program

| Content | What the sources say [R] | Can it ship in a GPLv3 program? |
|---|---|---|
| **ISF-Files** (the Vidvox collection, 327 shaders) | the repository is MIT; of 327 files about 70 carry their own licence line (about 67 MIT, 2 BSD, **1 CC BY-NC-SA 3.0**: `Boxinator.fs`); about 257 carry none and rely on the repository's MIT; 68 are converted gl-transitions; about 87 name authors other than Vidvox; **6 mention Shadertoy with no licence**. A repository-level MIT cannot relicense other people's code, and Vidvox staff said so | the MIT, BSD and ISC ones yes (with notices); `Boxinator` no; the Shadertoy-derived ones no unless their own header says MIT or CC0 |
| **gl-transitions** (125 transitions) | all headers read: 123 MIT, 1 BSD-3, 1 BSD-2; none under a non-commercial licence | yes, with notices |
| **Shadertoy** shaders | the site's default licence is CC BY-NC-SA 3.0 unless the author says otherwise; the FSF says "non-commercial" is not free, and GPLv3 forbids added restrictions | no, unless the header says MIT or CC0 |
| **The ISF specification** | MIT | yes |
| **Q-DDL LUT pack** (800+ `.cube` files) | reported as CC BY 4.0 (the pack's own licence file was not seen); the FSF lists CC BY 4.0 as compatible with all GPL versions | yes, as data with a credits file, once the licence file is checked |
| **Camera-vendor LUTs** (Sony, ARRI, Adobe) | their own licence agreements | no |
| **LUTs we make ourselves** (identity, warm, cool, teal-orange) | ours | yes, as GPLv3 |

**The policy this suggests (and LG-02 in the features list):** ship only content whose licence is written in its own header or a file next to
it and is MIT, BSD, ISC, Apache-2.0, CC0, Zlib or CC BY; keep a manifest (name, author, licence, URL); reject files with no licence;
and let users load **any** shader or table from disk, which is not bundling. The `.cube` *specification* document is licensed CC BY-NC (the PDF), but
the format is a tiny text grammar that ffmpeg, ImageMagick and others already implement: write our own parser and our own description, and
do not paste text or examples from the specification.

### 6.2 ffmpeg

* **Measured by me:** the ffmpeg on this PC is a GPLv3-or-later build (`ffmpeg -L`), configured with `--enable-gpl --enable-version3` and
  `libx264`, `libx265`, and **no** `--enable-nonfree`.
* ffmpeg's own licence rules [R, from its `LICENSE.md`]: the default is LGPL-2.1+; `--enable-gpl` makes it GPL-2+ and is needed for x264,
  x265, xvid, rubberband, vidstab, frei0r and others; `--enable-version3` makes it (L)GPL-3; **`--enable-nonfree`** (fdk-aac, DeckLink, CUDA
  SDK) gives a build that **cannot be redistributed**. The Windows builds by gyan.dev and BtbN's `gpl` build are GPLv3; **never ship the
  "nonfree" variants**.
* **A GPLv3 program may use or ship such an ffmpeg**, as a separate executable (the GPL FAQ's "mere aggregation") or linked (the whole is then
  GPLv3). Either way **we must offer the corresponding source of the binary we ship** (GPLv3, section 6): host a copy rather than rely on a
  link to someone else's page. Patents on H.264, HEVC and AAC are a separate matter and were not checked.
* Consequence for the long-term plan: the **LGPL-only restriction (hardware encoders only) disappears**, and in-process linking
  (option B) is no longer a licensing question, only an engineering one.

### 6.3 Plugin formats and hosting non-GPL plugins

* **The formats are fine for a GPLv3 host** [R]: CLAP is MIT, LV2 is ISC, the VST3 SDK is MIT (since 3.8.0), `ladspa.h` is LGPL-2.1-or-later, OpenFX
  (shelved) is BSD-3.
* **The open question is the plugins themselves.** The GPL FAQ says that fork-and-exec without intimate data sharing makes separate programs, that
  dynamic linking with shared data structures makes one combined program, and that shared memory with complex data is "pretty much equivalent to
  dynamic linking". A proprietary plugin loaded into a GPL host is therefore a debated case. **Ardour** (GPL) ships an explicit
  "Plugin Clarification" in its `COPYING` saying that plugins loaded through VST, AU, LV2, LADSPA or CLAP have no derivative relationship with the
  host and may have any licence; that is the developers' stated understanding, not a court ruling. The formal alternative is an *additional
  permission* under GPLv3 section 7. **Only a copyright holder can grant it, which is easy for a sole author now and hard after other people contribute.**
  The helper's advice, and mine: **decide now** whether to add an Ardour-style statement (Q-34).
* A **GPL plugin in a GPL host** is fine if the plugin is GPLv3-compatible. A GPLv2-*only* plugin loaded in-process is a grey area (the FAQ says v2 and v3
  code cannot be combined); in practice it is treated as a separate program.
* Scanning plugins in a child process (DS-04) is a separate program in the FAQ's sense; **running** a plugin in the program's own process is the
  case the questions above are about. [B]

### 6.4 The libraries we already use

All 232 crates in the dependency tree are compatible with GPLv3 (measured from `cargo metadata` by a helper). Notes: **15 or more crates are
Apache-2.0-only** (including `cpal`, `winit`, `ab_glyph`): fine for GPLv3, and they would already stop the program from being GPLv2-only. `libloading` is ISC; `option-ext`
is MPL-2.0 (file-level copyleft, unmodified: fine); `self_cell` is "Apache-2.0 OR GPL-2.0-only" (use the Apache option); 18 `icu_*` crates are
Unicode-3.0; `epaint_default_fonts` includes the **Ubuntu Font Licence, which is not on the FSF's list** (ship those fonts as separate files with
their notices, or choose others: a small item to check). **`Cargo.toml` has no `license` field yet.** Candidate dependencies of this plan: `clack` (CLAP
host; MIT/Apache), `lcms2` (MIT), OpenColorIO (BSD-3), the `lv2` and `ladspa` crates, the `vst3` crate (avoid `vst3-sys`, which has no recognised licence).

### 6.5 Mechanics (a checklist for the plan, phase 0)

* **GPL-3.0-or-later or GPL-3.0-only?** The FSF recommends "or later"; the SPDX identifier is `GPL-3.0-or-later`. "Only" freezes the
  licence at version 3 and makes any later change need every contributor. (Q-34.)
* A root `LICENSE` file with the GPLv3 text; `license = "GPL-3.0-or-later"` in `Cargo.toml`; an `SPDX-License-Identifier` line in each source file (`reuse
  lint` can check it); a `THIRD_PARTY` notices file (`cargo-about` or `cargo-deny` can make or enforce it); the ffmpeg source offer; the manifest
  of bundled shaders and tables (6.1).
* No contributor licence agreement is needed (inbound equals outbound); the FAQ's only copyright caveat is a claim by an employer or school.
* Not checked by anyone: the text of the Adobe `.cube` PDF; the licence file of Q-DDL; the per-file licences of the 87 ISF files by other authors; whether the Ubuntu
  Font Licence is compatible; patents; whether a link to gyan's sources satisfies the source offer.
---

## 7. What I verified, and what is only reported

**Verified by me on 2026-10-13** (throwaway scripts in the scratchpad, all re-runnable; the numbers are in the sections above):

| Claim | How | Result |
|---|---|---|
| The ACES matrices (AP0 to AP1, sRGB to AP0, sRGB to ACEScg) follow from the published primaries and white points | `aces_check.py`: built from primaries with a Bradford adaptation | match the published matrices within 0.0002 |
| AP0 contains every spectral colour, AP1 does not; AP0's blue is not a real colour | same script | confirmed (signs and magnitudes; the xy of the spectral colours were typed from memory) |
| Half floats are precise enough for linear light from sRGB code 1 up; 8-bit linear is not | same script | 0.05 to 0.1 % relative; 8-bit linear maps codes 1 to 5 to zero |
| ffmpeg converts **untagged** YUV with **BT.601 limited range at every size** | `untagged_matrix_check.py` (raw yuv444p at 720x576 up to 4K) | (213, 31, 142) at every size; BT.709 with an explicit `in_color_matrix` gives (227, 55, 144) |
| The ffmpeg pipe speeds (4K yuv420p 82 fps, RGBA 13.6 fps) and the cost of one ffmpeg start (19 ms) and one 1080p picture (60 to 100 ms) | the scripts of the previous round | (the long-term plan, appendix A) |
| WSL and VirtualBox are not installed; virtualization is on in the firmware; the repository's remote is on GitHub | PowerShell and `git remote -v` (read only) | section 5.1 |

**Reported, not verified:** everything else about OpenColorIO, ACES, camera logs, the prior art of groups, node graphs and render
processes, WSL2, VirtualBox, GitHub's runners, and licences. Each helper's own list of what it could **not** verify:

* *Colour:* the crates.io state of any OCIO crate; whether OCIO's ACES 2 shader uses textures or code; what "ACES 2.1" changes; Canon Log 3
  and RED details; ARRI's PDF footer; any WGSL port of the ACES 2.0 output transform (none found; the search was shallow).
* *Groups:* Premiere and After Effects details (their help pages could not be fetched); Resolve's official wording; offline media inside groups;
  undo granularity of groups; whether any project regretted building groups first.
* *Graphs and render process:* Olive's own rationale (from snippets); Cavalry; Fusion internals; Windows timeout-reset behaviour
  in the face of a *second* process's device; GPU readback bandwidth; any benchmark of `ipc-channel` or `iceoryx2`.
* *Test machines:* that WSL or WSLg works **on this machine**; VirtualBox 7.2 specifics; Metal reliability on hosted runners today; whether
  `wgpu` really picks `dzn` in WSL2; whether the ALSA-to-Pulse bridge works with cpal; xvfb with lavapipe for real eframe windows.

## 8. What changed in the features list because of this document

(The list is `plugin-system-features.md`; the identifiers are its.) CM-10 (the colour domain of an effect) and its consequences for the
hue shift and for LUTs; CM-12 and Q-05 (two reasons for a compatibility decision: linear light, and today's BT.601 assumption for untagged
video); CM-17 (YUV conversion on the GPU from raw frames and tags); CM-06 refined (AP0 as interchange, ACEScg as the working option, the
ACES 2.0 view transform as a later, baked or ported step); EN-12 (the helper's list of what keeps a render process possible); GR-09 and GR-10 and
Q-32 and Q-33 (adjustment clip; isolated or pass-through groups); QA-06 and Q-19 (WSL2 *or* VirtualBox, and hosted CI for macOS).
