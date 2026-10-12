//! Animating a property of a clip or of a track: what it is at a frame, whether and where it has keyframes, making one and
//! taking one away, and changing the property at a frame.
//!
//! Frames here are frames of the timeline. (Those of a clip's keyframes are counted from the clip's first frame, which is
//! worked out here.) A property that has no keyframes is what the clip or track says it is, as it always was; one that has
//! keyframes is what they say at every frame, and changing it at a frame puts a keyframe there.

use super::animation::{PropertyId, PropertyValue, placement_value, text_value, with_placement_value, with_text_value};
use super::tracks::{Clip, ClipEditError, ClipId, ClipPlacement, Gain, Placement, TextObject, TrackId};
use super::{EditError, Project, TimelineFrameIndex, fitted_placement};

/// What a keyframe, or a change of a property, is about: a clip, or a track.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnimationTarget {
    Clip(ClipId),
    Track(TrackId),
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
            Clip::Audio(_) => None,
        }
    }

    /// Where the clip `clip` is on the picture at `timeline_frame`: where it says, changed as its keyframes say. (A frame before
    /// the clip is taken for its first, one after it for its last.) `None` for a sound, which is not on the picture, and for a
    /// clip that is not there.
    pub fn placement_at(&self, clip: ClipId, timeline_frame: TimelineFrameIndex) -> Option<Placement> {
        let placement = self.placement_of(clip)?;
        let says = self.placement_on_the_picture(placement.clip)?;
        Some(
            placement
                .animation
                .placement_at(says, frames_into_the_clip(&placement, timeline_frame)),
        )
    }

    /// The text object of the text clip `clip` at `timeline_frame`: what it says, changed as its keyframes say. (A frame before
    /// the clip is taken for its first, one after it for its last.) `None` for what is not a text clip.
    pub fn text_object_at(&self, clip: ClipId, timeline_frame: TimelineFrameIndex) -> Option<TextObject> {
        let placement = self.placement_of(clip)?;
        let text = placement.clip.as_text()?;
        Some(
            placement
                .animation
                .text_object_at(&text.object, frames_into_the_clip(&placement, timeline_frame)),
        )
    }

    /// The value that `property` of the clip `clip` has when it is not animated.
    fn static_value_of_a_clip(&self, clip: &Clip, property: PropertyId) -> Option<PropertyValue> {
        if !property.applies_to_a_clip_of_kind(clip.kind()) {
            return None;
        }
        match property {
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
            _ => None,
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
            _ => Err(EditError::NotAPropertyOfTheTrack { track, property }),
        }
    }

    /// The clip `clip` as it is placed on the timeline, if it is there.
    fn the_placement_of_the_target(&self, clip: ClipId) -> Result<ClipPlacement<'_>, EditError> {
        Ok(self.placement_of(clip).ok_or(ClipEditError::UnknownClip { clip })?)
    }

    /// Whether `property` of `target` has keyframes.
    pub fn is_animated(&self, target: AnimationTarget, property: PropertyId) -> bool {
        match target {
            AnimationTarget::Clip(clip) => self
                .placement_of(clip)
                .is_some_and(|placement| placement.animation.is_animated(property)),
            AnimationTarget::Track(track) => self
                .track(track)
                .is_some_and(|track| track.animation().is_animated(property)),
        }
    }

    /// The value `property` of `target` has at `timeline_frame`: what its keyframes say if it has any, and otherwise what
    /// it is. For a clip, a frame before it or after it is taken for its first or its last. `None` if there is no such clip
    /// or track, or it does not have that property.
    pub fn property_value_at(
        &self,
        target: AnimationTarget,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
    ) -> Option<PropertyValue> {
        match target {
            AnimationTarget::Clip(clip) => {
                let placement = self.placement_of(clip)?;
                if !property.applies_to_a_clip_of_kind(placement.clip.kind()) {
                    return None;
                }
                let frames_in = frames_into_the_clip(&placement, timeline_frame);
                placement
                    .animation
                    .value_at(property, frames_in)
                    .or_else(|| self.static_value_of_a_clip(placement.clip, property))
            }
            AnimationTarget::Track(track) => {
                let track = self.track(track)?;
                if !property.applies_to_a_track() {
                    return None;
                }
                track
                    .animation()
                    .value_at(property, timeline_frame.value())
                    .or_else(|| self.static_value_of_a_track(track.id(), property))
            }
        }
    }

    /// Whether `property` of `target` has a keyframe exactly at `timeline_frame`.
    pub fn has_keyframe_at(
        &self,
        target: AnimationTarget,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
    ) -> bool {
        match target {
            AnimationTarget::Clip(clip) => self.placement_of(clip).is_some_and(|placement| {
                placement.offset_of(timeline_frame).is_some_and(|frames_in| {
                    placement
                        .animation
                        .keyframes_of(property)
                        .is_some_and(|keyframes| keyframes.at(frames_in).is_some())
                })
            }),
            AnimationTarget::Track(track) => self.track(track).is_some_and(|track| {
                track
                    .animation()
                    .keyframes_of(property)
                    .is_some_and(|keyframes| keyframes.at(timeline_frame.value()).is_some())
            }),
        }
    }

    /// The frame of the last keyframe of `property` of `target` before `timeline_frame`, however far before.
    pub fn keyframe_before(
        &self,
        target: AnimationTarget,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
    ) -> Option<TimelineFrameIndex> {
        self.keyframe_frames_around(target, property, timeline_frame).0
    }

    /// The frame of the first keyframe of `property` of `target` after `timeline_frame`, however far after.
    pub fn keyframe_after(
        &self,
        target: AnimationTarget,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
    ) -> Option<TimelineFrameIndex> {
        self.keyframe_frames_around(target, property, timeline_frame).1
    }

    /// The timeline frames of the keyframes of `property` of `target` that are before and after `timeline_frame`.
    fn keyframe_frames_around(
        &self,
        target: AnimationTarget,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
    ) -> (Option<TimelineFrameIndex>, Option<TimelineFrameIndex>) {
        match target {
            AnimationTarget::Clip(clip) => {
                let Some(placement) = self.placement_of(clip) else {
                    return (None, None);
                };
                let Some(keyframes) = placement.animation.keyframes_of(property) else {
                    return (None, None);
                };
                let first = placement.first_timeline_frame.value();
                let frame = timeline_frame.value();
                // A frame before the clip is before all of its keyframes (the first of them is after it, even one at the
                // clip's first frame), and one after the clip is after them all.
                let (before, after) = if frame < first {
                    (None, keyframes.iter().next().map(|keyframe| keyframe.frame))
                } else {
                    (
                        keyframes.frame_before(frame - first),
                        keyframes.frame_after(frame - first),
                    )
                };
                (
                    before.map(|offset| TimelineFrameIndex::new(first + offset)),
                    after.map(|offset| TimelineFrameIndex::new(first + offset)),
                )
            }
            AnimationTarget::Track(track) => {
                let Some(keyframes) = self
                    .track(track)
                    .and_then(|track| track.animation().keyframes_of(property))
                else {
                    return (None, None);
                };
                (
                    keyframes
                        .frame_before(timeline_frame.value())
                        .map(TimelineFrameIndex::new),
                    keyframes
                        .frame_after(timeline_frame.value())
                        .map(TimelineFrameIndex::new),
                )
            }
        }
    }

    /// Puts a keyframe at `timeline_frame` for `property` of `target`, with the value it has there now, so that the
    /// property changes from there on when another keyframe has another value. (If it has one already, nothing changes.)
    pub fn add_keyframe(
        &mut self,
        target: AnimationTarget,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
    ) -> Result<(), EditError> {
        let value = self
            .property_value_at(target, property, timeline_frame)
            .ok_or_else(|| not_a_property_of(target, property))?;
        self.put_keyframe(target, property, timeline_frame, value)
    }

    /// Takes away the keyframe of `property` of `target` at `timeline_frame`, if there is one. If it was the last, the
    /// property is not animated any more and stays what that keyframe said.
    pub fn remove_keyframe(
        &mut self,
        target: AnimationTarget,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
    ) -> Result<(), EditError> {
        let removed = match target {
            AnimationTarget::Clip(clip) => {
                let placement = self.the_placement_of_the_target(clip)?;
                let Some(frames_in) = placement.offset_of(timeline_frame) else {
                    return Ok(());
                };
                self.track_holding_mut_or_error(clip)?
                    .remove_clip_keyframe(clip, property, frames_in)?
            }
            AnimationTarget::Track(track) => self
                .track_mut_or_error(track)?
                .remove_own_keyframe(property, timeline_frame),
        };
        match removed {
            Some(removed) if removed.was_the_last => self.set_static_value(target, property, &removed.value),
            _ => Ok(()),
        }
    }

    /// Makes `property` of `target` have `value` at `timeline_frame`: if it is animated by putting a keyframe there (in
    /// place of the one there, if there is one), and if it is not by changing the property itself. (A rotation is
    /// the turn that is nearest the one before it, however many turns that is, so that it goes the short way round.)
    pub fn set_property_at(
        &mut self,
        target: AnimationTarget,
        property: PropertyId,
        value: PropertyValue,
        timeline_frame: TimelineFrameIndex,
    ) -> Result<(), EditError> {
        if !self.is_animated(target, property) {
            return self.set_static_value(target, property, &value);
        }
        let value = self.turned_the_short_way(target, property, value, timeline_frame);
        self.put_keyframe(target, property, timeline_frame, value)
    }

    fn set_static_value(
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

    fn put_keyframe(
        &mut self,
        target: AnimationTarget,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
        value: PropertyValue,
    ) -> Result<(), EditError> {
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
                Ok(())
            }
            AnimationTarget::Track(track) => {
                if self
                    .track_mut_or_error(track)?
                    .set_own_keyframe(property, timeline_frame, value)
                {
                    Ok(())
                } else {
                    Err(EditError::NotAPropertyOfTheTrack { track, property })
                }
            }
        }
    }

    /// `value`, if it is a rotation that is to be a keyframe at `timeline_frame` of an animated property, made the turn
    /// that is nearest the keyframe before it (or if none is before, the one after).
    fn turned_the_short_way(
        &self,
        target: AnimationTarget,
        property: PropertyId,
        value: PropertyValue,
        timeline_frame: TimelineFrameIndex,
    ) -> PropertyValue {
        let PropertyValue::Number(degrees) = value else {
            return value;
        };
        if property != PropertyId::Rotation {
            return value;
        }
        let (before, after) = self.keyframe_frames_around(target, property, timeline_frame);
        let reference = before
            .or(after)
            .and_then(|frame| self.keyframe_value_at(target, property, frame));
        match reference {
            Some(PropertyValue::Number(reference)) => {
                let difference = (degrees - reference + 180.0).rem_euclid(360.0) - 180.0;
                PropertyValue::Number(reference + difference)
            }
            _ => value,
        }
    }

    /// The value of the keyframe of `property` of `target` that is at `timeline_frame`.
    fn keyframe_value_at(
        &self,
        target: AnimationTarget,
        property: PropertyId,
        timeline_frame: TimelineFrameIndex,
    ) -> Option<PropertyValue> {
        match target {
            AnimationTarget::Clip(clip) => {
                let placement = self.placement_of(clip)?;
                let frames_in = placement.offset_of(timeline_frame)?;
                placement.animation.keyframes_of(property)?.at(frames_in).cloned()
            }
            AnimationTarget::Track(track) => self
                .track(track)?
                .animation()
                .keyframes_of(property)?
                .at(timeline_frame.value())
                .cloned(),
        }
    }
}

/// How many frames into `placement` (a clip) `timeline_frame` is: a frame before it is taken for its first. (A frame after it
/// is not cut back to its last: every keyframe of a clip is inside it, so a frame that is further than that has what the last
/// keyframe says, which is what the last frame has.)
fn frames_into_the_clip(placement: &ClipPlacement<'_>, timeline_frame: TimelineFrameIndex) -> u64 {
    timeline_frame
        .value()
        .saturating_sub(placement.first_timeline_frame.value())
}

fn not_a_property_of(target: AnimationTarget, property: PropertyId) -> EditError {
    match target {
        AnimationTarget::Clip(clip) => ClipEditError::NotAPropertyOfThisClip { clip, property }.into(),
        AnimationTarget::Track(track) => EditError::NotAPropertyOfTheTrack { track, property },
    }
}
