<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Phase 3: linear light and colour management (design)

Written 2026-10-16, after phase 2, **before the code** (the standing rule for the engine phases). It is the design of phase 3 of [the plan of action](plugin-system-plan-of-action.md): CM-01 to CM-05, CM-07 to CM-12, CM-14 to CM-16, CM-17 (complete), MD-11 and MG-01 of [the features list](plugin-system-features.md), with CM-20: **a separate change that is reviewed**, whose visible differences are listed with before and after pictures for the maintainer to look at.

Tags: **[V]** verified in the repository or by running something; **[D]** decided by the maintainer (date); **[P]** my proposal, which stands unless the maintainer objects; **[B]** my belief or estimate. Everything here is to be run on **software adapters only** (WARP, lavapipe: SF-01).

## 0. What phase 3 is, in one paragraph

Today every number in the engine is a *display-referred* one: the 8-bit code of a picture file, or what the YUV matrix makes of a video, divided by 255; scaling, rotating and laying layers over each other all average those codes, which is not what light does. Phase 3 makes the engine work in **linear light**: a source's numbers are turned into light by the curve that goes with the source (**input space**), the layers are put together in a **working space** (linear, with the primaries that the project chooses), and what is shown on the screen (**display transform**) or written to the file (**output space**) is made from the light by the curve that goes with *that*. Nothing that the program writes is a guess any more: the file gets tags that say what its numbers are. The maintainer asked for it as "important" (2026-10-13). **What a user sees:** edges of scaled and turned pictures, blends, text edges and blurs come out as light would make them; untagged HD video is read as BT.709 and not BT.601; exported files say what colours they are; there are three new settings and one new dialog; everything else (a picture that is only shown, a video that is only cut) comes out **the same numbers as before**.

## 1. What is there today (read in the code, 2026-10-16) [V]

