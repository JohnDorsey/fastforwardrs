// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading a value of a shader's effect from a mask (`docs/shader-masks-design.md`): which edits are allowed, and the rule that **a picture cannot be made from itself**.
//!
//! A mask that is a track is the picture that the track's clips make with their own effects. If one of those effects reads a mask too, the picture it reads is made first; so tracks
//! that read each other, in a ring of any size, have no picture to start from. The project refuses an edit that would make such a ring, and (because clips move between tracks, and
//! pasted effects arrive on others) the picture plan does not trust that: it makes nothing of a track that it is already making (see `composition::pictures`).

use std::collections::HashSet;

use super::groups::EffectOwner;
use super::tracks::TrackId;
use super::{DrivenValue, EditError, EffectInstanceId, InconsistentProject, Member, Project};

impl Project {
    /// Reads the value in place `slot` of the effect named `instance` in the chain of `owner` from a mask, as `driven` says, or from its slider again (`None`). Its slider and its
    /// keyframes are kept. It is an error, and nothing is changed, if there is no such effect, if the value is not a number with a range of a shader, if the track the mask is made
    /// of is not there, or if the track is made from the picture the effect is done to (see the top of this file).
    pub fn set_driven_value(
        &mut self,
        owner: EffectOwner,
        instance: EffectInstanceId,
        slot: usize,
        driven: Option<DrivenValue>,
    ) -> Result<(), EditError> {
        let chain = self.chain_of_the_member(owner).ok_or(EditError::NotInTheChain {
            owner,
            item: super::ChainItemKey::Effect(instance),
        })?;
        let effect = chain
            .iter()
            .find(|effect| effect.instance() == instance)
            .ok_or(EditError::NotInTheChain {
                owner,
                item: super::ChainItemKey::Effect(instance),
            })?;
        if let Some(driven) = driven {
            if let Some(reason) = effect.why_a_value_cannot_be_driven(slot) {
                return Err(EditError::ValueCannotBeDriven { reason });
            }
            for source in driven.source.tracks() {
                if self.track(source).is_none() {
                    return Err(EditError::UnknownTrack { track: source });
                }
                if self.a_mask_of_this_track_would_read_a_picture_made_from_it(owner, source) {
                    return Err(EditError::MaskWouldReadItself { track: source });
                }
            }
        }
        let chain = self
            .chain_of_the_member_mut(owner)
            .expect("the chain was there a moment ago");
        if chain.set_driven_value(instance, slot, driven) {
            Ok(())
        } else {
            // (Nothing of the effect is wrong that was not looked at above: the number was not one.)
            Err(EditError::ValueCannotBeDriven {
                reason: "the number for where the mask has nothing is not a number",
            })
        }
    }

    /// Everything in the project that has a chain of effects: every track, every clip, every group.
    fn everything_that_has_effects(&self) -> Vec<Member> {
        let mut members: Vec<Member> = self.tracks().iter().map(|track| Member::Track(track.id())).collect();
        for track in self.tracks() {
            members.extend(track.placements().map(|placement| Member::Clip(placement.id)));
        }
        members.extend(self.groups().iter().map(|group| Member::Group(group.id())));
        members
    }

    /// Takes every value of every effect that is read from the track `track` off its mask: it reads its slider again. (What is done when the track is deleted: nothing is left that reads
    /// a track that is not there.)
    pub(super) fn let_go_of_the_masks_made_of(&mut self, track: TrackId) {
        for member in self.everything_that_has_effects() {
            if let Some(chain) = self.chain_of_the_member_mut(member) {
                chain.let_go_of_the_track(track);
            }
        }
    }

    /// The first thing wrong with the masks of the project, if anything is: a value read from a track that the project does not have, or tracks that are made from each other (the
    /// project never makes either by an edit; a file written by hand could say either).
    pub(super) fn first_problem_with_the_masks(&self) -> Option<InconsistentProject> {
        let known: HashSet<TrackId> = self.tracks().iter().map(|track| track.id()).collect();
        for member in self.everything_that_has_effects() {
            for effect in self
                .chain_of_the_member(member)
                .into_iter()
                .flat_map(|chain| chain.iter())
            {
                if let Some(track) = effect
                    .driven_values()
                    .tracks()
                    .into_iter()
                    .find(|track| !known.contains(track))
                {
                    return Some(InconsistentProject::ValueReadFromAnUnknownTrack { track });
                }
            }
        }
        self.tracks()
            .iter()
            .map(|track| track.id())
            .find(|track| self.a_track_is_made_from_its_own_picture(*track))
            .map(|track| InconsistentProject::TracksAreMadeFromEachOther { track })
    }

