//! What the picture is at every frame: which videos and text objects are seen, one over another,
//! and where each is on the picture.
//!
//! The tracks are in front-to-back order. Every video, picture and text object that is there at a
//! frame is a *layer* of the picture, the one on the back-most track first, and each layer is
//! somewhere on the picture and turned as far as it is turned (its [`Placement`]; a video or a
//! picture that has not been put anywhere is fitted inside the picture). Where a layer does not cover the picture
//! what is behind it shows, down to black. So that what cannot be seen is not made, the layers
//! behind the front-most video that covers all of the picture (and is not turned) are left out.
//! A video that has no picture to show (its file is missing, or is shorter than its clip) is not
//! a layer at all there, and nor is a picture whose file is missing.
//!
//! The timeline is cut into *sections*, stretches of frames over which the same layers are seen.

use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::BTreeSet;

use crate::project::tracks::{CanvasArea, Clip, ClipId, Placement, TextObject, TrackId};
use crate::project::{
    Animation, AnimationTarget, FrameRate, Project, PropertyId, SourceMediaIdentifier, SpeedFactor, TimelineFrameIndex,
};

/// The properties that say where a layer is on the picture.
const PROPERTIES_OF_WHERE_A_LAYER_IS: [PropertyId; 3] = [PropertyId::Position, PropertyId::Size, PropertyId::Rotation];

/// The video that a layer shows during a section: the part of one clip that the section covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisibleVideo {
    pub source: SourceMediaIdentifier,
    /// The source frame shown by the first frame of the section.
    pub first_source_frame: u64,
    /// The frame just after the last source frame the section needs (the end is exclusive). The
    /// source file has all the frames up to this one, so the section is made entirely of pictures.
    pub end_source_frame: u64,
    /// How much faster than normal the video plays: the clip's speed times its track's. Every
    /// `speed`th source frame is shown, starting with the first. A symbol shows when it is above 1.
    pub speed: SpeedFactor,
}

/// What a layer is made of.
#[derive(Clone, Debug, PartialEq)]
pub enum LayerContent {
    Video(VisibleVideo),
    /// A picture, from an image file.
    Image {
        source: SourceMediaIdentifier,
    },
    Text(TextObject),
}

/// A video, a picture or a text object that is seen during a section, and where it is on the picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub clip: ClipId,
    /// The track the clip is on. (The pictures of a track's videos are got by a stream of the track's own.)
    pub track: TrackId,
    pub content: LayerContent,
    /// Where the layer is on the picture, and how far it is turned, before its keyframes change that. (For a text
    /// object this is where the object says it is, and for a video or a picture where it was put or, if it was not,
    /// where it is fitted.) Where it is at a frame is [`Self::placement_at`].
    pub placement: Placement,
    /// The keyframes of what is seen of the clip, from all the racks that animate it wherever they are kept: where it is, and
    /// for a text object how it looks. Their frames are frames of the timeline.
    pub animation: Animation,
}

impl Layer {
    /// The video, if this layer is one.
    pub fn video(&self) -> Option<&VisibleVideo> {
        match &self.content {
            LayerContent::Video(video) => Some(video),
            LayerContent::Image { .. } | LayerContent::Text(_) => None,
        }
    }

    /// Whether the layer has keyframes for where it is or how it looks, so that it is not the same at every frame.
    pub fn has_keyframes(&self) -> bool {
        !self.animation.is_empty()
    }

    /// The layer as it is at `timeline_frame` (a frame it is seen at), with what its keyframes say worked out: where it is, and
    /// what a text object says and how it looks, there. It has no keyframes, being the same at every frame.
    #[must_use]
    pub fn as_it_is_at(&self, timeline_frame: TimelineFrameIndex) -> Layer {
        Layer {
            clip: self.clip,
            track: self.track,
            content: match self.text_at(timeline_frame) {
                Some(object) => LayerContent::Text(object.into_owned()),
                None => self.content.clone(),
            },
            placement: self.placement_at(timeline_frame),
            animation: Animation::default(),
        }
    }

    /// Whether where the layer is on the picture is changed by keyframes, so that it is not in the same place at every frame.
    pub fn is_moved_by_keyframes(&self) -> bool {
        PROPERTIES_OF_WHERE_A_LAYER_IS
            .into_iter()
            .any(|property| self.animation.is_animated(property))
    }