* **The engine's numbers are codes.** `ingest.wgsl` premultiplies the 8-bit RGBA of a picture file, a text or a symbol and stores it as `Rgba16Float`; `yuv.wgsl` turns planes into R'G'B' with a matrix and a range and **no transfer function**; `colour_matrix.wgsl` (the hue shift) mixes the codes; `place.wgsl` samples them with a bilinear filter and lays them over what is there; `output.wgsl` clamps and writes 8 bits. Blending, filtering and scaling are therefore done **on gamma-encoded numbers**, as the old drawing (egui's textures and blending) does: the engine was built to be the same as that, and proved it (phase 1).
* **The export's planes are made from the 8-bit frame** the preview shows (`rgb_to_luma.wgsl`, `rgb_to_chroma.wgsl`), with the matrix and the range of the *first video* (`video_export/output_colour.rs`): a second rounding to 8 bits. An untagged first video makes an untagged file (BT.601, limited, no tags); a tagged one gives its tags back.
* **What a video says about itself is read**, all four tags (`VideoStreamFormat`: `colour_space`, `colour_range`, `colour_primaries`, `colour_transfer`), but **only the matrix and the range are used**, and only for `yuv420p` (`as_native_yuv420p`); **an untagged video is BT.601 limited**, because that is what ffmpeg does when it makes the RGBA itself (verified in phase 1: at every size, 4K included).
* **Every other kind of video is decoded to RGBA by ffmpeg** (10 bits to a number, 4:2:2, 4:4:4, `yuvj420p`'s full range, RGB), with ffmpeg's own matrix and no tags of ours; the colour differences of 4:2:0 are **copied to the four pixels they stand for**, as ffmpeg did (phase 1 decision E7: chroma replicated, not blended, because a blend was off by up to 82 levels from ffmpeg's pictures).
* **Settings** are a table (`project/settings.rs`: `Settings`, `ProjectSettings`, `SETTINGS`): each row has a global value and a per-project override, and the two settings windows and the two files are built from the table, with a test that goes through the table. **A source** (`SourceMedia`) has a link and its properties, and no colour field. **The project file is format 11**; a field that is not there reads as its default.
* **The window's own (old) drawing** is display-referred and stays so: it is the oracle of the engine's tests and what the preview falls back to when the engine stops.
* **The shown picture is 8-bit** and egui takes it as gamma-encoded (the design notes, section 1.8: `register_native_texture` needs `Rgba8Unorm`), so the engine ends with a pass of its own into an 8-bit texture: that pass is where the display transform goes.

## 2. Decisions

The numbers are the maintainer's identifiers where there are some (CM-xx); the C-numbers are new. **[D]** is what the maintainer has decided already (the features list and its decision log); everything else is mine and stands unless it is objected to.

| # | Decision | Why | Tag |
|---|---|---|---|
| C1 | **The working format is `Rgba16Float`, premultiplied, linear.** Values may go below 0 and above 1 inside the engine (a primaries conversion can); they are cut off only where a picture is made for the screen or the file, and where a display-referred effect asks for the encoded domain. | CM-01, CM-09, E4. Half floats have 0.05 % to 0.1 % of precision at every level from sRGB code 1 up [V, design notes 1.2]. | [D] CM-01 |
| C2 | **Colour spaces are data** (`colour_space/table.rs`): a name, three primaries, a white point. A conversion between two is `matrix, curve` made from the table (XYZ in the middle, Bradford adaptation when the whites differ), written once in Rust (the reference, tested against published numbers) and once in WGSL (tested against the Rust one), **with no matrix written by hand in a shader**: the numbers come from the table as uniforms. Rows to begin with: **Rec.709/sRGB, BT.601 525 (SMPTE C), BT.601 625 (EBU), Rec.2020, Display P3, ACEScg (AP1), ACES2065-1 (AP0)**. | CM-08, CM-15; Q-04 asked for 709, 2020 and AP0 as inputs and ACEScg as a working space; the other three are a row each. | [D] Q-04 + [P] the three rows |
| C3 | **Transfer functions** (curves, `colour_space/transfer.rs`): `Srgb` (the piecewise one), `Gamma22`, `Gamma24` (which is also BT.1886 with no black lift), `Gamma26`, `Rec709` (the camera curve BT.709 and BT.601 share: linear toe, 0.45 power), `Linear`. Later: PQ, HLG, camera logs (CM-18). | CM-07. | [P] |
| C4 | **The working space is a project setting**: *Rec.709 (linear)*, the default, and *ACEScg*. The setting is in the table, so it has a global default for new projects too. | CM-02, CM-14, Q-04. | [D] Q-04 |
| C5 | **A source has an input interpretation**: primaries, transfer, matrix and range, each from the file's tags and **overridable per source**. The override is a dialog opened from the source's menu (*Colour space of this file...*), four drop-downs, each with *Automatic* first and what *Automatic* is now written beside it. | CM-03, UI rules (no always-visible control). | [P] |
| C6 | **What a tag means** (the table in section 5): `bt709`, `smpte170m`, `bt470bg`, `bt2020-10`, `bt2020-12` transfer tags are read as **"video, shown as a computer screen shows it": the sRGB curve**. `iec61966-2-1` is the sRGB curve too. `linear` is linear; `gamma22`, `gamma28` are those powers. The camera curve (`Rec709`) and the 2.4 power are **offered in the dialog, and are never chosen from a tag**. HDR tags (`smpte2084`, `arib-std-b67`) are **not converted** in this phase: they are read as the sRGB curve with a note in the dialog that says so (CM-13 is later). | **A file tagged BT.709 is nearly always a recording of a screen or video that was made to be seen on a computer screen** (the maintainer's own: OBS), and every player shows it that way, so the numbers the picture has come out the same numbers on a computer screen, which is the property that matters most: *an unedited picture is unchanged by the whole pipeline* (C12). The camera curve, read as light, would lift every shadow. | [P], **the one for the maintainer to look at first** |
| C7 | **Untagged video:** matrix **BT.709 when the picture is more than 1024 across or more than 576 down, BT.601 otherwise** (the rule that players use), limited range, **Rec.709 primaries** (the SD primaries of 601 are a small difference that a file that does not say cannot be trusted with), the sRGB curve. **Untagged pictures** (files, text, symbol): sRGB. The assumption is said in the dialog of C5. | CM-03, Q-05 (decided 2026-10-13: *old projects are converted, no legacy flag*, so BT.709 for untagged HD). | [D] Q-05 |
| C8 | **The display transform is a setting, global with a per-project override** (the table does both): *sRGB* (the default), *Gamma 2.2*, *Gamma 2.4 (BT.1886)*. The display's primaries are Rec.709/sRGB's (a wide-gamut display shows them as they are, which is what an sRGB-assuming program does). The transform is: working space to Rec.709 primaries (one matrix), cut off at 0 and 1, encode. | CM-04, CM-14, Q-04. | [D] Q-04 + [P] the list |
| C9 | **The output space is a project setting** (the table: global default too): **1. *Rec.709, as the preview shows it*** (the default: the picture is encoded as the display transform encodes it, so the file's numbers are the preview's; tags `bt709` matrix, primaries and transfer, limited range), **2. *Rec.709, BT.1886 (gamma 2.4)*** (encoded with the 2.4 power; the same tags), **3. *sRGB*** (sRGB curve; transfer tag `iec61966-2-1`, matrix and primaries `bt709`). More rows later (Rec.2020, P3). | CM-05, Q-04 (export Rec.709). Row 1 keeps the numbers of every export that was made so far; the `bt709` transfer tag on sRGB-encoded numbers is what most players expect of screen video and treat as a computer screen (see C6). | [D] Q-04 + [P] the default |
| C10 | **Display-referred effects** (the hue shift, today's only effect) work in the **sRGB-encoded domain with Rec.709 primaries** whatever the working space is: the engine un-premultiplies, converts the light to that domain, runs the effect, converts back, premultiplies. An effect declares its domain (`ColourDomain::DisplayReferred` or `SceneLinear`); the engine converts around it, and **two neighbours of the same domain are not converted twice** (the colour matrix effects of a layer are fused already: E3). | CM-10, Q-07. | [D] Q-07 |
| C11 | **Colour parameters** are in the working space and are interpolated in linear light (MD-11). **No colour parameter exists today** (the text's colour is a text-object property, a code, and is read as an sRGB code), so this is for the effects of phase 4. | MD-11. | [D] |
| C12 | **The pipeline is the identity for what is not changed.** With the default settings, a picture or a video that is shown 1:1 and opaque comes out with the same 8-bit numbers it had (decode with the sRGB curve, encode with the sRGB curve: tested for all 256 codes), and so does a layer that is only moved without being filtered. **What changes is only what mixes pixels:** scaling, turning, edges, opacity, blends, antialiased text, blurs. | The gate's "written list of every visible difference" is then short and true. | [P] |
| C13 | **The export's planes are made from the float picture, not from the 8-bit one**: an *encode* pass writes the finished picture in the output encoding into a half-float texture, and the two plane passes read that. With the default output space the encode pass is **the same function** as the preview's output pass, so the export's RGB is the preview's, to within the rounding of the 8-bit step (a test says it: encoded, rounded, equal to the preview's bytes). No second rounding to 8 bits before the matrix. | CM-09. EN-03's "bit-identical" becomes "the same arithmetic, the same numbers up to the last rounding": the preview shows the display transform and the file has the output space, which may differ by choice. | [P] |
| C14 | **The tags of a file are the output space's, not the first video's.** The file says its matrix, primaries, transfer and range always (today an untagged first video gives an untagged file). This is the change that the plan calls "correct tags on export". A file that is not `limited` range is not made in this phase. | CM-05. | [D] |
| C15 | **The old display-referred behaviour stays inside the engine as `WorkingSpace::DisplayReferred`**: no input conversion, no output conversion, as today. It is **not a setting and has no way to be asked for in the program**: it is what the oracle tests of the engine (the engine against the old drawing, which is display-referred) and the tests of the window run in, so that they pass **unmodified** (the standing rule); and it is how the pictures of the differences are made (the same scene in the two modes). | CM-20, design notes 1.7. | [P] |
| C16 | **Old projects are converted** (format 11 and before): they load with the new colour handling and a *default* for each new field. **The format becomes 12**, so that an older program says that the file is from a newer one, instead of showing it without the new settings. (This takes the number that the plan had given to text generators, which become 13.) | CM-12, MG-01, MG-03 (a version and a fixture for every change of the saved form). | [P] |
| C17 | **Native video formats** (the compromises of phase 1 that exist only to keep today's pictures are paid back): the engine is given **`yuv420p`, `yuv422p`, `yuv444p` at 8 bits and at 10 bits, and `yuvj420p`'s full range**; the colour differences of 4:2:0 and 4:2:2 are **reconstructed smoothly** (bilinear, at the siting the file says: `left` is the default, `center` and `topleft` are read) and not copied to the four pixels. Every other format is still decoded to RGBA by ffmpeg, which is told the matrix and range that C6 and C7 chose (so that the picture is *not* ffmpeg's guess). | The plan's list for phase 3. | [D] plan |
| C18 | **What is not done:** HDR (PQ, HLG) conversion, camera logs, EXR, a view transform (ACES look), OpenColorIO, 10-bit export, a full-range export, dithering, a wide-gamut display, and the scaling that ffmpeg does *before* the engine sees a picture (see section 9). Each has a row in the features list that keeps it possible. | CM-13, CM-18, CM-19, Q-18, Q-35, Q-36. | [D] |

## 3. The colour model

### 3.1 Colour spaces and conversions (`src/colour_space/`)

A **colour space** is three primaries (CIE xy), a white point, and a **transfer function**: the table of C2. Three kinds of thing are made from it, all with **f64 in Rust** and given to the shaders as **f32 uniforms**:

* `rgb_to_xyz(space)`: the 3x3 matrix made from the primaries and the white in the usual way (the primaries' XYZ scaled so that their sum is the white).
* `adapt(from_white, to_white)`: **Bradford**, the identity when the whites are equal (all of the table is D65 but the two ACES spaces).
* `convert(from, to)`: `xyz_to_rgb(to) * adapt * rgb_to_xyz(from)`: **a 3x3 matrix between the linear light of two spaces**.

The transfer functions are `encode(light) -> code` and `decode(code) -> light` in f64, exactly the published formulas (section 3.2), and the same two written in WGSL as `fn colour_encode(curve: u32, light: vec3<f32>)` and `fn colour_decode(...)`, **chosen by a number in a uniform**, so that one shader serves every curve (no variant of a shader for each, and the number of pipelines does not grow).

### 3.2 The curves

| curve | decode (code `v` to light `L`) | encode |
|---|---|---|
| sRGB | `v <= 0.04045 ? v/12.92 : ((v+0.055)/1.055)^2.4` | `L <= 0.0031308 ? 12.92 L : 1.055 L^(1/2.4) - 0.055` |
| Rec.709 camera | `v < 0.081242858298 ? v/4.5 : ((v + 0.0992968268)/1.0992968268)^(1/0.45)` | `L < 0.018053968510807 ? 4.5 L : 1.0992968268 L^0.45 - 0.0992968268` |
| gamma g (2.2, 2.4, 2.6) | `v^g` | `L^(1/g)` |
| linear | `v` | `L` |

(The Rec.709 camera curve is written with the precise constants that BT.2020 gives (`alpha = 1.09929682680944`, `beta = 0.018053968510807`) and not BT.709's rounded `1.099`, `0.099`, `0.018`, whose two pieces do not meet: they are 0.00025 apart at the knee; the precise ones make the curve continuous, so that it has an inverse. The two agree to a quarter of a level.) All of them are applied to values cut off at 0 and 1 on the code side, and to `max(L, 0)` on the light side, so that a negative light (out of the gamut of the display) is the code 0 and never a `NaN`.

### 3.3 Published numbers the tests hold the code to (CM-16)

* sRGB: decode(0.5) = 0.21404114; encode(0.5) = 0.73535698 (**"50 % white over black is 0.5 in linear light, 0.735 in sRGB code"**: the gate's own sentence); decode(1) = 1, encode(0) = 0; the knee at 0.04045 and 0.0031308 is continuous.
* Rec.709 camera curve: encode(0.5) = 0.705436 (computed; the rounded constants of BT.709 give 0.705515, which is why the precise ones are used); the two halves meet at 0.018053968510807 (0.081242858298).
* **Rec.709/sRGB to XYZ (D65) is the matrix of IEC 61966-2-1**: `[0.4124 0.3576 0.1805; 0.2126 0.7152 0.0722; 0.0193 0.1192 0.9505]`, to 5e-5 (it is published with four decimals; the table derives `0.41239 0.35758 0.18048; 0.21264 0.71517 0.07219; 0.01933 0.11919 0.95053`).
* **Rec.709 to Rec.2020 is the matrix of ITU-R BT.2087**: `[0.6274 0.3293 0.0433; 0.0691 0.9195 0.0114; 0.0164 0.0880 0.8956]`, to 5e-5 (derived: `0.62740 0.32928 0.04331; 0.06910 0.91954 0.01136; 0.01639 0.08801 0.89560`).
* `convert(Rec.709, ACEScg)` is the matrix of the ACES documentation, `[0.6131324224 0.3395380158 0.0474166960; 0.0701243808 0.9163940113 0.0134515240; 0.0205876575 0.1095745716 0.8697854040]`, **to 1e-4**, and `convert(Rec.709, ACES2065-1)` is `[0.4397010 0.3829780 0.1773350; 0.0897923 0.8134390 0.0967616; 0.0175440 0.1115440 0.8707040]`, **to 3e-4**. **These two are not derived exactly the same way** (the documents round the primaries and the white, so their third row of AP0 does not add to 1 by 2e-4; derived by the code: `0.6131 0.3395 0.0474 / 0.0702 0.9164 0.0135 / 0.0206 0.1096 0.8698` and `0.4396 0.3830 0.1774 / 0.0898 0.8134 0.0968 / 0.0175 0.1115 0.8709`). The design notes' own check (1.2) found the same 0.00005 and 0.0002 against Python. [V, `scratchpad/colour_check.py`, 2026-10-16]
* Untagged video: `(Y, Cb, Cr) = (100, 150, 200)` limited range is **(213, 31, 142) with BT.601 and (227, 55, 144) with BT.709** [V, the same script; it is also in the design notes, 1.8].
* The luma weights of the YUV matrices are `Kr, Kb` of the standards (the code has them already).
* **Round trips:** every pair of the table, in both directions, on a grid of colours; every curve, `decode(encode(L)) = L` and the other way, to 1e-9 in f64 and to a level in the shader.
* **The 8-bit round trip of C12:** the sRGB code `c` of each of the 256 values, through ingest to a half float, and out through the output pass, is `c` again.

## 4. How a frame is made

`FrameDescription` gets a `colour` field (`FrameColour`): the working space (`WorkingSpace::DisplayReferred` or `Linear(primaries)`) and the **output** (what the last pass writes: primaries, curve, and whether the result is for the screen as 8 bits or for the file as a half float). It is in the fingerprint (the cache key), so a change of a setting is a different frame.

1. **Ingest** (once, when a picture is sent). `PicturePixels::Rgba8` gets a `colour` (primaries and curve of its numbers; sRGB for a file, a text, a symbol). The shader: `decode(curve)` the colour of the straight-alpha pixel, multiply by the matrix `convert(picture primaries, working primaries)`, **then** multiply by the alpha (premultiply in light), write half floats. A picture is never premultiplied in codes any more.
2. **Planes** (once, when a video picture is sent). `PicturePixels::Yuv` (C17) has the layout (420, 422, 444), the depth (8 or 10 bits in 16-bit planes), the chroma siting and a `colour` with the matrix, the range, the primaries and the curve. The shader: reconstruct the colour differences (smoothly), apply the matrix and the range, cut off at 0 and 1 (as today), `decode(curve)`, `convert(primaries, working)`.
3. **Effects** (per frame, per layer, before placement: E2). `engine:colour_matrix` is **display-referred** (C10): un-premultiply, `convert(working, Rec.709)`, `encode(sRGB)`, mix with the matrix and cut off at 0 and 1 (as today), `decode(sRGB)`, `convert(Rec.709, working)`, premultiply. The uniform holds the two conversion matrices and the number of the curve; in `DisplayReferred` mode they are the identity and the curve is `Linear` (so the shader is the same, and the mode costs a branch-free identity). Effects of strangers (the guard's and the watchdog's) get **their domain from the key's declaration** (`ColourDomain`; the only declaration today is the built-in's): a foreign effect is `DisplayReferred`, **which is what ISF shaders are written for** (CM-10), so none of them changes its look.
4. **Placement and composition** (`place.wgsl`): **no change**. It samples a premultiplied half-float texture with a bilinear sampler and lays it over what is there; the numbers are light now, so the filter and the blend are right.
5. **Output for the screen** (`output.wgsl`): un-premultiply is not needed (the accumulation is over opaque black: its alpha is 1); `convert(working, display primaries)`, cut off at 0 and 1, `encode(display curve)`, 8 bits. In `DisplayReferred` mode: cut off and write, as today.
6. **Output for the file** (`encode.wgsl`, new, export only): the same arithmetic with the **output space's** primaries and curve, written as `Rgba16Float` (C13), which `rgb_to_luma` and `rgb_to_chroma` then read (they already average the colour differences of four pixels; what they read is an encoded float picture now and not an 8-bit one, and they round to 8 bits once, at the end).

**What the cache and the pool see:** nothing new but a larger fingerprint and one more pool shape (the half-float encode texture of the export).

## 5. Sources: how a file's tags become an interpretation

`InputColour { primaries, curve, matrix, range, siting }` is made by one function, `interpret(&VideoStreamFormat, width, height, override) -> InputColour`, which the supplier calls, so the **window, the export and the command line cannot differ**.

| ffprobe says | we read |
|---|---|
| `color_space` `bt709` | matrix BT.709 |
| `bt470bg`, `smpte170m` | matrix BT.601 |
| `bt2020nc` | matrix BT.2020 (non-constant luminance); `bt2020c` (constant luminance) and the others (`smpte240m`, `fcc`, `ycgco`, `rgb`/`gbr`) are **not native**: the video is decoded to RGBA by ffmpeg, which is told the closest matrix that it has |
| nothing | by the size (C7) |
| `color_range` `tv`/`mpeg`, `pc`/`jpeg`, nothing | limited, full, limited (`yuvj420p` is full whatever it says) |
| `color_primaries` `bt709`; `smpte170m`, `bt470bg`; `bt2020`; `smpte432`; nothing | Rec.709; the two BT.601 rows; Rec.2020; Display P3; Rec.709 |
| `color_transfer` `bt709`, `smpte170m`, `bt470bg`, `bt2020-10`, `bt2020-12`, `iec61966-2-1`, nothing | **sRGB curve** (C6) |
| `linear`, `gamma22`, `gamma28` | linear, 2.2 power, 2.8 power (the last is not in the table: read as 2.4) |
| `smpte2084`, `arib-std-b67` | sRGB curve **with the note** (C6, C18) |
| `chroma_location` `left`, `center`, `topleft`, nothing | those three, `left` for nothing (what H.264 says when it says nothing) |

The result of each line is **shown in the dialog**, with the file's own words beside them (`bt709`, `unspecified`), so that "the assumption is said in the interface" (CM-03). A picture file (PNG, JPEG) and a text have no tags: sRGB, Rec.709 primaries.

## 6. Settings, the dialog, the file

* **Three settings** are three rows of the table (`project/settings.rs`), each with a type for its values: `DisplayTransform { Srgb, Gamma22, Gamma24 }`, `WorkingSpace { LinearRec709, AcesCg }`, `OutputSpace { Rec709AsPreviewed, Rec709Bt1886, Srgb }`. Both settings windows and both files get them for free, as the table's own doc comment promises. **The settings are in a project's own file only if the project set them** (the table's rule), which is also why an old project with none of them loads as a project that follows the global settings.
* **The dialog of C5** is a window like the other dialogs, opened from the source's context menu (`Colour space of this file...`); *OK* is one step of the history. Each of the four fields has *Automatic*, and the override is stored on the source (`colour: InputColourOverride`, every field optional). Nothing is always visible that was not before.
* **The file becomes format 12**: `sources[].colour` (omitted when there is no override) and the three settings (omitted when the project follows the global ones). **A format 11 file is read as it is** (the fields are not there), and **a fixture of format 11 is kept in the tests** (MG-03) with the check that it loads as converted: the default colour handling, and nothing else changed.
* **The window and the export read the same settings** (`settings.resolve`): the display transform is used by the preview only; the output space by the export only; the working space by both.

## 7. Pieces and their gates

Each is built behind tests that lock what was there, is verified on Windows (WARP) and Linux (lavapipe), and is a commit that CI has read before the next begins where it can be. The numbers are the plan's sizes.

| piece | what | gate |
|---|---|---|
| **3.1** | `colour_space`: the table, the curves, the conversions, in Rust | the published numbers of section 3.3; every round trip; no engine change |
| **3.2** | WGSL for them, and `WorkingSpace`/`FrameColour` in the engine with **`DisplayReferred` as the default**: all of today's engine tests pass **unmodified**; new tests run the passes in linear mode against the Rust reference | WGSL equals Rust to a level on a grid; the 256-code round trip of C12; "50 % over black is 0.735"; a scaled checkerboard is brighter in linear light by the number the arithmetic says |
| **3.3** | ingest, planes and the colour matrix effect with their colour; `PictureData` carries it; `describe_frame` passes it | the oracle tests (engine against the old drawing) pass **in `DisplayReferred`** and, in linear mode, differ **only** at the places that mix pixels, by bounds that are computed and not guessed |
| **3.4** | the preview in linear light: the display transform, the global setting, `interpret`, the window's use | a window test that an unedited picture has the same bytes; the setting changes the shown bytes as the arithmetic says |
| **3.5** | the export in linear light: `encode.wgsl`, the output space, the tags (C14), the project setting | the export's RGB equals the preview's rounded (C13); the planes agree with `reference_yuv420p` of the encoded picture to a level; the tags of each output space; the real-ffmpeg journeys pass with their edits listed |
| **3.6** | the per-source override: the model, the dialog, undo, the file (format 12 and the fixture of 11) | round trip of the file; a fixture of format 11 loads converted; the dialog's tests |
| **3.7** | native formats and smooth chroma (C17) | a video of each format is exported and compared against the arithmetic; the chroma of a red-on-blue edge is smooth where it was blocky |
| **3.8** | the list of visible differences, with the pictures (the same scenes in the two modes); the mutation campaign; the documents; the report | **the maintainer looks at the list and the pictures and says whether each difference is acceptable** (CM-20): the phase is not called done before |

## 8. The tests

* **Reference first.** The Rust functions are the specification; the WGSL ones are tested against them on a software adapter, over a grid of 4096 colours and each curve, with a bound in levels that is **measured**, then written down.
* **Published numbers** are the ones of section 3.3, with the source of each in the test.
* **The oracle stays.** The old drawing is display-referred; the engine's oracle tests run in `DisplayReferred` (C15) and so pass as written. **The tests that fail on purpose** are the ones that assert today's *numbers* at the places that mix pixels (a rotated layer's edge, a half-opaque layer over another) in the default mode of the window and the export; each is listed in the report with the new number and where it comes from. *(I expect few: the engine's own tests are written against the oracle, and the window's tests use the old drawing.)*
* **Identity tests.** A picture shown 1:1 is unchanged; a video of each native format shown 1:1 is what the arithmetic makes of it; **an export of an unedited video, with the default settings, has the same frames as before to within a level** (the old numbers are in the journeys' expected frames).
* **The mutation campaign** over the new modules, with the survivors closed.

## 9. What this does not change, and what is not exact

* **ffmpeg scales a video before the engine sees it** (to the size the layer needs: `scale=...,format=...`), **in the encoded domain**; the engine's own filtering of what it is given is in light. A video that is shown much smaller than it is gets its edges averaged by ffmpeg in codes. Moving that scaling into the engine means sending full-size pictures (4K pictures are 12 MB each in planes at 4:2:0): **not done, and said in the report** [B that it is the right trade for now].
* **8 bits at the screen** are 8 bits: no dithering, so a smooth dark gradient can show steps that a dithered output would not (it was so before).
* **A display that is not sRGB** is the maintainer's to set (C8); the program does not read the operating system's profile.

## 10. The visible differences (what I expect; the measured list replaces this in 3.8)

1. **Scaled and turned pictures:** edges and anything with fine contrast are brighter where light is averaged (a white line on black that is made thinner than a pixel is brighter, not darker).
2. **Opacity and blends:** a 50 % layer over another is the average of the *light*, which is lighter than the average of the codes for two different colours (50 % white over black: 0.735 and not 0.5, in codes).
3. **Text edges:** the antialiased edge of text is the coverage of the light, so light text on a dark picture looks bolder and dark on light thinner than before.
4. **Untagged HD video** is read with BT.709, not BT.601: reds and greens shift a little (the check in the design notes: (100, 150, 200) is (213, 31, 142) with BT.601 and (227, 55, 144) with BT.709).
5. **The colour differences of 4:2:0 are reconstructed smoothly** (3.7): a sharp coloured edge (red on blue) is smooth where it was blocky.
6. **The exported file says what it is** (tags), where before it said nothing if its video said nothing; the numbers in it are the same.
7. **Nothing else**, by C12: a picture that is only shown, a clip that is only cut, a hue shift on a flat colour.

## 11. For the maintainer

Decisions that I took and that you may want to look at before they are built, most important first:

1. **C6: a file tagged BT.709 is read as "shown like a computer screen" (the sRGB curve), not as the camera curve.** The effect of the other choice: every shadow in every video is lighter than the player shows it. It is in the dialog, so a file from a real camera can be set to the camera curve by hand.
2. **C9: the default output space is "as the preview shows it", tagged `bt709`**, so that the file is what the preview is. The other two are in the list.
3. **C16: the file format becomes 12**, so an older program refuses it.
4. **C8: three display transforms**, no profile reading.

*(This section is where the answers go.)*

## 12. Where the building is (written as the pieces are finished; the report replaces it)

| piece | state | commit | what is different from the design above |
|---|---|---|---|
| 3.1 `colour_space` | done, verified | `545f86d` and before | none |
| 3.2 WGSL and `WorkingSpace`/`FrameColour` | done, verified on WARP and lavapipe | `545f86d` | `DisplayReferred` is the default **inside the engine**; the window and the export ask for linear light |
| 3.3 ingest, planes, colour matrix with their colour | done | `545f86d` | the hue shift (display-referred) is run in the sRGB-encoded domain between two uniform matrices, as C10 says; **effects of strangers run on working-space numbers** for now: converting around them (C10's `ColourDomain`) waits for phase 4, where the first foreign effect that cares exists |
| 3.4 what a file's tags mean | done, verified | `417ecb0` | the function is `VideoStreamFormat::colour_of_a_picture_of_size_with`; the matrix enum moved to `colour_space` (`VideoMatrix`, was `NativeMatrix` in `media`) |
| 3.5 the export in linear light | done, verified | `417ecb0` | **C13 is done as written** (`ENCODED_FORMAT`, the same shader with a half-float target, no second pipeline of arithmetic); **C14 is done**: the planes are BT.709 limited whatever the first video was, the file says `bt709` matrix, `bt709` primaries, the output space's transfer, `tv`; `ExportPlan::first_video` is gone because nothing asks it any more |
| 3.6 the file's own colours | done, verified | (this commit) | the dialog is a **window whose choices apply at once**, each choice an edit joined to the one before it on the same file (as the settings windows apply at once), not a window with an *OK* button; it is opened from the clip's menu (*Colour space of this file...*); a **picture file** has the primaries and the curve (no matrix, no range); a sound has none; the file format is **12** and a fixture of a version-11 file is kept |
| 3.7 native formats and smooth chroma | **in part**: ffmpeg is told the matrix and the range (done); smooth chroma for 4:2:0 (done); native 4:2:2, 4:4:4 and 10-bit planes **not done** | this commit | **ffmpeg is told** what the program reads a file as (`scale=...:in_color_matrix=...:in_range=...`, from one function that the preview and the export both ask: `playback::reading_of_a_video`) for every form that it makes colours of itself, so an untagged high-definition 4:4:4, 4:2:2 or 10-bit video is BT.709 like the 4:2:0 one (measured: the guess it replaces is 5.8 levels away on average, the told reading is exact). **Smooth chroma** is in linear light only (the compatibility mode copies, as the oracle tests need), at the siting that the file says (`chroma_location`: left, centre, top left; left when it says nothing). **Not done, on purpose:** planes of 4:2:2, 4:4:4 and 10 bits sent to the card as they are: with the matrix told, ffmpeg's own conversion is right in the colours, and what native planes would add is the way the colour differences are blended, for a kind of file that a screen recording never is; it is a large change (the stream, the upload and the shader for each layout) for a small gain, and it waits for you to say whether it is wanted. |
| 3.8 the list of differences, the campaign, the report | not begun | | |

Two limits that the dialog says in words: the matrix and the range **do not apply to a video that is not stored as 8-bit 4:2:0** (ffmpeg makes its colours with its own matrix) until 3.7, and the **old drawing** (the window's fallback when the engine stops) ignores all colour settings, as it always did.
