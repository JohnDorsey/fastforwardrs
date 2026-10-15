// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Random commands in the window: the commands that make groups, change them and take them apart, together with the commands that edit
//! clips and tracks, as a person gives them one after another. After each, no rule of groups is broken and everything that is selected is in
//! the project; and what changed the project is one step of the history, which undoes to the project as it was and redoes to the project as
//! it was after the step (what is refused, with a message, changes nothing and is not a step).
//!
//! (The edits of the project itself are tried in random order in `project::groups::random_edits_tests`; these are about the commands: what
//! they do with a refusal, with the history, and with what is selected.)

use super::clip_drag_tests::project_of_tracks;
use super::test_window::Window;
use crate::application::commands::Command;
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::project::{
    AnimationTarget, ChainItemKey, ClipId, ClipMove, ClipSide, Effect, EffectInstanceId, EffectKind, GroupColour,
    GroupId, GroupKind, Member, Project, TimelineFrameIndex, TrackId, TrackSide,
};
use crate::test_support::DeterministicRandomNumbers;

/// A window with four tracks that have clips on them (eleven, with room between some of them).
fn window_with_clips_on_four_tracks() -> Window {
    Window::showing(project_of_tracks(&[
        &[(0, 20), (10, 20), (10, 20)],
        &[(5, 20), (20, 20)],
        &[(30, 20), (10, 20)],
        &[(0, 40), (30, 10)],
    ]))
}

/// What was done by kind, and how often each was made and unchanged.
#[derive(Default)]
struct Tally {
    made: std::collections::BTreeMap<&'static str, usize>,
    unchanged: std::collections::BTreeMap<&'static str, usize>,
    undone_and_redone: usize,
}

struct Fuzz {
    window: Window,
    random: DeterministicRandomNumbers,
    tally: Tally,
}

impl Fuzz {
    fn new(seed: u64) -> Self {
        let mut run = Self {
            window: window_with_clips_on_four_tracks(),
            random: DeterministicRandomNumbers::with_seed(seed),
            tally: Tally::default(),
        };
        // (A tag, and an effect on a clip of it and on the track the clip is on, to begin with: so that there are chains with more than one
        // thing in them to put in another order from the first step, which random commands would seldom come to.)
        let clips = run.clips();
        let tracks = run.tracks();
        for command in [
            Command::CreateGroup {
                kind: GroupKind::Tag,
                members: clips[..3].iter().copied().map(Member::Clip).collect(),
            },
            Command::AddEffect {
                target: AnimationTarget::Clip(clips[0]),
                kind: EffectKind::HUE_SHIFT,
            },
            Command::AddEffect {
                target: AnimationTarget::Track(tracks[0]),
                kind: EffectKind::HUE_SHIFT,
            },
        ] {
            run.window.do_this_and_draw_nothing(command);
        }
        run
    }

    fn below(&mut self, bound: usize) -> usize {
        self.random.next_below(bound as u64) as usize
    }

    fn project(&self) -> &Project {
        self.window.application().project()
    }

    fn clips(&self) -> Vec<ClipId> {
        self.project()
            .tracks()
            .iter()
            .flat_map(|track| track.entries().iter().map(|entry| entry.id()))
            .collect()
    }

    fn tracks(&self) -> Vec<TrackId> {
        self.project().tracks().iter().map(|track| track.id()).collect()
    }

    fn groups(&self, kind: Option<GroupKind>) -> Vec<GroupId> {
        self.project()
            .groups()
            .iter()
            .filter(|group| kind.is_none_or(|kind| group.kind() == kind))
            .map(|group| group.id())
            .collect()
    }

    fn some<T: Copy>(&mut self, from: &[T]) -> Option<T> {
        (!from.is_empty()).then(|| from[self.below(from.len())])
    }

