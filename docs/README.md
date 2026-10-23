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
| [`bundleable-isf-shaders.md`](bundleable-isf-shaders.md) | which ISF shaders can be fetched by the Bundles tab and why the others are left out |

## Design, plans and reports

| File | What is in it |
|---|---|
| [`long-term-plan-plugins-and-gpu-engine.md`](long-term-plan-plugins-and-gpu-engine.md) | the plan for the picture engine and the plugin system |
| [`plugin-system-features.md`](plugin-system-features.md), [`plugin-system-design-notes.md`](plugin-system-design-notes.md), [`plugin-system-plan-of-action.md`](plugin-system-plan-of-action.md), [`plugin-system-spike-findings.md`](plugin-system-spike-findings.md) | the feature list with its numbered requirements, the notes, the phases, and what the spikes found |
| [`engine-phase-1-design.md`](engine-phase-1-design.md) to [`engine-phase-6-report.md`](engine-phase-6-report.md) | the picture engine, phase by phase (design and report of each; phase 6, the ISF host, has the designs [`engine-phase-6-design.md`](engine-phase-6-design.md), [`engine-phase-6-4-design.md`](engine-phase-6-4-design.md) and [`engine-phase-6-6-design.md`](engine-phase-6-6-design.md)); [`engine-comparison*`](engine-comparison) holds the pictures that compare old and new |
| [`engine-phase-7-design.md`](engine-phase-7-design.md), [`engine-phase-7-report.md`](engine-phase-7-report.md) | the sound engine and LADSPA plugins |
| [`engine-phase-7-clap-design.md`](engine-phase-7-clap-design.md), [`engine-phase-7-clap-report.md`](engine-phase-7-clap-report.md) | CLAP plugins |
| [`engine-phase-9-design.md`](engine-phase-9-design.md) | phase 9, text objects become generators (the design; nothing built yet) |
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