    /// Where the layer is on the picture at `timeline_frame` (a frame it is seen at): where [`Self::placement`] says, changed
    /// as the keyframes say.
    pub fn placement_at(&self, timeline_frame: TimelineFrameIndex) -> Placement {
        self.animation.placement_at(self.placement, timeline_frame.value())
    }

    /// The text object the layer is at `timeline_frame`: what the object says, changed as the keyframes say. `None` if the
    /// layer is not a text object.
    pub fn text_at(&self, timeline_frame: TimelineFrameIndex) -> Option<Cow<'_, TextObject>> {
        let LayerContent::Text(object) = &self.content else {
            return None;
        };
        Some(if self.animation.is_empty() {
            Cow::Borrowed(object)
        } else {
            Cow::Owned(self.animation.text_object_at(object, timeline_frame.value()))
        })
    }

    /// Whether the layer fills all of the picture with pictures and is not turned, so that nothing
    /// behind it can be seen. (A text object never does: it is mostly clear. Nor does a picture: it
    /// may have clear places, and a picture file does not say whether it has. Nor does a video that keyframes move
    /// about: it may not at some other frame.)
    pub fn covers_the_picture(&self) -> bool {
        self.video().is_some()
            && !self.is_moved_by_keyframes()
            && self.placement.rotation.is_zero()
            && self.placement.area == CanvasArea::WHOLE_PICTURE
    }
}

/// A stretch of the timeline over which the picture is made of the same layers.
#[derive(Clone, Debug, PartialEq)]
pub struct PictureSection {
    /// The first frame of the section.
    pub first_timeline_frame: u64,
    /// The frame just after the last frame of the section (the end is exclusive).
    pub end_timeline_frame: u64,
    /// What is seen, the layer at the back first, which is the order they are drawn in, on black.
    /// Nothing at all is a black picture.
    pub layers: Vec<Layer>,
}

impl PictureSection {
    pub fn timeline_frame_count(&self) -> u64 {
        self.end_timeline_frame - self.first_timeline_frame
    }

    /// The source frame `video` (a video of one of this section's layers) shows at
    /// `timeline_frame`, or `None` if that frame is not in the section.
    pub fn source_frame_at(&self, video: &VisibleVideo, timeline_frame: TimelineFrameIndex) -> Option<u64> {
        let frame = timeline_frame.value();
        if frame < self.first_timeline_frame || frame >= self.end_timeline_frame {
            return None;
        }
        Some(video.first_source_frame + (frame - self.first_timeline_frame) * u64::from(video.speed.value()))
    }

    /// The part of the section from `first` up to `end` (frames of the timeline inside the section, with at least one frame):
    /// the same layers, where each video shows the pictures that come in those frames.
    #[must_use]
    pub fn slice(&self, first: u64, end: u64) -> PictureSection {
        debug_assert!(
            self.first_timeline_frame <= first && first < end && end <= self.end_timeline_frame,
            "{first}..{end} is not inside {}..{}",
            self.first_timeline_frame,
            self.end_timeline_frame
        );
        let layers = self
            .layers
            .iter()
            .map(|layer| match &layer.content {
                LayerContent::Video(video) => {
                    let speed = u64::from(video.speed.value());
                    let first_source_frame = video.first_source_frame + (first - self.first_timeline_frame) * speed;
                    Layer {
                        content: LayerContent::Video(VisibleVideo {
                            first_source_frame,
                            end_source_frame: (first_source_frame + (end - first) * speed).min(video.end_source_frame),
                            ..*video
                        }),
                        ..layer.clone()
                    }
                }
                LayerContent::Image { .. } | LayerContent::Text(_) => layer.clone(),
            })
            .collect();
        PictureSection {
            first_timeline_frame: first,
            end_timeline_frame: end,
            layers,
        }
    }

    /// The layers that are videos, the one at the back first.
    pub fn video_layers(&self) -> impl Iterator<Item = (&Layer, &VisibleVideo)> {
        self.layers.iter().filter_map(|layer| Some((layer, layer.video()?)))
    }

    /// The video that is in front of the others, if there is one. It is the one whose speed the
    /// fast-forward symbol tells.
    pub fn front_video(&self) -> Option<(&Layer, &VisibleVideo)> {
        self.video_layers().last()
    }
}

