# Shader inputs driven by a mask: design and costs (2026-11-01)

*Labels: **[V]** = measured or run today; **[B]** = believed from reading or reasoning, not run. The raw numbers, the harness and how to repeat them are in [`shader-masks-measurements/`](shader-masks-measurements/README.md).*

## 1. What was asked, and what is decided

A shader's number input (`float`) today has one value, from a slider. The maintainer wants it to be able to **vary across the picture**: a button next to the keyframe buttons lets another layer be chosen as the source, and a channel of that layer (R, G, B, or brightness) gives the value at each pixel. Decided by the maintainer:

* **While an input is driven, its slider is ignored and hidden.** (Its keyframes are kept, as the project keeps everything; they matter again when the input is let go.)
* **Only tracks are sources, to begin with.** To be able to put a track to this use without it being in the picture, **a track has an eye** (done: `f383d15`, file format 23). Later sources may be **contiguous clip groups, track groups and virtual layers** (a generator that is not on a track).
* **Later, arbitrary mappings** from the mask's value to the value used: a graph of line segments the user draws.
* **The shader files are not edited** by the user. If a shader has to be changed to do this, the program does it when the user asks, with the button.

## 2. The short answer

* **Can it be done without editing the shaders? Yes [V].** The program already rewrites every shader's GLSL before it compiles it (`src/isf/rewrite.rs`), and each input is a macro of the rewritten text (`#define radius (_isf_u[5].y)`). A driven input is a different macro. The user's file, and the copy of it kept in the project, do not change; what the project keeps is *which input is driven by what*, and the rewrite is done again, **in about 0.4 ms** [V: 0.33 ms for a shader that is not driven, 0.39 to 0.41 ms for one that is, a release build].
* **Can it be made to read the mask once per pixel? Yes [V]**, in a few dozen lines of the rewriter (`shader-masks-measurements/rewrite_poc.diff`): a global (`_isf_driven_radius`) that a generated function fills in from the mask before the shader's `main` runs (and before the initialisers of its globals, which may use the input). The generated WGSL has one read of the mask, before the loop, and the shader makes the same picture [V: with a mask that is the same everywhere it matches the shader that reads a uniform to within 2 of 255 (rounding), in at most 5,735 of 8.3 million bytes; with a mask that changes it differs where the mask differs from the uniform, as it must].
* **What it costs.** *(Corrected 2026-11-01, after the engine was built and measured: this sentence first said "about as much as one more video layer, and nothing but a texture read if it does not change", which the engine's numbers do not bear out.)* As built, **a mask is drawn and made into codes in every frame, even one that does not change**, and that costs **0.5 to 0.9 ms at 720p, 1.3 to 1.8 ms at 1080p and about 7.4 ms at 4K on the AMD graphics** (and nothing you can measure on the RTX 3090 below 4K), **before** the decode and the upload of a mask that is a video (section 4.1) are counted. Keeping a mask that does not change from one frame to the next is not built (section 4.4 says what it would save). The read of the mask in the shader costs **nothing you can measure beside the shader's own work** on the two adapters tried (section 4.2).

## 3. How it works, and why it stays simple

```
                       per driven input of an effect
   source (a track)  ──►  channel (R G B brightness)  ──►  map (a line / later a graph)  ──►  the value the shader sees
        │                                                        applied once per pixel, before main
        └─ rendered off the frame, as a picture of its own (the size of the frame), once for all the inputs that use it
```

**One idea carries all the sources.** The engine already has `LayerSource::Group`: *layers put together in a picture of their own, the size of the frame, clear where nothing is*. A track is a group of its clips' layers; a track group is a group of tracks; a contiguous clip group is a group of its clips; **a virtual layer is a group of one generator layer**. So a mask source is *a group that is drawn off the frame*, and what is new is only a way for an effect to name one. Nothing else about the engine has to know which kind of source it is. (**[B]**: I read this in `engine/scene.rs` and have not built it; it is the part of the work most likely to hold a surprise: see phase 2.)

| Piece | What it is | Where it lives |
|---|---|---|
| a driven input | `{ input, source, channel, map, value where the source has nothing }`, kept with the effect's values; the slider's value is kept and unused | the project (file format 24) |
| `source` | `Track(TrackId)` now; `TrackGroup`, `ClipGroup`, `VirtualLayer` later | an enum in the project; one function in `composition` turns it into a group description for a frame |
| `channel` | red, green, blue, brightness (of the codes) | an enum; the shader text says which |
| `map` | `Linear` (the input's MIN to MAX, to begin with) now; a **graph** later | an enum; a graph is **data, not shader text** (see below) |
| the off-frame groups | `FrameDescription` gets a list of them, drawn first; an effect call names one by its place | the engine |
| the value in the shader | one read of the mask per pixel, before `main`, per source; the channel and the map are a few operations on it | the rewriter |

**Why a graph must be data and not text.** A line is two numbers and can be a constant in the shader, but a graph that the user drags would make the program compile a new shader at every mouse move (0.4 ms each: affordable, but the cache of compiled shaders would fill with versions). A graph baked into a **256-entry one-dimensional texture** (1 KB) is read once per pixel with one more texture read, and an edit is a 1 KB upload. **[B]**: I did not measure the extra read; it is one small cached read of the same kind as the mask's, which was not measurable beside a shader's work. The shader text then has the same shape for every map, and only `Linear` needs no texture.

**Limits and checks that keep it simple**

* **Which inputs get the button.** `float` inputs. A `long` (a menu) or a `bool` is not a value that can vary smoothly. A float that **is used in the size of a buffer or of a pass** (the `WIDTH`/`HEIGHT` expressions of `PASSES`, worked out once for the whole frame by the CPU) cannot vary across the picture: the button is greyed, and says why. **[B]** `isf/size_expression.rs` knows which names those use; I have not checked that it can be asked.
* **A `const` that uses the input** does not compile today either (naga cannot make a constant from a uniform [V: the plain shader is refused with *Cannot access the type*]), so driving it changes nothing.
* **The picture of a mask is a shader picture like any other**: normalised coordinates, bilinear reads, so it does not matter whether the mask and the frame are the same size.
* **A cycle** (a mask whose own effects are driven by the layer that it drives) is refused when the source is chosen, and the plan puts the off-frame groups in an order that follows what each reads.
* **One texture for each different source**, shared by all the inputs that use it. A shader may read at most 7 pictures (its own, and 6): at most 6 different sources for one effect.
* **A source that is not in the picture is not a problem**: the eye (`hidden`) only keeps a track out of the frame; a track that is a mask is drawn off the frame whether or not its eye is open. A track that is hidden and not a mask is not decoded at all [V: `playback/wanted.rs`, tested].

## 4. What it costs

Measured on the maintainer's computer: a Ryzen 5 7600X, and two graphics adapters, an **NVIDIA GeForce RTX 3090** and the **AMD Radeon graphics of the processor** (the adapter the project's gate, 720p at 60 frames to the second, is defined on). All times are milliseconds.

### 4.1 A picture of video: decode, send to the card, draw into a texture

A mask that is a video has to be made like any other picture of video, every frame. **[V]**

| One picture | 1080p, RTX 3090 | 1080p, AMD | 4K, RTX 3090 | 4K, AMD |
|---|---|---|---|---|
| decode on the processor (H.264 test clip; scaled to the 720p preview in brackets) | 0.48 (0.66) | 0.48 (0.66) | 2.4 (3.1) | 2.4 (3.1) |
| send the planes of the decoder (8 bits; 3.0 MB at 1080p, 11.9 MB at 4K) | 0.16 to 0.27 | 0.81 to 0.91 | 0.59 to 0.90 | 3.0 to 3.1 |
| (the same as RGBA, for comparison: 7.9 MB, 31.6 MB) | 0.39 to 0.60 | 0.86 to 1.07 | 1.6 to 2.3 | 3.5 to 5.7 |
| draw the planes into a texture as colours | 0.017 | 0.35 | 0.07 | 1.4 |
| **memory of one more texture** (8 bits of each colour; half floats are twice that) | 7.9 MB | 7.9 MB | 31.6 MB | 31.6 MB |

* **One more live mask ≈ decode + send + draw**: the first estimate was **1.8 ms at 1080p and 6.9 ms at 4K on the AMD graphics** (the decode is on the processor and overlaps the card's work, so it costs less than the sum). **That estimate left out most of what the engine does with a mask**: its "draw" was the planes of a video picture drawn into a texture (0.35 and 1.4 ms), and the engine also puts the mask's group together in half floats and makes it into the codes of a screen with a pass of its own (section 4.4: 1.3 to 1.8 ms and about 7.4 ms for those two, for a mask that is a picture already on the card). **A live video mask is the engine's cost of 4.4 plus the upload of 4.1, and its decode on the processor** (the engine's 1.3 to 1.8 ms and the upload's 0.8 to 0.9 make 2.1 to 2.7 ms at 1080p, and 7.2 to 7.6 and 3.0 to 3.1 make 10.2 to 10.7 ms at 4K, on the AMD graphics; the decode, 0.5 ms and 2.4 ms, is on the processor and overlaps the card's work). For scale, the program's own benchmark makes the whole stack (four layers, two blurs, a matrix, a lookup, new video in every frame) in **9.5 ms at 720p, 22.8 ms at 1080p and 97 ms at 4K** on the AMD graphics, and in **2.5 ms at 1080p and 9.3 ms at 4K** on the RTX 3090. The preview draws at 720p at most (`playback::preview_picture_size`), so the preview pays less than the 1080p column; the export pays the full size but does not have to keep up with a clock.
* **Sending the planes is 2 to 3 times cheaper than sending RGBA**: the program's choice to send what the decoder gives and to convert on the card is right, and a mask should do the same.
* **The decode of real footage** takes longer than the test clip's (a camera's 10 to 100 Mbit/s against 2.6 and 7.4 here): **[B]** not measured.
* **A mask that does not change** (a picture, a generator that does not move, a still) could be drawn once and kept, so that its cost per frame would be the read in the shader, below. **It is not built**: see 4.4.

