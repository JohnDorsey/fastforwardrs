<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Synthesizers, and the data of Surge XT, that the Bundles tab fetches (2026-10-29)

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

## Surge XT's patches, wavetables and presets (the maintainer's second request, the same day)

The maintainer then asked for **"as many of Surge XT's factory patches and wavetables and other presets as we can legally bundle"**. The answer is a **sixth set, `surge-xt-data`, which is a set of a new kind: data** (a folder of files that nothing runs, that a plugin finds beside itself). It is **4,492 files, 383 MB on disk, from a release file of 252.8 MB**, and it has **a section of its own in the Bundles tab (*Data for plugins*), with its own button, size and question**, so that a person who wants the plugins is not made to fetch it (the button of the plugins is still 151.5 MB on Windows).

| What | Files | MB | Where it comes from |
|---|---:|---:|---|
| factory patches (17 folders: Basses, Brass, Chords, FX, Keys, Leads, MPE, Pads, Percussion, Plucks, Polysynths, Sequences, Splits, Templates, Tutorials, Vocoder, Winds) | 637 | 23.9 | Claes Johanson (Vember Audio) and the Surge Synth Team |
| third-party patches (32 folders, one for each author, from A.Liv to Zoozither) | 2,371 | 193.2 | contributed by their authors, see below |
| wavetables | 139 | 5.7 | the Surge Synth Team |
| third-party wavetables (9 folders: A.Liv, Damon Armani, Emu, Emu VSCO, Layzer, Philippe Favre, Qonundrai, TNMG, Venus Theory) | 573 | 150.0 | contributed by their authors |
| presets of the effects, presets of the modulators | 291 + 100 | 0.3 | the Surge Synth Team |
| skins | 180 | 8.2 | the Surge Synth Team |
| tuning files (`.scl` and `.kbm`, with their notes) | 200 | 1.5 | contributed (Jacky Ligon among them **[B]**) |

**Left out: 57 files.** `wavetables/Waldorf` (51 files, **named like the Waldorf Microwave's factory wavetables: `microwave1`, `MW E-Pian`, `vs1`, `JSMITH`, `WETFONE`... which are not the Surge Synth Team's** and have no statement of any licence that I found), and the skin `10 Adding Fonts` (6 files: its four fonts are under the SIL Open Font Licence, which is not on the list of the licences that may ship here: `LG-02`). Both are `leave_out` lines of the manifest, and **a pack that a maintainer would rather not ship is one more line**.

### Why it may ship, and what that rests on

* **[V] The team says the data is GPL-3.0.** Issue 6741 of the Surge repository (opened by a member of the team, still open): "GPLv3 is a poor fit for the type of material in our factory data", and it proposes to re-license it later "with the permission of all IP owners". The release is built from the repository, whose `LICENSE` is GPL-3.0, and the data is in the repository (`resources/data`) and in the source archive that the manifest names. The set is therefore `GPL-3.0-only`, which `LG-02` allows, with the licence text, the list of authors (`AUTHORS`), a notice and the corresponding source.
* **[V] Some of it says CC0 in its own metadata**: the 15 factory *Templates*, which the team re-licensed explicitly in 1.x (its issue 6743: "Replace the original set of Templates with some that are CC0-licensed"), and 78 patches in all.
* **[V] What I looked for and did not find.** I read the metadata of **all 3,008 patches**: 78 say CC0, 174 have an empty licence field, 2,756 have none, and **none says anything stricter**; and I searched every text file, the metadata of every patch and the ends of the audio files for "copyright", "all rights reserved", "non-commercial", "do not redistribute", "permission" and the like: **no hit** (the three PDFs are compressed and were not read **[B]**).
* **[V] The manual says** the third-party patches "were contributed by users and the Surge Synth Team". A contribution to a GPL-3.0 repository is made under it.
* **[B] What is not known.** **Nothing in the data states that each third-party author agreed to the GPL**: that rests on the team's statement and on the contribution. If an author never agreed, the team's statement is what the program relies on, as every packager of Surge does (Debian does not package Surge: **[V]** the pool answers 404; so there is no outside audit). I cannot do better from here; a maintainer who wants to be stricter has the per-folder `leave_out` lines, and the authors' folders are named in `NOTICE.txt`.

### How Surge finds it, and what the program does

