# The documents of fastforwardrs

The top-level [`README.md`](../README.md) is for someone who wants to install the program and use it. These are the rest.

## What the program does and how to use it

| File | What is in it |
|---|---|
| [`using-the-editor-in-detail.md`](using-the-editor-in-detail.md) | every menu, control, key and drag, with the rules behind each (tracks, clips, keyframes, racks, drawers, groups) |
| [`effects-and-shaders-in-detail.md`](effects-and-shaders-in-detail.md) | what an effect is, the effects there are, look-up tables, shaders (ISF: filters, generators, passes), where the files that effects use are kept |
| [`settings-colours-and-export.md`](settings-colours-and-export.md) | each setting, how colours are read and mixed, what the exported video is |
| [`project-file-format.md`](project-file-format.md) | the `.ffrs` and `.json` project files: layout, an example (it is run by a test), every version |
| [`plugins-and-trust.md`](plugins-and-trust.md) | sound plugins: who may run them, how they are found, how they run in the sound, the check of a plugin's start, the plugins that come with the program |

## For whoever builds it

| File | What is in it |
|---|---|
| [`developer-readme.md`](developer-readme.md) | where it was tested, how to run it from source, the layers of the code, how it is tested (mutation testing included), the known limits |
| [`distribution-notices.md`](distribution-notices.md) | what must go with a copy of the program: licence texts, ffmpeg's source offer, the bundled plugins |
| [`bundleable-clap-plugins.md`](bundleable-clap-plugins.md) | which CLAP plugins can be bundled and what was measured of them |
| [`bundleable-lv2-plugins.md`](bundleable-lv2-plugins.md) | which LV2 plugins the Bundles tab fetches, the ones left out, and what was tried with real plugins |
| [`bundleable-synthesizers.md`](bundleable-synthesizers.md) | the synthesizers the Bundles tab fetches (Surge XT, Kars, Nekobi), what trying them found in the program, the licence reading of Nekobi, and the synthesizers left out and why |
| [`bundleable-isf-shaders.md`](bundleable-isf-shaders.md) | which ISF shaders can be fetched by the Bundles tab and why the others are left out |

## Design, plans and reports

| File | What is in it |
|---|---|
| [`long-term-plan-plugins-and-gpu-engine.md`](long-term-plan-plugins-and-gpu-engine.md) | the plan for the picture engine and the plugin system |
| [`plugin-system-features.md`](plugin-system-features.md), [`plugin-system-design-notes.md`](plugin-system-design-notes.md), [`plugin-system-plan-of-action.md`](plugin-system-plan-of-action.md), [`plugin-system-spike-findings.md`](plugin-system-spike-findings.md) | the feature list with its numbered requirements, the notes, the phases, and what the spikes found |
| [`engine-phase-1-design.md`](engine-phase-1-design.md) to [`engine-phase-6-report.md`](engine-phase-6-report.md) | the picture engine, phase by phase (design and report of each; phase 6, the ISF host, has the designs [`engine-phase-6-design.md`](engine-phase-6-design.md), [`engine-phase-6-4-design.md`](engine-phase-6-4-design.md) and [`engine-phase-6-6-design.md`](engine-phase-6-6-design.md)); [`engine-comparison*`](engine-comparison) holds the pictures that compare old and new |
| [`engine-phase-7-design.md`](engine-phase-7-design.md), [`engine-phase-7-report.md`](engine-phase-7-report.md) | the sound engine and LADSPA plugins |
| [`engine-phase-7-clap-design.md`](engine-phase-7-clap-design.md), [`engine-phase-7-clap-report.md`](engine-phase-7-clap-report.md) | CLAP plugins |
| [`engine-phase-9-design.md`](engine-phase-9-design.md), [`engine-phase-9-report.md`](engine-phase-9-report.md) | phase 9, text objects are made of an effect (the design, with what was built in its sections 6 to 8; and the report): the last phase of the plan |
| [`engine-phase-11-lv2-design.md`](engine-phase-11-lv2-design.md), [`engine-phase-11-report.md`](engine-phase-11-report.md) | phase 11, LV2 plugins (a host of our own in Rust, no `lilv`; the scan of bundles, the sound, the state, windows): the design, with what was built in its sections 7, 8 and 10; and the report; [`engine-phase-11-presets-design.md`](engine-phase-11-presets-design.md): presets of LV2 plugins (what the real bundles have, the plan) |
| [`engine-phase-10-editors-design.md`](engine-phase-10-editors-design.md), [`engine-phase-10-report.md`](engine-phase-10-report.md) | phase 10, the windows of plugins (CLAP; a program of its own for each window, the state of a plugin kept in the project, file format 19): the design, with what was built in its section 6; and the report |
| [`engine-phase-12-design.md`](engine-phase-12-design.md), [`engine-phase-12-report.md`](engine-phase-12-report.md) | phase 12: effects that act on the sum of a track or a group, the sharing of a track (per clip, per run, per track with windows, per track indivisible; one setting for picture and one for sound), tails, the tail policies at the edge of a group, the edge fade, sound effects on groups: the design, with what was built in section 10 (**sound complete; for the picture only the sharing of state; ghost layers are not built**), and the report; `tools/group_cases/overlap_cases.py` enumerates the cases |
| [`engine-phase-13-midi-design.md`](engine-phase-13-midi-design.md) | phase 13: MIDI clips that feed synthesizer effects; a synthesizer consumes the notes that reach it unless a switch says otherwise; `.mid` import; the design, the answers, and an "as built" block for each piece |
| [`engine-phase-13-report.md`](engine-phase-13-report.md) | phase 13 report: what was asked, what exists, what was checked, what the maintainer should know, what is open |
| [`groups-feature-spec.md`](groups-feature-spec.md) | tags, clip groups and track groups |
| [`licence-plugin-clarification-draft.md`](licence-plugin-clarification-draft.md) | the options for the plugin clarification (`LICENSE-PLUGIN-CLARIFICATION` is the adopted one) |

## Where the old README's sections went

The documents above say "the README" and name a section. The README was split; the sections are in these files, as they were written.

| Section | Now in |
|---|---|
| *Status*, *Running it* (the picture engine), *How it is built*, *Testing*, *Known limits* | [`developer-readme.md`](developer-readme.md) |
| *Using it* (menus, window, keys, tracks, layers, keyframes, groups, sound, missing files) | [`using-the-editor-in-detail.md`](using-the-editor-in-detail.md) |
| *Using it*: effects, look-up tables, shaders | [`effects-and-shaders-in-detail.md`](effects-and-shaders-in-detail.md) |
| *Settings*, *Colours*, *Exported video* | [`settings-colours-and-export.md`](settings-colours-and-export.md) |
| *Project files* | [`project-file-format.md`](project-file-format.md) |
| *Plugins and who may run them* | [`plugins-and-trust.md`](plugins-and-trust.md) |
| *Licence* | [`README.md`](../README.md) |
