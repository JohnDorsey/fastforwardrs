# Phase 10 report: windows of plugins (CLAP)

*Written 2026-10-26, after piece 10.4. The design is [`engine-phase-10-editors-design.md`](engine-phase-10-editors-design.md) (what was asked, the decisions, and in its section 6 what was built). [V] is something I ran or read in the real program; [B] is what I believe from reading the code.*

## 1. What a person can do now

* A **CLAP plugin that has a window** (the scan says so: `has_a_window`, kept in the scan cache, scan format 4) has a button in its effect's box, **Open editor**, which becomes **Close editor** while the window is open. Nothing is shown for a plugin that has none, and there is no always-visible control. A LADSPA plugin has no window to ask for, so there is nothing to do for it [V: the standard has none].
* The window is **a window of its own** (not docked), made by another program, `fastforwardrs-plugin-editor`, that the editor starts for each window. It is in the same package, so `cargo build` builds it, it is found beside the editor, and it must be of the same build (the editor asks it and refuses another, saying why in the box). A plugin that crashes in its window ends that program and not the editor; the box says so [V: a test plugin that aborts].
* **Only from a folder that has been said yes to**, as the sound is: the window loads the library too.
* **What a hand does in the window is an edit of the project**, the same as the panel's control for the same value: one step of the history for one gesture (a drag is begin, values, end), keyframes at the playhead, undo and redo. **What the project says reaches the window** (a change in the panel, an undo, keyframes while the project plays), except the parameter a hand is holding.
* **What the plugin keeps that is not a parameter (its state) is kept in the project** (file format 19, `state`, base 64), is given to the plugin in the sound before its values, and to the window when it is opened and when an undo or redo brings back another. A preset loaded in the window, or a setting that only the window has, is not lost when the project is saved and opened again [V for the test plugin; B for real plugins other than the ones in 3].

## 2. What was built, in pieces (all pushed)

| piece | commit | what |
|---|---|---|
| 10.0 | `ec89593` (design, with 10.1) | the design: three decisions the maintainer agreed to (helper process; separate windows; state in the project) |
| 10.1 | `ec89593` | `clap_host::Editor`: an instance that is never run for sound, with `clap.gui`, `clap.state`, `clap.params` flush on an inactive instance, `host.gui`, `host.state`; the test plugin that has a window |
| 10.2 | `801ad59` | the helper program and its protocol (JSON lines, prefix `ffrs-editor: `, `--headless` for CI); winit window; follows a plugin that changes its size by itself |
| 10.3 | `089aa3f` | the button; `plugin_editor::sessions`; edits become `SetEffectValue`; the project told to the window; closing rules |
| 10.4 | `c6a7322` | the state in the project: model, file format 19, the sound, the window, history |

## 3. What I verified, and how

* **Real plugin windows** [V, 10.2, by eye on Windows]: Surge XT FX, Dragonfly Hall and MVerb were embedded correctly. Surge XT applies its zoom after it is shown without asking to resize; the helper polls the size the plugin says and follows it (a fix found by that look).
* **The loop app <-> helper** [V, headless, CI]: `tests/plugin_editor_journey.rs` (5) drives the real editor window with the real helper and the test plugin: the plugin's edit becomes one step; undo and redo reach the window; the button closes; taking the effect off closes it; another project closes it; the folder that nobody said yes to is refused in the box and opens when someone does; **the state is kept with the edit (one undo takes both), is given to a window that is opened again, and an undo that brings back an older state gives it to the open window without ending what can be redone**.
* **Tests of the pieces**: clap-host 47, `plugin_editor_program` 15, `plugin_editor_sessions` 14, journeys 5, the library 5400 (it was 5347 after phase 9).
* **Planted mistakes** (the tool in `tools/mutation_testing`): 10.1 and 10.2 earlier; 10.3 37, of which 6 survived and got tests; 10.4 43, of which 4 survived (3 got tests; the fourth is equivalent: the flag of a step that an undo pushes on the redo stack is never read).
* **CI**: 10.1 and 10.2 green on Windows, Ubuntu, macOS, licences and tools. **10.3 failed on all three systems** for two faults of my tests, not of the program: a click on "Open editor" before the box was drawn again (it passed on my machine by luck of timing) and a refusal test that ran `/bin/true`, which macOS does not have. Both fixed in the 10.4 commit; CI of that commit is [read it in the state note].

## 4. What was found on the way (and is now different)