### 4.2 Reading the mask in a shader, at every use or once

Shaders with a loop of K rounds that read the picture once in each round and use the input twice in each round (`loop8`, `loop32`, `loop128`), and one with branches that depend on the picture (`b32`), run as the program's own compiler makes them. Time of one full-screen pass, as a share of the same shader with the input from the uniforms (`u`), at 1080p; 4K gives the same shares to within 0.07. **[V]**

| | K=8 | K=32 | K=128 | b32 (branches) |
|---|---|---|---|---|
| **RTX 3090**: mask read at every use (`e`) | 1.12 | 1.03 | 1.00 | 0.93 |
| **RTX 3090**: mask read once per pixel (`o`) | 1.13 | 1.03 | 1.00 | 0.94 |
| **AMD**: `e` | 0.80 | 0.82 | 0.84 | **1.09** |
| **AMD**: `o` | 0.81 | 0.82 | 0.84 | **1.03** (1.05 at 4K) |
| either: the mask read at a place that changes in every round (`v`) | 1.7 to 1.8 | 1.8 to 1.9 | 1.8 to 1.9 | 1.3 to 1.5 |

(The absolute times: the RTX 3090 takes 0.04 to 0.5 ms for these passes at 1080p, the AMD graphics 1.3 to 18.6 ms; the passes with K=8 on the 3090 are so short that the 0.005 ms of the mask read shows as 12 %.)

