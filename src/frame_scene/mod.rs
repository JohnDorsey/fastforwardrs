// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a frame of the finished picture shows, and what the picture engine is asked to draw for it. **The preview and the export both use this**, so that they
//! are the same picture by construction (EN-03): the window builds a [`PreviewContent`] for the frame it shows, the export builds one for each frame it makes, and
//! [`describe_frame`] turns either into the engine's `FrameDescription`.
//!
//! (The types are called after the preview because that is where they began; a frame of the export is the picture that the preview shows.)
//!
//! A [`PreviewContent`] is the layers that are seen at a frame, the one at the back first, each with where it is, how its colours are changed and what its picture
//! is made of, and the fast-forward symbol over the front-most video when that is sped up. [`layers_of_a_section`] and [`symbol_of_a_section`] are where the picture
//! plan becomes these (for the window and for the export alike), and [`describe_frame`] **sends nothing itself**: for each picture that a layer is made of it asks a
//! [`PictureSupply`] to make sure the engine has it.

mod colour;
mod describe;
mod section;

pub use colour::{colour_of_a_picture_file, colour_of_the_export, colour_of_the_preview, working_space_of};
pub use describe::{HowToMakeIt, PictureSupply, PictureWanted, SentPictures, TextPictures, describe_frame};
pub use section::{layers_of_a_section, symbol_of_a_section};

use std::borrow::Cow;

use crate::colour_effects::ColourMatrix;
use crate::engine::PictureColour;
use crate::media::ThumbnailLibrary;
use crate::playback::SuppliedPicture;
use crate::project::{
    ClipId, PictureOperation, Placement, SourceMediaIdentifier, SpeedFactor, SpeedIndicatorLayout, TextObject, TrackId,
};
use crate::rgba_picture::RgbaPicture;

/// What shows where a video is in the picture.
pub enum VideoInThePicture<'a> {
    /// The picture of the video for the current frame.
    Picture(&'a SuppliedPicture),
    /// A thumbnail of the video, enlarged, stands in until the picture arrives.
    Thumbnail {
        source: SourceMediaIdentifier,
        second: usize,
        library: &'a ThumbnailLibrary,
    },
    /// There is a video, and nothing of it to show yet.
    Waiting,
}

/// What a layer of the picture is made of.
pub enum PreviewLayerContent<'a> {
    Video(VideoInThePicture<'a>),
    /// A picture from an image file, which is not decoded yet (`None`) for a moment after it is opened.
    Image {
        source: SourceMediaIdentifier,
        picture: Option<&'a RgbaPicture>,
        /// What the codes of the red, green and blue of the picture stand for: sRGB unless the user says that the file is another (see [`colour_of_a_picture_file`]).
        colour: PictureColour,
    },
    /// A text object as it is at the current frame: what the clip says, or, if it has keyframes, as they change it.
    Text(Cow<'a, TextObject>),
}

/// A video, a picture or a text object that is seen, and where it is on the picture.
pub struct PreviewLayer<'a> {
    pub clip: ClipId,
    /// The track the clip is on.
    pub track: TrackId,
    /// Where the layer is at the current frame: where the clip says, or, if it has keyframes, as they change that.
    pub placement: Placement,
    /// What the effects of the layer do to its picture at the current frame, one after another (see [`Layer::operations_at`](crate::composition::Layer::operations_at)).
    /// The old drawing does the colour mixes of them and leaves the others out ([`Self::colour`]).
    pub operations: Vec<PictureOperation>,
    pub content: PreviewLayerContent<'a>,
}

impl PreviewLayer<'_> {
    /// How the colour mixes among the operations of the layer change the colours of its picture, all of them together. (The picture that stands in for a video until
    /// its own arrives, a blurry thumbnail, has them changed too, so that the colours do not jump when the sharp picture comes.)
    pub fn colour(&self) -> ColourMatrix {
        self.operations
            .iter()
            .filter_map(|operation| match operation {
                PictureOperation::Mix(matrix) => Some(*matrix),
                PictureOperation::Exposure { .. }
                | PictureOperation::Opacity { .. }
                | PictureOperation::Blur { .. }
                | PictureOperation::Crop { .. } => None,
            })
            .fold(ColourMatrix::NOTHING_CHANGES, ColourMatrix::then)
    }

    /// The operations of a layer that has no effect but a mix of colours, which it leaves out if it changes nothing. (What the tests make layers of.)
    #[cfg(test)]
    pub fn operations_of_a_colour(colour: ColourMatrix) -> Vec<PictureOperation> {
        if colour.changes_nothing() {
            Vec::new()
        } else {
            vec![PictureOperation::Mix(colour)]
        }
    }
}

/// The fast-forward symbol: shown over the layer of this clip, which is the front-most video.
#[derive(Clone, Copy)]
pub struct SymbolOnThePreview {
    pub over: ClipId,
    pub speed: SpeedFactor,
}

/// What the preview shows.
pub struct PreviewContent<'a> {
    /// The size of the finished picture, in pixels, or `None` while there is nothing to show.
    pub picture_size: Option<(u32, u32)>,
    /// What is over the black picture now, the layer at the back first.
    pub layers: Vec<PreviewLayer<'a>>,
    /// The symbol of the video that is in front, if that is sped up.
    pub symbol: Option<SymbolOnThePreview>,
    pub indicator_layout: SpeedIndicatorLayout,
    pub selected_clip: Option<ClipId>,
}
