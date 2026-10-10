//! The playhead: a red line down the video track that shows the current frame, with a small red
//! triangle in the ruler above it that points down into the start of the line.

use egui::{Painter, Rect, Shape, Stroke, pos2, vec2};

use crate::application::theme;
use crate::application::timeline_geometry::TimelineGeometry;
use crate::project::TimelineFrameIndex;

/// How far inside the ends of the track the playhead line stays, in points. The line is two points
/// wide, so this keeps the whole of it on the track even on the very first frame, which starts
/// exactly at the track's left edge. (It moves the line by less than a sixth of a frame.)
const MARGIN_FROM_THE_ENDS: f32 = 1.0;

/// How wide and how tall the triangle in the ruler is, in points.
const MARKER_WIDTH: f32 = 12.0;
const MARKER_HEIGHT: f32 = 10.0;

/// How wide the line is, and how wide the dark outline that makes it stand out is. Both are even,
/// and the line sits exactly on a pixel boundary, so that the outline is the same on each side.
const LINE_WIDTH: f32 = 2.0;
const OUTLINE_WIDTH: f32 = 6.0;

/// Where the playhead goes when the current frame starts at `x`: there, but kept
/// [`MARGIN_FROM_THE_ENDS`] inside the track. `None` when `x` is further outside the track than
/// that (the current frame has been scrolled out of view).
pub(super) fn position(x: f32, track_left: f32, track_right: f32) -> Option<f32> {
    if x < track_left - MARGIN_FROM_THE_ENDS || x > track_right + MARGIN_FROM_THE_ENDS {
        return None;
    }
    Some(x.clamp(track_left + MARGIN_FROM_THE_ENDS, track_right - MARGIN_FROM_THE_ENDS))
}

/// Draws the playhead for `current_frame`, if that is in view.
pub(super) fn draw(
    painter: &Painter,
    geometry: TimelineGeometry,
    current_frame: TimelineFrameIndex,
    scroll_offset: f64,
    ruler_rectangle: Rect,
    track_rectangle: Rect,
) {
    let x = track_rectangle.left() + (geometry.x_of_frame(current_frame.value()) - scroll_offset) as f32;
    let Some(x) = position(x, track_rectangle.left(), track_rectangle.right()) else {
        return;
    };
    // The playhead may overhang the ends of the track by half the width of the triangle, but
    // nothing of it is drawn further out than that. (`with_clip_rect` could only make the area
    // smaller than the painter's own, which is the track's; this sets it.)
    let whole_timeline = ruler_rectangle.union(track_rectangle);
    let mut painter = painter.clone();
    painter.set_clip_rect(whole_timeline.expand2(vec2(MARKER_WIDTH / 2.0, 0.0)));

    let pixels_per_point = painter.ctx().pixels_per_point();
    let x = (x * pixels_per_point).round() / pixels_per_point;
    let line = [pos2(x, ruler_rectangle.bottom()), pos2(x, track_rectangle.bottom())];
    painter.line_segment(line, Stroke::new(OUTLINE_WIDTH, theme::PLAYHEAD_OUTLINE));
    painter.line_segment(line, Stroke::new(LINE_WIDTH, theme::PLAYHEAD));

    // The triangle sits in the ruler with its point exactly at the start of the line.
    painter.add(Shape::convex_polygon(
        vec![
            pos2(x - MARKER_WIDTH / 2.0, ruler_rectangle.bottom() - MARKER_HEIGHT),
            pos2(x + MARKER_WIDTH / 2.0, ruler_rectangle.bottom() - MARKER_HEIGHT),
            pos2(x, ruler_rectangle.bottom()),
        ],
        theme::PLAYHEAD,
        Stroke::new(1.5, theme::PLAYHEAD_OUTLINE),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_playhead_is_where_the_current_frame_is_when_that_is_well_inside_the_track() {
        assert_eq!(position(250.0, 16.0, 1264.0), Some(250.0));
        assert_eq!(position(17.0, 16.0, 1264.0), Some(17.0));
        assert_eq!(position(1263.0, 16.0, 1264.0), Some(1263.0));
    }

    #[test]
    fn the_playhead_on_the_first_frame_is_drawn_just_inside_the_left_edge_of_the_track() {
        assert_eq!(position(16.0, 16.0, 1264.0), Some(17.0));
    }

    #[test]
    fn the_playhead_is_kept_inside_the_right_end_of_the_track_too() {
        assert_eq!(position(1264.0, 16.0, 1264.0), Some(1263.0));
    }

    #[test]
    fn a_playhead_scrolled_out_of_view_is_not_drawn() {
        assert_eq!(position(-300.0, 16.0, 1264.0), None);
        assert_eq!(position(14.9, 16.0, 1264.0), None);
        assert_eq!(position(1265.1, 16.0, 1264.0), None);
        assert_eq!(position(5000.0, 16.0, 1264.0), None);
        // Just off the edge it is still drawn (moved onto the track), so that it does not blink out
        // at the very moment the strip scrolls by a point.
        assert_eq!(position(15.0, 16.0, 1264.0), Some(17.0));
    }
}