    /// One clip, and some of those that are near it (a few frames away, on its track or the next).
    fn some_clips_near_each_other(&mut self) -> Vec<Member> {
        let clips = self.clips();
        let Some(first) = self.some(&clips) else {
            return Vec::new();
        };
        let place = self.project().rectangle_of_the_clip(first).unwrap();
        let mut members = vec![Member::Clip(first)];
        for clip in clips {
            let other = self.project().rectangle_of_the_clip(clip).unwrap();
            let near = clip != first
                && members.len() < 4
                && other.first_track.abs_diff(place.first_track) <= 1
                && other.first_frame.abs_diff(place.first_frame) <= 40
                && self.below(2) == 0;
            if near {
                members.push(Member::Clip(clip));
            }
        }
        members
    }

    /// What names each thing in the chain of `owner` that can be moved in it.
    fn keys_of_the_chain_of(&self, owner: Member) -> Vec<ChainItemKey> {
        self.project()
            .chain_rows_of(owner)
            .iter()
            .filter_map(|row| row.key())
            .collect()
    }

    /// Something that has a chain of effects that has at least two things in it that can be moved (a clip, a track or a group), if there is one.
    fn some_owner_of_a_chain_that_can_be_put_in_another_order(&mut self) -> Option<Member> {
        let mut owners: Vec<Member> = self.clips().into_iter().map(Member::Clip).collect();
        owners.extend(self.tracks().into_iter().map(Member::Track));
        owners.extend(self.groups(None).into_iter().map(Member::Group));
        owners.retain(|owner| self.keys_of_the_chain_of(*owner).len() >= 2);
        self.some(&owners)
    }