* **Reading the mask at every use is not worse than reading it once, in a loop that always reads it, on both adapters**: the driver takes the repeated read out of the loop. The compiler of the program does not (the generated WGSL has the read in the loop [V]); the driver's does. **Where a use is inside a branch that depends on the picture, the AMD driver does not**: reading at every use costs 9 % more than a uniform, and reading once 3 to 5 % more. **So read once**: it costs nothing where it makes no difference and saves where it does, and it does not depend on a driver that was not tried (Intel, Apple's, Mesa's were not).
* **A read at a place that changes in every round costs 1.3 to 1.9 times**, because it is a second texture read for every round. That is not what is being built (the value is the mask's at the pixel being made, not at each sample of the shader's own loop), but it is what a person gets if they put the mask in the shader as a picture and read it by hand.
* **An unexplained result, reported as it was measured**: on the AMD graphics the shaders that read a mask were 16 to 25 % *faster* than the one with the uniform, in every loop shader, with the same picture made (checked pixel by pixel), with a flat mask (every shader reads the same places), with the variants taking turns and with the first one measured twice. The cause is not known; it is probably how that driver compiles the read of a uniform in a loop. It does not change the conclusion, which is that a mask costs no more than a slider.

### 4.3 The other costs

| What | Cost | |
|---|---|---|
| **recompiling the shader** when the button is clicked or a source is changed | 0.4 ms (release build) | [V] for a 20-line shader; a long one of the collection was not timed |
| **the look-up of a source** in the plan | a group of layers, built by the same code as any group | [B] |
| **a track that is hidden and is not a mask** | it is not decoded, not sent, not drawn: the eye is a saving | [V] |
| **a track that is hidden and is a mask** | its decode, send and draw (4.1), once, however many inputs read it | [B] |
| **the file** | a driven input is a few numbers; a graph is a few points | [B] |

