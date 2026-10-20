<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Phase 7, piece 7.8 report: CLAP sound plugins

Written 2026-10-20, at the end of piece 7.8 (the design is [engine-phase-7-clap-design.md](engine-phase-7-clap-design.md): decisions C1 to C14, six pieces, and a section at the end for each piece with what it found; the report of the rest of phase 7 is
[engine-phase-7-report.md](engine-phase-7-report.md)). This is the short form: **done, what can be seen, tests rewritten on purpose, numbers, limits, mistakes, verified versus believed, what I need from you**.

**[V]** means I ran something and saw it. **[B]** means I believe it from reading the code. You have listened to 7.8.4 (the test plugins, in the preview and the export) and said it sounds good; **no real CLAP plugin has been heard by anyone** (this computer has
no CLAP folder, and you said to test with the test plugins).

## 1. Done

| piece | what | commit |
|---|---|---|
| 7.8.1 | `crates/clap-host` (a safe host over the `clack` crates: one `unsafe`, the loading of a file, counted by a test) and `crates/clap-test-plugins` (16 plugins, 3 that misbehave on purpose) | `79e959b` |
| 7.8.2 | the scan program reads `.clap` (the same child process, failures, cache and 5 s limit); `CLAP_PATH` and the standard folders, folders below them too; macOS bundles | `e5902fc` |
| 7.8.3 | the registry (`PluginAddress`, `value_handles`); a CLAP plugin is an effect (key `clap:<id>`, values from parameters: number, whole number, choice, switch, logarithmic slider); the *CLAP* menu | `7358489` |
| 7.8.4 | CLAP plugins run in the preview and the export: values as events at the sample, latency made up for to the sample, the main-thread worker, restarts; one `PluginEffect` for both formats | `2215d1d` |
| (your three requests) | the question *Trust plugins from this folder?*; a placeholder that says why for a sound plugin that does nothing; a scan program older than the editor is refused | `5c20823`, `e53a14f` |
| 7.8.5 | sound from before a clip for the plugins that remember (the pre-roll), so a seek into an echo sounds as if played through | `ea7e5f3` |
| 7.8.6 | the report on the real set (ignored, as for LMMS), the conformance tests, this report | the commit of this report |

### The gates of the table, line by line

7.8.1 *the real test library loaded, described, made, run on every plugin; the threads; events at their offsets; latency and tail read* [V]. 7.8.2 *the real scan program on the test library and every misbehaviour; a bundle made by a test on macOS* [V on Windows,
Ubuntu and macOS by the CI]. 7.8.3 *the CLAP menu has the plugins; a project with a CLAP effect is saved and read back whole, and where the plugin is not installed it is kept and says so* [V]. 7.8.4 *to the sample, preview = export, with the real ffmpeg* [V]. 7.8.5 *a seek into
a reverb gives the samples of playing through, within the tolerance of its tail* [V: under 1e-4 with the echo of the test library, in the export and the preview, against the control that is not]. 7.8.6 *the conformance round trips (a plugin gone, one whose parameter ids
changed, one whose kind of value changed), byte for byte* [V]; *the real set* is a report that runs on your computer (`cargo test --test sound_plugins_clap_set -- --ignored --nocapture` with `FASTFORWARDRS_CLAP_FOLDER`): I ran it on the folder of the test library, as a
dry run of the report itself (12 plugins, all fine), **not on real plugins**.

## 2. What can be seen

The *CLAP* smaller menu under the plus button, with the plugins under the names of their vendors; their parameters as controls; keyframes on them; the placeholder line (*Not running: ... Its values are kept.*) when the plugin does nothing, with the reason; the question about a folder in
its new words; a note in the window when a scan program is too old. Nothing else is always visible that was not before.

## 3. Tests rewritten on purpose

