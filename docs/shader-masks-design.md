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
* **What it costs.** A mask that is a track costs **about as much as one more video layer** each frame, and **nothing but a texture read** if it does not change (a picture, a generator that does not move). The read of the mask in the shader costs **nothing you can measure beside the shader's own work** on the two adapters tried (section 4.2).

## 3. How it works, and why it stays simple

```
                       per driven input of an effect
   source (a track)  ──►  channel (R G B A brightness)  ──►  map (a line / later a graph)  ──►  the value the shader sees
        │                                                        applied once per pixel, before main
        └─ rendered off the frame, as a picture of its own (the size of the frame), once for all the inputs that use it
```

**One idea carries all the sources.** The engine already has `LayerSource::Group`: *layers put together in a picture of their own, the size of the frame, clear where nothing is*. A track is a group of its clips' layers; a track group is a group of tracks; a contiguous clip group is a group of its clips; **a virtual layer is a group of one generator layer**. So a mask source is *a group that is drawn off the frame*, and what is new is only a way for an effect to name one. Nothing else about the engine has to know which kind of source it is. (**[B]**: I read this in `engine/scene.rs` and have not built it; it is the part of the work most likely to hold a surprise: see phase 2.)

| Piece | What it is | Where it lives |
|---|---|---|
| a driven input | `{ input, source, channel, map }`, kept with the effect's values; the slider's value is kept and unused | the project (file format 24) |
| `source` | `Track(TrackId)` now; `TrackGroup`, `ClipGroup`, `VirtualLayer` later | an enum in the project; one function in `composition` turns it into a group description for a frame |
| `channel` | red, green, blue, alpha, brightness | an enum; the shader text says which |
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

* **One more live mask ≈ decode + send + draw**: about **1.8 ms at 1080p and 6.9 ms at 4K on the AMD graphics** (the decode is on the processor and overlaps the card's work, so it costs less than the sum). For scale, the program's own benchmark makes the whole stack (four layers, two blurs, a matrix, a lookup, new video in every frame) in **9.5 ms at 720p, 22.8 ms at 1080p and 97 ms at 4K** on the AMD graphics, and in **2.5 ms at 1080p and 9.3 ms at 4K** on the RTX 3090. The preview draws at 720p at most (`playback::preview_picture_size`), so the preview pays less than the 1080p column; the export pays the full size but does not have to keep up with a clock.
* **Sending the planes is 2 to 3 times cheaper than sending RGBA**: the program's choice to send what the decoder gives and to convert on the card is right, and a mask should do the same.
* **The decode of real footage** takes longer than the test clip's (a camera's 10 to 100 Mbit/s against 2.6 and 7.4 here): **[B]** not measured.
* **A mask that does not change** (a picture, a generator that does not move, a still) is drawn once and kept: its cost per frame is the read in the shader, below. **[B]** the program does not yet keep such a texture between frames; it is a part of phase 2.

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

## 5. A plan, in pieces that can each be checked

0. **Done: the eye** (`f383d15`): a track can be hidden; the plan leaves it out; file format 23.
1. **The rewriter**: a driven input, read once per pixel, in `rewrite.rs`, with tests: **every float input of the 218 shaders of the collection** compiles driven (the corpus test that exists for the shaders does this for the plain ones); the size-expression check; the order (the value is set before the globals' initialisers). No window, no engine.
2. **The engine** (the risky piece): off-frame groups in `FrameDescription`, an effect call that names one, the order of drawing, the cycle check, and keeping a mask that does not change. Checked with the engine's own benchmark, which gets a scene with a mask, against the numbers in section 4.
3. **The project and the file** (format 24): the driven inputs, the effects' snapshots, undo, the `Command`s.
4. **The window**: the button beside the keyframe buttons, a chooser of the track (and the channel), the slider hidden while an input is driven, the box of the effect saying what drives it.
5. **Later, each with its own tests**: a track group, a contiguous clip group, a virtual layer as a source; a graph as a map.

## 6. What I would like the maintainer to decide

1. **What a value outside the layer is.** Where the source has no clip (a gap) the picture is clear black and clear: should its value be the **low end** of the input's range (my suggestion: brightness 0, so the low end), or should the input fall back to the slider's value there? (The second means the slider is not entirely ignored.)
2. **Brightness**: worked out from the colours as they are on the screen (the codes, as the shaders see them; my suggestion) or from light.
3. **A track that is seen is allowed as a source** (my suggestion: yes, nothing forbids it; the eye is only for when it should not be in the picture), and a track can drive inputs on several clips.
4. **What the box of a driven input shows**: only that it is driven, and by what? (There is room for the channel, and later for the graph.)

## 7. What was not measured

Other adapters and drivers (Intel, Apple, Mesa); real camera footage's decode; many masks at once; the **real** engine drawing a mask (it does not exist yet: the numbers are the parts); the extra read of a graph's table; a mask drawn at half the size of the frame (a saving of a quarter of 4.1's draw and memory if it is ever needed; a mask is usually smooth, so it would probably look the same: **[B]**).
