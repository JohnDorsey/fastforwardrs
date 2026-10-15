# Phase 0.5: what the three spikes found

Written 2026-10-15, after running them. The code is in [`tools/spikes/phase-0.5/`](../tools/spikes/phase-0.5/README.md) (throwaway, kept so that every number below can be
repeated). This note says **Verified** for what was run and measured, and **Believed** for what is reasoned or read but was not run. Plan: [phase 0.5 of the plan of action](plugin-system-plan-of-action.md).

## How the spikes were run (and what that does and does not prove)

* **Only software adapters were opened**, never a real graphics card (SF-01): on Windows the *Microsoft Basic Render Driver* (WARP, Direct3D 12, driver 10.0.19041.5794, the one
  that comes with Windows 10); on Linux, in WSL2 Ubuntu 26.04, *llvmpipe* (lavapipe, Vulkan, Mesa 26.0.8, LLVM 21.1.8), with `VK_DRIVER_FILES` set so that the driver for the real
  card in WSL is never even loaded. The code panics if no adapter of device type `Cpu` exists. The one exception is the two-triangle program `race_repro`, which was also run on the four
  real adapters of this PC (Direct3D 12 and Vulkan, the RTX 3090 and the integrated AMD) because it is no heavier than what the editor itself does each frame.
* wgpu 30.0.1 and naga 30.0.1, the versions the program has. The machine is a Ryzen 5 7600X (6 cores, 12 threads); timings of software drawing are for orientation only.
* **The sample shaders are mine.** I wrote eight ISF files in the style of the Vidvox examples from the ISF specification (a colour filter, a vignette, a posterise with a `long`
  input, a kaleidoscope, a generator, a two-pass blur, a transition, and one with old idioms). That is *not* a third-party corpus: what the spike shows is that the mechanism works and
  what the rewriting step has to handle, **not** a compile rate on real files. The golden corpus with pictures from the ISF Editor (PH-15, Q-24) is still to be made.

## S1: an ISF shader through a rewriting step and naga to a picture

**Result: it works.** All eight shaders were read, rewritten, compiled by naga's GLSL front end, written as WGSL, drawn by wgpu on both software adapters, and checked.

**Verified, the values (not only the looks).**
* A pass-through filter (`IMG_THIS_NORM_PIXEL`) gives the input back exactly (largest difference 0 of 255).
* `color_controls.fs` (uniforms: floats, a bool, a colour) against a CPU calculation of the same arithmetic: defaults 0 of 255, two other settings at most **1 of 255** (the GPU rounds, the CPU
  reference rounds in its own way).
* The pictures made by WARP/Direct3D 12 and by lavapipe/Vulkan agree to **within 1 of 255 on all eight shader pictures, with no pixel more than 1 off**.
* Orientation (ISF is OpenGL: `isf_FragNormCoord` and `gl_FragCoord` have y pointing up): checked with a shader that writes the coordinates as colours.
* A two-pass shader (`PASSES` with a `TARGET` buffer and `PASSINDEX`) gives a blur; the buffer is read by the second pass (this is where the WARP bug below was found).

**What the rewriting step has to do (all found by running into it).** This list is the specification for the ISF host of phase 5:
1. Read the header (`/*{ ... }*/`), keep the body, **blank the comment out line by line** so that compiler line numbers are file line numbers, and **subtract the lines of the generated
   prelude** from every reported position (verified for syntax errors and for validation errors: line 12 of the file is reported as line 12).
2. Put every non-image input in **one uniform block** and take the member offsets **from naga** (`module.types[..].members[..].offset`), never compute them by hand. A `bool` or `event`
   cannot be a uniform in Vulkan-style GLSL (naga says the global is invalid): make it an `int` and `#define name (_isf_b_name != 0)`.
3. Images: naga's GLSL front end **does not implement a combined `sampler2D` global** ("variable qualifier": not implemented). Declare a separate `texture2D name_tex` and `sampler name_samp`
   per image, and sample with `texture(sampler2D(name_tex, name_samp), uv)`.
4. naga's preprocessor **has no token pasting (`##`)**, so ISF's `IMG_PIXEL`, `IMG_NORM_PIXEL`, `IMG_THIS_PIXEL`, `IMG_THIS_NORM_PIXEL`, `IMG_SIZE` (and `texture2D`, `texture2DRect`) cannot be
   macros. The spike rewrites each call `IMG_NORM_PIXEL(name, ...)` into a call of a generated function `_isf_IMG_NORM_PIXEL_name(...)` (a small scanner, 40 lines). **Limit:** an image passed
   to a function of the shader's own (`vec4 f(sampler2D img)`) is refused ("Expected RightParen, found sampler2D"); that is rare but real.
