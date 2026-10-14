# Groups: the maintainer's rules, what they mean in this code, and the questions they leave

Status: **draft 2**, 2026-10-13. Draft 1 asked the questions; the maintainer answered them the same day (section 6 has the answers and the decisions
that follow from them, **read it first**). Groups are built **before** the effects-system work (the maintainer's order of 2026-10-13); the effect
system's plan (`plugin-system-features.md`) is adjusted to follow. This document is the single place for: the rules as the maintainer gave them
(section 1), what I checked in the code (section 2), how I read the rules (section 3), the inconsistencies and gaps I found with the answer I would
use if no answer comes (section 4; each now says how it was answered), the order of work (section 5), and the answers (section 6). Evidence tags:
**[V]** read in this repository today; **[B]** my opinion.

---

## 1. The rules, as given (2026-10-13, restated; nothing added)

**Three kinds of group.**

1. **Scattered group** (called a **tag** in the interface for now). Holds clips (video clips, image clips, generated clips, crates, and so on) from
   any tracks, including when that leaves clips that are *not* members between members in the layer order. Effects added to a scattered group do **not**
   apply to the group's composite: they apply to **each member individually**. So members and non-members can be interleaved without issue. A clip
   can be a member of several scattered groups, or of one or more scattered groups and another kind of group. **Any kind of group can directly be a
   member of a scattered group.** The order in which the effects of several scattered groups are applied is controlled by **membership badges** (section "membership").
2. **Contiguous group** (called a **clip group** in the interface for now). Holds only clips that fit within a **rectangular area of the timeline**
   (duration x layer count). It is always fit to its members: **the only way to resize it is to add or remove members.** Its effects are applied to the
   **composite of all its members**. A scattered group cannot directly be a member of a contiguous group; a contiguous group can be a member of another
   contiguous group or of a track group.
3. **Track group.** Just like a contiguous group, but instead of fitting itself horizontally (in time) to its members it **spans the whole duration of the
   project**. A track group can be a member of a contiguous group or of a track group.

**Appearance in the timeline.** The outlines of groups must fully contain the outlines of all their members, even when the members include groups with
unusual outlines. In the first version there is **no way to see a group's name in the timeline, to select a group by clicking it in the timeline, or to
drag a group in the timeline** (titles will come later, probably near the top of the outline, and will probably be the only place to click a group to drag it).
- *Scattered group:* from the members' positions an undirected graph with no loops is calculated that connects them all with invisible **spokes** that do
  not cross each other or any member clip (they may cross non-member clips); the coloured outline is then drawn as **one continuous loop** around all the
  members, not crossing any spoke, as if with yarn that resists bending tightly, conforming to the outsides of clips and spokes.
- *Contiguous group:* a coloured rectangle with rounded corners, fit to its members.
- *Track group:* drawn like a contiguous group, but visually extended to encompass the **track title boxes**.

**Appearance in the properties panel.** A thick coloured outline with rounded corners, and inside it:
- the **title bar**, in the outline's colour. Its text gives the group's kind and its (editable) name, in white or black, whichever contrasts most with the
  bar. The bar also has the **colour control**, which sets the colour used for every interface decoration of the group;
- the **list of members**. It cannot be reordered. Clicking a member selects it in the timeline; right-clicking gives a context menu option to **remove it from
  the group**. If removing would leave a non-member clip interleaved with member clips in a contiguous group, the option is still there but gives a refusal
  message in a popup window with an explanation and has no effect.

**Group membership.** Every member of a group has a **membership badge** in its properties panel, in the group's colour, with the group's name and kind. The
badge is an item **in the effects chain** and says exactly where in the chain the group's effects are applied to that member. For members of contiguous
groups and of track groups the badge is **always at the end** of the member's chain, because the group's effects are applied after the member is composited, so
it can have no more effects applied to it individually. A member of a scattered group can have its badge, and so the application of the group's effects,
**anywhere** in its own chain. Objects have no badge for groups they are only indirectly part of through another group.

**Changing membership by dragging** (in the timeline, or in the properties panel by showing nested groups): in the future, **not now**.

**Creating groups.** After selecting more than one clip on the timeline, right-clicking any clip of the selection offers to **create a scattered group**.
Whenever a group is made from a selection, everything in the selection is deselected and **the group becomes the only thing selected**.

The maintainer asked to be told of anything inconsistent or incomplete.

---

## 2. What the code says today [V]

* **Clips and tracks have effect chains.** A clip entry (`TrackEntry`) has `effects: Effects`; a track has `effects: Effects`. An `Effects` is an ordered list with
  **at most one effect of each kind**, and today there is one kind (hue shift). A rendered layer's effect list is the clip's effects, then its track's (`Layer.effects`
  in `composition/pictures.rs`).
