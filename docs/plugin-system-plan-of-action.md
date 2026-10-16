# Plugin system: plan of action

Status: **draft 1 for the maintainer's review**, written 2026-10-15, after the last open question of `plugin-system-features.md` was answered. It says **in what
order the work is done, what each phase delivers to a user, and what has to be true before the next phase starts**. It replaces section 9 of
`long-term-plan-plugins-and-gpu-engine.md` (that section is kept for history). What the system must do is in the features list (identifiers such as `ID-10` or `SF-06` point there);
why the design is what it is, is in the long-term plan and in `plugin-system-design-notes.md`.

Tags: **[V]** verified by me in the repository or on this PC on 2026-10-15; **[D]** decided by the maintainer (date); **[P]** my proposal, which stands unless the maintainer
objects; **[B]** my belief or estimate. Sizes are [B]: S, M, L, XL are orders of magnitude for one person who knows this code, **not a calendar**.

---

## 1. Where the work starts

* **Round 12 is committed** (`222b3cc`) [V]. Groups (tag, clip group, track group) are built; their **effects are a placeholder** (applied to each member's picture, the same
  picture only where members are opaque: features list, section 13) until the engine can composite a group.
* **The model is still closed** [V]: `PropertyId` is used 1380 times in 96 files and `EffectKind` 343 times in 42 files (the long-term plan counted 1250 and 262 in 79 and 25 on
  2026-10-12; the work of rounds 11 and 12 added to both). There is one effect (the hue shift), at most one of each kind per clip or track *(until 0.4 step B, 2026-10-15: a chain may have several of one kind now)*.
* **The project file was format 10** [V] (`FILE_FORMAT_VERSION`; groups took it; **0.4 step B made it 11**). The features list called the plugin model "v10" and text generators "v11": **the plugin model is
  therefore format 11 and text generators format 12.**
* **The mutation tooling is in the repository** [V] (`tools/mutation_testing/`, round 12), the `.ffrs` archive reads media in place (round 10), and the whole library suite runs in about
  26 s.
* **The repository has no `LICENSE` file, no `license` in `Cargo.toml`, and no CI** [V] (`.github` is absent). The remote is `github.com/JohnDorsey/fastforwardrs`, public [D, 2026-10-15].
* **Linux:** the Ubuntu app is installed (`CanonicalGroupLimited.Ubuntu` is in the list of Windows apps) **WSL first reported "no installed distributions"** [V]; the maintainer then started Ubuntu, and `wsl -l -v` shows `Ubuntu` (26.04.1 LTS, WSL 2, 8 processors, 15 GB) [V, 2026-10-15]. It has `git` and `curl` and **no Rust toolchain or C compiler yet** (phase 0.1 installs them).
* **Decisions that bind this plan** (all in the decision log of the features list): GPLv3-or-later with the plugin clarification; Windows first, Linux tested, macOS builds in CI;
  `.cube` first, then ISF; LADSPA before CLAP; linear light is important; groups first (done); a node editor and image-input parameters both stay possible; embedded native plugins
  allowed behind a severe prompt; preview of feedback effects is a global setting and never changes the export.

## 2. Rules for every phase

1. **Behaviour is locked by tests before it is changed** (the standing rule): the existing tests stay and pass unmodified wherever behaviour is meant to stay; a test is rewritten
   only on purpose, and the report says which.
2. **One visible change at a time, and each one reviewed.** The engine is first built to look exactly like today (CM-20); linear light is a separate step with its differences listed.
3. **The old path stays as the oracle** until parity is shown (EN-02), and is deleted in the phase that says so, with its tests.
4. **Each phase ends with** the full suite, `clippy -D warnings`, `rustdoc -D warnings`, a mutation campaign over the code it added (`tools/mutation_testing/`), the README and the
   features list brought up to date, and a report in the usual form (done / decisions / tests rewritten on purpose / limits / mistakes / verified versus believed). I do not commit.
5. **No always-visible control is added** unless asked for (`UI-01`, `UI-11`, `UI-12` are menus, settings and prompts only).
6. **Shaders and plugins are never tested on the maintainer's GPU or session** (SF-01): software adapters, a machine set aside, or WSL.
7. **Any phase after phase 2 can be cancelled** without wasting the earlier ones: each ends with something usable.

## 3. The phases

Dependencies at a glance: `0 -> 1 -> 2 -> 3 -> 4 -> 5`, `4 -> 6`, `0 -> 7` (sound runs beside the picture phases), `7 and 6 -> 8`, `6 or 7 -> 9`. **Order of work since 2026-10-16: 3, 4, 7, 5, 6, 8, 9** (the maintainer moved LADSPA earlier).

### Phase 0 — Foundations (size M + M + S + S)

Five pieces that need no GPU and no new user-visible behaviour. They can be done in any order; 0.4 gates phase 1 and 0.1 to 0.3 gate nothing but should come first because they are cheap
and every later phase uses them.