5. Not implemented by naga and handled by rewriting: `varying` (turned into a plain global, so it reads zero), `#version` (removed). `void main()` is renamed and a generated `main` sets
   `isf_FragNormCoord = gl_FragCoord.xy / RENDERSIZE` (and the `vv_FragNormCoord` alias) first, and copies `gl_FragColor` (a macro for a global) to the output last.
6. **Sampling with `textureLod(…, 0.0)` and not `texture(…)`** is a correctness choice, not something naga forced: naga and wgpu **accept** a plain `texture()` inside a branch on a
   non-uniform value (verified: it compiled and drew on both adapters), but its gradients are undefined on real graphics hardware at the edge of such a branch (**Believed**, from the WGSL
   rules; not testable here). Nothing in the ISF files uses mipmaps, so `textureLod` at 0 means the same.
7. **Pictures inside the engine are stored in GL orientation (row 0 is the bottom row)**: flip once when a decoded frame is uploaded (free in the upload) and once in the output pass, and
   **nothing in any shader needs rewriting** (verified by the checks above). This is the decision for phase 1.

**Idioms (Verified that they compile, which is not the same as being right).** Of 39 small idioms, 33 compile unchanged, among them the ones that worried me: implicit `int` to
`float` (`TIME * 2`, `pow(x, 2)`, `float f = 1;`, `mod(x, 2)`, `vec3(1)`, `clamp(x, 0, 1)`), `const` arrays with the old and the new initialiser style, a global with a non-constant
initialiser, `switch`, `discard`, `mat2`, `atan(y, x)`, loops with float counters, `#define` constants and function macros, `#if`, `out` parameters, `struct`, `texelFetch`,
`textureLod`, `fwidth`, and `precision highp float;`. The 6 cases that are refused: a combined `sampler2D` global, `##` in a macro (two cases: the author's own macros cannot use it either), `varying`, a `bool` in the uniform block, and
recursion (not legal GLSL); and the error probe found one more: `sampler2D` as a function parameter. Only the eight shaders were checked *numerically*; the implicit conversions were not.

**Errors.** naga's GLSL parser reports **only the first** syntax error (the spike's missing semicolon on line 11 is reported at the start of line 12, column 2, which is where the parser noticed).
Messages are located but not always helpful ("Unknown function 'mix'" for `mix` with two arguments). Validation errors (type mismatches) carry spans, but the text is only "Function is
invalid" until the **cause chain** is followed (`error.source()`): "The type of [18] doesn't match the type stored in [19]". Errors inside the generated epilogue show up as lines past the end of the file.
**The header:** serde_json takes duplicate keys (the last wins) and an integer `ISFVSN`, but **refuses a trailing comma**, so PH-10 needs a more tolerant JSON reader (an
own small one, or a JSON5-style pre-pass).

**Cost (Verified, debug build).** Reading + rewriting + naga + WGSL: 2 to 5 ms per shader. The first draw, which includes making the pipeline, took 30 to 160 ms on the software adapters.

### The wgpu / WARP finding that changes the engine design

**Verified:** on WARP (DX12), **two draws that use different, statically bound uniform buffers both see the data of the last one**, unless the submission is waited for between them.
It does not matter whether the draws are in one command buffer, in separate submissions, in release builds, whether the buffers are filled with `write_buffer` or at creation, or whether
the buffers are kept alive. **It is specific to uniform buffers bound through a descriptor: textures and samplers in bind groups are right, a dynamic-offset uniform buffer is right** (wgpu
binds those as root descriptors, `wgpu-hal/src/dx12/command.rs:245`), and **the real adapters** (DX12 and Vulkan on the RTX 3090 and the AMD; one round of the three basic variants) **and lavapipe are all right**.
The short program `race_repro.rs` shows it with no ISF code. I did not find the root cause (WARP or wgpu-hal); it is **Believed** to be a WARP behaviour that wgpu does not allow for.
Consequences:
* **Rule for the engine (phase 1): the uniforms of all passes of a frame live in one arena buffer and are bound with dynamic offsets.** This is also the efficient design. The first
  multi-pass engine test must run on WARP, because that is the only place where a mistake shows.
* This matters for CI: GitHub's Windows runners have no GPU, so wgpu tests there run on WARP (a newer WARP than this PC's; **Believed**, not checked).
* wgpu's **automatic pipeline layout leaves out bindings the shader does not use**: bind groups must be made from an **explicit layout** that the descriptor determines.

