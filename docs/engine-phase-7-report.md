<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Phase 7 report: the sound engine and LADSPA

Written 2026-10-19, at the end of phase 7 of [the plan of action](plugin-system-plan-of-action.md). How the phase was designed, and what each piece found, is in [its design](engine-phase-7-design.md) (decisions G1 to G18, pieces 7.1 to 7.7, with a
section for each piece); this is the short form: **done, what can be seen, decisions, tests rewritten on purpose, limits, mistakes, verified versus believed**, and then **what I need from you**.

**[V]** means I ran something and saw it. **[B]** means I believe it from reading the code. **Nothing here has been listened to by anyone**: the sound is verified as numbers (to the sample against the old graph, and against the arithmetic of
the plugins), not as music. You stopped me after 7.2 and 7.6 for that, and 7.2 was heard and approved.

## 1. Done

| piece | what | commits |
|---|---|---|
| 7.1 | the block graph of the sound engine (positions to the sample, the volume at every sample, strips, the mix) and the null test harness | `d65278f`, `cc5a691` |
| 7.2 | clip feeds (an ffmpeg for each clip), the preview and the export switched to the graph (the export through a temporary file); the old graph stays, selectable | `f05e1e6` to `9c66fe5` |
| 7.3 | who may run a plugin: project file format 14 (plugin libraries, linked or embedded), approvals, the severe question held for five seconds, extraction with a hash check, the command line refuses | `0458a95` |
| 7.4 | the host of LADSPA plugins in a crate of its own (the only `unsafe`), the plugin library written for the tests, the scan in a program of its own with a cache and a breadcrumb | `f593a53` to `88503df` |
| 7.5 | plugins as effects: the registry, the + menu, controls from the ports, the window of where to look and of libraries that plugins need, the plan of the sound carries the effects | `55e82d2`, `261ae74` |
| 7.6 | plugins run in the graph (the preview and the export through the same code); **one question for each folder** (your change); the command line looks for and refuses; the real set | `e681dd6`, `a0417ea` |
| 7.6 + | the menu of the + button is a directory of smaller menus by where effects come from, each with a scroll bar when taller than the window (your request) | `a4ba175` |
| 7.7 | conformance tests (the round trips of what a project has of a plugin that is gone, changed, or built for another computer), the mutation campaign over what was left, the README, the features list, the plan, this report | the commit of this report |

### The gate of the plan, line by line

*The null test* ([V] 7.1/7.2): the new graph is held to the old filter graph over a corpus (the projects the tests make, random projects with several tracks, cuts, gaps, keyframed gain and mutes, sped-up clips in the three settings),
and the two make the same sound to the tolerance the design names. *The LMMS set loads, plays and exports in a test project* ([V] 2026-10-19, on your computer, each plugin in a process of its own, `tests/sound_plugins_lmms_set.rs`): 119
libraries scan, **211 plugins can be effects, 197 made the same sound three times, 14 do not make the same sound twice from the same input (their own random numbers: shown on the host alone), none failed to load or run, none gave a
number that is not a number, none was left out for being slow, none said anything.** *A zero-byte or damaged DLL is "failed", never fatal* ([V] `tests/plugin_scan.rs`). *The prompt tests* ([V] the hold, the key, the reset on release,
declining as pass-through; and, for a folder, the one question, the no that holds, the export that waits).

### The numbers

Mutation campaigns by piece (planted mistakes; each survivor was read): 7.2: see the design; 7.3: 63; 7.4: 95; 7.5: 90; 7.6: **166, 150 noticed in the end after 25 new tests, 15 equivalent or unreachable, 1 does not compile**; 7.7: see
section 9. Library tests: 4548 on Windows (all 42 test programs: 4894 passed, none failed, at the end of 7.7). Ubuntu was run in WSL after 7.6 (exit 0, 42 programs); the run after 7.7 could not start (WSL had no system resources), so Ubuntu for the last commit is the CI's. Unsafe places: 25, each with its reason, counted by a test.

## 2. What can be seen

*Edit, Sound plugins...* (where to look for plugins, and for the libraries that plugins need; what the scan found and why some failed; *Scan again*); the **+ menu of a clip, a track or a tag** has a smaller menu for *Built-in* and one for *LADSPA*
(and later one for each other place), each scrolling when taller than the window, with the plugins under the name of their maker; a question *Run plugins from this folder?* when a plugin of a folder nobody has answered for is needed; the
severe question for a plugin inside a project file; a message when a plugin was left out of the preview for being slow, could not be loaded, or the last run ended badly while using a library. Pictures of the window and the menus are
made by the screenshot test (`tests/application_screenshots.rs`, ignored by default).

