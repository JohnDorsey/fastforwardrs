<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# LV2 plugins that the Bundles tab fetches (2026-10-27)

The maintainer asked (2026-10-27): "add LV2 plugins that we can legally bundle to the plugin manager bundle downloader. Then we can try those and you can work on presets." This is what was
added, what was left out and why, and what was tried. **[V]** = I ran it or read it myself today; **[B]** = I believe it from reading.

*I am not a lawyer; this is a reading of the licences by someone who has to ship the result, not legal advice.* The policy is the one of `distribution-notices.md` (LG-02 as amended on
2026-10-20): GPL and LGPL licences compatible with GPLv3 and the permissive ones; not AGPL, not GPL-2.0-only.

## What was added: six sets (five of them for macOS too)

The authors of Dragonfly Reverb, DPF-Plugins and Wolf Shaper ship an **LV2 bundle (a folder) beside the CLAP file in the same release file** that the CLAP sets already fetch, so the
Dragonfly and DPF sets cost **no extra download** (the Bundles tab downloads a release file once however many sets are in it). The only new file is Wolf Shaper (5 MB). Totals for the whole
tab: **Windows 151.5 MB (was 146), Linux 114.5 MB (was 109), macOS 269.8 MB (unchanged: the LV2 bundles are in the disk images that the CLAP sets fetch).**

| Set (id) | Licence | Plugins | From |
|---|---|---|---|
| Dragonfly Reverb (LV2) (`dragonfly-reverb-lv2`) | GPL-3.0-or-later | Early Reflections, Hall, Plate, Room | the files of `dragonfly-reverb` (the same win64 zip and linux tar.xz) |
| MVerb (LV2) (`mverb-lv2`) | GPL-3.0-or-later | MVerb | the DPF-Plugins v1.7 files |
| DPF Mini-Series (LV2) (`dpf-mini-series-lv2`) | LGPL-3.0-only | 3BandEQ, PingPongPan | the DPF-Plugins v1.7 files |
| DPF Max-Gen plugins (LV2) (`dpf-max-gen-lv2`) | LGPL-3.0-only AND ISC (the wrapper) | MaBitcrush, MaFreeverb, MaGigaverb, MaPitchshift | the DPF-Plugins v1.7 files |
| CycleShifter (LV2) (`cycleshifter-lv2`) | MIT | CycleShifter | the DPF-Plugins v1.7 files |
| Wolf Shaper (LV2) (`wolf-shaper-lv2`) | GPL-3.0-or-later (the SPDX line of its source says "GPL v3+") | Wolf Shaper | `wolf-shaper-v1.0.2+20230515144200-windows-x64.zip` and `wolf-shaper-v1.0.2-linux-x64.zip`; source `wolf-shaper-v1.0.2-source.tar.gz` |

**Why the LV2 plugins are sets of their own and not members of the CLAP sets:** the same plugin as CLAP and as LV2 would be two effects with one name in the effect menu; a person
who fetches the CLAP set should not get both, and the LV2 sets are the way to try the LV2 host with real plugins.
**Why DPF plugins that were left out of the CLAP sets are here:** three DPF plugins share one CLAP id (`studio.kx.distrho.MVerb`), which is why only MVerb is a CLAP set; the LV2 URI of each
DPF plugin is its own, so the clash does not exist here.

### Left out, with the reason

* **3BandSplitter** (2 audio inputs, 6 audio outputs) and **AmplitudeImposer** (4 inputs, 2 outputs): scanned well, but **no effect of the program has those shapes**, so the scan lists them
  as "not used". They were in the first version of the manifest and are out of it. [V]