    /// A command that is worth trying now, and what to call it.
    fn a_command(&mut self) -> Option<(&'static str, Command)> {
        let clips = self.clips();
        let tracks = self.tracks();
        Some(match self.below(29) {
            0..=2 => {
                let count = 1 + self.below(4);
                let members: Vec<Member> = (0..count).filter_map(|_| self.some(&clips)).map(Member::Clip).collect();
                (
                    "make a tag",
                    Command::CreateGroup {
                        kind: GroupKind::Tag,
                        members,
                    },
                )
            }
            3..=5 => (
                "make a clip group",
                Command::CreateGroup {
                    kind: GroupKind::ClipGroup,
                    members: self.some_clips_near_each_other(),
                },
            ),
            6 => {
                let first = self.below(tracks.len());
                let last = (first + self.below(3)).min(tracks.len() - 1);
                (
                    "make a track group",
                    Command::CreateGroup {
                        kind: GroupKind::TrackGroup,
                        members: tracks[first..=last].iter().copied().map(Member::Track).collect(),
                    },
                )
            }
            7 => {
                let tags = self.groups(Some(GroupKind::Tag));
                let group = self.some(&tags)?;
                let members = vec![Member::Clip(self.some(&clips)?)];
                ("add to a tag", Command::AddToGroup { group, members })
            }
            8 | 9 => {
                let groups = self.groups(None);
                let group = self.some(&groups)?;
                let members = self.project().group(group)?.members().to_vec();
                let member = self.some(&members)?;
                ("take out of a group", Command::RemoveFromGroup { group, member })
            }
            10 => {
                let groups = self.groups(None);
                ("disband a group", Command::DisbandGroup(self.some(&groups)?))
            }
            11 => {
                // (Seldom: it deletes clips and tracks, and the project would soon be empty.)
                if self.below(5) != 0 {
                    return None;
                }
                let groups = self.groups(None);
                (
                    "delete a group and all that is in it",
                    Command::DeleteGroupAndEverythingInIt(self.some(&groups)?),
                )
            }
            12 => {
                let groups = self.groups(None);
                (
                    "put an effect on a group",
                    Command::AddGroupEffect {
                        group: self.some(&groups)?,
                        kind: EffectKind::HUE_SHIFT,
                    },
                )
            }
            13 => {
                let groups = self.groups(None);
                let group = self.some(&groups)?;
                let on_it: Vec<EffectInstanceId> = self
                    .project()
                    .group(group)?
                    .effects()
                    .iter()
                    .map(Effect::instance)
                    .collect();
                // (One that is not on it, when none is: the command is refused, and the project is as it was.)
                let instance = self.some(&on_it).unwrap_or(EffectInstanceId::FIRST);
                (
                    "take an effect off a group",
                    Command::RemoveGroupEffect { group, instance },
                )
            }
            14 => {
                let target = if self.below(3) == 0 {
                    AnimationTarget::Track(self.some(&tracks)?)
                } else {
                    AnimationTarget::Clip(self.some(&clips)?)
                };
                if self.below(3) == 0 {
                    let on_it: Vec<EffectInstanceId> = self
                        .project()
                        .effects_of(target)?
                        .iter()
                        .map(Effect::instance)
                        .collect();
                    let instance = self.some(&on_it).unwrap_or(EffectInstanceId::FIRST);
                    return Some((
                        "take an effect off a clip or a track",
                        Command::RemoveEffect { target, instance },
                    ));
                }
                (
                    "put an effect on a clip or a track",
                    Command::AddEffect {
                        target,
                        kind: EffectKind::HUE_SHIFT,
                    },
                )
            }
            15..=17 => {
                let owner = self.some_owner_of_a_chain_that_can_be_put_in_another_order()?;
                let keys = self.keys_of_the_chain_of(owner);
                let item = self.some(&keys)?;
                let before = if self.below(4) == 0 { None } else { self.some(&keys) };
                (
                    "move something in a chain",
                    Command::MoveChainItem { owner, item, before },
                )
            }
            18 => {
                let groups = self.groups(None);
                let group = self.some(&groups)?;
                if self.below(2) == 0 {
                    let name = format!("name {}", self.below(100));
                    ("name a group", Command::RenameGroup { group, name })
                } else {
                    let colour = GroupColour::new(self.below(256) as u8, self.below(256) as u8, self.below(256) as u8);
                    ("colour a group", Command::SetGroupColour { group, colour })
                }
            }
            19 => (
                "copy a clip",
                Command::DuplicateClips {
                    clips: vec![self.some(&clips)?],
                    side: if self.below(2) == 0 {
                        ClipSide::Left
                    } else {
                        ClipSide::Right
                    },
                },
            ),
            20 => {
                let clip = self.some(&clips)?;
                let place = self.project().rectangle_of_the_clip(clip).unwrap();
                let frames_inside = place.end_frame.saturating_sub(place.first_frame + 1).max(1);
                let frame = place.first_frame + 1 + self.random.next_below(frames_inside);
                (
                    "cut a clip",
                    Command::SplitAt {
                        frame: TimelineFrameIndex::new(frame),
                        only: Some(vec![clip]),
                    },
                )
            }
            21 => ("delete a clip", Command::DeleteClips(vec![self.some(&clips)?])),
            22 | 23 => {
                let clip = self.some(&clips)?;
                let to_track = self.some(&tracks)?;
                let start = TimelineFrameIndex::new(self.below(120) as u64);
                (
                    "move a clip",
                    Command::move_clips(vec![ClipMove { clip, to_track, start }]),
                )
            }
            24 => match self.below(4) {
                0 => ("add a track", Command::AddTrack),
                1 => ("delete a track", Command::DeleteTrack(self.some(&tracks)?)),
                2 => (
                    "copy a track",
                    Command::DuplicateTrack {
                        track: self.some(&tracks)?,
                        side: if self.below(2) == 0 {
                            TrackSide::Above
                        } else {
                            TrackSide::Below
                        },
                    },
                ),
                _ => (
                    "move a track",
                    Command::MoveTrack {
                        track: self.some(&tracks)?,
                        to_position: self.below(tracks.len()),
                    },
                ),
            },
            26..=27 => {
                let groups = self.groups(Some(GroupKind::ClipGroup));
                let group = self.some(&groups)?;
                let frame = self.below(200) as u64;
                let rectangle = self.project().rectangle_of_the_group(group)?;
                let (first_frame, end_frame) = if self.below(2) == 0 {
                    (frame, rectangle.end_frame)
                } else {
                    (rectangle.first_frame, frame)
                };
                (
                    "move an edge of a clip group",
                    Command::SetTheFramesOfAClipGroup {
                        group,
                        first_frame,
                        end_frame,
                    },
                )
            }
            _ => {
                let groups = self.groups(None);
                let group = self.some(&groups)?;
                (
                    "select a group",
                    Command::SelectItems {
                        items: vec![SelectionItem::Group(group)],
                        change: SelectionChange::Replace,
                    },
                )
            }
        })
    }

