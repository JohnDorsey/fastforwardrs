// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Cutting the sections of the picture into the stretches over which what is exported does not change, where layers have
//! keyframes.
//!
//! The picture plan's sections have the same layers throughout, but a layer that has keyframes is somewhere else, or (a text
//! object) looks like something else, at each of its frames. The filter graph is made of sections whose layers are each in
//! one place and are each one picture, so a section that has such a layer is cut here into the stretches over which every
//! layer is exported as it was in the stretch before: in the same place in whole pixels, and made of the same picture.
//! Each stretch has its layers as they are at its first frame, with no keyframes.
//!
//! A change that is slow is a few stretches, because the place it comes to is only different when it has moved a whole
//! pixel, and one that is fast is a stretch for each frame. That is not cheap: ffmpeg takes time to set up each section
//! of the filter graph, and more than that for each one when there are many (measured with ffmpeg 9.0.2, for sections of
//! one picture each that are only scaled and put on black: half a second for 100 sections, 1.5 seconds for 300, 6.7 for
//! 1000, 13 for 1500, 26 for 2000 and 71 for 3000). So an export that would need more than [`MOST_STRETCHES_OF_ANIMATION`]
//! of them is refused.

use super::ExportPlanError;
use super::filter_graph::PlaceToExport;
use super::plan::place_to_export;
use crate::colour_effects::ColourMatrix;
use crate::composition::{Layer, LayerContent, PictureSection};
use crate::project::{Placement, PropertyId, TimelineFrameIndex};
use crate::text_rendering::TextAppearance;

/// The most stretches that sections with keyframes may be cut into, all together. Past this, ffmpeg takes tens of seconds, and
/// then minutes, only to set up the filter graph.
pub const MOST_STRETCHES_OF_ANIMATION: usize = 1500;

/// The steps, in degrees, that a layer that is turned by keyframes is turned in. (A slow turn then does not make a stretch
/// for every frame, and an angle that is a tenth of a degree out moves the edge of a picture across the whole picture by less
/// than a pixel and a half.)
const TURN_STEP_IN_DEGREES: f32 = 0.1;

/// What the export makes of a layer that has keyframes, at a frame: when this is the same at two frames, so is the picture.
#[derive(Clone, Debug, PartialEq)]
struct WhatIsExported {
    place: PlaceToExport,
    /// How the effects of the layer change the colours of its picture.
    colour: ColourMatrix,
    /// What the picture of a text object is a picture of.
    text: Option<TextAppearance>,
}

/// The sections of `sections` with every layer that has keyframes worked out: a section that has some is cut into the stretches
/// over which none of them is exported differently (and each of them is as it is at the stretch's first frame), and the others
/// are as they were. `picture_size` is the size in pixels of the finished picture.
pub(super) fn with_keyframes_resolved(
    sections: &[PictureSection],
    picture_size: (u32, u32),
) -> Result<Vec<PictureSection>, ExportPlanError> {
    let mut resolved = Vec::with_capacity(sections.len());
    let mut stretches_made = 0;
    for section in sections {
        if !section.layers.iter().any(Layer::has_keyframes) {
            resolved.push(section.clone());
            continue;
        }
        let mut stretch_start = section.first_timeline_frame;
        let mut exported_at_the_start: Option<Vec<WhatIsExported>> = None;
        for frame in section.first_timeline_frame..=section.end_timeline_frame {
            // (One frame past the end, to close the last stretch.)
            let exported =
                (frame < section.end_timeline_frame).then(|| what_is_exported_at(section, frame, picture_size));
            if exported == exported_at_the_start {
                continue;
            }
            if exported_at_the_start.is_some() {
                stretches_made += 1;
                if stretches_made > MOST_STRETCHES_OF_ANIMATION {
                    return Err(ExportPlanError::TooMuchAnimation {
                        most: MOST_STRETCHES_OF_ANIMATION,
                    });
                }
                resolved.push(stretch_of(section, stretch_start, frame));
            }
            stretch_start = frame;
            exported_at_the_start = exported;
        }
    }
    Ok(resolved)
}

/// What every layer of `section` that has keyframes is exported as at `frame`, the one at the back first.
fn what_is_exported_at(section: &PictureSection, frame: u64, picture_size: (u32, u32)) -> Vec<WhatIsExported> {
    let frame = TimelineFrameIndex::new(frame);
    section
        .layers
        .iter()
        .filter(|layer| layer.has_keyframes())
        .map(|layer| {
            let as_it_is = layer_as_exported(layer, frame);
            WhatIsExported {
                place: place_to_export(as_it_is.placement, picture_size),
                colour: as_it_is.colour_matrix(),
                text: match &as_it_is.content {
                    LayerContent::Text(object) => Some(TextAppearance::of(object, picture_size.0, picture_size.1)),
                    LayerContent::Video(_) | LayerContent::Image { .. } => None,
                },
            }
        })
        .collect()
}

/// The part of `section` from `first` up to `end` that is exported as one thing: every layer that has keyframes is as it is at
/// `first`.
fn stretch_of(section: &PictureSection, first: u64, end: u64) -> PictureSection {
    let mut stretch = section.slice(first, end);
    for layer in &mut stretch.layers {
        if layer.has_keyframes() {
            *layer = layer_as_exported(layer, TimelineFrameIndex::new(first));
        }
    }
    stretch
}

/// `layer` as it is exported at `frame`: as it is there, with a turn that is made by keyframes made a whole number of steps, and
/// the values of its effects as the effects say they are exported (as near as is worth telling apart).
fn layer_as_exported(layer: &Layer, frame: TimelineFrameIndex) -> Layer {
    let mut as_it_is = layer.as_it_is_at(frame);
    for effect in &mut as_it_is.effects {
        effect.values = effect.kind.as_exported(&effect.values);
    }
    if layer.animation.is_animated(PropertyId::Rotation) {
        as_it_is.placement = Placement {
            rotation: as_it_is.placement.rotation.snapped_to(TURN_STEP_IN_DEGREES),
            ..as_it_is.placement
        };
    }
    as_it_is
}
