<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Synthesizers that the Bundles tab fetches (2026-10-29)

The maintainer asked (2026-10-29): "find some synthesizers (LV2 and CLAP) that we can legally bundle with this project, and make them downloadable from the plugin manager ... I have tried Surge XT before and I like it, so please make it a priority if its license is compatible with ours." This is what was added, what was left out and why, what trying them found, and what is still open. **[V]** = I ran it or read it myself today; **[B]** = I believe it from reading, and could not check it.

*I am not a lawyer; this is a reading of the licences by someone who has to ship the result, not legal advice.* The policy is the one of `distribution-notices.md` (LG-02 as amended on 2026-10-20): GPL and LGPL licences compatible with GPLv3, and the permissive ones; not AGPL, not GPL-2.0-only, nothing without a licence.

## What was added: five sets (no new download at all)

All five come out of release files that the Bundles tab fetches already for the reverbs, so **the download is exactly as big as it was** (Windows 151.5 MB, Linux 114.5 MB, macOS 269.8 MB: `bytes_to_download` counts a release file once).

| Set (id) | Formats | Licence | Source of the release file |
|---|---|---|---|
| Surge XT (synthesizer) (`surge-xt-synthesizer`) | CLAP | GPL-3.0-or-later (the same text as the effects set) | `surge-xt-{win64,linux,macos}-1.3.4-pluginsonly`, the file that `surge-xt-effects` is in |
| Kars (synthesizer) (`kars`, `kars-lv2`) | CLAP, LV2 | ISC | DPF-Plugins v1.7, the files of MVerb |
| Nekobi (synthesizer) (`nekobi`, `nekobi-lv2`) | CLAP, LV2 | **GPL-2.0-or-later (see below)** | DPF-Plugins v1.7 |

**Surge XT is licence-compatible and is in**, as the maintainer asked (GPL-3.0; 1.3.4 is the newest release, 2024-08-11 [V]). **LV2 synthesizers are Kars and Nekobi**; Surge XT's LV2 bundle exists for Linux only and the program cannot use it yet (see *Left out*).

**A correction of mine about Nekobi.** `bundleable-clap-plugins.md` said Nekobi is "GPL-2.0 only" and left it out; the maintainer's decision of 2026-10-20 repeated that. **I had read the table of the DPF-Plugins pack, which says only `GPL-2.0`, and not the source files.** Today **[V]**: every source file of Nekobi that has a notice (`DistrhoPluginNekobi.hpp`, `DistrhoUINekobi.*`, `NekoWidget.hpp`, `nekobee_synth.c`, `nekobee_voice.c`, `nekobee_voice.h`, `minblep_tables.c`, `nekobee_types.h`) says "either version 2 of the License, or (at your option) any later version", which is **GPL-2.0-or-later and is allowed by LG-02**. Two files have no notice (`nekobee_voice_render.c`, and `DistrhoArtworkNekobi.cpp`, which is the picture of the plugin as numbers: "Auto-generated binary data file"), and `nekobee_synth.h` mentions the LGPL v2.1 for a part that came from alsa-lib (compatible). **That is a reading, not a certainty**: the pack's own licence table and the text kept in the plugin's folder (the text of version 2) do not say "or later", so if you read the pack's table as the final word, Nekobi is out. **The manifest says so in the `licence_note` of both sets; taking Nekobi out is deleting two sets** (and the names in two tests).

## What trying them found (two faults of the program, now fixed)

Trying a real synthesizer found what the test instruments could not. Both are in the CLAP host and the scan:

