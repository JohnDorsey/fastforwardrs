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
//! # A placeholder: group effects are applied to each member for now
//!
//! **Neither the preview nor the export can make the picture that the members of a group make together yet.** Each draws every layer
//! by itself, one over another, with the colour change of the layer applied to the layer's own picture (see
//! [`crate::composition::Layer::effects`]). A new rendering engine that can draw some layers into a picture of their own, change that
//! picture, and then draw it over the rest, is planned (`docs/long-term-plan-plugins-and-gpu-engine.md`), and this is for it to do.
//!
//! Until it can, the effects of a clip group or a track group are **applied to the own picture of each member** instead: they are added
//! to the effects of the layer of each clip that is inside the group ([`Project::effects_applied_to_the_clip`]). That is the maintainer's
//! decision of 2026-10-13 (recorded in `docs/groups-feature-spec.md`, G-26), and it is not what the effect is meant to do, so:
//!
//! * for an effect that changes the colour of each pixel by itself and does not change how clear it is (every effect there is: the
//!   hue shift is a colour matrix) the picture is the same, **except where a colour is cut off at the least or the most there is**.
//!   Where a layer that is partly clear is over another, and the effect pushes a colour of either past that limit, the colour that is cut
//!   off is not the colour of what the layers make together, and the picture is off by some of the 255 steps of a colour. **How many
//!   depends on how vivid the colours are and how clear the layer in front is; it is not limited to a few steps.** For two layers 40%
//!   and 50% clear (a blue one over an orange one) and a turn of 120 degrees it is 14 steps in one colour (measured with ffmpeg, which
//!   the export uses, and the same by arithmetic), 20 at the worst turn for those two colours, and for colours chosen at random among
//!   all there are, with random clearness and turns, a search found differences of up to about 100 steps, more than 8 in about a
//!   quarter of the pairs. Where the layers are **not clear at all** (a video, a picture without see-through parts) there is **no
//!   difference**, because the layer in front is all that is seen; and where only one layer is, or the colours are greys or stay
//!   inside the limits, there is none either (but for a step of rounding). So what shows is the edges of what is see-through (the soft
//!   edge of text, of a turned picture, of a picture with a see-through part) where it is over something else. (The tests of this
//!   module that have `placeholder` in their names work these differences out, so that it is known what the placeholder gets wrong, and
//!   they are the ones to throw away with it);
//! * for an effect that would **not** make the same picture that way (a blur, which is a mix of the pixels beside each pixel, a glow, a
//!   vignette that is round the group and not round each member), applying it to each member is wrong, not a little off. Such an
//!   effect says so in the table of effects (`on_a_composite` is `false`), and a clip group or a track group does not offer it, until
//!   the engine can make the picture that it is applied to. A tag offers every effect: for a tag each member is what it is applied to.
//!
//! The order the effects are applied in is: the effects of the clip itself, with those of its tracks and of its tags where their badges
//! are in its chain; then those of the clip groups it is in, the nearest first (each with its own tags, where its badges are); then those
//! of the track groups its track is in, the nearest first.

use std::collections::HashMap;

use super::{Group, GroupId, Member};
use crate::project::tracks::{ClipId, ClipKind, Track};
use crate::project::{
    AnimationTarget, Badge, ChainItem, ChainItemKey, EditError, Effect, EffectInstanceId, EffectKind, Effects, Project,
    PropertyId, PropertyValue,
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
                            .filter(|effect| effect.kind().applies_to_a_clip_of_kind(kind))
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
        self.effects_applied_to_a_clip(&self.parents(), track, clip, placement.effects, placement.clip.kind())
    }

    /// What [`Self::effects_applied_to_the_clip`] gives for every clip there is, worked out together (which is quicker).
    pub fn effects_applied_to_every_clip(&self) -> HashMap<ClipId, Vec<AppliedEffect<'_>>> {
        let parents = self.parents();
        let mut applied = HashMap::new();
        for track in &self.tracks {
            for entry in track.entries() {
                applied.insert(
                    entry.id(),
                    self.effects_applied_to_a_clip(&parents, track, entry.id(), entry.effects(), entry.clip().kind()),
                );
            }
        }
        applied
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
        let shapes_round_the_clip = self.shapes_round(parents, Member::Clip(clip));
        let shapes_round_the_track = self.shapes_round(parents, Member::Track(track.id()));
        for shape in shapes_round_the_clip.into_iter().chain(shapes_round_the_track) {
            self.add_the_effects_of_the_group(shape, self.groups.len(), &mut applied);
        }
        applied.retain(|applied| applied.effect.kind().applies_to_a_clip_of_kind(kind));
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
        let found = self.group_mut(group).ok_or(EditError::UnknownGroup { group })?;
        if !kind.applies_to_a_group_of_kind(found.kind()) {
            return Err(EditError::NotAnEffectOfTheGroup { group, kind });
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
