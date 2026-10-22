<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Phase 6, the rest: the plugin manager with shaders, the ISF bundles, transitions, the preview's lead-up (design)

Written 2026-10-22, from what the maintainer said on that day (below), [the design of phase 6](engine-phase-6-design.md), [the design of 6.4](engine-phase-6-4-design.md) (section 6 is the three ways to build EN-14), PH-16, LG-02,
UI-09, UI-11 and EN-14 of [the features list](plugin-system-features.md), and the code as it is. Tags: **[V]** verified in the code or by running something; **[D]** decided by the maintainer; **[P]** my proposal, which stands unless it
is objected to; **[B]** my belief.

## 0. What the maintainer said, 2026-10-22 [D]

> Add a "view" menu to the menu bar. Under it, have a checkbox item for the user to choose whether or not to use background catch-up (call the option "generate lead-up on seek"). If enabled, use option 1 from your list; if disabled, option 3.
> We do transitions by putting one clip on a track above another and applying an effect to the clip on top.
> I give you permission to download shaders. I'd like a bundle downloader for license-compatible ISFs. You can rename the sound plugins window to just "plugin manager" and include ISFs in it too alongside LADSPA and CLAP.

| piece | what | state |
|---|---|---|
| **6.4.3a** | the View menu, the checkbox *Generate lead-up on seek*, and the checkpoint interval and memory budget (global only) | built, commit `f2e68a6` |
| **6.7a** | the corpus test (`engine::isf_corpus_tests`), and what it found: the scaling of points | built, commit `da9d097` |
| **6.8** | **the plugin manager**: the window is renamed and finds ISF shaders in the folders, lists them (usable or why not), and offers the usable ones in the menus | section 1 |
| **6.9** | **the ISF bundles**: a manifest of licence-compatible shaders and a downloader for it, in the manager's *Bundles* tab | section 2 |
| **6.6** | **transitions**: a shader with `startImage`, `endImage` and `progress`, as an effect of the clip on top | section 3 |
| **6.4.3b** | the lead-up on a seek (option 1, the export's describer on a worker) and the checkpoints with the approximate marker (option 3) | section 4 |

## 1. 6.8: the plugin manager [P unless said]

### What there is [V, read 2026-10-22]

* *Edit > Sound plugins...* opens a window (`sound_plugins_window.rs`) with two tabs: *Plugins* (folders to look in, folders with libraries that plugins need, *Scan again*, what the scan found, the libraries that failed) and *Bundles*. The scan
  is a thread (`sound_plugins_flow.rs`) that runs the scan **program** on every `.so`/`.dll`/`.clap` of the folders and gives `LibraryReport`s; the plugins that can be effects become the installed effects (`set_installed_plugins`), and the menu of
  the + button offers them under the heading of their kind. Plugin libraries are **code**, so a folder is asked about once (`plugin_trust`); a folder that the program fetched into is trusted.
* A shader is **data**: `Add effect from file...` takes one, reads it, compiles it, and the project keeps its text (format 16). Nothing finds shaders by itself, and the menu of the + button lists a shader only after one was added (J4 of phase 6:
  "no folder is scanned and nothing ships in the box until you say so"). **The maintainer has now said so.**

### Decisions

| | | why |
|---|---|---|
| R1 | **The window is called *Plugin manager*, the menu item *Plugin manager...*.** The code's names follow (`PluginManager*`, the files `plugin_manager_*.rs`); the scan of sound plugins keeps its names (`sound_plugins_flow.rs`: it is about sound plugins). | [D] "rename ... to just plugin manager". A window that is about shaders too must not be called after sound in the code. |
| R2 | **One list of folders for everything**: the folders a person added, the usual places, the folder of the plugins that came with the program and the folders that the *Bundles* tab fetches into are looked in **for sound plugin libraries and for shaders** (`.fs` and `.isf` files, found below a folder to a depth of 4, at most 3000 files looked at in all). The usual places for shaders are those that ISF's own README names: macOS `/Library/Graphics/ISF` and `~/Library/Graphics/ISF`, Windows `%ProgramData%\ISF`; there is none on Linux (nothing is made up). | [D] "include ISFs in it ... alongside LADSPA and CLAP". One list is what a person expects of a manager. |
| R3 | **No question is asked about a shader's folder.** A shader is data (J3 of phase 6): the worst it can do is waste the card's time, which the engine's guard stops (EN-09). | UI-12: no prompt that need not be. |
| R4 | **The scan of shaders is in the process, on the thread that scans**, after the scan program has done the libraries (the shaders are read and compiled in the worker that already compiles them, with its time limit); a scan again does both. A shader that does not read or does not compile is **listed with the reason**, never left out silently. | The files are small (at most `MOST_BYTES_OF_A_SHADER`) and the compiler is the one the program uses. |
| R5 | **What the scan found of shaders is installed like plugins are**: `set_installed_shaders` keeps the kinds of effect of the usable ones. `EffectKind::offered()` lists the **filters and the transitions** after the built-in effects and before the plugins, and a new `EffectKind::installed_generators()` lists the generators. The + menu shows them under the heading **ISF** (which `SOURCES_IN_ORDER` has had since phase 6), grouped by the first of their `CATEGORIES`; **New > Generator** on an empty part of a track lists the generators, each a clip in one step, as *Generator from file...* makes one. Both of those stay. | The shader's text is put in the project when it is used (format 16), as it is for a file chosen by hand, so a project never depends on the folder. |
| R6 | **The same shader in two folders is one**: the first folder (the order of who wins, as for plugins) wins, and the other is listed as a duplicate. Two shaders that have the same file name and different texts are two kinds (the key has the digest in it). | |
| R7 | **The window shows the shaders**: a line in the summary (*"212 shaders can be used, 9 cannot"*), and under the libraries two foldable lists, **Shaders that can be used (n)** (name, kind: filter, generator or transition, and the file) and **Shaders that cannot be used (n)** (name, the file, **why**), the second open when it is not empty. The folders list and *Scan again* serve both. | UI-09: what failed must be seen with the reason. |
| R8 | **Nothing is always visible**: the window is opened from the Edit menu, as it was. The menu's name changes, and nothing else on the screen. | The rule about unrequested UI. |
| R9 | **Reading a shader file is one function in `isf`** (it was in `shader_flow`, behind a window's command): `isf::kind_of_a_shader_file(path)` reads the text, the images beside it (K15 of 6.4: the folder of the shader or below it, at most six, PNG or JPEG), makes the kind and says what is wrong, in words. The window's *Add effect from file...*, the generator's chooser and the scan all use it. | One place for the rules about what a shader file may be. |

## 2. 6.9: the ISF bundles [P unless said]

### What was found [V, 2026-10-22: the files were downloaded and read, each in a folder of its own, never run]

* **The one collection with a licence that allows it is Vidvox's own, [`Vidvox/ISF-Files`](https://github.com/Vidvox/ISF-Files)**: MIT (`LICENSE`, "Copyright (c) 2018", the name left blank), at the newest commit `395072d` of 2026-06-04: **327 `.fs` shaders** (and 38 `.vs`, which are
  vertex shaders of particular programs, and 7 pictures), a header of JSON each, all of which `isf::read` reads. **68 of them are transitions that came from gl-transitions** ("Automatically converted from ..."), each keeping its own `// License: MIT` line and
  author, so there is no need to convert gl-transitions (the repository's own `LICENSE` says nothing; its files say MIT one by one, and Vidvox has already done that conversion).
* **Not every file in it is MIT.** What a file says of itself, read by a script (`classify`, kept in the notes of the research) and then by eye:
  `Boxinator.fs` says *Creative Commons Attribution-NonCommercial-ShareAlike 3.0* (**not allowed**); `Inverted Page Curl.fs` and `Stereo Viewer.fs` carry BSD notices (3-clause and 2-clause, **allowed**, and kept in the file); 59 + 7 files say `License: MIT`
  (gl-transitions); 5 more have the MIT licence of Ashima Arts' noise or of Felix Turner's *Bad TV* in them. Six are **ported from Shadertoy or from the code of Inigo Quilez** (`ASCII Art`, `Smoke Screen`, `Perlin Transition`, `Broken LCD`, `Dirty Lens`,
  `Smudged Lens`), whose default licence is the non-commercial one; **44 name another person's code** ("adapted from", "ported", "based on <an address>", a blog, Stack Overflow) and say no licence of it; `Brick Pattern` and `Video Bricks` come from the Book of
  Shaders; the shaders credited to `mojovideotech` are Creative Commons non-commercial; four bring pictures of their own whose licence is not said.
* **The rule that follows** (LG-02, widened by the maintainer on 2026-10-20: the permissive licences, and GPL and LGPL that are compatible with GPLv3): **a shader is in the bundle only if what the file says of its licence is an allowed one, or the file
  is credited to the people of the repository (Vidvox, zoidberg, vade, v002, Carter Rosenberg, or no credit at all) or to an individual who gave it to the repository, names no other source of code and no licence that does not allow it, and brings no pictures** (a gift to
  a repository with an MIT `LICENSE` is under it: GitHub's terms say so, and I believe it [B, 85 %]). Anything else is **left out, and the reason is in `docs/bundleable-isf-shaders.md`**. This leaves **280** of the 327 that may be bundled.
* **The program runs 157 of those 280 today** (the corpus test, which reads, compiles, installs and *draws* each on the software adapter): 132 filters and 25 generators; **63 are transitions** that wait for 6.6, 20 draw their last pass into a buffer (not run yet), 8 read more than 7
  pictures, 8 read a second picture that is not the layer's, 10 do not compile (audio inputs, `_imgRect` variables, a construct of GLSL that naga's front end does not take), 2 have a pass whose size names an input. **Each of them says why**, and none is in the manifest
  until it runs. (The corpus also found a real mistake in the host: a point was multiplied by the picture's height over 1080, and 79 of the 90 points of these shaders are fractions of the picture. Fixed, with a regression test: commit `da9d097`.)

### Decisions

| | | why |
|---|---|---|
| B1 | **A second manifest, `bundled-plugins/shaders.json`, kept in the program** like the first, with its own checks (`plugin_bundles::shaders`): per set an id, a name, the commit it is of, the authors, the licence as an SPDX expression of the licences of its files, the home page, **the licence texts** (each with an address and a checksum), where the source is, and **every file**: address, sha256, size, the name it is given, its licence and the name of the shader for a person. The checks: every address is https; every file has a checksum and a size that is at most `MOST_BYTES_OF_A_SHADER`; every name is a plain `.fs` file name; every licence is an allowed one (`licence_is_allowed`); no name twice. | The same policy as for plugins, written once. |
| B2 | **The addresses are of single files at a commit** (`raw.githubusercontent.com/Vidvox/ISF-Files/<commit>/ISF/<name>.fs`), **not of an archive**: the bytes of a file at a commit do not change, and an archive that GitHub makes is not promised to be the same twice. | Found when thinking about the checksum; an archive's would one day not match. |
| B3 | **One curl, many files**: the files of a set are fetched by one run of `curl` given a config file (`url` and `output` for each), so that the connection is kept (about 120 ms a file measured on this computer for 374 files, and the Windows build of curl failed 6 of 6 when run in parallel with `schannel` revocation checks, which a plain run did not); each is checked against its checksum **before** it is used, and a set is built in a folder of its own and put in place only when all of it is there (as for plugins). `--ssl-revoke-best-effort` is given (only where curl's build has it), because the check that a certificate was not revoked cannot be made on some networks, and **the checksum, not the connection, is what says the file is the right one**. | Measured 2026-10-22; the Windows revocation failure is a fact of this machine. |
| B4 | **A new folder, `fastforwardrs/shaders` in the data folder of the person** (next to `plugins`), is where the *Bundles* tab puts shaders (a folder for each set, with the `NOTICE.txt` and the licence texts); it is one of the folders the scan looks in. It is not the folder of plugins, which is trusted as code. | R2, R3. |
| B5 | **The *Bundles* tab has a section for each kind**: *Sound plugins* (as it was) and *Shaders*, each with its own button, question, progress, cancel and message. A person who wants only the shaders (about 0.8 MB) is not made to download 145 MB of plugins. The question names the number of shaders, their size, the sites they come from and the folder, and says that each file is checked against a checksum kept in the program. Nothing is downloaded until the question is answered yes. | The permission given on 2026-10-22 is to download shaders; what a person downloads is still their question. |
| B6 | **`docs/bundleable-isf-shaders.md`** is the record: the repository, the commit, the licence of each file and the reason for each left out. `NOTICE.txt` in a set's folder says who wrote the shaders, that the files are unaltered, and where the source is. `docs/distribution-notices.md` and the README say what a copy of the program must carry. | LG-02, DS-notices. |
| B7 | **The bundle holds only shaders that run**: the manifest is made by a script (`tools/bundled_plugins/make_the_shader_manifest.py`) from the corpus's own list of what it drew; an ignored test (run by hand when the manifest changes, as the one for the plugins is) fetches every file from the network, checks each against its checksum and has the corpus read, compile and draw each. | A shader in the manifest that does not run is a bug that reaches a person. |

## 3. 6.6: transitions [to be written before building]

## 4. 6.4.3b: the lead-up on a seek [to be written before building]
