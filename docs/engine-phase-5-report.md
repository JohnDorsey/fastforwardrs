<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Phase 5 report: groups that are exact, adjustment clips and generator clips

Written 2026-10-21, at the end of phase 5 of [the plan of action](plugin-system-plan-of-action.md). How it was designed is in [its design](engine-phase-5-design.md) (decisions H1 to H11).
**[V]** means I ran something and saw it; **[B]** means I believe it from reading the code. Everything that ran on a graphics adapter ran on **software adapters** (WARP on Windows 10, lavapipe
on Ubuntu); nothing of this phase has run on a real graphics card (SF-01).

## 1. Done

| piece | what | commit |
|---|---|---|
| 5.1 | **the engine draws groups**: a layer whose source is a group is drawn from the composite of the layers in it, in a picture of its own, and its effects are done to that picture | `49ddc19` |
| 5.2 | **the plan makes groups**: the effects of a clip group or a track group are done to the picture its members make together; the placeholder is gone; blur, crop and opacity can be on a group | `287b769` |
| 5.3 | **adjustment clips** (GR-09), file format 15 | `1b63ac0` |
| fix | a **latent bug since phase 4**: a video with an opacity, crop or blur effect hid what was behind it in the plan (`Layer::covers_the_picture` ignored the layer's own effects) | `66b307d` |
| 5.4 | **generator clips**: Solid colour (EF-02, Q-11), in format 15 | `7a283f6` |

## 2. The gate, line by line

| the gate | where it stands |
|---|---|
| "ungroup gives the same render" (GR-07) and "a group with no effects equals no group" | **Met [V]**, in the engine's tests and in the export's frame tests (5.2). |
| a group's effect is the effect of the composite | **Met [V]**: measured 2 levels of 255 from the composite (rounding), 108 from what the placeholder did. |
| the restriction of blur, crop and opacity to groups is lifted, its tests rewritten on purpose | **Met** (5.2: five tests). |
| preview equals export | **Met [V]**: one `frame_scene` function describes a frame for both; the solid colour and the adjustment clip go through it. |
| real export, colour by colour | **Met [V]** with ffmpeg: groups of the three kinds, adjustment clips (for exactly the frames they last; two of them turn by both turns), and solid colours (cover, rectangle, hue shift, crop, an animated colour). |

## 3. Decisions of mine (the design's H8 and H10 stood unless you objected; no objection had come when I built them)

* **A generator's content is a kind (`SolidColour`) and its colour is the `Colour` property of a text object**, not an effect instance (the design said an instance). This gives the colour button, the stopwatch,
  racks and the keyframes in a file for nothing. **You may overrule it.** An ISF generator (phase 6) will be a second way for a clip to be a generator.
* **The engine's `Solid` has no picture for effects**, so a solid colour with effects is described as a group of one whole-frame solid put in the clip's place (a crop cuts the colour's own edges).
* **A solid hides what is behind it only if** it is not see-through, its colour and place have no keyframes, it is the whole picture and upright, and its effects keep every pixel opaque.
* **A solid colour can be resized to any shape** on the preview, as text can; one that fills the picture is not grabbed until it is picked, as a video is not.
* **The label on a generator clip in the timeline is black or white**, whichever contrasts more with its colour (4.5 to 1 at the least).
* File format stays **15**: I **believe [B]** no 15 was released before the generator was added, and an older program refuses a 15 either way.

## 4. Tests rewritten on purpose

* Five tests of the placeholder and of the restriction of effects to groups (5.2), and the tests that pinned the file version at 14 (now `FILE_FORMAT_VERSION`).
* `effects_do_not_change_where_the_sections...` of the random projects (the fix): it said that no effect changes what the plan shows, which was the bug. It now says it of the effects that change
  colours and nothing else; the others have `a_video_that_an_effect_makes_see_through_does_not_hide_what_is_behind_it`.
* No test of phases 1 to 4 about the engine was edited, but for the few that copied a `LayerSource` (it is not `Copy` any more).

## 5. Numbers

* **Tests [V]:** the library **4798 passed, 0 failed, 8 ignored**; every integration program passes; clippy with `-D warnings`, `cargo fmt` and `cargo doc` clean.
* **Planted mistakes:** 5.2: 29 (3 survived: 2 closed with tests, 1 equivalent); 5.3: 16, all caught; the covering fix: 3, all caught; 5.4: **41, of which 39 are caught** (6 survived the first run: four were closed with tests, which
  found nothing wrong in the code but showed that the history of a colour change, the name of the menu and the colour of the clip on the timeline were not held by any test).
* **The two that remain [B]:** (1) the colour of a generator at a frame is asked at the clip's frame (clamped into the clip) and not at the timeline's: the two differ only for a keyframe outside its clip, and I believe none can be
  made; (2) the colour button of the properties panel sending its command: I found no way to drive egui's colour picker in a test (the text object's colour button is in the same state).
* **Oracles:** the random projects now also have solid colours (`random_project_with_generators*`, a flag on the builder so that the older seeds make the same projects): the plan against looking at each frame, the effects of each
  layer, and the round trip through the file.

## 6. Limits

* The **old drawing** (the fallback of a computer with no usable graphics adapter) leaves a group's effects out and draws a solid colour with only the colour mixes among its effects.
* **Sound:** a group's effects in sound are still the placeholder (a plugin of a group is heard on each member); the symbol of a fast video inside a group gets the group's effects.
* **No keyframes for the effects of a group** (H11); an adjustment clip's effects have them.
* **No blend modes**, and **no mask** on an adjustment clip.
* **Nothing can embed a plugin from the program yet** (phase 8).
* **I did not see the new things in a real window**, and none of it ran on a real graphics card [V that it did not].

## 7. What I need from you

* Say if the generator should be an **effect instance** (H8 as written) rather than what I built (section 3). Building it later means a change of the file for the clips already saved, so earlier is cheaper.
* Look at a solid colour and an adjustment clip in the window: their colour on the timeline, the order of the menu, and the properties panel. None of it was seen by anyone.
* The next piece in the order of the plan is **phase 6 (the ISF host)**, size L; I suggest **high effort** for it, with a design first.
