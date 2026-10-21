<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Phase 5: group effects that are exact, adjustment clips and generator clips (design)

Written 2026-10-21, **before the code** (the standing rule for the engine phases). It is the design of phase 5 of [the plan of action](plugin-system-plan-of-action.md): GR-02, GR-06 to GR-10, EF-02, EF-03 (the backdrop slot, first use), EF-04, Q-11, Q-33 of [the features list](plugin-system-features.md), and it ends the **placeholder** of [the groups specification](groups-feature-spec.md) (G-26).

Tags: **[V]** verified in the repository or by running something; **[D]** decided by the maintainer; **[P]** my proposal, which stands unless it is objected to; **[B]** my belief or estimate. Everything is run on **software adapters only** (SF-01).

## 0. What phase 5 is, in one paragraph

Today the effects of a clip group or a track group are **applied to each member's own picture** (a documented placeholder: the same picture only where the members are not see-through, off by up to about 100 of 255 levels at see-through edges, and wrong for a blur, which is why blur, crop and opacity are not offered to groups). Phase 5 makes the engine **draw a group into a picture of its own** (the composite of its members), run the group's effects on that picture, and then lay it over what is behind, which is what GR-02 says. It uses the same mechanism for two new kinds of clip: an **adjustment clip** (its effects apply to the composite of everything behind it, for its length: GR-09) and a **generator clip** (a clip with no media whose content is an effect instance, 5 seconds by default: EF-02, Q-11).

## 1. What is there today [V, read in the code, 2026-10-21]

* **The scene is flat** (`engine/scene.rs`): a `FrameDescription` has `layers` (back to front), each a `LayerSource` (a sent `Picture` or a `Solid` colour), four corners and a list of `EffectCall`s that run on **the layer's own picture** before it is put in its place. The plan (`engine/plan.rs`) is `[Effects(layer)…, PlaceLayers, Output]`; `begin_frame` (`engine/frame_renderer.rs`) records it. Everything is **premultiplied half floats**; the colour passes un-premultiply for themselves; `PlaceLayers` draws all the layers in one pass over opaque black.
* **The picture plan** (`composition/pictures.rs`) is sections of layers; a layer's `effects` are **those of the clip, its track, its tags where the badges are, and then its clip groups' and track groups' (the placeholder)**, from `Project::effects_applied_to_every_clip`, which the sound uses too. Layers behind the front-most layer that covers the picture are **dropped**.
* **The rules of groups guarantee a tree** [V, `project/groups/rules.rs`]: a clip is directly in one clip group at most, a clip group in one clip group, a track in one track group, a track group in one track group; **a clip group that shares tracks with a track group is inside it** (never across its edge); two track groups never partly overlap. So the layers of a section, back to front, are **contiguous for every group** that has any: a group is a **range** of the layers, ranges nest, and a clip group is always inside the track groups of its tracks.
* **A group's effects have no keyframes** [V, `layer_effect_of`]; a tag's effects are applied to each member where the badge is (exact, unchanged by this phase).
* **The preview and the export share** `layers_of_a_section` → `PreviewContent` → `describe_frame` → `FrameDescription` [V]; the window's old drawing (egui) can do colour mixes only.

## 2. Decisions