* **0.1 Linux and CI** [P]. The maintainer starts Ubuntu once (section 1). Then I build and run the suite in WSL and fix what is Windows-only (spike S5), and add GitHub Actions for
  Windows, Ubuntu and macOS: build, `clippy`, `fmt`, and the headless tests on software adapters (WARP, lavapipe); macOS only builds and runs the headless tests (PL-03). The workflow file
  is created in the tree and **the maintainer commits and pushes it**. *Gate:* the whole suite passes in WSL and CI is green on the three systems (macOS may be "builds" only,
  as decided in Q-19). *Size:* M (what breaks on Linux is unknown until it is tried). *Delivers:* a second system that keeps everything honest.
  **Progress 2026-10-15 [V]:** Rust 1.99, a C toolchain and the libraries are installed in Ubuntu; the project builds there; `tools/linux/run_in_wsl.sh` runs any command on a copy of the project in
  the Linux file system; fmt, clippy `-D warnings` and rustdoc `-D warnings` are clean on Linux; the library suite passes (3464) after **one fix**: `snip.rs`'s test compared the sound bit for bit, and `sin`/`exp`
  differ in the last bit between Windows and Linux (at most 4e-8, in about 1 sample in 100), so it now compares the sound to a thousandth (a change of 0.02 % in one pitch or 0.3 % in a time is still noticed).
  **Integration tests: with Ubuntu's ffmpeg 8.0.1, 2 of 189 fail; with ffmpeg 9 (the BtbN GPL build n9.0.2, in `~/ffmpeg9` in Ubuntu, downloaded 2026-10-15 with the maintainer's
  permission), all 189 pass and so do the 3464 library tests** [V]. The cause is ffmpeg 8 moving the pictures of a file whose sound starts before its picture by one frame after `-ss 0`
  (README, *ffmpeg*). **Decided 2026-10-15: ffmpeg 9 or newer is required**: an older ffmpeg is not used and the error says why (`OLDEST_FFMPEG_MAJOR_VERSION_THAT_WORKS`); the code path for
  ffmpeg older than 7 (`-filter_complex_script`) was dead and is removed, with its tests. `.github/workflows/ci.yml` is written and **not yet run**; `tools/linux/` has the helper scripts.
  *Remaining for 0.1:* the first CI run on GitHub (the maintainer commits and pushes the workflow), and what it shows.
* **0.2 Licence mechanics** (LG-06, LG-07) [P]. `LICENSE` (GPLv3), `license = "GPL-3.0-or-later"` in `Cargo.toml`, an SPDX line in each source file (checked by `reuse lint`),
  `THIRD_PARTY` notices (`cargo-about` or `cargo-deny`), the ffmpeg source offer, the manifest of shipped content (LG-02), the Ubuntu Font Licence check, and **the plugin
  clarification text (LG-04, Q-34), which only the copyright holder can approve: I draft it, you decide the words**. *Gate:* `reuse lint` and `cargo deny` clean in CI. *Size:* S.
  **0.2 DONE 2026-10-15 [V]:** `LICENSE`, `LICENSES/`, `LICENSE-PLUGIN-CLARIFICATION` (variant A, adopted), `Cargo.toml` (`GPL-3.0-or-later`), an SPDX line with `2026 John Dorsey` in each of the 363
  Rust and Python files, `REUSE.toml` for the rest (`reuse lint`: compliant, REUSE 3.3), `deny.toml` (`cargo deny check`: licences, bans, sources and advisories pass; one reviewed exception, the unmaintained
  `ttf-parser`, RUSTSEC-2026-0192, reached through `ab_glyph` and `cosmic-text`), `about.toml`/`about.hbs` and `THIRD_PARTY.md`, `docs/distribution-notices.md` (manifest, ffmpeg source offer rules; both ffmpeg
  builds checked to be `--enable-gpl --enable-version3` and not nonfree), and a `licences` job in the CI workflow. Not done: the ffmpeg source offer itself (needs a release to exist).
* **0.3 Split `FastForwardApplication`'s state** (agreed 2026-10-12) [D]. The application struct is still large (`fast_forward_application/mod.rs` is 702 lines [V]); it is split into
  sub-structs so that the engine, the plugin registry and the prompts have places to live that are not "yet another field". Mechanical; behind the existing tests, which must not
  change. *Gate:* all tests green and unmodified. *Size:* M.
  **0.3 DONE 2026-10-15 [V]:** the 38 fields of `FastForwardApplication` are in six groups in `fast_forward_application/state.rs`: `document` (project, file path, history, unsaved flags,
  archive cache), `playback` (clock, preview pictures, sound), `sources` (what is known of the source files, openings), `jobs` (export, background save, tidying), `interface` (selection,
  views, dialog, closing) and `environment` (ffmpeg, file chooser, settings file, fonts, wake-up); three methods that touch one group moved into it (`sources.is_available`,
  `sources.available_locations`, `playback.reading`). About 760 field paths were rewritten by a script and checked by the compiler (the paths in about 100 test lines changed; no test was
  rewritten otherwise); all 3465 library tests and the 189 integration tests pass on Windows, and on Linux with ffmpeg 9. The groups are where the engine (`playback`), the plugin registry
  (`environment`/`document`) and the prompts (`interface`) will go.