* **There is no way to reorder effects**: the properties panel shows one box per effect with a menu that only deletes it, and a plus button that appends.
  (A badge that "can be anywhere in the chain" needs a way to move it; see G-12.)
* **Ids are made up on load and never written**: a clip is `ClipId(origin track, number)`, a track `TrackId`. The file refers to clips by (track index, clip
  index), as racks that point at other clips already do (`project_file/drawers.rs`). Groups saved in the file must do the same.
* **Selection** is a set of `SelectionItem::{Clip, Track, Keyframe, Rack}`; tracks can be multi-selected. A group would be one more item.
* **The properties panel** shows one box for a single selected thing (`clip_properties`, `track_properties`, ...) with the effects boxes below it
  (`effects::show`), and a list when several things are selected (`selection_list`). The box with a menu (`kind_box::show_with_a_menu`) is the pattern for a title
  bar with a menu.
* **Menus**: the clip menu (`timeline_panel/clip_menu.rs`: duplicate right/left, include/remove from the project file, delete) works on the selected clips when
  the right-clicked clip is one of them; the track header has a menu (duplicate above/below, delete). Refusals that need explaining use the modal message dialog.
* **Clips cannot overlap on a track**; dragging "stops against" what is in the way; the move function refuses an arrangement that would overlap. Track
  reordering by dragging the title box exists and is validated. Every operation that creates, moves, copies, retimes or deletes clips and tracks is a method of
  `Project`/`Track` (about 30), and **an undo step is a clone of the whole `Project`**, so groups kept inside `Project` are undone for free.
* **Every effect that exists is a per-pixel colour matrix** (`ColourMatrix`, applied in the preview and as an ffmpeg `colorchannelmixer` per layer). A linear
  per-pixel map commutes with alpha compositing, so **applying a group's effect to the composite of its members gives the same picture as applying it to each member,
  except where a colour channel is clamped to its range** (measured with ffmpeg, `gpu_research/group_effect_equivalence.py`: two overlapping, partly transparent
  layers and a 120-degree hue turn: identical in the regions covered by one layer and where nothing is, and **14 levels off in one channel where the layers overlap and
  the more saturated one is pushed past 255**). So group effects can be rendered *now*, in the preview and the export, by adding the group's effects to each
  member layer's chain, **as a placeholder that is exact except where colours clamp**; a real offscreen composite (exact) comes with the wgpu engine, and
  is needed anyway by effects that are not colour matrices, which do not exist yet.
  **Correction (2026-10-13, made while building it):** the "14 levels" above was **one measured example, and I wrote it as if it were a bound. It is not.** For
  two layers 40% and 50% clear (a blue one over an orange one) and a 120 degree turn it is 14 levels in one colour, and 20 at the worst turn for those two; for
  colours chosen at random among all there are, with random clearness and turns, a search found differences of **up to about 100 of the 255 levels**, and more than
  8 levels in about a quarter of the pairs (see the tests of `project/groups/effects_of_groups.rs` that have `placeholder` in their names, which work these
  differences out). Where the layers are **not clear at all** (a video, a picture without see-through parts) there is no difference: the layer in front is all that is
  seen. So the placeholder is wrong, by a lot for vivid colours, only at the edges of what is see-through (soft edges of text, of a turned picture, of a picture
  with a see-through part) where it is over something else. That is why it is documented as a placeholder everywhere it is in the code.
* **Crates** (keyframe crates) are clips (`Clip::Crate`); the generated clips of the rules do not exist yet (text objects are the nearest).

---

## 3. How I read the rules