/// The picture at every frame of the timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct PicturePlan {
    pub frame_rate: FrameRate,
    /// The size of the finished picture, in pixels.
    pub picture_size: (u32, u32),
    pub total_frame_count: u64,
    /// The sections, in order, with no gaps between them: together they cover the whole timeline.
    pub sections: Vec<PictureSection>,
    /// How many frames have a video in them that has no picture to show only because the file that
    /// it is made from does not have enough pictures for the clip (which happens when a project is
    /// pointed at a shorter file than the one it was edited on). A video behind one that covers the
    /// picture is not seen, so it is not missed.
    pub frames_missing_a_picture: u64,
}

impl PicturePlan {
    /// The section that `timeline_frame` is in, if it is on the timeline.
    pub fn section_at(&self, timeline_frame: TimelineFrameIndex) -> Option<&PictureSection> {
        self.sections.get(self.section_index_at(timeline_frame)?)
    }

    /// Where in [`Self::sections`] the section is that `timeline_frame` is in, if it is on the timeline.
    pub fn section_index_at(&self, timeline_frame: TimelineFrameIndex) -> Option<usize> {
        let frame = timeline_frame.value();
        let index = self
            .sections
            .partition_point(|section| section.end_timeline_frame <= frame);
        self.sections
            .get(index)
            .filter(|section| section.first_timeline_frame <= frame)
            .map(|_| index)
    }
}

/// A video or text clip as the planning sees it.
struct Candidate<'project> {
    /// Where the clip's track is in the list of tracks: 0 is the front.
    track_position: usize,
    track: TrackId,
    clip: ClipId,
    first_timeline_frame: u64,
    end_timeline_frame: u64,
    effective_speed: SpeedFactor,
    content: &'project Clip,
    /// For a video: how many frames the source file has (none, if the file cannot be used).
    source_frame_count: u64,
    /// Where the clip is on the picture, before its keyframes change that.
    placement: Placement,
    /// The keyframes of what is seen of the clip.
    animation: Animation,
}

impl Candidate<'_> {
    /// The layer that the clip is, with `content`.
    fn layer(&self, content: LayerContent) -> Layer {
        Layer {
            clip: self.clip,
            track: self.track,
            content,
            placement: self.placement,
            animation: self.animation.clone(),
        }
    }

    /// For a video that starts at the source frame `clip_first_source_frame`: the source frame it
    /// shows at `timeline_frame`, a frame at which the clip is there.
    fn source_frame_at(&self, clip_first_source_frame: u64, timeline_frame: u64) -> u64 {
        clip_first_source_frame + (timeline_frame - self.first_timeline_frame) * u64::from(self.effective_speed.value())
    }

    /// For a video: how many of the frames `first..end` of the timeline it has pictures for.
    fn pictures_available(&self, first: u64, end: u64) -> u64 {
        let Some(media) = self.content.as_media() else {
            return 0;
        };
        let speed = u64::from(self.effective_speed.value());
        let first_source_frame = self.source_frame_at(media.first_source_frame().value(), first);
        let limit = media.end_source_frame().value().min(self.source_frame_count);
        if first_source_frame >= limit {
            0
        } else {
            (limit - first_source_frame).div_ceil(speed).min(end - first)
        }
    }
}