* **0.4 Open up effect identity** (the refactor of the long-term plan, section 3) [D agreed 2026-10-12]. Descriptors in the project, effect instances with ids (several of one type allowed),
  parameter values by string id, `PropertyId::EffectParam { instance, slot }` so that the key stays `Copy + Ord`, the hue shift as `builtin:hue_shift`, **format 11** (reads 10 and older,
  keeps unknown fields: MG-03), the missing-effect behaviour (MD-07) with a badge, and copying/undo/racks working as today. The effects of groups use the same model (the placeholder of
  section 13 is unchanged). **First a spike (QA-07, two days, throwaway)** confirms the slot scheme against the real code; if it is awkward the fallback is `Arc<str>` ids behind a type alias
  (more touching, nothing structural). *Gate:* every existing behaviour test passes unmodified except those that assert "at most one effect of each kind" and the file-format tests; a
  format-10 file round-trips to format 11 and back to the same project; a test file of each format version is in the tests (MG-03). *Size:* M-L (about 100 files, almost all mechanical).
  *Delivers to a user:* nothing visible, except that two hue shifts on one clip are now allowed.
  **0.4 is done in three steps, each verified before the next (decided 2026-10-15 after the spike, which found the slot scheme sound: about 45 production sites and
  70 test lines used the old hue shift property, and nothing else needed a deep change):**
  **A. Effects are named (DONE 2026-10-15 [V]; behaviour unchanged, file format still 10).** An effect in a chain has an `EffectInstanceId` (the first is `FIRST`, each next is one more
  than the largest in that chain: a name is unique in one chain, not in the project: keyframes are kept for a clip or a track, so two chains may use the same). The old property `PropertyId::HueShift`
  is `PropertyId::EffectValue { effect: EffectKind, instance, slot }` (`Copy + Ord`; the kind of effect is in the key so that what kind of value it is, what it is called and where it can be are known
  without looking in the chain), `PropertyId::ALL` became `PropertyId::OWN` (the nine a clip or a track has itself), chain items are named by instance (`ChainItemKey::Effect(EffectInstanceId)`), an
  `EffectKind`'s table says the kind and the file name of each of its values (`value_kinds`, `value_names`) instead of listing properties, and the file code finds the value of an effect by its name in the
  chain of what the keyframes are of (and writes it by the name the effect says). At most one effect of each kind per chain is still the rule, which is why the file did not need to change.
  **B. Several effects of one kind, and file format 11 (DONE 2026-10-15 [V]).** A chain may have as many effects of one kind as are put on: `Effects::add` and `put_on` give back the
  name the effect has (`EffectInstanceId`: the first is 0, each next one more than the largest in the chain), `remove` and the commands `RemoveEffect` / `RemoveGroupEffect` and the editing
  methods work **by name**, and `AlreadyHasThatEffect` / `TheGroupHasThatEffectAlready` are gone. **File format 11**: an effect has `instance` (the number it has in its list) and a rack of the
  keyframes of the value of an effect has `effect` (that number, in the list of what the keyframes are of); format 10 is read as before (effects are named in the order they are in, a rack is of
  the one effect that has a value of that name). **What the person sees:** the menu of the plus button offers every effect the thing can have, also those it has; the box of an effect, and its
  racks and keyframe buttons, say **which of its kind it is only when there are several** ("Hue shift 2", counting from 1 in the order the effects are done in: so the numbers follow the order
  when a box is dragged, and a lone effect has none). **Tests changed on purpose:** the ones that asserted "at most one" (put on twice is refused, the menu does not offer what it has, the plus
  button is dimmed when the clip has all, a file with two of a kind is refused, `TheGroupHasThatEffectAlready`) and the expectations of the saved file (version 11, `instance`). The suite has
  28 more tests than after step A (3662 → 3690 on Windows; 3689 on Linux with ffmpeg 9, where the library has one test fewer): chains, editing, groups, names, panel, drag, files, files of version 10, the README example,
  and real-ffmpeg exports of two hue shifts and of a second one with keyframes. **Mutation set `_b`**: 57 planted
  mistakes (27 of step A's that still apply, 30 new), 52 noticed after two real gaps were fixed (the name of a rack in another drawer; deleting the *second* effect through the panel); **5 remain
  and are equivalent only while there is one kind of effect with one value** ("a value of another kind of effect is a value of this one", "a value has the kind of the first place", "the values at
  a frame are looked for in the place 0", "the control of a value is the control of the first place", "the place among its kind counts what is not of its kind"): **they must be killed by
  the first test that has a second kind of effect or an effect with two values (step C or phase 4); rerun `mutations_round13_b.py` then.**
  **C. Open effect types (DONE 2026-10-15 [V]; so 0.4 is done).** Done in three verified pieces. **C1, the model:** `EffectKind` is no longer an enum: it is a small `Copy + Eq + Hash + Ord` value that
  is a reference to an `EffectDescriptor` (`project/effects/descriptor.rs`: key, version, name, description, domain (picture or sound), whether it changes each pixel by itself, and its values, each a
  `ParameterDescriptor` with an id, a name, a label, what it begins as, and the range, unit and decimals of its control), made by **interning** (`EffectKind::from_snapshot`: the descriptor is kept for the rest
  of the run, so the same descriptor is the same kind, and a kind can stay a part of `PropertyId::EffectValue` with names and kinds of values that need no project or list at hand: the step-A design). The
  program's own effects are descriptors too (`project/effects/built_in.rs`, with the one place that knows what each does: colour matrix and export rounding), `EffectKind::BUILT_IN` lists them, `HUE_SHIFT` is
  `builtin:hue_shift` 1.0 with the value `degrees`. **A snapshot is the program's effect when it has the same key and major number, the same domain and no value that the program's has not** (ID-04, as
  far as it goes); any other is a kind of its own that is *not installed*. **C2, the file (format 11, final):** `plugins` has a snapshot of each descriptor in use; an effect says its key, version, instance
  and values **by id**; racks of effect values say the id and the instance; unknown fields of an effect, a snapshot and a value are kept and written back (`UnknownFields`; the misspelt-field test became
  "kept" on purpose); old shapes are read through the frozen table `project_file/legacy_effects.rs` (version 10, and version 11 as step B wrote it: effects by name, values in a list, no `plugins`). New code
  is in `project_file/effects.rs`. **C3, missing effects:** an effect the program does not have is kept with all values and keyframes, shown in the properties panel with the name its file gave it, a note
  (*Not installed: it does nothing here, and its values are kept.*) and controls from its snapshot (a number has a slider, the other kinds are said in words until the effect surface of phase 4), does nothing
  to the picture, the preview or the export (it is left out of the picture plan, so its keyframes do not split sections), is edited, deleted, undone, copied and saved back like any, **and exporting asks
  first, listing what would be left out, with *Export anyway* (Q-12, decided earlier)**. **Verified:** fmt, clippy `-D warnings`, rustdoc `-D warnings`, reuse; Windows 3740 tests (3549 library + 191
  integration), Linux with ffmpeg 9 3739, 0 failed. **Mutation sets** `_c` (88) + `_d` (6) + `_e` (12): 106 planted mistakes, all noticed after **6 real test gaps** were fixed (a foreign key with major 1 and
  few values was taken for the hue shift; a sound effect that changes each pixel was allowed on a group; the clip kinds of a sound effect's values; the label of a value read without one; keyframe buttons
  for non-numbers; the unit of a slider), and **the 5 equivalents that step B left are killed now** (a second kind and an effect with several values exist as tests). **Limits, so that nobody is surprised:**
  (1) binding is the simple rule above: no alias table for renamed values, no "newest minor wins" beyond it, and no clamping of values to a changed range (a number can be typed beyond its range anyway);
  (2) the kinds that a file brought live for the whole run (a few hundred bytes each: interning leaks on purpose); (3) unknown fields are kept at the effect, snapshot and value level only (clips, tracks
  and the rest still refuse an unknown field); (4) no `enabled` flag (bypass), `category`, `role`, input slots, resources or `extra` blob yet: a file that has them keeps them as unknown fields, but the
  program does not use them (phase 4+); (5) a sound-domain effect can be kept and shown but nothing makes it do anything (phase 7); (6) the descriptor of a built-in effect must not change without a version
  change (a test has the hue shift as it was in 1.0, but a version change brings the job of *migrating* what was saved with the old one, which is not written, because nothing changed yet).
* **0.5 Spikes** (QA-07, S1-S3) [P], throwaway code, each ends with a written finding: **S1** one ISF shader through the rewriting step and naga to a picture; **S2** a deliberate hang and a
  lost device on a software adapter / WSL (never the maintainer's GPU), to decide how plugin passes are guarded (EN-05); **S3** one `.cube` in a shader against ffmpeg's `lut3d`.
  (The old S4, the seek latency of a linked libav, is dropped: libav linking is not on this plan, section 5.)
  **0.5 is DONE (2026-10-15 [V]); the findings are in [plugin-system-spike-findings.md](plugin-system-spike-findings.md), the code in `tools/spikes/phase-0.5/`.** Only software adapters were
  opened (WARP on Windows, lavapipe in WSL), on both. **S1:** eight ISF-style shaders (mine, not a third-party corpus) go through a rewriting step, naga (GLSL in, WGSL out) and wgpu to pictures
  that agree with a CPU calculation to 1 of 255 and between WARP and lavapipe to 1 of 255; the rewriting step's job is now a list (no combined `sampler2D`, no `##`, no `varying`, bool as int, offsets
  from naga, `IMG_*` calls rewritten to generated functions). **A real finding: on WARP, wgpu 30.0.1 gives two draws that use different static uniform buffers the data of the last one** (dynamic
  offsets, textures, real GPUs and lavapipe are all right): the engine keeps the uniforms of a frame in one arena with dynamic offsets. **S2:** a watchdog (poll with a timeout) works and a
  fresh device can be made at once, **but a runaway shader on a software adapter keeps using the CPU until the process ends**; a loop cap injected in naga's IR bounds every loop (0.5 s instead of
  never) and leaves ordinary shaders' pictures byte-identical; a real card's timeout (TDR) was not tested (by rule). **S3:** the shader equals a CPU reference to 3e-7 and ffmpeg's `lut3d` to
  1 of 255 (**ffmpeg truncates; and ignores `DOMAIN_MIN/MAX` for the table tried**). Two questions for the maintainer are at the end of the findings.

### Phase 1 — The engine skeleton, in compatibility mode (size L)

EN-01, EN-04, EN-12, EN-02, EN-08, EN-09, CM-09, CM-17 (in part), CM-20.

One renderer on wgpu draws the preview from the plan: decode (ffmpeg children, now giving raw frames in their native pixel format) -> upload -> **YUV to RGB on the GPU** -> placement ->
effect stack (the hue shift as a WGSL pass) -> composite -> track effects -> an output pass into an 8-bit texture that egui shows (CM-04). The working format is `Rgba16Float`, but the
mode is **display-referred and reproduces today's pictures**, **including today's accident that untagged video is converted as BT.601** (so that parity with the oracle can be *proved*;
phase 3 corrects it). Textures come from a pool; the engine runs on a thread of its own behind channels, and its interface is **message-shaped** (an immutable serialisable scene value in,
a frame out; EN-12), so that a command-line export and a render process stay possible (Q-31). The old egui layers stay as the oracle.
*Gate (from the long-term plan):* random projects (the existing generators) rendered by the engine and by the oracle agree within a stated tolerance and a pixel-count budget; a typical stack
(four layers, two blurs and a LUT) plays at **720p60 on the integrated GPU** (**1080p60 until 2026-10-16**: see the answers after phase 2), measured by a benchmark in the repository (EN-07); a deliberately bad shader gives an error and never a hang
(EN-05); CI is green on software adapters. *Delivers:* nothing visibly new except adaptive preview quality (EN-08: lower resolution before dropped frames, full resolution when still) and
a preview that no longer converts colours in ffmpeg. **Rules from the spikes (see plugin-system-spike-findings.md):** the uniforms of every pass of a frame live in **one arena buffer bound
with dynamic offsets** (WARP gets static ones wrong), bind groups come from **explicit layouts** (an automatic layout drops unused bindings), pictures inside the engine are in **GL orientation**
(row 0 is the bottom row; flipped once at upload and once at output), and the engine runs on **its own device**, because egui-wgpu and eframe do not handle a lost device.
**Correction of 2026-10-15, when the design of phase 1 was written ([engine-phase-1-design.md](engine-phase-1-design.md), E5): pictures in the engine are top-row-first, like the window's and the oracle's; GL orientation is the ISF host's business
(its generated `IMG_*` functions and `isf_FragNormCoord` flip at its own boundary, which costs nothing because they are generated anyway).** The design also records two deliberate departures from the list in EN-04, both open to the
maintainer's objection: a clip's effects run **before** placement, in the clip's own picture (E2), and the hue shifts of a layer are one fused colour-matrix call (E3).
**Progress, 2026-10-15 [V]:** pieces 1.1 (skeleton), 1.2 (place and composite, the egui-wgpu oracle), 1.3 (video pictures in planes: probe, `FrameDelivery::Yuv420p`, the planes event, the YUV pass) and 1.4 (the colour matrix effect,
`describe_frame`, and the gate that runs the real old drawing against the engine on 400 random scenes: 2 levels at most except on the edges of layers) are built and verified on Windows (WARP) and Linux (lavapipe; the real-ffmpeg tests
on CI). 1.3 is described in section 9 of the design, with the one decision that measuring it changed: **chroma is replicated like ffmpeg does, not blended** (within 3 levels of ffmpeg at 1 to 1 and 2 to 1; a blend was off by up to 82).
**Progress, 2026-10-16 [V]:** 1.5 (the window can make its preview with the engine: `FASTFORWARDRS_PREVIEW=engine`, off by default until the maintainer has looked at it on a real card, and the old drawing is the fallback if the engine stops) and 1.6 (the guard:
loop caps in every shader that is given to the engine, `Engine::compile_effect_shader`, the watchdog that replaces the device when a frame does not finish or the device is lost, effects of strangers switched off for the run, three replacements within a minute stop the engine; the gate tests, one of
them in a process of its own that is killed by its id) are built and verified on Windows (WARP) and Linux (lavapipe).
**Progress, 2026-10-16, the end of the phase [V]:** 1.7 (the quality governor and its use by the preview, the benchmark, the pictures of what differs from the old drawing, the mutation campaign, the README) is built and verified on the same two systems, and CI was green (Windows, Ubuntu, macOS, licences, tools) at the last commit
before this paragraph. **The maintainer ran the benchmark on 2026-10-16 (SF-01 keeps me from running it): the gate is NOT made on the integrated graphics.** The AMD Radeon makes the whole stack at 1080p in 20.09 ms (49.8 frames to the second) against the 16.67 ms of 60; the RTX 3090 makes it in 2.18 ms (459.1).
The maintainer accepted the one visible difference (the edges of saturated colours). **The gate stays open and is carried into phase 2:** the benchmark measures one frame at a time from pictures that are on the card already, so it says nothing of a read-back that overlaps with the next frame or of a video that sends a new picture in every frame, and
phase 2 builds both (the ring of read-back buffers, the streaming upload); the benchmark gets a scene and a mode for them and the maintainer is asked to run it again (details and the estimates of what would close the 17 % in section 10 of the design). Looking at the preview with `FASTFORWARDRS_PREVIEW=engine` on a real card, which is also how the engine
becomes the default (one line), has not been reported yet. The report of the phase, with what was decided and what is asked, is [engine-phase-1-report.md](engine-phase-1-report.md).
*Risk:* parity of text, rotation and edge filtering; this is the phase where a long list of small visible differences is most likely,
and each is listed and either fixed or accepted by the maintainer.

### Phase 2 — Export through the engine (size M)

EN-03, EN-10, EN-11.

Frames are rendered by the same engine, converted to the encoder's YUV layout on the GPU, read back through a ring of buffers and piped to ffmpeg as raw video; audio is still the old
ffmpeg path until phase 7. A **command-line export** (no window) is the first user of the message-shaped interface and gives the engine tests a way to run. Then the old export code
is deleted: `filter_graph/pictures.rs`, `animated_sections.rs`, `EffectKind::as_exported`, and their tests (the tests that compared with plain evaluation are kept as the engine's oracle
tests). *Gate:* **preview and export are bit-identical on one machine** and within a stated tolerance across machines (EN-03); the real-ffmpeg journeys pass unchanged; the exported files'
tags are the same as before. *Delivers:* animated values are no longer quantised into stretches (the reason `animated_sections.rs` existed), and exports no longer depend on ffmpeg's filter graph.
**Progress, 2026-10-16, the end of the phase [V]:** the export is made by the engine, and the old picture export is deleted (with the 128 tests that only tested it; the 96 export tests of the old suite pass on the new export with their assertions as they were).
Pieces 2.1 to 2.8 are built and verified on Windows (WARP) and on Linux (lavapipe, ffmpeg 9): 3907 and 3906 library tests, every program of integration tests, `clippy`, `rustdoc` and `reuse` clean; CI was green on all five jobs at 2.7 (`1280da6`), at 2.8 (`b42fb86`) and at `c8bdab8`.
**The gate, line by line, is in [engine-phase-2-report.md](engine-phase-2-report.md), section 1:** preview and export make what a frame shows with one function and draw it with one engine (the export's planes are the preview's 8-bit frame converted, within a level, on 40 scenes of every kind; WARP and lavapipe
draw the same scenes within 4 levels, 98.5 % of the values exactly; that the 8-bit frame is **byte for byte** the same in both is [B], it is not read back); the real-ffmpeg journeys pass with mechanical edits and one replacement; the tags are the same for untagged, `bt709` and `smpte170m` videos and differ on purpose for a video of the full range.
*Delivers:* animated values are exact at every frame (a journey exports 1560 frames each in another place, where the old export refused), and the export no longer depends on ffmpeg's filter graph. **Measured against the old export on 13 projects** (design, section 9, reproduced from a copy of the old commit): the same frames; colours within 3 levels
(the hue shift nearer to the arithmetic); the edge pixels of what is turned or scaled differ; the old export put a half-size video one row too high. **A planted-mistakes campaign over the new code** (274 mistakes; 12 not noticed, each with its reason: design section 11.3) found gaps in the tests and dead code, and a panic in the export's thread that would have left
the window waiting for an end for ever: all mended. **The 1080p60 gate on the integrated graphics is still open:** the benchmark has the scene with a new video picture in every frame and a table with two frames on their way (on the software adapter (WARP) at 640 by 360, 30 frames of each scene, the whole stack makes **8.7 frames to the second** with its pictures already on the card and **5.8** when every frame is given new pictures of video (5.9 with two frames on their way: a processor does not do two things better than one), which says nothing of the gate); the maintainer is asked to run it again, and to try `fastforwardrs-export --adapter best`, which has
never been run by anyone (SF-01). The CI has a step that runs the tests of the engine and of the export on the Metal device of the Mac that it runs on, with failures as warnings: **Its result [V, the notices of the run of `c8bdab8`]:** on `Apple Paravirtual device (Metal, IntegratedGpu)` (the virtual graphics of a GitHub runner, which is a virtual machine: not the graphics of a real Mac) **the 365 library tests of the engine and the export that were run passed, and all 35 tests of the export's integration programs passed, none skipped** (`command_line_export` 6, `video_export_encoder` 5, `video_export_pictures` 9, `video_export_runs` 15: the export is made on Metal, end to end, with the real ffmpeg, and cancelling works when ffmpeg is stuck). Three more library tests skipped themselves, being about software adapters. (The first run of the step, at `b42fb86`, had 4 library failures, each with a software adapter as its premise: they skip there now.)

**The maintainer's answers of 2026-10-16, after phase 2 [D]**, and what was done with them (the benchmark was run on the integrated AMD Radeon at 1080p: 50.3 frames to the second one frame at a time, 44.1 with a new video picture in every frame, 43.6 with two frames on their way; the RTX 3090 459.1):

* **The gate is 720p at 60 frames to the second on the integrated graphics, not 1080p.** The benchmark's default size, its report ("the gate is 60 at 720p on the integrated graphics") and its tests say so (`FRAME_SIZE_OF_THE_GATE`); `--size 1920x1080` still measures 1080p, as a figure that is not the gate. **Whether 720p makes it is not measured [B]:** the work per frame is nearly all per pixel, and 720p has 44 % of the pixels of 1080p, so about 9 to 10 ms a frame, 100 frames to the second, would follow from the figures above; the maintainer is asked to run `cargo run --release --example engine_benchmark` once more, which measures it by default now. (The preview is never made bigger than 1280 by 720: `playback::preview_picture_size`.)
* **The edges of saturated colours, the chroma-edge difference: accepted** (phase 1), **and the pictures of the differences of the export: acceptable** (phase 2, question 3).
* **X10: yes, the export says when it held a picture.** The window's message at the end and the command line's warning say how many pictures of videos were held (`VideoExport::pictures_held`, `words_about_held_pictures`).
* **A separate `fastforwardrs-export` program is acceptable.**
* **E8: the engine preview is the default.** `FASTFORWARDRS_PREVIEW=old` asks for the old drawing, which stays as the oracle and as the fallback when the engine cannot start or stops.
* **The window announces a replaced graphics device** (a message that waits for a question that is being asked and is put under a message that is showing).
* **LADSPA moves earlier:** the order of work is now **3, 4, 7, 5, 6, 8, 9**: the sound engine and LADSPA (phase 7) are built straight after phase 4, as the first plan offered (section 4). The phases keep their numbers.
* **"7. continue":** phase 3 follows.
* **A bug the maintainer found while testing:** every load and save of a project file, with nothing changed, made it about 35 KB longer (the saves append the text of the project, and the garbage that they leave was taken away only when it was at least 4 MiB). Mended: a project file that has little in use (8 MiB or less: every project that has no file included in it) is written again without its garbage whenever there is any, when the user saves and when the window is closed; the maintainer's file of 242 149 bytes comes out at 35 349 and reads the same [V]. The text that looked like invalid JSON was the file around the JSON: a `.ffrs` file is a zip, uncompressed, and held the text of the project once for every save (7 times in the file looked at); the JSON entry itself is valid [V].

### Phase 3 — Linear light and colour management (size M-L)

CM-01 to CM-05, CM-07 to CM-12, CM-14 to CM-16, CM-17 (complete), MD-11, MG-01. **A separate, reviewed change** (CM-20).

The compatibility mode is turned off (and with it the compromises of phase 1 that exist only to keep today's pictures: **copying the colour planes of a video to the pixels they stand for, as ffmpeg does, where a smooth reconstruction is
the better picture; the files ffprobe calls `yuvj420p` (full-range H.264), 10-bit, 4:2:2 and 4:4:4 being decoded to RGBA by ffmpeg instead of being given to the engine as they are**): compositing in linear light, a working colour space per project (linear Rec.709 primaries by default; ACEScg offered), an input space per source (read
from the file's tags; the assumption for untagged video is **BT.709 for HD and BT.601 for SD**, Q-05), a display transform in the global settings, an output space and correct tags
on export. **Old projects are converted** (Q-05: no legacy flag): they look slightly different at scaled edges, in blends and for untagged HD video, and the release notes say so. Effects
declare their colour domain (CM-10); the hue shift stays display-referred (Q-07). *Gate:* the round-trip tests and the tests against published numbers (CM-16, including "50 % white over
black is 0.5 in linear light, 0.735 in sRGB code"); **a written list of every visible difference with before/after pictures, reviewed by the maintainer** before the phase is called done.
*Delivers:* the colour handling that was asked for as "important".

### Phase 4 — The effect surface, built-in effects and the `.cube` host (size L)

SC-04, ID-01 to ID-09, MD-01 to MD-10, RS-01 to RS-09, EF-01, EF-07, UI-01 to UI-07, UI-11, DS-01 to DS-03 (folders; binary scanning waits for phase 7), PH-01 to PH-05, PH-20,
MG-02, LG-02 (manifest), SF-02, SF-03, SF-11, Q-12.

What a user sees for the first time: the **+ menu with categories and a search field** listing effect types; the **properties panel built from descriptors** (sliders, pairs, colours,
choices, toggles, a file chooser), each parameter with the stopwatch and keyframes in racks; **several instances of an effect on one object**, reorderable, bypassable, duplicable; **missing effects** with a badge
and an **export that refuses and lists them, with an "export anyway" button** (Q-12); the **built-in effects** (brightness/contrast, saturation, exposure, blur, crop, opacity, and the LUT effect,
Q-10); **"Add effect from file..."** for a `.cube`; **resources embedded or linked** by a setting in the project settings and in the global settings (embedding is the default; a plain-JSON
project falls back to link with a note, and the program asks once whether to save as `.ffrs`: Q-03), with the 64 MiB limit and its prompt (RS-07). The `.cube` host reads 1-D and 3-D tables **and the
Resolve variant (read only)** and applies them with tetrahedral interpolation on the GPU. Our own sample LUTs ship (LG-08). *Gate:* a table through the shader equals ffmpeg's `lut3d=interp=tetrahedral`
within a tolerance (PH-04); conformance tests for each built-in; a project with a missing effect survives a round trip unchanged and renders as pass-through; mutation campaign.
*Delivers:* **the first plugin family the maintainer asked for.** This is the first phase after which the program is visibly a different program.

### Phase 5 — Group effects, adjustment clips and generator clips (size M)

GR-02, GR-06 to GR-10, EF-02, EF-03 (the backdrop slot, first use), EF-04, Q-33.

The engine composites a group, so the **placeholder of section 13 is replaced by the real thing**: a group's effects apply to the composite of what is in it. A group is isolated if it has
effects, an opacity below 100 % or a blend mode, and pass-through otherwise (hidden from the user; Q-33). **Adjustment clips** (an effect over everything behind a clip for its length) are
the cheapest use of the backdrop mechanism. **Generator clips** (no media, 5 s by default, extendable: Q-11) exist as a clip kind, with the built-in effects that generate. The groups
specification's rules (what may be inside a rectangle, what a message says) are unchanged. *Gate:* **"ungroup gives the same render"** (GR-07) and "group with no effects equals no group"
as tests; the on-composite effect table's restriction (`on_a_composite`) is lifted and its tests are rewritten on purpose; preview equals export. *Delivers:* effects on groups that are exact.

### Phase 6 — The ISF host (size L)

PH-10 to PH-15, PH-16, EF-03 (transitions), EN-05, EN-06, EN-14, ID-10, AN-05 (silence), SF-01.

ISF 2.0: the header becomes a descriptor, the source goes through the rewriting step and naga into WGSL **in a worker thread that survives a panic**, with errors shown with line and column;
multi-pass shaders, persistent buffers, `TIME` as clip-local time, imported images as resources (found only by entry name: RS-08); **filters, generators and transitions** told apart by their inputs;
**image inputs** are in the descriptor (ID-10; the interface to fill one from another layer can come later). Every plugin pass runs under a **watchdog**; a pass that takes too long or a lost device disables
that instance, says which, and recreates the device. **Feedback effects:** the preview offers the two global modes of EN-14 (*Bounded*, 300 frames, with a checkpoint interval and a memory budget; *Warm-up only*), the export is always sequential and
exact, and the interface says "depends on earlier frames". The shaders that ship are **gl-transitions and the clearly MIT original Vidvox shaders** with notices (Q-25). *Gate:* the **golden corpus** (10 to 20 shaders compared by
hand with the ISF Editor on Windows, then frozen as expected pictures: Q-24) passes on the shipped shaders; the deliberate-hang test passes on a software adapter and in WSL; a feedback effect behaves as EN-14 says on seek.
*Delivers:* the second plugin family, including **transitions**.

### Phase 7 — The sound engine and LADSPA (size L + M), runs beside phases 1 to 6

AU-01, AU-03 to AU-09, AU-12, DS-04, DS-05, SF-04 to SF-10, UI-12, PL-04.

A **block graph in the program** that the speakers and the export both call replaces the two ffmpeg sound graphs; the old graphs stay as the oracle until a **null test** shows the mixes match (AU-09). `atempo`
stays for sped-up clips (Q-29). Plugins are changed sample-accurately, reset on a seek with a pre-roll, and a missing plugin passes the sound through. **LADSPA** comes first, with the **LMMS 1.2.2 plugins** as the
test set (119 DLLs, 64-bit [V in the features list]); scanning happens in a **child process** so that a crashing plugin is listed as failed and never takes the editor down. **This is also where native code
first loads, so the whole safety design of section 14 of the features list is built here, before any binary is loaded:** the warning prompt for a linked plugin (per plugin, by path and hash), the severe prompt for an
embedded one (every time a project is opened; accepted by holding "Load it" or Enter for 5 seconds while it fills; never Space), declining as a safe state, several platform builds per embedded plugin,
extraction to a cache folder with a hash check, the 256 MiB cap [D], and the command-line refusal without `--allow-plugins`. *Gate:* the null test; the LMMS set loads, plays and exports in a
test project; a zero-byte or damaged DLL is "failed", never fatal; the prompt tests (including the hold, the key, the reset on release, and "declined renders as pass-through"). **CLAP follows** as a second
step inside this phase (a different host, the same graph). *Delivers:* the sound plugins of the original request.

### Phase 8 — The plugin manager (size S-M)

UI-09, DS-05, DS-06, PH-21 (if wanted), LG-05, QA-03 (corpus tooling).

A window, opened from a menu, listing the effect types found and those that failed with the reason, folders to scan, disabling a type, and which of two same-named types won (user folder over program folder).
Optionally the program's own plugin format (a descriptor plus WGSL, no ISF shim) with a documented example (LG-05). *Gate:* the manager's data comes from the same registry the menu uses (no second list). *Delivers:* a
way to understand why an effect is missing.

### Phase 9 — Text objects become generators (format 12) (size M-L)

EF-06, ID-05 (the font parameter type), SC-05, Q-17.

Text becomes a generator effect with the parameters it has today (text, font, size, colour), drawn on the CPU and uploaded (a second kind of host, "CPU image producer"). One migration from format 11. Done **last** because it
changes a lot for no new capability and benefits from a settled model. *Gate:* every text test passes unmodified except the file-format ones; old projects look the same.

## 4. Order: what I considered

* **Sound earlier or later.** Sound needs nothing from the GPU phases, only phase 0.4, so it could run first or in parallel. I put it as phase 7 because you asked for `.cube` and ISF first, and because it carries the
  heaviest safety work. **If you would rather have LADSPA sooner (it was in your original request), it can move to straight after phase 4 with no cost to the others; say so.** [P] **Said so, 2026-10-16 [D]: phase 7 is built straight after phase 4.**
* **LUTs before the engine** (the long-term plan's 3a) is dropped: it would build a second, colour-unaware LUT path (Q-37, decided).
* **Linear light together with the engine port** is rejected (CM-20): neither could be checked.
* **A node editor, a render process, libav linking, OpenFX** are not scheduled. They are kept possible by rules already in the features list (EN-12, EN-15, ID-10). Libav linking (option B of the long-term plan) is a
  separate decision, to be taken **only if phase 1's measurements show the decode path is the bottleneck**; the GPL licence no longer forces any choice.
* **A shortcut I rejected:** shipping `.cube` LUTs sooner by running them through ffmpeg's `lut3d` before the engine exists. It would be the only colour-unaware, display-referred LUT path in
  a program that is about to become colour-managed, and would have to be thrown away (Q-37).
* **If you want something sooner than phase 4:** phases 0.1 (Linux and CI) and 0.2 (licence files) are cheap and independent of everything else, and the sound engine can move earlier (first bullet).

## 5. Not in this plan (and not impossible)

HDR and PQ/HLG (the working format holds it; CM-13), EXR/DPX and image sequences (Q-18), a master bus (Q-28), on-canvas handles for point parameters (Q-20), 10-bit and other codecs and hardware encoders (Q-36), OpenColorIO (Q-35),
audio-reactive shaders (Q-22), a node-editor interface (Q-30), a render process (Q-31), presets (EF-08), per-keyframe interpolation (AN-03), linking one parameter to another and expressions (AN-07), masks and tracking, VST3 and LV2,
WASM plugins, OpenFX (shelved). Each can be asked for later; the features list says what keeps each possible.

## 6. Risks, and what would change the plan

* **Parity is the risky phase (1).** If the oracle comparison keeps finding small differences in text, rotation or filtering, the right response is to list them and let the maintainer accept each, not to loosen the tolerance silently.
* **The identity refactor touches about 100 files.** The spike (0.4) exists to find out early; the fallback is stated.
* **Shader hangs and driver bugs** are real. The watchdog and "disable the instance" behaviour are built in phase 1/6 *before* user shaders are accepted, and are tested only on software adapters or in WSL.
* **Cross-vendor drift** (at most 1/255 in measurements): "equal" means equal on one machine and within a tolerance across machines (EN-03); this is written in the documentation.
* **ISF correctness** is not the compile rate: the golden corpus is a gate (phase 6).
* **A native plugin that is embedded is code that runs with your rights.** The prompt is honest about it; the design cannot make it safe, only deliberate.
* **Solo-maintainer load.** Nine phases are a lot; each ends with something usable, and phases 6 to 9 can be dropped without harming 0 to 5.
* **What would change the plan:** a hard requirement for 4K scrubbing on integrated GPUs (favours libav linking early); evidence that WSL cannot run the engine tests (favours a VM or only CI); a decision to close the source (the
  GPL ffmpeg and several shipped shaders would have to go); a user base asking for a specific OpenFX plugin.

## 7. What I need from the maintainer, and when

| When | What |
|---|---|
| ~~now~~ done 2026-10-15 | ~~Start Ubuntu once~~ |
| before phase 0.2 ends | **the words of the plugin clarification** (Q-34): I draft, you decide (accepted 2026-10-15) |
| before phase 0.1 ends | **commit and push** the GitHub Actions workflow I create (accepted 2026-10-15; I do not commit) |
| whenever | say if **sound should move earlier** (section 4) |
| during phase 3 | review of the **list of visible colour differences** |
| during phase 6 | **run the ISF Editor by hand** on the 10 to 20 shaders of the golden corpus (or tell me to use another reference) |

## 8. Feature list to phase map

| Features | Phase |
|---|---|
| LG-01 to LG-07 (the licence mechanics), PL-02 to PL-04, QA-06 | 0.1, 0.2 |
| ID-01 to ID-06, ID-08, ID-09, MD-01 to MD-10, MD-12, MG-03, MG-04, QA-07 | 0.4 (descriptors and instances), 4 (what the user sees) |
| EN-01, EN-02, EN-04, EN-08, EN-09, EN-12, CM-04, CM-09, CM-20 | 1 |
| EN-03, EN-10, EN-11 | 2 |
| CM-01 to CM-03, CM-05, CM-07, CM-08, CM-10 to CM-12, CM-14 to CM-17, MD-11, MG-01 | 3 |
| UI-01 to UI-07, UI-11, RS-01 to RS-09, PH-01 to PH-05, PH-20, EF-01, EF-07, DS-01 to DS-03, DS-06, SC-04, Q-12 | 4 |
| GR-02, GR-06 to GR-10, EF-02, EF-03, EF-04 | 5 |
| PH-10 to PH-16, EN-05, EN-06, EN-14, EN-15, ID-10, AN-05 | 6 |
| AU-01 to AU-09, AU-12, DS-04, DS-05, SF-04 to SF-10, UI-12 | 7 |
| UI-09, PH-21, LG-05 | 8 |
| EF-06, SC-05 | 9 |
| ID-07, UI-08, AU-10, AU-11, CM-06, CM-13, CM-18, CM-19, EF-08, AN-03, AN-07, PL-03, SC-02, SC-07 | not scheduled (later or shelved) |
