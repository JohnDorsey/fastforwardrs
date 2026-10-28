<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Effects that act on the sum, and tails that cross the edges of groups: design (phase 12, 2026-10-27)

**Status: a design, nothing is built.** The maintainer asked (2026-10-27): "Audio needs better handling, like video has, when it comes to applying [effects] to groups. [...] I would like audio effects on tracks or contiguous clip groups to
act on the sum of the things in that group. If reverb is added to a track, one clip's reverb should be able to overlap another clip, because it should really only be one reverb effect that hears both clips in sequence. [...] I might
want a video effect to create a ghostly after-image of a clip after that clip is over, much like how reverb would do for audio. [...] once someone knows how the application processes video, they should immediately be able to guess how
it processes audio. I want to eliminate quirks and hacks and make the system as powerful as everyone expects it to be or hopes it could be."

Then (after the first exploration, section 4): "Default scope is run. User should be able to choose unshared, shared with desharing allowed, shared with slicing around, or shared atomic. (These choices are per-track for now.) [...] If shared
atomic is chosen, the user can't accidentally put those clips in a group with effects. Add the per-clip control for what will be done with the clip's tail when it interferes with another group, and per-group control for what to do with
the group's tail in relation to other clips it may interfere with."

**[V]** = I read it in the code or ran it (the enumeration of section 4 was run on a model of the rules, not on the program); **[P]** = a plan, not built; **[B]** = belief; **[D]** = a decision that is the maintainer's to confirm (section 12).
The old decisions that this document changes are named by their numbers in `groups-feature-spec.md` (G-xx).

## 1. What is wrong today [V by reading]

* **A track's effects are copied into every clip, in picture and in sound.** `Project::effects_applied_to_every_clip` stamps the clip's effects, then the track's, then the tags' (where their badges are) into the chain of each clip, and the picture
  (`composition/pictures.rs`, `Layer.effects`) and the sound (`composition/sound`, `sound_engine/plugins.rs`: "in the chain of every clip") both use it. So a tag and a track are the same mechanism.
* **State is kept per clip.** A feedback shader's state is keyed by `hash_of(clip)` (`state_id` in `pictures.rs`), and a sound clip's processors live in `Live` and are dropped at the clip's end. So an effect with memory starts clear at every clip and
  its tail is cut where the clip ends. A reverb on a track cannot ring over the next clip, and an after-image cannot outlive its clip.
* **Groups act on a composite in picture only.** Clip groups and track groups are nodes in the layer tree (`composition/pictures/group_nodes.rs`, `GroupSpan`, `PlaceGroup`) and keep one state for the whole group. In sound they do nothing (G-15), and
  `PluginInsertions::for_strip` (the hook for an effect on a whole track) returns an empty list.
* **A group exists only where it has layers.** `nodes_of` builds a group node from the consecutive layers that are in it, so in a frame where no member has a layer there is no node. Nothing of a group can outlive its members.
* **The rules about groups talk about clips only.** "No clip that is not in a clip group is inside its rectangle" (`rules.rs`) is the only rule about what may be in a rectangle. A tail is the first thing that is in a rectangle and is not a clip.

## 2. The model in one page [P]

Everything that can have effects has a **chain**. What a chain acts on is the **sum** of what is below it, and picture and sound agree on that:

| Where the effects are | What they act on | One instance for... |
|---|---|---|
| a clip | that clip | the clip |
| a tag | each member, at its badge's place (as today) | each member (a tag is never a shared state) |
| a track | the sum of the clips of the track | depends on the track's **sharing** (section 5): the clip, the run, the track |
| a clip group or a track group | the sum of its members after their own chains (innermost first) | the group |
| (later) the project | the sum of everything | the project |

The order is G-13 with sound added: **clip chain (with tag badges) -> the track's chain -> clip groups, innermost first -> track groups, innermost first.**

Four words, used in the same way for picture and sound:

* **Tail**: how long an effect goes on after its input has ended (a reverb's ring, a delay's repeats, an after-image). Section 6.
* **Sharing** of a track: how much of a track an instance of its effects covers. Section 5.
* **Edge**: the border of a clip group's rectangle in time. A tail that crosses an edge is handled by a **tail policy**. Section 7.
* **Owner** of a tail: the thing that decides what happens to it at an edge. Section 7.1.

"Run" means: a stretch of consecutive clips on one track that are in the same groups (the same chain of clip groups and track groups) and whose gaps are no longer than the tail. It is the unit that a shared instance covers by default.

## 3. How groups work, and what this changes in them

### 3.1 Track groups, as they are implemented [V by reading `rules.rs`, `queries.rs`, `group_nodes.rs`]

* A track group's **direct members are tracks and track groups** (G-01). A clip on a member track is **inside** the group by where it is, not a member (`clip_is_inside`), and has no badge of it. A new clip on the track is simply inside.
* Its rectangle is **all the time of the project** and the rows of its tracks, which must be next to each other with no track between that is not in it (`IsBetweenTheTracksOfATrackGroupWithoutBeingInIt`).
* A **track group is not in a clip group** and a **clip group may be in a track group only by geometry**.
* **A clip group cannot straddle the edge of a track group: yes, that is correct.** `ClipGroupReachesAcrossTheEdgeOfATrackGroup` is a problem whenever a clip group's tracks are neither all inside the track group's tracks nor all apart from them. Two track
  groups may not partly overlap either (G-06).
* In the picture a track group is a node in the layer tree like a clip group, from the layers of its tracks, and its effects run on the composite with a state keyed by the group (`SpanOwner::Group`).

### 3.2 Is there a new conflict with track groups? No [V by reasoning, B that the code agrees]

A tail stays on the row of its own track, and a track is wholly inside or wholly outside a track group, so **a tail never crosses the edge of a track group**. A clip group inside a track group is in its composite as a member of it. A track group's effect
is one instance on the sum of its tracks, so a clip group can't take part of its input: the clip group is a member of the sum. What is new is only that **a track group gets sound effects** (G-15 is reversed, section 9), and one thing to remember:
a track's sharing (section 5) is per track, and a track group does not choose it for its tracks (a possible later addition, section 14).

### 3.3 The old decisions that matter, and what happens to each

| Decision | Now |
|---|---|
| G-04: any number of tags, at most one clip group or track group directly | kept |
| G-06: rectangles of two clip groups that are not nested may overlap where there is no clip | kept; but on a track whose sharing is **per track, with windows** (5.3) the time ranges of two groups that both have members on that track must not overlap: one more rule |
| G-10: an edit that would break a rule is refused | kept; two rules added (5.4, 5.3) and the refusals say why |
| G-13: the order of the chains | kept, extended to sound |
| G-14: group effects are not animated | kept (possible later, section 14) |
| G-15: groups have no sound effects | **reversed**: groups have sound effects, on the sum of the sounds of their members |
| the invariant "no clip that is not in a clip group is inside its rectangle" | kept; **tails may be inside a rectangle**, which the tail policies (section 7) decide |

## 4. The cases: what the enumeration found [V]