### 4.4 What the engine takes for a mask, as built (phase 2) [V]

The engine's own benchmark (`cargo run --release --example engine_benchmark -- --adapter N --size WxH --frames 60 --in-flight 1`) has two scenes more since phase 2: **four pictures with a shader of one pass that reads no mask**, and **the same shader reading a mask** that is a group of one picture (not video: it is on the card already, so nothing is decoded or sent). What the second takes more than the first is what the engine does for a mask: **the group is put together in half floats (one pass), made into the codes of a screen (one pass), and read by the shader (one more texture read per pixel)**. Mean time of a frame in milliseconds, the raw tables in `shader-masks-measurements/results/engine_mask_scenes_*.txt`:

| | no mask | with the mask | the mask takes |
|---|---|---|---|
| AMD, 720p (three runs) | 2.02 to 2.11 | 2.63 to 2.89 | **+0.52, +0.75, +0.87** |
| AMD, 1080p (two runs) | 4.63 and 4.90 | 6.45 and 6.19 | **+1.82, +1.29** |
| AMD, 4K (two runs) | 20.46 and 20.38 | 27.63 and 27.94 | **+7.17, +7.56** |
| RTX 3090, 720p | 0.83 and 0.92 | 0.92 and 0.93 | +0.09, +0.01 |
| RTX 3090, 1080p | 1.99 and 1.93 | 1.85 and 1.90 | −0.14, −0.03 (nothing you can measure) |
| RTX 3090, 4K | 5.92 and 6.58 | 6.40 and 6.10 | +0.48, −0.48 (nothing you can measure) |

* **Runs differ by up to 0.4 ms from one another on the AMD graphics at 1080p**, so the figures are ranges. The scenes are in the compatibility mode (the numbers are codes already), where the pass that makes a mask into codes copies; **in light it does the conversion, which is the same pass with more arithmetic** (not measured).
* **The cost is the two full-frame passes**, which the AMD graphics (integrated, sharing the memory of the processor) pays for in bandwidth: a picture of the frame in half floats is 7.4 MB at 720p, 16.6 MB at 1080p and 66 MB at 4K, and the mask makes two of them (the group, and the group in codes), each written once and read once. The discrete card has the bandwidth to hide them.
* **What can be saved**, in order of what it would give, and none of it is built: (1) **keep a mask that does not change** (a picture, a colour, a generator that does not move) between frames, which saves both passes (the whole of the cost above, but the read in the shader); (2) **draw the mask at half the size of the frame** (a quarter of both passes; a mask is usually smooth); (3) **do the conversion into codes in the shader's one read of the mask**, which saves the second pass but puts the working space's curve and primaries into the generated GLSL; (4) leave out the second pass in the compatibility mode (it copies). Each is a change in the engine and not in the design, and all of them leave `EffectCall::masks` as it is.
* **The gate** (720p and 60 frames to the second on the AMD graphics) is the whole stack of the benchmark in 8.4 ms, with 16.7 ms to spend: one mask is 0.5 to 0.9 ms of it.

## 5. A plan, in pieces that can each be checked