* **Object**: anything that can be a member of a group: a clip, a track, a group.
* **Direct and indirect membership**: A is a direct member of G if G's member list holds A. Badges exist only for direct memberships.
* **Rectangle of a contiguous group** (and of a track group): the smallest rectangle, in (frames x track positions), that holds the footprints of all its direct
  members; a nested group's footprint is its own rectangle, a track's footprint is the whole duration of its row. **The invariant: no clip that is not a (direct or
  indirect) member of the group lies inside its rectangle.** Everything that edits clips or tracks must keep it true.
* **Effects**: a group's effect chain has the same kind of items as a clip's. The effects of a **tag** apply to each member at the badge's place in the member's
  chain; those of a **clip group** or **track group** apply after the members are composited, innermost group first.

---

## 4. Inconsistencies and gaps, with the answer I would use if none comes

Each item has a number for the answer log. **Blocking** means the model or the file format depends on it.

### 4.1 Structure

* **G-01 (blocking). What does a track group hold directly?** The rules say a contiguous group "holds only clips", a track group is "just like" one, and that a
  track group can be a member of a contiguous group and a contiguous group of a track group. But a track group spans the whole duration, so *every* clip on its
  tracks is inside its rectangle and, by the invariant, would have to be a member, including clips added later. That is only workable if the group's members are
  **tracks** (and track groups): the clips on them are then indirect members and need no badges, the title boxes belong to the outline because tracks are the
  members, and new clips on those tracks are simply inside. *My answer if none comes:* **a track group's direct members are tracks and track groups; a track
  has a badge for it at the end of the track's chain.** Then "a contiguous group is a member of a track group" has no direct meaning (a contiguous group inside
  a track group is *inside it by geometry*, with its clips on the group's tracks); is that acceptable, or did you mean something else by that sentence?
* **G-02 (blocking). A contiguous group that contains a track group is degenerate.** A track group spans the whole duration, so a contiguous group that has one
  as a member must span the whole duration too, and (with the invariant) every clip on its tracks must be a member: it is a track group under another name.
  *My answer:* **forbid it**: a contiguous group may hold clips and contiguous groups only; a track group may hold tracks and track groups; a track group
  may contain contiguous groups only by geometry (G-01). If you want the sentence as written, I need an example of what you would use it for.
* **G-03 (blocking). Who may be in a tag?** The rules say clips and "any kind of group". *My answer:* **clips and groups, not bare tracks** (a track is a member
  of a track group, which can itself be in a tag), and **no cycles** (a group cannot, directly or through others, be a member of itself). A tag in a tag: the outer tag's
  effects apply to each member of the inner one, at the inner tag's badge place in the chain of the inner tag.
* **G-04. How many direct memberships?** The rules say "one or more scattered groups and another kind of group". *My answer:* **any number of tags, and at most
  one clip group or track group as direct parent** (for a clip: one clip group; for a track: one track group; for a clip group or track group: one parent).
* **G-05. Group size.** Creation needs "more than one clip". *My answer:* **a tag may be made from one clip** (a tag is a label as well as a group), a clip group needs at
  least two members, a track group at least one track; a group that loses its last member is **deleted automatically**; a group left with one member stays.
* **G-06. Overlapping rectangles.** Two contiguous groups that are not nested may have rectangles that overlap in an area that holds no clip (the invariant is
  still true). *My answer:* **allowed**; but a contiguous group may not **partly** overlap a track group (it would be neither inside nor outside its composite), and
  two track groups may not partly overlap.

### 4.2 Making groups, and changing them

* **G-07 (blocking). Creating clip groups and track groups.** Only creating a tag is described. *My answer:* the right-click menu of a selection of clips offers
  **Create tag** (any selection of one or more clips) and **Create clip group** (two or more clips, enabled if the clips' rectangle holds no other clip; otherwise
  the item gives the refusal message and names the first clip in the way); the **track menu** (on one or several selected tracks) offers **Create track group**
  (the tracks must be next to each other, and the group must not cut through a contiguous group or another track group).
* **G-08 (blocking). Adding members after creation.** Dragging is deferred, but no other way to add a member is described, so a group could only ever lose members.
  *My answer:* **tags get "Add to tag" in the clip menu** (a list of the tags; cheap, no geometry), **clip groups and track groups get nothing in the first version**
  (to grow one, delete it and make it again, or wait for dragging). Is that what you want?
