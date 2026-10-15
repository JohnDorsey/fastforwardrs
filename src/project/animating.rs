// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Animating a property of a clip or of a track: what it is at a frame, whether and where it has keyframes, making one and
//! taking one away, and changing the property at a frame.
//!
//! Frames here are frames of the timeline. The keyframes of a property are in racks, in drawers, which may be anywhere:
//! what they are all together is [`super::racks`]'s. A property that has no keyframes is what the clip or track says it
//! is, as it always was; one that has keyframes is what they say at every frame, and changing it at a frame changes or
//! makes the keyframe there.

use std::fmt;

use super::animation::{PropertyId, PropertyValue, placement_value, text_value, with_placement_value, with_text_value};
use super::racks::NewKeyframe;
use super::tracks::{Clip, ClipEditError, ClipId, ClipPlacement, Gain, Placement, TextObject, TrackId};
use super::{EditError, Project, TimelineFrameIndex, fitted_placement};

/// What a keyframe, or a change of a property, is about: a clip, or a track.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnimationTarget {
    Clip(ClipId),
    Track(TrackId),
}

impl fmt::Display for AnimationTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clip(clip) => clip.fmt(formatter),
            Self::Track(track) => track.fmt(formatter),
        }
    }
}

impl Project {
    /// Where `clip` is on the picture: where it says, or if it does not say (a video or a picture that has not been put
    /// anywhere) where it is fitted inside the picture. `None` for a sound, which is not on the picture.
    pub fn placement_on_the_picture(&self, clip: &Clip) -> Option<Placement> {
        let picture_size = self.picture_size();
        match clip {
            Clip::Video(media) => {
                let shown_size = self
                    .source(media.source())
                    .and_then(|source| source.properties.video())
                    .map_or(picture_size, |video| {
                        (video.display_width_in_pixels, video.display_height_in_pixels)
                    });
                Some(
                    media
                        .placement()
                        .unwrap_or_else(|| fitted_placement(shown_size, picture_size)),
                )
            }
            Clip::Image(image) => {
                let shown_size = self
                    .source(image.source())
                    .and_then(|source| source.properties.display_size_in_pixels())
                    .unwrap_or(picture_size);
                Some(
                    image
                        .placement()
                        .unwrap_or_else(|| fitted_placement(shown_size, picture_size)),
                )
            }
            Clip::Text(text) => Some(text.object.placement()),
            Clip::Audio(_) | Clip::Crate(_) => None,
        }
    }

    /// Where the clip `clip` is on the picture at `timeline_frame`: where it says, changed as its keyframes say. (A frame before
    /// the clip is taken for its first, one after it for its last.) `None` for a sound, which is not on the picture, and for a
    /// clip that is not there.
    pub fn placement_at(&self, clip: ClipId, timeline_frame: TimelineFrameIndex) -> Option<Placement> {
        let placement = self.placement_of(clip)?;
        let says = self.placement_on_the_picture(placement.clip)?;
        Some(
            self.animation_of(AnimationTarget::Clip(clip))
                .placement_at(says, the_frame_the_clip_is_at(&placement, timeline_frame)),
        )
    }

    /// The text object of the text clip `clip` at `timeline_frame`: what it says, changed as its keyframes say. (A frame before
    /// the clip is taken for its first, one after it for its last.) `None` for what is not a text clip.
    pub fn text_object_at(&self, clip: ClipId, timeline_frame: TimelineFrameIndex) -> Option<TextObject> {
        let placement = self.placement_of(clip)?;
        let text = placement.clip.as_text()?;
        Some(
            self.animation_of(AnimationTarget::Clip(clip))
                .text_object_at(&text.object, the_frame_the_clip_is_at(&placement, timeline_frame)),
        )
    }

    /// The value that `property` of the clip `placement` is has when it is not animated. `None` if the clip does not have the
    /// property (a value of an effect that it does not have is not one of its properties).
    fn static_value_of_a_clip(&self, placement: &ClipPlacement<'_>, property: PropertyId) -> Option<PropertyValue> {
        let clip = placement.clip;
        if !property.applies_to_a_clip_of_kind(clip.kind()) {
            return None;
        }
        match property {
            PropertyId::HueShift => placement.effects.value_of(property),
            PropertyId::Gain => clip
                .as_media()
                .map(|media| PropertyValue::Number(media.gain().decibels())),
            PropertyId::Muted => clip.as_media().map(|media| PropertyValue::Flag(media.is_muted())),
            PropertyId::Position | PropertyId::Size | PropertyId::Rotation => {
                placement_value(&self.placement_on_the_picture(clip)?, property)
            }
            PropertyId::Text | PropertyId::FontFamily | PropertyId::FontSize | PropertyId::Colour => {
                text_value(&clip.as_text()?.object, property)
            }
        }
    }

