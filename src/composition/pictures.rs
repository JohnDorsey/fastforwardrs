// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

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
use std::collections::{BTreeSet, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use crate::colour_effects::ColourMatrix;
use crate::cube_table::CubeTable;
use crate::project::tracks::{CanvasArea, Clip, ClipId, Placement, TextObject, TrackId};
use crate::project::{
    Animation, AnimationTarget, AppliedEffect, EffectInstanceId, EffectKind, FrameRate, GroupId, ImportedPicture,
    Member, PictureOperation, Project, PropertyId, PropertyValue, ResolvedAnimations, SourceMediaIdentifier,
    SpeedFactor, TailLength, TimelineFrameIndex, text_object_of_values,
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
    /// A text: its text effect, as a layer has an effect, with the keyframes of its values (what it says and how it looks); where it is on the picture is the layer's [`Layer::placement`].
    /// (The text effect is not among the layer's `effects`: it makes the picture and is not done to one.)
    Text(LayerEffect),
    /// A generator clip: what it makes is its first effect, which is a shader that reads no picture, and the layer has nothing to be drawn from but that (see [`GeneratorClip`](crate::project::GeneratorClip)).
    Generator,
    /// An adjustment clip: no picture of its own. **It is not in the layers of a section**: what it does is a group of everything behind it (see [`GroupSpan`]).
    Adjustment,
}

/// The clock that an effect that is a shader (`TIME`, `FRAMEINDEX`) reads: **never the wall clock**, but the frames since the thing the effect is on began (a clip's first frame; for a
/// track, a tag or a group, the first frame of the timeline), at the frame rate of the project (PH-12).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectClock {
    pub first_frame: u64,
    pub frames_per_second: f64,
}

impl Default for EffectClock {
    fn default() -> Self {
        Self {
            first_frame: 0,
            frames_per_second: 30.0,
        }
    }
}

impl EffectClock {
    /// How many frames of the project there are since the clock began, at `timeline_frame` (none before it).
    pub fn frames_at(self, timeline_frame: u64) -> u64 {
        timeline_frame.saturating_sub(self.first_frame)
    }

    /// How many seconds there are since the clock began, at `timeline_frame`.
    pub fn seconds_at(self, timeline_frame: u64) -> f64 {
        self.frames_at(timeline_frame) as f64 / self.frames_per_second
    }
}

/// An effect that is on a layer: one that its clip has, or that its track has, with what its values are and the keyframes of them.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerEffect {
    /// What `TIME` and `FRAMEINDEX` are for it, if it is a shader (see [`EffectClock`]).
    pub clock: EffectClock,
    /// Which effect of the chain of its clip or its track it is, which its values are named by.
    pub instance: EffectInstanceId,
    pub kind: EffectKind,
    /// What its values are when they are not animated, one for each of the properties of the kind of effect, in their order.
    pub values: Vec<PropertyValue>,
    /// The keyframes of its values, from all the racks that animate them wherever they are kept. Their frames are frames of the
    /// timeline.
    pub animation: Animation,
    /// The table that each of its values is, one for each value, `Some` for a value that names a resource of the project that is a table the program can read (the
    /// resources are not in the layer, so what they are is worked out when the plan is made).
    pub tables: Vec<Option<Arc<CubeTable>>>,
    /// What its plugin saved of what is not one of its values, if the project has it (see [`crate::project::Effect::state`]): a plugin that is made for the sound is given it before
    /// its values.
    pub state: Option<Arc<[u8]>>,
    /// How long the effect goes on after its input ends, when its person has said (see [`crate::project::Effect::tail_length`]): a sound plugin that goes on is given silence for that long
    /// (phase 12).
    pub tail_length: TailLength,
    /// **What tells the clips that share one state of this effect** (phase 12, piece 12.5; section 5 of docs/engine-phase-12-design.md): an effect of a track whose sharing of picture effects is not
    /// *one copy for each clip* is one instance for a run of clips (or for the whole track), so the clips of the run have the same number here, and the state that a shader that depends on
    /// earlier frames keeps goes on from one of them to the next. `None`: the state is the clip's own, as it is for the effects of a clip and of a tag.
    pub shared_by: Option<u64>,
}

impl LayerEffect {
    /// Whether any of the values of the effect has keyframes, so that it is not the same at every frame. (What the tests ask of a plan; the export asks for the
    /// values at each frame and does not care.)
    #[cfg(test)]
    pub fn is_animated(&self) -> bool {
        !self.animation.is_empty()
    }

