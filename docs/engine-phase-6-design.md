<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Phase 6: the ISF host (design)

Written 2026-10-21, before building, from [the plan of action](plugin-system-plan-of-action.md) (phase 6), the requirements PH-10 to PH-17, EN-05, EN-06, EN-14, ID-10, SF-01 to SF-03 of [the features list](plugin-system-features.md), and
[the spike of phase 0.5](plugin-system-spike-findings.md) (`tools/spikes/phase-0.5/src/isf.rs`, which is the first draft of the rewriting step). Tags: **[V]** verified in the repository or by running something; **[D]** decided by the
maintainer; **[P]** my proposal, which stands unless it is objected to; **[B]** my belief. Everything runs on **software adapters only** (SF-01).

## 0. Why this phase is the one that matters most

On 2026-10-21 the maintainer decided that **fastforwardrs is to be slim and to do much with non-built-ins**, and took the one built-in generator (Solid colour) out again so that generators come back as ISF shaders. The ISF host is
therefore not one more feature: it is **how the program gets generators, transitions and most new effects without growing**. This changes what I build first: the host has to be small, strict and boring, and everything
that can be a shader file stays one.

## 1. What is there already [V, read in the code, 2026-10-21]

* **The engine runs shaders that others wrote**: `Engine::compile_effect_shader(key, wgsl)` reads and guards a WGSL module (loop limits, checked entry point), sends it to the engine's thread; a frame asks for it by an `EffectCall`
  with the key and numbers (`engine/effect_shaders.rs`, `guard.rs`, `handle.rs`). Its form: fragment entry `fs_main`; `@binding(0)` a uniform of the call's numbers (64 `f32`), `@binding(1)` the picture of the layer
  (premultiplied half float), `@binding(2)` a filtering sampler. It runs in the **display-referred domain** ("what shader files are written for"; `KindOfEffect::Shader`). It is **tested, but nothing in the program uses it yet.**
* **The watchdog and device recreation exist** (phase 1.6): the effects of a frame that did not finish are switched off for the run, three recreations in a minute stop the engine.
* **Effect kinds are descriptors** (`EffectDescriptor`: key, version, domain, parameters with kind of value, range, unit, control, category, unknown fields); a project file keeps a **snapshot of the descriptor of every kind it uses**, and
  a kind the program does not have is kept and does nothing (`EffectKind::from_snapshot`, `is_installed`).
* **The spike** wrote the rewriting step and ran eight ISF files of my own through naga to pictures on WARP and lavapipe, agreeing within 1 of 255.

## 2. Decisions