0. **Done: the eye** (`f383d15`): a track can be hidden; the plan leaves it out; file format 23.
1. **The rewriter** (**done**): `rewrite_with_driven_inputs` / `compile_with_driven_inputs` in `src/isf`: a driven input, read once per pixel, with tests. As built: each mask is one more image of the shader (`_isf_mask_N`, after the images it has; `ImageSlot::Mask(N)` in the engine's description); the value of an input is `mix(its own number in the call, MIN + (MAX - MIN) * unit, how much the mask covers)` where `unit` is the channel of the mask's colour with its opacity divided out (0 where nothing covers); an input that sets the size of a buffer, a name that is not a float, an input twice, or masks that skip a number are refused with a message. **Checked [V]**: the rewritten text of all 218 shaders of the collection is byte for byte what it was (SHA-256 of each, before and after); **all 520 float inputs** compile read from a mask alone, and 195 shaders compile with all their floats read from three masks at once (`every_float_input_of_the_corpus_can_be_read_from_a_mask`, run by hand); none of the 218 shaders has a float that sets a size, so that refusal is tested only on a shader written for the test. Not checked yet: the pixels (phase 2 draws them).
2. **The engine** (**done, except keeping a mask that does not change**): the mask is **a group on the effect call** (`EffectCall::masks`), not a list in the `FrameDescription` as first thought: the engine already numbers a group after the container its layer is in and draws the containers from the last to the first, so a mask is drawn before the effect that reads it with no new ordering, and no cycle can be written (the tree is finite; the project refuses a track that is its own mask). `EffectStep::Mask { call, mask }` makes each mask into codes right before the call (as `EffectStep::Backdrop` does for the backdrop). A call is run only if it has as many masks as its shader reads. Tested with no graphics card (the plan, the steps, the numbering, the fingerprint) and on the software adapter with the real rewriter (the channels, the range, the number of the call where the mask has nothing, partly see-through masks, two masks, masks in groups and groups in masks, effects in a mask, loops, light and codes); two deliberate mistakes (no conversion into codes, no division by the opacity) are each caught by the tests made for them. Checked with the engine's own benchmark, which gets a scene with a mask, against the numbers in section 4.
3. **The project, the file and the plan** (**done**): `Effect::driven_values()` (`DrivenValue { source: MaskSource::Track, channel, value_where_nothing }` by the place of the number; only a number with a range of a shader; not one that a buffer's size is made from), `Project::set_driven_value` (refuses a track that is not there and **a ring of tracks that read each other, of any size**, through the effects of clips, tracks and groups, switched on or off), `Command::SetDrivenValue` (one step of the history, merged for one number's drag), file format 24 (`driven` in an effect, tracks by their place in the file; a ring, an unknown track, a source that is not a track, an unknown channel and a missing number are refused), a deleted track lets go of what was read from it. The plan: **a mask is the plan of its track alone** (`Scope::OnlyTrack`: hidden or not, without the effects of groups), `PictureSection::masks`, sections cut where a mask changes, a video's one range kept across the cuts, masks of masks, a ring made by a moved clip cut where it closes; `operations_of_effects` makes the key of the shader that reads the masks (`isf::driven_variant_of`) and puts the number where the mask has nothing in the call; the playback asks for the pictures of mask videos (decoders are per track), the export finds their runs, and `describe_frame` gives the effect call its `masks`. **The whole chain is checked to the pixel on the software adapter** (`video_export/frames/tests.rs`: a hidden picture file's red, read by a shader in an exported frame).
4. **The window** (**done**, with a thumbnail that is not a picture of the track): the **mask button** next to the keyframe buttons of a number of a shader that has a range (greyed out with the reason when a buffer's size is made from it); while a number is read from a mask **its slider and keyframe buttons are not there and the thumbnail is in their place** (a bar of the channel with the name of the track and the channel), which opens the **window of the mask**: the track, the channel (red, green, blue, brightness), the slider that says what the number is where the track has nothing, *Use the slider again*. Every choice is a command. **What the thumbnail shows is not the track's pictures but what the mask does to the number** (a picture of the track at the playhead needs the pictures of the track outside the engine, or a read-back of the mask; it is for later).
5. **Later, each with its own tests** (**none is built**; the plan above is complete up to here): a track group, a contiguous clip group, a virtual layer as a source; a graph as a map.

## 6. What the maintainer decided (2026-11-01)

1. **What the value is where the source has nothing** (a gap in the track: the picture is clear there): **a value of its own, set by a slider made for this, in the settings window of the mask.** It is not the slider of the input (which is hidden and ignored while the input is driven) and it is not the low end of the range. It is carried to the shader in the input's own place among the numbers of the call, which nothing else uses while the input is driven, so the shader needs nothing more for it. Where the source is partly clear (a soft edge, a track that is half opaque), the value is a mixture of this value and the mask's, by how much the mask covers.
2. **Brightness is worked out from the codes** (the numbers the shaders read), with the weights of Rec. 709 (0.2126, 0.7152, 0.0722). The channels offered are red, green, blue and brightness.
3. **A track that is seen can be a source**, and a track can drive inputs on several clips.
4. **What the box of a driven input shows: the thumbnail of the mask**, in the place where the slider was. **Clicking the thumbnail opens the settings window of the mask**, which has the choice of the track, the choice of the channel, the slider for the value where the source has nothing, and (later) the graph of line segments that maps the mask's value to the value used.

## 7. What was not measured

Other adapters and drivers (Intel, Apple, Mesa); real camera footage's decode; many masks at once; the **real** engine drawing a mask (it does not exist yet: the numbers are the parts); the extra read of a graph's table; a mask drawn at half the size of the frame (a saving of a quarter of 4.1's draw and memory if it is ever needed; a mask is usually smooth, so it would probably look the same: **[B]**).

## 8. Found by use (2026-11-02): two things a mask did not do that the track does

The maintainer tested masks on a project in which a clip on a front track has an effect that reads the picture of a track behind it, that track is **seen** (its eye is open), the front clip does not cover the frame, and the clip on the track behind it has the *HorizVertHold* shader (a television's hold, which keeps where the picture has rolled to in a buffer that lasts) and then the *Doom Screen Transition* shader. Two things were wrong. Both were found by reading the engine and **reproduced by a test that failed** before the fix, and both are fixed.

1. **The picture of the track, where it was seen, did not go on in time.** [V] The effect of the clip is one effect, so the picture plan gives it **one state** (`EffectState`: an identity and the effect's own frame), and it is called twice in a frame: in the frame, and in the mask. The engine let **one call keep a state in a frame and made the second run "as a call that has none"** (`frame_renderer.rs`, the buffers that last), and a call that has none reads a cleared buffer. The masks are drawn **before** the layers that read them (container order), so the mask's call kept the state and the call that makes what is seen read nothing, in every frame: the hold rolled once and stayed there, while the mask went on as it should. The fix: **a second call of the same state at the same frame, whose buffers would be the same size, goes on from the same state before the frame as the first and draws its buffers that last into pictures of its own; it keeps, clears, restores and takes a checkpoint of nothing** (`BuffersOfACall::for_another_call_of_the_same_state`). It is the same effect done to the same picture again, so it makes the same picture (that is what "the same frame again" already meant), and the state goes on by one step. A second call whose buffers would be another size cannot do that and goes without, as every second call did. The test that said the second call "reads nothing, and is what is seen" (`persistent_buffer_tests`) said the very thing that froze the picture, and was changed on purpose. Tests: `engine::mask_tests` (the scenario in the engine, and the fall-back), `video_export::frames::tests` (a real project through the describer and the engine, four frames in order, with the old behaviour put back by hand: the picture where it is seen is 25 on frame 1 and not 51).
2. **A transition on a clip of the track was not done in the mask.** [V] A shader that reads **what is behind its layer** (a transition: `startImage`) was run only in the frame itself; in any group the plan counted it among the effects that are not run (`engine-phase-6-6-design.md`, T3), because a group's picture begins clear. A **mask is a group**, so the picture of the clip, which has the transition in the frame, had none in the mask, and the effect that reads the mask read the clip as it is before the transition. The fix: **in a mask a transition is done, over what is behind the layer in the mask, which is the mask's picture so far, and it begins as nothing**: the track on its own has nothing behind it. A new pass, `PassPlan::PlaceGroupOver`, puts the layers that come after it over the mask's picture as `PlaceLayersOver` does in the frame, and the step that makes the backdrop into codes reads the container's picture and not the frame's. **What a person sees follows from that, and is the decision to look at:** where the transition lets what is behind the clip show, the mask sees **nothing** there (the number is the one for where the track has nothing, mixed in by how much the mask covers), and not the track that is behind the clip in the frame. A transition in an ordinary group (not a mask) is still not run (T3: the group's picture is not opaque, and a person is told). Tests: `engine::mask_plan_tests` (the passes), `engine::mask_tests` (the arithmetic, in codes and in light: the number is 200 times the progress, where it was 200 whatever the progress), `video_export::frames::tests` (a real project; with the old rule put back by hand the number is 200 and not 100).

**Looked for and not found** [V]: the video pictures. A clip that is a layer and a mask is **one picture of one frame of its source** in the preview (a picture for each track) and in the export (taken once for each clip), and a test (`composition::pictures::mask_tests`) holds the source frame that the picture and the mask show to be the same in every piece that the host's section is cut into.

**What is left, and is a limit and not a mistake**: (a) a **hidden** track is made only where an effect reads it, so an effect of its clip that keeps a state goes on only while it is read (and starts clear when a reader begins, mid-clip); a track that is seen goes on everywhere it is seen, and the mask is the same picture then. (b) A mask is the track **without the effects of the groups** its clips are in (as decided in 3). (c) A transition in an ordinary group is not run.