    /// The value that `property` of the track `track` has when it is not animated.
    fn static_value_of_a_track(&self, track: TrackId, property: PropertyId) -> Option<PropertyValue> {
        let track = self.track(track)?;
        match property {
            PropertyId::Gain => Some(PropertyValue::Number(track.gain().decibels())),
            PropertyId::Muted => Some(PropertyValue::Flag(track.is_muted())),
            // (A value of an effect that the track does not have is not one of its properties.)
            _ => track.effects().value_of(property),
        }
    }

    /// Makes `property` of `clip` have `value` when it is not animated.
    fn set_static_value_of_a_clip(
        &mut self,
        clip_id: ClipId,
        property: PropertyId,
        value: &PropertyValue,
    ) -> Result<(), EditError> {
        let clip = self
            .placement_of(clip_id)
            .ok_or(ClipEditError::UnknownClip { clip: clip_id })?
            .clip
            .clone();
        let wrong = || ClipEditError::NotAPropertyOfThisClip {
            clip: clip_id,
            property,
        };
        if !property.applies_to_a_clip_of_kind(clip.kind()) || value.kind() != property.kind() {
            return Err(wrong().into());
        }
        match (property, value) {
            (PropertyId::HueShift, _) => Ok(self
                .track_holding_mut_or_error(clip_id)?
                .set_effect_value_of_the_clip(clip_id, property, value)?),
            (PropertyId::Gain, PropertyValue::Number(decibels)) => {
                self.set_clip_gain(clip_id, Gain::from_decibels(*decibels))
            }
            (PropertyId::Muted, PropertyValue::Flag(muted)) => self.set_clip_muted(clip_id, *muted),
            (PropertyId::Position | PropertyId::Size | PropertyId::Rotation, _) => {
                let current = self.placement_on_the_picture(&clip).ok_or_else(wrong)?;
                self.set_clip_placement(clip_id, with_placement_value(current, property, value))
            }
            (PropertyId::Text | PropertyId::FontFamily | PropertyId::FontSize | PropertyId::Colour, _) => {
                let text = clip.as_text().ok_or_else(wrong)?;
                self.set_text_object(clip_id, with_text_value(&text.object, property, value))
            }
            // (The value was found above to be of the kind the property has, so these are not given another kind. Every property
            // is named in this match, so that one that is added is not quietly given to the text object, which ignores it.)
            (PropertyId::Gain | PropertyId::Muted, _) => Err(wrong().into()),
        }
    }

    /// Makes `property` of `track` have `value` when it is not animated.
    fn set_static_value_of_a_track(
        &mut self,
        track: TrackId,
        property: PropertyId,
        value: &PropertyValue,
    ) -> Result<(), EditError> {
        match (property, value) {
            (PropertyId::Gain, PropertyValue::Number(decibels)) => {
                self.set_track_gain(track, Gain::from_decibels(*decibels))
            }
            (PropertyId::Muted, PropertyValue::Flag(muted)) => self.set_track_muted(track, *muted),
            (property, value) if property.effect().is_some() => {
                if self.track_mut_or_error(track)?.set_effect_value(property, value) {
                    Ok(())
                } else {
                    Err(EditError::NotAPropertyOfTheTrack { track, property })
                }
            }
            _ => Err(EditError::NotAPropertyOfTheTrack { track, property }),
        }
    }

