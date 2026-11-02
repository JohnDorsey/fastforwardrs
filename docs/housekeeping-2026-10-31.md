# Housekeeping, 2026-10-31

*Labels: **[V]** = I ran it and saw it; **[B]** = I believe it from reading, and could not run it. Every number is from this day unless it says otherwise.*

## 1. What was asked

After the plugin manager's tabs (`2fcc5d3`, CI run 172 green) the maintainer went away for hours: *"use this time to do whatever housekeeping you would like to do. Be bold, we can always revert changes that don't work out."* This is what was done, in the order of what it was worth, and what was found that is not for this repository to fix.

## 2. CI: from 48 to about 21 minutes [V]

The jobs and their steps can be read without signing in to GitHub through its public API (`/actions/runs/ID/jobs`: each step's start and end; `/check-runs/JOB/annotations`: the "what ran" notices, with the time of every program of tests). A run, in minutes:

| Run | What | In all | Windows | Ubuntu | macOS |
|---|---|---|---|---|---|
| 172 (`2fcc5d3`) | one job for each system | **48.3** | 48.2 | 21.7 | 25.9 |
| 173 (`e9ef457`) | Windows in three jobs side by side | **24.8** | 14.5 / 13.9 / 18.3 | 24.7 | 22.1 |
| 175 (`0415db5`) | Ubuntu and macOS too (unit tests, integration tests), the halves of Windows rebalanced | **21.4** | 9.0 / 16.0 / 21.3 | 13.2 / 14.8 | 18.0 / 7.8 |
| 177 (`444b9de`) | the same, after the experiment of section 3 was taken out (a cold cache: every dependency built again) | **22.8** | 12.3 / 12.7 / 22.8 | 12.8 / 16.4 | 20.2 / 6.9 |

* **How it is cut.** Every system is a job of unit tests (format, clippy, the library and host tests, and the downloads from the plugins' authors) and one or two of integration tests. `tools/ci/select_integration_tests.sh` says which programs of `tests/` a job runs: all, or a *first* half named in the script, or a *second* half that is **every other program**, so a program that is added later is run and never left out. `tools/ci/test_select_integration_tests.py` checks that the halves are every program between them and none twice, that the four programs with steps of their own are the same in the script and in `ci.yml`, and that every system has the right jobs (9 planted mistakes: all noticed). Every step runs the same command as before; `if:` says which job runs it.
* **Runs vary by about three minutes** with the runners (the first half of Windows took 16.0 minutes in run 175 and 12.7 in run 177 for the same programs), so the halves were not tuned further.
* **A bug in the notice** (found by this): `tools/ci/run_tests_and_report.sh` wrote the result lines of the first 30 programs of tests and there are 50, so 20 programs' times were written nowhere that can be read without an account. Regression test added; it failed first (30 of 60).
* **A mistake of mine, corrected [V].** From that cut-off notice I first concluded that Windows spent about 12 of its 32 integration minutes *compiling*. With the whole notice: **the compile of an integration job is 1.6 to 2.8 minutes, and test execution is what takes the time** (Windows, 50 programs: 1,503 s in run 173; `media_export` 332 s, `application_journeys` 197, `sound_plugins_clap_in_the_sound` 168, `media_export_tracks` 156, `media_export_effects` 135, `midi_in_the_sound` 87). The balance of the halves was made from those times.
* `actions/checkout` and `actions/setup-python` are at v7 (the Node 20 notices of run 172 are gone).

## 3. The Windows runners got slower, and it is not this repository [V]

From run 174 on, the programs of integration tests that **start helper programs** took 40 to 75 % longer on Windows than in run 173, and the programs that only export did not (seconds):

| | run 173 | 174 | 175 | 176 |
|---|---|---|---|---|
| `application_journeys` | 197 | 263 | 250 | 259 |
| `sound_plugins_clap_in_the_sound` | 168 | 272 | 241 | 264 |
| `plugin_editor_journey` | 39 | 62 | 58 | 62 |
| `plugin_scan_clap` | 19 | 34 | 32 | 29 |
| `media_export` | 332 | 347 | 352 | 353 |

Run 176 put CI back as it was in run 173 (`CARGO_PROFILE_DEV_DEBUG=2`, see section 4) and was as slow as the others; on the maintainer's computer the same programs run as in run 173 under either setting (section 4); **Ubuntu did not change** (623 s of tests in run 173, 594 s in run 175). So the cause is outside the repository, and **[B]** is a change of the Windows runners after 17:52 (starting a program costs more; in the same runs the library tests of Windows got faster, 11.7 to 6.7 minutes). Nothing was changed for it.

## 4. Debug information, and what did not help [V]

The build folder had grown to **54 GB** (`target/debug`), 37 GB of it `.pdb` files (about 230 MB for each program that is linked). Measured on this Windows PC, for one integration test with everything that it needs, the best of two runs (the PC was shared with another session, so single runs varied by a factor of 2; the sizes do not vary):

| `[profile.dev] debug =` | `.pdb` of the test | build folder | from nothing | again after a change of the test's file |
|---|---|---|---|---|
| full (what it was) | 359 MB | 6.9 GB | 127 s | 3.8 s |
| `"line-tables-only"` (now) | 105 MB | 3.2 GB | 70 s | 2.4 s |
| none | 32 MB | 2.3 GB | 68 s | 2.7 s |

The old folder was cleaned (`cargo clean --profile dev`: 54,273 files, 66 GiB, 15 s) and **everything was built again in 108 s (`cargo test --workspace --no-run`): 9.5 GB**. A backtrace still has a file and line for every frame. A debugger that shows the values of variables needs `CARGO_PROFILE_DEV_DEBUG=2`. With the setting: 6,079 library tests and the 482 tests of the 50 integration programs pass here. It does **not** make the Windows tests of section 3 slower: `plugin_scan_clap` 18.5 s against 18.8, `plugin_editor_journey` 42 against 43, `sound_plugins_clap_in_the_sound` 150 against 154 (two rounds, alternating, never slower).

Tried and **not** worth it: `rust-lld` as the linker (2.3 s against 2.4 s after a change); compiling the dependencies with `opt-level = 2` (the library suite 479 s against 436 s: 9 %). The suite is a flat tail of heavy tests (the slowest 10 are 5 % of the time, the slowest 500 are 74 %), so there is no single test to mend.

## 5. The minimum Rust version was not true [V]

The locked tree needs **Rust 1.95** (egui, epaint, emath, egui-wgpu, egui_kittest and others declare it; 16 packages need more than 1.88), and cargo refuses to build a dependency that needs a newer compiler than the one that runs: "Rust 1.88 or newer to build it" in the README could not be done. Worse, the resolver limits itself to the lowest `rust-version` of the workspace, so `cargo update --dry-run` proposed to *downgrade* `ordered-float`, `smol_str` and `uuid` to fit 1.88. The seven manifests, the README and the developer readme say 1.95 now, and it proposes no downgrade. (52 patch and minor updates are available and **not applied**: nothing forces them, CI's advisory check is green, and `zip` carries the project file format.)

## 6. Dead code, tests, documents

* **Four functions that nothing calls**, found by counting each `pub` item's name over all the Rust files (the names are long and spelled out, so a name that occurs once is unused): 4 of 4,816 `pub` items. **Two dependencies that no code names**: `pathdiff`, and `serde_json` as a dev-dependency of `ladspa-host`; `THIRD_PARTY.md` was made again by `cargo about` 0.9.2, which first made the committed file byte for byte.
* **Six test modules of 600 to 1,000 lines** that were inside their files (`placement`, `drawers`, `plan`, `keyboard`, `source_probe`, `frame_stream`) are in `<module>/tests.rs` like 270 others. The sorted list of the 5,926 test names of the library is identical before and after, and each moved body equals the old one but for whitespace.
* **Planted mistakes over the plugin manager's work** (`tools/mutation_testing/mutations_plugin_manager_tabs.py`): 15 of 18 were noticed at first. **Two were real gaps in tests written that morning**: the tests compared the text with the program's own `LEAST_CONTRAST`, so a constant made 3.0 got through (they compare with 4.5, the number of the guidelines, now); and the fallback to the first tab when the chosen tab goes away was never tried (a test now). The third is *equivalent* (egui's weak colour is the text colour at 60 % alpha, and `quieter_text_colour` starts by `to_opaque()`): left out of the list, with the reason as a comment. The first run of the campaign was refused by its own controls, because I had started it at low priority while building in the same tree; run alone it took 611 s.
* **Surge XT.** The documents said that the cause of the crash was not known and offered to write a report to the Surge team. The fork of the phase 14 work found it: an array that holds the output of each LFO is never given a value, and it is **fixed in Surge's own source since 2025-05-27, not in the release 1.3.4 that is bundled**. `bundleable-synthesizers.md` has a section for it (what was measured, what is only read), and the comment of the silent first block says when it can go: **when the Surge XT that the *Bundles* tab fetches is a release that has the fix.**
* **A leak of test folders on Windows [V].** `%TEMP%` held 3,525 `.tmp*` folders (958 MB, since 2026-10-26), 3,368 of them with a `test.lv2` bundle in them: `lv2_test_plugins::write_bundle` copies the test library into a fresh temporary folder, the host never unloads a library (on purpose: some crash when they are), and Windows cannot delete a library file that is loaded, so the cleanup of the `TempDir` fails and says nothing: about 65 bundles for each full run of the tests. **Not fixed** (the 14 callers would have to share a folder, or a run would have to clean up what the one before left). The folders older than 3 hours that held only a bundle were removed, and so were 13.7 GB of work copies of an old mutation campaign (`%TEMP%\fastforwardrs-mutation-workers`; `mutate.py --clean` is what removes them).
* **Git.** `git status` listed two files of `crates/clap-host/` as modified with an empty `git diff`: only their line endings had changed on disk, and git trusts a different file *size* without looking at the content. `git add` of the two cleared it and staged nothing.
* No broken relative link in the 56 `.md` files; no TODO or FIXME in the code; 30 `#[allow]`, 20 of them `unsafe_code` in the host crates.

## 7. What is left for the maintainer

* ~~`tests/surge_xt_in_a_process_real.rs` still has the fork's uncommitted knobs (`SURGE_SCENE_MODE`, `SURGE_PROGRAM`)~~ **discarded on 2026-11-01** (the maintainer's word: the crash is fixed upstream, so the experiment is over); the diff was kept outside the repository.
* The 52 dependency updates (section 5), if wanted.
* The leak of test bundles (section 6): `.tmp*` folders in `%TEMP%` that hold a `test.lv2` can be deleted when no test is running.
* The Windows runners (section 3): nothing to change here; if the times matter, a later run will show whether they came back.
* Instruments still run in programs of their own and the other plugins in the editor (`THE_PLUGINS_IN_A_PROCESS_OF_THEIR_OWN`): not changed.
