// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The effects of groups, and what is applied to a clip when everything it is in and on is counted.
//!
//! A group has a chain of effects like a clip or a track (see [`crate::project::Effects`]). What the chain of a group does depends on
//! the kind of the group:
//!
//! * the effects of a **tag** are applied to each member, one at a time, at the place in the member's own chain where the member has the
//!   badge of the tag. (A clip has the effects of its tags in the middle of its own, if its badges are there.) That is what a tag
//!   means, so it is exact;
//! * the effects of a **clip group** and of a **track group** are meant to be applied to the picture that the members make *together*:
//!   after the clips of the group have been laid over one another, and after the effects of the clips and of their tracks, and
//!   of the clip groups and track groups that are in this one.
//!
//! # The picture of a group (phase 5)
//!
//! **The engine draws the members of a clip group or a track group into a picture of their own, and does the effects of the group to that picture** (see
//! `docs/engine-phase-5-design.md`: `Project::effects_applied_to_the_group` gives them, `PictureSection::groups` carries them to the engine). A group that has no
//! effect that does anything is pass-through: its members are drawn as if it was not there. So a clip's own picture is made with [`Project::effects_applied_to_every_clip_by_itself`]
//! (its own effects, its track's and its tags' where the badges are), and **the effects of the clip groups and track groups it is in are not among them**.
//!
//! [`Project::effects_applied_to_the_clip`] and [`Project::effects_applied_to_every_clip`] still give *every* effect that concerns a clip, the groups' last, for what asks
//! "which effects does this clip have" (the sound, which has no effects of groups; a check of which effects the program does not have): they are not the order a picture is
//! made in.
//!
//! Until phase 5 the effects of a clip group or a track group were a **placeholder**: they were applied to the picture of each member by itself, which is the same
//! picture only where the members are not see-through (and off by up to about 100 of the 255 levels of a colour at the see-through edges of layers that are over others:
//! the tests of this module with `placeholder` in their names work that out, and are kept as the record of what was wrong). That is why a blur, a crop and an opacity were not
//! offered to a clip group or a track group, which they are now.
//!
//! The order the effects of a clip are applied in is: the effects of the clip itself, with those of its tracks and of its tags where their badges are; then those of the clip
//! groups it is in, the nearest first (each with its own tags, where its badges are); then those of the track groups its track is in, the nearest first.

use std::collections::HashMap;

use super::{Group, GroupId, Member};
use crate::project::tracks::{ClipId, ClipKind, Track};
use crate::project::{
    AnimationTarget, Badge, ChainItem, ChainItemKey, EditError, Effect, EffectInstanceId, EffectKind, Effects,
    MOST_EFFECTS_ON_ONE_THING, Project, PropertyId, PropertyValue,
};

/// What has a chain of effects: a clip, a track or a group.
pub type EffectOwner = Member;

impl From<AnimationTarget> for Member {
    fn from(target: AnimationTarget) -> Self {
        match target {
            AnimationTarget::Clip(clip) => Self::Clip(clip),
            AnimationTarget::Track(track) => Self::Track(track),
        }
    }
}

/// An effect that is applied to a clip, and whose chain it is in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppliedEffect<'project> {
    /// The clip, the track or the group that has the effect in its chain. (The keyframes of its values are the keyframes of the clip or the
    /// track that has it; a group's effects have none.)
    pub owner: EffectOwner,
    pub effect: &'project Effect,
}