A tail is not special: a signal made inside one group context (the groups a clip is in) is present, later, in another. The same thing happens when a shared instance carries the state of one clip into the next. So **one question covers both**:
*a signal made in context A is present at a place in context B: what happens at the edges between them?*

I enumerated every layout the rules allow on small grids (`tools/group_cases/overlap_cases.py`, a model of `rules.rs`: clips of one cell, a clip group is a set of clips whose tight rectangle holds no other clip, two groups are nested or share no clip,
rectangles may overlap in empty space; a tail of 1 to 2 cells follows a clip on its own track) and classified every place a clip's tail is present:

| Grid | Layouts | Situations | Classes |
|---|---|---|---|
| 2 tracks x 5 frames, up to 2 groups | 60,911 | 547,492 | 12 |
| 3 tracks x 4 frames, up to 3 groups | 1,682,401 | 16,640,657 | 23 |

**Every class is "(groups exited, groups entered)", plus one more kind.** Depth (2 or 3 groups at once) only repeats the same step, so the base cases are:

| Case | Meaning | Example |
|---|---|---|
| A: same context | nothing crosses an edge | a and b in one group; one reverb hears both |
| B: exit | a member's tail goes past its group's edge | the last clip of a group, and a clip that is not in it comes next |
| C: enter | a tail of a clip that is not in a group runs into a group's rectangle | a clip, then a group's first clip |
| D: cross | exit one group and enter another | two groups one after the other on a track |
| E: claimed twice | the tail lands in empty space that **two rectangles that are not nested** share | found by the 3 x 4 grid: tracks 1-2 x frames 1-2 and tracks 2-3 x frames 2-3 share one cell |

Four findings:

1. **Only one thing can be cut: a track.** A clip group cuts a track when it has some of its clips and the track has others. Clip groups never cut each other (their members are disjoint), and track groups never cut anything (3.2). So every
   conflict of sharing is a track's effect meeting a clip group's edge.
2. **Case E exists only for tails** (it never happened on a clip), and **only an attribution by place has no answer there**: whose is a tail in a cell that two rectangles claim? An attribution by origin ("a tail belongs to the clip it came from")
   has an answer in every case. That is why the design is by origin, and place is used only where a shared instance gives no choice (5.3).
3. **In sound, nothing has a depth, so a foreign tail inside a group just adds up around it.** In picture a group is one flat layer, and a foreign tail inside its rectangle would sit **between** layers of the group (`nodes_of` would split the group in
   two). That is a conflict that sound does not have (section 8), and the one place where the two media cannot be fully alike.
4. **The enumeration is of a model and of small grids.** It says the cases are these on every grid I could afford; it does not prove it for all projects, and it does not model track groups (3.2 is an argument). Section 11 makes it a test of the real rules.

## 5. The sharing of a track [P]