/// Works out the picture at every frame of the project. A video from a file that is not `usable`
/// has no pictures, as a video has none where its file has no more.
pub fn plan_pictures(project: &Project, is_usable: impl Fn(SourceMediaIdentifier) -> bool) -> PicturePlan {
    let picture_size = project.picture_size();
    let animations = project.resolved_animations();
    let mut candidates = Vec::new();
    for (track_position, track) in project.tracks().iter().enumerate() {
        for placement in track.placements() {
            let source_frame_count = match placement.clip {
                Clip::Video(media) if is_usable(media.source()) => {
                    project.frame_count_of_source(media.source()).unwrap_or(0)
                }
                Clip::Video(_) => 0,
                // A picture has no pictures to run out of: it is there or its file is not.
                Clip::Image(image) => u64::from(is_usable(image.source())),
                Clip::Text(_) => 0,
                // (Nothing is seen of a sound, or of a crate, which holds nothing but keyframes.)
                Clip::Audio(_) | Clip::Crate(_) => continue,
            };
            let where_it_is = project
                .placement_on_the_picture(placement.clip)
                .expect("a clip that is not a sound is on the picture");
            candidates.push(Candidate {
                track_position,
                track: track.id(),
                clip: placement.id,
                first_timeline_frame: placement.first_timeline_frame.value(),
                end_timeline_frame: placement.end_timeline_frame.value(),
                effective_speed: placement.effective_speed,
                content: placement.clip,
                source_frame_count,
                placement: where_it_is,
                animation: animations
                    .of(AnimationTarget::Clip(placement.id))
                    .restricted_to(PropertyId::is_seen),
            });
        }
    }

    let total_frame_count = project.total_frame_count();
    let boundaries: BTreeSet<u64> = candidates
        .iter()
        .flat_map(|candidate| [candidate.first_timeline_frame, candidate.end_timeline_frame])
        .chain([0, total_frame_count])
        .collect();
    let boundaries: Vec<u64> = boundaries
        .into_iter()
        .filter(|&frame| frame <= total_frame_count)
        .collect();

    // Going from one boundary to the next, the clips that are there are kept track of (they are
    // few: about one for each track), so that the work does not grow with the square of the number of clips.
    let mut by_start: Vec<usize> = (0..candidates.len()).collect();
    by_start.sort_by_key(|&index| candidates[index].first_timeline_frame);
    let mut next_to_start = 0;
    let mut active: Vec<&Candidate<'_>> = Vec::new();

    let mut sections: Vec<PictureSection> = Vec::new();
    let mut frames_missing_a_picture = 0;
    for pair in boundaries.windows(2) {
        let (first, end) = (pair[0], pair[1]);
        active.retain(|candidate| candidate.end_timeline_frame > first);
        while let Some(&index) = by_start.get(next_to_start)
            && candidates[index].first_timeline_frame <= first
        {
            active.push(&candidates[index]);
            next_to_start += 1;
        }
        let (new_sections, frames_missing_pictures_here) = sections_between(first, end, &active);
        frames_missing_a_picture += frames_missing_pictures_here;
        for section in new_sections {
            push_merging_with_the_one_before(&mut sections, section);
        }
    }
    PicturePlan {
        frame_rate: project.frame_rate(),
        picture_size,
        total_frame_count,
        sections,
        frames_missing_a_picture,
    }
}

