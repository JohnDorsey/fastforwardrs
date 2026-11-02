# Phase 15 design: VST3 plugins, each in a process of its own

*Written 2026-11-02, before any code. The maintainer asked: "I'd like us to support VSTs now. I'd like them to run in their own process for crash isolation." Tags: **[D]** decided by the maintainer, **[V]** verified by reading the code of this repository or running something, **[R]** read in a web search on 2026-11-02 (not run), **[P]** my proposal, **[B]** belief from what I know of the VST3 API, to be checked in piece 15.0.*

## 0. What was asked and decided [D]

| # | Question | Answer |
|---|---|---|
| D1 | Which formats? | **VST3 now, VST2 as a later phase.** The code must leave room for a VST2 kind (an address variant, a scan kind, a window kind) without rework. |
| D2 | How is the host built? | **A crate of our own, `crates/vst3-host`, on the `vst3` bindings**, like `lv2-host` and `clap-host`: the `unsafe` stays in the crate, the root crate keeps `#![forbid(unsafe_code)]`. |
| D3 | Which systems are proven? | **Windows is proven with real plugins.** Linux and macOS are written for and checked by CI with test plugins of our own; I will say which is which in the report. |
| D4 | Windows (the plugin's own GUI)? | **Sound and parameters first; windows are the second piece of the same phase.** |
| D5 | Instruments? | **Effects and instruments**, main audio bus and main event bus only. |
| D6 | Real plugins to try? | **Our own test plugins, and VST3 builds of GPL-3-compatible plugins that can legally be bundled (Bundles tab; where I have read the licence and a Windows build exists).** The maintainer owns no VST plugins, so there are no "own plugins" to try (2026-11-02). |
| D7 | Processes? | **VST3 only.** A VST3 plugin always runs in a plugin program of its own and never in the editor's process. CLAP, LV2 and LADSPA effects stay as they are (`THE_PLUGINS_IN_A_PROCESS_OF_THEIR_OWN` is not touched). |
| D8 | Parameters? | **Visible, writable, automatable ones become the effect's values; stepped ones are choices.** Hidden and read-only ones are left out; the bypass parameter is a switch among the values, as for CLAP (written before as "the effect's own on/off", which was wrong: corrected in section 9). |

## 1. What there is [V, read 2026-11-02]

* **Three plugin kinds exist**, each a crate with its own `unsafe`: `ladspa-host`, `clap-host`, `lv2-host`. The seam between a kind and the rest is the trait `PluginInstance` (`src/plugin_host/instance.rs`): `set_value(handle, value, at_sample)`, `run`, `run_with_notes`, `reset`, `latency`, `outputs`, `between_blocks`. `PluginEffect` does the layouts, the looks at the keyframes, the silence, the notes and the "left out" handling once for every kind. A VST3 copy is one more implementor.
* **Processes exist** (phase 14): `src/plugin_process/` (`protocol`, `remote`, `program`, `told`) and the program `src/bin/fastforwardrs-plugin-process.rs`. One process for each copy; a crash, a hang (5 s for one block) or an exit becomes an `Err` of that call; `RemoteCopy` starts the program again (up to 5 times), re-sends the values, the held notes and the state, and replays the failed block. The protocol's `Address` enum has `Lv2 { uri }` etc. (`protocol.rs:157`), and `program.rs` has one `make_a_*_copy` function for each kind (lines 167 and 236 for LV2). **Measured then [V]:** 0.10-0.15 ms for a block of 512 samples against 0.015 ms in the editor's process; a start takes about 55 ms for Surge XT against 13 ms.
* **Scan** runs in a program of its own (`fastforwardrs-plugin-scan`, `src/plugin_scan/`): `SCAN_FORMAT` is 9 now; a scan that crashes does not take the editor with it; results are cached.
* **Windows of plugins** run in a program of their own too (`src/plugin_editor/`, `PluginKind::{Clap, Lv2}`, `--kind`).
* **Addresses**: `PluginAddress::{Ladspa, Clap{id}, Lv2{uri}}` (`src/project/effects/installed.rs`). A project carries the snapshot of the effect like any plugin's; an effect whose plugin is missing passes the sound through and keeps its settings.
* **Trust** (`src/plugin_trust/`): the unit is the library (LV2: the folder that holds the bundles).
* **Bundles tab** (`src/plugin_bundles/`): downloads sets of CLAP and LV2 plugins, reads their licences, refuses what cannot be bundled.

## 2. What VST3 is, as far as the host is concerned [B unless marked]

* **A module.** On Windows a `.vst3` is normally a **folder** (`Name.vst3/Contents/x86_64-win/Name.vst3` is the DLL); older plugins are a single DLL called `.vst3`. Linux: `Name.vst3/Contents/x86_64-linux/Name.so`. macOS: a bundle. The library exports `GetPluginFactory`, plus `InitDll`/`ExitDll` (Windows), `ModuleEntry`/`ModuleExit` (Linux) or `bundleEntry`/`bundleExit` (macOS). Folders: Windows `%COMMONPROGRAMFILES%\VST3` and `%LOCALAPPDATA%\Programs\Common\VST3`; Linux `~/.vst3`, `/usr/lib/vst3`, `/usr/local/lib/vst3`; macOS `/Library/Audio/Plug-Ins/VST3` and the user's.
* **A factory** lists **classes**; each has a 16-byte class id, a category (`"Audio Module Class"` is a plugin) and subcategories (`"Fx|Reverb"`, `"Instrument|Synth"`). **The class id is the address** (`PluginAddress::Vst3 { class_id }`); one library can hold many.
* **A plugin is two objects**: the *component* (`IComponent` + `IAudioProcessor`, the sound) and the *controller* (`IEditController`, the parameters and the window). They are one object or two classes (`getControllerClassId`); when they are two the host connects them (`IConnectionPoint`) and must pass the component's state to the controller (`setComponentState`). This is the part with no counterpart in CLAP.
* **Processing**: `setBusArrangements`, `setupProcessing` (sample rate, most samples, 32-bit float), `setActive(true)`, `setProcessing(true)`, then `process(ProcessData)` with audio buses, **parameter changes** (a queue of `(sample offset, normalised value)` for each parameter: sample-accurate, as CLAP's), **events** (note on/off with a sample offset), and the process context (tempo, position). Latency is `getLatencySamples`, the tail `getTailSamples`.
* **Parameters** are `ParamID` (u32) with a value in **0..1 normalised**; the plugin turns it into a plain value and a text (`normalizedParamToPlain`, `getParamStringByValue`) and says `stepCount`, units and flags (`kCanAutomate`, `kIsReadOnly`, `kIsHidden`, `kIsList`, `kIsProgramChange`, `kIsBypass`).
* **State** is a byte stream (`IBStream`, which the host implements) for the component and another for the controller; both belong in the project.
* **What the plugin asks of the host** (`IComponentHandler`: `beginEdit`/`performEdit`/`endEdit` when its own window moves a knob, `restartComponent` for a changed latency, IO or parameter list; `IHostApplication`, `IMessage` for the pair's private talk) must be answered, or some plugins refuse to work.
* **Windows**: the controller makes a view (`createView("editor")` gives `IPlugView`), which is attached to a native parent (`HWND`, `NSView`, X11 window id), told its size, and given a frame (`IPlugFrame::resizeView`) to ask for another. **On Linux the plugin also needs a run loop from the host (`IRunLoop`)**, the biggest piece of work for Linux windows.
* **Licence** [R]: the VST3 SDK is **MIT since 3.8 (October 2025)**; the `vst3` crate (coupler-rs/vst3-rs) is **MIT OR Apache-2.0**, bindings generated from the SDK's headers; the older SDK was GPLv3-or-proprietary. All of it fits a GPL-3.0-or-later host. Whether the crate needs the SDK and libclang at build time is not clear from what I read (version 0.2.0's notes say yes, the `vst3-host` crate's say it ships pre-generated bindings): **15.0 settles it.**

## 3. Decisions of design [P]

| # | Decision | Why |
|---|---|---|
| V1 | **The `vst3` crate's bindings are used for both the host and our test plugins**; if it cannot be built on all three CI systems without the SDK and libclang, the fallback is bindings generated once and committed (the SDK is MIT now), with the licence text in `THIRD_PARTY.md`. | The LV2 lesson: a test plugin written from my own reading of the headers proves nothing about the ABI. Bindings made from the real headers do, and a real plugin (Surge XT or Dragonfly as VST3) closes it. |
| V2 | **The address is `PluginAddress::Vst3 { class_id }`**, with the id written in the portable form of Steinberg's own text (32 hex digits of the four 32-bit numbers `a, b, c, d` of `INLINE_UID`), **not the raw 16 bytes**, which differ between Windows and the other systems [V, section 6.2]. Effect key `vst3:<hex>`; the library is the `.vst3` folder or file. The project file very probably needs **no new format version**, since the address is not stored in it (the effect's key and snapshot are; LV2 added none) [B, to confirm in 15.2]. | One library holds many classes; the key must be the class, and the same on every system. |
| V3 | **`Vst3Copy` implements `PluginInstance` inside `fastforwardrs-plugin-process`; the editor never makes one.** `PluginHost` gets a rule that a VST3 plugin is *always* made as a `RemoteCopy`, whatever `InAProcessOfItsOwn` says. In the crate's own tests (`vst3-host`) a copy is made directly. `PluginHost::new()` with `Nothing` for the other kinds stays, so the old tests run what they ran. | D7, and P6 of phase 14: if the program is missing, the plugin is not run. |
| V4 | **Values are plain values** (like CLAP's): the value range of an effect value is `normalizedParamToPlain(0)..(1)`, a keyframe is interpolated in plain space and turned back with `plainParamToNormalized` for the plugin. A plugin with a logarithmic knob has a linear slider here, exactly as CLAP plugins do. A stepped parameter (`stepCount > 0`) is a choice with the plugin's own texts. | One rule for all kinds; D8. |
| V5 | **Values reach the plugin as parameter changes at their samples** (`takes_values_at_samples` is true), a change made by the plugin's own window comes back through `performEdit` and is queued into the next block, and is kept as the effect's value (the way CLAP's edits are). | VST3 is sample-accurate like CLAP. |
| V6 | **State is the component's byte stream, plus the controller's when the controller has one** (written together as one blob in the effect's `state`); restoring runs `setState` on the component, then `setComponentState` and `setState` on the controller, **and a controller that answers "not implemented" is not an error** (all three vendors tried did so [V]). Like Surge XT's CLAP crash (a state and a note in one block), **an instrument given a state runs one silent block with no events before any note; 15.0 showed that the VST3 build needs it too (section 6.6)**. | Phase 14, section 6.2: the workaround is cheap and was proven; the census in 6.6 proves it again. |
| V7 | **Only the main audio input and output bus and the main event input are used, and "main" is the bus whose type is `kMain`, not the first one**: Surge XT's only audio input is a side-chain (`kAux`) and so it is an instrument, as with CLAP [V]. Other buses (side-chain, extra outputs) are left inactive; a plugin that cannot work without them is shown as unusable with the reason. | D5. |
| V8 | **Notes**: note on/off (with the velocity, the pitch, a note id) at their samples. Pitch bend and controllers go through `IMidiMapping` if the plugin offers one, and are dropped otherwise (the effect says which). Note expressions and MPE are not used. | VST3 has no raw MIDI CC events; the mapping is the standard way. |
| V9 | **The scan lists classes**, runs in the scan program, and a library that crashes or hangs is reported with why, as for CLAP. The scan makes the component and the controller long enough to read the buses, parameters and latency, then drops them. `SCAN_FORMAT` goes to 10. | Same crash isolation as the rest. |
| V10 | **Trust**: the unit is the `.vst3` (a folder or a file), as for CLAP. | Same rule. |
| V11 | **Plugins that need a window, a login or a licence check to start** (an iLok dialog, an activation screen at load) are shown as "could not be started without a person" with the words they gave, and are not used. | They cannot be started headless; a crash-free refusal is the honest answer. |

## 4. Pieces [P]

| Piece | What | Gate |
|---|---|---|
| **15.0** | **Done 2026-11-02**: a throwaway program loads real VST3 plugins (Dragonfly Reverb, Surge XT, Surge XT Effects) through the `vst3` bindings and the open facts are settled or moved to 6.5. (The baseline run of the existing suite was not repeated: nothing in the repository changed.) | section 6 |
| **15.1** | **Done 2026-11-02 (section 8).** `crates/vst3-host`: find modules, load the library, call the entry, the factory and its classes, `IHostApplication`, a memory `IBStream`, a component handler, the pair (component + controller, connected), buses, parameters (list, plain and normalised values, texts), process with parameter changes and events, latency and tail, state both ways, `restartComponent`. Plus **`crates/vst3-test-plugins`**: a gain/filter effect with a parameter that is stepped and one hidden, a latency, a two-class (split) plugin, an instrument, hooks like `FFRS_TEST_PLUGIN` (crash when run/made/loaded, hang, exit, noise on stdout). | the host's own tests (not through the program); the test plugins; planted mistakes |
| **15.2** | **Done 2026-11-02 (section 9).** Scan and installation: the scan program learns VST3, cache (format 10), `PluginAddress::Vst3`, `InstalledPlugin` and its descriptor, folders and trust, the plugin manager and the add-effect menu list them under *VST3*, a library that fails says why. | scan tests (also with a library that crashes); screenshots |
| **15.3** | Sound: `Vst3Copy`, the protocol's `Address::Vst3`, the program makes it, `PluginHost` makes only remote VST3 copies, restart with replay, state in and out, notes. | the same differential idea as phase 14: the program's bits equal the crate's direct bits; a crash does not take the test down; the restart is bit-identical to a never-crashing copy |
| **15.4** | End to end: an effect on a clip, an instrument played by a MIDI clip, a project file with VST3 effects saved and loaded (new file-format version, old files still read), a plugin missing from the machine, a crash during a play. | the existing sound journeys with a VST3 plugin; a journey |
| **15.5** | Real plugins: VST3 sets in the Bundles tab (licence read first; the Windows build must exist), tried on this PC. Whatever breaks becomes a test. | `tests/bundled_*_real.rs`-style tests (ignored by default; they need the downloaded set) |
| **15.6** | **Windows** (the second half): `--kind vst3` in the editor program, `IPlugView` attached to the program's window, resize both ways, edits (`performEdit`) flowing back into the effect, controller state saved. Windows first; macOS and Linux code written for; Linux needs `IRunLoop`. | tests like the LV2 window tests with a test view; a real window by eye on Windows |
| **15.7** | Planted-mistake campaign over the new tests, documents (`using-the-editor-in-detail.md`, `developer-readme.md`, `project-file-format.md`, `THIRD_PARTY.md`), the report `engine-phase-15-report.md`. | mutation results, CI green on three systems |

Each piece is committed and pushed after the usual checks (fmt, clippy `-D warnings` on Windows and Linux, rustdoc `-D warnings`). Tests first, as before.

## 5. Compromises, said before they are made

1. **No VST2** (D1). Plugins that are VST2-only do not load.
2. **One process for each copy** (phase 14's rule). A project with many VST3 effects is many processes; each starts in about 50 ms and a heavy plugin (a sampler that loads gigabytes) is loaded once per copy. A shared process for copies of one library is not planned; if it matters, it is a later change in `plugin_process`.
3. **Main buses only**: no side-chain, no extra outputs, no multi-output instruments.
4. **No windows in 15.1-15.5** (D4). A plugin whose useful settings live only in its window can be loaded and its state saved but not edited until 15.6.
5. **Linux and macOS are unproven** (D3). The Linux window needs a run loop of our own (`IRunLoop`) and is the least certain piece.
6. **CI never loads a real VST3 plugin.** Only our test plugins and, by hand on this PC, real ones; the report will say which sound was heard from what.
7. **Plain, not normalised, values**; linear sliders for logarithmic knobs (V4).
8. **Notes only**; controllers and pitch bend only where the plugin has an `IMidiMapping` (V8).
9. **Plugins that cannot start without a person** are refused (V11).
10. **Parameters that are hidden, read-only or not automatable are not values** (D8); their numbers still count against the limit of numbers an effect can hold only if they are exposed.
11. **VST3 builds in the Bundles tab are the plugins we can legally ship that I can find** (D6): the maintainer owns none. CI never loads one; the real-plugin tests are ignored by default and run on this PC.
12. **An effect holds at most 255 values** (`MOST_VALUES_OF_AN_EFFECT`): a plugin with more has its first 255 exposed values kept and the rest left out, as with CLAP. For Surge XT's 774 automatable parameters those are not the ones a person would choose (the first are modulation slots). A better choice (a list of the parameters a person picks, or the plugin's own ordering by unit) is not planned.

## 6. What the spike (15.0) found, 2026-11-02

The spike is a throwaway program (`E:/s/sffrs/vst3_spike`, not in the repository): it loads a module with `libloading`, reads the factory, makes the component and the controller, connects them, reads the buses and parameters, runs 240 blocks of 512 samples at 48 kHz with an input sine or a note, plays one parameter change at a sample, reads the state and puts it back. **It ran on real plugins**: Dragonfly Reverb 3.2.10 (Room, Hall, Plate, Early Reflections) and Surge XT 1.3.4 (the instrument and the effects), both from the archives that the Bundles tab already downloads (their Windows archives hold `.vst3` folders; I fetched `dragonfly-reverb-3.2.10-win64.zip`, 22 MB, and `surge-xt-win64-1.3.4-pluginsonly.zip`, 49 MB, into the scratch folder).

### 6.1 The bindings [V on Windows and Linux, R for macOS]

* **`vst3` 0.3.0 (2025-12-07, MIT OR Apache-2.0) ships its bindings already generated** (`src/bindings.rs`, 15,436 lines; `build = false`; the changelog: "it is no longer necessary for users to supply a copy of the VST 3 SDK"). No SDK and no libclang. **V1's fallback (bindings of our own) is not needed.** Every interface this plan names is in it (`IPluginFactory3`, `IComponent`, `IAudioProcessor`, `IEditController`, `IPlugView`, `IPlugFrame`, `IRunLoop`, `IMidiMapping`, `IUnitInfo`, `IComponentHandler`, `IConnectionPoint`, `IBStream`, `IHostApplication`, `IParameterChanges`, `IEventList`, `IMessage`, `IAttributeList`).
* **Cost of building it: about 0.7 s** for the crate in a debug build (7 MB of build output); its one dependency is `com-scrape-types`.
* **A real Linux difference, found by `cargo check` in WSL: the constants of the enums (`MediaTypes_::kAudio`, `BusDirections_::kInput`, `ProcessModes_::kRealtime`, `SymbolicSampleSizes_::kSample32`) are `i32` on Windows and `u32` on Linux**, while the structs and methods take `i32`: the spike, written on Windows, has 17 errors there. **Rule for 15.1: every such constant is written with `as _`**, and CI on Ubuntu is the check. macOS is believed to be like Linux (not run).
* The `Class` / `ComWrapper` machinery implements host-side objects from Rust (`IBStream`, `IHostApplication`, `IComponentHandler`, `IParameterChanges`, `IParamValueQueue`, `IEventList` all took a few lines each).

### 6.2 What real plugins did [V, this PC, Windows]

| | Dragonfly (Room, Hall, Plate, Early) | Surge XT Effects | Surge XT (instrument) |
|---|---|---|---|
| Classes in the factory | 2 (processor, controller class) | 3 (processor, controller, "Plugin Compatibility Class") | 3 |
| Component and controller | separate classes, connected through `IConnectionPoint` | separate | separate |
| Audio buses | in 1 main, out 1 main | in: main + side-chain (`kAux`), out 1 main | **in: only a side-chain**; out: main + 2 aux (inactive by default) |
| Event buses | none | none | 1 (16 channels) |
| Parameters | 9 to 20, of which 2 are hidden and read-only (buffer size, sample rate) | 14, bypass 1 | **2,855**; 774 automatable; bypass 1 |
| `IMidiMapping` | no | yes | yes |
| Tail | 0 | 96,000 samples | 96,000 samples |
| Latency | 0 | 0 | 0 |
| Component state | 351 bytes | 1,122 bytes | 50,239 bytes |
| Controller `getState` | "not implemented" | "not implemented" | "not implemented" |
| Block of 512 | 0.015 to 0.10 ms | 0.011 ms | 0.045 ms (first block 3.2 ms) |
| Sound | reverb tail heard after the input stopped (energy 14.7, then 0) | passes the sine through | note on at block 0, off at 40: energy during, a tail after, silence late |

* **Times** (debug build of the spike, warm): loading the library 21 to 29 ms (88 to 244 ms the first time, from a cold disk), making the component 8 to 12 ms, the whole class cycle (make, connect, activate, 240 blocks, state, tear down) about 40 ms. This is *less* than the process start of phase 14 (55 ms for Surge XT as CLAP), so a process for each VST3 copy costs about what a CLAP one does.
* **The class id**: the 16 bytes that `getClassInfo` gives are **not the same on Windows and Linux/macOS** (`uid()` in the `vst3` crate swaps bytes on Windows; Dragonfly's id is `20 46 50 44 ...` here, "DPF " read backwards, and Surge's `01 ef cd ab 82 91 eb fa` reads as `0xABCDEF01, 0x9182FAEB` once the bytes of each number are put back in the order the crate's `uid()` code says; that reading is my arithmetic from the crate's source, checked only against these two ids [B]). V2 therefore keys on the four 32-bit numbers, so that a project made on one system finds its plugin on another.
* **Only the classes of category "Audio Module Class" are plugins**; "Component Controller Class" and "Plugin Compatibility Class" are skipped.
* **Everything ran on one thread** (the one that made the objects): no message loop and no COM initialisation were needed for these two vendors' plugins (Surge XT is, I believe, built on JUCE [B], whose plugins often want a message loop for windows, but not for sound). So the plugin program can use one thread for VST3, as for LV2.
* **`controller.getState` and (for Surge) `setComponentState` answer `kNotImplemented` (0x80004001)**: the component's state is the whole patch. V6 is changed to say so.
* **Surge XT Effects called `restartComponent(16)` (`kParamTitlesChanged`) and `performEdit` ten times without a window**, in the run with a parameter change at block 5. A host must answer them and **must not take such an edit for a person's**: with no window open, edits from the plugin are not the person's (the cause, probably the echo of our own parameter change, is to be read in 15.1).
* **No plugin asked `IHostApplication::createInstance`** (for `IMessage` / `IAttributeList`); the spike answered "not implemented" and nothing failed. A minimal `IMessage` is still to be written in 15.1 for plugins that do ask [B].
* **The effect values**: Surge XT as an instrument has 774 automatable parameters against `MOST_VALUES_OF_AN_EFFECT` = 255. The CLAP side keeps the first 255 and drops the rest (`effect_of_a_clap_plugin`, `plugin_scan/effects.rs`); with D8 the VST3 side will do the same. For Surge the first ones are modulation-slot parameters ("M1: -"), so it will not be the ones a person wants: **said as a compromise below (12), as it already is for CLAP**.

### 6.3 What `NoteEvent` needs [V, read `sound_engine/note_events.rs`]

`NoteEventKind` is `On`, `Off`, `Controller`, `PitchBend` and `AllOff`. `On` and `Off` are VST3 events as they are. `Controller` and `PitchBend` have **no VST3 event**: they are parameters found through `IMidiMapping::getMidiControllerAssignment(bus, channel, controller)` (pitch bend and controller 1..127 have numbers in that call; `kPitchBend` is controller 129, `kCtrlAllNotesOff` is 123). Surge has the mapping; Dragonfly (effects) does not need one. `AllOff` is done by the mapped all-notes-off controller if there is one, else by `setProcessing(false)` then `setProcessing(true)` (a reset), to be chosen by a test in 15.1.

### 6.4 Not tested by the spike (said plainly)

* (The silent first block was tested after the first run of the spike; see 6.6.)
* **Windows (editor views)**, `IMessage`, `IUnitInfo` (programs and units), `IParameterFinder`, a crashing plugin, a hanging one, many copies at once, unloading and reloading a library, and any run on Linux or macOS.
* **A census of crashes**: three vendors, 6 modules, no crash seen.

### 6.5 Still open for 15.1

* The cause of Surge Effects' ten `performEdit` calls (an echo of a host change, or the plugin's own?) and the rule that follows.
* All-notes-off by mapping or by reset (6.3).
* The unusual cases the spike did not meet: a plugin whose controller must exist before the component's `setState`, a component that is also the controller (the single-component kind), a plugin with no `kMain` output, an output of more than two channels.
* Which further GPL-3-compatible VST3 builds can be bundled besides the two sets already in the manifest (Dragonfly and Surge XT: their Windows archives hold `.vst3` folders, so for these 15.5 may be only a change of the file lists for each system; the Linux and macOS archives still have to be listed [B]). Other candidates [B]: the DPF-Plugins archive the manifest already names (MVerb, Kars, ...), Vital; licences to be read.

### 6.6 Does Surge XT as a VST3 have the same bug? Yes, so the workaround stays [V]

Asked on 2026-11-02 ("test whether Surge as a VST3 has the same bug, and don't include the workaround if it isn't necessary"). The census of phase 14, again through the VST3 interface: **one fresh process for each play**, the patch's `.fxp` chunk given with `setState` on the component (and `setComponentState` on the controller), a note on at sample 0 of block 0 of 100, the buses set up as a host would (`E:/s/sffrs/vst3_spike/census.py`; the 3,008 patches of the factory and third-party sets).

| Condition | Plays | Failed |
|---|---|---|
| State and note in the **first** block, no silent block; every patch once | 3,008 | **1 hang** (Vincent Zauhar / Sequences / Chord Me), 0 crashes |
| the same, every patch three times | 9,024 | **3 hangs** (all Chord Me), 0 crashes |
| Chord Me alone, with the controller | 12 | **11 hangs** |
| Chord Me alone, without the controller | 12 | **12 hangs** |
| Chord Me, note in block 1, in block 2, or no note at all | 12 each | 0 |
| **A silent block with no events before the first note**; every patch three times | 9,024 | **0** |
| the silent block, Chord Me | 12 | 0 |
| the seven patches that crashed the CLAP build (Holy Emoji, Lanes, Pick Me Minor Arp, Brass Nostalgia, FM3 Deiified, Neustation, Wavetextures), no silent block | 280 | 0 |

(A play that did not end in 30 seconds was counted as a hang and its process killed.)

**What this says.** The trigger is the one of the CLAP build: it fails only when the note is in the same block as the state's application; a note one block later, or no note, is safe. **The patches that fail are others, and the failure is a hang and not an access violation**, which is what the same uninitialised memory would do with another layout of the heap in another wrapper [B: I did not read Surge's code]. It is rarer here (1 patch of 3,008, which then fails 11 of 12 plays) than for CLAP (7 patches, about 4.5 % of their plays), but it is real, and a hang costs a block's whole limit (5 seconds) and a restart. **So the workaround is necessary, and V6 keeps it.** The upstream fix is on Surge's main branch and not in release 1.3.4, which the Bundles tab downloads (memory note on the root cause).

**What this does not say.** The `.fxp` chunk is accepted as the VST3 state as it is (a note after the state sounded, peak 0.38 for Holy Emoji); 33 of the 9,024 plays gave silence (probably patches that make no sound for key 60 at velocity 0.8, not a failure to load; not checked).

## 7. Unscheduled to-do

* **One process shared by the copies of one library** (asked 2026-11-02 as a "later, if it hurts"). Phase 14 starts a process for each copy; a project with a heavy VST3 sampler on ten clips would load it ten times. Not planned; to be looked at only if it is measured to hurt. Sharing means one plugin instance per copy inside one process, which gives up some of the crash isolation (a crash takes every copy of that library), so it would need a decision on that too.
* **VST2** (D1): a later phase, with its own decision about the headers.
* **The 255 values of an effect** (asked 2026-11-02: why is there a limit, and can it be removed?). *Why* [V, read in the code]: a value is named in the code by its **slot**, a `u8` in `PropertyId::EffectValue { effect, instance, slot }` (`src/project/animation/property.rs`), which is the key of every animation, edit and history entry, and a mask source reads its value by it too; `MOST_VALUES_OF_AN_EFFECT = 255` (`src/project/effects/descriptor.rs`) is the size of that number, chosen to keep `PropertyId` small and `Copy`. Nothing deeper makes it 255. *Can it be removed* [B for the cost]: yes. The slot would become a `u16` (65,535 values); the compiler finds the 16 places that turn a slot into a `u8` (`u8::try_from(slot)` in `project`, `project_file`, `composition`, the editor's flows and the properties panel) and the tests that pin 255 (`project/effects/kind_tests.rs`, the CLAP and LV2 scan tests). What that does not settle: how the effect panel shows hundreds of sliders (it has no search or grouping, and I have not looked at how it copes with 255), how big a project file gets with a snapshot of 2,855 values (Surge XT as VST3), and which values should come first (the first 255 of Surge XT's 774 automatable ones are modulation slots). Not started.

## 8. 15.1 as built (2026-11-02)

**What exists.** `crates/vst3-host` (GPL-3.0-or-later; `unsafe` only inside it, every block says why; a test counts them) and `crates/vst3-test-plugins` (CC0; a `cdylib` + `rlib`, in the way of the other test-plugin crates). Both are in the workspace; **nothing in the editor uses them yet** (that is 15.2 and 15.3), so the shipped program, and `THIRD_PARTY.md`'s list of what it uses, are the same except for the lock file (below).

| Module of `vst3-host` | What it does |
|---|---|
| `classid` | `ClassId`: the four numbers, written as 32 hex digits; reads the bytes of this system (`from_tuid`) and makes them (`to_tuid`); saved as text. |
| `module` | `Module::open` (a `.vst3` folder or file; the library where the system keeps it; `InitDll`/`ExitDll`, `ModuleEntry`/`ModuleExit` with the loader's real handle, `bundleEntry`/`bundleExit`), the factory, `classes()`; `Drop` lets go of the factory, calls the end function, then unloads. |
| `description` | The values a scan sends between programs: `ClassDescription`, `BusDescription`, `ParameterDescription`, `ClassFacts`; names in Unicode where the factory gives them. |
| `objects` | What the host gives a plugin: a memory stream, the host and the component handler (it collects edits and restart flags), parameter changes (queues kept from block to block), the event list. |
| `instance` | `Instance::new` (make, initialize, find or make the controller, connect, buses, state, set up, switch on the main buses, start, the silent block), `set_value`, `run`, `run_with_notes`, `reset`, `between_blocks`, `save_state`/`restore_state`, `plain_value`, `control_outputs`, notes through `IMidiMapping`. |
| `scan` | `scan_module`: loads, makes and runs each plugin class once on silence, saying each step. |
| `state`, `notes` | The bytes of a state (`FFV3`, version, the component's, the controller's); the notes. |

**The test plugins** are 14 classes and are strict on purpose, because the first planted-mistake run (32 of 36 noticed) showed that a forgiving test plugin hides host mistakes: a plugin of two classes gives nothing until its halves are connected; the main output must be switched on; a bus that is on must be given its channels and one that is off none; the points of a parameter must be in order of the sample; an effect has its side chain **before** its main input. The classes: a gain effect in two classes (every kind of parameter, a meter, a state), a delay with no controller, an instrument with a mapping of controllers, a plugin that is its own controller, the instrument as Surge XT has it, the effect with a side chain, a compatibility class, and four that fail (initialize, activate, process, sound that is not numbers). The controller can be told to make an edit and to ask for each kind of restart, by the state it is given.

**What was checked [V].**

* **59 tests** of the host (`cargo test -p vst3-host`): the ids, the folder of a module, a file that is not a library, a library that is not a module (a system library), the classes and their categories, the scan, the buses and parameters of every class, sound at a sample, several values in one block in any order, a value given to one block only, a list parameter, the bypass, a meter, a delay across blocks and its reset, missing channels, a block too long, notes (on and off with ids, two notes of one key, the wheel, a controller, all-off by mapping and by stopping, a controller nothing maps, a note past the end of the block), the state (two parts, every cut, into a new copy and into a running one, refused, none), an edit and each restart flag, plugins that fail, letting go 20 times, two copies, sending a copy to another thread, and the audit of the `unsafe`.
* **Linux** (Ubuntu in WSL): the same 59 pass, and clippy is clean there; **macOS is not run** [B].
* **Real plugins**: `cargo test -p vst3-host --test real_plugins -- --ignored` (set `FFRS_VST3_MODULES` to a folder of `.vst3` modules): **the four Dragonfly Reverb plugins, Surge XT Effects and Surge XT (6 classes) are found, made, played (peaks 0.41 to 0.61), their state saved and a new copy made from it and played, a value given at a sample** — with no change to the host after the first run. Nothing like it runs in CI.
* **The real plugin found one thing the test plugins could not**: Dragonfly printed "asked to delete component while audio processor still active" when a copy was let go, because the component's pointer was released before the processor's. The fields of `Instance` are now in the order of letting go (the warning is gone [V]); a test plugin cannot show it (a Rust COM object has one count for all its interfaces), so there is no regression test.
* **36 planted mistakes** (`tools/mutation_testing/mutations_phase15_vst3_host.py`, 30 seconds with the crate's own tests): the first run noticed 32; the four that were not (the controller not connected, the points of a parameter not in order, the main input or output taken for any) were closed **by making the test plugins strict**, and a new effect with its side chain first; the second run noticed **36**. Left out of the list as not seeable (said in the file): a module's start-up function saying no, a latency that changes, and a channel slice shorter than the block.
* `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` (Windows and Linux), `RUSTDOCFLAGS=-D warnings cargo doc`, `cargo deny check licenses bans`, and the root crate's library tests (6,147 passed) after the lock file changed.

**The lock file changed in ways I did not ask for.** Adding the two crates made cargo choose newer versions already allowed by the existing requirements: `tempfile` 3.27.0 now uses `getrandom` 0.4.3 where it used 0.3.4, and the `windows-sys` of five packages moved from 0.52/0.59 to 0.61.2. `THIRD_PARTY.md` was regenerated (it gains `getrandom` 0.4.3 under MIT). The root crate's library tests pass with it; CI will say whether anything else cares.

**What is not done, said plainly** (pieces 15.2 and after): the scan program does not know VST3; `PluginAddress::Vst3` and the effect built from a class do not exist; there is no `Vst3Copy` for the plugin program and `PluginHost` does not make one; no project file holds a VST3 effect; no window; the crash and hang cases of the program (a plugin that aborts, that never returns) are for 15.3, where the test plugin gets the same switch the CLAP one has; macOS passes no bundle to `bundleEntry` and its entry-point names are as I remember them [B]; `IMessage` / `IAttributeList` are not given (nothing asked); the host runs everything on one thread.

## 9. 15.2 as built (2026-11-02)

**What exists.** The scan program, the cache and the editor know VST3 modules; a class of a module becomes a type of effect that can be put on a clip, kept in a project and found again. **Nothing runs a VST3 plugin yet** (15.3): a clip with one has its sound go through unchanged, and the effect says so once ("VST3 plugins are not run yet").

| Where | What |
|---|---|
| `Cargo.toml` | the root crate depends on `vst3-host` (and, to test, on `vst3-test-plugins`). The shipped program now has `vst3` in it: `THIRD_PARTY.md` lists it. |
| `plugin_scan::folders` | `is_a_vst3_module`, `vst3_folders` (the usual places of the VST3 documentation; there is no variable for them), `vst3_modules_in` (files and folders that end in `.vst3`, down to six folders, not looked into; not a folder being made); the walks for CLAP libraries and LV2 bundles do not go into a module. |
| `plugin_scan::protocol` / `failure` / `cache` / `child` / `scanner` / `program` | `ScanAnswer::{ScannedVst3, NotAVst3Module}`, `LibraryOutcome::ScannedVst3`, `CachedOutcome::ScannedVst3`, `ScanFailure::NotAVst3Module`; `SCAN_FORMAT` 10; a module that is a folder is fingerprinted like an LV2 bundle (every file in it); the scan program calls `vst3_host::scan_module`. |
| `plugin_scan::effects` | `effect_of_a_vst3_plugin`, `key_of_a_vst3_plugin` (`vst3:` and the 32 digits), `pre_roll_of_a_vst3_plugin`; `plugins_found_in` lists a module's classes under their id. |
| `project::PluginAddress::Vst3 { class_id }` | the address; the project file needs **no new version** (its effect names its plugin by the key, and the snapshot carries the rest). |
| `plugin_host` | for a VST3 plugin `processor_for` says the plugin is not run yet, once, and gives nothing, so the sound goes through. |
| editor | the plugin manager says "N plugins (VST3)" or why a module failed; the **+** menu has a *VST3* group after the others; the placeholder of a VST3 plugin that is gone explains in the words of the other kinds, naming the `.vst3` modules that failed and no others. |

**The rules by which a class is an effect** (D8 of section 0, with one correction): the class must be a plugin (`Audio Module Class`); a main audio output of one or two channels; a main audio input of one or two channels (an effect), or none and an event input (a synthesizer, played by notes: Surge XT, whose only input is a side chain, is one); anything else is "not used" with the reason. A parameter is a value only if it is **shown, can be set, can be automated and is not a change of program**. A switch (one step, 0 to 1) is a flag, a list with a text for each step is a choice whose options are the plain values, another stepped parameter is a whole number, the others are numbers in the plugin's units, on a logarithmic slider from a thousand-fold range above zero. **The first 255 values are kept, as for CLAP.**

**A correction of what I told you.** In the question about parameters I wrote that "bypass becomes the effect's own on/off". It does not: the effect has no on/off of its own, and a VST3 plugin's bypass parameter is **a switch among its values**, as a CLAP plugin's is. (Nothing else was built on that sentence.)

**A choice made, said plainly.** A tail of 0 is read as "nothing said" and given half a second of sound before a clip (`pre_roll_of_a_vst3_plugin`), where CLAP reads 0 as "no tail". Dragonfly Reverb says 0 for its reverbs. A plugin that really has no tail is then run for half a second more before a clip than it needs, which costs a little time and changes no sound.

**What was checked [V].**

* **Unit tests** (`cargo test --lib`): the folders and the walks, the fingerprint, the words between the programs, the cache, the effect made of a class in 23 cases (hand-made classes), the manager's words, the menu, the placeholder, the host's answer, a VST3 effect saved in a project and read back with the plugin there and gone (format 27 unchanged).
* **The scan program, for real** (`tests/plugin_scan_vst3`): the test module is scanned by `fastforwardrs-plugin-scan` (10 plugin classes; four fail with their reasons); a folder of modules is scanned, kept, found again from the cache after a restart, and scanned again when the library inside changes; things named like a module that are not one are told apart (an empty folder, a text file, a CLAP library); the effects that the classes make (values, choices, switches, the synthesizers, the side-chain effect, the delay's tail).
* **Real plugins**, through the real scan program into effects (`FFRS_VST3_MODULES=E:/s/sffrs/vst3_real cargo test --test plugin_scan_vst3 -- --ignored`): the four Dragonfly Reverb plugins (7, 18, 9 and 17 values), Surge XT Effects (14) and Surge XT (a synthesizer with 255 values: the cap) are effects, with the same names and counts as their CLAP and LV2 builds in the same folder (Surge XT Effects has one more value: its bypass).
* **33 planted mistakes** (`tools/mutation_testing/mutations_phase15_vst3_scan.py`, 741 s): 32 noticed on the first run; the one that was not (more texts than steps still made a choice: unreachable from the real scan, which gives exactly one text for each step) is closed by a test with four texts for three steps; the second run noticed all.
* The checks before a push: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` on Windows and Linux, `RUSTDOCFLAGS=-D warnings cargo doc`, `cargo deny`, the library tests.

**Not done, said plainly.** Nothing runs a VST3 plugin (15.3); there is no window and no `has_a_window` (15.6); a project cannot carry a VST3 module inside it (as LV2 cannot); the Bundles tab has no VST3 sets (15.5); a VST3 plugin is not trusted or asked about yet in a way of its own: it goes through the folder trust that the other kinds use when it is run, which is 15.3; the units of a parameter (`IUnitInfo`) are not read, so values are not grouped by unit.