1. **Surge XT was not taken for a synthesizer.** Its only audio input is a stereo port called "Sidechain" that says it is **not the main one**; the program took any plugin that has an audio input for an effect. [V: the scan of the real file says `audio_inputs: [{name: "Sidechain", is_main: false}]`; the test said "surge xt is not taken for a synthesizer".] **Now the main input is the first port if it says it is the main one, or if the plugin takes no notes** (`main_audio_input_of`); a plugin that takes notes and has no main input is an instrument, whose other inputs are given silence. An effect that does not flag its ports at all is still an effect.
2. **Surge XT made silence even after that.** **All of its 775 parameters say that their default is 0** (the plugin's wrapper does not say), and the program gave a plugin every value it has at its first block, so every level of Surge was set to 0. [V: called by hand the plugin plays; through the program it did not, until the next change.] **Now the default of a value is what a new plugin has** (`get_value` of every parameter, read when the plugin is scanned), and the default the plugin says is used only when the plugin says nothing. For plugins that tell the truth the two are the same. (`SCAN_FORMAT` is 9, so that a scan kept from before, which does not say what a plugin starts at, is made again.)

Both have tests that fail without them: a test instrument in `clap-test-plugins` that is made like Surge XT (one input that is not the main one, and a default that says 0 while it starts at 1), tested in the CLAP host, in the scan and **through the plugin host into sound**. **12 mistakes planted in the new code** (the rule for the main input in the scan, the host's run and its check of channels, the reading of the starting value and its use): **every one was noticed** (two of the twelve were the same mistake written twice, so eleven different ones).

## What was checked

* **[V] The files, on Windows and in Ubuntu (WSL):** the real script `tools/bundled_plugins/fetch_bundled_plugins.py` made the folder for the five sets from the real release files (every checksum right), on both systems.
* **[V] The sound, on Windows and in Ubuntu:** `tests/bundled_synthesizers_real.rs` (ignored; run by hand on that folder) scans the folder, **finds every synthesizer of the manifest as one**, plays middle C held for one second through the plugin host as an effect, and says what came out: every sample finite, a peak between 0.07 and 0.97, sound while the note is held, silence (below a tenth of that) three seconds after it ends. All five (Surge XT as CLAP; Kars and Nekobi as CLAP and as LV2) pass on both systems. **Kars and Nekobi sound a little different as CLAP and as LV2** (Windows: Kars's loudness while held is 0.049 as CLAP and 0.092 as LV2, Nekobi's peak 0.072 and 0.118; Kars starts each note with a burst of noise, so no two runs of it are the same): nobody checked why.
* **[V] A MIDI clip played by each of them through the whole engine, on Windows and in Ubuntu:** the second test of `tests/bundled_synthesizers_real.rs` puts the real synthesizer among the effects of a MIDI clip in a project (three notes: 0.1 to 0.4 s, 0.5 to 1.5 s, 2.0 to 2.2 s) and makes the sound with the plan, the graph and the plugin host. For every one of the five: **silence before the first note, and the first sound at the sample of the note** (4800 for Kars, 4806 for Nekobi, 4815 to 4865 for Surge XT, whose sound builds up over about a millisecond), sound in each of the three notes, **silence after the last note's tail**, and **after a seek to 1 s, in the middle of the second note, the held note is heard at once**, with silence before the third. No sample is compared with a number, as it is for the test instruments (a real plugin's sound is not exact arithmetic).
* **[V] The window of Surge XT:** the program that shows a plugin's window opened it (1141 by 711), and **a photograph of the real window** shows the whole interface: patch browser ("Init", by the Surge Synth Team, CC0), oscillators, filters, envelopes, effects, modulation. Nobody played it with a mouse.
* **[V] CI:** run 161 (`72c8880`, which has `0261271` under it) passed on all five jobs: macOS, Windows, Ubuntu, the Python tools, and the licences.
* **[V] The manifest:** 15 sets, checked by the policy (no AGPL, no GPL-2.0-only, https, checksums); the tests of the manifest, of the tab and of the script pass.
* **[V by the CI, on a Mac (macOS 26, arm64), run 161, commit `72c8880`]:** the members of the five sets on macOS were written by the pattern of MVerb (`dpf-DPF-Plugins-clapbundles.pkg/Payload/Kars.clap`, `...lv2bundles.pkg/Payload/Kars.lv2`, and `Surge XT.clap/` in Surge's zip), and **the real fetch of all 15 sets by the program's own downloader passed on the Mac (94 s), the scan of what it fetched passed (3 tests), and the test of the synthesizers passed (2 tests: a note through the host, and a MIDI clip through the whole engine, for all five)**. Nobody has heard them or seen their windows on a Mac. (The Intel half of the universal bundles is not run by that runner.)
* **Not checked:** listening; the windows of Kars (it has none) and Nekobi with a mouse; Surge XT with a patch other than Init (see below); the plugins in a real project through the editor (what is proven is the host, the scan and the sound of one note).

## Limits you should know about

* **Surge XT's factory patches and wavetables are not fetched.** Surge looks for its data in a folder (`SurgeXTData` beside the plugin, or `SURGE_DATA_HOME`, or its installer's folder), and the program's plugin release file has none: **a person gets the Init patch (a sawtooth) and the oscillators that need no file, and the patch browser lists only Init** [V]. The data is a separate release file of 252 MB (`surge-xt-portable-content-1.3.4.tar.gz`), and **its licences were not read** (the Init patch says CC0; the factory set has third-party patches **[B]**). Fetching it would need a new kind of member in the manifest (a folder of data, not a plugin). It is the first thing I would do next if the maintainer wants Surge with its patches.
* **The start check does not run for instruments** (it needs audio to feed: a plugin with no main input is `NotChecked`, as the test instruments were), so whether Surge's, Kars's or Nekobi's sound at the start depends on old memory (the fault that made Aura Lite unreliable) was **not tested** for them. Nothing was run for them as it was for the reverbs (800 checks each).
* **Nothing is distributed yet** (no release has been made); the sets are fetched by the Bundles tab from their authors' pages, as before.

## Left out, and why

What was looked at (GitHub's API, the licence files and the release lists; nothing was downloaded for these), and why each is not offered. **[V]** unless marked.

| Synthesizer | Licence of its code | Why it is not in |
|---|---|---|
| **OctaSine** (CLAP, VST2) | AGPL-3.0 | AGPL is not bundled (the maintainer's decision) |
| **Six Sines** (CLAP; v1.2.1 has zips for Windows and Linux) | MIT (its own code) | **its binary is built with JUCE** (`libs/JUCE` is `juce-framework/JUCE`, in its `.gitmodules` and `CMakeLists.txt`), **whose modules are AGPLv3 or commercial** (the `LICENSE.md` of JUCE today), and with Surge's filter library (`sst-filters`, GPL-3.0 **[B]**); **[B]** which JUCE commit it pins. So the "MIT" of the repository does not say what the binary is, and it may be AGPL. I would not ship it before its author says |
| **Firefly Synth 2** (CLAP zips for all three systems) | MIT (the repository's licence file) | the same: its README names JUCE; **[B]** the binary is then not MIT |
| **Dexed** (a DX7 clone; CLAP not confirmed) | GPL-3.0 | the releases of today are nightlies and old tags **[V]**, and **the plugin carries DX7 factory cartridges, whose licence nobody states [B]** |
| **Geonkick** (drums) | GPL-3.0 | a zip for Linux only; Windows is an installer (`.exe`) that the program cannot open **[V]**; not tried |
| **Odin 2** | not stated by GitHub **[V]** | installers (`.exe`, `.pkg`, `.deb`) only |
| **ChowKick** (drums) | BSD-3-Clause (its code) | installers only, and a JUCE binary (as for Six Sines) |
| **sfizz** (a sampler of SFZ files) | BSD-2-Clause | a sampler needs instruments to play, and the files are installers |
| **Monique, Shortcircuit XT, Conduit** | MIT or not stated | only "Nightly" releases, whose files are replaced by the next night: no checksum stays right |
| **Vital, Helm, OB-Xd, amsynth, ZynAddSubFX, Calf, ADLplug** | various | no usable release file (installers, source only, a `.7z`, or archived without files) **[B for each: I read the release lists and not the licences]** |
| **Cardinal** | GPL-3.0 | a modular host in a 1.1 GB archive |
| **Surge XT as LV2** (Linux only) | GPL-3.0-or-later | **it is in the file for Linux and the scan lists it as "not used"**: it has 2 audio inputs (a side chain) and 6 outputs (the main one and two scene outputs) **[V in Ubuntu]**, and the LV2 host accepts only the shapes of an effect (mono or stereo). Teaching it side chains and extra outputs, as was done for CLAP today, is a piece of work of its own; a set for it was made and taken out again |
| **SoulForce, AmplitudeImposer, CycleShifter** | MIT | not synthesizers: they take audio (SoulForce is a waveshaper); the other two are effects that are offered as before |

**Why JUCE decides so many of these:** a plugin whose window is made with JUCE contains JUCE, and JUCE is AGPLv3 or commercial (Surge XT 1.3.4 uses the Surge team's own fork of JUCE, **[V]** in its `.gitmodules`; that it is of an older, GPLv3-licensed version is **[B]**, and it is why I take Surge's own statement of GPL-3.0 as it stands). That is a reason to **ask a plugin's author what its binary is licensed under** before bundling it, and not to trust the licence file of the repository.