    /// Does `command` as the window does what a person asks for, and draws one frame after it. A message that says why it was refused is read
    /// (it goes away), and what was refused is told by the project being as it was.
    fn do_it(&mut self, command: Command) {
        self.window.do_this_and_draw_nothing(command);
        self.window.application_mut().interface.dialog = None;
        self.window.harness.step();
    }

    fn check_the_rules_and_what_is_selected(&self, context: &str) {
        assert_eq!(
            self.project().problems_with_the_groups(),
            vec![],
            "a rule of groups is broken after {context}"
        );
        for item in self.window.application().selected_items() {
            assert!(
                item.is_in(self.project()),
                "{item:?} is selected and is not there after {context}"
            );
        }
    }

    /// One random command, and the checks.
    fn step(&mut self, number: usize) {
        let Some((name, command)) = self.a_command() else {
            return;
        };
        let before = self.project().clone();
        let context = format!("step {number}, {name}: {command:?}");
        self.do_it(command.clone());
        self.check_the_rules_and_what_is_selected(&context);
        let after = self.project().clone();
        if after == before {
            *self.tally.unchanged.entry(name).or_default() += 1;
            return;
        }
        *self.tally.made.entry(name).or_default() += 1;
        // (What a gesture makes over and over is joined to the step before it if it is about the same: those are not looked at here.)
        if command.gesture_key().is_some() {
            return;
        }
        self.do_it(Command::Undo);
        assert_eq!(
            self.project(),
            &before,
            "undoing {context} does not give the project as it was"
        );
        self.check_the_rules_and_what_is_selected(&format!("undoing {context}"));
        self.do_it(Command::Redo);
        assert_eq!(
            self.project(),
            &after,
            "redoing {context} does not give the project as it was after it"
        );
        self.check_the_rules_and_what_is_selected(&format!("redoing {context}"));
        self.tally.undone_and_redone += 1;
    }
}

#[test]
fn whatever_commands_are_given_in_whatever_order_no_rule_of_groups_is_left_broken_what_is_selected_is_there_and_each_edit_is_one_step_of_the_history()
 {
    let mut tally = Tally::default();
    for seed in 1..=8u64 {
        let mut run = Fuzz::new(seed);
        for number in 0..40 {
            run.step(number);
        }
        for (name, count) in &run.tally.made {
            *tally.made.entry(name).or_default() += count;
        }
        for (name, count) in &run.tally.unchanged {
            *tally.unchanged.entry(name).or_default() += count;
        }
        tally.undone_and_redone += run.tally.undone_and_redone;
    }
    if std::env::var_os("FASTFORWARDRS_SHOW_RANDOM_EDIT_STATISTICS").is_some() {
        println!(
            "made: {:#?}\nunchanged: {:#?}\nundone and redone: {}",
            tally.made, tally.unchanged, tally.undone_and_redone
        );
    }
    // The commands that the rules are worth checking after are made, many times (measured: 21 tags, 10 clip groups, 7 track groups, 7 additions to
    // a tag, 37 takings out of a group, 11 groups disbanded, 8 effects put on a group, 7 things put in another order in a chain, and 167 steps of the
    // history undone and redone).
    for (command, at_least) in [
        ("make a tag", 10),
        ("make a clip group", 5),
        ("make a track group", 3),
        ("add to a tag", 3),
        ("take out of a group", 15),
        ("disband a group", 5),
        ("put an effect on a group", 4),
        ("move something in a chain", 3),
        ("name a group", 2),
        ("colour a group", 2),
    ] {
        let made = tally.made.get(command).copied().unwrap_or(0);
        assert!(
            made >= at_least,
            "{command:?} was made {made} times, and the test is worth little if it is not made at least {at_least}"
        );
    }
    assert!(
        tally.undone_and_redone >= 80,
        "{} steps of the history were undone and redone",
        tally.undone_and_redone
    );
}
