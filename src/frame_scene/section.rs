// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the picture plan becomes what is drawn: the layers of a section of the plan as they are at a frame, and the symbol over them. **The window and the
//! export both make the [`PreviewContent`](super::PreviewContent) of a frame from these two functions**, which is what makes the picture of the export the
//! picture of the preview (EN-03) and not two sums that are meant to agree.
//!
//! What the two do not share is passed in: what shows where a video is (the window may have only a thumbnail of it yet, or nothing; the export always has the
//! picture) and the decoded picture of an image file.

use super::{PreviewGroup, PreviewLayer, PreviewLayerContent, SymbolOnThePreview, VideoInThePicture};
use crate::composition::{Layer, LayerContent, PictureSection, VisibleVideo, operations_of_effects};
use crate::engine::PictureColour;
use crate::project::{SourceMediaIdentifier, TimelineFrameIndex};
use crate::rgba_picture::RgbaPicture;

/// The layers of `section` as they are at `frame` (a frame of the section), the one at the back first: each where its keyframes say it is *at that frame*, with the
/// colours its effects make at that frame, and a text as its keyframes make it look at that frame.
///
/// `video` says what shows where a video layer is (it is asked once for each video layer, in their order), and `image` gives the decoded picture of an image
/// file, or `None` for a moment after it is opened, and what the codes of it stand for (it is asked once for each image layer).
pub fn layers_of_a_section<'a>(
    section: &'a PictureSection,
    frame: TimelineFrameIndex,
    mut video: impl FnMut(&Layer, &VisibleVideo) -> VideoInThePicture<'a>,
    mut image: impl FnMut(SourceMediaIdentifier) -> (Option<&'a RgbaPicture>, PictureColour),
) -> Vec<PreviewLayer<'a>> {
    section
        .layers
        .iter()
        .map(|layer| PreviewLayer {
            clip: layer.clip,
            track: layer.track,
            placement: layer.placement_at(frame),
            operations: layer.operations_at(frame),
            content: match &layer.content {
                LayerContent::Video(visible) => PreviewLayerContent::Video(video(layer, visible)),
                LayerContent::Image { source } => {
                    let (picture, colour) = image(*source);
                    PreviewLayerContent::Image {
                        source: *source,
                        picture,
                        colour,
                    }
                }
                LayerContent::Text(_) => {
                    PreviewLayerContent::Text(layer.text_at(frame).expect("a text layer has a text object"))
                }
                LayerContent::Generator => PreviewLayerContent::Generator,
                // (Never: an adjustment clip is a group of what is behind it, and is not a layer of a section.)
                LayerContent::Adjustment => unreachable!("an adjustment clip is not a layer of a section"),
            },
        })
        .collect()
}

/// The groups of `section` that make a picture of their own, as they are at `frame`: which layers they hold and what their effects do (the same at every frame).
pub fn groups_of_a_section(section: &PictureSection, frame: TimelineFrameIndex) -> Vec<PreviewGroup> {
    section
        .groups
        .iter()
        .map(|group| PreviewGroup {
            first_layer: group.first_layer,
            end_layer: group.end_layer,
            operations: operations_of_effects(&group.effects, frame),
            over_black: group.over_black,
        })
        .collect()
}

/// The fast-forward symbol of `section`: over the video that is in front, if that plays faster than normal.
pub fn symbol_of_a_section(section: &PictureSection) -> Option<SymbolOnThePreview> {
    section
        .front_video()
        .filter(|(_, video)| video.speed.is_accelerated())
        .map(|(layer, video)| SymbolOnThePreview {
            over: layer.clip,
            speed: video.speed,
        })
}

#[cfg(test)]
mod tests;
