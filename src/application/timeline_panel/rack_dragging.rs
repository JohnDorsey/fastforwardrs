//! Dragging a rack of keyframes: where the pointer takes it.
//!
//! A drag is begun by pressing a rack (see the `racks` module, which notes what was taken hold of). From then on, in each
//! frame of the window until the button is let go, the rack is moved to where the pointer is: into the drawer (or the
//! keyframe crate) that it is over, or if it is over an empty stretch of a track, into a new crate that fills that stretch.
//! It goes along the timeline as far as the pointer does (as far as the drawer it is over has frames for), so that it can be
//! put where it is wanted. A rack is as long as the drawer it is in, so it cannot go where the first and the last of its
//! keyframes would have to be closer together than they are: over such a place the words "not enough space here" are
//! shown by the pointer, and the rack stays where it was last put.

use egui::{CursorIcon, Pos2, Ui};

use super::strip_view::StripView;
use super::{TimelineContent, TimelineViewState};
use crate::application::commands::Command;
use crate::project::{EditError, Project, RackDestination, Room, Track};

/// What is shown by the pointer when the rack cannot go where it is over because it is not long enough there for it.
pub const NOT_ENOUGH_SPACE_MESSAGE: &str = "not enough space here";

/// Where a rack that is let go at `pointer` goes: into the drawer (or crate) that the pointer is over, of those that are
/// open (`drop_targets`, the last of them being the one that is on top), or if it is over an empty stretch of a track (in
/// `rows`) into a new crate that fills it, as far as the finished video lasts. `None` if it is over nothing a rack can go
/// to.
pub(super) fn destination_under(
    pointer: Pos2,
    drop_targets: &[super::racks::DropTarget],
    rows: &[(&Track, StripView)],
    project: &Project,
) -> Option<RackDestination> {
    if let Some(target) = drop_targets
        .iter()
        .rev()
        .find(|target| target.rectangle.contains(pointer))
    {
        return Some(target.destination);
    }
    let (track, view) = rows.iter().find(|(_, view)| view.rectangle.contains(pointer))?;
    let frame = view.frame_at(pointer.x)?;
    match track.room_at(frame) {
        Room::Free {
            from_frame,
            until_frame,
        } => {
            let end = until_frame.unwrap_or_else(|| project.total_frame_count());
            (from_frame < end).then_some(RackDestination::NewCrate {
                track: track.id(),
                start: from_frame,
                length: end - from_frame,
            })
        }
        Room::Occupied(_) => None,
    }
}

/// If a rack is being dragged, adds the command that moves it towards the pointer, as far as that can be done, and says by
/// the pointer when it cannot go where it is over because there is not room.
pub(super) fn keep_dragging(
    ui: &Ui,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    rows: &[(&Track, StripView)],
    commands: &mut Vec<Command>,
) {
    let Some(drag) = state.rack_drag else {
        return;
    };
    // A drag ends with the button.
    if !content.can_edit || !ui.input(|input| input.pointer.primary_down()) {
        state.rack_drag = None;
        return;
    }
    let project = content.project;
    let (Some(located), Some(pointer), Some((_, view))) = (
        project.located_rack(drag.rack),
        ui.input(|input| input.pointer.interact_pos()),
        rows.first(),
    ) else {
        return;
    };
    let Some(first_frame_now) = located.rack.first_frame().map(|frame| located.timeline_frame_of(frame)) else {
        return;
    };
    // Where its first keyframe is wanted: as far from where it was when it was taken hold of as the pointer has gone.
    let frames_the_pointer_has_gone =
        view.frame_at(pointer.x).map_or(0, |frame| frame.value()) as i64 - drag.pointer_frame_at_the_start as i64;
    let wanted_shift =
        drag.first_keyframe_frame_at_the_start as i64 + frames_the_pointer_has_gone - first_frame_now as i64;

    let Some(destination) = destination_under(pointer, &state.drop_targets, rows, project) else {
        ui.ctx().set_cursor_icon(CursorIcon::NotAllowed);
        return;
    };
    match project.shift_that_fits(drag.rack, destination, wanted_shift) {
        Ok(shift) => {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            let is_in_it_already = destination == RackDestination::Drawer(located.owner);
            if !is_in_it_already || shift != 0 {
                commands.push(Command::MoveRack {
                    rack: drag.rack,
                    to: destination,
                    shift_in_frames: shift,
                });
            }
        }
        Err(EditError::NoRoomForTheKeyframes) => {
            ui.ctx().set_cursor_icon(CursorIcon::NotAllowed);
            state.pointer_message = Some(NOT_ENOUGH_SPACE_MESSAGE.to_owned());
        }
        Err(_) => ui.ctx().set_cursor_icon(CursorIcon::NotAllowed),
    }
}