/// The sections (one, or more if a video runs out of source pictures partway) for the frames
/// `first` up to `end`, over which no clip starts or ends, and how many of those frames have a video
/// in front of what is seen that has no picture to show.
fn sections_between(first: u64, end: u64, active: &[&Candidate<'_>]) -> (Vec<PictureSection>, u64) {
    debug_assert!(
        active
            .iter()
            .all(|candidate| candidate.first_timeline_frame <= first && end <= candidate.end_timeline_frame),
        "every clip in `active` is there for the whole of the stretch"
    );
    let mut back_to_front: Vec<&Candidate<'_>> = active.to_vec();
    back_to_front.sort_by_key(|candidate| Reverse(candidate.track_position));

    // Each video shows pictures until it runs out: that is where the stretch is cut.
    let runs_out_at = |candidate: &Candidate<'_>| first + candidate.pictures_available(first, end);
    let mut cuts = BTreeSet::from([first, end]);
    for candidate in back_to_front
        .iter()
        .filter(|candidate| candidate.content.as_media().is_some())
    {
        cuts.insert(runs_out_at(candidate));
    }
    let cuts: Vec<u64> = cuts.into_iter().collect();

    let mut sections = Vec::new();
    let mut frames_missing_a_picture = 0;
    for pair in cuts.windows(2) {
        let (from, until) = (pair[0], pair[1]);
        // The layers there are over these frames, the one at the back first (the ones with their
        // track position, which tells what is in front of what).
        let mut layers: Vec<(usize, Layer)> = Vec::new();
        let mut missing_pictures_in_front_of = Vec::new();
        for candidate in &back_to_front {
            match candidate.content {
                Clip::Video(media) => {
                    if from >= runs_out_at(candidate) {
                        missing_pictures_in_front_of.push(candidate.track_position);
                        continue;
                    }
                    let first_source_frame = candidate.source_frame_at(media.first_source_frame().value(), from);
                    let limit = media.end_source_frame().value().min(candidate.source_frame_count);
                    let speed = u64::from(candidate.effective_speed.value());
                    // The range ends where the last picture's group of source frames does, but not
                    // beyond what the file (and the clip) have, so that a whole clip is cut exactly as the clip is.
                    let end_source_frame = (first_source_frame + (until - from) * speed).min(limit);
                    layers.push((
                        candidate.track_position,
                        candidate.layer(LayerContent::Video(VisibleVideo {
                            source: media.source(),
                            first_source_frame,
                            end_source_frame,
                            speed: candidate.effective_speed,
                        })),
                    ));
                }
                Clip::Image(image) => {
                    if candidate.source_frame_count > 0 {
                        layers.push((
                            candidate.track_position,
                            candidate.layer(LayerContent::Image { source: image.source() }),
                        ));
                    }
                }
                Clip::Text(text) => layers.push((
                    candidate.track_position,
                    candidate.layer(LayerContent::Text(text.object.clone())),
                )),
                Clip::Audio(_) | Clip::Crate(_) => {}
            }
        }

        // What is behind the front-most layer that covers the picture cannot be seen.
        let covering = layers.iter().rposition(|(_, layer)| layer.covers_the_picture());
        if let Some(position_in_layers) = covering {
            layers.drain(..position_in_layers);
        }
        // A video with no picture is only missed if it would have been seen: nothing that covers
        // the picture is in front of it. (A track position that is less is further front.)
        let front_most_covering_track_position = covering.map(|_| layers[0].0);
        let is_missed = missing_pictures_in_front_of
            .iter()
            .any(|&track_position| front_most_covering_track_position.is_none_or(|covering| track_position < covering));
        if is_missed {
            frames_missing_a_picture += until - from;
        }

        sections.push(PictureSection {
            first_timeline_frame: from,
            end_timeline_frame: until,
            layers: layers.into_iter().map(|(_, layer)| layer).collect(),
        });
    }
    (sections, frames_missing_a_picture)
}

/// Adds `section` after the others, joined to the one before it if they show the same things and
/// run on from one another.
fn push_merging_with_the_one_before(sections: &mut Vec<PictureSection>, section: PictureSection) {
    if let Some(previous) = sections.last_mut()
        && previous.end_timeline_frame == section.first_timeline_frame
        && let Some(carried_on) = layers_carried_on(previous, &section)
    {
        previous.layers = carried_on;
        previous.end_timeline_frame = section.end_timeline_frame;
        return;
    }
    sections.push(section);
}

/// The layers of `previous` and `section` together, if each layer of `section` is the one of
/// `previous` carrying on: the same clip, with the same text or picture file or with the pictures
/// that come right after. (A clip is in one place, and has the same keyframes, for as long as it lasts, so the
/// same clip is in the same place and has the same keyframes.)
fn layers_carried_on(previous: &PictureSection, section: &PictureSection) -> Option<Vec<Layer>> {
    if previous.layers.len() != section.layers.len() {
        return None;
    }
    previous
        .layers
        .iter()
        .zip(&section.layers)
        .map(|(before, after)| {
            if before.clip != after.clip {
                return None;
            }
            debug_assert_eq!(
                (before.placement, &before.animation),
                (after.placement, &after.animation),
                "a clip is in one place, with the same keyframes, for as long as it lasts"
            );
            match (&before.content, &after.content) {
                (LayerContent::Text(first), LayerContent::Text(second)) if first == second => Some(before.clone()),
                (LayerContent::Image { source: first }, LayerContent::Image { source: second }) if first == second => {
                    Some(before.clone())
                }
                (LayerContent::Video(first), LayerContent::Video(second))
                    if runs_on(*first, previous.timeline_frame_count(), *second) =>
                {
                    Some(Layer {
                        content: LayerContent::Video(VisibleVideo {
                            end_source_frame: second.end_source_frame,
                            ..*first
                        }),
                        ..before.clone()
                    })
                }
                _ => None,
            }
        })
        .collect()
}

/// Whether `after` shows the pictures that come right after those `before` shows over
/// `frame_count` frames.
fn runs_on(before: VisibleVideo, frame_count: u64, after: VisibleVideo) -> bool {
    before.source == after.source
        && before.speed == after.speed
        && after.first_source_frame == before.first_source_frame + frame_count * u64::from(before.speed.value())
}

#[cfg(test)]
mod animation_tests;
#[cfg(test)]
mod tests;
