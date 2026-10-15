// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Random edits of projects with groups: whatever is done, in whatever order, no rule of groups is left broken, and an edit that is refused
//! changes nothing.
//!
//! This is the test that the rest stands on. The editing is written so that every edit that can break a rule is taken back if it does; here a
//! great many edits of every kind are made at random, on projects of clips and groups, and the one check that finds every broken rule
//! (`Project::problems_with_the_groups`) is made after each. (So is the one that finds what is wrong with the racks of keyframes and the
//! keyframe crates that hold them, `Project::problems_with_the_racks`: some of the edits make keyframes and crates, and the others have to
//! leave them as they should be, as a crate lasts as long as its keyframes need and goes if it has none.)

use std::collections::BTreeMap;

use super::*;
use crate::project::groups::test_projects::{Laid, lay_out};
use crate::project::racks_tests::{db, put_a_rack_in};
use crate::project::{
    AnimationTarget, ClipMove, ClipSide, Effect, EffectInstanceId, EffectKind, FrameRate, Project, PropertyId,
    RackDestination, RackId, TextObject, TimelineFrameIndex, TrackSide,
};
use crate::test_support::DeterministicRandomNumbers;

/// What was done and whether it was refused, by kind of edit, and which of the arrangements that are worth trying the project was in at some
/// time.
#[derive(Default)]
struct Tally {
    made: BTreeMap<&'static str, usize>,
    refused: BTreeMap<&'static str, usize>,
    reached: BTreeMap<&'static str, usize>,
}

impl Tally {
    fn note(&mut self, edit: &'static str, was_made: bool) {
        *(if was_made { &mut self.made } else { &mut self.refused })
            .entry(edit)
            .or_default() += 1;
    }

    fn add(&mut self, other: &Tally) {
        for (edit, count) in &other.made {
            *self.made.entry(edit).or_default() += count;
        }
        for (edit, count) in &other.refused {
            *self.refused.entry(edit).or_default() += count;
        }
        for (arrangement, count) in &other.reached {
            *self.reached.entry(arrangement).or_default() += count;
        }
    }
}

/// The arrangements of groups, chains and tracks that are worth being sure that the random edits come to, which the rules are then checked in.
fn arrangements_in(project: &Project) -> Vec<&'static str> {
    let mut found = Vec::new();
    let kind_of = |group: GroupId| project.group(group).map(Group::kind);
    for group in project.groups() {
        let holds = |kind: GroupKind| {
            group
                .members()
                .iter()
                .any(|member| matches!(member, Member::Group(inner) if kind_of(*inner) == Some(kind)))
        };
        match group.kind() {
            GroupKind::Tag => {
                if holds(GroupKind::Tag) {
                    found.push("a tag that has a tag in it");
                }
                if holds(GroupKind::ClipGroup) {
                    found.push("a tag that has a clip group in it");
                }
                if holds(GroupKind::TrackGroup) {
                    found.push("a tag that has a track group in it");
                }
            }
            GroupKind::ClipGroup => {
                if holds(GroupKind::ClipGroup) {
                    found.push("a clip group that has a clip group in it");
                }
                if group.members().len() >= 3 {
                    found.push("a clip group of three or more");
                }
            }
            GroupKind::TrackGroup => {
                if holds(GroupKind::TrackGroup) {
                    found.push("a track group that has a track group in it");
                }
                if group.members().len() >= 3 {
                    found.push("a track group of three or more");
                }
            }
        }
        if group.effects().tag_badges().count() >= 1 && !group.effects().is_empty() {
            found.push("a group that has effects and is in a tag");
        }
    }
    let clip_groups_and_track_groups_exist = project
        .groups()
        .iter()
        .any(|group| group.kind() == GroupKind::ClipGroup)
        && project
            .groups()
            .iter()
            .any(|group| group.kind() == GroupKind::TrackGroup);
    if clip_groups_and_track_groups_exist {
        found.push("clip groups and track groups at once");
    }
    for track in project.tracks() {
        for entry in track.entries() {
            if entry.effects().tag_badges().count() >= 2 {
                found.push("a clip that is in two tags");
            }
            if entry.effects().keeps_the_badge_of_the_track() {
                found.push("a clip whose badge of its track is not at the end");
            }
            if !entry.effects().is_empty() && entry.effects().tag_badges().count() >= 1 {
                found.push("a clip that has an effect and a badge");
            }
        }
    }
    found.sort_unstable();
    found.dedup();
    found
}

