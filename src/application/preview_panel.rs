//! The live preview: the picture for the current frame, with the fast-forward symbol on it when
//! the current clip is sped up.
//!
//! The symbol can be moved and resized right on the preview. It is the same one symbol for the
//! whole project, so it is set once and shows in the same place in every sped-up clip. Nothing is
//! drawn for this unless the pointer is over the symbol: then an outline and a corner to grab
//! appear.

use egui::{Color32, CursorIcon, Id, Rect, Sense, Stroke, StrokeKind, Ui, WidgetInfo, WidgetType, pos2, vec2};

use super::commands::Command;
use super::textures::Textures;
use super::theme;
use crate::media::ThumbnailStrip;
use crate::playback::SuppliedPicture;
use crate::project::{SpeedFactor, SpeedIndicatorLayout};
use crate::speed_indicator::{SMALLEST_HEIGHT_IN_PIXELS, indicator_aspect_ratio, place_indicator};

/// The size of the corner that is grabbed to resize the symbol, in points.
const RESIZE_CORNER_SIZE: f32 = 14.0;

/// What the preview shows.
pub struct PreviewContent<'a> {
    /// The shape of the video's picture, if the project has a video.
    pub video_display_size: Option<(u32, u32)>,
    /// The picture for the current frame, if there is one to show.
    pub picture: Option<&'a SuppliedPicture>,
    /// The second whose thumbnail is shown, enlarged, when there is no picture to show yet.
    pub placeholder_thumbnail_second: Option<usize>,
    pub thumbnails: &'a ThumbnailStrip,
    /// The speed of the current clip, if it is sped up (so that the symbol is shown).
    pub accelerated_speed: Option<SpeedFactor>,
    pub indicator_layout: SpeedIndicatorLayout,
}

/// Fills the rest of the window with the preview.
pub fn show(ui: &mut Ui, content: &PreviewContent<'_>, textures: &mut Textures, commands: &mut Vec<Command>) {
    let panel_rectangle = ui.available_rect_before_wrap();
    ui.painter()
        .rect_filled(panel_rectangle, 0.0, theme::PREVIEW_BACKGROUND);

    let Some((video_width, video_height)) = content.video_display_size else {
        return;
    };
    let video_rectangle = fit_inside(panel_rectangle.shrink(theme::SPACING), video_width, video_height);
    draw_picture(ui, content, textures, video_rectangle);

    if let Some(speed_factor) = content.accelerated_speed {
        show_speed_indicator(
            ui,
            content.indicator_layout,
            speed_factor,
            video_rectangle,
            textures,
            commands,
        );
    }
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

fn draw_picture(ui: &Ui, content: &PreviewContent<'_>, textures: &mut Textures, video_rectangle: Rect) {
    let whole_picture = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    if let Some(picture) = content.picture {
        let texture = textures.preview_picture(ui.ctx(), picture);
        ui.painter()
            .image(texture.id(), video_rectangle, whole_picture, Color32::WHITE);
    } else if let Some(texture) = content
        .placeholder_thumbnail_second
        .and_then(|second| textures.thumbnail(ui.ctx(), content.thumbnails, second))
    {
        ui.painter()
            .image(texture.id(), video_rectangle, whole_picture, Color32::WHITE);
    } else {
        ui.painter().rect_filled(video_rectangle, 0.0, theme::TRACK_BACKGROUND);
    }
}

fn show_speed_indicator(
    ui: &mut Ui,
    layout: SpeedIndicatorLayout,
    speed_factor: SpeedFactor,
    video_rectangle: Rect,
    textures: &mut Textures,
    commands: &mut Vec<Command>,
) {
    let placement = place_indicator(
        layout,
        video_rectangle.width(),
        video_rectangle.height(),
        indicator_aspect_ratio(speed_factor),
    );
    let symbol_rectangle = Rect::from_min_size(
        video_rectangle.min + vec2(placement.left, placement.top),
        vec2(placement.width, placement.height),
    );

    let height_in_pixels =
        ((placement.height * ui.ctx().pixels_per_point()).round() as u32).max(SMALLEST_HEIGHT_IN_PIXELS);
    let texture = textures.symbol(ui.ctx(), speed_factor, height_in_pixels);
    ui.painter().image(
        texture.id(),
        symbol_rectangle,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );

    // Dragging the symbol moves it; dragging its corner resizes it. Neither can take the keyboard
    // focus, so the space bar always means "cut here".
    let drag_sense = Sense::CLICK | Sense::DRAG;
    let corner_rectangle = Rect::from_min_size(
        symbol_rectangle.right_bottom() - vec2(RESIZE_CORNER_SIZE, RESIZE_CORNER_SIZE),
        vec2(RESIZE_CORNER_SIZE, RESIZE_CORNER_SIZE),
    );
    let body = ui.interact(symbol_rectangle, Id::new("speed indicator body"), drag_sense);
    let corner = ui.interact(corner_rectangle, Id::new("speed indicator corner"), drag_sense);
    body.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, "Fast-forward symbol"));
    corner.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, "Fast-forward symbol size handle"));

    if corner.dragged() {
        // The corner follows the pointer: the symbol is as tall as the distance down to it.
        if let Some(pointer) = corner.interact_pointer_pos() {
            let new_height = (pointer.y - symbol_rectangle.top()).max(1.0);
            commands.push(Command::SetSpeedIndicatorLayout(SpeedIndicatorLayout::new(
                placement.left / video_rectangle.width(),
                placement.top / video_rectangle.height(),
                new_height / video_rectangle.height(),
            )));
        }
    } else if body.dragged() {
        let delta = body.drag_delta();
        commands.push(Command::SetSpeedIndicatorLayout(SpeedIndicatorLayout::new(
            (placement.left + delta.x) / video_rectangle.width(),
            (placement.top + delta.y) / video_rectangle.height(),
            layout.height_fraction_of_frame_height(),
        )));
    }

    let in_use = body.dragged() || corner.dragged();
    if body.hovered() || corner.hovered() || in_use {
        ui.painter().rect_stroke(
            symbol_rectangle,
            2.0,
            Stroke::new(1.5, theme::INDICATOR_OUTLINE),
            StrokeKind::Outside,
        );
        ui.painter()
            .rect_filled(corner_rectangle.shrink(3.0), 1.0, theme::INDICATOR_OUTLINE);
        let cursor = if corner.hovered() || corner.dragged() {
            CursorIcon::ResizeNwSe
        } else if body.dragged() {
            CursorIcon::Grabbing
        } else {
            CursorIcon::Grab
        };
        ui.ctx().set_cursor_icon(cursor);
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