| # | Decision | Why | Tag |
|---|---|---|---|
| J1 | **A new module `src/isf/`, with no graphics in it**: the reader of the file (`reader`), the descriptor made from the header (`descriptor`), the rewriting step (`rewrite`) and the compile to WGSL (`compile`, naga's GLSL front end). It is pure and runs on a worker thread that **survives a panic** (`catch_unwind`; a panic is a refusal with a message). | Testable without a card; a bad shader cannot take the editor down (EN-05, PH-11). | [P] |
| J2 | **The header is read leniently** (a small JSON reader that takes a trailing comma, comments, duplicate keys (the last wins) and an integer `ISFVSN`), and the body keeps its line count (the header comment becomes empty lines), so that a compiler line is a file line. A refusal says the line and column. Only the **first** syntax error is told (naga's front end stops there). | PH-10, PH-11; spike S1 [V]. | [P] |
| J3 | **An ISF effect is a kind of effect, and its source is part of the kind.** `EffectDescriptor` gets one optional field, `shader: Option<ShaderSource>` (language `isf`, the text, up to 256 KiB); the key is `isf:<name>-<8 hex of the hash of the source>`, so that two different files of one name are two kinds and the same file twice is one. A project's snapshot of the kind **carries the source**, so a project opened where the shader was never installed still runs it. **Data has no prompt** (SF-04). The file format is **16**. | One way in, no folder to scan, nothing to install; the plugins' path (a folder, a trust question) is for native code and is not needed for text. Slim. | [P] |
| J4 | **Where a shader comes from**: *Add effect from file...* in the **+ menu** (the one that takes `.cube` already) takes an `.fs`/`.isf` file as well, and the project keeps it. **No folder is scanned and nothing ships in the box** until you say so (PH-16 says gl-transitions and the MIT Vidvox shaders may ship; that is a download I will not make without your word). | The rule about unrequested UI: nothing new on the screen. | [P] |
| J5 | **The parameters** become the descriptor's values: `float` → number (range from `MIN`/`MAX`, default `DEFAULT`), `bool` → flag (a switch), `long` → a choice (`VALUES`/`LABELS`), `color` → colour, `point2D` → pair (**in points of a picture 1080 high**, the way the project keeps a blur's radius, and multiplied by the picture's height over 1080 in the shader, so that a point is in the same place at any size of the picture; ISF says a point is in pixels, which I believe [B, 80 %] and which the golden corpus will check), `event` → a flag that is off (a button is not meaningful on a timeline; said in the properties box), `image` → an input slot, not a value. `LABEL` is the control's label; `NAME` is the id. | PH-10. | [P] |
| J6 | **The call's numbers** are the values in the order of the descriptor, each as its numbers (a number 1, a pair 2, a colour 4 not premultiplied, a flag 0 or 1, a choice its value). The shader reads them from **one `vec4` array**; the rewriting step `#define`s each ISF name as a swizzle of it (`#define gain _isf_u[1].x`), so that no member offsets are computed by hand (the spike's rule is kept: take them from naga where there are any). The **engine appends four numbers after the 64 of a call: the width and the height of the picture the effect works on and two zeros** (`RENDERSIZE`); `TIME`, `TIMEDELTA`, `FRAMEINDEX` and `PASSINDEX` are the first four numbers of **every** ISF call, put there by the plan (`DATE` is zeros): they are *clip-local* (PH-12), never the wall clock (for an effect of a track, a tag or a group, since the first frame of the timeline). The effect is **display-referred** like every shader. | The engine already writes numbers one after another; this changes nothing for the shaders of phase 1. | [P] |
| J7 | **Orientation: the engine is top-row-first and the host flips at its boundary** (decided in phase 1, spike S1 [V]). **Sampling is `textureLod(…, 0.0)`**, never `texture` (shaders sample inside branches). | Spike. | [D] (phase 1) |
| J8 | **Pieces 6.1 to 6.3 are single-pass filters** (what most ISF files are); **6.4 is multi-pass and persistent buffers** (the engine learns a pass that has several textures in and out, EN-06, EN-14); **6.5 generators come back** as clips whose content is such an effect (H8 of phase 5, now by an ISF shader: a clip kind `Generator` that holds one effect instance of a kind that takes no image); **6.6 transitions** (a backdrop input, EF-03, and how two clips are laid to overlap: a question for you before I build it). | Each piece can be seen and tested alone. | [P] |
| J9 | **The guard stays as phase 1 built it** (loop limits, own device and thread, watchdog and recreation); **PH-15, the golden corpus**, is 10 to 20 shaders compared by you with the ISF Editor on Windows: I cannot make it alone, and I will say when the shaders it needs are ready to be compared. | PH-15, Q-24. | [D] |

## 3. Pieces and their gates

| piece | what | gate |
|---|---|---|
| **6.1** *(built 2026-10-22)* | `src/isf/`: the reader, the lenient JSON, the descriptor from a header, the shape (filter, generator, transition) | the eight spike files and a set of old and odd headers read as they should; every refusal says where; no graphics |
| **6.2** *(built 2026-10-22)* | the rewriting step and the compile to WGSL, on a worker that survives a panic | the eight spike files compile and the WGSL passes the guard; a file with a mistake gives its file line and column; a panic in the compile is a refusal; `sampler2D` as a function parameter is refused with words |
| **6.3** *(built 2026-10-22)* | **the filter runs**: `EffectDescriptor.shader`, `EffectKind` from a file, the plan, the engine, the + menu, file format 16, preview and export | a real export of a project with an ISF filter (colour by colour, as phase 4's); the preview equals the export; the project reopens where the shader is not installed; `TIME` is clip-local; every earlier test passes unmodified |
| **6.4** | multi-pass shaders, persistent buffers, imported images, the feedback modes | the spike's two-pass blur; a feedback shader behaves as EN-14 says on seek |
| **6.5** | generators as ISF | a generator over a video; extending it; a file round trip |
| **6.6** | transitions | after your answer about the timeline |
| **6.7** | the shaders in the box and the golden corpus | after your word about the download and your comparison |

## 4. What this does not change

* The built-in effects stay as they are: they have the exactness work of phases 3 and 4 (one fused matrix, linear light) that a shader file would not.
  **My opinion [B, moderate]:** a built-in that is only a shader (a vignette, say) never gets added; one that needs the engine (a colour change in light) stays.
* **Sound** is untouched.

## 5. For the maintainer

Nothing blocks 6.1 and 6.2: they have no effect on the program but a new module. Before 6.3 I would like your word on **J3** (the shader's source inside the kind and the project's snapshot; a file format of 16) and **J4** (no folder, no shipped shaders yet);
my proposals stand if you say nothing. Before 6.6 I need your answer about how two clips overlap for a transition.