    /// The clip `clip` as it is placed on the timeline, if it is there.
    fn the_placement_of_the_target(&self, clip: ClipId) -> Result<ClipPlacement<'_>, EditError> {
        Ok(self.placement_of(clip).ok_or(ClipEditError::UnknownClip { clip })?)
    }

    /// The value `property` of `target` has at `timeline_frame`: what its keyframes say if it has any, wherever they are
    /// kept, and otherwise what it is. For a clip, a frame before it or after it is taken for its first or its last. `None`
    /// if there is no such clip or track, or it does not have that property.
    pub fn property_value_at(
        &self,
        target: AnimationTarget,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
    ) -> Option<PropertyValue> {
        match target {
            AnimationTarget::Clip(clip) => {
                let placement = self.placement_of(clip)?;
                if !property.is_had_by_a_clip(placement.clip.kind(), placement.effects) {
                    return None;
                }
                self.keyframes_of(target, property)
                    .value_at(the_frame_the_clip_is_at(&placement, timeline_frame))
                    .or_else(|| self.static_value_of_a_clip(&placement, property))
            }
            AnimationTarget::Track(track) => {
                let track = self.track(track)?;
                if !property.is_had_by_a_track(track.effects()) {
                    return None;
                }
                self.keyframes_of(target, property)
                    .value_at(timeline_frame.value())
                    .or_else(|| self.static_value_of_a_track(track.id(), property))
            }
        }
    }

    /// Whether `property` of `target` has a keyframe exactly at `timeline_frame`, in whichever rack.
    pub fn has_keyframe_at(
        &self,
        target: AnimationTarget,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
    ) -> bool {
        self.locate_the_keyframe_at(target, property, timeline_frame.value())
            .is_some()
    }

    /// The frame of the last keyframe of `property` of `target` before `timeline_frame`, however far before.
    pub fn keyframe_before(
        &self,
        target: AnimationTarget,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
    ) -> Option<TimelineFrameIndex> {
        self.keyframes_of(target, property)
            .frame_before(timeline_frame.value())
            .map(TimelineFrameIndex::new)
    }

    /// The frame of the first keyframe of `property` of `target` after `timeline_frame`, however far after.
    pub fn keyframe_after(
        &self,
        target: AnimationTarget,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
    ) -> Option<TimelineFrameIndex> {
        self.keyframes_of(target, property)
            .frame_after(timeline_frame.value())
            .map(TimelineFrameIndex::new)
    }

    /// Puts a keyframe at `timeline_frame` for `property` of `target`, with the value it has there now, so that the
    /// property changes from there on when another keyframe has another value. The keyframe is put in a rack in the drawer
    /// of the clip or the track itself (a rack is made there if the drawer has none for the property), and the result says
    /// where. (If the property has a keyframe there already, wherever it is, nothing changes and there is no result.)
    pub fn add_keyframe(
        &mut self,
        target: AnimationTarget,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
    ) -> Result<Option<NewKeyframe>, EditError> {
        let value = self
            .property_value_at(target, property, timeline_frame)
            .ok_or_else(|| not_a_property_of(target, property))?;
        if self.has_keyframe_at(target, property, timeline_frame) {
            return Ok(None);
        }
        self.put_keyframe(target, property, timeline_frame, value)
    }

    /// Takes away the keyframe of `property` of `target` at `timeline_frame`, from whichever rack it is in, if there is one.
    /// If it was the last, the property is not animated any more and stays what that keyframe said.
    pub fn remove_keyframe(
        &mut self,
        target: AnimationTarget,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
    ) -> Result<(), EditError> {
        match target {
            AnimationTarget::Clip(clip) => {
                self.the_placement_of_the_target(clip)?;
            }
            AnimationTarget::Track(track) => {
                self.track_mut_or_error(track)?;
            }
        }
        let frame = timeline_frame.value();
        let Some(removed_value) = self.keyframes_of(target, property).at(frame).cloned() else {
            return Ok(());
        };
        // (Each rack that has a keyframe there loses it, not only the one that counts: a keyframe that is left under it would
        // show again.)
        let racks_that_have_one: Vec<_> = self
            .located_racks()
            .filter(|located| located.rack.target() == target && located.rack.property() == property)
            .filter_map(|located| {
                let frame_in_the_rack = frame.checked_sub(located.start)?;
                located
                    .rack
                    .at(frame_in_the_rack)
                    .map(|_| (located.owner, located.rack.id(), frame_in_the_rack))
            })
            .collect();
        for (owner, rack, frame_in_the_rack) in racks_that_have_one {
            if let Some(drawer) = self.drawer_mut(owner) {
                if let Some(rack) = drawer.rack_mut(rack) {
                    rack.remove_at(frame_in_the_rack);
                }
                drawer.remove_the_racks_with_no_keyframes();
            }
        }
        if self.is_animated(target, property) {
            Ok(())
        } else {
            self.set_static_value(target, property, &removed_value)
        }
    }

    /// Makes `property` of `target` have `value` at `timeline_frame`: if it is animated by putting a keyframe there (in
    /// place of the one there, if there is one, wherever it is kept), and if it is not by changing the property itself. (A turn
    /// is what it is given, however many turns that is: between two keyframes a clip goes round as often as they say.) The
    /// result says where a keyframe was made, if one was.
    pub fn set_property_at(
        &mut self,
        target: AnimationTarget,
        property: PropertyId,
        value: PropertyValue,
        timeline_frame: TimelineFrameIndex,
    ) -> Result<Option<NewKeyframe>, EditError> {
        if !self.is_animated(target, property) {
            self.set_static_value(target, property, &value)?;
            return Ok(None);
        }
        self.put_keyframe(target, property, timeline_frame, value)
    }

    pub(super) fn set_static_value(
        &mut self,
        target: AnimationTarget,
        property: PropertyId,
        value: &PropertyValue,
    ) -> Result<(), EditError> {
        match target {
            AnimationTarget::Clip(clip) => self.set_static_value_of_a_clip(clip, property, value),
            AnimationTarget::Track(track) => self.set_static_value_of_a_track(track, property, value),
        }
    }

    /// Gives `property` of `target` the keyframe `value` at `timeline_frame`: in place of the one there if there is one, in
    /// whichever rack it is kept, and otherwise in a new keyframe in the rack of the drawer of the clip or the track itself.
    fn put_keyframe(
        &mut self,
        target: AnimationTarget,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
        value: PropertyValue,
    ) -> Result<Option<NewKeyframe>, EditError> {
        if let Some(location) = self.locate_the_keyframe_at(target, property, timeline_frame.value()) {
            let changed = self
                .drawer_mut(location.owner)
                .and_then(|drawer| drawer.rack_mut(location.rack))
                .map(|rack| rack.set(location.frame_in_the_rack, value))
                .is_some_and(|result| result.is_ok());
            return if changed {
                Ok(None)
            } else {
                Err(not_a_property_of(target, property))
            };
        }
        match target {
            AnimationTarget::Clip(clip) => {
                let placement = self.the_placement_of_the_target(clip)?;
                let frames_in = placement
                    .offset_of(timeline_frame)
                    .ok_or(ClipEditError::NotWithinTheClip {
                        clip,
                        frames_in: timeline_frame
                            .value()
                            .saturating_sub(placement.first_timeline_frame.value()),
                    })?;
                self.track_holding_mut_or_error(clip)?
                    .set_clip_keyframe(clip, property, frames_in, value)?;
                Ok(self.the_keyframe_that_is_in_the_drawer_of(target, property, frames_in))
            }
            AnimationTarget::Track(track) => {
                if self
                    .track_mut_or_error(track)?
                    .set_own_keyframe(property, timeline_frame, value)
                {
                    Ok(self.the_keyframe_that_is_in_the_drawer_of(target, property, timeline_frame.value()))
                } else {
                    Err(EditError::NotAPropertyOfTheTrack { track, property })
                }
            }
        }
    }

    /// The keyframe of `property` of `target` that is `frame_in_the_rack` frames into the rack in the drawer of `target`
    /// itself, with where it is.
    fn the_keyframe_that_is_in_the_drawer_of(
        &self,
        target: AnimationTarget,
        property: PropertyId,
        frame_in_the_rack: u64,
    ) -> Option<NewKeyframe> {
        let rack = self.drawer_of(target)?.first_rack_for(target, property)?;
        let keyframe = rack.at(frame_in_the_rack)?;
        Some(NewKeyframe {
            owner: target,
            rack: rack.id(),
            keyframe: keyframe.id(),
        })
    }
}

/// The frame of the timeline that `placement` (a clip) is at when `timeline_frame` is asked about: that frame, or if it is
/// before the clip the clip's first, or if it is after it the clip's last.
fn the_frame_the_clip_is_at(placement: &ClipPlacement<'_>, timeline_frame: TimelineFrameIndex) -> u64 {
    let first = placement.first_timeline_frame.value();
    let last = placement.end_timeline_frame.value().saturating_sub(1).max(first);
    timeline_frame.value().clamp(first, last)
}

fn not_a_property_of(target: AnimationTarget, property: PropertyId) -> EditError {
    match target {
        AnimationTarget::Clip(clip) => ClipEditError::NotAPropertyOfThisClip { clip, property }.into(),
        AnimationTarget::Track(track) => EditError::NotAPropertyOfTheTrack { track, property },
    }
}