    /// What the values of the effect are at `timeline_frame`: what the keyframes say for each that has some, and what it is for
    /// each that has none.
    pub fn values_at(&self, timeline_frame: u64) -> Vec<PropertyValue> {
        self.values
            .iter()
            .enumerate()
            .map(|(slot, value)| {
                let property = PropertyId::effect_value(
                    self.kind,
                    self.instance,
                    u8::try_from(slot).expect("an effect has fewer than 256 values"),
                );
                self.animation
                    .value_at(property, timeline_frame)
                    .unwrap_or_else(|| value.clone())
            })
            .collect()
    }
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
    /// for a text object how it looks. Their frames are frames of the timeline. (Those of its effects are in the effects.)
    pub animation: Animation,
    /// What is done to the picture of the layer, one after another: the effects that are applied to its clip by itself, in the order they are
    /// applied in (see [`Project::effects_applied_to_every_clip_by_itself`]): those of the clip itself, those of its track where the badge of the
    /// track is in the chain of the clip, and those of the tags the clip is in where their badges are. **The effects of the clip groups and
    /// the track groups it is in are not here** (phase 5): they are done to the picture that the group makes, which is [`PictureSection::groups`].
    pub effects: Vec<LayerEffect>,
}

impl Layer {
    /// The video, if this layer is one.
    pub fn video(&self) -> Option<&VisibleVideo> {
        match &self.content {
            LayerContent::Video(video) => Some(video),
            LayerContent::Image { .. } | LayerContent::Text(_) | LayerContent::Generator | LayerContent::Adjustment => {
                None
            }
        }
    }

    /// Whether the layer has keyframes for where it is, how it looks or what its effects do, so that it is not the same at every
    /// frame. (What the tests ask of a plan.)
    #[cfg(test)]
    pub fn has_keyframes(&self) -> bool {
        !self.animation.is_empty() || self.effects.iter().any(LayerEffect::is_animated)
    }

    /// What the effects of the layer do to its picture at `timeline_frame` (a frame it is seen at), one after another in the order they are applied in. **An effect that
    /// does nothing at the frame is left out, and neighbouring effects that mix colours are one** (a hue shift, a saturation and a brightness make one operation, which
    /// the engine does in one pass).
    pub fn operations_at(&self, timeline_frame: TimelineFrameIndex) -> Vec<PictureOperation> {
        operations_of_effects(&self.effects, timeline_frame, hash_of(self.clip))
    }

    /// How the effects of the layer that mix colours change the colours of its picture at `timeline_frame` (a frame it is seen at), all of them
    /// together: the matrix that changes nothing if none of them changes colours. (What the old drawing does; the others are left out of it.)
    pub fn colour_matrix_at(&self, timeline_frame: TimelineFrameIndex) -> ColourMatrix {
        self.effects
            .iter()
            .filter_map(|effect| effect.kind.colour_matrix(&effect.values_at(timeline_frame.value())))
            .fold(ColourMatrix::NOTHING_CHANGES, ColourMatrix::then)
    }

    /// How the effects of the layer change the colours of its picture, all of them together, with the values they have when they
    /// are not animated. (What the tests ask of a plan that has no keyframes; the export asks [`Self::colour_matrix_at`].)
    #[cfg(test)]
    pub fn colour_matrix(&self) -> ColourMatrix {
        self.effects
            .iter()
            .filter_map(|effect| effect.kind.colour_matrix(&effect.values))
            .fold(ColourMatrix::NOTHING_CHANGES, ColourMatrix::then)
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

    /// The text object the layer is at `timeline_frame`: what it says and how it looks, which are the values of its text effect, and where it is, each changed as the keyframes say. `None` if the
    /// layer is not a text object.
    pub fn text_at(&self, timeline_frame: TimelineFrameIndex) -> Option<Cow<'_, TextObject>> {
        let LayerContent::Text(effect) = &self.content else {
            return None;
        };
        Some(Cow::Owned(text_object_of_values(
            &effect.values_at(timeline_frame.value()),
            self.placement_at(timeline_frame),
        )))
    }

