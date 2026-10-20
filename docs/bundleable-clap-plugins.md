<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# CLAP plugins that could be bundled with the program (a search, 2026-10-20)

You asked (2026-10-20, after Aura Lite): "find me some CLAP plugins that can be legally bundled with this project now". This is the search. **[V]** = I checked it
myself today, on the project's own pages or release listings (GitHub's API; the listing of the files inside a zip, read from its central directory without downloading the
zip). **[B]** = I believe it from reading, and did not check it. **Nothing here was downloaded or run in our host yet**, so *"does it load and sound right in our host"* is
untested for every plugin below.

*I am not a lawyer; this is a reading of the licences by someone who has to ship the result, not legal advice.*

## What the program's own rules say today

* `LG-02` (plugin-system-features.md): *content that ships with the program* (shaders, tables) may be **MIT, BSD, ISC, Apache-2.0, CC0, Zlib or CC BY**, with a manifest. It was
  written for data files, and says nothing about **binaries of someone else's plugin**.
* `LG-03`: ffmpeg binaries may be shipped if they are a GPL build and **the corresponding source is offered** (GPLv3 section 6).
* `LICENSE-PLUGIN-CLARIFICATION`: a plugin that the program loads at run time through a published interface (CLAP is one) is a separate work and need not be GPL. A release
  that includes someone else's plugin needs **that plugin's own licence file** in addition (`distribution-notices.md`).

So there are two tiers, and **the second one needs a decision from you** (below): **A**, permissive plugins, fit `LG-02` as it is written; **B**, copyleft plugins (GPL, LGPL,
AGPL) are allowed to be shipped by their licences (the program is GPLv3 and the plugin is loaded as a separate work), but each release then **owes their corresponding source
and licence text** exactly as it does for ffmpeg.

## The candidates

| Plugin | What it is | Licence **[V]** | Windows CLAP in a release? | Tier |
|---|---|---|---|---|
| **Dragonfly Reverb 3.2.10** (michaelwillis/dragonfly-reverb) | Hall, Room, Plate reverbs and an early-reflections plugin | GPL-3.0 (its README; Freeverb parts GPL-2+ per Debian's file, **[B]**) | **[V]** `dragonfly-reverb-3.2.10-win64.zip` (21.9 MB) has `DragonflyHallReverb.clap`, `DragonflyRoomReverb.clap`, `DragonflyPlateReverb.clap`, `DragonflyEarlyReflections.clap`; the release has the source tarball | B |
| **DPF-Plugins v1.7** (DISTRHO) | a pack: MVerb (reverb), MaFreeverb, MaGigaverb (reverbs), MaPitchshift, MaBitcrush, 3BandEQ, 3BandSplitter, PingPongPan, AmplitudeImposer, CycleShifter, and synths and a visualiser | **per plugin, from the pack's LICENSE:** MVerb GPL-3.0; MaBitcrush, MaFreeverb, MaGigaverb, MaPitchshift, 3BandEQ, 3BandSplitter, PingPongPan LGPL-3.0; **AmplitudeImposer, CycleShifter, SoulForce MIT**; Kars ISC; **Nekobi GPL-2.0 only (do not ship: not compatible with GPLv3)**; ProM LGPL-3.0; glBars GPL-3.0 | **[V]** `DPF-Plugins-v1.7-win64.zip` (72.8 MB, mostly ProM's presets) has `.clap` for 3BandEQ, 3BandSplitter, AmplitudeImposer, CycleShifter, MVerb, MaBitcrush, MaFreeverb, MaGigaverb, MaPitchshift, PingPongPan (and Kars, Nekobi, ProM); the listing I read stopped at 25 names, so SoulForce and glBars are **[B]**. No source tarball among the release files (the source is the git tag) | A for the two MIT ones, B for the rest |
| **Surge XT 1.3.4** effects (surge-synthesizer/releases-xt) | `Surge XT Effects.clap`: chorus, phaser, flanger, reverbs, distortion, EQ and more in one plugin (12 slots) | GPL-3.0 | **[V]** `surge-xt-win64-1.3.4-pluginsonly.zip` (47.9 MB) has `Surge XT Effects.clap` and `Surge XT.clap` (a synth: not an effect for us) | B |
| **Airwindows Consolidated** (baconpaul/airwin2rack) | **all** of Airwindows' 400+ effects as **one** plugin with an effect selector | the Airwindows code is MIT **[V]**; the plugin is built with JUCE and its README says that brings GPL-3 parts into the binary, so **treat the binary as GPL-3 [B]** | **[V]** `AirwindowsConsolidated-2026-10-17-d8fe301-Windows.zip` (13 MB) has `Airwindows Consolidated.clap` | B |
| **LSP Plugins 1.2.35** | a very large set (equalisers, compressors, gates, reverbs, delays, ...) | LGPL-3.0 (GPL-3.0 files too) | **No Windows binary on GitHub**: the release has Linux and FreeBSD builds only (Linux x86_64: 37.9 MB). Windows builds are sold or registered at lsp-builds.com (terms **not read**), so we could not redistribute them without reading those; the alternative is to build from source (LGPL-3.0) | B (Linux now) |
| **ZL Equalizer 2, ZL Compressor, ZL Splitter** | an equaliser, a compressor, a splitter | **AGPL-3.0** | `.msi` installers (**contents not listed; CLAP not confirmed [B]**) | B+ (AGPL; I read GPLv3 section 13 as allowing the combination, **[B]**, and would leave it for last) |
| **free-audio/clap-plugins** | the CLAP project's own example plugins | MIT | no releases: build from source | A |

Left out, and why **[B]** (not checked one by one): freeware such as Valhalla, TAL and u-he's free plugins is free to download but, as far as I know, not free to redistribute;
Vital, Calf and the x42 plugins I did not search for CLAP builds (Calf and x42 are known to me as LV2); Cardinal's release is a 1 GB modular host.

## How they would fit our host **[B] for all of this**

* **Reverbs with tails** (Dragonfly x3, MVerb, MaGigaverb, MaFreeverb): the same kind as Aura Lite, so they are the right test of *"is the roar and the warble Aura's or ours"*: if
  one of them does the same, it is us; if none does, it is Aura.
* **Latency**: MaPitchshift (a pitch shifter) is the likely one to report latency, which tests the latency compensation in the graph.
* **Plugins whose parameter list changes with a choice** (Airwindows Consolidated: the selector changes what the parameters are; Surge XT Effects: the labels of its parameters change
  with each slot's effect) are the **worst fit** for a host that reads a plugin's parameters when it scans: expect them to load but to be awkward. I would try them last.
* **Several plugins in one library** (Dragonfly, DPF): each file is its own library holding one plugin, which is the easy case for the scan.

## What a release would owe for tier B (the same as LG-03 for ffmpeg)

1. the plugin's licence text next to it and its copyright notices kept;
2. the **corresponding source of that exact version** (the source tarball in the release where there is one, else a copy of the git tag) or a written offer valid for three years;
3. a line in `distribution-notices.md` and the manifest (name, author, licence, URL, version, where the source is kept);
4. nothing renamed or altered (unaltered binaries need no trademark worries, and **[B]** the Surge name is the Surge Synth Team's).

## What I would do first (needs your answers; see the report)

Download **Dragonfly Reverb 3.2.10 (win64)** and the **ten effects of DPF-Plugins v1.7**, scan them with our scanner, run them through the same measurements as Aura Lite, and listen.
Then Surge XT Effects, then the rest. For Linux and Ubuntu CI, the same plugins' Linux builds can be run in WSL.
