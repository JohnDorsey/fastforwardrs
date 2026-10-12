//! Things to make a preview from, for the tests of what is on it and what the pointer does to it.

use egui::{Rect, pos2, vec2};

use super::geometry::PictureOnScreen;
use super::{PreviewContent, PreviewLayer, PreviewLayerContent, VideoInThePicture};
use crate::project::{CanvasArea, ClipId, Placement, SpeedIndicatorLayout, TrackId};

/// A picture of 1600 by 900 pixels drawn 800 by 450 points, with its top left corner at (100, 50).
pub fn picture() -> PictureOnScreen {
    PictureOnScreen::new(Rect::from_min_size(pos2(100.0, 50.0), vec2(800.0, 450.0)), 1600, 900)
}

/// The clip that is number `number`: each is on a track of its own.
pub fn clip(number: u32) -> ClipId {
    ClipId::new(TrackId::new(number), 0)
}

/// A video, not yet shown, of clip number `number` that is where `placement` says.
pub fn layer(number: u32, placement: Placement) -> PreviewLayer<'static> {
    PreviewLayer {
        clip: clip(number),
        track: TrackId::new(number),
        placement,
        content: PreviewLayerContent::Video(VideoInThePicture::Waiting),
    }
}

/// A preview of these layers, the one at the back first, with no symbol, and `selected` selected.
pub fn content(layers: Vec<PreviewLayer<'static>>, selected: Option<u32>) -> PreviewContent<'static> {
    PreviewContent {
        picture_size: Some((1600, 900)),
        layers,
        symbol: None,
        indicator_layout: SpeedIndicatorLayout::DEFAULT,
        selected_clip: selected.map(clip),
    }
}

pub fn whole_picture() -> Placement {
    Placement::upright(CanvasArea::WHOLE_PICTURE)
}

/// A box in the middle of the picture, 400 by 225 points: from (300, 162.5) to (700, 387.5).
pub fn in_the_middle() -> Placement {
    Placement::upright(CanvasArea::new(0.25, 0.25, 0.5, 0.5))
}