**Not spiked (still open for phase 5):** `PERSISTENT` buffers, `IMPORTED` images, `WIDTH`/`HEIGHT` expressions of passes (the spike gives every buffer the render size), `audio` inputs,
`texture2DRect` exactness against a real host, and whatever 87 real files will turn up.

## S2: a shader that does not finish

**Verified on both software adapters** (a 64×64 draw whose shader loops, in a child process that is killed by its id at a hard limit):

| | WARP (Direct3D 12) | lavapipe (Vulkan) |
|---|---|---|
| a draw of 10^5 loop passes | done in 0.18 s | done in 1.2 s |
| 10^7 passes | done in **18.2 s** (no timeout of the operating system, no device loss) | still running after **40 s** |
| 10^8 passes, and a real `loop {}` with a break that never happens | still running | still running |
| `poll(Wait { timeout })` | **returns `Timeout` at the time asked** (5.018 s for 5 s) | the same (5.002 s) |
| device-lost callback within 6 s more | **never** | **never** |
| `device.destroy()` | returns at once (0.024 s) | returns at once (0.020 s) |
| a **fresh instance and device** afterwards | draws a trivial picture in 0.36 s | in 0.06 s |
| CPU used **after** the old device was destroyed and a new one made | **about 8 cores' worth, going on** until the process ends | **1 core, going on** |

Meaning:
1. **A watchdog is possible**: wgpu's poll with a timeout comes back, so the engine can give a draw a budget.
2. **Abandoning the device is possible and cheap**, and a new one works at once.
3. **But the runaway work cannot be cancelled on a software adapter**: it goes on using the CPU until the process exits (all the cores on WARP). On these adapters "disable the instance and
   recreate the device" stops the *effect on the picture* but not the *cost*. Only a process boundary can cancel it.
4. wgpu's own loop bounding (`force_loop_bounding`, on by default) did **not** save the infinite loop: its bound is about 2^64 passes (**Believed**, from the documentation; what was
   Verified is that the loop did not end in the 25 to 40 s watched).
5. **Not tested, by the rule of SF-01: a real graphics card.** On Windows the operating system's timeout (TDR, normally 2 s) resets the card and the program gets a lost device
   (**Believed**). Whether other programs on the card are disturbed is **Believed** to depend on the Windows version and **unknown here**. What *is* Verified by reading the source: **neither
   egui-wgpu 0.36.2 nor eframe 0.36.2 has any handling of a lost device** (no occurrence of `DeviceLost`, `set_device_lost_callback` or `on_uncaptured_error`), so if plugin passes ran on
   the *window's* device, a lost device would take the window down. This is the reason the plan has the engine on its own device and thread (phase 1).

**A guard that works: loop caps in the rewriting step (Verified).** `guard.rs` walks the naga IR and gives every loop a counter of its own that breaks the loop after N passes (about 60 lines, works for
GLSL- and WGSL-made modules alike, because it works on the IR). Results with N = 20 000, at 256×192:
* `while (true)` with no real exit: drawn in **0.5 s** (WARP) and 0.44 s (lavapipe), where it never ended without the cap.
* a `for` of two billion passes: 0.41 s and 0.55 s.
* the blur, kaleidoscope and plasma shaders give **byte-identical pictures** with and without the cap (both adapters).
* The counter counts **all the passes a loop makes in one shader invocation**, also over several entries into it, so nested loops of 1000×1000 = 10^6 passes need a cap of at least 10^6:
  with 20 000 the picture changed (as designed, it is *truncated*) and with 2 000 000 the draw took 22.2 s on WARP. **A cap bounds the work; it does not make the work cheap.** The honest worst case
  is about `cap × loops × pixels × cost of a pass`, and the engine can compute a cap from a frame budget.

### What I recommend for EN-05 (my opinion; confidence: medium, because the real card is untested)

1. **Loop caps in the rewriting step, always on**, default chosen so that the worst case on the *slowest supported adapter* stays under a few seconds at preview size (for 20 000: about 0.5 s on WARP at 256×192;
   scales with the pixel count), raised per effect only by the maintainer's setting. Truncation is reported to the user as "this effect was cut short" when it happens (the engine can count it with the
   same counter, a later refinement).
2. **The engine's own device and thread** (already planned), never the window's device.
3. **A watchdog around each frame** with the poll timeout; on expiry: mark the instance (and the effect type, for this run) as *stuck*, say which, abandon the device and make a new one. Accept that on a
   software adapter the abandoned work keeps using the CPU; say so in the message.
4. **A first-use probe**: the first time an instance of a plugin shader is drawn it is drawn small (e.g. 32×32) under the watchdog, and only then at full size. It costs one tiny draw and turns "hangs the
   preview" into "the effect is refused with a message" in the common case of a shader that is simply slow.