A track has a **sharing**, one of four values, chosen per track (the track's menu, "Sharing of effects", two submenus; nothing always visible). **The default is *run*.** The sharing applies to the effects of the track in picture and in sound
(**D7: two settings per track, one for its picture effects and one for its sound effects**, each with the same four values).

| Value | The maintainer's words | One instance for | Tails | A clip group with effects may take... |
|---|---|---|---|---|
| **Separate for each clip** (`PerClip`) | unshared | each clip | each clip's own, by origin | any of the clips |
| **One copy for each run of clips** (`PerRun`, default) | shared, desharing allowed | each run | the run's, by origin | any of the clips (a copy for each run: a group's edge starts a new copy) |
| **One for the track, groups take a window** (`PerTrackWithWindows`) | shared, slicing | the track | one for the track, by place | any of the clips (a group takes a window of the one instance's output) |
| **One for the track, never divided** (`PerTrackIndivisible`) | shared, atomic | the track | one for the track | **none**, unless it has all the clips of the track |

In the text below the four are called *per clip*, *per run*, *per track with windows* and *per track, indivisible*. (The words "split" and "sliced" are not used: they looked too alike.)

### 5.1 Per clip and per run: by origin [P]

Each instance hears only the clips it covers, so what comes out of it **belongs to those clips** and goes with them into the groups they are in. This is exact for linear effects (a reverb of two clips is the sum of the reverbs of the clips: two copies
summed give the same sound as one instance, which I will test, section 11), and it is what the maintainer calls desharing: a copy of the track effect for the clips inside a group and another for the clips outside, so a tail never crosses from one to the
other. For an effect that is **not** linear (a compressor, a limiter) the copies sound different from one instance, because each does not hear the others' loudness. That is the point of the choice and the reason it is the user's: **the program cannot tell**
(a plugin does not say whether it is linear).

*Run* is the middle of the two: clips that are together in the same groups share an instance (so "one reverb that hears both clips in sequence" is true), and a group's edge starts a new one.

### 5.2 Runs and gaps [P]

A run ends where the groups of the next clip are different, **or where the gap before the next clip is longer than the tail** of the chain (6.3): after a gap that long the instance's memory is gone, so a new instance is the same sound and costs nothing while
it is idle. (The one thing that is not the same: an effect whose own clock goes on, an LFO, starts again at the same phase. **[B]** that nobody will hear it.) A run's instance is **keyed by the run's first clip** (6.5).

### 5.3 Per track with windows: by place, with its costs said out loud [P]

One instance for the whole track; **a group takes a window of its output** (the time of its rectangle, extended by the group's tail policy, 7.4). This is the only choice for an effect whose state mixes the clips, and it has three costs that no setting can remove:

* **A foreign tail leaks in**: the window holds whatever the instance outputs then, including the tail of a clip that is not in the group. The two cannot be pulled apart.
* **A hard edge**: where the window ends, the group's effect stops acting on the member's tail, which goes on unprocessed. The program puts a short fade (the **edge fade**, a setting, default 10 ms, see 9) on both sides of a window's edge so that it does not click.
* **The windows must not overlap** (case E): on a track with windows, **two clip groups with effects that both have members on that track must not overlap in time** (refused as an edit, as everything in G-10; `OverlappingWindows`). A window that is lengthened by
  a keep-alive (7.4) ends where the next window starts.

In picture the window is the same: the track's layer (and its after-image) is a child of the group's node in the frames of the window, **at its real depth**, so a track with windows has no depth conflict.

### 5.4 Per track, indivisible: the group cannot cut it [P]

A track that is indivisible is one block: one instance, never divided into copies, never given out in windows. So **a clip group that has effects cannot take only some of its clips.** The rule (`CutsAnIndivisibleTrack`, in `rules.rs` beside the others, so it is checked after every edit and the existing machinery
that takes an edit back, `upkeep`, applies it): a clip group that **has an effect that is switched on** and has at least one member on a track that is *indivisible* (in the medium of that effect: see below), while that track has a clip that is not inside the group, is a problem. It is refused when:

* a clip group is made, or its membership changes, and it would break the rule;
* an **effect is added to a clip group** (or switched on) that has members on an indivisible track and does not have all its clips;
* a track's sharing is **changed to indivisible** while a clip group with effects cuts it (the message names the group);
* a clip is inserted, imported, moved or duplicated onto an indivisible track where a clip group with effects has members on it and the clip would not be inside the group.

A group that has **no effect that does anything** is not affected (as it is not a node in the picture today); a group that **has all the clips of the track** is not either; tags are never affected; a track group is never a cut (3.2). The message says what to
do: "Clip group Intro has effects and would take some of the clips of Track 2, whose sound effects are indivisible. Change the sharing of Track 2, or take the effects off the group."

**This rule is deliberately static**: it looks at the structure and not at whether an effect has a tail or whether it is a picture effect or a sound effect, so a refusal never appears or goes away because a plugin's tail changed. It sometimes refuses
something that would have been harmless (a group with a picture effect on a track that has only sound effects). **[D]** I prefer the simple rule.

## 6. Tails [P]

### 6.1 What a tail is, and where it comes from

The **tail of an effect** is how long it goes on making output after its input ends. Where it comes from, in this order:

1. **A value the maintainer set on the effect** (the effect box's menu: "Tail...": *Automatic*, *None*, or a length). Nothing is added that is always visible.
2. **What the plugin says**: a CLAP plugin's `tail` extension (the program already uses it as the pre-roll: [V] `pre_roll_frames_of`, "a clip with a tail says how much sound from before a clip it wants").
3. **A default**: for a sound plugin that says nothing (CLAP without the extension, LV2, which has none) **0.5 s**, as the pre-roll has today; for a built-in effect **0** unless it declares one; for a picture effect that keeps a buffer between frames (a feedback shader)
   **the bound of EN-14, 300 frames** [B that this is the number the code uses: read it again before building] , because such a shader never ends by itself; for a picture effect with no state, 0.

The tail of a **chain** is the sum of the tails of the effects in it (a safe upper bound).

### 6.2 What the tail changes

* An instance lives from the start of its first clip to the end of its last clip **plus the tail**; after that it is gone and costs nothing. A clip's tail is its clip effects' tails and the track's, in the instance that has them.
* **In picture a clip leaves a ghost layer**: after the clip's last frame the layer goes on existing for the tail, with no source and the state of its effects as input, so an after-image is drawn. (Today a layer exists only where its clip does.)
* **The same number is the lead-up** (EN-14) **and the pre-roll**: to make the state right at a frame, run the instance from one tail before it. The file-based pre-roll of sound ("the sound of the file from before the clip") goes away: it was a
  substitute for a state that the next clip could not have, and it played noise from before the clip's in-point. A seek into the middle of a tail gets the same "approximate" marker as picture (EN-14 E1) **[P]**.

### 6.3 Gaps: see 5.2.

### 6.4 Tails in time, and sped-up clips

Tails are measured in time of the timeline, after the clip's speed (the effects see the sped-up sound). Today a sped-up clip has no pre-roll; it gets a tail like any clip. **[B]** that the effects see the sound after the speed change: check in `composition::sound`
before building.

### 6.5 The state of an instance, and edits [P]

The identity of an instance (for the states kept between frames, `EffectState { id, frame }`, and for the sound's processors) is: *per clip*: the clip; *per run*: the run's first clip and the effect; *per track with windows* and *per track, indivisible*: the track and the effect. An edit that
changes a run (a clip joins a group, a gap is closed) makes **new ids**, so the states start clear and the lead-up (6.2) puts them right. Nobody has to remember anything across such an edit.

## 7. Tail policies at an edge [P]

### 7.1 One rule: the owner decides

A tail at an edge is handled by the policy of its **owner**, and an owner is **the innermost thing that holds it**: a clip's tail is held by the clip's groups from the innermost out; when a group lets it go, or no group held it, the clip holds it again. **A group a tail is
not in cannot refuse it**: the policy belongs to the one that emits the tail. A run's tail belongs to the **last clip of the run** [D] (a tail begins where the clip it follows ends).

There are three things that can happen at an edge, the same three for every tail, in picture and in sound:

| Outcome | A tail that **leaves its own group** | A tail that **enters a group that it is not in** |
|---|---|---|
| **Processed** | **Keep alive**: the group goes on after its last member for the tail, and its effects process it | **Into**: the group adopts it for as long as it is inside, and its effects process it (and it is then like a member for that group's own edge) |
| **Untouched** | **Release**: it leaves the group at the edge, unprocessed by the group, and is the clip's again (the clip's policy decides at the next edge) | **Around**: it goes past the group untouched |
| **Cut** | the tail ends at the edge (with the edge fade) | the tail ends at the edge (with the edge fade) |

Nested groups: the tail is handled at each edge in turn, from the innermost; a tail that is released from an inner group and is still inside an outer group is in the outer group's sum.

### 7.2 The per-clip control (the clip's menu: "Tail toward other groups") [P]

*Around* (default) / *Into* / *Cut*, plus a picture setting **where it is drawn when it goes around**: *behind the group* (default) / *in front of the group*. It decides what happens to **this clip's tail where it enters a group that the clip is not in**
(and, after a group has released it, at the next edge). The track's sharing limits the choices (7.5).

### 7.3 Where "around" is drawn in the picture [P]

A group is one flat layer, so a tail that goes around cannot sit between its layers. **Behind** draws it just behind the group's node, **in front** just in front. Where it would not be inside any group's depths, it is drawn at the depth of its own track, as always. A live
clip is always in front of the tails of its own track. **This is the one place where picture has a rule that sound does not** (sound adds the tail up around the group), and it is a declared setting and not a hidden choice. **[D]** default behind: a memory is behind what is live.

### 7.4 The per-group control (the group's menu: "Tail of members") [P]

*Keep alive* (default) / *Release* / *Cut*, plus *behind* (default) / *in front* for where a kept-alive group's tail is drawn when it lands over clips that are not in it. A kept-alive tail is **always around** a group it is not in: if the maintainer wants another group
to process it, the first group *releases* it and the clip's own policy (*Into*) applies.

The group's **own effects' tail** (a reverb on the group) is not a member's tail: it rings out for the **tail of the effect** (6.1; set it to *None* to cut it), is owned by the group, and goes around everything else, with the same *behind/in front* setting.

A group that has **no effect that does anything** is not a node (as today) and its policy does nothing: its members' tails belong to what holds the group.

### 7.5 What each sharing allows [P]

Whether a tail can be kept apart from other sound depends on whether the instance holds only that tail. *Group has members on the track* is the case where a group and other clips share the track.

| Sharing | Clip: Around / Into / Cut (toward a group with members on the track) | Group: Keep alive / Release / Cut |
|---|---|---|
| Per clip, per run | all three | all three |
| Per track with windows | **Into only** (the window takes it; shown as such in the menu) | **Keep alive** (the window is lengthened) or **Release** (the window is the rectangle, with the fade). Cut is not possible: the output after the edge also holds other clips |
| Per track, indivisible | not applicable: such a group cannot have effects (5.4) | all three: the group has all the clips of the track, so the output after the edge is its own tail |

Where a group has **no members on the track** (the track only passes through its rectangle, empty), what is inside the window is only tails, and all three of the clip's policies work with any sharing. A choice that is not possible is not hidden: the menu shows it and,
when it is chosen, says why (as the refusals of G-10 do), and the control shows the value that is in force.

## 8. Picture and sound side by side [P]

| Thing | Picture | Sound | Alike? |
|---|---|---|---|
| a chain acts on the sum below it | yes | yes | yes |
| track effects: sharing | 4 values | 4 values | yes |
| tail of an effect, its source | shader state: the EN-14 bound or the setting | plugin tail, or the setting | yes (one control) |
| after the last clip | a ghost layer | the instance goes on | yes |
| lead-up on a seek | EN-14 (checkpoints, lead-up, chase) | the same tail as pre-roll, same marker | yes |
| group effects | on the composite | on the sum | yes |
| tail policy at an edge | three outcomes | three outcomes | yes |
| a foreign tail inside a group's rectangle | **depth conflict**: needs *behind / in front* | adds up | **no: intrinsic** (depth) |
| mute, gain | opacity and visibility, if they exist | gain and mute | see section 13 |

## 9. What changes in the model [P]

* `Track.picture_sharing` and `Track.sound_sharing`, each a `TrackSharing { PerClip, PerRun, PerTrackWithWindows, PerTrackIndivisible }` (default `PerRun`).
* `Clip.tail_policy: ClipTailPolicy { edge: Around | Into | Cut, depth: Behind | InFront }`, `Group.tail_policy: GroupTailPolicy { edge: KeepAlive | Release | Cut, depth: Behind | InFront }` (clip groups and track groups; a tag has none), and an optional **tail override** on every effect.
* **File format 20** (now 19). A file of format 19 or earlier gets `PerRun` for both on every track and the defaults everywhere: **a behaviour change** (an effect with memory on a track now rings over the next clip; a clip's effect rings out past the clip).
  **[D]** I consider it a bug fix and do not offer a compatibility switch. (A project file stays readable by the old program only as far as it always did: not.)
* Rules (`rules.rs`): `CutsAnIndivisibleTrack`, `OverlappingWindows`; both in the random-edit tests and in the refusals.
* **The sound plan** (`composition::sound`, shared by the export's graph and the live preview, [V] `sound_plan.rs`) says **instances** (clip, run, track, group) with the clips they hear, their tails, and **windows** for tracks with windows, instead of a chain stamped into every
  clip; `sound_engine` makes a `Live` for each instance and keeps it for its lifetime. G-15 is reversed: groups get sound effects (`PluginInsertions::for_strip` stops being empty).
* **The picture plan**: ghost layers, group nodes that exist for a tail, instances with the keys of 6.5, windows, and the depth setting. `nodes_of` builds a group from its layers **and the ghost layers that belong to it**.
* **G-14 stays** (group effects are not animated); effect values of a track's effects are keyed by time as today, so a run's instance sees the track's racks as one.
* **The edge fade** is a setting of the project (length in milliseconds, default 10, in the project's settings, not in a menu of the timeline), stored in the file with format 20 and used by every edge, in picture-less sound only (a picture has no click).
* Menus: the track's *Sharing of picture effects* and *Sharing of sound effects*, the clip's *Tail toward other groups*, the group's *Tail of members*, the effect's *Tail...*. All in menus that exist; nothing is added that is always visible.

## 10. Pieces, and the order [P]

Each piece: tests first that lock the present behaviour, then the change, then the same tests unmodified where the behaviour is meant to stay, then a campaign of planted mistakes. **Suggested effort: high for all; 12.2 and 12.5 are the risky ones** (they change what the plans mean).

* **12.0 Lock and enumerate.** Pin today's behaviour (a track effect is per clip, tails cut: this is the *per clip* sharing with tail 0). Move the enumeration of section 4 into a test of the real rules (11). No behaviour change.
  **12.0 as built (2026-10-27) [V]:**
  * `src/composition/sound/tails_as_they_are_tests.rs` (2 tests): a track's effect is a copy in the chain of each clip, and nothing in the plan says two clips share it; **a sound effect cannot be put on a clip group or a track group at all** (`applies_to_a_group_of_kind`
    allows only picture effects there, so G-15 holds by refusal and not by "ignoring"; a tag can have any effect, and it is a copy in each member). The picture side was already locked by `pictures/state_tests.rs` (a state for each clip, and for each group).
  * `tests/sound_plugins_clap_in_the_sound.rs` (2 tests, the real echo plugin, export and preview): the echo is cut where its clip ends (exact silence after), and a second clip with the same effect has an echo of its own that begins empty.
  * `src/project/groups/tail_cases_tests.rs` (4 tests): **the real editing (`create_group`) accepts a set of clip groups exactly when the model of section 4 says** (every rectangle holds only its own clips; two groups are nested or have no clip in common), on every layout of 2 x 3 and
    2 x 4 cells (all sets of clips, so the refusals are tested too) and, run by hand, on 3 x 4 (474,110 layouts, 4,404,071 places), **and the classes and their numbers are those of `tools/group_cases/overlap_cases.py`, including the 192 places that two groups claim**.
    A planted-mistake run: 5 of 7 noticed; the 2 that were not are equivalent mutants (a tail of 3 cells on a grid 3 cells wide; "share any clip" for "one is in the other", which is the same while no two groups partly overlap, and **would differ if they could: the to-do of section 15**).
  * Whole library suite 5442 passed, 0 failed, 11 ignored; clippy and fmt clean on Windows and Linux (WSL). No behaviour changed.
* **12.1 The model, the rules, the file, the menus.** Fields, `CutsAnIndivisibleTrack`, `OverlappingWindows`, format 20 with a migration test, the menus and the refusals. The engines ignore the new values.
  **12.1 as built (2026-10-27) [V]:**
  * **The model** (`src/project/tails.rs`, `tail_editing.rs`): `TrackSharing` (`PerClip`, `PerRun` the default, `PerTrackWithWindows`, `PerTrackIndivisible`) in `EffectSharing { picture, sound }` on a track; `ClipTailPolicy`, kept by each clip entry and carried by a cut, a copy, a move to another track and a copy of a track; `GroupTailPolicy` on a clip group or a track group (a tag is refused: `ATagHasNoEdge`); `TailLength` on an effect (`Automatic`, `None`, `Milliseconds`, at most ten minutes), kept by a copy of the effect. 7 tests (`tail_editing/tests.rs`) and one (in `composition/sound/tails_as_they_are_tests.rs`) that **nothing in the sound plan or the picture plan changes when any of them does**.
  * **The two rules** (`rules.rs`, `GroupProblem::CutsAnIndivisibleTrack` and `OverlappingWindows`), looked at per medium and only for clip groups with an effect that is switched on; the edits that could break them are checked and taken back with the others: `set_track_sharing`, adding an effect to a group, switching a group's effect on, copying a group's effect, and (as they were already) putting a clip on a track, moving it and so on. 9 tests (`sharing_rules_tests.rs`), and a run of random edits (60 seeds of 400 edits, one in four about sharing and tails, `Randomly::with_sharing`) in which every edit keeps every rule, a refused edit changes nothing, and the rules are refused for hundreds of times. Two tests that existed were changed on purpose: one that pinned the file format (19 to 20), and `a_group_in_itself_does_not_make_what_is_applied_go_on_for_ever`, which built a group in itself *first* and then gave the groups effects through the editing, which now checks the rules of a project that has a group in itself (it builds the groups first and puts them in each other after).
  * **The file, format 20** (`format_20_tests.rs`, 9 tests): `sharing`, `clip_tails` (by the place of the clip), `tail` of a group, `tail` of an effect and the project's `edge_fade`, all left out when they are the default; a file of format 19 is read as the defaults; a tag with a tail, a clip that is not there, a word that is not one, and a file that breaks the two rules are refused with the reason. `docs/project-file-format.md` says all of it.
  * **The setting** `EdgeFade` (*No fade*, 5, **10 ms**, 20, 50 ms), a setting of both layers (the global one and one that a project may set for itself), so the settings windows show it and the two settings files keep it.
  * **The commands and the menus** (`tail_menus.rs`; `SetTrackSharing`, `SetClipTailPolicy`, `SetGroupTailPolicy`, `SetEffectTailLength`; 10 tests in `tail_menu_tests.rs`): *Sharing of picture effects* and *of sound effects* in the menu of the box of a track, *Tail toward other groups* in the menu of a clip (of all the selected clips, in one step), *Tail of members* in the menu of the bar of a clip group or a track group (a tag has none), *Tail length* in the menu of the box of an effect; the one in force is marked; each is one step of undo; a sharing that a rule would break is refused with the reason in a message and changes nothing.
  * **Decisions made while building, to be said**: a policy that cannot be in force where it is made (7.5: a clip on a track that gives windows) is **stored all the same**, and the menu says what is in force; for several selected clips with different policies, a choice sets the whole policy of all of them; the *Tail length* menu is on every effect but what a generator makes. **Sound effects are still refused on a clip group or a track group** (the permission flips with 12.3, when the sound engine can process them: until then they would be accepted and do nothing).
  * **Verified**: whole library suite 5480 passed, 0 failed, 11 ignored (5442 before); the integration programs that read project files and settings pass (`application_journeys` 32, `command_line_export` 9, `text_objects_as_they_are` 6, `sound_plugins_journey` 4, `sound_plugins_clap_journey` 1); clippy, fmt and doc clean on Windows and, for clippy, Linux (WSL). **24 planted mistakes in the new code, all noticed** (the rules, the edits that check them, what a cut, a copy, a move and a copy of a track carry, what the file writes and reads, the default of the setting, the length limit, the commands and the menus). Not run: macOS, and a real window.
* **12.2 Sound: instances, tails, runs.** *per clip* and *per run*; a clip's tail rings out; the gap rule; lead-up from the tail; the file pre-roll goes. *Risky: the sound plan changes.*
  **12.2 is made in steps, each verified and pushed.** **12.2b as built (2026-10-28) [V]: the instance of a track's sound effects for a run of clips.** The plan (`composition::sound`) gives a strip `buses`: each is
  the effects of the track and the first and last item of a run (consecutive clips, no silence between: the gap rule comes with the tails, 12.2c); a clip whose track effects are not at the end of its chain
  (the badge of the track moved to the middle) keeps its own copy there, and the sharing *one copy for each clip* keeps the stamping as it was. The specification (`SoundBusToExport`) carries them; the engine
  (`sound_engine/graph.rs`) makes one instance when the first clip of a run is first heard (`Insertions::for_bus`, with the plugin host behind it), gives it the **sum of the clips of the run, each made as its
  own effects and its volume make it**, and lets go of it after the last sample; an instance that has a **latency is made up for by making the sound that goes in that many samples ahead** of the sound that
  comes out, across the clips. **The order of a clip's sound is now: its own effects, its volume, the effects of its track (as one instance for the run), the sum of the tracks** (it was: its own effects, the
  effects of its track, its volume: it differs only for a track effect that is not linear, with a clip whose volume is not 0 dB; the track's volume is still the clip's, and moves after the effects with D9, 12.2e).
  Tests: 8 of the plan (`buses_tests.rs`), 5 of the engine with made-up effects (the state goes on from one clip to the next; made when first heard and let go of after the last sample; any block size; the
  volume before the instance; a latency made up for across clips), and one with the real echo plugin (**one echo that hears two clips sounds as the one clip they were cut from does**, with the other
  sharing as the control); whole library suite 5492 passed, the sound integration programs pass, clippy clean.
  **12.2c as built (2026-10-28) [V]: tails.** `tail_samples_of` (`composition::sound`, with a test that holds its 48,000 to the engine's `SAMPLE_RATE`): an effect that is a plugin that is installed rings on
  for what its person said (`None`, or a length: `LayerEffect.tail_length`) and where they said nothing for what the plugin says (the tail of a CLAP plugin, up to two seconds; half a second for LV2; none
  for LADSPA: `InstalledPlugin.pre_roll_samples`, which was already that); a chain for the tails added up. **The engine**: a clip's chain is given silence for its tail after the clip ends and its
  `Live` is let go of when the ring ends; a ring overlaps the clips after it, so an item is made in a work buffer and **added**; the volume of the clip is done to the ring too; a bus is heard from its first
  clip until its last clip has rung on, **and then for the tail of the track's effects**; a ring past the end of the timeline is cut there (the sound is as long as the timeline). **The plan: the gap rule**:
  a run goes on across a silence that is no longer than the tail of the track's effects (the silence is items of the bus, heard as silence), and a clip that does not share them ends a run. Tests: engine 7
  (the ring is exactly the effect given silence for its tail and then nothing; it is as loud as the clip and added to the next clip; let go of when it ends; any block size; cut at the end of the
  sound; a bus rings on for the tail of the track's effects; a clip's own ring goes through the bus), plan 3 (the tails, frames that reach, the gap rule), and with the real echo plugin: it rings
  on for the tail it says and for no longer, **`None` is the old behaviour (cut where the clip ends)**, a length that was said is the length it rings for, the ring of one clip is added to the start of the
  next, and **an echo for each clip that rings on and one echo for the run sound the same (an echo is linear: the null test of the design, to 1e-4 with a tail long enough that nothing is left out)**.
  The two tests that pinned the old cutting were rewritten on purpose. *Not yet: the lead-up (a seek into a ring: 12.2d), the track's volume and mute after the effects (D9: 12.2e).*
  **12.2d as built (2026-10-28) [V]: the lead-in (what the design calls the lead-up, for sound).** A plan wanted from a frame begins before it by as much as the effects ring on for (`SoundPlan.lead_in_frames`): for
  each clip, the tails of all the sound effects that its sound goes through (its own, its tags' and its track's) added up; the greatest of those, no more than **ten seconds**
  (`MOST_SECONDS_OF_LEAD_IN`: past that the sound at the start is as it would be if the effects had begun to listen ten seconds before) and no more than there is before the frame. The sound begins there,
  and **the first frames are made and not heard**: `SoundGraphSpecification.lead_in_frames`, which the block graph makes and throws away at its first call (so every consumer, the preview's stream and the
  export, gets the sound from where it is wanted; `total_samples()` and `position()` count what is heard), and which the filter graph of ffmpeg cuts off the front (`atrim`; a null test holds the two alike). So a
  seek to just after a clip, or into the middle of a run, hears the ring of what came before it, and the effects of a run are in the state that playing through gives them. (The sound that the file has
  from before a clip's first frame, `pre_roll_frames_of`, is as it was: the lead-in makes it not needed for a clip that began earlier, and it is only given to clips that begin in the lead-in.) Tests: engine 2 (the
  sound is the end of the sound made from the start, for any blocks; the position counts what is heard), plan 3 (the lead-in, its limits, the tails of a chain added, ten seconds), a null test with the filter
  graph, and with the real echo: **sound wanted from the frame where its clip ends has the ring of the clip, and is the sound that playing through makes**, in the export and in the preview. *Not yet: the
  "approximate" mark that video has (EN-14 E1) for a ring longer than the lead-in.*
  **12.2e as built (2026-10-28) [V]: the volume and the mute of a track come after its effects (D9).** `SoundStrip` has `loudness` (the track's own volume, counted from the start of the plan) and `muted`
  (the stretches, in frames from the start of the plan, where the track is muted); a clip's `loudness` is now **only its own**, and the mute of a track no longer cuts its clips into heard and silent items (a clip's
  own mute still does). So the effects of a track hear the sound as its clips make it, **and what they remember goes on while the track is muted** (a muted track's reverb has been listening all along, and
  is not empty when the mute ends). The engine (`Strip`) does the volume (`GainCurve`, counted from the first sample of the sound) and then the mutes (silence over their samples) to the sound of the
  strip after its processors; the filter graph of ffmpeg, which has no effects of a track, does the same to the joined clips (`volume=` and `volume=0:enable=between(t,..)` after `concat`). **A track muted
  all the way through has no strip** (nothing of it is ever heard, whatever its effects remember). Measured, not assumed: the filter graph switches `enable` a chunk of sound at a time, so at the edge of
  a mute it is up to one chunk (4096 moments) out of step with the block graph, which is exact to the sample; the null test compares them to one rounding of a sample everywhere else and checks that the edges
  do differ (so that it is not comparing nothing). Tests: engine 3 (the volume is after the effects: `(x + 100) / 2`, not `x / 2 + 100`; a changing volume is counted from the start of the sound for any
  blocks; a mute is silent after the effects, **the effect's count goes on meanwhile**, any block size), plan: 4 tests that pinned the clip-and-track product were rewritten on purpose (the clip's volume and the track's
  are two curves that together say what both say; a muted stretch is a stretch of the strip and its clips are all there; a clip's mute and a track's mute are two things), the randomised plan check also counts strips with a changing
  volume; a null test with the filter graph (a volume for a whole track: exact; the mutes: exact but at the edges). **6 planted mistakes in the engine, all noticed** (the mute does nothing, forgets a channel, begins late, ends early;
  the volume does nothing, counts from the wrong sample). Whole library suite 5510 passed, 0 failed, 11 ignored; the sound integration programs and `application_journeys` pass; clippy clean on Windows.
* **12.3 Sound: group effects.** Clip groups and track groups on the sum; the three outcomes, with the fade.
  **12.3a-c as built (2026-10-28) [V]: a clip group and a track group can have sound effects, done to the sum of what their members make.** `EffectKind::applies_to_a_group_of_kind` is now true for every effect but the text (it
  was: picture only, so that G-15 held by refusal). **The plan** (`composition::sound`): `SoundPlan.groups` has a `SoundGroup { group, effects, parent }` for each clip group or track group that has a sound effect that is switched on
  and installed (the others are pass-through, as they are in the picture); a clip says where its sound goes, `SoundClip::sent_to`: the nearest of the groups round it that is one of those (the clip groups it is in, one in another, then the track groups of its track), or
  the sum of all the sound; a group's `parent` is the next of those round it. The effects of a clip by itself no longer include its groups' (`effects_applied_to_every_clip_by_itself`; the pictures already used that), and **the edge of a group ends a run of the
  effects of a track** (clips that are sent to different places are not one instance: a tail does not cross the edge, which is desharing). The lead-in counts the tails of the groups' effects too; the hash of the sound effects covers them (a group's effects are in the
  list of each clip's). The picture of a group ignores its sound effects. **The engine** (`sound_engine/graph.rs`) is rewritten as **pulling**: the sum of all the sound asks for what is sent to it, and a group asks for what is sent to it in the same way, **added**
  and given to **one instance** of its effects, made when the first of its members is first wanted (`Insertions::for_group`, with the plugin host behind it, as for a bus) and let go of when the last has rung on and the effects have rung on after it (`GroupNode`); a group
  inside a group gives its sum to the one round it; **a track's volume and mutes are done to what it sends, before the group's effects** (and the effects of a track, a bus, before them: clip effects, clip volume, track effects, track volume, group effects); a group's effects
  that have a latency are made up for by asking its members for the sound that many samples later (`ahead`), **through a chain of groups too**. A strip no longer has effects of its own (`Insertions::for_strip` was a placeholder for this, and is gone). The filter graph of ffmpeg,
  which cannot do effects, adds the clips as it always did (a group without effects is the same sum: held by a test).
  Tests: plan 9 (`groups_tests.rs`: which groups there are, where each clip goes, nested, track groups, switched off, the picture-only group, a run ends at the edge, the lead-in, the hash), engine 11 (`tests/groups.rs`: added and given to the effects once, a group that does nothing is
  transparent, volume and mute before the effects, a group in a group inside out, the track's effects before the group's, made when first heard and let go of, none made for a group that hears nothing, the tail, latency, latency in latency, any block size), and with the real
  echo plugin: **an echo on a group that hears two tracks sounds as an echo for each does (it is linear)**, in the export and in the preview. Three tests that pinned the refusal of a sound effect on a group were rewritten on purpose (`kind_tests` x2 and the 12.0 lock test, which now
  holds that the effect is in the chain of **no clip**). *Not looked at in a window: groups of the sound in the properties panel.*
  **12.3d as built (2026-10-28) [V]: the tail policies at an edge, and the edge fade.** What a clip's effects go on making after it ends (its **ring**), and the ring of an instance of a track's effects after its last clip, goes where
  the policies say: `composition/sound/tail_routes.rs` works it out at **plan** time, in frames, as a list of **routes** (`TailRoute { from_frame, to: Group(n) | Sum | Cut }`, on `SoundClip` for a clip that no instance hears and on `SoundBus`
  for the instance, whose policy is **its last clip's**: D3). The first stretch, the clip itself, goes where `sent_to` says; after that the events are met in the order of their frames: **the edge of a group the ring is in** (the frame its last member ends: `rectangle_of_the_group().end_frame`):
  its policy says keep alive (the ring stays in the instance of the group's effects, **and nothing after it changes where the ring goes**), release (it goes to the group round that one, or to the sum) or cut; and **the first frame of a group the ring is not in, whose rectangle reaches the clip's track**
  (and only a group that has sound effects is one): the clip's policy says around (nothing), into (the group adopts it, and the ring then meets that group's edge) or cut. A cut is a route to nowhere. Where nothing changes the list is empty (**the defaults are
  exactly the old behaviour**, which the old tests hold). **The engine** makes an item or an instance that has routes **once, in order, as far as anyone asks**, keeps what it made (a `Cache`, let go of from before the end of each block), and each place it goes
  to takes its share by a weight that is 1 inside a stretch, and **crossfades over the edge fade** (the setting `EdgeFade`, 10 ms by default: `SoundGraphSpecification::edge_fade_milliseconds`, set from the effective settings by the export and the preview, and in the preview's signature) where the
  stretch changes (less and less to the one before, more and more to the new one, which add up to all of it); a cut fades out. A group is made when the first of what it hears is wanted, and **the extent it is heard for follows the routes** (a ring that left it does not keep it alive; a ring that went into it does).
  A ring that is cut ends where the cut has faded out: its effects are let go of then. The sound's hash for the preview now has the groups' places and policies. Because the sound is asked for ahead for the effects that delay it (`ahead`), and a ring is asked for by places that are ahead by
  different amounts, **what is made is kept and read at each place's own time**, which is why a group with a latency does not move a ring that leaves it (a test). **Limits, said out loud**: a ring that an inner group keeps alive is the inner group's from then on (an outer group's release or cut cannot reach into the instance of the inner group's
  effects); the picture has none of this yet (12.5); the menus of the policies were already there (12.1) and now do something. Tests: interpreter 17 (`tail_routes_tests.rs`: every policy at every edge, the order of events, the geometry), engine 8 (`tests/routes.rs`: release, cut, into, crossfade, cut with fade, any block size and a latency, an instance's routes, when a group is let go of), and with the real CLAP gain
  and echo: **a group that releases leaves the ring not halved from the edge on, and the control keeps it halved**, in the export and in the preview.
* **12.4 Sound: per track with windows, and indivisible.**
  **12.4 as built (2026-10-28) [V]: one instance of a track's sound effects for the whole track.** With *per track, with windows* and *per track, indivisible* the plan makes **one instance for the track**: a run goes on across any silence and across the edge
  of any group, and only a clip that has a copy of the track's effects of its own (the badge of the track moved to the middle of its chain) ends it (`runs_of_clips_that_share_the_effects_of_the_track` with the sharing). `SoundBus` says where it sends
  what it makes, `sent_to` (it was worked out from its clips: the engine now takes it from the specification). **Indivisible** needs nothing more: the rule `CutsAnIndivisibleTrack` keeps every group with effects from taking part of the track, so all
  its clips go to one place, and the policies of the group that has all of them work as for any instance (a test). **Windows**: the instance's `sent_to` is where its sound goes **outside the windows** (the nearest track group that holds the track and has sound
  effects, or the sum), and `tail_routes` are the **windows**, worked out by `window_routes_of`: **each clip group that has sound effects and whose rectangle reaches across the track takes what the instance makes while it is there** (whether or not it has a clip
  on the track: a group that only passes over the track takes what rings in it), from the first frame of the rectangle to its end, **lengthened to the end of the sound if the group keeps its tails alive, but only as far as the first frame of the next
  window** (a window that is lengthened ends where the next begins and does not come back); a group that releases has the rectangle; **one that cuts releases too** (a window cannot cut: what is after its edge is also what other clips make); of windows that are at
  one frame the one that began last is taken, which is the inner one, and where it ends the outer one comes back. The engine's routes (12.3d) do the rest, with the edge fade at each edge of a window: **it is exactly the one instance, so what rings in it from before the
  window is done to by the group too** ("a foreign tail leaks in": section 5.3 says it, and a test with the real echo shows it: an echo still ringing from the first clip is halved with the second clip's by the group that takes the window, and was not when each run had its own instance).
  Tests: the plan 8 (`sharing_tests.rs`: one instance across the silences for both sharings and not for *per run*, the group that has all the clips of an indivisible track, a window and its lengthening, two windows, the track group that is where the sound goes outside the windows,
  a group that only passes over the track, the other sharings make no windows, where the instance sends what it makes), the windows on made-up groups 4, the engine 1 (an instance sends what it makes where it is told and not where its clips are said to go), and with the real echo the
  case above, in the export and in the preview. 10 planted mistakes, all noticed (6 needed tests).
* **12.5 Picture: ghost layers, instances, groups with tails, depth.** *Risky: the layer list per frame changes and `nodes_of` with it.*
  **12.5 and 12.6 as built (2026-10-28) [V]: the state of a track's picture effects is shared as the sharing says; ghost layers and the policies are NOT built.** `LayerEffect::shared_by` is the number that tells the clips that share the state of an effect of a track
  (`runs_of_clips_that_share_the_state_of_a_tracks_effects` in `composition::pictures`; `state_id` uses it instead of the clip and the place in the chain, so it is the same state whatever clip it is done to). *One copy for each clip*: none, as before. *One copy for each run*
  (the default): clips **one right after another that are in the same groups that make a picture of their own** (the edge of a group that makes one ends a run, as the edge of a group with sound effects does in the sound; a group with no effect that does anything is not an edge) share
  the state, so **a shader that depends on earlier frames goes on from one clip to the next across a cut**: the frame of its clock is the timeline's (it always was for a track), so the next clip's first frame is the frame after the last of the one before, and the engine goes on instead of starting clear. *Per track, with
  windows* and *per track, indivisible*: all the clips of the track share it. The effects of a clip, of a tag and of a group are as they were (a state for each clip, for each clip a tag is done to, for each group). **What is not built, and why**: (a) *ghost layers* (the layer going on for the
  tail after the clip's last frame, and across a gap, with no source): the engine could draw it as it draws a generator (a transparent picture of the size of the frame with the effects done to it), but **the buffers that last are the size of the layer's picture, and a video's is not the size of the frame**, so a state would
  start clear when the clip gives way to its ghost; to keep one, a layer without a picture would have to be given the size of the picture that it stands in for, which is a change of the engine (the sizes of the layers' pictures and what `Checkpoint::is_of` says of them), and it is not made here; so **a gap, or the end of the last clip, ends the state**
  (the sound has all of the tail); (b) *the tail policies of a clip and of a group, and the depth (behind, in front of a group)* for a picture, which are the policies of 12.3d with a ghost to apply them to; (c) *windows* as a thing of their own for the picture: with *per track, with windows* the one state of the track's effects goes on across the edge of a group that makes a picture, and what the group makes its picture of is the layers of its clips after the track's effects, so the group has a window of the instance by place, as the design says; with *one copy for each run* the edge of a group is where the state begins again. The model, the menus and the rules for all of it exist since 12.1; the settings that a picture cannot yet do are *tail policies*, *depth* and the *windows*. Tests: 5 of the plan
  (`state_tests.rs`: it goes on across a cut, after a gap and with one copy for each clip it does not, with one instance it does across a gap, the edge of a group that makes a picture and the one that does not); 6 planted mistakes, all noticed.
* **12.6 Picture: per track with windows, and indivisible.** *(Built with 12.5: the state of the track's effects is one for the whole track; see above.)*
* **12.7 Report.** `engine-phase-12-report.md`; the table of cases as it is built.

## 11. How we will know that every case is accounted for [P]

1. **A pure function** (`edges::outcome`) of a layout, a source of tail, the sharing and the policies gives, for every place a tail is present, one of: *processed by these groups*, *around, drawn behind or in front*, *cut at this edge*, or *refused by this rule*.
   There is no case in which it is not defined.
2. **The enumeration of section 4 becomes a test** over the same grids, on the **real group rules** (`problems_with_the_groups`, so a layout the rules refuse is not counted and one they allow is not missed), and checks, for every layout, every sharing and every
   policy: (a) the function is defined; (b) **conservation**: each moment of each tail goes to exactly one place, and nothing is processed twice or dropped, except where the policy is *Cut*; (c) the cases found are the classes of section 4 and no others
   (a new class makes the test fail with the layout, so a rule that nobody thought of is found by a test and not by a user).
3. **The table of cases is generated** from that test (a golden file) and so is the gallery page the maintainer sees (the explorer of the first exploration, made from the real data), so the picture of the cases cannot be out of date.
4. **Numbers, not trust**: *per clip* and *per run* give **the same sound** for a linear effect (a reverb) on a real project (a null test); a track with no groups gives the same sound and picture in all four sharings; *per track with windows* equals *per run* when no tail crosses an edge;
   a layout with no tails is **bit-identical to today**.
5. **Planted mistakes** in the function, the rules and both plans, as in every phase; a survivor gets a test.

## 12. Decisions that are the maintainer's [D]

| # | Decision | My answer |
|---|---|---|
| D1 | Old projects get *per run* and the defaults (a behaviour change), with no compatibility switch | **decided: yes** |
| D2 | The rule for an indivisible track is static (5.4); with two settings per track it is looked at **per medium**: a group counts for a medium if it has a switched-on effect of that medium | **decided: yes** (per medium follows from D7) |
| D3 | A run's tail belongs to its last clip | yes |
| D4 | *Behind* is the default depth for a tail that goes around | **decided: yes** |
| D5 | The fade at an edge is a setting, default 10 ms | **decided: a setting, default 10 ms** |
| D6 | The defaults of a tail: setting, plugin, 0.5 s (sound), 300 frames (a picture state), 0 otherwise | yes |
| D7 | One sharing per track for picture and sound | **decided: two settings per track, one for each medium** |
| D8 | A transition clip has no tail (its effect belongs to the moment of the change) and an adjustment clip has the policy of a group | yes |
| D9 | Mute and gain of a track act **after** the effects, so the state goes on running while muted and a tail is not cut by muting | **decided: yes** |

## 13. What we may have missed [B]

Things that are in the design because of this list are marked; the others are open.

1. **Gaps and runs** (5.2): a run ends after a gap longer than the tail. In the design.
2. **Plugin latency**: a plugin that reports latency delays the sound; a copy for a run and a window must have the same delay as the others and the sum must be compensated. The bundled plugins report none ([V] `bundleable-lv2-plugins.md`), so nothing tests it yet. **Open.**
3. **Mute, solo and gain** of tracks and groups: where they are in the chain (D9). A group has no gain or mute today; with sound effects on groups someone will ask.
4. **Adjustment clips** are groups made on the spot (`make_the_adjustments_groups`): the policy of a group (D8). **Transitions**: D8.
5. **Export and preview must agree**: both consume the sound plan ([V] `sound_plan.rs`), so the instances must be in the plan, and the null test of the export against the preview must run for each sharing. In piece 12.2.
6. **A sound-less clip in a group** (a text clip, a crate) has no sound tail; **a clip with no picture** (a sound clip) has no ghost layer. The rules for which clip counts as "in" a group for which medium do not change.
7. **Cost**: tails keep instances alive after the clips; a feedback shader's tail (300 frames by default) costs real time in the preview. The setting and "None" are the answer; the chase (EN-14) already has a budget.
8. **Effects whose own clock goes on** (an LFO) restart after a gap or at a new copy (5.2).
9. **Undo**: the sharing and the policies are commands with one undo step each; a change of sharing restarts the states (6.5), which is expected.
10. **Discoverability**: everything is in right-click menus (the maintainer's rule: nothing unrequested and always visible), so a person who never opens a track's menu never learns that sharing exists. If this turns out to be a problem, a small mark on a track whose sharing is not the default is the cheapest answer, and it is not in this design.
11. **A tag's effect with memory** is one instance per member (a tag is scattered: there is no sum). Offered as nothing.
12. **A group's effect that makes something from nothing** (a generator on a group) does not exist in the frames where the group has no layer (1); the spec says a track group spans the whole project. **Open**; not changed here.
13. **Keyframes on a track's effect across a new copy**: the values are by time of the timeline, so each copy sees the same values; **[B]**, to be tested in 12.2.

## 14. Not implemented, but possible

Each of these can be asked for later; the design leaves them possible and none is scheduled.

* **Keyframes on group effects** (G-14).
* **A master chain**: one more node above everything, for both media.
* **A sharing for a track group's tracks** (set on the group, inherited by its tracks).
* **A sidechain / backdrop in sound** (the sound of one place as the control of an effect in another), the counterpart of the backdrop of a transition in picture. Needs explicit wires, so it belongs to the question of effect graphs (Q-30, EN-15, ID-10).
* **A group's effect that makes something from nothing in the frames where it has no member** (13.12).
* **A per-clip flag "keep this clip out of shared state"**, if the sharing of the track turns out not to be fine enough.

## 15. Unscheduled to-do

* **Ghost layers for the picture** (12.5, not built). The tail of a picture effect that keeps a state (a feedback shader) after the last clip of a run, across a gap, and the policies and the depth that go with it. What is needed first: **a layer without a picture that has the size
  of the picture of the layer it stands in for**, so that the buffers that last keep their size (a video's picture is not the size of the frame: see `engine::effect_states`, `Checkpoint::is_of`, and how `frame_scene` describes a generator); then `LayerContent::Ghost` in the plan
  (sections after the clip, `Candidate`), the ghost counted by the lead-up, the chase and the checkpoints (EN-14) like any layer, and the policies of `composition::sound::tail_routes` applied to ghosts (around, into and cut at a group's rectangle; keep alive, release and cut at its edge; behind and in front of a group's node).

* **Contiguous groups of clips that overlap without nesting** (asked 2026-10-27, "in the future"). **I do not know yet whether this is possible**, so it is a to-do and not a possibility. What I know: today a clip is directly in **one** clip group or track group at most (`InTwoShapes`, `rules.rs`; G-04),
  and a rectangle may overlap another only where there is no clip (G-06). What would have to be answered first: what a clip that is in two composites is in the layer tree (drawn twice? in which order?); whose effects come first (G-13 would need a second badge at the end of the chain);
  what the tail policies (7) mean when the cell is a member of both (section 4, case E would no longer be empty space); and what the rectangle invariant becomes. The enumeration of section 4 is the tool to look at it with.