    /// Whether the picture of `track` is made from itself, through the masks of the effects done to it (or of those done to the tracks it reads, and so on).
    fn a_track_is_made_from_its_own_picture(&self, track: TrackId) -> bool {
        let mut seen: HashSet<TrackId> = HashSet::new();
        let mut to_look_at = self.tracks_read_by_the_picture_of(track);
        while let Some(found) = to_look_at.pop() {
            if found == track {
                return true;
            }
            if seen.insert(found) {
                to_look_at.extend(self.tracks_read_by_the_picture_of(found));
            }
        }
        false
    }

    /// The tracks whose pictures the effects of `owner` are done to: a clip's effects, to the picture of the clip, which is on its track; a track's, to the picture of each of its clips;
    /// a group's, to the clips or the tracks that it is made of (**more than that, for a group that makes a picture of its own**: its effects are done to the sum of its layers, which no
    /// mask is made from, so this is more tracks than the rule needs; a ring that goes through such a group is refused too).
    fn tracks_whose_pictures_the_effects_of_are_done_to(&self, owner: Member) -> Vec<TrackId> {
        match owner {
            Member::Clip(clip) => self.track_holding(clip).map(|track| track.id()).into_iter().collect(),
            Member::Track(track) => vec![track],
            Member::Group(group) => {
                let mut tracks = self.tracks_in(group);
                tracks.extend(
                    self.clips_in(group)
                        .into_iter()
                        .filter_map(|clip| self.track_holding(clip).map(|track| track.id())),
                );
                tracks
            }
        }
    }

    /// The tracks that the picture of `track` is made from by way of the masks of its effects: the sources of every value that is read from a mask in a chain that is done to the
    /// clips of the track (the chains of the clips, of the track, and of the groups that the clips are in, whether or not the effect is switched on, so that switching it on makes no ring).
    fn tracks_read_by_the_picture_of(&self, track: TrackId) -> Vec<TrackId> {
        let Some(found) = self.track(track) else {
            return Vec::new();
        };
        let clips: Vec<_> = found.placements().map(|placement| placement.id).collect();
        let mut members: Vec<Member> = clips.iter().map(|clip| Member::Clip(*clip)).collect();
        members.push(Member::Track(track));
        members.extend(
            self.groups()
                .iter()
                .filter(|group| {
                    self.clips_in(group.id()).iter().any(|clip| clips.contains(clip))
                        || self.tracks_in(group.id()).contains(&track)
                })
                .map(|group| Member::Group(group.id())),
        );
        let mut tracks: Vec<TrackId> = Vec::new();
        for member in members {
            for effect in self
                .chain_of_the_member(member)
                .into_iter()
                .flat_map(|chain| chain.iter())
            {
                for source in effect.driven_values().tracks() {
                    if !tracks.contains(&source) {
                        tracks.push(source);
                    }
                }
            }
        }
        tracks
    }

    /// Whether a mask made of `source`, read by an effect of `owner`, would be made from the picture that the effect is done to: `source` is, or is made from by way of the masks of
    /// its own effects (as far as they go), a track that the effect is done to.
    fn a_mask_of_this_track_would_read_a_picture_made_from_it(&self, owner: Member, source: TrackId) -> bool {
        let hosts = self.tracks_whose_pictures_the_effects_of_are_done_to(owner);
        let mut seen: HashSet<TrackId> = HashSet::new();
        let mut to_look_at = vec![source];
        while let Some(track) = to_look_at.pop() {
            if hosts.contains(&track) {
                return true;
            }
            if seen.insert(track) {
                to_look_at.extend(self.tracks_read_by_the_picture_of(track));
            }
        }
        false
    }
}
