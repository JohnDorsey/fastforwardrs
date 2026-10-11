//! The live preview: the finished picture for the current frame, which is the video that is in
//! front, with the fast-forward symbol on it when that video is sped up, and the text objects that
//! are in front of the video.
//!
//! The symbol can be moved and resized right on the preview. It is the same one symbol for the
//! whole project, so it is set once and shows in the same place in every sped-up clip. Nothing is
//! drawn for this unless the pointer is over the symbol: then an outline and a corner to grab
//! appear.
//!
//! Text objects can be moved by dragging them, and the selected one can be resized by dragging
//! any of its four corners; see the `text_overlay` module.

mod speed_indicator_overlay;
mod text_overlay;

// The names the text objects and their handles are known by to screen readers, which the tests look them up by.
#[cfg(test)]
pub use text_overlay::{corner_handle_label, text_object_label};

use egui::{Color32, Rect, Ui, pos2, vec2};

use super::commands::Command;
use super::textures::Textures;
use super::theme;
use crate::composition::VisibleText;
use crate::media::ThumbnailStrip;
use crate::playback::SuppliedPicture;
use crate::project::{ClipLocation, SourceMediaIdentifier, SpeedFactor, SpeedIndicatorLayout};
use crate::text_rendering::LazyTextRenderer;

/// What shows where the video is in the picture.
pub enum VideoInThePicture<'a> {
    /// No video is in front at this frame: the picture is black.
    None,
    /// The picture of the video for the current frame.
    Picture(&'a SuppliedPicture),
    /// A thumbnail of the video, enlarged, stands in until the picture arrives.
    Thumbnail {
        source: SourceMediaIdentifier,
        second: usize,
        strip: &'a ThumbnailStrip,
    },
    /// There is a video, and nothing of it to show yet.
    Waiting,
}

/// What the preview shows.
pub struct PreviewContent<'a> {
    /// The size of the finished picture, in pixels, or `None` while there is nothing to show.
    pub picture_size: Option<(u32, u32)>,
    pub video: VideoInThePicture<'a>,
    /// The speed of the video that is shown, if it is sped up (so that the symbol is shown).
    pub accelerated_speed: Option<SpeedFactor>,
    pub indicator_layout: SpeedIndicatorLayout,
    /// The text objects that are over the picture now, the one at the back first.
    pub texts: &'a [VisibleText],
    pub selected_clip: Option<ClipLocation>,
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
    draw_video(ui, content, textures, picture_rectangle);

    if let Some(speed_factor) = content.accelerated_speed {
        speed_indicator_overlay::show(
            ui,
            content.indicator_layout,
            speed_factor,
            picture_rectangle,
            textures,
            commands,
        );
    }
    text_overlay::show(
        ui,
        text_overlay::TextOverlayContent {
            texts: content.texts,
            selected_clip: content.selected_clip,
            picture_size_in_pixels: (picture_width, picture_height),
            picture_rectangle,
        },
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

/// Draws the black picture, with the video on it.
fn draw_video(ui: &Ui, content: &PreviewContent<'_>, textures: &mut Textures, picture_rectangle: Rect) {
    let whole_picture = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    match &content.video {
        VideoInThePicture::None => {
            ui.painter().rect_filled(picture_rectangle, 0.0, Color32::BLACK);
        }
        VideoInThePicture::Picture(picture) => {
            // The picture is already the shape of the finished picture, with black bars if the video is another shape.
            let texture = textures.preview_picture(ui.ctx(), picture);
            ui.painter()
                .image(texture.id(), picture_rectangle, whole_picture, Color32::WHITE);
        }
        VideoInThePicture::Thumbnail { source, second, strip } => {
            ui.painter().rect_filled(picture_rectangle, 0.0, Color32::BLACK);
            if let Some(texture) = textures.thumbnail(ui.ctx(), *source, strip, *second) {
                // A thumbnail has the shape of its video, which is fitted inside the picture as the video is.
                let size = texture.size();
                let fitted = fit_inside(picture_rectangle, size[0] as u32, size[1] as u32);
                ui.painter().image(texture.id(), fitted, whole_picture, Color32::WHITE);
            }
        }
        VideoInThePicture::Waiting => {
            ui.painter()
                .rect_filled(picture_rectangle, 0.0, theme::TRACK_BACKGROUND);
        }
    }
}

#[cfg(test)]
mod tests {
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
