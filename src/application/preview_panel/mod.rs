// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The live preview: the finished picture for the current frame, which is every video and text
//! object that is seen, one over another, each where it is on the picture and turned as far as it
//! is turned, with the fast-forward symbol over the video that is in front when that is sped up.
//!
//! The symbol can be moved and resized right on the preview. It is the same one symbol for the
//! whole project, so it is set once and shows in the same place in every sped-up clip. Nothing is
//! drawn for this unless the pointer is over the symbol: then an outline and a corner to grab
//! appear.
//!
//! Videos, pictures and text objects can be moved by dragging them, and the selected one can be
//! resized by dragging any of its four corners and turned by dragging just outside a corner. What
//! the pointer can take hold of where is worked out in `grab`, what is done with it in `gestures`,
//! and what is drawn in `layers`.
//!
//! They can be put partly past the edge of the picture. What is past it is not seen (it is cut off,
//! as in the export), so only the selected one is taken hold of there; and the box of the selected one
//! (its outline, its corners, the places to turn it from) is drawn and taken hold of wherever it is over the preview panel,
//! outside the picture too, and nowhere else: it is not drawn over the other panels. The widgets that take the mouse are
//! made in `catchers`, and the names the parts of the preview are known by are in `names`.
//!
//! The picture can be zoomed out, to half the size it fits the panel at, and in, until its pixels can be seen, with the wheel
//! (see `zooming`), and moved about when it is bigger than the panel. Where it is drawn is worked out in `view`, from how it is
//! zoomed; the layers and what the mouse does to them are all in terms of where the picture is, so they are the same whatever
//! the zoom is. A button in the bottom right corner of the panel brings the picture back to where it fits, while it is not there.

mod catchers;
mod engine_view;
#[cfg(test)]
mod engine_view_tests;
mod geometry;
mod gestures;
mod grab;
mod layers;
mod names;
mod speed_indicator_overlay;
#[cfg(test)]
mod test_content;
mod view;
#[cfg(test)]
mod zoomed_pictures_tests;
mod zooming;

pub use view::PreviewView;
#[cfg(test)]
pub use zooming::FIT_BUTTON_LABEL;

// The names the layers and their handles are known by to screen readers, which the tests look them up by.
#[cfg(test)]
pub use names::{
    catcher_id, corner_handle_id, corner_handle_label, picture_label, text_object_label, turn_handle_label,
    turn_place_id, video_label,
};

use std::borrow::Cow;

use egui::{Rect, Ui, vec2};

use super::commands::Command;
use super::textures::Textures;
use crate::colour_effects::ColourMatrix;
use crate::media::ThumbnailLibrary;
use crate::playback::SuppliedPicture;
use crate::project::{
    ClipId, Placement, SourceMediaIdentifier, SpeedFactor, SpeedIndicatorLayout, TextObject, TrackId,
};
use crate::rgba_picture::RgbaPicture;
use crate::text_rendering::LazyTextRenderer;
use geometry::PictureOnScreen;
use speed_indicator_overlay::SymbolOnScreen;

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
    /// How the effects of the layer change the colours of its picture at the current frame, all of them together. (The picture that
    /// stands in for a video until its own arrives, a blurry thumbnail, has them changed too, so that the colours do not jump when
    /// the sharp picture comes.)
    pub colour: ColourMatrix,
    pub content: PreviewLayerContent<'a>,
}

impl PreviewLayer<'_> {
    fn is_text(&self) -> bool {
        matches!(self.content, PreviewLayerContent::Text(_))
    }

    /// Whether the layer is a video that is all of the picture, so that there is nothing outside it.
    fn fills_the_picture(&self) -> bool {
        matches!(self.content, PreviewLayerContent::Video(_))
            && self.placement.rotation.is_zero()
            && self.placement.area == crate::project::CanvasArea::WHOLE_PICTURE
    }

    /// The name the layer is known by to screen readers (and the tests).
    fn label(&self) -> String {
        match self.content {
            PreviewLayerContent::Text(_) => names::text_object_label(self.clip),
            PreviewLayerContent::Video(_) => names::video_label(self.clip),
            PreviewLayerContent::Image { .. } => names::picture_label(self.clip),
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

impl<'a> PreviewContent<'a> {
    /// The layer of `clip`, if it is on the preview.
    fn layer(&self, clip: ClipId) -> Option<&PreviewLayer<'a>> {
        self.layers.iter().find(|layer| layer.clip == clip)
    }

    /// The selected layer, if there is one and it is on the preview.
    fn selected_layer(&self) -> Option<&PreviewLayer<'a>> {
        self.selected_clip.and_then(|clip| self.layer(clip))
    }

    /// Where the symbol is on the screen, if it is shown.
    fn symbol_on_screen(&self, picture: &PictureOnScreen) -> Option<SymbolOnScreen> {
        self.symbol
            .map(|symbol| SymbolOnScreen::of(self.indicator_layout, symbol.speed, picture.rectangle))
    }
}

