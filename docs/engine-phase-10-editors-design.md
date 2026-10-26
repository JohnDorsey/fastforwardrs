<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Phase 10 design: plugin editor windows (CLAP first, VST3 later)

Written 2026-10-24, before any code. **[V]** = I read it in the code; **[B]** = I believe it from the specifications and have not run it; **[D]** = you decided it. Nothing here is built.

## 0. What was asked [D]

Plugin editor windows, especially for CLAP plugins, which are supposed to have their own windows; what is built should carry over to VST3. And whether a LADSPA plugin can want a window of its own.

**LADSPA: no.** The specification has audio and control ports and nothing else: no window, no way to ask for one. The nearest things are DSSI (a LADSPA plugin plus a separate GUI program that talks to it over OSC; Linux only, and not kept up) and LV2 (which has real UIs). So a LADSPA plugin keeps the rows that the panel makes from its ports. If you meet a plugin that comes with its own GUI program, it is a DSSI plugin, and I would not build that.

## 1. What there is [V, read 2026-10-24]

* `crates/clap-host` uses `clack-host 0.2`; `clack-extensions 0.2` **has a `gui` extension** (host side: `is_api_supported`, `create` floating or embedded, `set_scale`, `get_size`, `can_resize`, `adjust_size`, `set_size`, `show`, `hide`, `destroy`; plugin callbacks `request_resize`, `request_show`, `request_hide`, `closed`; windows from `raw-window-handle` 0.5 and 0.6). The host does not use it (`C12` of `engine-phase-7-clap-design.md` said "not here").
* **Every plugin instance lives on one thread of its own, "clap main thread"**, which only waits for jobs (`main_thread.rs`). That is not the operating system's main thread and has no message loop. A plugin window made on it would not be pumped on Windows (it would freeze), and **on macOS a plugin window must be made on the real main thread**, which `eframe` owns.
* Parameters go to a plugin in `process` as events and never in a flush (`host.rs`: `request_flush` does nothing). Nothing reads the plugin's **output** events, so a change that a plugin makes to its own parameters (what its window does) is lost.
* Plugin **state** (`clap.state`) is detected (`has_state`) and not used; the scan already says, for a plugin that has state and two parameters or fewer, "it keeps settings that only its own window can change, which cannot be shown here". Nothing of a plugin is saved in the project but its parameter values.
* A plugin's audio instances are made by the sound engine for the preview and the export, from the project's values. A project has **no** place for anything but values.
* `fastforwardrs-plugin-scan` is already a program of its own that loads a plugin library so that a crash does not take the editor down; the trust model asks about each folder before the editor loads a library into itself.

## 2. What I propose [P]