The tests that read the old words of the question about a folder (*Run plugins from this folder?*, *Don't load*, *Load it*), and the one that checked that it names both plugins (it now checks that it names the folder); the test that checked the grey *Not installed* line for a
plugin that is gone (it now checks the placeholder and its reasons); the plugin counts of the test library (9 LADSPA ones are unchanged; the CLAP library went from 8 to 16 plugins as the pieces added them); the name of the `callbacks` test plugin's first test (it has a parameter now).
The 257 tests of the LADSPA host were not rewritten: they passed with only their construction changed, when the effect became one for both formats.

## 4. The numbers

Planted mistakes: 7.8.1 **31** (30 noticed; the one left is a library with `clap_entry` and no factory, which a test cannot make); 7.8.2 **27** (24 noticed, 2 left that only Ubuntu and macOS can notice, and 1 removed as redundant); 7.8.3 **35** (all noticed after 5 tests); 7.8.4 **34**
(31, then the 3 others: 1 equivalent, 1 removed, 1 closed); your three requests **21**; 7.8.5 **21** (all noticed after 4 tests). Library tests on Windows: 4615 at the end (all test programs of the workspace ran and none failed in the last run; in earlier runs a `media_export` or `media_in_place` test twice timed out on
the software graphics adapter under load and passed alone, as such tests did before CLAP). CI: green on five jobs at `79e959b`, `e5902fc`, `7358489`, `2215d1d`, `e53a14f`; red once, at `5c20823`, for **a test of mine that named the folder of a file that was not there** (a path that does not exist is
made absolute differently on Windows (short names) and macOS (links): mended in `e53a14f`).

## 5. Limits

No window of a plugin, no state, no presets, no instruments, no notes: only effects, with their parameters (C11, C12). Plugins run in the editor's own process: one that crashes ends the editor, and the breadcrumb says which file. A CLAP plugin that needs the real main thread of the
process is not served by the thread of its own that the host starts [B]. A clip that is sped up has no pre-roll. A plugin that says nothing of its tail costs a clip half a second of sound read before it. A latency that changes when a plugin is made again is not made up for until the next play
(a note says so; untested [B]). An effect has at most 255 values: a plugin with more keeps its first 255. Embedded plugins' effects (CLAP and LADSPA) still do not run (AU-13, later, as you said). A scan program is refused after any change to `src` or `crates` until everything is built again.

## 6. Mistakes (mine)

7.8.1: the test library was looked for beside the build and not beside the test program, so with a fresh build folder every test that loaded it failed (found by the work copies of the mutation tool, and it would have failed the CI step I had just added). 7.8.4: the `latency` test plugin
asked for a restart, which loses its memory, so exact compensation could not be tested (a separate plugin asks now). **The one you met:** `cargo run --release` builds the editor and not the scan program beside it, so a scan program from before 7.8.2 read your `.clap` as a LADSPA library
and blamed it: now refused, with the cure in the message. And the red CI at `5c20823` above. Bash heredocs of this environment corrupt `\u`, `\n` and `\r` inside Python strings; I write scripts with the file tool (a cause of three false starts).

## 7. Verified versus believed

**Verified [V]:** everything in section 1 marked so; the sound of the test plugins in the graph to the sample for any blocks; preview = export with the real ffmpeg; latency made up for to the sample; a seek into an echo; the placeholder's reasons; the refusal of an old scan program.
**Believed [B]:** that real CLAP plugins load and sound right (none was tried); the macOS bundle (the test ran on the CI's Mac and passed, but it is a bundle I made, not a vendor's); plugins that need the process's main thread; a latency that changes after a restart.

## 8. What I need from you

(1) **Try a real CLAP plugin** if you have one: put it in `%COMMONPROGRAMFILES%\CLAP` (or a folder you add), say yes to the folder, put it on a clip; or run the report: `FASTFORWARDRS_CLAP_FOLDER=<folder> cargo test --test sound_plugins_clap_set -- --ignored --nocapture` and send me the lines.
(2) The windows of plugins (C12) are a phase of their own, as you said: say when. (3) **Embedded and bundled plugins that run without installing** (AU-13) are the other thing you named for later; and the search for plugins whose licences let us bundle them is still not done, as you asked. (4) The order:
the plan says phase 5 next.
