//! Making a track taller or shorter by dragging the boundary under its title box.
//!
//! The boundary is the gap between the title box of a track and the one under it. (Under the last
//! track there is a gap too: the rows leave the same room after their last one.) The zone that takes
//! hold of the mouse reaches a little way into the box above the gap and the one below it, so that
//! it is not too thin to aim at.

use egui::{CornerRadius, CursorIcon, PointerButton, Rect, Sense, Ui, pos2, vec2};

use super::view_state::{TimelineViewState, TrackResize};
use crate::application::commands::Command;
use crate::application::theme;
use crate::project::Track;

/// How far the zone that takes hold of the mouse reaches into the title boxes on both sides of the
/// gap, in points.
pub(super) const REACH_INTO_THE_BOXES: f32 = 2.0;

/// How tall the line is that shows which boundary the pointer has hold of, in points.
const LINE_HEIGHT: f32 = 2.0;

/// The zone of the boundary under the title box in `header_rectangle`.
fn zone_under(header_rectangle: Rect) -> Rect {
    Rect::from_min_max(
        pos2(
            header_rectangle.left(),
            header_rectangle.bottom() - REACH_INTO_THE_BOXES,
        ),
        pos2(
            header_rectangle.right(),
            header_rectangle.bottom() + theme::SPACE_BETWEEN_TRACKS + REACH_INTO_THE_BOXES,
        ),
    )
}

/// Takes care of the boundary under the title box of `track`, which is in `header_rectangle`: shows
/// that it can be dragged, and while it is dragged adds a command that gives the track the height
/// that the pointer has gone to. `rows_area_height` is how tall the rows are together now.
///
/// The height follows how far the pointer has gone since it took hold, and not where it is in
/// relation to the track, because what is drawn moves while the track changes size.
pub(super) fn show(
    ui: &mut Ui,
    header_rectangle: Rect,
    track: &Track,
    can_edit: bool,
    rows_area_height: f32,
    state: &mut TimelineViewState,
    commands: &mut Vec<Command>,
) {
    let zone = zone_under(header_rectangle);
    let response = ui.interact(
        zone,
        ui.id().with(("track boundary", track.id())),
        if can_edit { Sense::DRAG } else { Sense::hover() },
    );
    if !can_edit {
        return;
    }
    if response.drag_started_by(PointerButton::Primary)
        && let Some(pointer) = response.interact_pointer_pos()
    {
        state.track_resize = Some(TrackResize {
            track: track.id(),
            height_at_the_start: header_rectangle.height(),
            pointer_y_at_the_start: pointer.y,
            rows_area_height,
        });
    }
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
        let middle_of_the_gap = header_rectangle.bottom() + theme::SPACE_BETWEEN_TRACKS / 2.0;
        ui.painter().rect_filled(
            Rect::from_center_size(
                pos2(header_rectangle.center().x, middle_of_the_gap),
                vec2(header_rectangle.width(), LINE_HEIGHT),
            ),
            CornerRadius::same(1),
            theme::ICON_BUTTON_PRESSED,
        );
    }
    if response.dragged_by(PointerButton::Primary)
        && let Some(resize) = state.track_resize.filter(|resize| resize.track == track.id())
        && let Some(pointer) = ui.input(|input| input.pointer.interact_pos())
    {
        // (The track is given the height that is drawn as tall as the pointer has made it, with the rows zoomed as they are.)
        let height = state
            .vertical_zoom
            .height_that_is_drawn_as(resize.height_at_the_start + (pointer.y - resize.pointer_y_at_the_start));
        if height != track.height() {
            commands.push(Command::SetTrackHeight {
                track: track.id(),
                height,
            });
        }
    }
    if response.drag_stopped() {
        state.track_resize = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_zone_is_the_gap_under_the_box_and_a_little_way_into_the_boxes_either_side() {
        let header = Rect::from_min_size(pos2(10.0, 100.0), vec2(150.0, 104.0));
        let zone = zone_under(header);
        assert_eq!(zone.left(), 10.0);
        assert_eq!(zone.right(), 160.0);
        assert_eq!(zone.top(), 204.0 - REACH_INTO_THE_BOXES);
        assert_eq!(
            zone.bottom(),
            204.0 + theme::SPACE_BETWEEN_TRACKS + REACH_INTO_THE_BOXES,
            "the same way into the next box"
        );
        assert!(zone.height() > theme::SPACE_BETWEEN_TRACKS, "taller than the gap");
    }
}