/// One row of what the properties panel shows of a chain, from the first thing that is applied to the last.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChainRow<'project> {
    Effect(&'project Effect),
    /// The badge of a tag that the owner of the chain is in.
    TagBadge(&'project Group),
    /// The badge of the track that a clip is on, which is there when the track has effects that apply to the clip.
    TrackBadge {
        track: &'project Track,
        /// How many of the track's effects apply to the clip.
        effects_that_apply: usize,
    },
    /// The clip group or track group that the owner of the chain is directly in. It is always last, and cannot be moved.
    ShapeBadge(&'project Group),
}

impl ChainRow<'_> {
    /// What names the row among the others of the chain, if it can be moved to another place in it.
    pub fn key(&self) -> Option<ChainItemKey> {
        match self {
            Self::Effect(effect) => Some(ChainItemKey::Effect(effect.instance())),
            Self::TagBadge(group) => Some(ChainItemKey::Badge(Badge::Tag(group.id()))),
            Self::TrackBadge { .. } => Some(ChainItemKey::Badge(Badge::Track)),
            Self::ShapeBadge(_) => None,
        }
    }
}

/// The error for something that is not there to have a chain.
fn nothing_has_the_chain(owner: EffectOwner) -> EditError {
    match owner {
        Member::Clip(clip) => EditError::Clip(crate::project::ClipEditError::UnknownClip { clip }),
        Member::Track(track) => EditError::UnknownTrack { track },
        Member::Group(group) => EditError::UnknownGroup { group },
    }
}

impl Project {
    /// What the properties panel shows of the chain of `owner`, row by row, in the order it is applied in: its effects and the badges that
    /// are in the chain, and last, the clip group or the track group that `owner` is directly in. The badge of the track is only a row when
    /// the track has effects that apply to the clip. (Nothing, if there is no such thing.)
    pub fn chain_rows_of(&self, owner: EffectOwner) -> Vec<ChainRow<'_>> {
        let Some(chain) = self.chain_of_the_member(owner) else {
            return Vec::new();
        };
        let is_a_clip = matches!(owner, Member::Clip(_));
        let mut rows = Vec::new();
        for item in chain.items(is_a_clip) {
            match item {
                ChainItem::Effect(effect) => rows.push(ChainRow::Effect(effect)),
                ChainItem::Badge(Badge::Tag(tag)) => {
                    if let Some(group) = self.group(tag) {
                        rows.push(ChainRow::TagBadge(group));
                    }
                }
                ChainItem::Badge(Badge::Track) => {
                    if let Member::Clip(clip) = owner
                        && let Some(track) = self.track_holding(clip)
                        && let Some(placement) = self.placement_of(clip)
                    {
                        let kind = placement.clip.kind();
                        let effects_that_apply = track
                            .effects()
                            .iter()
                            .filter(|effect| effect.is_enabled() && effect.kind().applies_to_a_clip_of_kind(kind))
                            .count();
                        if effects_that_apply > 0 {
                            rows.push(ChainRow::TrackBadge {
                                track,
                                effects_that_apply,
                            });
                        }
                    }
                }
            }
        }
        if let Some(shape) = self.shape_group_of(owner) {
            rows.push(ChainRow::ShapeBadge(shape));
        }
        rows
    }

    /// Every effect that is applied to the picture of the clip `clip`, from the first applied to the last (see the module documentation for
    /// the order). Nothing, if there is no such clip. The effects that do not apply to a clip of its kind are left out.
    pub fn effects_applied_to_the_clip(&self, clip: ClipId) -> Vec<AppliedEffect<'_>> {
        let (Some(placement), Some(track)) = (self.placement_of(clip), self.track_holding(clip)) else {
            return Vec::new();
        };
        self.effects_applied_to_a_clip(
            &self.parents(),
            track,
            clip,
            placement.effects,
            placement.clip.kind(),
            true,
        )
    }

    /// What [`Self::effects_applied_to_the_clip`] gives for every clip there is, worked out together (which is quicker).
    pub fn effects_applied_to_every_clip(&self) -> HashMap<ClipId, Vec<AppliedEffect<'_>>> {
        self.effects_of_every_clip(true)
    }

    /// What [`Self::effects_applied_to_every_clip`] gives **without the effects of the clip groups and the track groups**: those of the clip, of its track and of its tags where the
    /// badges are. (The picture of a group is made from the pictures of its members and its own effects are done to that, see [`Self::effects_applied_to_the_group`]; the effects
    /// that are applied to a clip by itself are these.)
    pub fn effects_applied_to_every_clip_by_itself(&self) -> HashMap<ClipId, Vec<AppliedEffect<'_>>> {
        self.effects_of_every_clip(false)
    }

    fn effects_of_every_clip(&self, with_the_groups: bool) -> HashMap<ClipId, Vec<AppliedEffect<'_>>> {
        let parents = self.parents();
        let mut applied = HashMap::new();
        for track in &self.tracks {
            for entry in track.entries() {
                applied.insert(
                    entry.id(),
                    self.effects_applied_to_a_clip(
                        &parents,
                        track,
                        entry.id(),
                        entry.effects(),
                        entry.clip().kind(),
                        with_the_groups,
                    ),
                );
            }
        }
        applied
    }

    /// The effects that are done to the picture of the clip group or the track group `group`: its own, and where its chain has the badge of a tag, those of the tag, from the first
    /// done to the last. The ones that are switched off, and the ones that a group of its kind cannot have, are left out. (Nothing, if there is no such group.)
    pub fn effects_applied_to_the_group(&self, group: GroupId) -> Vec<AppliedEffect<'_>> {
        let Some(found) = self.group(group) else {
            return Vec::new();
        };
        let mut applied = Vec::new();
        self.add_the_effects_of_the_group(group, self.groups.len(), &mut applied);
        applied.retain(|applied| {
            applied.effect.is_enabled() && applied.effect.kind().applies_to_a_group_of_kind(found.kind())
        });
        applied
    }

    /// The clip groups and track groups that the clip `clip` is inside, **the outermost first** (the track groups of its track, and then the clip groups it is in): the order in
    /// which the pictures they make are made, from the one that holds all the others to the one that holds the clip. (Nothing, if there is no such clip.)
    pub fn shapes_round_the_clip_outermost_first(&self, clip: ClipId) -> Vec<GroupId> {
        let Some(track) = self.track_holding(clip) else {
            return Vec::new();
        };
        let parents = self.parents();
        let mut shapes = self.shapes_round(&parents, Member::Track(track.id()));
        shapes.reverse();
        let mut of_the_clip = self.shapes_round(&parents, Member::Clip(clip));
        of_the_clip.reverse();
        shapes.extend(of_the_clip);
        shapes
    }

    /// The effects applied to the clip `clip`, which is on `track`, has the chain `chain` and is of kind `kind`. (All of those are the
    /// project's.)
    fn effects_applied_to_a_clip<'project>(
        &'project self,
        parents: &super::queries::Parents,
        track: &'project Track,
        clip: ClipId,
        chain: &'project Effects,
        kind: ClipKind,
        with_the_groups: bool,
    ) -> Vec<AppliedEffect<'project>> {
        let mut applied: Vec<AppliedEffect<'project>> = Vec::new();
        for item in chain.items(true) {
            match item {
                ChainItem::Effect(effect) => applied.push(AppliedEffect {
                    owner: Member::Clip(clip),
                    effect,
                }),
                ChainItem::Badge(Badge::Track) => {
                    applied.extend(track.effects().iter().map(|effect| AppliedEffect {
                        owner: Member::Track(track.id()),
                        effect,
                    }));
                }
                ChainItem::Badge(Badge::Tag(tag)) => {
                    self.add_the_effects_of_the_group(tag, self.groups.len(), &mut applied)
                }
            }
        }
        if with_the_groups {
            let shapes_round_the_clip = self.shapes_round(parents, Member::Clip(clip));
            let shapes_round_the_track = self.shapes_round(parents, Member::Track(track.id()));
            for shape in shapes_round_the_clip.into_iter().chain(shapes_round_the_track) {
                self.add_the_effects_of_the_group(shape, self.groups.len(), &mut applied);
            }
        }
        // (An effect that is switched off is in its chain, and is not applied.)
        applied.retain(|applied| applied.effect.is_enabled() && applied.effect.kind().applies_to_a_clip_of_kind(kind));
        applied
    }

    /// Adds the effects of the chain of the group `group` to `applied`: its own, and where its chain has the badge of a tag, those of the
    /// tag, and so on. (`depth` is how many groups in groups it follows, so that a group in itself, which the rules do not allow, is not
    /// followed for ever.)
    fn add_the_effects_of_the_group<'project>(
        &'project self,
        group: GroupId,
        depth: usize,
        applied: &mut Vec<AppliedEffect<'project>>,
    ) {
        let Some(found) = self.group(group) else {
            return;
        };
        for item in found.effects().items(false) {
            match item {
                ChainItem::Effect(effect) => applied.push(AppliedEffect {
                    owner: Member::Group(group),
                    effect,
                }),
                ChainItem::Badge(Badge::Tag(tag)) => {
                    if let Some(depth) = depth.checked_sub(1) {
                        self.add_the_effects_of_the_group(tag, depth, applied);
                    }
                }
                ChainItem::Badge(Badge::Track) => {}
            }
        }
    }

    /// Puts the effect `kind` on the group `group`, last, with the values it begins with, and gives the name it has in the chain of the
    /// group. (There may be effects of that kind on it already: it is one more.) It is an error, and nothing is changed, if there is no such
    /// group, or a group of its kind cannot have the effect (see [`EffectKind::applies_to_a_group_of_kind`]).
    pub fn add_effect_to_the_group(&mut self, group: GroupId, kind: EffectKind) -> Result<EffectInstanceId, EditError> {
        // (A group that has an effect now may be one that takes part of a track whose effects are never divided: the rules of groups are checked.)
        self.keeping_the_groups_valid(|project| project.add_effect_to_the_group_without_the_rules(group, kind))
    }

    fn add_effect_to_the_group_without_the_rules(
        &mut self,
        group: GroupId,
        kind: EffectKind,
    ) -> Result<EffectInstanceId, EditError> {
        let found = self.group_mut(group).ok_or(EditError::UnknownGroup { group })?;
        if !kind.applies_to_a_group_of_kind(found.kind()) {
            return Err(EditError::NotAnEffectOfTheGroup { group, kind });
        }
        if found.effects_mut().is_full() {
            return Err(EditError::TooManyEffects {
                limit: MOST_EFFECTS_ON_ONE_THING,
            });
        }
        Ok(found.effects_mut().add(kind))
    }

    /// Takes the effect that is named `instance` off the group `group`. It is an error, and nothing is changed, if there is no such group
    /// or it does not have the effect.
    pub fn remove_effect_from_the_group(
        &mut self,
        group: GroupId,
        instance: EffectInstanceId,
    ) -> Result<(), EditError> {
        let found = self.group_mut(group).ok_or(EditError::UnknownGroup { group })?;
        if found.effects_mut().remove(instance) {
            Ok(())
        } else {
            Err(EditError::TheGroupDoesNotHaveThatEffect { group, instance })
        }
    }

    /// Gives the value of the effect of the group `group` that is `property` the value `value`. (The values of the effects of a group are
    /// not animated.) It is an error, and nothing is changed, if there is no such group, it does not have an effect that has that value,
    /// or `value` is not the kind of value it has.
    pub fn set_effect_value_of_the_group(
        &mut self,
        group: GroupId,
        property: PropertyId,
        value: &PropertyValue,
    ) -> Result<(), EditError> {
        let found = self.group_mut(group).ok_or(EditError::UnknownGroup { group })?;
        if found.effects_mut().set_value(property, value) {
            Ok(())
        } else {
            Err(EditError::NotAPropertyOfTheGroup { group, property })
        }
    }

    /// Switches the effect that is named `instance` in the chain of `owner` on or off (see [`Effect::is_enabled`]). It is an error, and nothing is changed, if
    /// there is no such thing as `owner`, or its chain has no such effect.
    pub fn set_effect_enabled(
        &mut self,
        owner: EffectOwner,
        instance: EffectInstanceId,
        enabled: bool,
    ) -> Result<(), EditError> {
        if matches!(owner, Member::Group(_)) {
            // (An effect of a group that is switched on may make it take part of a track whose effects are never divided: see `add_effect_to_the_group`.)
            return self.keeping_the_groups_valid(|project| {
                project.set_effect_enabled_without_the_rules(owner, instance, enabled)
            });
        }
        self.set_effect_enabled_without_the_rules(owner, instance, enabled)
    }

    fn set_effect_enabled_without_the_rules(
        &mut self,
        owner: EffectOwner,
        instance: EffectInstanceId,
        enabled: bool,
    ) -> Result<(), EditError> {
        if let Some(clip) = self.the_clip_whose_content_is(owner, instance) {
            return Err(EditError::TheContentOfAGenerator { clip });
        }
        let chain = self
            .chain_of_the_member_mut(owner)
            .ok_or_else(|| nothing_has_the_chain(owner))?;
        if chain.set_enabled(instance, enabled) {
            Ok(())
        } else {
            Err(EditError::NotInTheChain {
                owner,
                item: ChainItemKey::Effect(instance),
            })
        }
    }

    /// Gives the effect that is named `instance` in the chain of `owner` the state that its plugin saved (see [`Effect::state`]), or takes it away (`None`). It is an error, and
    /// nothing is changed, if there is no such thing as `owner`, or its chain has no such effect. (A state may be as big as the plugin makes it.)
    pub fn set_effect_state(
        &mut self,
        owner: EffectOwner,
        instance: EffectInstanceId,
        state: Option<std::sync::Arc<[u8]>>,
    ) -> Result<(), EditError> {
        let chain = self
            .chain_of_the_member_mut(owner)
            .ok_or_else(|| nothing_has_the_chain(owner))?;
        if chain.set_state(instance, state) {
            Ok(())
        } else {
            Err(EditError::NotInTheChain {
                owner,
                item: ChainItemKey::Effect(instance),
            })
        }
    }

    /// Puts a copy of the effect that is named `instance` in the chain of `owner` right after it, with the values it has (and whether it is switched off), named as the next
    /// effect of the chain is, and gives that name. **Its keyframes are not copied**: the values the copy has are the values the original has when it is not animated. It is an
    /// error, and nothing is changed, if there is no such thing as `owner`, or its chain has no such effect.
    pub fn duplicate_effect(
        &mut self,
        owner: EffectOwner,
        instance: EffectInstanceId,
    ) -> Result<EffectInstanceId, EditError> {
        if matches!(owner, Member::Group(_)) {
            return self
                .keeping_the_groups_valid(|project| project.duplicate_effect_without_the_rules(owner, instance));
        }
        self.duplicate_effect_without_the_rules(owner, instance)
    }

    fn duplicate_effect_without_the_rules(
        &mut self,
        owner: EffectOwner,
        instance: EffectInstanceId,
    ) -> Result<EffectInstanceId, EditError> {
        if let Some(clip) = self.the_clip_whose_content_is(owner, instance) {
            return Err(EditError::TheContentOfAGenerator { clip });
        }
        let has_the_track_badge = matches!(owner, Member::Clip(_));
        let chain = self
            .chain_of_the_member_mut(owner)
            .ok_or_else(|| nothing_has_the_chain(owner))?;
        let Some(original) = chain.instance(instance).cloned() else {
            return Err(EditError::NotInTheChain {
                owner,
                item: ChainItemKey::Effect(instance),
            });
        };
        if chain.is_full() {
            return Err(EditError::TooManyEffects {
                limit: MOST_EFFECTS_ON_ONE_THING,
            });
        }
        let keys = chain.keys(has_the_track_badge);
        let after_it = keys
            .iter()
            .position(|key| *key == ChainItemKey::Effect(instance))
            .and_then(|place| keys.get(place + 1))
            .copied();
        let copy = chain.put_on(original);
        if after_it.is_some() {
            chain.move_before(ChainItemKey::Effect(copy), after_it, has_the_track_badge);
        }
        Ok(copy)
    }

    /// Moves `item` of the chain of `owner` to right before `before`, or to the end of what is shown of the chain if that is `None`
    /// (what is moved is an effect or a badge; the clip group or track group that the owner is in is always last, and is not in the chain).
    /// The badge of the track of a clip is shown only when its track has effects that apply to the clip (see [`Self::chain_rows_of`]): when it is
    /// not, and it has not been put anywhere, it is not in the way of what is moved to the end, which is put before it, where it stays at the
    /// end. It is an error, and nothing is changed, if there is no such thing as `owner`, or `item` or `before` is not in its chain.
    pub fn move_chain_item(
        &mut self,
        owner: EffectOwner,
        item: ChainItemKey,
        before: Option<ChainItemKey>,
    ) -> Result<(), EditError> {
        // (The content stays first: it is not moved, and nothing is moved in front of it.)
        for key in [Some(item), before].into_iter().flatten() {
            if let ChainItemKey::Effect(instance) = key
                && let Some(clip) = self.the_clip_whose_content_is(owner, instance)
            {
                return Err(EditError::TheContentOfAGenerator { clip });
            }
        }
        let is_a_clip = matches!(owner, Member::Clip(_));
        let badge_of_the_track_is_hidden_at_the_end = is_a_clip
            && self
                .chain_of_the_member(owner)
                .is_some_and(|chain| !chain.keeps_the_badge_of_the_track())
            && !self
                .chain_rows_of(owner)
                .iter()
                .any(|row| matches!(row, ChainRow::TrackBadge { .. }));
        let before = match before {
            None if badge_of_the_track_is_hidden_at_the_end => Some(ChainItemKey::Badge(Badge::Track)),
            other => other,
        };
        let chain = self
            .chain_of_the_member_mut(owner)
            .ok_or_else(|| nothing_has_the_chain(owner))?;
        if chain.move_before(item, before, is_a_clip) {
            Ok(())
        } else {
            Err(EditError::NotInTheChain { owner, item })
        }
    }
}

#[cfg(test)]
mod tests;
