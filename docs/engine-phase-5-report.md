<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Phase 5 report: groups that are exact and adjustment clips

Written 2026-10-21, at the end of phase 5 of [the plan of action](plugin-system-plan-of-action.md). How it was designed is in [its design](engine-phase-5-design.md) (decisions H1 to H11).
**[V]** means I ran something and saw it; **[B]** means I believe it from reading the code. Everything that ran on a graphics adapter ran on **software adapters** (WARP on Windows 10, lavapipe
on Ubuntu); nothing of this phase has run on a real graphics card (SF-01).

**Generator clips were built (piece 5.4, `7a283f6`, with Solid colour as the only generator) and taken out again the same day, at the maintainer's request:** the program is to be slim and do much with
non-built-ins, so generators come back with the ISF host (phase 6), where a solid colour is a few lines of shader. The removal is the revert of that commit (`git revert`, no other change of code); the
design keeps H8 to H10 as the record of what was asked.

## 1. Done

| piece | what | commit |
|---|---|---|
| 5.1 | **the engine draws groups**: a layer whose source is a group is drawn from the composite of the layers in it, in a picture of its own, and its effects are done to that picture | `49ddc19` |
| 5.2 | **the plan makes groups**: the effects of a clip group or a track group are done to the picture its members make together; the placeholder is gone; blur, crop and opacity can be on a group | `287b769` |
| 5.3 | **adjustment clips** (GR-09), file format 15 | `1b63ac0` |
| fix | a **latent bug since phase 4**: a video with an opacity, crop or blur effect hid what was behind it in the plan (`Layer::covers_the_picture` ignored the layer's own effects) | `66b307d` |
| 5.4 | generator clips: built and removed (see above) | `7a283f6`, reverted |

## 2. The gate, line by line

| the gate | where it stands |
|---|---|
| "ungroup gives the same render" (GR-07) and "a group with no effects equals no group" | **Met [V]**, in the engine's tests and in the export's frame tests (5.2). |
| a group's effect is the effect of the composite | **Met [V]**: measured 2 levels of 255 from the composite (rounding), 108 from what the placeholder did. |
| the restriction of blur, crop and opacity to groups is lifted, its tests rewritten on purpose | **Met** (5.2: five tests). |
| preview equals export | **Met [V]**: one `frame_scene` function describes a frame for both; the adjustment clip goes through it. |
| real export, colour by colour | **Met [V]** with ffmpeg: groups of the three kinds, and adjustment clips (for exactly the frames they last; two of them turn by both turns). |

## 3. Tests rewritten on purpose

* Five tests of the placeholder and of the restriction of effects to groups (5.2), and the tests that pinned the file version at 14 (now `FILE_FORMAT_VERSION`).
* `effects_do_not_change_where_the_sections...` of the random projects (the fix): it said that no effect changes what the plan shows, which was the bug. It now says it of the effects that change
  colours and nothing else; the others have `a_video_that_an_effect_makes_see_through_does_not_hide_what_is_behind_it`.
* No test of phases 1 to 4 about the engine was edited, but for the few that copied a `LayerSource` (it is not `Copy` any more).

## 4. Numbers

* **Planted mistakes:** 5.2: 29 (3 survived: 2 closed with tests, 1 equivalent); 5.3: 16, all caught; the covering fix: 3, all caught.
* Tests, clippy, fmt and doc after the removal: see section 6.

## 5. Limits

* The **old drawing** (the fallback of a computer with no usable graphics adapter) leaves a group's effects out.
* **Sound:** a group's effects in sound are still the placeholder (a plugin of a group is heard on each member); the symbol of a fast video inside a group gets the group's effects.
* **No keyframes for the effects of a group** (H11); an adjustment clip's effects have them.
* **No blend modes**, and **no mask** on an adjustment clip.
* **Nothing can embed a plugin from the program yet** (phase 8).
* **I did not see an adjustment clip in a real window**, and none of it ran on a real graphics card.
* **There was no clip that makes a picture** until the ISF host was built: generator clips are phase 6.5 (2026-10-22), made by a shader.

## 6. What I need from you

* Look at an adjustment clip in the window: its colour on the timeline, the order of the menu, and the properties panel.
* The next piece in the order of the plan is **phase 6 (the ISF host)**, size L; I suggest **high effort** for it, with a design first.