* **Zam Plugins 4.5** (compressors, gates, EQs, delay, reverb; LV2 bundles in the win64 zip, which is **128 MB**, and in the Linux and macOS files): the repository's `COPYING` is the GPL
  version 2 and I **did not check the headers of each plugin for "or any later version"** (ZamVerb and ZamHeadX2 carry other people's code and data). With the *Download all* button the 128 MB
  would also be fetched by everyone. Not added; it would need a per-set choice in the tab or a licence check of every file. [B for its licence]
* **x42 plugins** (GPL-2.0-or-later, **[B]**): the Windows files are installers (`.exe`) that the program cannot open, and I found no zip.
* **LSP Plugins 1.2.35**: LV2 bundle for Linux only on GitHub (see `bundleable-clap-plugins.md`); Windows builds are behind a registration. **[B]**
* **Surge XT** has no LV2 in its Windows zip (`Surge XT Effects.clap`, `.vst3`, `.exe`) [V]; **Airwindows** by the maintainer's decision; **ZL** by licence (AGPL). **(Nekobi was left out here on a wrong reading of its licence, "GPL-2.0-only"; its source files say "or any later version". It and Kars, two synthesizers, were added on 2026-10-29: see `bundleable-synthesizers.md`.)**

## What was checked

* **The download.** The real fetch from the authors' addresses on Windows **[V]**: all ten sets, 151.5 MB in 87 s, every checksum right, every LV2 bundle a folder with its `manifest.ttl`.
  The Linux files were fetched by the script in Ubuntu (WSL) **[V]**: see the section on Linux below for what ran.
* **The scan [V on Windows].** All 13 LV2 effects (and the 6 CLAP effects) scan with **no problem in their Turtle** and load: `tests/bundled_plugins_real.rs` (ignored, run by hand on a fetched
  folder). **This is the first time the LV2 host met plugins that nobody wrote for it**: the Turtle reader (`oxttl`), the binaries on Windows, the ports and the features all passed.
* **The sound [V on Windows].** `every_real_lv2_plugin_makes_finite_sound_that_is_not_silent_and_is_changed_by_it` runs each effect for 200 blocks of 512 frames at 48 kHz with two tones and
  noise, every control at its default: every sample finite, peaks 0.03 to 0.8 for an input of 0.3, no silence, and **every effect but the flat EQ changes the sound** (its defaults make it
  an identity: 3 Band EQ's change is exactly 0, and Wolf Shaper's defaults draw a straight line, change 0.0005). Wolf Shaper's one control output (its input meter, port 16) read 0.274 for a
  signal whose peak is 0.275: **the meters of the last piece see a real plugin's meter**. Nothing reported a latency (all 0).
* **The interfaces [V on Windows].** `the_interfaces_of_the_real_bundled_plugins_open_without_a_window_and_close` opens the 8 interfaces that can be shown (Cycle Shifter, 3 Band EQ,
  Ping Pong Pan, the four Dragonfly plugins, MVerb) in the editor-window program without a window, and each says it is ready with a size (Dragonfly Hall 920 x 345) and ends well. A real
  window of **Dragonfly Hall** (`look_at_the_window_of_a_real_plugin`) was opened on this screen and photographed: the interface is drawn, with its knobs, its list of rooms and its
  spectrogram, in a window of 936 x 384. **No one has used one with the mouse yet [B that they respond]; no one has listened to any of them in the program.**
* **Wolf Shaper has no window here, and it is not our fault [V]:** its Windows build says its interface is `ui:X11UI` (the class of X11), so there is no interface of the Windows class and
  the program offers only the panel of controls. A host on Windows would do the same.

## How the program finds them

A bundle is a folder, so a member of the manifest that ends in `.lv2` is a **folder of the archive** (a name that ends in `/`: out of a zip or a tar it is taken with everything in it; out
of the installer of a macOS disk image it is a path in what the installer holds), and the scan now looks for bundles **in the folders of a folder that is looked in, down to six folders**,
as it does for CLAP libraries (the Bundles tab puts a set in a folder of its own; a bundle is never looked into, nor a `.clap` bundle, nor a folder that a download is making).

## macOS

The CLAP builds for macOS come out of the installers of the disk images (`dpf-<name>-clapbundles.pkg/Payload/...`). **The LV2 bundles are in a sibling installer,
`dpf-<name>-lv2bundles.pkg/Payload/<Plugin>.lv2`, in the same two disk images (Dragonfly Reverb and DPF-Plugins) [V: a probe step of the CI on a Mac listed them, run 135]**, so five of the six
sets have a macOS build, from files that are fetched anyway. **Wolf Shaper has none**: its macOS file is a zip that holds a disk image, which is one more layer than the program opens.
The probe step is gone from the CI; what the next CI on a Mac does with them (the real fetch, the scan, the sound of each plugin, every preset) says whether they work there: **until it is
read, they are [B] on macOS**, and nobody has heard or seen them on a Mac.
