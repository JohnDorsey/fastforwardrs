// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Changing a track: cutting, changing speeds and volumes, deleting, adding and changing text.

use std::collections::BTreeMap;

use thiserror::Error;

use super::{Room, Track, TrackEntry};
use crate::project::tracks::clip::Clip;
use crate::project::tracks::drawer::Drawer;
use crate::project::tracks::gain::Gain;
use crate::project::tracks::ids::{ClipId, RackId};
use crate::project::tracks::placement::Placement;
use crate::project::tracks::text_object::{TextClip, TextObject};
use crate::project::tracks::track_height::TrackHeight;
use crate::project::{AnimationTarget, EffectKind, Effects, PropertyId, PropertyValue};
use crate::project::{SpeedFactor, TimelineFrameIndex};

/// What a cut puts at the edges of the two parts, in each of the racks of the cut clip's own keyframes: the value that the
/// property has at the last frame of the first part, if its keyframes go on beyond the cut, and at the first frame of the
/// second part, if they come from before it. They are what the property is there by *all* the keyframes it has, wherever
/// they are kept, which is the project's to work out (see [`Track::racks_cut_by`]).
pub type CutEdges = BTreeMap<RackId, (Option<PropertyValue>, Option<PropertyValue>)>;

/// One of the racks of a clip's own keyframes that a cut, or making the clip shorter, divides, and what it needs at the edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RackCut {
    pub rack: RackId,
    pub property: PropertyId,
    /// Whether the rack has keyframes at or after the cut, so that the property goes on beyond it (and the first part needs a
    /// keyframe at its last frame to have what the whole had there).
    pub keyframes_go_on_beyond: bool,
    /// Whether the rack has keyframes before the cut, so that the property comes from before it (and the second part needs a
    /// keyframe at its first frame to have what the whole had there).
    pub keyframes_come_from_before: bool,
}

/// The result of cutting a clip in two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitOutcome {
    /// The first half, which is the clip that was cut: it keeps the clip's name.
    pub first_half: ClipId,
    pub second_half: ClipId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum SplitError {
    #[error("there is no clip at {timeline_frame} to cut")]
    NoClipThere { timeline_frame: TimelineFrameIndex },
    #[error("the clip at {timeline_frame} already starts there")]
    AlreadyAtClipBoundary { timeline_frame: TimelineFrameIndex },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ClipEditError {
    #[error("the track has no {clip}")]
    UnknownClip { clip: ClipId },
    #[error("{clip} is not a video or a sound, so this does not apply to it")]
    NotAMediaClip { clip: ClipId },
    #[error("{clip} is not a text object, so this does not apply to it")]
    NotATextClip { clip: ClipId },
    #[error("{clip} is a sound, which is not on the picture, so this does not apply to it")]
    NotOnThePicture { clip: ClipId },
    #[error("{clip} lasts as long as the part of its video or sound it is made from, so this does not apply to it")]
    LastsAsLongAsItsFile { clip: ClipId },
    #[error(
        "{clip} is a keyframe crate, which lasts from its first keyframe to its last, so this does not apply to it"
    )]
    LastsAsLongAsItsKeyframes { clip: ClipId },
    #[error("{clip} is not a keyframe crate, so this does not apply to it")]
    NotAKeyframeCrate { clip: ClipId },
    #[error("{clip} would be on top of another clip there")]
    WouldOverlap { clip: ClipId },
    #[error("{property:?} is not a property of {clip}, or the value is not the kind it has")]
    NotAPropertyOfThisClip { clip: ClipId, property: PropertyId },
    #[error("{clip} is not as long as {frames_in} frames, so it has nothing at that frame")]
    NotWithinTheClip { clip: ClipId, frames_in: u64 },
    #[error("{clip} cannot have the {kind} effect")]
    NotAnEffectOfThisClip { clip: ClipId, kind: EffectKind },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum InsertError {
    #[error("{timeline_frame} is taken by another clip")]
    Occupied { timeline_frame: TimelineFrameIndex },
    #[error("the clip would run into the next one: there is room for {frames_of_room} frames, and it needs {needed}")]
    NotEnoughRoom { frames_of_room: u64, needed: u64 },
}

