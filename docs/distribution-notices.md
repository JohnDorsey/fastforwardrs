<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# What has to go with a copy of the program

Written 2026-10-15 (phase 0.2 of `plugin-system-plan-of-action.md`; features list LG-02, LG-03, LG-07). Nothing is distributed yet: this is what a release must
include, so that it is not found out on the day.

## Licence files (in the repository now)

| File | What it is |
|---|---|
| `LICENSE` | the GNU General Public License, version 3 |
| `LICENSE-PLUGIN-CLARIFICATION` | the copyright holder's clarification about plugins (adopted 2026-10-15) |
| `LICENSES/` | the texts of the licences named in the SPDX lines (`reuse lint` checks that every file has one) |
| `THIRD_PARTY.md` | the Rust libraries and their licences, made by `cargo about generate about.hbs | tr -d '\r' > THIRD_PARTY.md`; CI fails if it is out of date |
| `deny.toml` | which licences, sources and advisories are accepted (`cargo deny check`; CI) |
| `assets/fonts/Hack-Regular-license.txt` | the licence of the font the program draws with (Hack: MIT; Bitstream Vera) |

## Content that ships with the program (the manifest of LG-02)

| What | Where it is from | Licence |
|---|---|---|
| `assets/fonts/Hack-Regular.ttf` | the Hack project (Source Foundry), with Bitstream Vera and DejaVu glyphs | MIT, and the Bitstream Vera licence (its licence file is next to it) |
| the default fonts of egui (`epaint_default_fonts`, inside the program) | the egui project | the crate says `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`; the fonts are data that is only embedded and drawn, not code that is combined with GPL code, and their notices are in `THIRD_PARTY.md` (LG-07 asked for this check; **verdict: acceptable as data, with the notices; to be looked at again if a release ships the font files separately**) |

No shader, LUT, sound or plugin ships yet. When some do, each is added here with its author, licence and URL, and only with a licence of the kinds LG-02 allows.

