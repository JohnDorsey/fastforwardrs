<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# VST3 plugins that the Bundles tab fetches (2026-11-03)

Piece 15.5 of `engine-phase-15-vst3-design.md` (the maintainer owns no VST3 plugin, D6: "let's rely on our own test plugins and GPL3-compatible-bundlable ones you can find online").
This is what was added, what was left out and why, and what was tried. **[V]** = I ran it or read it myself today; **[B]** = I believe it from reading.

*I am not a lawyer; this is a reading of the licences by someone who has to ship the result, not legal advice.* The policy is the one of `distribution-notices.md` (LG-02 as amended on
2026-10-20): GPL and LGPL licences compatible with GPLv3 and the permissive ones; not AGPL, not GPL-2.0-only.

## What was added: seven sets, and no new download at all

The authors of Dragonfly Reverb, DPF-Plugins and Surge XT put **a `.vst3` module (a folder) beside the CLAP file in the same release file** that the CLAP sets already fetch, so the Bundles
tab downloads **exactly what it did: 404,310,339 bytes for Windows with the data of Surge XT (151.5 MB without it), counted once for each release file** [V: the real fetch below].

| Set (id) | Licence | Plugins | From |
|---|---|---|---|
| Dragonfly Reverb (VST3) (`dragonfly-reverb-vst3`) | GPL-3.0-or-later | Early Reflections, Hall, Plate, Room | the files of `dragonfly-reverb` |
| MVerb (VST3) (`mverb-vst3`) | GPL-3.0-or-later | MVerb | the DPF-Plugins v1.7 files |
| MaGigaverb (VST3) (`magigaverb-vst3`) | LGPL-3.0-only AND ISC (the wrapper) | MaGigaverb | the DPF-Plugins v1.7 files |
| Surge XT Effects (VST3) (`surge-xt-effects-vst3`) | GPL-3.0-or-later | Surge XT Effects | the files of `surge-xt-effects` |
| Surge XT (synthesizer, VST3) (`surge-xt-synthesizer-vst3`) | GPL-3.0-or-later | Surge XT | the same |
| Kars (synthesizer, VST3) (`kars-vst3`) | ISC | Kars | the DPF-Plugins v1.7 files |
| Nekobi (synthesizer, VST3) (`nekobi-vst3`) | GPL-2.0-or-later (read from its source files: see `bundleable-synthesizers.md`) | Nekobi | the DPF-Plugins v1.7 files |

They are **the VST3 sets of the plugins that have a CLAP set**: everything but the plugins and the names of the files is the CLAP set's (the authors, the licence, the licence texts, the
source, the release files and their checksums), and a test of the manifest says so (`test_the_vst3_sets_are_these...`). Each is a set of its own for the reason the LV2 ones are: the same
plugin as CLAP and as VST3 would be two effects with one name in the menu, and a person who fetches the CLAP set should not get both. **Surge XT is therefore in the menu twice, under CLAP
and under VST3**, and so is each Dragonfly reverb (and the VST3 build has the 774 values of Surge XT, all of them since the limit of 255 went: see section 11 of the design document).

**What a set of this kind is made of.** A member is `"from": "dragonfly-reverb-3.2.10/DragonflyHallReverb.vst3/"` (a folder of a zip or a tar: the `/` at the end is what takes the folder
and not one file), or, for the disk images of macOS, a path in what the installer holds. The manifest and `fetch_bundled_plugins.py` accept `.vst3` as they accept `.lv2`.

## What was read about the licence of a VST3 build (it is not the licence of the plugin alone)

A VST3 file contains the interface of VST3, and that interface has a licence of its own. Steinberg's SDK is "Steinberg VST3 License **or** GPL-3.0" (since 2017) and, from 3.8 (2022),
MIT [B for the date]; what each of these builds carries:

* **DPF-based plugins (Dragonfly, MVerb, MaGigaverb, Kars, Nekobi): DPF's own header files, "travesty" [V]**: a pure C description of the interface of VST3 written by Filipe Coelho,
  **ISC**, with its notice in every header ("Copyright (C) 2021-2022 Filipe Coelho", `dpf/distrho/src/travesty/base.h`). I read it **in the tag `v1.7` of DPF-Plugins and in the DPF commit
  `dc6557a` that Dragonfly Reverb 3.2.10 holds**; the DPF-Plugins licence table lists DPF as ISC. **The VST3 file therefore has the licence of the plugin and nothing more.** The binaries were
  not taken apart [B that they were built from these headers; DPF has no other way to build a VST3 plugin that I know of].
* **Surge XT: the SDK that its JUCE carries [V]**: the Surge team's JUCE (`surge-synthesizer/JUCE`, commit `cf5754b`, the one that `release_xt_1.3.4` pins) has Steinberg's SDK in
  `modules/juce_audio_processors/format_types/VST3_SDK`, whose `LICENSE.txt` says "licensed under the terms of the Steinberg VST3 License, **or alternatively under the terms of the General
  Public License (GPL) Version 3**. You may use the Software Development Kit according to either of these licenses". Surge XT is GPL-3.0-or-later, so the second applies. The JUCE of Surge
  was already in the CLAP build (see `bundleable-synthesizers.md`), so the VST3 build adds the SDK and nothing else. Not taken apart [B].