impl Track {
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    pub fn set_gain(&mut self, gain: Gain) {
        self.gain = gain;
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    pub fn set_height(&mut self, height: TrackHeight) {
        self.height = height;
    }

    /// Makes everything on the track play `speed` times faster (on top of the clips' own speeds).
    /// The clips that come from files get shorter or longer, and the ones touching them follow.
    pub fn set_speed(&mut self, speed: SpeedFactor) {
        if speed == self.speed {
            return;
        }
        let anchors = self.anchors();
        self.speed = speed;
        self.reposition(&anchors);
    }

    /// Cuts the clip at `timeline_frame` in two, so that the clip's frame at that position becomes
    /// the first frame of the second half. Both halves keep everything else about the clip, and
    /// together last as long as the clip did, so that nothing else moves. What is in the clip's drawer is cut with it: see
    /// [`Self::split_at_with_edges`]. (The racks of the clip's own keyframes are given what their own keyframes say at the
    /// edges: when others of its keyframes are kept elsewhere, the project works out what they all say.)
    pub fn split_at(&mut self, timeline_frame: TimelineFrameIndex) -> Result<SplitOutcome, SplitError> {
        let edges = self.edges_by_the_own_keyframes_of_the_clip_at(timeline_frame);
        self.split_at_with_edges(timeline_frame, &edges)
    }

    /// The values that a cut at `timeline_frame` needs at the edges of the racks of the clip's own keyframes, by what those
    /// keyframes say by themselves.
    fn edges_by_the_own_keyframes_of_the_clip_at(&self, timeline_frame: TimelineFrameIndex) -> CutEdges {
        let Some(placement) = self.placement_covering(timeline_frame) else {
            return CutEdges::new();
        };
        let frames_in = timeline_frame.value() - placement.first_timeline_frame.value();
        self.racks_cut_by(placement.id, frames_in)
            .into_iter()
            .filter_map(|cut| {
                let curve = placement.drawer.rack(cut.rack)?.curve();
                let before = cut
                    .keyframes_go_on_beyond
                    .then(|| frames_in.checked_sub(1).and_then(|last| curve.value_at(last)))
                    .flatten();
                let after = cut
                    .keyframes_come_from_before
                    .then(|| curve.value_at(frames_in))
                    .flatten();
                Some((cut.rack, (before, after)))
            })
            .collect()
    }

    /// The racks of the keyframes of `clip` itself that are in its drawer and that a cut `frames_in` frames after its first
    /// frame divides, with what each needs at the edges (see [`CutEdges`]).
    pub fn racks_cut_by(&self, clip: ClipId, frames_in: u64) -> Vec<RackCut> {
        let Some(entry) = self.index_of(clip).map(|index| &self.entries[index]) else {
            return Vec::new();
        };
        entry
            .drawer
            .racks()
            .iter()
            .filter(|rack| rack.target() == AnimationTarget::Clip(clip))
            .map(|rack| RackCut {
                rack: rack.id(),
                property: rack.property(),
                keyframes_go_on_beyond: rack.keyframes().iter().any(|keyframe| keyframe.frame >= frames_in),
                keyframes_come_from_before: rack.keyframes().iter().any(|keyframe| keyframe.frame < frames_in),
            })
            .collect()
    }

    /// Cuts the clip at `timeline_frame` in two, as [`Self::split_at`] does, with `edges` for the racks of the clip's own
    /// keyframes. What is in the clip's drawer is cut with it: each rack gives the keyframes before the cut to the first half
    /// and the others to the second half (counted from the cut), which gets a rack of its own. The racks of the clip's own
    /// keyframes become racks of the keyframes of the first half and of the second, and are given the values at the edges that
    /// `edges` has for them, so that each half has what the clip had at every frame it has; racks of the keyframes of other
    /// clips are cut without any, because the keyframes they have together are the same as before. A rack that has nothing
    /// left in a half is not in it.
    pub fn split_at_with_edges(
        &mut self,
        timeline_frame: TimelineFrameIndex,
        edges: &CutEdges,
    ) -> Result<SplitOutcome, SplitError> {
        let placement = self
            .placement_covering(timeline_frame)
            .ok_or(SplitError::NoClipThere { timeline_frame })?;
        let index = placement.index;
        let frames_in = timeline_frame.value() - placement.first_timeline_frame.value();
        let (first_half, second_half) = placement
            .clip
            .split_after(frames_in, self.speed)
            .ok_or(SplitError::AlreadyAtClipBoundary { timeline_frame })?;

        let second_id = self.take_next_clip_id();
        let first_id = self.entries[index].id;
        // (Both halves have the effects the clip had.)
        let effects = self.entries[index].effects.clone();
        let drawer_before = std::mem::take(&mut self.entries[index].drawer);
        let mut first_drawer = Drawer::default();
        let mut second_drawer = Drawer::default();
        for rack in drawer_before.racks() {
            let is_of_the_clip_itself = rack.target() == AnimationTarget::Clip(first_id);
            let (value_before, value_after) = if is_of_the_clip_itself {
                edges.get(&rack.id()).cloned().unwrap_or((None, None))
            } else {
                (None, None)
            };
            let target_of_the_second = if is_of_the_clip_itself {
                AnimationTarget::Clip(second_id)
            } else {
                rack.target()
            };
            let (first_rack, second_rack) = rack.split_at(
                frames_in,
                value_before,
                value_after,
                (self.take_next_rack_id(), target_of_the_second),
            );
            if !first_rack.is_empty() {
                first_drawer.push(first_rack);
            }
            if !second_rack.is_empty() {
                second_drawer.push(second_rack);
            }
        }
        self.entries[index].clip = first_half;
        self.entries[index].drawer = first_drawer;
        self.entries.insert(
            index + 1,
            TrackEntry {
                id: second_id,
                clip: second_half,
                drawer: second_drawer,
                effects,
                gap_before_frames: 0,
            },
        );
        self.rebuild_derived_values();
        Ok(SplitOutcome {
            first_half: first_id,
            second_half: second_id,
        })
    }

    /// Plays the clip `clip` at `speed`. The clips touching it move along to close up or make room.
    pub fn set_clip_speed(&mut self, clip: ClipId, speed: SpeedFactor) -> Result<(), ClipEditError> {
        self.change_media_clip(clip, |media| media.with_speed(speed))
    }

    /// Makes the clip `clip` louder or quieter than it was.
    pub fn set_clip_gain(&mut self, clip: ClipId, gain: Gain) -> Result<(), ClipEditError> {
        self.change_media_clip(clip, |media| media.with_gain(gain))
    }

    /// Mutes the clip `clip`, or lets it be heard again.
    pub fn set_clip_muted(&mut self, clip: ClipId, muted: bool) -> Result<(), ClipEditError> {
        self.change_media_clip(clip, |media| media.with_muted(muted))
    }

    fn change_media_clip(
        &mut self,
        clip: ClipId,
        change: impl FnOnce(&crate::project::tracks::media_clip::MediaClip) -> crate::project::tracks::media_clip::MediaClip,
    ) -> Result<(), ClipEditError> {
        let index = self.index_of(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        let anchors = self.anchors();
        let changed = match &self.entries[index].clip {
            Clip::Video(media) => Clip::Video(change(media)),
            Clip::Audio(media) => Clip::Audio(change(media)),
            Clip::Image(_) | Clip::Text(_) | Clip::Crate(_) => return Err(ClipEditError::NotAMediaClip { clip }),
        };
        self.entries[index].clip = changed;
        self.reposition(&anchors);
        Ok(())
    }

    /// Takes the clip `clip` off the track. If it was a video or a sound, the clips touching its
    /// end move up to where it began, so that no hole is left; if it was text, nothing moves.
    pub fn delete_clip(&mut self, clip: ClipId) -> Result<(), ClipEditError> {
        let index = self.index_of(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        let mut anchors = self.anchors();
        let removed_anchor = anchors.remove(index);
        let removed = self.entries.remove(index);
        if let Some(next_anchor) = anchors.get_mut(index) {
            if removed.clip.is_media() && next_anchor.moves_along {
                // It was touching the clip that has gone, so it closes up to where that began.
                *next_anchor = super::layout::Anchor {
                    start: removed_anchor.start,
                    moves_along: false,
                    length: next_anchor.length,
                };
            } else if !removed.clip.is_media() {
                // A text object that goes leaves what came after it where it was.
                next_anchor.moves_along = false;
            }
        }
        self.reposition(&anchors);
        Ok(())
    }

    /// Changes what the text clip `clip` says and how it looks. (Its length is not touched: see
    /// [`Self::set_clip_duration`].)
    pub fn set_text_object(&mut self, clip: ClipId, object: TextObject) -> Result<(), ClipEditError> {
        let index = self.index_of(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        let Clip::Text(text) = &mut self.entries[index].clip else {
            return Err(ClipEditError::NotATextClip { clip });
        };
        text.object = object;
        Ok(())
    }

    /// Puts the clip `clip` (a video or a text object) at `placement` on the picture. Where it is on
    /// the picture has nothing to do with when it is shown, so nothing on the track moves.
    pub fn set_clip_placement(&mut self, clip: ClipId, placement: Placement) -> Result<(), ClipEditError> {
        let index = self.index_of(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        let placed = self.entries[index]
            .clip
            .with_placement(placement)
            .ok_or(ClipEditError::NotOnThePicture { clip })?;
        self.entries[index].clip = placed;
        Ok(())
    }

    /// How many frames the clip `clip`, which is a text object or a picture, would last if it were asked to last
    /// `duration_in_frames`: that, or as long as there is room for before the next clip if that is less (and at least one
    /// frame).
    pub fn duration_a_clip_would_be_given(&self, clip: ClipId, duration_in_frames: u64) -> Result<u64, ClipEditError> {
        let index = self.index_of(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        if matches!(self.entries[index].clip, Clip::Crate(_)) {
            return Err(ClipEditError::LastsAsLongAsItsKeyframes { clip });
        }
        let start = self.first_timeline_frames[index];
        let longest = self
            .first_timeline_frames
            .get(index + 1)
            .map_or(u64::MAX, |next_start| next_start - start);
        self.entries[index]
            .clip
            .duration_in_frames_it_says()
            .ok_or(ClipEditError::LastsAsLongAsItsFile { clip })?;
        Ok(duration_in_frames.clamp(1, longest))
    }

    /// Makes the clip `clip`, which is a text object or a picture, last `duration_in_frames`
    /// frames, or as long as there is room for before the next clip if that is less (and at least
    /// one frame). Nothing else moves. Returns how many frames it lasts now.
    ///
    /// What is in its drawer is shortened with it. The keyframes that it has no frame for any more come in to its last frames,
    /// where there is room for them (the empty frames between the keyframes are what is given up), and are lost only if there
    /// is not: see [`crate::project::Rack::truncated_to`]. (A clip made longer keeps the keyframes it had, which are all before
    /// its old end.)
    pub fn set_clip_duration(&mut self, clip: ClipId, duration_in_frames: u64) -> Result<u64, ClipEditError> {
        let new_duration = self.duration_a_clip_would_be_given(clip, duration_in_frames)?;
        let index = self.index_of(clip).ok_or(ClipEditError::UnknownClip { clip })?;
        let start = self.first_timeline_frames[index];
        let old_duration = self.entries[index]
            .clip
            .duration_in_frames_it_says()
            .ok_or(ClipEditError::LastsAsLongAsItsFile { clip })?;
        if let Some(changed) = self.entries[index].clip.with_duration_in_frames_it_says(new_duration) {
            let old_end = start + old_duration;
            self.entries[index].clip = changed;
            self.entries[index].drawer = self.entries[index]
                .drawer
                .with_each_rack_changed(|rack| rack.truncated_to(new_duration));
            if let Some(next) = self.entries.get_mut(index + 1) {
                // The next clip stays where it is, so the gap before it takes up the difference.
                next.gap_before_frames = next.gap_before_frames + old_end - (start + new_duration);
            }
            self.rebuild_derived_values();
        }
        Ok(new_duration)
    }

    /// How many frames long a new text clip can be if it starts at `timeline_frame`, when it is
    /// wanted to last `wanted_frames`: that, or less if it would run into the next clip. `None` if
    /// another clip is at that frame.
    pub fn longest_new_text_clip_at(&self, timeline_frame: TimelineFrameIndex, wanted_frames: u64) -> Option<u64> {
        match self.room_at(timeline_frame) {
            Room::Occupied(_) => None,
            Room::Free { until_frame, .. } => Some(
                until_frame
                    .map_or(wanted_frames, |until| {
                        (until - timeline_frame.value()).min(wanted_frames)
                    })
                    .max(1),
            ),
        }
    }

    /// Whether a clip that lasts `needed` frames could be put on the track starting at `timeline_frame`, which it can if
    /// there is nothing at any of the frames it would occupy. If it can, gives the first frame of the empty stretch of
    /// timeline that it would be put in (the end of the clip before it, or 0); if it cannot, says why not.
    pub fn start_of_the_free_stretch_that_has_room_for(
        &self,
        timeline_frame: TimelineFrameIndex,
        needed: u64,
    ) -> Result<u64, InsertError> {
        let Room::Free {
            from_frame,
            until_frame,
        } = self.room_at(timeline_frame)
        else {
            return Err(InsertError::Occupied { timeline_frame });
        };
        if let Some(until_frame) = until_frame {
            let frames_of_room = until_frame - timeline_frame.value();
            if needed > frames_of_room {
                return Err(InsertError::NotEnoughRoom { frames_of_room, needed });
            }
        }
        Ok(from_frame)
    }

    /// Puts `clip` on the track so that it starts at `timeline_frame`. There must be nothing at
    /// any of the frames it occupies. Nothing else moves.
    pub fn insert_clip_at(&mut self, timeline_frame: TimelineFrameIndex, clip: Clip) -> Result<ClipId, InsertError> {
        let needed = clip.timeline_frame_count(self.speed);
        let from_frame = self.start_of_the_free_stretch_that_has_room_for(timeline_frame, needed)?;

        let index = self
            .first_timeline_frames
            .partition_point(|&first| first <= timeline_frame.value());
        let id = self.take_next_clip_id();
        let new_start = timeline_frame.value();
        let new_end = new_start + needed;
        if let Some(next) = self.entries.get_mut(index) {
            // The next clip stays where it is, so the gap before it takes up the difference.
            next.gap_before_frames -= new_end - from_frame;
        }
        self.entries.insert(
            index,
            TrackEntry {
                id,
                clip,
                drawer: Drawer::default(),
                effects: Effects::none(),
                gap_before_frames: new_start - from_frame,
            },
        );
        self.rebuild_derived_values();
        Ok(id)
    }

    /// Adds a text clip at `timeline_frame` that lasts `wanted_frames` frames, or less if it would
    /// run into the next clip. Returns the clip's name and how many frames it lasts, or `None` if
    /// another clip is at that frame.
    pub fn insert_text_at(
        &mut self,
        timeline_frame: TimelineFrameIndex,
        object: TextObject,
        wanted_frames: u64,
    ) -> Option<(ClipId, u64)> {
        let duration = self.longest_new_text_clip_at(timeline_frame, wanted_frames)?;
        let clip = Clip::Text(TextClip::new(object, duration).ok()?);
        let id = self.insert_clip_at(timeline_frame, clip).ok()?;
        Some((id, duration))
    }
}
