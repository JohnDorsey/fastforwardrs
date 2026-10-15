// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The groups in a project file (version 10 and after): what a group is as the file has it, and the chains of effects with the badges of
//! the tags in them.
//!
//! A group is named in the file by its place in the list of groups (counted from 0), as the keyframes name the clips and tracks they are of by
//! where they are: the names that groups, clips and tracks are known by when a project is edited are made up when it is loaded, and are never
//! written. What is in a group is by position too: a clip is a track number and a clip number, a track a track number, a group a group number.
//!
//! A badge is written with how many of the effects of its chain come before it, which is all there is to know about where it is: the chain
//! `[effect, badge of a tag, effect, badge of another tag]` is two badges, one after one effect and one after two. (A badge that is the
//! badge of the track of a clip is only written when it is not at the end of the chain, which is where it is when nothing says otherwise.)

use std::collections::HashMap;

use super::document::{
    BadgeDocument, BadgeOfDocument, GroupColourDocument, GroupDocument, GroupKindDocument, MemberDocument,
};
use super::drawers::Positions;
use super::effects::{EffectDocument, KindsOfEffects, effect_documents_of, effects_from_documents};
use crate::project::{
    Badge, ChainEntry, ClipId, EffectKind, Effects, Group, GroupId, GroupKind, Member, Project, TrackId,
};

/// The chain that the effects and the badges of a clip or a group make, if they make one: effects of kinds that it can have, each with a name
/// of its own in the chain, and a badge once and in a place that there is, of a tag that there is (`group_count` is how many groups the
/// file has).
pub(super) fn chain_from_documents(
    effects: Vec<EffectDocument>,
    badges: Vec<BadgeDocument>,
    kinds: &KindsOfEffects,
    can_have: impl Fn(EffectKind) -> bool,
    group_count: usize,
) -> Result<Effects, String> {
    let effects = effects_from_documents(effects, kinds, can_have)?;
    let effect_count = effects.len();
    let mut places_of_the_badges: Vec<(usize, Badge)> = Vec::with_capacity(badges.len());
    for document in badges {
        if document.effects_before > effect_count {
            return Err(format!(
                "a badge is said to come after {} of the effects, and there are {effect_count}",
                document.effects_before
            ));
        }
        let badge = match document.of {
            BadgeOfDocument::Track => Badge::Track,
            BadgeOfDocument::Tag(number) if number < group_count => {
                Badge::Tag(GroupId::new(u32::try_from(number).unwrap_or(u32::MAX)))
            }
            BadgeOfDocument::Tag(number) => {
                return Err(format!(
                    "a badge is of the tag number {number}, and the file has {group_count} groups (counted from 0)"
                ));
            }
        };
        places_of_the_badges.push((document.effects_before, badge));
    }
    let mut entries = Vec::with_capacity(effect_count + places_of_the_badges.len());
    let mut effects = effects.iter();
    for place in 0..=effect_count {
        entries.extend(
            places_of_the_badges
                .iter()
                .filter(|(effects_before, _)| *effects_before == place)
                .map(|(_, badge)| ChainEntry::Badge(*badge)),
        );
        entries.extend(effects.next().cloned().map(ChainEntry::Effect));
    }
    Effects::from_entries(entries).ok_or_else(|| "it has the same badge twice".to_owned())
}

/// The badges that are kept in `chain`, as the file has them, with the number of the group of each badge of a tag (see `numbers`).
pub(super) fn badge_documents_of(
    chain: &Effects,
    numbers: &HashMap<GroupId, usize>,
) -> Result<Vec<BadgeDocument>, String> {
    let mut effects_before = 0;
    let mut documents = Vec::new();
    for entry in chain.entries() {
        match entry {
            ChainEntry::Effect(_) => effects_before += 1,
            ChainEntry::Badge(Badge::Track) => documents.push(BadgeDocument {
                of: BadgeOfDocument::Track,
                effects_before,
            }),
            ChainEntry::Badge(Badge::Tag(group)) => documents.push(BadgeDocument {
                of: BadgeOfDocument::Tag(
                    *numbers
                        .get(&group)
                        .ok_or_else(|| format!("a chain has the badge of {group}, which is not in the project"))?,
                ),
                effects_before,
            }),
        }
    }
    Ok(documents)
}

