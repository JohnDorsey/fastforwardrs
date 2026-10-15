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

No shader, LUT or sound ships yet. When some do, each is added here with its author, licence and URL, and only with a licence of the kinds LG-02 allows.

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