* **Said where a person reads it**: each set's `licence_note` in the manifest says which of these it is.
* **The name.** "VST" is a trademark of Steinberg Media Technologies GmbH. The program writes *VST3* in its menus and in its documents as the name of a format, and has no logo of it [B that
  this needs no licence; not a lawyer].

## Left out, with the reason

* **The other VST3 modules of the same release files**: 3BandEQ, PingPongPan, MaBitcrush, MaFreeverb, MaPitchshift and CycleShifter (their LV2 sets exist; I listed the DPF-Plugins files and they are in them). Nothing is wrong with
  them that I know of; they were not added **to keep the menu from having a second copy of every plugin** until a person says they want the format for them. Each is one line in the
  manifest. [B that their VST3 builds work as these do] **Wolf Shaper**: I did not look whether its release has a VST3 build.
* **3BandSplitter, AmplitudeImposer, SoulForce, glBars, ProM**: as for CLAP and LV2, the shapes of their buses are not used by any effect of the program (ProM and glBars are pictures, and ProM
  is 4,246 files in its module).
* **A VST3 build of the Surge XT patches**: there is none; the data of Surge XT is found by the VST3 build too, since Surge looks for `SurgeXTData` in the folders above its own file [B: the
  VST3 module is two folders deeper than the CLAP file, and the search goes up through every folder].

## What was checked

* **The manifest.** 23 sets, the Rust and the Python checks both fit; `.vst3` is accepted as a folder member (and `Thing.vst3` without the `/` is refused with the reason), tests of both.
* **The download [V].** The program's own downloader fetched all 23 sets on **Windows (404,310,339 bytes in 97.9 s) and on Linux in Ubuntu (95.6 s)**: every checksum right, every VST3
  module a folder with `Contents` in it. Before fetching, the release files were listed to see that the members are there: the Windows zips of Dragonfly Reverb, DPF-Plugins and Surge XT
  and the Linux files of Dragonfly Reverb, DPF-Plugins and Surge XT all hold the `.vst3` folders under the names the manifest takes [V].
* **The scan [V on Windows and Linux].** `every_plugin_of_the_manifest_is_found_loads_can_be_an_effect_and_starts_steady` (ignored, by hand on a fetched folder) now has the VST3 modules: each
  is found, scanned, has no problem. The effects found are as many as the files.
* **The sound of the effects [V on Windows and Linux].** `every_real_vst3_effect_makes_finite_sound_that_is_not_silent_and_changes_the_sound_it_is_given` runs each VST3 effect for 200
  blocks of 512 frames with two tones and noise, every value as the effect gives it at its first block:

  | Effect | values | peak | change |
  |---|---:|---:|---:|
  | Dragonfly Early Reflections | 9 | 0.26 | 0.079 |
  | Dragonfly Hall Reverb | 20 | 0.31 | 0.078 |
  | Dragonfly Plate Reverb | 11 | 0.24 | 0.038 |
  | Dragonfly Room Reverb | 19 | 0.26 | 0.034 |
  | MVerb | 12 | 0.30 | 0.103 |
  | MaGigaverb | 8 | 0.80 | 0.186 |
  | Surge XT Effects | 14 | 0.27 | 0.000 (no effect is in its slots until a patch puts one there [B]) |

  (The values counted here are those of the module, with the hidden ones; the effect has the ones that a person can use, as the scan says.)
* **The sound of the instruments [V on Windows and Linux].** `bundled_synthesizers_real` finds the three VST3 synthesizers by the word in the name of their sets, and each plays a note and is
  quiet after it, and plays a MIDI clip through the plan, the graph and the host **where its notes are, silent before the first, with its tail, and a note held across a seek**. On Windows
  **Kars as VST3 is, to the five digits that are printed, the Kars of CLAP and of LV2** (first sound at sample 4800, the levels of the three notes 0.07516, 0.03018, 0.17071); Surge XT and Nekobi
  sound as their CLAP builds do and are not the same bit for bit (Surge starts its note a few samples earlier or later, and Nekobi has voices that are not the same each time).
* **Through the program that runs plugins.** The VST3 plugins above ran in the plugin program too, in the host test of piece 15.3 with Dragonfly and Surge XT; the Bundles tab only puts the
  files where the scan finds them, and the folder is the program's own, trusted without the question.

## Not done, said plainly

* **macOS.** The members of the sets for macOS follow the names of the CLAP and the LV2 ones (`dpf-DPF-Plugins-vst3bundles.pkg/Payload/Kars.vst3`, `Surge XT.vst3/` in the zip): **[B]**, I
  could not open a disk image here. The CI job of a Mac fetches every set with the program's own downloader and scans what it fetched (its failures are warnings): see the end of this
  document for what it said.
* **Nobody has listened to them in the editor, and no window of them is shown** (piece 15.6).
* **A reverb's tail after a clip** with these plugins (Dragonfly's rings on after the sound stops) was not tried through a project; the code that plays a tail is the same for every format.
