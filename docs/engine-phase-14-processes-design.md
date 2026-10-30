# Phase 14: sound plugins in processes of their own (design)

*Written 2026-10-30 before the code. The maintainer asked (2026-10-30): "run instruments in processes of their own; we might want to do the same with other sound
plugins later." The reason is measured, not guessed: Surge XT dies at random (an access violation inside its own library, or a hang) when a patch is loaded as state and notes
are played (`docs/bundleable-synthesizers.md`, "Playing the patches"), and a plugin that runs in the editor's process takes the editor and the project with it.*

Tags: **[V]** verified by a test or a measurement, **[B]** believed from reading.

## 1. What is decided

| # | Decision | Why |
|---|---|---|
| P1 | **One process for each copy of a plugin** (`fastforwardrs-plugin-process`, built beside the editor like the scan program and the window program, and checked to be of the same build). | A crash then costs one copy of one plugin, not every plugin of the clip; nothing is shared between two copies of one library (so the "two copies alive at once" trouble of `Lives` cannot happen between processes [B]). |
| P2 | **The seam is `PluginInstance`** (`plugin_host/instance.rs`): the process holds a real `ClapCopy` / `Lv2Copy` / `LadspaCopy`, and the editor holds a `RemoteCopy`, which is a `PluginInstance` too. Nothing above it (`PluginEffect`, the graph, the plan) knows. | The effect already does the layouts, the values at samples, the latency, the notes and the "left out" handling once, for every format. |
| P3 | **A local socket** (TCP on 127.0.0.1, a port the editor listens on and the program is told, and a random token the program says first), not the program's standard input and output. | A plugin may print to standard output (the scan program has the same trouble and uses lines with a fixed first word); the sound is binary, and a socket has a read time limit, which a pipe has not. |
| P4 | **Which plugins**: a setting of the host, `InAProcessOfItsOwn::{Nothing, Instruments, Everything}`. The editor and the export program use `Instruments`; `PluginHost::new()` stays `Nothing` (so every test that exists runs what it ran), and `Everything` is there for the tests (every kind of plugin goes through the same seam) and for the day the maintainer wants it. | The maintainer asked for instruments now and "other sound plugins later". |
| P5 | **What happens when the process dies or does not answer**: the copy answers every call after that with an error, and `PluginEffect` does what it already does with a plugin that fails to run: it is left out for the rest of the play and says why (on the board of the effect's window and in the notes of the play). The next play makes a new process. **No automatic restart in the middle of a play** (a restarted instrument has forgotten the notes it was holding, and the sound would be wrong in a way nobody hears being announced). | The editor survives and says what happened; the rest is a follow-up the maintainer can ask for. |
| P6 | **If the program is not beside the editor, or was built from other code, the plugin is not run** and the effect says to build everything together (`cargo build --bins`), as the window program does. It is **not** run in the editor's process instead. | The point is that a plugin never runs in the editor. |

## 2. What the editor and the program say

A frame is `u32 length` (little-endian, of what follows), `u8 kind`, `u32 header length`, the header, and the rest, which is raw. The editor speaks first after the program says hello.

| Frame | From | Header | Raw |
|---|---|---|---|
| Hello | program | the token | |
| Make | editor | JSON: the format and address of the plugin, its library, the sample rate, the most frames of a run, what the scan said it is (ports, parameter ids, the LADSPA label), its name | the plugin's state, if the project has one |
| Made | program | JSON: latency, whether values are taken at samples, the most samples at once, what was said of a state that was not taken, what the plugin asked for between blocks | |
| Refused | program | JSON: *could not be loaded* / *not what was scanned* / *could not be made*, with the words | |
| Run | editor | binary: the commands since the last run (a value at a sample, a reset), the frames, the number of input and output channels, whether notes are given, the notes (16 bytes each) | the input channels, `f32` little-endian |
| Ran | program | binary: ok or the error, the latency now, the control outputs, what the plugin asked for between blocks | the output channels |

Nothing else: **the end of the connection is the end** on both sides (the program exits at once when the editor is gone, even if the plugin is in the middle of a call: the thread that reads the socket ends the process). Floats are sent as their 32 bits, so what a plugin makes in a process is the same bits as in the editor [V when tested: the tests compare bit for bit].

`between_blocks` (the plugin asked to be made again, or for a call on its main thread) is done by the program **right after each run** and said in the answer; the editor's `between_blocks()` gives what came since it was last asked. (Asking at every block would be a round trip more for each plugin and block.)

## 3. Limits

* Make: 30 seconds (the limit of the scan). Run: 20 seconds for one run (a block is 512 samples; a plugin that needs 20 seconds for one is hung). Both can be set shorter for the tests.
* The program is given the environment the scan and window programs get (where the libraries a plugin needs are looked for).
* A frame is at most 1 GiB; a longer one is a broken connection.

## 4. Pieces

| Piece | What | Check |
|---|---|---|
| 14.0 | Tests that lock today's behaviour (the host tests, the synthesizer tests) pass before anything changes. | the baseline run |
| 14.1 | `plugin_process::protocol`: frames, the messages, the binary codec; a reader that never panics on short or wrong input. | round trips, cut and flipped inputs |
| 14.2 | The program (`plugin_process::program`) and `RemoteCopy` (`plugin_process::remote`), CLAP first. | the same blocks through a copy in the editor and a copy in a process give the same bits (instrument with notes, an effect with values, latency, restarts); a plugin that aborts, hangs or exits does not take the test with it |
| 14.3 | LV2 and LADSPA in the program. | the same differential tests |
| 14.4 | `PluginHost` makes remote copies where the setting says; the words for what goes wrong are the in-process words (shared functions). | the host tests that name those words run again with `Everything` |
| 14.5 | The editor and the export program use `Instruments`; end to end: a MIDI clip through a process, a crash during a play leaves the editor alive and the next play sounds. | `midi_in_the_sound`, `synthesizers_in_the_sound` with processes; a journey |
| 14.6 | Surge XT's random crash through the process (the repro of the synthesizers document), the cost of the round trips, planted mistakes, the documents, the report. | measured |

## 5. Not done (said before it is built)

* No restart of a process in the middle of a play (P5).
* No shared memory: the sound goes through the socket (512 samples of two channels is 4 KiB a block, at about 94 blocks a second).
* The windows of plugins already have a program of their own (phase 10) and are not touched.
* No limit on the memory or the time of a process (an operating-system feature of each system).

## 6. As built (2026-10-30)

*Labels: **[V]** I ran it and saw it; **[B]** I believe it from reading and could not run it.*

**Names.** The module is `plugin_process` (`protocol`, `remote`, `program`), the program is `fastforwardrs-plugin-process` (`src/bin/`), the setting is `InAProcessOfItsOwn` (`Nothing`, `Instruments`, `Everything`) on `PluginHost`, and the one place that says what the editor and the export program use is `THE_PLUGINS_IN_A_PROCESS_OF_THEIR_OWN` (`Instruments`); a later "other sound plugins too" is that one line. `PluginHost::new()` still runs everything in the editor's process, so every test that was written before this phase runs what it ran.

**What differs from section 2.**
* `Make` has no sample rate and no frame limit: both sides use the constants of the sound engine (`SAMPLE_RATE`, 512), and a field that the CLAP path ignored would have been a lie. `Make` carries `has_state`, because a state of no bytes is a state.
* The program does a copy's `between_blocks()` once after it is made and after every run, and says it with the answer. The copy in the editor gives what came since it was last asked. [V] with the plugins that ask to be made again (`callbacks`, `latency_changes`): the same restarts at the same blocks as in the editor's process.
* A program that ended is described with its exit code in the words of the scan (`0xC0000005: access violation`, `signal 6 (abort)`), and what it said on its error output, last 300 characters, if it said anything.
* The editor stops a program as soon as its copy is gone: the connection is closed (which is what tells the program to end, even when the plugin never returns from a call: the thread that reads the socket ends the process), it is given 0.3 s to end by itself, and then it is stopped. [V] 7 ms for Surge XT.
* **Two copies of one plugin do not disturb each other** when they are in two programs (`single_instance`, which gives its sound with the sign changed when another copy is alive, is the same in both) [V], so the host does not wait for the copies of a stream that is being stopped, as it does for copies in its own process.
* The editor does not write the breadcrumb for a plugin in a program: it never loads it.

**What was measured [V]** (this computer, a debug build). Starting a program, connecting, loading the library and making the plugin: 19 ms for the test gain plugin, 136 ms for the first test instrument (cold), **55 ms for Surge XT** (13 ms in the editor's process: **about 40 ms more at each seek**). A block of 512 samples (10.7 ms of sound): 0.015 ms in the editor's process and **0.10 to 0.15 ms in the program**, about 1.4% of real time for one plugin. The export of a three-minute song with one instrument is 17,000 blocks, so 2.5 seconds more. (`cargo test --test plugin_process what_a_process_costs -- --ignored --nocapture`.)

**The check that matters [V].** With every plugin forced into a program (a temporary switch, removed), **the whole library suite (5,878 tests) and the integration programs `synthesizers_in_the_sound`, `midi_in_the_sound`, `sound_plugins_clap_in_the_sound`, `sound_plugins_lv2_in_the_sound`, `sound_plugins_in_the_sound`, the four plugin journeys, `application_sound` and `plugin_editor_sessions` pass unmodified**, at the same speed. The 8 tests that fail are the ones that are about the editor's process: the breadcrumb that is written before the editor loads a library (4), which libraries the editor has loaded (1), the host waiting for the copies of a stopping stream (3: the other two of them show that two programs do not need it), and the bundle that the editor read once and keeps (1). All 8 pass with `PluginHost::new()`, as they always did.

**Real plugins [V].** Surge XT, Kars (CLAP and LV2) and Nekobi (CLAP and LV2), fetched by the *Bundles* tab, play a MIDI clip through the plan, the graph and a program of their own: silence before, sound in the notes, the tail, and a note that is held across a seek (`tests/bundled_synthesizers_real.rs`, which now makes its host the way the editor does).

**Not done, said plainly.**
* **Surge XT's random crash was not reproduced through a program.** It needs a factory patch as the plugin's state and notes (the synthesizers document has the numbers), and the 253 MB of patches that I fetched for that are no longer on this computer. What *is* shown is that a plugin which aborts, hangs, exits, or crashes when it is loaded or made does not take the test program with it, and that the effect says why (`tests/plugin_process.rs`, `tests/plugin_host_processes.rs`).
* **No restart in the middle of a play** (P5): after a crash the instrument is silent until the next play. A restart would lose the notes it was holding.
* **No memory or time limit** on a program, and no job object / process group: a plugin that starts a program of its own leaves it behind.
* macOS: the code is the same and nothing in it is specific to a system, but nothing was run there [B].