* **An edit that nobody made ends what can be redone.** The first version kept every state a window said, including the answer to the program's own asking after an undo: that was a new edit, and a redo was gone. A state is now kept only when the plugin said that it changed (and is asked for, not more than every 150 ms), the helper drops a change that a plugin says while it is being given a state, the editor ignores one that comes a moment after it told the window something, and a state that is the same as the effect's is not an edit. A journey (undo, ask, redo) pins it, and the test plugin now says "state changed" when it is given a state, as many plugins do.
* **A state rides on the step before it.** The state arrives a moment after the value that made the plugin say so; `History::record_rider` joins it to the latest step if that was edited less than a second ago (or a hand is on a window), and makes a step of its own if not; never to a step that was undone to.
* **The sound's signature is asked at every frame while playing**, so the state's content is a fingerprint worked out once when it is given, not hashed again at every frame (a state can be megabytes).
* **The helper says a state that changed when it ends**, whoever ends it (the button, the window's close box, the editor going away), so that the last thing a person did is kept.
* Tests that had to change on purpose: the version guard (18 to 19), `editor_tests` (a state given to the test plugin now makes it say that its state changed), three `plugin_editor_program` tests (a state is said before `Closed`), and the key of an unknown field in a plugin test (`state` is a known field now; the test says `settings`).

## 5. Two timing tests failed on this machine, before phase 10 as well (2026-10-26)

`tests/application_playback.rs::playing_after_jumping_into_the_middle...` (it wanted more than 100 frames drawn in about 2.5 s and got 86 to 99) and `tests/application_sound.rs::dragging_a_volume_slider_while_playing...` (a gap in the sound where it is made again: 0.020 to 0.022 s, once 0.074 and once 0.178; it wanted less than 0.02). **I built the commits of phase 9 (`a0e1021`) and of 10.3 (`089aa3f`) in separate copies and ran both tests there on this machine [V]: the playback test failed 3 of 3 runs at `a0e1021` (91, 86, 94 frames), and the slider test 1 of 4 there and 2 of 4 at `089aa3f`**; so phase 10 did not do it. The maintainer said to ease the limits unless the speed could be improved in a sensible way.

* **The sound: a real defect, found and fixed.** I printed what happens at each take-over of a new sound for the old (the hot swap, `AudioSupplier::take_over_if_the_replacement_has_sound`) [V]: the new sound is always late (never early), so no silence is put in; but **after the join what waits at the speakers is one chunk of the new sound, about 2290 moments (47 ms), where the target is 0.3 s, and it ran to zero 15 to 26 times in a run**: the next frame of the window refills it, and a frame that is late (a slow machine, a debug build with frames of 25 ms) lets the speakers run dry, which is the gap. The fix is **to fill the speakers from the new stream in the very frame in which it takes over** (`update`, 6 lines). A test with the scripted streams reproduced it (2288 moments waiting where 14400 were wanted) and passes now. **With the fix the slider test passed 54 of 54 runs at the original limit of 0.02 s** (there was one gap of 0.071 s in 67 runs, in a batch with the fix, on a machine that may have been busy), against about half before; so **the limit of the sound tests stays 0.02 s**.
* **The playback test: its limit is eased.** The count is how many frames a debug build of the whole window, drawn by a test harness, makes in 2.5 s (about 25 ms each): it says how fast the machine is, and is not a speed that matters to the program as it is used; I did not profile it. **The limit is 60 frames now** (it was 100), with the reason in a comment. Both pass here, 3 runs of 3.

## 6. What I did not do, and what I do not know

* **macOS and Linux windows** are written for and not tried [B]: the helper is one program for all three, `raw-window-handle` gives the system's handle, but I could only look at Windows, and CI has no screen (it runs `--headless`).
* **The window is not docked** (decision 2).
* **The windowed (not headless) path in CI** has not run: it needs a screen. I ran it by hand on Windows for the three plugins in section 3.
* **A real run of the whole app with a real plugin window** by hand, after 10.3 and 10.4, I have not done [B for the combination]: what the journeys prove is the real helper with the test plugin.
* **A plugin that says its state changed some time after it was given one** (not while it is given) would make an edit nobody made, and one step of the history, once. The helper handles "while being given"; the editor's moment of 500 ms handles values it told; anything else I cannot know without the plugin.
* **A state of any size is kept** (the 16 MiB limit that 10.4 had was taken away 2026-10-26 at the maintainer's word). A sampler's state, with its samples in it, makes a project file as big as the base 64 of it (a third bigger than the bytes), is hashed once when it is given, and goes through the pipe to the window as one line: a state of 24 MiB was given to the real helper in a test in a few seconds [V]; I have not tried a state of hundreds of megabytes [B], and the steps of the history share the one state (it is an `Arc`), and do not copy it.
* **VST3 and Nyquist** are not done. VST3 would be a second kind in the same helper (the design's P4 is what it uses); Nyquist is an effect model of its own (it renders ahead and offline), not a window.

## 7. What is open for the maintainer

1. **Look at a real window of a plugin you use** (the three I tried are free): open an effect, move a knob, load a preset in the window, close the window, save, open the project again and open the window: the preset should be there. Say which plugin behaves oddly.
2. **LV2 next** (asked 2026-10-26; VST3 later or through Carla): see `engine-phase-11-lv2-design.md`.