/// Fills the rest of the window with the preview, with the picture zoomed and moved as `view` says (which the wheel and the mouse
/// change).
pub fn show(
    ui: &mut Ui,
    content: &PreviewContent<'_>,
    view: &mut PreviewView,
    textures: &mut Textures,
    text_renderer: &mut LazyTextRenderer,
    commands: &mut Vec<Command>,
) {
    let panel_rectangle = ui.available_rect_before_wrap();

    let Some(picture_size) = content.picture_size else {
        return;
    };
    zooming::handle_the_wheel_and_the_mouse(ui, view, panel_rectangle, picture_size);
    let picture_rectangle = view.picture_rectangle(panel_rectangle, picture_size);
    // (What is drawn is cut off at the edge of the panel, as everything is.)
    ui.painter().rect_filled(picture_rectangle, 0.0, egui::Color32::BLACK);
    let filter = view.filter(panel_rectangle, picture_size, ui.ctx().pixels_per_point());
    layers::show(
        ui,
        content,
        PictureOnScreen::new(picture_rectangle, picture_size.0, picture_size.1).shown_as(filter),
        panel_rectangle,
        textures,
        text_renderer,
        commands,
    );
    zooming::show_the_button_that_makes_the_picture_fit(ui, view, panel_rectangle);
}

/// The largest rectangle of the shape `width` by `height` that fits inside `available`, centred.
fn fit_inside(available: Rect, width: u32, height: u32) -> Rect {
    let shape = width.max(1) as f32 / height.max(1) as f32;
    let (fitted_width, fitted_height) = if available.width() / available.height().max(1.0) > shape {
        (available.height() * shape, available.height())
    } else {
        (available.width(), available.width() / shape)
    };
    Rect::from_center_size(available.center(), vec2(fitted_width.max(1.0), fitted_height.max(1.0)))
}

#[cfg(test)]
mod tests {
    use egui::pos2;

    use super::*;

    fn rectangle(width: f32, height: f32) -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(width, height))
    }

    #[test]
    fn a_wide_video_in_a_tall_space_fills_the_width() {
        let fitted = fit_inside(rectangle(800.0, 800.0), 1600, 900);
        assert!((fitted.width() - 800.0).abs() < 1e-3);
        assert!((fitted.height() - 450.0).abs() < 1e-3);
        assert_eq!(fitted.center(), pos2(400.0, 400.0));
    }

    #[test]
    fn a_wide_video_in_a_wide_space_fills_the_height() {
        let fitted = fit_inside(rectangle(2000.0, 450.0), 1600, 900);
        assert!((fitted.height() - 450.0).abs() < 1e-3);
        assert!((fitted.width() - 800.0).abs() < 1e-3);
        assert_eq!(fitted.center(), pos2(1000.0, 225.0));
    }

    #[test]
    fn a_tall_video_is_fitted_by_its_height() {
        let fitted = fit_inside(rectangle(1000.0, 600.0), 1080, 1920);
        assert!((fitted.height() - 600.0).abs() < 1e-3);
        assert!((fitted.width() - 337.5).abs() < 1e-2);
    }

    #[test]
    fn the_fitted_rectangle_has_the_shape_of_the_video_and_stays_inside_the_space() {
        for (space_width, space_height) in [(800.0, 600.0), (300.0, 900.0), (1920.0, 200.0)] {
            for (width, height) in [(1920, 1080), (1080, 1920), (640, 640), (2560, 1080)] {
                let space = rectangle(space_width, space_height);
                let fitted = fit_inside(space, width, height);
                let video_shape = width as f32 / height as f32;
                assert!((fitted.width() / fitted.height() - video_shape).abs() < 0.01);
                assert!(fitted.width() <= space_width + 1e-3 && fitted.height() <= space_height + 1e-3);
            }
        }
    }

    #[test]
    fn a_space_with_no_room_still_gives_a_usable_rectangle() {
        let fitted = fit_inside(rectangle(0.0, 0.0), 1920, 1080);
        assert!(fitted.width() >= 1.0 && fitted.height() >= 1.0);
    }
}