    /// Whether the layer fills all of the picture with pictures and is not turned, so that nothing
    /// behind it can be seen. (A text object never does: it is mostly clear. Nor does a picture: it
    /// may have clear places, and a picture file does not say whether it has. Nor does a video that keyframes move
    /// about: it may not at some other frame.)
    pub fn covers_the_picture(&self) -> bool {
        self.video().is_some()
            // (An effect that makes pixels clear, an opacity, a crop, a blur, lets what is behind show.)
            && effects_keep_the_opacity(&self.effects)
            && !self.is_moved_by_keyframes()
            && self.placement.rotation.is_zero()
            && self.placement.area == CanvasArea::WHOLE_PICTURE
    }
}

/// Whether `effects` keep every pixel of the picture they are done to as opaque as it was: every one is a change of colours by itself (a mix, an exposure, a table), by the kind of
/// the effect and whatever its values are. (One that is not, an opacity, a crop, a blur, may let what is behind show.)
pub(crate) fn effects_keep_the_opacity(effects: &[LayerEffect]) -> bool {
    effects
        .iter()
        .all(|effect| effect.kind.descriptor().changes_each_pixel_by_itself)
}

/// A number made of `value`, the same for the same value every time (what tells the states of effects from each other).
fn hash_of(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// What tells the state of the effect that is number `place` of `effects`, which are done to what `seed` says (a clip, or a group), from every other state: it is the same at every
/// frame, and **not the same for another clip, another place in the chain, or another kind of effect**. (An effect of a track or of a tag is done to each clip it reaches, with one
/// instance, so the instance alone would not tell them apart; a change of the order of the chain makes the effect start clear.)
fn state_id(seed: u64, place: usize, effect: &LayerEffect) -> u64 {
    // (An effect that clips share is the same state whatever clip it is done to and wherever it is in the chain of that clip.)
    match effect.shared_by {
        Some(shared_by) => hash_of((shared_by, effect.instance, effect.kind.key())),
        None => hash_of((seed, place, effect.instance, effect.kind.key())),
    }
}

/// The images that the shader of `effect` brings with it, as pictures, in the order its file lists them (none for an effect that is not a shader). One that cannot be read leaves the list
/// short, and the effect is not run then: a shader that is not usable is not installed, so it does not come to this.
fn imported_pictures_of(effect: &LayerEffect) -> Vec<ImportedPicture> {
    let Some(source) = effect.kind.shader_source() else {
        return Vec::new();
    };
    source
        .images
        .iter()
        .enumerate()
        .filter_map(|(number, image)| {
            Some(ImportedPicture {
                id: hash_of((effect.kind.key(), number)),
                picture: image.pixels().ok()?,
            })
        })
        .collect()
}

/// What `effects` do to a picture at `timeline_frame`, one after another in the order they are applied in. An effect that does nothing at the frame is left out, and neighbouring
/// effects that mix colours are one. `seed` is a number that tells what the effects are done to (a clip, or a group) from every other thing: the states of the effects that depend on
/// earlier frames are told from each other by it.
pub fn operations_of_effects(
    effects: &[LayerEffect],
    timeline_frame: TimelineFrameIndex,
    seed: u64,
) -> Vec<PictureOperation> {
    let mut operations: Vec<PictureOperation> = Vec::new();
    for (place, effect) in effects.iter().enumerate() {
        let values = effect.values_at(timeline_frame.value());
        let next = if effect.kind.shader_source().is_some() {
            // (A shader that cannot be run is an effect that does nothing, as an effect that the program does not have does.)
            if !effect.kind.is_installed() {
                continue;
            }
            let clock = effect.clock;
            let mut numbers = vec![
                clock.seconds_at(timeline_frame.value()) as f32,
                (1.0 / clock.frames_per_second) as f32,
                clock.frames_at(timeline_frame.value()) as f32,
                0.0,
            ];
            numbers.extend(crate::isf::numbers_of_the_values(&values));
            PictureOperation::Shader {
                key: effect.kind.key().to_owned(),
                numbers,
                state: effect
                    .kind
                    .depends_on_earlier_frames()
                    .then(|| (state_id(seed, place, effect), clock.frames_at(timeline_frame.value()))),
                images: imported_pictures_of(effect),
                reads_the_backdrop: effect.kind.is_a_transition(),
            }
        } else {
            let Some(next) = effect.kind.picture_operation_with_tables(&values, &effect.tables) else {
                continue;
            };
            next
        };
        if next.changes_nothing() {
            continue;
        }
        match operations.last().and_then(|last| last.then_if_alike(&next)) {
            Some(together) => {
                operations.pop();
                // (Two that undo each other are none.)
                if !together.changes_nothing() {
                    operations.push(together);
                }
            }
            None => operations.push(next),
        }
    }
    operations
}

/// A clip group or a track group that makes a picture of its own in a section (phase 5): **the layers `first_layer..end_layer` of the section are drawn together, and the
/// effects of the group are done to the picture they make**, which is then put over what is behind. A group with no effect that does anything is not one of these: its layers are
/// drawn straight into what is behind (it is pass-through: GR-10, Q-33). The spans of a section are in the order of their first layers, the one that holds another first.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupSpan {
    pub owner: SpanOwner,
    /// The first of the section's layers that is in the group.
    pub first_layer: usize,
    /// The layer after the last of the group's.
    pub end_layer: usize,
    /// What is done to the group's picture, one after another. (A clip group's or a track group's effects have no keyframes, so they are the same at every frame; an
    /// adjustment clip's may have, like any clip's.)
    pub effects: Vec<LayerEffect>,
    /// The group's layers are put over opaque black first (an adjustment clip at the frame: what is behind it is over black, as everything is).
    pub over_black: bool,
}