| # | Decision | Why | Tag |
|---|---|---|---|
| H1 | **A group is a layer whose source is a group** (`LayerSource::Group(i)`), and `FrameDescription` has `groups`, each a list of layers (`GroupDescription`). Group *i* may hold layers that are groups with an index **greater than *i***, so the scene has no cycles by construction and the groups are drawn from the last to the first. The frame's own layers may use any. A description that breaks that is refused (`RenderProblem::BadGroups`) before anything is drawn. A description without groups is **what it was**: `groups` is not written when empty and every test of phases 1 to 4 runs unmodified. | A group then has a picture like any other layer, and everything that a layer can have (effects, corners, tint) works on it with no new code. | [P] |
| H2 | **A group is drawn into a picture of its own the size of the frame**, cleared to **clear** (not black), by the same pass that draws the frame's layers (premultiplied over). Its effects run on that picture (the same passes as for an uploaded picture), and the group is put in its place with the corners of the whole frame and a white tint. A picture the size of the frame put in place over the whole frame is **not resampled** (a test holds it to the pixel). | The composite of the members, GR-02. | [P] |
| H3 | **A group with no effect that applies is not a node** (pass-through, Q-33): its members are drawn straight into what is behind. A group is a node if it has at least one **enabled, installed picture effect that applies to a group** after the chain is made (tags' effects at their badges included). Which effects apply to a group is the table `applies_to_a_group_of_kind`: **the `on_a_composite` restriction is lifted** (blur, crop and opacity are offered to clip groups and track groups; their tests are rewritten on purpose). | GR-10, Q-33 (the choice is hidden). | [P], [D] Q-33 |
| H4 | **The plan of a section has groups as ranges of its layers** (`PictureSection.groups`: owner, range, the static effects), built from the rules; the layers' own effects **no longer contain the effects of the groups that are nodes** (an effect is applied once). The sound is not touched (it goes on using `effects_applied_to_every_clip`, whose group effects are still the sound's placeholder). The preview content and `describe_frame` carry the ranges to the description. | One source for the preview and the export, as before (EN-03). | [P] |
| H5 | **The layers behind a covering layer are dropped inside their own group**, and a group covers the picture only if **its effects keep every pixel opaque where it was** (colour mixes, exposure and tables do; blur, crop and opacity do not) **and a layer of it covers the picture**. | The existing saving, kept where it is still true. | [P] |
| H6 | **The old drawing of the window leaves a group's effects out** (as it leaves out a blur: it can do colour mixes of a layer's own picture only). It is a fallback for a computer with no usable graphics adapter, and its words say so. | It cannot draw a group into a picture; the placeholder is the thing being removed. | [P] |
| H7 | **An adjustment clip** is a new kind of clip (`Adjustment`): no media, a length (5 s when made, like a picture), the chain of effects every clip has, no placement (it is the whole frame). In a section it is a layer at its track's place; **everything behind it in its own container becomes a group whose effects are the adjustment's** (at the frame's root the group begins with an opaque black layer, because black is what is behind the first layer; inside a group it is transparent). An adjustment with no effect that applies is nothing. | GR-09: an effect over the composite of what is behind, for the length of the clip, built from the same group mechanism and nothing else. | [D] GR-09, [P] |
| H8 | **A generator clip** is a new kind of clip (`Generator`): no media, a length (5 s when made, extendable by dragging: Q-11), and **its content is one effect instance of a generator kind**, with the chain of effects every clip has and a placement (so it can be moved, scaled and turned like a picture). The first generator is **Solid colour** (one value: the colour, with its opacity): the engine's `Solid` source, which exists. The others come with ISF (phase 6) and the text generator (phase 9). | EF-02, Q-11, ID-08 | [D] EF-02, Q-11, [P] |
| H9 | **File format 15**: the clip types `adjustment` and `generator` (the generator's kind, version and values as an effect snapshot), the file of 14 and before loads as it was, **an older program refuses a 15** with the "newer file" message. A fixture of 14 and one of 15 are kept. | MG-01 to MG-03. | [P] |
| H10 | **Making them** (the right-click menu of an empty place of a track, the *New* smaller menu where *Text object* is): **Adjustment clip** and **Generator > Solid colour**. They are selected when made and their properties show (a generator's value is a box made from its descriptor, as an effect's is). Nothing is added to the screen that was not asked for. | The rule about unrequested UI: a menu item is not always visible. | [P] |
| H11 | **Group effects stay without keyframes** (GR-06 says "animated with the same racks"; the model has none for groups and giving it some is a change of its own, which is not asked for here). | Keep the phase one thing. | [P] |

## 3. Pieces and their gates

Each is built behind tests that lock what was there, verified on WARP (and lavapipe on the CI), and is a commit that CI has read before the next begins where it can be.

| piece | what | gate |
|---|---|---|
| **5.1** | **The engine draws groups** (H1, H2): the scene (`groups`, `LayerSource::Group`, the fingerprint, serde with a default), the plan (groups first, from the last, each then its own effects passes), the renderer (one picture per group from the pool, cleared to clear), the refusal of a bad description | **every test of phases 1 to 4 passes unmodified**; a group with no effects drawn over black **is** the same frame as its layers drawn flat (to a level of the 255: half floats); a group with effects has them run on its composite (a colour change of two overlapping see-through layers differs from the same change on each layer, by the amount the arithmetic says); a blur of a group is a blur of the composite (two layers, one blur); groups in groups; a group used twice; a description that refers to a group that does not exist, or to itself, is refused; the passes are counted |
| **5.2** | **The plan makes groups** (H3 to H6): `effects_applied…` without the groups' effects for pictures, the ranges of a section, the pass-through rule, the drain per container, the preview content and `describe_frame`, `applies_to_a_group_of_kind` lifted | "**ungroup gives the same render**" (GR-07) and "**a group with no effects equals no group**" as tests, on the engine; the placeholder tests of `project/groups/effects_of_groups.rs` that have `placeholder` in their names are thrown away with it and the ones that stay are rewritten on purpose; preview equals export; a clip group inside a track group; a tag's effects at a badge of a group |
| **5.3** | **Adjustment clips** (H7, H9, H10): the clip kind (model, history, the timeline's drawing, the properties panel, selection, the file with format 15 and its fixtures), the layer, the group it makes | an adjustment over two clips changes both and not the clip above it; at the root it works over black; inside a clip group it works only on what is behind it in the group; its length; undo; a file round trip |
| **5.4** | **Generator clips** (H8 to H10): the kind, Solid colour, the properties box, the file, the engine's `Solid` | a generator is the colour over the frame; with a placement it is a rectangle; with an effect on it; extending it; a file round trip |
| **5.5** | conformance, the mutation campaign, the README, the features list, the plan and the report | as phase 4's 4.8: a conformance test for each of the new things; the campaign over the new code, with survivors closed |

## 4. The tests

* **The same picture as the flat description** is the oracle of 5.1 and 5.2 (what an extra level of groups must not change), as the old drawing was the oracle of phase 1.
* **The arithmetic of a composite** is worked out by hand in the tests (two see-through layers, a colour matrix: the effect of the composite against the effect of each layer, which is the number the placeholder documents).
* **The conformance tests** of phase 4 (defaults change nothing, keyframes at every frame) are run for a group's effects too.

## 5. What this does not change, and what is not exact

* **The sound** keeps its placeholder for groups (the effects of a group's sound plugins are not built).
* **Blend modes** are not in the model (GR-10 names them): a group has opacity by the *Opacity* effect and no blend mode. The backdrop input of transitions (EF-03) is not here either.
* **The old drawing** does not draw group effects (H6).
* **An adjustment clip is the whole frame**: it has no placement, and no mask.

## 6. For the maintainer

Nothing blocks 5.1 and 5.2. Before 5.3 I would like your word on **H10** (the two menu items, and that the adjustment clip and the generator are made as the text object is) and **H8** (Solid colour as the first generator, a placement on a generator clip); my proposals stand if you say nothing.
