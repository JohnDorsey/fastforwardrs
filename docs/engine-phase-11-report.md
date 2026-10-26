# Engine phase 11 report: LV2 plugins and their windows (2026-10-26)

*Written after the last piece (11.5). Labels: [V] = I ran it and saw it; [B] = I believe it from reading, and could not run it. The design is [`engine-phase-11-lv2-design.md`](engine-phase-11-lv2-design.md) (sections 7, 8 and 10 say what was built).*

## 1. What was asked

LV2 plugins after the CLAP ones (VST3 later, or through Carla), the 16 MiB limit on a plugin's state taken away, the timing limits of the sound tests eased unless the sound could be made better, and then **windows for LV2 plugins**.

## 2. What exists

| Piece | What | Commit |
|---|---|---|
| (before) | no limit on the state of a plugin; the sound is filled again from a new stream in the very frame that it takes over (the real cause of the gaps that the slider test showed: **a defect of the program, fixed**, with the limit of that test unchanged at 0.02 s); one playback test's frame count eased from 100 to 60 | `d5226b0` |
| 11.1 | **a host of our own for LV2, in Rust, with no `lilv`** (`crates/lv2-host`): the Turtle of a bundle is read (`oxttl`), the library loaded, a plugin made with the features it needs, run, its state saved and restored; **and a library of plugins for the tests** (`crates/lv2-test-plugins`: nine plugins that do arithmetic and misbehave on request) | `81e4ba4` |
| 11.2 | the **scan** reads bundles (in the scan program, so a library that crashes costs that program), the plugins are **effects** (key, values from the control ports, latency), the places that bundles are looked for, the plugin manager lists them | `2fe4b37` |
| 11.3 | the plugins **run in the sound**, the preview and the export; the project's state is given to them | `2fe4b37` |
| 11.4 | the state of a plugin is saved and restored (done inside 11.3); **nothing in the editor makes one** (a window that does would need atoms: see 4) | |
| 11.5 | **windows for LV2 plugins** | the commit after this report |
| 11.6 | this report | |

## 3. What was checked

* **[V] The whole suite, at the end of 11.5: 5972 passed, 0 failed, 45 ignored; clippy with `-D warnings`, `rustfmt` and `rustdoc` clean.** The 45 ignored are the tests that need a person (a real card, a window, a download), as before; one of them is new: `look_at_the_window_of_the_test_interface` opens the real window for a person to look at.
* **[V] Planted mistakes:** 56 in the host (11.1: all noticed but one that does the same thing as the code it replaces), 44 in the scan and the sound (11.2 and 11.3: all noticed but one of the same kind), **66 in the windows (11.5: all noticed in the end; two needed tests: what a window does after it was taken down, and the test that says which class of interface is this system's, which must say it by itself and not by asking the program that it is testing)**.
* **[V] A real window, on Windows.** The program made its own window, the test interface made a child window in it, the size that the system says (380 by 250) was not the size that the interface had asked for (360 by 240) and is the one that was used, and I looked at it on the screen: the window, titled with the plugin's URI, with the child window and its line of text in it. A test does the same without a person and is part of the suite (on Windows only).
* **[V] CI:** `2fe4b37` green on Windows, Ubuntu, macOS, licences and tools. `81e4ba4` failed once on Windows in a test of the chase of the lead-up that has nothing to do with this phase (a timing test that did not change; the next commit passed). The CI of the commit of 11.5 is [read it in the state note].
* **A mistake of mine in the process, said plainly:** the first run of the campaign of 11.5 was started twice (the first launch looked dead and was not), the two runs shared their work folders, and the result was nonsense (tests that "failed once and passed alone", anchors that were "not found"). **I threw that result away**, started one run alone, and that gave the numbers above. The note about it is in the state file.

## 4. What the maintainer should know (decisions I made)

1. **A host of our own, not `lilv`.** `lilv` is a C library that has to be built or installed on each system; `lilv-sys` links the one on the system, and there is none on Windows. The price is that I wrote the part of the standard that this host uses from the headers, and **a plugin that is made with a mistake of mine in reading them could pass my tests** (the test plugins were written from the same reading): see 5.
2. **`log:log` is not given** (its function takes a format and a variable number of arguments, which stable Rust cannot write), so a plugin that *requires* it is not made, and the plugin manager says so. Most plugins that ask for it only like it.
3. **The window of an LV2 plugin does less than a CLAP window**, and the guide says how: the plugin is **not run** in the program that shows the window (the sound is made elsewhere), so **a meter or any control output does not move**; **an interface that talks in atoms (what a sampler's file chooser or a preset list does) is shown, and what it says in atoms is thrown away**, so it cannot change the plugin's state; Gtk and Qt interfaces are not shown (a plugin that has only those has no button and has the panel's controls).
4. **What is the trust:** the interface's binary is in the bundle, and the bundle is loaded only if the folder that it is in has been said yes to, as for the sound. The window is the same program as for CLAP (`fastforwardrs-plugin-editor --kind lv2`), so a window that crashes does not take the editor with it.

## 5. What is open for the maintainer

* **Try a real LV2 plugin with a window** (Dragonfly Reverb and Surge XT ship LV2 builds; none is installed on this machine). *This is the one test that matters most [B]*: if a real plugin does not load or its window is wrong, it will be a place where my reading of the headers is not what the plugin expects. Tell me which plugin and what happened.
* macOS and Linux windows are written and **never run** (the size there comes from `ui:resize` alone; CI has no screen, so what runs there is the headless path).
* VST3: later, or through Carla's LV2 plugin (not tried; its window and its state would need what 3 above says is not done).
* Small things left: `ui:showInterface` (a window of its own), `ui:requestValue`, atoms (and with them a state that a window makes), a plugin that runs in the window's program (for meters), a change of the screen's scale while a window is open.