**Amended 2026-10-20 (the maintainer's decision):** LG-02 now also allows **GPL and LGPL licences that are compatible with GPLv3** (GPL-3.0, GPL-3.0-or-later, GPL-2.0-or-later,
LGPL-3.0, LGPL-2.1-or-later) for what ships, **plugin binaries included**, on the conditions of the ffmpeg section below: the licence text next to it, the copyright notices
kept, nothing altered or renamed, and the **corresponding source of that exact version** offered with the release, with a line in the manifest (name, author, licence, URL,
version, where the source is kept). **Not GPL-2.0-only** (not compatible with GPLv3), and **not AGPL** (the maintainer's decision). The candidates found are in
`bundleable-clap-plugins.md`.

## ffmpeg (LG-03)

The program runs `ffmpeg` and `ffprobe` as separate programs. A release that **ships** them must:

1. ship a **GPL** build (`--enable-gpl --enable-version3`, never `--enable-nonfree`), which is licensed GPLv3 or later (the build used for development on this machine: gyan.dev's 9.0.2, and
   on Linux BtbN's n9.0.2, both verified to be GPL builds);
2. include the licence text that comes with that build, and the build's `--version` output (which lists how it was configured);
3. **offer the corresponding source** of that exact build (for a build made by someone else, a link to the same revision's source on their site is not enough on its own: GPLv3
   section 6 asks for a written offer valid for three years, or the source itself, from whoever conveys the binary), and say where it is hosted.

*The place and the wording of that offer are decided when the first release is made.* The program requires ffmpeg **9 or newer** (README, *ffmpeg*).

## The plugin clarification

See `LICENSE-PLUGIN-CLARIFICATION`. A **release that includes someone else's plugin** (none does) needs that plugin's own licence file, in addition.

## Fetching the plugins from the program, and shipping them (2026-10-21)

The *Bundles* tab of the sound plugins window downloads the plugins of the manifest for a person who asks. **The legal reading of the difference between that and shipping them in a release [B: a reading, not legal advice]: there is no difference that makes it worse, and one that makes it simpler.** What a release that *contains* a GPL or LGPL plugin conveys is the plugin, and so owes its source; when the program fetches the file from its authors' own release page, **the authors convey it** and their offer of source is the one that applies, and what the program conveys is only the manifest (names, addresses, checksums). What stays the same either way: the plugin is unaltered (the checksum says so), it is used as a separate work (`LICENSE-PLUGIN-CLARIFICATION`), the licence of each is in the manifest and **is shown on the tab before the download and written beside the files after it, with a `NOTICE.txt` that says who wrote the set and where its source is**, and the policy of what may be offered is the same (no AGPL, no GPL-2.0-only). What is different: it is a network connection the person must agree to (the tab asks first and says which sites), the files are only there while their authors keep them at those addresses (the checksums refuse a changed file), and a release that wants to **work without a network** still has to ship the files, which is what the script and the section below are for.

## Plugins that ship with the program (2026-10-20)

`bundled-plugins/manifest.json` is the manifest (name, authors, licence, home page, version, the release file of each system with its checksum, the licence texts with their checksums, and where the source is). **It is checked by CI against the licence policy above** (`tools/bundled_plugins/fetch_bundled_plugins.py --check`: no AGPL, no GPL-2.0-only, every address https, a checksum for every file). Nothing is distributed yet; what a release does is:

1. `py tools/bundled_plugins/fetch_bundled_plugins.py --platform <system> --out <the release folder>/plugins --with-source` for each system it is made for (`windows-x86_64`, `linux-x86_64` and `macos-universal` have builds; the macOS ones are bundles, taken as they are from their authors' zip or installer: unaltered, universal (x86_64 and arm64), ad-hoc signed by their authors, and downloaded with `curl`, which does not mark a file as quarantined), which puts in `plugins/<set>/` only the `.clap` files named, **the licence texts, a `NOTICE.txt`, and the source archive** of that exact version (the corresponding source that GPLv3 section 6 and LGPLv3 owe, as for ffmpeg above);
2. keeps the source archives where the release is hosted too (the archive of the tag that GitHub makes for DPF-Plugins has no stable checksum: **a release keeps its own copy**, and the checksum of that copy is then put in the manifest);
3. `plugins/README.txt` says what is there.

| Set | Authors | Licence | Plugins | Source |
|---|---|---|---|---|
| Dragonfly Reverb 3.2.10 | Michael Willis, Rob van den Berg | GPL-3.0-or-later (the Freeverb parts it carries are GPL-2.0-or-later) | Hall, Plate and Room reverbs | `dragonfly-reverb-3.2.10-src.tar.xz` of its release |
| MVerb (DPF-Plugins v1.7) | Martin Eastwood, Filipe Coelho | GPL-3.0-or-later | MVerb | the tag `v1.7` of DISTRHO/DPF-Plugins |
| MaGigaverb (DPF-Plugins v1.7) | Cycling '74 (the Gen~ export), Filipe Coelho | LGPL-3.0-only AND ISC (the wrapper) | MaGigaverb | the tag `v1.7` of DISTRHO/DPF-Plugins |
| Surge XT Effects 1.3.4 | the Surge Synth Team and the authors of Surge | GPL-3.0-or-later | Surge XT Effects | `surge-src-1.3.4.tar.gz` of its release (md5 published by the project) |

**Left out on purpose:** Airwindows Consolidated (the maintainer's decision for now), the AGPL plugins (ZL), Nekobi (GPL-2.0-only), and the three plugins of DPF-Plugins v1.7 that share MVerb's CLAP id (3BandEQ, 3BandSplitter, PingPongPan).

**Trust:** a folder called `plugins` beside the program, or the one that `FFRS_BUNDLED_PLUGINS` names, is looked in last and **trusted without the question** (in the editor and in `fastforwardrs-export`): its files are the release's own and are as trusted as the program. Where the licence texts could not be found in a release archive, they are taken from the project's repository at the tag that matches the version (the manifest says which, with a checksum). Where **the licence of a set is not the one its authors state in one place** (MaGigaverb: the DPF licence table says LGPL-3.0, the file of its wrapper says ISC), the manifest says which it took and why.