| | decision | why |
|---|---|---|
| **P1** | **The editor window lives in a helper program, not in the editor**: a new program `fastforwardrs-plugin-editor`, one process for each window that is open. It loads the plugin library, makes **its own instance** of the plugin (the *editor instance*, which is never run for sound), makes a window with `winit` on its own main thread, and puts the plugin's window in it (embedded; floating if that is all the plugin offers). | **A plugin's window is the part of a plugin that crashes most, and it is code from a stranger.** In a helper, it cannot take the project down (the unsaved edits). The operating system's main-thread rules (Windows' message loop, macOS's main thread) hold in the helper, whose main thread is the plugin's main thread, so **one design is right on all three systems** and the editor's own `clap main thread` is not touched. It is what the scan already does for loading. |
| **P2** | **The sound is still made by the instances the sound engine makes; the window talks to it through the project.** What the window changes arrives as **parameter changes** (`begin`, `value`, `end` of a gesture) and becomes **the command that the panel's control would have made** (`SetEffectValue`; a gesture is one undo step, as a drag of a slider is; a value that has keyframes gets its keyframe at the playhead, as it does for the panel). What the project says reaches the window as parameter events (the panel, undo, the keyframes while the project plays, throttled). | **No new path to the sound**, no second source of truth, undo and keyframes work for free, and the panel and the window show the same values. It is also exactly what a VST3 editor needs (`beginEdit`, `performEdit`, `endEdit`). |
| **P3** | **A plugin's state that is not parameters is kept in the project** (file format 19: an effect may have `state`, the bytes that `clap.state` saves, in base 64). When the plugin says its state changed (`host.state.mark_dirty`) the helper saves it and sends it up; it becomes one undoable edit; the sound engine loads it into the instances it makes, before the parameters are set. When the editor saves a state it also sets every parameter value in the project from the editor instance, so that the two do not disagree. | **Without this a sampler, a convolver or a plugin whose window has its own curves shows a window whose changes do not reach the sound.** It is the only part that changes the project file. |
| **P4** | **One protocol between the editor and the helper**: lines of JSON on the helper's standard input and output (as the scan's answer is). Editor to helper: `open` (library, plugin id, window title, scale, current values, state), `value` (id, value), `state`, `close`. Helper to editor: `ready` (size, can resize), `edit` (id, value, `begin`/`value`/`end`), `state` (bytes), `closed`, `failed` (why). **Nothing in it names CLAP**: a plugin is "a library, an id and a kind". | So VST3 is a second *kind* in the helper (IPlugView, IComponentHandler, getState/setState), and not a second design. |
| **P5** | **The window is a button on the box of a plugin effect**: *Open editor* (in the heading's menu and as a small button in the box, only for a plugin that has a window; the box is otherwise as it is). One window for each effect at a time; it closes when the effect is deleted, the library is no longer trusted, or the project is closed; what the helper says is shown in the box if it fails ("The plugin's window could not be opened: ..."). | The one new thing a person sees, and it is what you asked for. |
| **P6** | **Only for a library in a folder that has been said yes to**, as now: the editor starts the helper only for a plugin it would run itself. The helper runs the same breadcrumb and the same start check. | The trust model is not widened. |
| **P7** | **Headless for the tests**: `--headless` makes the helper skip the window and make the plugin's window floating-without-a-window (the test plugin does not draw), so the protocol is tested in CI where there is no display (Ubuntu). A **test plugin with a window** is added to `crates/clap-test-plugins`: it supports `clap.gui`, and when it is shown it changes one parameter once (a gesture) and says its state changed once. | CI cannot make windows on Linux. |

## 3. Pieces [P]

| | piece | ends with |
|---|---|---|
| **10.0** | this design | your word on the questions below |
| **10.1** | `clap-host`: an **editor session** (instance made for a window; the `gui` calls; `params.flush` and the output events with gestures; `request_flush` that does something; `host.state` and saving and loading the state); the test plugin with a window. In-process tests with the test plugin. | green; committed alone |
| **10.2** | the helper program and the protocol: `winit` window, embedding, scale, resize both ways, closing both ways, the crash of the plugin or of the helper; tests through the real helper, headless. | green |
| **10.3** | the editor's side: the sessions, the button, the commands from edits, the values and the state to the window, the rules for closing, the note in the box. **A first real look at a real plugin's window**: the bundled plugins that have one (I can open one and check that the window was made, its size, that an edit arrives; I cannot judge how it looks). | green; I say what I saw and what I could not |
| **10.4** | **state in the project** (P3; file format 19): saved, loaded into the sound engine's instances, undo, the scan's note taken out for plugins that now can. | green |
| **10.5** | planted mistakes, documents, report. | the report |

**Size: L**, the largest piece of plugin work since the sound engine (about the size of the CLAP host plus the sound it makes). **Riskiest: 10.2 and 10.3** (embedding a stranger's window in a window of another process, per system). I suggest **high effort**.

## 4. What this does not do

* **A window of a plugin inside the editor's own window** (docked). A window of another process cannot be put in ours on macOS, and across processes on the other two it is fragile. The windows are windows of their own.
* **A window for a LADSPA plugin** (there is none to ask for), **for VST3** (the second kind, after this; P4 is what it will use), or **presets** of a plugin (its own window has its own).
* **macOS and Linux are written for and not tried**: the helper is the same program on all three and I can only run it on Windows (CI has no screen for a window). [B] for the other two.
* **A plugin that draws its window with a graphics API that conflicts with the helper's** is a plugin's problem; the helper uses the system's window and nothing else.

## 5. What I will not know until it is built [B]

* Whether the plugins you have with windows offer **embedded** windows on Windows (some JUCE-based CLAP plugins do only embedded, some only floating; the helper does both, and I have not seen how the bundled ones behave).
* Whether **`winit` 0.30's child-window and scale handling** is what plugins expect (a plugin gets the system's window handle and the scale factor, and does the rest).
* How long a gesture's round trip takes (a knob in the helper, through two pipes and the editor's command, to the sound): I expect a few milliseconds, and will measure.
