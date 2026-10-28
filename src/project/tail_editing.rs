// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Saying how much of a track one instance of its effects covers, and what the tails of clips and groups do at the edge of a group (phase 12, piece 12.1; see `tails.rs` and
//! docs/engine-phase-12-design.md). Nothing is heard or seen differently for it yet: the plans read these values in the pieces after this one.

use super::{
    ClipEditError, ClipId, ClipTailPolicy, EditError, EffectDomain, EffectInstanceId, GroupId, GroupKind,
    GroupTailPolicy, Member, Project, TailLength, TrackId, TrackSharing,
};

impl Project {
    /// Says how much of the track `track` one instance of its effects of `domain` (its picture effects or its sound effects) covers. It is an error, and nothing is changed, if the
    /// track is not there or if that would break a rule of groups (a track that is made indivisible cannot have a group with effects that takes only some of its clips).
    pub fn set_track_sharing(
        &mut self,
        track: TrackId,
        domain: EffectDomain,
        sharing: TrackSharing,
    ) -> Result<(), EditError> {
        self.keeping_the_groups_valid(|project| {
            let track = project.track_mut_or_error(track)?;
            let changed = track.sharing().with(domain, sharing);
            track.set_sharing(changed);
            Ok(())
        })
    }

    /// Says what the tail of the clip `clip` does at the edge of a group that the clip is not in.
    pub fn set_clip_tail_policy(&mut self, clip: ClipId, policy: ClipTailPolicy) -> Result<(), EditError> {
        Ok(self
            .track_holding_mut_or_error(clip)?
            .set_clip_tail_policy(clip, policy)?)
    }

    /// What the tail of the clip `clip` does at the edge of a group that the clip is not in. `None` if there is no such clip.
    pub fn clip_tail_policy(&self, clip: ClipId) -> Option<ClipTailPolicy> {
        self.track_holding(clip)?
            .placement_of(clip)
            .map(|placement| placement.tail_policy)
    }

    /// Says what the tails of the members of the clip group or track group `group` do at its edge. A tag has no edge, and it is an error to give it a policy.
    pub fn set_group_tail_policy(&mut self, group: GroupId, policy: GroupTailPolicy) -> Result<(), EditError> {
        let found = self.group_mut(group).ok_or(EditError::UnknownGroup { group })?;
        if found.kind() == GroupKind::Tag {
            return Err(EditError::ATagHasNoEdge { group });
        }
        found.set_tail_policy(policy);
        Ok(())
    }
}

impl Project {
    /// Says how long the effect that is named `instance` in the chain of `owner` (a clip, a track or a group) goes on after its input ends: [`TailLength::Automatic`] is what its plugin says, or the
    /// default for its kind. It is an error, and nothing is changed, if there is no such thing as `owner` or its chain has no such effect.
    pub fn set_effect_tail_length(
        &mut self,
        owner: Member,
        instance: EffectInstanceId,
        tail_length: TailLength,
    ) -> Result<(), EditError> {
        let chain = self.chain_of_the_member_mut(owner).ok_or(match owner {
            Member::Clip(clip) => EditError::Clip(ClipEditError::UnknownClip { clip }),
            Member::Track(track) => EditError::UnknownTrack { track },
            Member::Group(group) => EditError::UnknownGroup { group },
        })?;
        if chain.set_tail_length(instance, tail_length) {
            Ok(())
        } else {
            Err(EditError::NotInTheChain {
                owner,
                item: super::ChainItemKey::Effect(instance),
            })
        }
    }
}

#[cfg(test)]
mod tests;
