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

mod geometry;
mod gestures;
mod grab;
mod layers;
mod speed_indicator_overlay;
#[cfg(test)]
mod test_content;

// The names the layers and their handles are known by to screen readers, which the tests look them up by.
#[cfg(test)]
pub use layers::{catcher_id, corner_handle_label, picture_label, text_object_label, turn_handle_label, video_label};

use egui::{Rect, Ui, vec2};

use super::commands::Command;
use super::textures::Textures;
use super::theme;
use crate::media::ThumbnailLibrary;
use crate::playback::SuppliedPicture;
use crate::project::{ClipLocation, Placement, SourceMediaIdentifier, SpeedFactor, SpeedIndicatorLayout, TextObject};
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
    Text(&'a TextObject),
}

/// A video, a picture or a text object that is seen, and where it is on the picture.
pub struct PreviewLayer<'a> {
    pub clip: ClipLocation,
    pub placement: Placement,
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
            PreviewLayerContent::Text(_) => layers::text_object_label(self.clip),
            PreviewLayerContent::Video(_) => layers::video_label(self.clip),
            PreviewLayerContent::Image { .. } => layers::picture_label(self.clip),
        }
    }
}

/// The fast-forward symbol: shown over the layer of this clip, which is the front-most video.
#[derive(Clone, Copy)]
pub struct SymbolOnThePreview {
    pub over: ClipLocation,
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
    pub selected_clip: Option<ClipLocation>,
}

impl<'a> PreviewContent<'a> {
    /// The layer of `clip`, if it is on the preview.
    fn layer(&self, clip: ClipLocation) -> Option<&PreviewLayer<'a>> {
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

/// Fills the rest of the window with the preview.
pub fn show(
    ui: &mut Ui,
    content: &PreviewContent<'_>,
    textures: &mut Textures,
    text_renderer: &mut LazyTextRenderer,
    commands: &mut Vec<Command>,
) {
    let panel_rectangle = ui.available_rect_before_wrap();
    ui.painter()
        .rect_filled(panel_rectangle, 0.0, theme::PREVIEW_BACKGROUND);

    let Some((picture_width, picture_height)) = content.picture_size else {
        return;
    };
    let picture_rectangle = fit_inside(panel_rectangle.shrink(theme::SPACING), picture_width, picture_height);
    ui.painter().rect_filled(picture_rectangle, 0.0, egui::Color32::BLACK);
    layers::show(
        ui,
        content,
        PictureOnScreen::new(picture_rectangle, picture_width, picture_height),
        panel_rectangle,
        textures,
        text_renderer,
        commands,
    );
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