* **G-09 (blocking). Selecting and deleting a group later.** After creation the group is selected, and clicking its outline is not possible yet, so nothing
  describes how to select it again, or how to get rid of it. *My answer:* **clicking a member's membership badge selects that group**; clicking a member in a group's list
  selects the member (a nested group in the list selects the group); **the title bar has a menu (as effect boxes have) with "Delete group"**; with a group selected
  the **Delete key deletes the group only** (members stay, the badges go); the group's outline is drawn thicker while it is selected.
* **G-10 (blocking). Edits that would break a contiguous group.** The rules give one refusal (removing a member). Every other edit can break the invariant: dragging
  a clip (member or not) so that a non-member ends inside a rectangle or a rectangle swallows a non-member, importing or inserting a clip, duplicating, speeding up or
  slowing down a clip (its length changes the rectangle), reordering tracks, and so on. *My answer:* **such an edit is refused**, the way an edit that would overlap
  another clip is today: a drag **stops against** the obstacle (a rectangle is an obstacle for non-members), an import or insert goes **in front of the
  groups** (on a new track in front if the empty tracks are inside rectangles), a speed or duration change that would swallow a non-member is refused with a message,
  and a track reorder that would put a non-member track into a track group's range does not go there. **Cutting** a member makes both pieces members of the same
  groups (with the same badge places); **duplicating** a member makes the copy a member of the **same groups** (and refuses if that would swallow a non-member);
  **deleting** a member removes it from its groups (a rectangle shrinks).
* **G-11. What counts as "interleaved" when removing a member.** *My answer:* removing a member is refused when, after it, the group's rectangle (from the remaining
  members) would still hold the removed clip. So **only members on the edge of the rectangle can be removed**; a middle one cannot, which is a consequence of the
  rules and not a bug.

### 4.3 Effects and badges

* **G-12 (blocking). Moving a badge.** A scattered badge may be "anywhere in the chain", but nothing reorders items of a chain today. *My answer:* **only badges can be
  dragged** (up and down among the effect boxes, with the grip dots the timeline already uses); effects keep the order they were added in; a **new effect** is put
  after the scattered badges and before the fixed badges (contiguous and track group), and a **new scattered badge** (a clip joins a tag) goes at the end of the movable
  part of the chain. Do you also want effects themselves to be reorderable now?
* **G-13 (blocking). Track effects versus "the end of the chain".** A clip's layer has its own effects and then its track's. A contiguous group's effect applies after
  the member is composited, so after the track's effects. *My answer:* the order is **clip chain (with tag badges), then the track's chain, then the clip group's and
  track group's effects, innermost first**. The badge of a clip group sits at the end of the clip's own chain (as you say); the track's chain is not part of it. It means
  a tag's effect can never be placed *after* the track's effects. Acceptable?
* **G-14 (blocking). Animating group effects.** Effect values can be keyframed today (racks, drawers) in the drawer of a clip or a track; a group has no row and no drawer,
  and "from which frame" is not defined for a rectangle that moves with its members. *My answer:* **in the first version the values of a group's effects are not
  animated** (no stopwatch on them); keyframes for groups come with the plugin work or on request.
* **G-15. Sound.** *My answer:* **groups have no effect on sound** in the first version (no gain, mute or sound effects on a group); a picture effect on a group is
  ignored by members that have no picture (sound clips, crates).
* **G-16. Which effects can a group have.** *My answer:* every effect that applies to pictures (hue shift today), the same menu as a clip's, at most one of each kind per group.

### 4.4 Outlines

* **G-17. Nested outlines.** A nested group's outline must be inside its parent's: *my answer:* **each level of nesting adds 4 points of padding** (so a parent is always a little
  bigger than its biggest child); a tag's outline contains the **outlines** of its group members, not only their clips.
* **G-18. Unrelated outlines may cross.** Two tags always can, and a tag's loop may pass over non-member clips (as the rules allow). *My answer:* **crossing is fine and is
  distinguished by colour**; an outline is drawn **over the clips and under the playhead**, 2 points wide, with no fill, and 3 points wide while its group is selected.
* **G-19. The spokes.** *My answer:* straight when a straight spoke avoids the other members, otherwise bent along the gaps between the tracks (which are free), connecting
  the members as a minimum spanning tree; the loop is then the **outer boundary** of (members + spokes), padded and **smoothed with a minimum bend radius** (so it
  rounds convex corners and fills sharp notches), computed on a pixel grid, cached while nothing moves, and with holes ignored (one loop). It is an approximation of
  "yarn", not a physical simulation; is that acceptable?