5. **The process option stays open, and is not built yet.** Phase 1 keeps the engine's interface message-shaped (EN-12, Q-31), which is what makes a render process possible later. It is the only thing that
   *cancels* runaway work on a software adapter and the only protection I can argue for against the real-card unknown. I would build it when a user reports a freeze that 1 to 4 did not stop, not before.

## S3: a `.cube` table in a shader against ffmpeg's `lut3d`

**Result: they agree to the last bit of ffmpeg's rounding, and the plan's test is possible with one correction.** Method: three tables of 17³, 33³ and 17³-with-a-domain points (a deliberately bumpy
table, so that interpolation matters), a 256×256 picture (ramps and 64 000 random colours), the same lookup done **by a hand-written WGSL shader** (tetrahedral and trilinear, the table in a 3-D texture
read with `textureLoad`), **by a CPU reference in f64**, and **by ffmpeg 9.0.2** (`lut3d=…:interp=tetrahedral` and `trilinear`). All Verified, on both adapters (the numbers are the same to the last digit
except the 32-bit comparison, 1.7e-7 against 3.2e-7).

| comparison (17³ and 33³) | largest difference | notes |
|---|---|---|
| shader (32-bit float target) vs CPU f64 | **1.7e-7 to 3.5e-7** (0.0001 of an 8-bit step) | the shader *is* the algorithm |
| shader (8-bit target) vs CPU rounded | 1 of 255, mean 0.0000 | |
| **ffmpeg vs CPU rounded** | 1 of 255, **mean −0.49** | ffmpeg's numbers are about half a level lower |
| **ffmpeg vs CPU truncated** | 1 of 255, mean 0.0002 | **ffmpeg 9.0.2 truncates instead of rounding** |
| shader vs ffmpeg | 1 of 255, mean +0.49, none over 1 | the whole difference is that |
| the sampler's own trilinear on a 16-bit float table vs the CPU trilinear | 1 of 255, mean 0.01 to 0.02 | a **fast path is possible**; the table in 16-bit loses nothing visible |
| tetrahedral vs trilinear, on the same table | up to **3 of 255**, 2864 values over 1 (17³) | the choice of interpolation is visible on a bumpy 17-point table, and small on 33 points |

* **The gate of phase 4** ("equals ffmpeg's `lut3d=interp=tetrahedral`") should read: *within 1 of 255 per value, with ffmpeg's truncation known and a mean bias of 0.49 when we round*. If it is
  to be exact, compare against a **floor**-quantised reference. Both are now known to hold.
* **ffmpeg ignores `DOMAIN_MIN`/`DOMAIN_MAX` for the table tried** (`0.05..0.95`, `0..1`, `0.1..0.9`): ffmpeg's output matches the CPU lookup *with the domain taken as 0 to 1* to within 1 of 255, and differs from the
  domain-correct result by up to **41 of 255** (mean 7.6). **Verified for that table; the reason (that ffmpeg does not subtract the minimum) is Believed** from the pattern, I did not read ffmpeg's source.
  So our shader follows the specification, and **a parity test against ffmpeg must use tables with the domain 0 to 1**. Because phase 2 deletes the ffmpeg picture path, there is no preview/export
  disagreement to fear: both go through the engine. (This also supports the answer to Q-37: no `lut3d` shortcut.)
* Not tested: 1-D tables, the Resolve variant (shaper plus 3-D), sizes of 65 and above, speed (software adapters say nothing), and colour spaces (CM-11).

## What this changes in the plan, in one list

1. **Phase 1:** uniforms in one arena with dynamic offsets (and a multi-pass test on WARP); explicit bind-group layouts; pictures in GL orientation inside the engine, flipped at upload and at output.
2. **Phase 5 (ISF host):** the rewriting list above is its specification; a lenient JSON reader; loop caps; refuse `sampler2D` function parameters with a clear message; first error only.
3. **EN-05:** the five-part guard above; the render *process* is an option kept open by the message-shaped interface, not a phase.
4. **Phase 4 gate for `.cube`:** within 1 of 255 against ffmpeg (which truncates), domain 0 to 1 for the parity table; a 16-bit-float sampler fast path is available.
5. **CI:** Windows CI runs wgpu on WARP and Linux CI on lavapipe (the Ubuntu job installs `mesa-vulkan-drivers`); both are now known to draw the same pictures within 1 of 255.

## Two questions for the maintainer

1. Do you accept the guard of EN-05 as recommended (caps, own device, watchdog with recreation, first-use probe) with the render process **kept open but not built** until a freeze shows it is needed?
2. Should **sound (LADSPA) move earlier** in the plan, after phase 4 (still open from before)?
