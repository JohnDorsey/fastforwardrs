//! Drawing the clips of the video track: a strip of pictures taken from the video, one for every
//! second of the finished video, side by side with no gaps, where each clip has its own corners
//! rounded a little.

use egui::epaint::RectShape;
use egui::{Color32, CornerRadius, Painter, Rect, Stroke, StrokeKind, Ui, pos2, vec2};

use super::TimelineContent;
use crate::application::textures::Textures;
use crate::application::theme;
use crate::application::timeline_geometry::{TileLayout, TimelineGeometry};
use crate::media::thumbnail_second_for_source_frame;
use crate::project::VideoTrack;

/// The part of a picture a tile shows: from the left edge to this fraction of the picture's width.
fn picture_part_up_to(fraction_of_width: f32) -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(fraction_of_width, 1.0))
}

/// Which corners of a tile are rounded: those that are corners of its clip. The first tile of a
/// clip has the clip's left corners and the last tile has its right corners; a tile in between has
/// none, so the clip's picture strip stays seamless.
pub(super) fn corner_radius_of_tile(tile: &TileLayout, radius: u8) -> CornerRadius {
    let left = if tile.starts_clip { radius } else { 0 };
    let right = if tile.ends_clip { radius } else { 0 };
    CornerRadius {
        nw: left,
        sw: left,
        ne: right,
        se: right,
    }
}

/// Fills the strip's background, which shows through at the rounded corners.
pub(super) fn draw_background(painter: &Painter, track_rectangle: Rect) {
    painter.rect_filled(
        track_rectangle,
        CornerRadius::same(theme::CLIP_CORNER_RADIUS),
        theme::TRACK_BACKGROUND,
    );
}

/// Draws the line around the strip.
pub(super) fn draw_border(painter: &Painter, track_rectangle: Rect) {
    painter.rect_stroke(
        track_rectangle,
        CornerRadius::same(theme::CLIP_CORNER_RADIUS),
        Stroke::new(1.0, theme::TRACK_BORDER),
        StrokeKind::Inside,
    );
}

/// Draws the pictures of the clips that are in view.
pub(super) fn draw_tiles(
    ui: &Ui,
    painter: &Painter,
    content: &TimelineContent<'_>,
    geometry: TimelineGeometry,
    scroll_offset: f64,
    textures: &mut Textures,
    track_rectangle: Rect,
) {
    let view_width = f64::from(track_rectangle.width());
    for tile in geometry.visible_tiles(content.project.video_track(), scroll_offset, view_width) {
        let left = track_rectangle.left() + (tile.left - scroll_offset) as f32;
        let tile_rectangle = Rect::from_min_size(
            pos2(left, track_rectangle.top()),
            vec2(tile.width as f32, theme::TRACK_HEIGHT),
        );
        let corner_radius = corner_radius_of_tile(&tile, theme::CLIP_CORNER_RADIUS);
        let second = thumbnail_second_for_source_frame(geometry.frame_rate(), tile.source_frame);
        let shape = match textures.thumbnail(ui.ctx(), content.thumbnails, second) {
            Some(texture) => RectShape::filled(tile_rectangle, corner_radius, Color32::WHITE)
                .with_texture(texture.id(), picture_part_up_to(tile.fraction_of_picture_shown)),
            None => RectShape::filled(tile_rectangle, corner_radius, theme::THUMBNAIL_PLACEHOLDER),
        };
        painter.add(shape);
    }
}

/// Draws a thin line where each clip after the first begins, light beside dark so that it shows
/// up against any picture. The lines stop short of the top and bottom by the radius of the
/// corners, so that they do not cover the notches where two clips' rounded corners meet.
pub(super) fn draw_clip_boundaries(
    painter: &Painter,
    geometry: TimelineGeometry,
    track: &VideoTrack,
    scroll_offset: f64,
    track_rectangle: Rect,
) {
    for boundary in geometry.clip_boundaries(track) {
        let x = track_rectangle.left() + (boundary - scroll_offset) as f32;
        if x < track_rectangle.left() - 2.0 || x > track_rectangle.right() + 2.0 {
            continue;
        }
        let top = track_rectangle.top() + f32::from(theme::CLIP_CORNER_RADIUS);
        let bottom = track_rectangle.bottom() - f32::from(theme::CLIP_CORNER_RADIUS);
        painter.line_segment(
            [pos2(x - 1.0, top), pos2(x - 1.0, bottom)],
            Stroke::new(1.0, theme::CLIP_BOUNDARY_LIGHT),
        );
        painter.line_segment(
            [pos2(x + 0.5, top), pos2(x + 0.5, bottom)],
            Stroke::new(2.0, theme::CLIP_BOUNDARY_DARK),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::SourceFrameIndex;

    fn tile(starts_clip: bool, ends_clip: bool) -> TileLayout {
        TileLayout {
            clip_index: 0,
            left: 0.0,
            width: 100.0,
            fraction_of_picture_shown: 1.0,
            source_frame: SourceFrameIndex::new(0),
            starts_clip,
            ends_clip,
        }
    }

    #[test]
    fn the_first_tile_of_a_clip_rounds_the_left_corners_only() {
        assert_eq!(
            corner_radius_of_tile(&tile(true, false), 4),
            CornerRadius {
                nw: 4,
                sw: 4,
                ne: 0,
                se: 0
            }
        );
    }

    #[test]
    fn the_last_tile_of_a_clip_rounds_the_right_corners_only() {
        assert_eq!(
            corner_radius_of_tile(&tile(false, true), 4),
            CornerRadius {
                nw: 0,
                sw: 0,
                ne: 4,
                se: 4
            }
        );
    }

    #[test]
    fn a_tile_in_the_middle_of_a_clip_has_square_corners_so_the_strip_is_seamless() {
        assert_eq!(corner_radius_of_tile(&tile(false, false), 4), CornerRadius::ZERO);
    }

    #[test]
    fn a_clip_of_one_tile_has_all_four_corners_rounded() {
        assert_eq!(corner_radius_of_tile(&tile(true, true), 4), CornerRadius::same(4));
    }
}