struct Randomly {
    random: DeterministicRandomNumbers,
    laid: Laid,
    log: Vec<String>,
    tally: Tally,
}

impl Randomly {
    fn new(seed: u64) -> Self {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        // Five tracks, each with a few clips one after another, with gaps between them.
        let mut layout: Vec<Vec<(u64, u64)>> = Vec::new();
        for _ in 0..5 {
            let mut clips = Vec::new();
            let mut cursor = random.next_below(12);
            for _ in 0..(2 + random.next_below(4)) {
                let frames = 4 + random.next_below(20);
                clips.push((cursor, frames));
                cursor += frames + random.next_below(12);
            }
            layout.push(clips);
        }
        let layout_as_slices: Vec<&[(u64, u64)]> = layout.iter().map(Vec::as_slice).collect();
        Self {
            random,
            laid: lay_out(&layout_as_slices),
            log: Vec::new(),
            tally: Tally::default(),
        }
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.random.next_below(bound as u64)).unwrap()
    }

    fn all_clips(&self) -> Vec<ClipId> {
        self.laid
            .project
            .tracks()
            .iter()
            .flat_map(|track| track.entries().iter().map(|entry| entry.id()))
            .collect()
    }

    fn all_groups(&self, kind: Option<GroupKind>) -> Vec<GroupId> {
        self.laid
            .project
            .groups()
            .iter()
            .filter(|group| kind.is_none_or(|kind| group.kind() == kind))
            .map(Group::id)
            .collect()
    }

    fn some_clip(&mut self) -> Option<ClipId> {
        let clips = self.all_clips();
        (!clips.is_empty()).then(|| clips[self.below(clips.len())])
    }

    fn some_group(&mut self, kind: Option<GroupKind>) -> Option<GroupId> {
        let groups = self.all_groups(kind);
        (!groups.is_empty()).then(|| groups[self.below(groups.len())])
    }

    fn some_track(&mut self) -> TrackId {
        let tracks = self.laid.project.tracks();
        let position = self.below(tracks.len());
        self.laid.project.tracks()[position].id()
    }

    /// A few clips that are near one another, which has some chance of being a clip group that the rules allow.
    fn some_clips_near_each_other(&mut self) -> Vec<Member> {
        let Some(first) = self.some_clip() else {
            return Vec::new();
        };
        let first_place = self.laid.project.rectangle_of_the_clip(first).unwrap();
        let mut members = vec![Member::Clip(first)];
        for clip in self.all_clips() {
            if clip == first || members.len() >= 4 {
                continue;
            }
            let place = self.laid.project.rectangle_of_the_clip(clip).unwrap();
            let near = place.first_track.abs_diff(first_place.first_track) <= 1
                && place.first_frame.abs_diff(first_place.first_frame) <= 30
                && self.below(2) == 0;
            if near {
                members.push(Member::Clip(clip));
            }
        }
        members
    }

    /// A run of tracks that are next to each other, with the track groups that are inside it standing for their tracks.
    fn some_tracks_next_to_each_other(&mut self) -> Vec<Member> {
        let count = self.laid.project.tracks().len();
        let first = self.below(count);
        let last = (first + self.below(3)).min(count - 1);
        let mut members: Vec<Member> = Vec::new();
        for position in first..=last {
            let track = self.laid.project.tracks()[position].id();
            let mut stands_for = Member::Track(track);
            // The outermost track group that is inside the run stands for the tracks in it.
            let mut at = Member::Track(track);
            while let Some(parent) = self.laid.project.shape_group_of(at) {
                let tracks = self.laid.project.tracks_in(parent.id());
                let inside = tracks.iter().all(|track| {
                    self.laid
                        .project
                        .track_position(*track)
                        .is_some_and(|place| (first..=last).contains(&place))
                });
                if !inside {
                    break;
                }
                stands_for = Member::Group(parent.id());
                at = Member::Group(parent.id());
            }
            if !members.contains(&stands_for) {
                members.push(stands_for);
            }
        }
        members
    }

    /// Makes one random edit, notes what it was, and checks that no rule is broken and that a refused edit changed nothing.
    fn edit_at_random(&mut self) {
        let before = self.laid.project.clone();
        let (name, was_made, description) = self.make_a_random_edit();
        self.log.push(format!(
            "{name}: {description}: {}",
            if was_made { "made" } else { "refused" }
        ));
        self.tally.note(name, was_made);
        if !was_made {
            assert_eq!(
                self.laid.project,
                before,
                "the edit was refused and the project changed:\n{}",
                self.recent_log()
            );
        }
        let problems = self.laid.project.problems_with_the_groups();
        assert!(problems.is_empty(), "{problems:?} after:\n{}", self.recent_log());
        let problems = self.laid.project.problems_with_the_racks();
        assert!(problems.is_empty(), "{problems:?} after:\n{}", self.recent_log());
        assert!(!self.laid.project.tracks().is_empty());
        for arrangement in arrangements_in(&self.laid.project) {
            *self.tally.reached.entry(arrangement).or_default() += 1;
        }
        // Nothing the rest of the program asks of the groups goes wrong either.
        let applied = self.laid.project.effects_applied_to_every_clip();
        assert_eq!(applied.len(), self.all_clips().len());
        for group in self.laid.project.groups() {
            let _ = self.laid.project.rectangle_of_the_group(group.id());
            let _ = self.laid.project.chain_rows_of(Member::Group(group.id()));
        }
    }

    fn recent_log(&self) -> String {
        let start = self.log.len().saturating_sub(12);
        self.log[start..].join("\n")
    }

    fn make_a_random_edit(&mut self) -> (&'static str, bool, String) {
        let choice = self.below(114);
        match choice {
            0..=11 => {
                let kind = GroupKind::Tag;
                let mut members = Vec::new();
                for _ in 0..(1 + self.below(3)) {
                    let member = if self.below(4) == 0 {
                        self.some_group(None).map(Member::Group)
                    } else {
                        self.some_clip().map(Member::Clip)
                    };
                    members.extend(member);
                }
                let result = self.laid.project.create_group(kind, &members);
                ("create a tag", result.is_ok(), format!("{members:?}"))
            }
            12..=23 => {
                let mut members = self.some_clips_near_each_other();
                if self.below(4) == 0
                    && let Some(group) = self.some_group(Some(GroupKind::ClipGroup))
                {
                    members.push(Member::Group(group));
                }
                let result = self.laid.project.create_group(GroupKind::ClipGroup, &members);
                ("create a clip group", result.is_ok(), format!("{members:?}"))
            }
            24..=31 => {
                let members = self.some_tracks_next_to_each_other();
                let result = self.laid.project.create_group(GroupKind::TrackGroup, &members);
                ("create a track group", result.is_ok(), format!("{members:?}"))
            }
            32..=37 => {
                let Some(group) = self.some_group(None) else {
                    return ("add to a group", false, "no group".to_owned());
                };
                let member = match self.below(3) {
                    0 => self.some_group(None).map(Member::Group),
                    1 => Some(Member::Track(self.some_track())),
                    _ => self.some_clip().map(Member::Clip),
                };
                let Some(member) = member else {
                    return ("add to a group", false, "nothing to add".to_owned());
                };
                (
                    "add to a group",
                    self.laid.project.add_to_the_group(group, member).is_ok(),
                    format!("{member:?} to {group}"),
                )
            }
            38..=45 => {
                let Some(group) = self.some_group(None) else {
                    return ("take out of a group", false, "no group".to_owned());
                };
                let members = self.laid.project.group(group).unwrap().members().to_vec();
                let member = members[self.below(members.len())];
                (
                    "take out of a group",
                    self.laid.project.take_out_of_the_group(group, member).is_ok(),
                    format!("{member:?} from {group}"),
                )
            }
            46..=50 => {
                let Some(group) = self.some_group(None) else {
                    return ("disband a group", false, "no group".to_owned());
                };
                (
                    "disband a group",
                    self.laid.project.disband_group(group).is_ok(),
                    format!("{group}"),
                )
            }
            51..=52 => {
                let Some(group) = self.some_group(None) else {
                    return ("delete a group and all in it", false, "no group".to_owned());
                };
                (
                    "delete a group and all in it",
                    self.laid.project.delete_the_group_and_everything_in_it(group).is_ok(),
                    format!("{group}"),
                )
            }
            53..=56 => {
                let Some(clip) = self.some_clip() else {
                    return ("delete a clip", false, "no clip".to_owned());
                };
                (
                    "delete a clip",
                    self.laid.project.delete_clip(clip).is_ok(),
                    format!("{clip}"),
                )
            }
            57 => {
                let track = self.some_track();
                (
                    "delete a track",
                    self.laid.project.delete_track(track).is_ok(),
                    format!("{track}"),
                )
            }
            58..=62 => {
                let frame = TimelineFrameIndex::new(self.random.next_below(120));
                let cuts = self.laid.project.split_at(frame, None);
                ("cut at a frame", !cuts.is_empty(), format!("{frame}"))
            }
            63..=66 => {
                let Some(clip) = self.some_clip() else {
                    return ("duplicate a clip", false, "no clip".to_owned());
                };
                let side = if self.below(2) == 0 {
                    ClipSide::Right
                } else {
                    ClipSide::Left
                };
                (
                    "duplicate a clip",
                    self.laid.project.duplicate_clip(clip, side).is_ok(),
                    format!("{clip} {side:?}"),
                )
            }
            67..=68 => {
                let track = self.some_track();
                let side = if self.below(2) == 0 {
                    TrackSide::Above
                } else {
                    TrackSide::Below
                };
                (
                    "duplicate a track",
                    self.laid.project.duplicate_track(track, side).is_ok(),
                    format!("{track} {side:?}"),
                )
            }
            69..=80 => {
                let mut moves = Vec::new();
                for _ in 0..(1 + self.below(2)) {
                    if let Some(clip) = self.some_clip() {
                        let to_track = self.some_track();
                        moves.push(ClipMove {
                            clip,
                            to_track,
                            start: TimelineFrameIndex::new(self.random.next_below(150)),
                        });
                    }
                }
                (
                    "move clips",
                    self.laid.project.move_clips(&moves).is_ok(),
                    format!("{moves:?}"),
                )
            }
            81..=84 => {
                let track = self.some_track();
                let position = self.below(self.laid.project.tracks().len() + 1);
                (
                    "move a track",
                    self.laid.project.move_track(track, position).is_ok(),
                    format!("{track} to {position}"),
                )
            }
            85..=88 => {
                let Some(clip) = self.some_clip() else {
                    return ("change how long a clip lasts", false, "no clip".to_owned());
                };
                let frames = 1 + self.random.next_below(40);
                (
                    "change how long a clip lasts",
                    self.laid.project.set_clip_duration(clip, frames).is_ok(),
                    format!("{clip} to {frames}"),
                )
            }
            89..=91 => {
                let track = self.some_track();
                let frame = TimelineFrameIndex::new(self.random.next_below(150));
                let frames = 1 + self.random.next_below(30);
                (
                    "put a text object on a track",
                    self.laid
                        .project
                        .insert_text(track, frame, TextObject::default(), frames)
                        .is_some(),
                    format!("{track} at {frame} for {frames}"),
                )
            }
            92..=94 => {
                let Some(group) = self.some_group(None) else {
                    return ("an effect of a group", false, "no group".to_owned());
                };
                let present: Vec<EffectInstanceId> = self
                    .laid
                    .project
                    .group(group)
                    .unwrap()
                    .effects()
                    .iter()
                    .map(Effect::instance)
                    .collect();
                let result = if present.is_empty() || self.below(2) == 0 {
                    self.laid
                        .project
                        .add_effect_to_the_group(group, EffectKind::HUE_SHIFT)
                        .map(|_| ())
                } else {
                    let instance = present[self.below(present.len())];
                    self.laid.project.remove_effect_from_the_group(group, instance)
                };
                ("an effect of a group", result.is_ok(), format!("{group}"))
            }
            95..=97 => {
                let owner = match self.below(3) {
                    0 => self.some_clip().map(Member::Clip),
                    1 => Some(Member::Track(self.some_track())),
                    _ => self.some_group(None).map(Member::Group),
                };
                let Some(owner) = owner else {
                    return ("move a thing in a chain", false, "nothing".to_owned());
                };
                let keys: Vec<_> = self
                    .laid
                    .project
                    .chain_rows_of(owner)
                    .iter()
                    .filter_map(ChainRow::key)
                    .collect();
                if keys.is_empty() {
                    // Give it something to move: a clip or a track gets an effect.
                    let target = match owner {
                        Member::Clip(clip) => Some(crate::project::AnimationTarget::Clip(clip)),
                        Member::Track(track) => Some(crate::project::AnimationTarget::Track(track)),
                        Member::Group(_) => None,
                    };
                    let added = target
                        .map(|target| self.laid.project.add_effect(target, EffectKind::HUE_SHIFT).is_ok())
                        .unwrap_or(false);
                    return ("an effect of a clip or track", added, format!("{owner:?}"));
                }
                let item = keys[self.below(keys.len())];
                let before = if self.below(3) == 0 {
                    None
                } else {
                    Some(keys[self.below(keys.len())])
                };
                (
                    "move a thing in a chain",
                    self.laid.project.move_chain_item(owner, item, before).is_ok(),
                    format!("{item:?} before {before:?} in the chain of {owner:?}"),
                )
            }
            98 => {
                let rates = [24, 25, 30, 48, 60];
                let rate = FrameRate::new(rates[self.below(rates.len())], 1).unwrap();
                (
                    "count again at a frame rate",
                    self.laid.project.try_set_frame_rate(rate).is_ok(),
                    format!("{rate}"),
                )
            }
            100..=102 => {
                // (A crate is a clip like the others: one that is put inside the rectangle of a clip group is a clip in it that is not in it.)
                let track = self.some_track();
                let frame = TimelineFrameIndex::new(self.random.next_below(150));
                let frames = 1 + self.random.next_below(30);
                let made = self.laid.project.insert_crate(track, frame, frames);
                if let Ok(crate_clip) = made {
                    // (A crate that has nothing in it does not stay: this one has a rack of the loudness of its track, with a keyframe at
                    // each end, so that it lasts as long as it was made to.)
                    put_a_rack_in(
                        &mut self.laid.project,
                        AnimationTarget::Clip(crate_clip),
                        AnimationTarget::Track(track),
                        PropertyId::Gain,
                        &[(0, db(0.0)), (frames - 1, db(-6.0))],
                    );
                }
                (
                    "put a keyframe crate on a track",
                    made.is_ok(),
                    format!("{track} at {frame} for {frames}"),
                )
            }
            103..=105 => {
                let target = match self.some_clip() {
                    Some(clip) if self.below(4) > 0 => AnimationTarget::Clip(clip),
                    _ => AnimationTarget::Track(self.some_track()),
                };
                let properties = match target {
                    AnimationTarget::Clip(_) => [PropertyId::Rotation, PropertyId::Position, PropertyId::Size],
                    AnimationTarget::Track(_) => [PropertyId::Gain, PropertyId::Muted, PropertyId::Gain],
                };
                let property = properties[self.below(properties.len())];
                let frame = TimelineFrameIndex::new(self.random.next_below(150));
                (
                    "put a keyframe",
                    self.laid.project.add_keyframe(target, property, frame).is_ok(),
                    format!("{property:?} of {target:?} at {frame}"),
                )
            }
            110..=113 => {
                let Some(group) = self.some_group(Some(GroupKind::ClipGroup)) else {
                    return ("move an edge of a clip group", false, "no clip group".to_owned());
                };
                let Some(rectangle) = self.laid.project.rectangle_of_the_group(group) else {
                    return ("move an edge of a clip group", false, "nothing in it".to_owned());
                };
                let frame = self.random.next_below(200);
                let (first_frame, end_frame) = if self.below(2) == 0 {
                    (frame, rectangle.end_frame)
                } else {
                    (rectangle.first_frame, frame)
                };
                (
                    "move an edge of a clip group",
                    self.laid
                        .project
                        .set_the_frames_of_the_clip_group(group, first_frame, end_frame)
                        .is_ok(),
                    format!("{group} to {first_frame}..{end_frame}"),
                )
            }
            106..=109 => {
                let racks: Vec<RackId> = self
                    .laid
                    .project
                    .located_racks()
                    .map(|located| located.rack.id())
                    .collect();
                if racks.is_empty() {
                    return ("move a rack", false, "no rack".to_owned());
                }
                let rack = racks[self.below(racks.len())];
                let to = if self.below(2) == 0 {
                    let from_frame = self.random.next_below(100);
                    RackDestination::NewCrate {
                        track: self.some_track(),
                        from_frame,
                        until_frame: from_frame + 1 + self.random.next_below(200),
                    }
                } else {
                    RackDestination::Drawer(match self.some_clip() {
                        Some(clip) if self.below(3) > 0 => AnimationTarget::Clip(clip),
                        _ => AnimationTarget::Track(self.some_track()),
                    })
                };
                let shift = i64::try_from(self.random.next_below(81)).unwrap() - 40;
                (
                    "move a rack",
                    self.laid.project.move_racks(&[rack], to, shift, None).is_ok(),
                    format!("{rack} to {to:?} by {shift}"),
                )
            }
            _ => {
                let Some(group) = self.some_group(None) else {
                    return ("rename a group", false, "no group".to_owned());
                };
                (
                    "rename a group",
                    self.laid.project.rename_group(group, "renamed").is_ok(),
                    format!("{group}"),
                )
            }
        }
    }
}