* **G-20. A track group's right edge.** "The whole duration of the project" changes as clips move. *My answer:* from the left edge of the title boxes to **the end of the
  longest track** (the project's length), padded; it grows and shrinks with it.
* **G-21. Rows with open drawers.** *My answer:* an outline around clips on different tracks goes **across the drawers between them** (it follows the layout of the rows), and does not enclose a
  drawer's racks on its own.

### 4.5 The properties panel and small things

* **G-22. Names and colours.** *My answer:* default names "Tag 1", "Clip group 1", "Track group 1" (the smallest unused number of the kind); default colours from a palette of eight
  distinguishable hues, in turn, avoiding the selection colour; names need not be unique; the colour control is the colour button the text properties already use.
* **G-23. What the panel shows for a group.** *My answer:* the title bar, then the member list, then the **effects boxes and plus button** of a clip (a group's own chain), and
  badges of the groups the group itself belongs to, in its chain as for any member.
* **G-24. Member list labels.** *My answer:* the same labels the multi-selection list already uses (kind of clip and what it is made from), tracks by title, groups by kind and name.
* **G-25. File format.** *My answer:* **format 10** saves the groups (kind, name, colour, effects, members by position, badge places); format 9 files load with no groups; a file with groups
  is not readable by older versions (they say "newer file format").
* **G-26 (blocking). Rendering.** *My answer:* a group's effects are added to the effect lists of the member layers (section 2), in the preview and the export. **This is
  a placeholder for "applied to the composite"**: identical where the layers are not see-through, and off where partly clear layers overlap and a colour channel clamps
  (**my first text said "up to 14 levels"; that was one measured example and not a bound, see the correction in section 2: up to about 100 of 255 levels were found for vivid
  colours**). Making it exact in today's renderers means an offscreen composite per group in both the egui preview and the ffmpeg filter graph, which is work that the wgpu
  engine replaces. Do you accept the placeholder until the engine?
* **G-27. History.** *My answer:* every group operation is one step of the history; creating a group selects only it (so undo restores the selection as it was).

---

## 5. Order of work (after the questions are answered)

0. **Lock behaviour**: the existing tests stay as they are; new tests first for each piece.
1. **Model**: `Group`, `GroupId`, members, the chain with badges (clip entries, tracks, groups), `Project` storage, the **invariant check** (one function that every edit
   is validated by), selection item, the commands (create, delete, rename, colour, add a member, remove a member, move a badge), undo, and unit and property tests
   (random edits never break the invariant).
2. **Editing rules**: split, duplicate, delete, move, insert, import, speed/duration, track move, track delete, frame-rate retiming, all validated; refusal messages.
3. **File format 10**: reader, writer, fixtures of every version, round-trip tests.
4. **Rendering**: group effects folded into member layers (preview and export), with equivalence tests.
5. **Properties panel**: the group box (title bar, colour, member list, context menu, effects), badges in chains and their dragging, the menus that create groups, refusals.
6. **Timeline outlines**: rectangles first (clip groups, track groups, padding by depth), then tags (spokes and the smoothed loop), then the selected look.
7. README, mutation campaign, the gates, a real-window check.

---

## 6. Answers and the decisions that follow (2026-10-13)

### 6.1 The maintainer's answers, as given

| # | Answer |
|---|---|
| (colour) Q-05 | **Option b: old projects are converted** (no "legacy" flag): they get the new colour handling and look slightly different |
| G-01 | **Correction to the first definition:** a track group is *not* just like a clip group. **A track group directly holds tracks and track groups, and only those.** |
| G-02 | Default: a clip group cannot contain a track group |
| G-03 | A tag holds **clips, clip groups, track groups and other tags** (no cycles). Not bare tracks. |
| G-07 | Default: the clip menu offers *Create tag* and *Create clip group*; the track menu offers *Create track group* |
| G-08 | Default: only tags can be added to after creation (*Add to tag* in the clip menu) |
| G-09 | Default, with names: **"disband"** deletes the group and none of its members; the title bar's context menu also has **"delete all"**, which deletes all the members and the group |
| G-10, G-11 | Default: an edit that would put a non-member inside a clip group's rectangle is refused; only members on the edge can be removed |
| G-12 | **Changed: effects are reorderable too** (not only badges): every item of a chain can be dragged |
| G-13 | Default, **extended**: when a track has effects that apply to a clip, the clip's chain has a **grey track membership badge** that names the track and the number of its effects; it can be moved anywhere in the clip's chain "just like a tag badge", and the track's whole chain is applied at that place; **double-clicking it selects the track** |
| G-14 | Default: the values of a group's effects are not animated in the first version |
| G-15 | Groups themselves have no effect on sound **now, but their effects may in the future**: the chain of a group must not be picture-only by construction |
| G-19 | Acceptable (the yarn loop is an approximation) |
| G-26 | **Option 1 accepted** (second message of 2026-10-13, after I explained it in more detail): until the new rendering engine can make the composite of a group's members, the effects of a clip group or track group are applied to each member's own picture. **This is a placeholder and must be documented as one** (done in: the module documentation of `groups/effects_of_groups.rs`, the effect table row, `Layer::effects`, the README, `plugin-system-features.md`, and a test that pins what the difference is). Effect kinds say whether the placeholder gives the same picture (`on_a_composite`), and only those can be put on a clip group or a track group |
| other | **In the properties panel, badges and similar things that select another object need a double left click**, so that they can be dragged without selecting what they stand for |

### 6.2 Decisions I took where the rules and the answers leave room (flag any you dislike)

1. **Disband puts the members in the group's place.** If the group was inside a clip group or track group, its members become direct members of that parent (so
   nothing that was inside the parent leaves it); if it was in tags, its members join those tags, with the badge at the place the disbanded group's badge was.
   (Otherwise disbanding an inner group would have to be refused whenever it sits in an outer one.)
2. **"Delete all" is recursive**: every clip and group inside (directly or not), and for a track group every track with its clips. It is refused when it would
   delete every track of the project ("a project always has a track"). It is one step of the history, so it can be undone.
3. **The track badge is only stored when it is not at the end.** A fresh clip chain equals the old "no effects" chain; the track badge is implicitly last; new
   effects and new tag badges are put before an end-placed track badge, so the old order (the clip's effects, then the track's) is what you get until you move it.
   It is shown only when the track has at least one effect that applies to the clip (its count says how many); an audio clip or a crate, to which picture effects
   do not apply, shows none.
4. **Creating a clip group from a selection**: a selected clip that is already in a clip group counts as that group's member if *all* of that group's clips are
   selected too (nesting), and is refused otherwise ("x is in clip group y, which has clips that are not selected"). Groups in the selection (a group selected by
   double-clicking its badge, then Ctrl-clicking a clip) are members like clips.
5. **The Delete key with a group selected disbands it** (members stay). With clips selected too, it deletes the clips as it always did and disbands the groups.
6. **The list that the properties panel shows for a selection of several things keeps its single click**; only badges and the member lists of groups need the
   double click. (One line to change if you want that list to follow the same rule.)
7. **Badges are written to the file by position** (see section 5, step 3): `badges` lists, for each chain, the tag badges and a track badge that is not last, each
   with how many of the chain's effects come before it.

---

## 7. What was built (2026-10-13), and where it differs from what the sections above planned

**Where things are.**

| What | Where |
|---|---|
| The model: kinds, members, ids, the one validator that every edit is checked by, editing, upkeep (edits that cannot break a rule are not special; every edit that can is made through `keeping_the_groups_valid`), chains with badges, the placeholder | `src/project/groups/`, `src/project/effects.rs` |
| File format 10 (groups by position, badges, colours; version 9 files load with no groups; the version written is always 10) | `src/project_file/groups.rs`, `document.rs` |
| Rendering (group effects added to each member's layer: the **placeholder**) | `src/composition/pictures.rs`, documented in `src/project/groups/effects_of_groups.rs` |
| Commands, menus (clip menu: *Create tag*, *Create clip group*, *Add to tag*; track menu: *Create track group*), refusals as messages naming what is in the way | `src/application/commands.rs`, `fast_forward_application/group_flow.rs`, `application/group_messages.rs`, `timeline_panel/{clip_menu,header}.rs` |
| The properties panel: the box of a group (thick outline, bar with kind + name + colour, list of members, menus), the chain of effects with badges that are dragged and double-clicked | `src/application/properties_panel/{group_properties,chain,badge,kind_box}.rs` |
| The outlines on the timeline | `src/application/timeline_panel/group_outlines.rs` and `group_outlines/shape.rs` |
| Dragging clips stops against the rectangle of a clip group (the first place that is not allowed is worked out by arithmetic, so it takes the same time however far the clips are dragged; trying each place is what it falls back to, and what the tests of the arithmetic compare it with) | `src/application/clip_moving.rs` (`stopped_by_the_groups`, `stop_worked_out_by_arithmetic`, `stopped_by_trying_each_place`) |

**Where what was built differs from the plan above (flag any you dislike).**

1. **The chain of effects of a group is under its outlined box**, as the effects of a clip are under the box of the clip (G-23 did not say whether the thick outline round the
   box should have the effects inside it too; the outline goes round the bar and the list of members).
2. **The outlines of tags are not found on a pixel grid** (G-19): the rectangles of the members (and of the groups in them), padded, and the spokes between the members are put
   together exactly (the edges of all of them cut the plane into a grid of cells, and the edge between the cells that are inside and those that are outside is followed), and
   the corners are rounded: outward corners as round as the piece they belong to, inward corners 8 points (they fill in). It takes the same time at any zoom and is the same
   at any scroll position, and needs no cache.
3. **The outline of a group is drawn over the bottom of the ruler where the group has the first row in it** (its top edge is 3 points above the row, more for groups in
   groups), and the outline of a track group reaches left over the boxes of the titles.
4. **A drag stops at the first place where an edit would not be allowed, and does not go on past a rectangle to the place beyond it** (it would have been a jump).
5. **A clip that is dragged to the end of what the rules allow is not told so with a message** (the drag simply stops, as it does against another clip); the messages are for
   edits that are asked for (a copy, a speed, a length, a frame rate, a track moved, a new text object, a rack dropped on an empty place).
6. **The track badge** (G-13): shown only when the track has an effect that applies to the clip; stored in a clip's chain only when it is not last (so a project that never
   moved one is saved exactly as before); counted by *how many of the track's effects apply to the clip*.
7. **A bug found while auditing the edits that make clips: dragging a rack of keyframes onto an empty place of a track made a keyframe crate there without looking at the
   rules of groups** (the crate could be inside the rectangle of a clip group). Fixed (`insert_crate` is checked like the other edits, and the check that tells the drag
   whether a drop is allowed says so before it is made) and added to the random-edit test.

8. **The outline of a tag is worked out every time the timeline is drawn** (there is no cache), so what it costs matters for a tag that has a great many clips in it (the tag of
   every clip of a long project is a natural thing to make). Measured in a build for testing, with other programs busy, for a tag of clips in 10 rows: 60 clips 2 ms, 150 clips 7 ms,
   300 clips 20 ms, 600 clips 65-72 ms (a build for release is several times quicker). **My first way of finding the spokes took the cube of the number of clips, and I only measured it
   after the tests of the shape were done: 300 clips took 0.5 s and 600 clips 4.3 s.** It keeps, for each clip that is not joined yet, the nearest of those that are, which takes the
   square, and finds exactly the same spokes (the slow way is kept in the tests, and compared with it on random pieces, with many that are as near as one another).
9. **How far a drag can go is worked out by arithmetic, not by trying each frame** (the first way copied the project once for each frame of the drag, which would have made dragging
   slow when the timeline is zoomed far out). The trying is kept for the cases that the arithmetic does not say (a clip that is held to a time on another track, where it is not
   allowed as it is now) and as what the tests of the arithmetic are compared with.

**Known limits.**

* The placeholder (section 2, the correction): wrong at the see-through edges of layers that are over others, by up to about 100 levels for vivid colours.
* The outlines of tags are an approximation of "yarn": the spokes are straight or bend once between the middles of two members and cross whatever is between them (the
  clips of other groups, and clips that are in no group), which the rules allow; they are not routed round clips.
* A group is not selected by clicking its outline, not dragged, and its name is not shown on the timeline (as asked: later).
* Nothing is animated for a group (as asked), and groups have no effect on sound (as asked: not now).