* **Where.** The data is put in **`SurgeXTData` in the folder of the fetched plugins**, beside the folders of the plugins. On **Windows and Linux** Surge XT looks for a folder of that name **in the folders above its own file** (`SurgeStorage.cpp` of 1.3.4, which I read), so both the synthesizer and the effects plugin find it. **On macOS it does not**: Surge looks there only in the person's own `~/Library/Application Support/Surge XT`, which the program does not write in, so **the data has no build for macOS** (the tab says "no build for this computer"). A person can still set `SURGE_DATA_HOME`.
* **A fault found by the real archive, and fixed: the program now reads a `.tar.gz` itself.** The first fetch put **three names wrong** (`Café` came out `Caf├⌐`, `µcomputer` came out `┬╡computer`, `Wavetextures™` came out `WavetexturesΓäó`): **the `tar` of Windows reads the names of a tar that was made on Linux by the code page of the console.** The test that I had written for names that are not ASCII had passed, because the archive in it was made and read by the same `tar`. `archive.rs` now reads a gzipped tar itself (GNU and pax formats, long names, `./` in front of the names, a refusal of a name that goes out of the folder), with `flate2`, which the program has already; a test builds the archive byte by byte, as the real file's are, with the names as UTF-8. The `.tar.xz` files (the plugins for Linux) are still given to the system's `tar`, whose names are all ASCII.
* **The notice.** The notice, the text of the GPL and `AUTHORS.txt` are in `SurgeXTData` itself, where they go with the data.

### What was checked