## 3. Decisions

Those of the design (G1 to G18) stand, with these of mine [P]: the scan also makes each plugin and runs it once (a plugin that crashes in its constructor must not take the editor down); failures are kept by size and time of change, not by a hash;
a track's effects are done to each of its clips; no pre-roll; a plugin that is too slow in the preview is left out after two seconds in a row (never in the export); a plugin's output is silenced if it is not a number or is more than
1000 times full scale (your decision, 2026-10-20). **Yours:** the folders for the libraries that plugins need and the box to look in the folders above (on by default); one question for each folder; the menu as a directory.

## 4. Tests rewritten on purpose

7.3: the tests that spelled the file format version now say 14. 7.5: two tests of the rule for what a sound effect can be put on (a video has a soundtrack). 7.6: the two that spelled the old rule (a yes for a file by path and hash) now say
the folder; the lines of the command line and dialog tests about *file*. After 7.6: about 40 tests that chose an effect straight from the + menu now open the smaller menu first (a helper), and the ones that looked at the headings in the
menu itself (`effect_menu_tests`, one of `sound_plugins_tests`) look at the directory. Nothing was weakened to pass.

## 5. Limits

Plugins run **in the editor's own process**: one that crashes ends the editor (the breadcrumb says which, the next time). A sample more than 1000 times full scale is made silent (your decision, 2026-10-20). Plugins that do not repeat themselves do not make the same
sound in the preview and the export (the plugin's own randomness). No pre-roll, no latency compensation, no generators, no master bus. A track's effect is not one effect on the sum of its clips. Sound effects of a group of clips or tracks
do not exist. The effects of an embedded plugin do not run (the question is asked). On Linux and macOS the editor cannot change its own library search path after it starts: a plugin that needs a library that is not beside it may scan and
not load [B, not tried]. CLAP is not here.

## 6. Mistakes (mine)

7.2: the stream's drop did not stop the ffmpeg of the clips (found by the CI of Windows), and the first mend made 60 ms gaps on Linux (found by the sound tests of Ubuntu). 7.2: an export could not be stopped while a clip waited for sound
(found on Ubuntu). 7.4: `u64::from` that is a real conversion on Windows and not on Linux; two rustdoc links. 7.5: a Windows path in a test that ran on Linux. 7.6: the host's default for the folders above a plugin library was off while
yours is on (found by a test I wrote for the mutation campaign); **the tests that install plugins shared a list with the tests that depend on it, and the CI found a test (a sound has no + button) running while a plugin was installed**:
now a shared and an exclusive side, and the list is emptied when an installing test lets go. **It happened again in 7.7**, and the reason was a test of the command line that went through the program's own entry point, which looks for plugins and so changed the list without holding it: the list now refuses to be changed by any test that does not hold it (an assertion in `set_installed_plugins`, in test builds only), which found that one test and will find the next. Each is in the design with its mend and its regression test.

## 7. Verified versus believed

**Verified [V]:** the arithmetic of the plugins in the graph to the sample, for blocks of any size, with keyframes, and that a seek gives the same samples as playing through (a gain that rises); preview = export to the sample with the real ffmpeg;
a slow plugin is left out of the preview and not the export; the questions for folders in the editor; the real program (`fastforwardrs-export`) refuses, finds the plugins with the real scan program, runs them, and the video is half as loud
as the plugin says; an export from the editor has the plugin's sound; the 119 libraries of LMMS; CI green on five jobs at `a0417ea` (the menu change after it: see its own run).
**Believed [B]:** that it sounds right (nobody has listened to 7.6); that plugins load in the editor on Linux and macOS when they need a library that is not beside them; that no real card is needed (the sound uses none).

## 8. What I need from you

(1) **Listen** to 7.6: a plugin on a clip, a value moved and keyframed, an export. (2) (Answered 2026-10-20: yes, beyond 1000 times full scale; built.) Whether a plugin's **enormous output** should be silenced (my suggestion: beyond ±1000, with a note). (3) **CLAP** (7.8): its design is written after
you have heard 7.6; say when. (4) Embedded plugins' effects: later, as you said. (5) The order: the plan says phase 5 next.

## 9. The campaign of 7.7

7.7 added no code that runs; it added the conformance tests (three round trips, byte for byte, of what a project has of a plugin: one whose ports changed, a linked library whose file is gone, an embedded library with a build for another
computer only) and read the survivors of the campaigns again. The code of the menu directory (`grouped_by_source`, the smaller menus) has a test of its own for the grouping, and the tests of the menus.