/// What makes a [`GroupSpan`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpanOwner {
    /// A clip group or a track group that has an effect that does something.
    Group(GroupId),
    /// An adjustment clip: everything behind it is its group.
    Adjustment(ClipId),
}

impl GroupSpan {
    /// What the effects of the group do to its picture at `timeline_frame`.
    pub fn operations_at(&self, timeline_frame: TimelineFrameIndex) -> Vec<PictureOperation> {
        operations_of_effects(&self.effects, timeline_frame, hash_of(self.owner))
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
    /// The groups that make a picture of their own over some of the layers, the one that holds another first (see [`GroupSpan`]).
    pub groups: Vec<GroupSpan>,
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
    /// What is done to its picture by itself: the effects that are applied to the clip (those of its groups are done to the picture of the group, see
    /// [`Layer::effects`]).
    effects: Vec<LayerEffect>,
    /// For a text clip, its text effect, which is not among `effects` (it makes the picture and is not done to one).
    text: Option<LayerEffect>,
    /// The clip groups and track groups that make a picture of their own that the clip is in, the outermost first.
    shapes: Vec<GroupId>,
}

impl Candidate<'_> {
    /// The layer that the clip is, with `content`, in the groups it is in.
    fn entry(&self, content: LayerContent) -> Entry {
        Entry {
            track_position: self.track_position,
            layer: Layer {
                clip: self.clip,
                track: self.track,
                content,
                placement: self.placement,
                animation: self.animation.clone(),
                effects: self.effects.clone(),
            },
            shapes: self.shapes.clone(),
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

/// An effect that is applied to a clip, as a layer has it: with the keyframes of its values. (The values of an effect of a clip or of a track are
/// animated by that clip's or track's keyframes; those of the effect of a group are not animated.)
pub(crate) fn layer_effect_of(
    project: &Project,
    animations: &ResolvedAnimations,
    applied: &AppliedEffect<'_>,
) -> LayerEffect {
    let kind = applied.effect.kind();
    let animation = match applied.owner {
        Member::Clip(clip) => Some(AnimationTarget::Clip(clip)),
        Member::Track(track) => Some(AnimationTarget::Track(track)),
        Member::Group(_) => None,
    }
    .map_or_else(Animation::default, |target| {
        animations
            .of(target)
            .restricted_to(|property| applied.effect.value_of(property).is_some())
    });
    LayerEffect {
        clock: EffectClock {
            first_frame: match applied.owner {
                Member::Clip(clip) => project
                    .placement_of(clip)
                    .map_or(0, |placement| placement.first_timeline_frame.value()),
                Member::Track(_) | Member::Group(_) => 0,
            },
            frames_per_second: project.frame_rate().frames_per_second(),
        },
        instance: applied.effect.instance(),
        kind,
        values: applied.effect.values().to_vec(),
        state: applied.effect.state().cloned(),
        tail_length: applied.effect.tail_length(),
        shared_by: None,
        animation,
        tables: kind
            .parameters()
            .iter()
            .zip(applied.effect.values())
            .map(|(parameter, value)| {
                project
                    .resource_named_by(&parameter.control, value)
                    .and_then(|resource| resource.cube_table().ok())
            })
            .collect(),
    }
}

/// For each clip on a track whose sharing of picture effects is not *one copy for each clip*, the number that tells which clips share the state of the track's effects: **for a run** (clips
/// one right after another that are in the same groups: where a group begins or ends a run does, as it does for the sound) the number of its first clip; **for one instance for the track**
/// (per track, with windows, or indivisible) the same number for all its clips. (A clip that follows another after a gap begins a run of its own: no layer is kept for the gap, so the state is
/// not there at the end of it. See section 6.2 of docs/engine-phase-12-design.md.)
fn runs_of_clips_that_share_the_state_of_a_tracks_effects(
    project: &Project,
    group_effects: &HashMap<GroupId, Vec<LayerEffect>>,
) -> HashMap<ClipId, u64> {
    use crate::project::TrackSharing;
    let mut keys = HashMap::new();
    for track in project.tracks() {
        match track.sharing().picture {
            TrackSharing::PerClip => {}
            TrackSharing::PerTrackWithWindows | TrackSharing::PerTrackIndivisible => {
                let key = hash_of(("the picture effects of a track", track.id()));
                keys.extend(track.entries().iter().map(|entry| (entry.id(), key)));
            }
            TrackSharing::PerRun => {
                let mut run: Option<(u64, u64, Vec<GroupId>)> = None;
                for placement in track.placements() {
                    // (Only the groups that make a picture of their own are edges: the others are not there, as far as the picture goes.)
                    let groups: Vec<GroupId> = project
                        .shapes_round_the_clip_outermost_first(placement.id)
                        .into_iter()
                        .filter(|group| group_effects.contains_key(group))
                        .collect();
                    let key = match &run {
                        Some((key, end, in_groups))
                            if *end == placement.first_timeline_frame.value() && *in_groups == groups =>
                        {
                            *key
                        }
                        _ => hash_of(("the picture effects of a track", track.id(), placement.id)),
                    };
                    keys.insert(placement.id, key);
                    run = Some((key, placement.end_timeline_frame.value(), groups));
                }
            }
        }
    }
    keys
}

/// Works out the picture at every frame of the project. A video from a file that is not `usable`
/// has no pictures, as a video has none where its file has no more.
pub fn plan_pictures(project: &Project, is_usable: impl Fn(SourceMediaIdentifier) -> bool) -> PicturePlan {
    let picture_size = project.picture_size();
    let animations = project.resolved_animations();
    let applied_effects = project.effects_applied_to_every_clip_by_itself();
    let group_effects = effects_of_the_groups_that_make_pictures(project, &animations);
    let runs = runs_of_clips_that_share_the_state_of_a_tracks_effects(project, &group_effects);
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
                Clip::Text(_) | Clip::Adjustment(_) | Clip::Generator(_) => 0,
                // (Nothing is seen of a sound, or of a crate, which holds nothing but keyframes.)
                Clip::Audio(_) | Clip::Crate(_) | Clip::Midi(_) => continue,
            };
            let where_it_is = match placement.clip {
                // (An adjustment clip is the whole picture, and is not drawn: what it does is done to what is behind it.)
                Clip::Adjustment(_) => Placement::upright(CanvasArea::WHOLE_PICTURE),
                clip => project
                    .placement_on_the_picture(clip)
                    .expect("a clip that is not a sound is on the picture"),
            };
            let effects_of_the_picture = {
                let mut effects: Vec<LayerEffect> = applied_effects
                    .get(&placement.id)
                    .into_iter()
                    .flatten()
                    .filter(|applied| applied.effect.kind().is_installed())
                    .map(|applied| {
                        let mut effect = layer_effect_of(project, &animations, applied);
                        // (The state of an effect of a track is shared by the clips of its run, or by all the clips of the track.)
                        if let Member::Track(owner) = applied.owner {
                            effect.shared_by = runs.get(&placement.id).copied().filter(|_| owner == track.id());
                        }
                        effect
                    })
                    .collect();
                // (A text clip's text effect makes its picture: it is taken out of what is done to the picture, so that nothing about the places of the others changes.)
                let text = match placement.clip {
                    Clip::Text(text) => effects
                        .iter()
                        .position(|effect| effect.kind == EffectKind::TEXT && effect.instance == text.content())
                        .map(|place| effects.remove(place)),
                    _ => None,
                };
                (effects, text)
            };
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
                // (What the effects of the clip and of the track do is in the layer's effects, not here: both have their own
                // value of the same property.)
                animation: animations
                    .of(AnimationTarget::Clip(placement.id))
                    .restricted_to(|property| property.is_seen() && !property.is_a_value_of_an_effect()),
                // (An effect that the program does not have, which a project file brought, does nothing to the picture: it is not here, so
                // that its keyframes do not make the picture different at every frame, which makes the export slower.)
                effects: effects_of_the_picture.0,
                text: effects_of_the_picture.1,
                shapes: project
                    .shapes_round_the_clip_outermost_first(placement.id)
                    .into_iter()
                    .filter(|shape| group_effects.contains_key(shape))
                    .collect(),
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
        let (new_sections, frames_missing_pictures_here) = sections_between(first, end, &active, &group_effects);
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
fn sections_between(
    first: u64,
    end: u64,
    active: &[&Candidate<'_>],
    group_effects: &HashMap<GroupId, Vec<LayerEffect>>,
) -> (Vec<PictureSection>, u64) {
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
        let mut layers: Vec<Entry> = Vec::new();
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
                    layers.push(candidate.entry(LayerContent::Video(VisibleVideo {
                        source: media.source(),
                        first_source_frame,
                        end_source_frame,
                        speed: candidate.effective_speed,
                    })));
                }
                Clip::Image(image) => {
                    if candidate.source_frame_count > 0 {
                        layers.push(candidate.entry(LayerContent::Image { source: image.source() }));
                    }
                }
                // (A text clip that has no text effect, which a project that is consistent does not have, makes no picture.)
                Clip::Text(_) => {
                    if let Some(effect) = &candidate.text {
                        layers.push(candidate.entry(LayerContent::Text(effect.clone())));
                    }
                }
                Clip::Generator(_) => layers.push(candidate.entry(LayerContent::Generator)),
                Clip::Adjustment(_) => layers.push(candidate.entry(LayerContent::Adjustment)),
                Clip::Audio(_) | Clip::Crate(_) | Clip::Midi(_) => {}
            }
        }

        // The layers that are in a group that makes a picture of its own are nodes of a tree: what is behind the front-most layer that covers the picture (or the
        // group that covers it) cannot be seen, inside each group and in the frame.
        let mut nodes = make_the_adjustments_groups(nodes_of(layers, 0, group_effects), true);
        let covering = nodes.iter().rposition(Node::covers_the_picture);
        let front_most_covering_track_position =
            covering.map(|position| nodes[position].front_most_covering_track_position());
        drain_what_is_behind_what_covers(&mut nodes);
        // A video with no picture is only missed if it would have been seen: nothing that covers
        // the picture is in front of it. (A track position that is less is further front.)
        let is_missed = missing_pictures_in_front_of
            .iter()
            .any(|&track_position| front_most_covering_track_position.is_none_or(|covering| track_position < covering));
        if is_missed {
            frames_missing_a_picture += until - from;
        }

        let (mut layers, mut groups) = (Vec::new(), Vec::new());
        flatten(nodes, &mut layers, &mut groups);
        sections.push(PictureSection {
            first_timeline_frame: from,
            end_timeline_frame: until,
            layers,
            groups,
        });
    }
    (sections, frames_missing_a_picture)
}

/// Adds `section` after the others, joined to the one before it if they show the same things and
/// run on from one another.
fn push_merging_with_the_one_before(sections: &mut Vec<PictureSection>, section: PictureSection) {
    if let Some(previous) = sections.last_mut()
        && previous.end_timeline_frame == section.first_timeline_frame
        && previous.groups == section.groups
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
                (before.placement, &before.animation, &before.effects),
                (after.placement, &after.animation, &after.effects),
                "a clip is in one place, with the same keyframes and effects, for as long as it lasts"
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

mod group_nodes;
use group_nodes::{
    Entry, Node, drain_what_is_behind_what_covers, effects_of_the_groups_that_make_pictures, flatten,
    make_the_adjustments_groups, nodes_of,
};

#[cfg(test)]
mod adjustment_tests;
#[cfg(test)]
mod animation_tests;
#[cfg(test)]
mod conformance_tests;
#[cfg(test)]
mod effects_tests;
#[cfg(test)]
mod generator_tests;
#[cfg(test)]
mod group_effects_tests;
#[cfg(test)]
mod imported_tests;
#[cfg(test)]
mod operations_tests;
#[cfg(test)]
mod shader_tests;
#[cfg(test)]
mod state_tests;
#[cfg(test)]
mod tests;