* **[V] The program's own downloader**, on Windows (404.3 MB in 97 s and in 104 s) and in Ubuntu (367.2 MB in 104 s), fetched every set including the data, every checksum right. **The real checks then passed with exact counts**: 637 factory patches, 2,371 third-party ones, 291 and 100 presets, 128 `.wt` wavetables in the folder `wavetables` (179 minus the 51 Waldorf ones) and 382 `.wav` third-party ones, no `Waldorf` folder, no skin `10`, and **`µcomputer.fxp` where it has to be**.
* **[V] The Python script** (`tools/bundled_plugins/fetch_bundled_plugins.py`, which a release uses) makes **the same tree, file for file and byte for byte** (4,495 files; only the wording of `NOTICE.txt` differs, as it always did).
* **[V] A photograph of the real window of Surge XT, from the fetched folder**: with the data absent it opened on the patch *Init*; **with it, it opens on *Init Saw* (category *Templates*, by the Surge Synth Team, CC0), and its patch browser lists the 16 categories of factory patches and the 32 folders of third-party ones**, with *Tutorials*.
* **[V] 24 mistakes planted in the new code** (the reader of a gzipped tar: the check of the header, the `./`, long names, pax headers, the prefix of a POSIX name, the padding, a name that goes up, a folder that only begins with the name; the rules of the manifest for data; what is left out; the place of the set; the section of the tab): **every one was noticed**. (A first run of the campaign stopped at the eleventh, because the tests of a mistake that gave the archive back to the system's `tar` printed names that my script could not decode; it was run again from there.)
* **[V] CI, run 163 (`d64a79d`), green on all five jobs**, and the new step of the Ubuntu job (a warning if it fails) ran for real on GitHub's machine: the program's own downloader fetched everything including the data in 38 s, the scan and the count of the data passed (4 tests, none skipped), and the synthesizers were played (2 tests). On the Mac the same step fetched, scanned and played (the data is not there for macOS, and its test says so and is skipped).
* **[V] Played since (see *Playing the patches*)**: all 637 factory patches and 1,736 third-party ones were played through the plugin host and made sound (20 did not, mostly for reasons that are not faults); **and Surge XT crashes at random when a patch is loaded and notes are played**.
* **[B] Not done**: a wavetable *file* from `wavetables_3rdparty` or `wavetables` was never chosen (the patches carry the wavetable they use, so the sweeps do not touch the files; choosing one is a thing for the window).

## What trying them found (two faults of the program, now fixed)

Trying a real synthesizer found what the test instruments could not. Both are in the CLAP host and the scan:

1. **Surge XT was not taken for a synthesizer.** Its only audio input is a stereo port called "Sidechain" that says it is **not the main one**; the program took any plugin that has an audio input for an effect. [V: the scan of the real file says `audio_inputs: [{name: "Sidechain", is_main: false}]`; the test said "surge xt is not taken for a synthesizer".] **Now the main input is the first port if it says it is the main one, or if the plugin takes no notes** (`main_audio_input_of`); a plugin that takes notes and has no main input is an instrument, whose other inputs are given silence. An effect that does not flag its ports at all is still an effect.
2. **Surge XT made silence even after that.** **All of its 775 parameters say that their default is 0** (the plugin's wrapper does not say), and the program gave a plugin every value it has at its first block, so every level of Surge was set to 0. [V: called by hand the plugin plays; through the program it did not, until the next change.] **Now the default of a value is what a new plugin has** (`get_value` of every parameter, read when the plugin is scanned), and the default the plugin says is used only when the plugin says nothing. For plugins that tell the truth the two are the same. (`SCAN_FORMAT` is 9, so that a scan kept from before, which does not say what a plugin starts at, is made again.)

Both have tests that fail without them: a test instrument in `clap-test-plugins` that is made like Surge XT (one input that is not the main one, and a default that says 0 while it starts at 1), tested in the CLAP host, in the scan and **through the plugin host into sound**. **12 mistakes planted in the new code** (the rule for the main input in the scan, the host's run and its check of channels, the reading of the starting value and its use): **every one was noticed** (two of the twelve were the same mistake written twice, so eleven different ones).

## Playing the patches (the same day): 2,373 of 3,008 heard, and a crash that is not explained

**How:** the chunk of each `.fxp` (what follows its 60-byte header) was given to Surge XT as the state of a plugin, through the plugin host, as the program gives a project's state to an effect; then middle C, held for one second, in three seconds. **[V] Surge takes it**: the sound changes with the patch (Acidofil has 117 zero crossings where Init Saw has 471), and the state that Surge itself saves of the same patch is larger (50,809 bytes against 30,501), so the chunk is not the form it saves. **The values of the project come after the state and win** (a rule of the host): given every parameter's default next to a state, the patch is lost [V: the same Acidofil sounds like Init Saw]. The window of a plugin saves values and state together, so a project made through the window should not do this **[B: not tried in the program]**; the sweeps gave no values.

**What was heard [V]** (the results of every run put together; a run stops where the crash below stops it): **2,373 patches: all 637 factory ones and 1,736 of the 2,371 third-party ones** (26 of the 32 authors' folders). **No sample that is not a number**, and **2,364 different sounds among the 2,373** (the nine that are the same are probably copies, such as *Tom L* and *Tom R*). 635 third-party patches were not heard, **only because the crash got in the way**.

* **20 are silent while the note is held** (0.1 to 1 s): seven are the *Audio In* templates and the vocoder patches, which need a sound to process and were given none **[B: from their names]**; seven are short percussive sounds (shakers, hats, a ticking clock) **[B]**; *Twisted Nimbus* and *Dispell* have a slow attack (Nimbus is clearly heard on keys 36 and 60 given 8 seconds, and not on 84; Dispell's peak is 0.003 to 0.005 **[V]**); *Talking Synth (Alien)* is almost silent (peak 0.003 on three keys **[V]**) **[B: Talking Synth patches may want a controller]**; and **Ghost Glass Harmonium (two copies, in Keys and in Pads) is silent on keys 36, 60 and 84 for 8 seconds, and why is not known**.
* **Loud ones:** eight patches reach a peak of 4.5 or more in the merged results, four of them exactly 8.0 (which looks like a ceiling). The first full run had three factory ones (*Banjo Remains* 4.7, *Cottage* 7.4, *Moody Statement* 8.0) that were lower in a later run: a patch's level varies from run to run **[B: random starting phases]**. The test of a single note says `peak < 8.0`, which is not a promise that patches keep.

**A crash [V, measured]: Surge XT dies, at random, when a patch is loaded as state and notes are played.** The process ends with an access violation **inside `Surge XT.clap`** (a read of a heap address; the two crashes that were looked at are in the same routine, `+0x479ccd` and `+0x479ee8`), or it hangs in the block that plays. The rates:

| What was played | Crashes |
|---|---|
| the default patch, no state (many hundreds in a row, and 2,404 in one run) | none, in more than 4,000 plays |
| *Init Saw* given as state | none in 1,500 |
| any patch as state, **no notes** | none in 1,500 |
| any patch as state, **with notes** | *Quiz* died within 70 to 270 plays in 5 of 6 runs (and in 3 of 3 with Surge's own saved state); *Toms 1* within 40 plays in 5 of 10 runs; *Bass Sawari* in 2 of 2; **every full sweep died, after 79 to 1,857 patches** |

**What did not remove it** (each tried): waiting 100 ms between making the plugin and playing; **Surge's own saved state** instead of the chunk; **one silent block, or twenty, before the first note** (it lowered the rate, the sweeps reaching 1,857 and 1,329 patches, and did not end it); **giving the state before the plugin is activated** instead of after (79 patches; taken out again, since it changed nothing); **keeping the plugins alive** instead of letting them go (it died either way); a **new library for each patch** (died either way). **Not the restart path** of the host (a debug line showed no restart was asked, with or without a state).

**Not known: whether the fault is Surge's or the host's.** Nothing in the code of the host points at it, and Surge is made to be run many times in a program; but the crash needs the patch's state and a note together, which is how the program plays a clip.

**Why it matters [B, not tried in the program]:** the program makes a plugin with its saved state at each start of play and each seek, and a note in its first block (or one held across the seek) is what the sweeps did. If it happens there, **it takes the whole editor down**, since the sound plugins run in the editor's process. A robust answer is to run instruments (or all sound plugins) in a process of their own, as the scan and the windows already are: **a decision for the maintainer**, and not a small piece of work.

**A mistake of mine on the way, so that it is not repeated:** the first sweeps made a new plugin host for each patch; a library that is loaded and let go **546 times cannot be loaded again** on this machine (`LoadLibraryExW failed`), which looked like "every patch after the 546th fails" and is the test's own way of using it, not the program's (it keeps one host). Later runs used one host, as the program does.

## What was checked

* **[V] The files, on Windows and in Ubuntu (WSL):** the real script `tools/bundled_plugins/fetch_bundled_plugins.py` made the folder for the five sets from the real release files (every checksum right), on both systems.
* **[V] The sound, on Windows and in Ubuntu:** `tests/bundled_synthesizers_real.rs` (ignored; run by hand on that folder) scans the folder, **finds every synthesizer of the manifest as one**, plays middle C held for one second through the plugin host as an effect, and says what came out: every sample finite, a peak between 0.07 and 0.97, sound while the note is held, silence (below a tenth of that) three seconds after it ends. All five (Surge XT as CLAP; Kars and Nekobi as CLAP and as LV2) pass on both systems. **Kars and Nekobi sound a little different as CLAP and as LV2** (Windows: Kars's loudness while held is 0.049 as CLAP and 0.092 as LV2, Nekobi's peak 0.072 and 0.118; Kars starts each note with a burst of noise, so no two runs of it are the same): nobody checked why.
* **[V] A MIDI clip played by each of them through the whole engine, on Windows and in Ubuntu:** the second test of `tests/bundled_synthesizers_real.rs` puts the real synthesizer among the effects of a MIDI clip in a project (three notes: 0.1 to 0.4 s, 0.5 to 1.5 s, 2.0 to 2.2 s) and makes the sound with the plan, the graph and the plugin host. For every one of the five: **silence before the first note, and the first sound at the sample of the note** (4800 for Kars, 4806 for Nekobi, 4815 to 4865 for Surge XT, whose sound builds up over about a millisecond), sound in each of the three notes, **silence after the last note's tail**, and **after a seek to 1 s, in the middle of the second note, the held note is heard at once**, with silence before the third. No sample is compared with a number, as it is for the test instruments (a real plugin's sound is not exact arithmetic).
* **[V] The window of Surge XT:** the program that shows a plugin's window opened it (1141 by 711), and **a photograph of the real window** shows the whole interface: patch browser ("Init", by the Surge Synth Team, CC0), oscillators, filters, envelopes, effects, modulation. Nobody played it with a mouse.
* **[V] CI:** run 161 (`72c8880`, which has `0261271` under it) passed on all five jobs: macOS, Windows, Ubuntu, the Python tools, and the licences.
* **[V] The manifest:** 15 sets, checked by the policy (no AGPL, no GPL-2.0-only, https, checksums); the tests of the manifest, of the tab and of the script pass.
* **[V by the CI, on a Mac (macOS 26, arm64), run 161, commit `72c8880`]:** the members of the five sets on macOS were written by the pattern of MVerb (`dpf-DPF-Plugins-clapbundles.pkg/Payload/Kars.clap`, `...lv2bundles.pkg/Payload/Kars.lv2`, and `Surge XT.clap/` in Surge's zip), and **the real fetch of all 15 sets by the program's own downloader passed on the Mac (94 s), the scan of what it fetched passed (3 tests), and the test of the synthesizers passed (2 tests: a note through the host, and a MIDI clip through the whole engine, for all five)**. Nobody has heard them or seen their windows on a Mac. (The Intel half of the universal bundles is not run by that runner.)
* **Not checked:** listening; the windows of Kars (it has none) and Nekobi with a mouse; Surge XT with a patch other than Init **by ear** (it was played, and measured; see *Playing the patches*, with the crash); the plugins in a real project through the editor (what is proven is the host, the scan and the sound of one note).

## Limits you should know about

* **Surge XT's patches and wavetables are a separate download** (the section *Data for plugins* of the tab, 252.8 MB), **and are not there for macOS** (see above). Without them a person gets the Init patch (a sawtooth) and the oscillators that need no file, and the patch browser lists only *Init* [V].
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