fn kind_from_document(kind: GroupKindDocument) -> GroupKind {
    match kind {
        GroupKindDocument::Tag => GroupKind::Tag,
        GroupKindDocument::ClipGroup => GroupKind::ClipGroup,
        GroupKindDocument::TrackGroup => GroupKind::TrackGroup,
    }
}

fn kind_document_of(kind: GroupKind) -> GroupKindDocument {
    match kind {
        GroupKind::Tag => GroupKindDocument::Tag,
        GroupKind::ClipGroup => GroupKindDocument::ClipGroup,
        GroupKind::TrackGroup => GroupKindDocument::TrackGroup,
    }
}

/// The thing that a member of a group in the file is, if the file has it: `clips_in_each_track` is how many clips each track of the file has.
fn member_from_document(
    document: MemberDocument,
    clips_in_each_track: &[usize],
    group_count: usize,
) -> Result<Member, String> {
    let track_id = |track: usize| TrackId::new(u32::try_from(track).unwrap_or(u32::MAX));
    match document {
        MemberDocument::Clip { track, clip } => {
            let clips = clips_in_each_track
                .get(track)
                .ok_or_else(|| format!("there is no track {track} in the file to have a clip in the group"))?;
            if clip >= *clips {
                return Err(format!("there is no clip {clip} on track {track} to be in the group"));
            }
            Ok(Member::Clip(ClipId::new(
                track_id(track),
                u32::try_from(clip).unwrap_or(u32::MAX),
            )))
        }
        MemberDocument::Track(track) if track < clips_in_each_track.len() => Ok(Member::Track(track_id(track))),
        MemberDocument::Track(track) => Err(format!("there is no track {track} in the file to be in the group")),
        MemberDocument::Group(group) if group < group_count => {
            Ok(Member::Group(GroupId::new(u32::try_from(group).unwrap_or(u32::MAX))))
        }
        MemberDocument::Group(group) => Err(format!("there is no group {group} in the file to be in the group")),
    }
}

/// The groups that the documents describe, in their order (the id of each is its place in the list), if they make sense in themselves: the
/// project is the one to say whether they agree with what it has (see `Project::with_groups`).
pub(super) fn groups_from_documents(
    documents: Vec<GroupDocument>,
    clips_in_each_track: &[usize],
    kinds: &KindsOfEffects,
) -> Result<Vec<Group>, String> {
    let group_count = documents.len();
    let mut groups = Vec::with_capacity(group_count);
    for (number, document) in documents.into_iter().enumerate() {
        let describe = |reason: String| format!("group number {}: {reason}", number + 1);
        let kind = kind_from_document(document.kind);
        let members = document
            .members
            .into_iter()
            .map(|member| member_from_document(member, clips_in_each_track, group_count))
            .collect::<Result<Vec<_>, _>>()
            .map_err(describe)?;
        let effects = chain_from_documents(
            document.effects,
            document.badges,
            kinds,
            |effect| effect.applies_to_a_group_of_kind(kind),
            group_count,
        )
        .map_err(describe)?;
        groups.push(
            Group::new(
                GroupId::new(u32::try_from(number).unwrap_or(u32::MAX)),
                kind,
                document.name,
                document.colour.0,
            )
            .with_members(members)
            .with_effects(effects),
        );
    }
    Ok(groups)
}

/// The documents of the groups of `project`, in their order, with `positions` saying where the clips and the tracks are in the file.
pub(super) fn group_documents_of(
    project: &Project,
    positions: &Positions,
    numbers: &HashMap<GroupId, usize>,
) -> Result<Vec<GroupDocument>, String> {
    project
        .groups()
        .iter()
        .map(|group| {
            Ok(GroupDocument {
                kind: kind_document_of(group.kind()),
                name: group.name().to_owned(),
                colour: GroupColourDocument(group.colour()),
                members: group
                    .members()
                    .iter()
                    .map(|member| positions.member_document_of(*member, numbers))
                    .collect::<Result<Vec<_>, _>>()?,
                effects: effect_documents_of(group.effects()),
                badges: badge_documents_of(group.effects(), numbers)?,
            })
        })
        .collect()
}

/// Which place in the list of groups each group has.
pub(super) fn numbers_of_the_groups(project: &Project) -> HashMap<GroupId, usize> {
    project
        .groups()
        .iter()
        .enumerate()
        .map(|(number, group)| (group.id(), number))
        .collect()
}