/// A project that has had `edits` random edits made to it, which is what a seed makes it: for the tests of other things that are about
/// projects that have groups of every kind in them, in every state.
pub(crate) fn project_after_random_edits(seed: u64, edits: usize) -> Project {
    let mut run = Randomly::new(seed);
    for _ in 0..edits {
        run.edit_at_random();
    }
    run.laid.project
}

/// [`project_after_random_edits`], and then as many more clip groups as there is room for: clips that are near one another, tried 20 times,
/// which the rules let be a group where nothing that is not in it is inside the rectangle they make. (A clip group is seldom what is left
/// at the end of random edits, which take it apart as often as they make it.)
pub(crate) fn project_after_random_edits_with_more_clip_groups(seed: u64, edits: usize) -> Project {
    let mut run = Randomly::new(seed);
    for _ in 0..edits {
        run.edit_at_random();
    }
    for _ in 0..20 {
        let members = run.some_clips_near_each_other();
        if members.len() >= 2 {
            // (Refused when it breaks a rule, which it often does: something is inside, or a clip is in a clip group already.)
            let _ = run.laid.project.create_group(GroupKind::ClipGroup, &members);
        }
    }
    run.laid.project
}

#[test]
fn whatever_edits_are_made_in_whatever_order_no_rule_of_groups_is_left_broken_and_an_edit_that_is_refused_changes_nothing()
 {
    let mut tally = Tally::default();
    for seed in 1..=60 {
        let mut run = Randomly::new(seed);
        assert_eq!(run.laid.project.problems_with_the_groups(), []);
        for _ in 0..400 {
            run.edit_at_random();
        }
        tally.add(&run.tally);
    }
    if std::env::var_os("FASTFORWARDRS_SHOW_RANDOM_EDIT_STATISTICS").is_some() {
        println!(
            "made: {:#?}\nrefused: {:#?}\nreached: {:#?}",
            tally.made, tally.refused, tally.reached
        );
    }
    // The arrangements that the rules are worth checking in: the random edits come to each of them, many times.
    for arrangement in [
        "a tag that has a tag in it",
        "a tag that has a clip group in it",
        "a tag that has a track group in it",
        "a clip group that has a clip group in it",
        "a clip group of three or more",
        "a track group that has a track group in it",
        "a track group of three or more",
        "a group that has effects and is in a tag",
        "clip groups and track groups at once",
        "a clip that is in two tags",
        "a clip whose badge of its track is not at the end",
        "a clip that has an effect and a badge",
    ] {
        assert!(
            tally.reached.get(arrangement).copied().unwrap_or(0) >= 50,
            "{arrangement} was reached too few times (counting every edit it lasted over): {:?}",
            tally.reached
        );
    }
    // The test is only worth anything if the edits that matter are really made, and really refused, and not only one of the two.
    for edit in [
        "create a tag",
        "create a clip group",
        "create a track group",
        "add to a group",
        "take out of a group",
        "disband a group",
        "delete a group and all in it",
        "delete a clip",
        "cut at a frame",
        "duplicate a clip",
        "duplicate a track",
        "move clips",
        "move a track",
        "change how long a clip lasts",
        "put a text object on a track",
        "put a keyframe crate on a track",
        "put a keyframe",
        "move a rack",
        "move an edge of a clip group",
        "an effect of a group",
        "move a thing in a chain",
    ] {
        assert!(
            tally.made.get(edit).copied().unwrap_or(0) >= 20,
            "{edit} was made too few times: {:?}",
            tally.made
        );
    }
    for edit in [
        "create a clip group",
        "create a track group",
        "add to a group",
        "take out of a group",
        "move clips",
        "move a track",
        "duplicate a clip",
        "change how long a clip lasts",
        "put a text object on a track",
        "put a keyframe crate on a track",
        "move an edge of a clip group",
    ] {
        // (Ten: a longer clip is refused only when the longer rectangle of its clip group would take in another clip, which is rarer than a
        // move that does not fit.)
        assert!(
            tally.refused.get(edit).copied().unwrap_or(0) >= 10,
            "{edit} was refused too few times: {:?}",
            tally.refused
        );
    }
}

#[test]
fn the_same_seed_makes_the_same_edits() {
    let log_of = |seed| {
        let mut run = Randomly::new(seed);
        for _ in 0..60 {
            run.edit_at_random();
        }
        run.log
    };
    assert_eq!(log_of(5), log_of(5));
    assert_ne!(log_of(5), log_of(6));
}
