//! What the mouse does to the preview: a click picks a layer, dragging a layer moves it, dragging a corner of
//! the selected layer resizes it, dragging round from just outside a corner turns it, and the fast-forward
//! symbol is moved and resized the same way.
//!
//! The widgets that take every click and drag on the preview (the "catchers") are made once for the panel
//! and once for each place of the selected layer that can be taken hold of, and what each press is about is
//! worked out from where it began, with [`grab_at`], when it comes. That is not the same as giving every
//! layer a widget of its own that is only sensitive when it is on top of what is under the pointer: egui
//! decides who gets a press at the start of a frame from the widgets of the frame before, so a click that
//! came in the same frame as the move of the pointer that brought it there (a tap on a touch screen, or a
//! quick click) would be given to what the pointer was over a frame earlier, or to nothing.

use egui::{Align2, CursorIcon, FontId, Id, Painter, PointerButton, Pos2, Stroke, Ui, vec2};

use super::catchers::Catchers;
use super::geometry::{PictureOnScreen, TURN_ZONE_RADIUS, cursor_for_resizing_at};
use super::grab::{Grab, grab_at};
use super::{PreviewContent, PreviewLayer};
use crate::application::commands::Command;
use crate::application::selection::SelectionChange;
use crate::application::theme;
use crate::project::{Angle, ClipId, Placement};

/// A layer that is turned with Shift held is turned to a whole number of these many degrees.
const SNAP_STEP_DEGREES: f32 = 15.0;

/// How far the pointer has to go for a change of where a layer is to count as a change, in fractions
/// of the picture, or of a turn in degrees. A pointer that is held still can still make the same
/// place come out a hair different from where it was, because of rounding, and that is not a change
/// worth saving.
const SMALLEST_CHANGE_OF_AREA: f32 = 1e-5;
const SMALLEST_CHANGE_OF_ANGLE_IN_DEGREES: f32 = 1e-3;

/// What a drag took hold of when it began, which is kept until it ends.
#[derive(Clone, Copy)]
struct Gesture {
    grab: Grab,
    /// For turning a layer: where the turn began.
    turn: Option<TurnBegun>,
}

/// What turning a layer remembers of where it began: which way the pointer was from the middle of the
/// layer, and how far the layer was turned. (How far the pointer has gone round since is added to it.)
#[derive(Clone, Copy)]
struct TurnBegun {
    direction_of_the_pointer: Angle,
    rotation_of_the_layer: Angle,
}

/// What the pointer is doing on the preview in one frame.
#[derive(Clone, Copy, Debug)]
pub(super) struct Pointing {
    /// What the pointer has hold of, if it is dragging something, and otherwise what it is over.
    pub grab: Option<Grab>,
    /// Whether the pointer has hold of it.
    pub is_dragging: bool,
    /// While a layer is being turned: where the pointer is, and how far round the layer is turned.
    turning: Option<(Pos2, Angle)>,
}

/// The command that makes `clip` the only clip that is selected.
fn select_only(clip: ClipId) -> Command {
    Command::Select {
        clips: vec![clip],
        change: SelectionChange::Replace,
    }
}

/// Does what the mouse is doing to the preview in this frame, by adding commands, and says what the
/// pointer is on.
pub(super) fn handle(
    ui: &Ui,
    catchers: &Catchers,
    content: &PreviewContent<'_>,
    picture: &PictureOnScreen,
    commands: &mut Vec<Command>,
) -> Pointing {
    let gesture_id = Id::new("what the preview is being dragged by");
    if catchers.drag_started_by(PointerButton::Primary) {
        let begun = ui
            .input(|input| input.pointer.press_origin())
            .and_then(|origin| begin(content, picture, origin, commands));
        ui.data_mut(|data| data.insert_temp(gesture_id, begun));
    }

    let is_dragging = catchers.dragged_by(PointerButton::Primary);
    let gesture = if is_dragging {
        ui.data(|data| data.get_temp::<Option<Gesture>>(gesture_id)).flatten()
    } else {
        ui.data_mut(|data| data.remove_temp::<Option<Gesture>>(gesture_id));
        None
    };

    let mut turning = None;
    if let Some(gesture) = gesture {
        turning = carry_on(ui, catchers, gesture, content, picture, commands);
    } else if catchers.clicked_by(PointerButton::Primary) {
        click(catchers, content, picture, commands);
    }

    let grab = if is_dragging {
        gesture.map(|gesture| gesture.grab)
    } else if catchers.hovered() {
        ui.input(|input| input.pointer.hover_pos())
            .and_then(|point| grab_at(content, picture, point))
    } else {
        None
    };
    Pointing {
        grab,
        is_dragging: gesture.is_some(),
        turning,
    }
}

/// What a drag that begins at `origin` takes hold of: nothing, if there is nothing there. Taking hold
/// of a layer that is not the selected one picks it.
fn begin(
    content: &PreviewContent<'_>,
    picture: &PictureOnScreen,
    origin: Pos2,
    commands: &mut Vec<Command>,
) -> Option<Gesture> {
    let grab = grab_at(content, picture, origin)?;
    let mut turn = None;
    match grab {
        Grab::Layer(clip) => {
            if content.selected_clip != Some(clip) {
                commands.push(select_only(clip));
            }
        }
        Grab::PlaceToTurnFrom { layer, .. } => {
            let layer = content.layer(layer)?;
            turn = Some(TurnBegun {
                direction_of_the_pointer: layer
                    .placement
                    .direction_to(picture.fractions_at(origin), picture.size_in_pixels),
                rotation_of_the_layer: layer.placement.rotation,
            });
        }
        Grab::Handle { .. } | Grab::Symbol | Grab::SymbolCorner => {}
    }
    Some(Gesture { grab, turn })
}

/// Carries on what a drag took hold of, by adding the command that puts it where the pointer has taken
/// it. When it is a layer that is being turned, this gives where the pointer is and how far round the
/// layer is turned.
fn carry_on(
    ui: &Ui,
    catchers: &Catchers,
    gesture: Gesture,
    content: &PreviewContent<'_>,
    picture: &PictureOnScreen,
    commands: &mut Vec<Command>,
) -> Option<(Pos2, Angle)> {
    let pointer = catchers.interact_pointer_pos()?;
    match gesture.grab {
        Grab::Layer(clip) => {
            let layer = content.layer(clip)?;
            let delta = catchers.drag_delta();
            let moved = layer.placement.moved_by(
                delta.x / picture.rectangle.width().max(1.0),
                delta.y / picture.rectangle.height().max(1.0),
            );
            push_if_changed(commands, layer, moved);
        }
        Grab::Handle { layer, corner } => {
            let layer = content.layer(layer)?;
            // Text takes whatever shape is dragged; a picture keeps its own.
            let resized = layer.placement.with_corner_dragged_to(
                corner,
                picture.fractions_at(pointer),
                picture.size_in_pixels,
                !layer.is_text(),
            );
            push_if_changed(commands, layer, resized);
        }
        Grab::PlaceToTurnFrom { layer, .. } => {
            let layer = content.layer(layer)?;
            let begun = gesture.turn?;
            let direction = layer
                .placement
                .direction_to(picture.fractions_at(pointer), picture.size_in_pixels);
            let mut rotation = begun
                .rotation_of_the_layer
                .plus(direction.minus(begun.direction_of_the_pointer));
            if ui.input(|input| input.modifiers.shift) {
                rotation = rotation.snapped_to(SNAP_STEP_DEGREES);
            }
            push_if_changed(
                commands,
                layer,
                Placement {
                    rotation,
                    ..layer.placement
                },
            );
            return Some((pointer, rotation));
        }
        Grab::Symbol => {
            let symbol = content.symbol_on_screen(picture)?;
            commands.push(Command::SetSpeedIndicatorLayout(symbol.layout_moved_by(
                content.indicator_layout,
                picture.rectangle,
                catchers.drag_delta(),
            )));
        }
        Grab::SymbolCorner => {
            let symbol = content.symbol_on_screen(picture)?;
            commands.push(Command::SetSpeedIndicatorLayout(
                symbol.layout_resized_to(picture.rectangle, pointer),
            ));
        }
    }
    None
}

/// Does what a click at the place of the catcher that was clicked does: picks the layer there.
fn click(catchers: &Catchers, content: &PreviewContent<'_>, picture: &PictureOnScreen, commands: &mut Vec<Command>) {
    let Some(point) = catchers.interact_pointer_pos() else {
        return;
    };
    if let Some(Grab::Layer(clip)) = grab_at(content, picture, point) {
        commands.push(select_only(clip));
    }
}

/// Whether two places are the same, but for the rounding of what a pointer that is held still gives.
fn places_are_the_same(first: &Placement, second: &Placement) -> bool {
    [
        (first.area.left(), second.area.left()),
        (first.area.top(), second.area.top()),
        (first.area.width(), second.area.width()),
        (first.area.height(), second.area.height()),
    ]
    .into_iter()
    .all(|(before, after)| (before - after).abs() < SMALLEST_CHANGE_OF_AREA)
        && first.rotation.minus(second.rotation).degrees().abs() < SMALLEST_CHANGE_OF_ANGLE_IN_DEGREES
}

/// Adds the command that puts `layer` at `placement`, unless it is already there.
fn push_if_changed(commands: &mut Vec<Command>, layer: &PreviewLayer<'_>, placement: Placement) {
    if !places_are_the_same(&placement, &layer.placement) {
        commands.push(Command::SetClipPlacement {
            clip: layer.clip,
            placement,
        });
    }
}

impl Pointing {
    /// Whether `layer` is to be outlined: the pointer is over it, and it is something to say so about
    /// (a video that is all of the picture says nothing until it is picked: all of the picture is it).
    pub fn outlines(&self, content: &PreviewContent<'_>, layer: &PreviewLayer<'_>) -> bool {
        !self.is_dragging
            && self.grab == Some(Grab::Layer(layer.clip))
            && (content.selected_clip == Some(layer.clip) || !layer.fills_the_picture())
    }

    /// Whether the symbol is to be outlined, with its corner shown: the pointer is over it or has hold of it.
    pub fn emphasises_the_symbol(&self) -> bool {
        matches!(self.grab, Some(Grab::Symbol | Grab::SymbolCorner))
    }

    /// The mouse pointer that shows what is under it can be taken hold of and what it does.
    fn cursor(&self, content: &PreviewContent<'_>, picture: &PictureOnScreen) -> Option<CursorIcon> {
        match self.grab? {
            Grab::Layer(clip) => {
                if self.is_dragging {
                    Some(CursorIcon::Grabbing)
                } else {
                    let layer = content.layer(clip)?;
                    (content.selected_clip == Some(clip) || !layer.fills_the_picture()).then_some(CursorIcon::Grab)
                }
            }
            Grab::Handle { layer, corner } => {
                let layer = content.layer(layer)?;
                Some(cursor_for_resizing_at(picture, &layer.placement, corner))
            }
            Grab::PlaceToTurnFrom { .. } => Some(CursorIcon::Crosshair),
            Grab::Symbol => Some(if self.is_dragging {
                CursorIcon::Grabbing
            } else {
                CursorIcon::Grab
            }),
            Grab::SymbolCorner => Some(CursorIcon::ResizeNwSe),
        }
    }

    /// Shows what the pointer is on, with `painter` for what is drawn (which is not cut off anywhere): the
    /// mouse pointer, a ring round the corner that a layer is turned from, and how far round a layer being
    /// turned is turned.
    pub fn show(&self, ui: &Ui, painter: &Painter, content: &PreviewContent<'_>, picture: &PictureOnScreen) {
        if let Some(cursor) = self.cursor(content, picture) {
            ui.ctx().set_cursor_icon(cursor);
        }
        if let Some(Grab::PlaceToTurnFrom { layer, corner }) = self.grab
            && let Some(layer) = content.layer(layer)
        {
            // A ring round the corner shows where the layer can be turned from.
            painter.circle_stroke(
                picture.corner_of(&layer.placement, corner),
                TURN_ZONE_RADIUS * 0.75,
                Stroke::new(1.5, theme::SELECTION_OUTLINE.gamma_multiply(0.7)),
            );
        }
        if let Some((pointer, rotation)) = self.turning {
            painter.text(
                pointer + vec2(18.0, -18.0),
                Align2::LEFT_BOTTOM,
                format!("{:.0}\u{b0}", rotation.degrees()),
                FontId::proportional(14.0),
                theme::SELECTION_OUTLINE,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::preview_panel::test_content::{
        clip, content, in_the_middle, layer, picture, whole_picture,
    };
    use crate::project::{CanvasArea, Corner};

    #[test]
    fn places_that_differ_by_rounding_only_are_the_same_and_places_that_were_moved_or_turned_are_not() {
        let place = Placement::new(CanvasArea::new(0.25, 0.5, 0.5, 0.2), Angle::from_degrees(30.0));
        let nudged_by_rounding = Placement::new(
            CanvasArea::new(0.25 + 1e-7, 0.5 - 1e-7, 0.5, 0.2 + 1e-7),
            Angle::from_degrees(30.0 + 1e-5),
        );
        assert!(places_are_the_same(&place, &nudged_by_rounding));
        assert!(!places_are_the_same(&place, &place.moved_by(0.001, 0.0)));
        assert!(!places_are_the_same(
            &place,
            &Placement::new(place.area, place.rotation.plus(Angle::from_degrees(0.5)))
        ));
        assert!(!places_are_the_same(
            &place,
            &place.with_corner_dragged_to(Corner::BottomRight, (0.9, 0.9), (1600.0, 900.0), false)
        ));
    }

    #[test]
    fn a_turn_across_the_half_way_point_is_a_turn_and_not_a_nearly_whole_circle() {
        // 179.9 and -179.9 are close together on the circle: they are not 359.8 degrees apart.
        let place = Placement::new(CanvasArea::DEFAULT, Angle::from_degrees(179.9999));
        let other = Placement::new(CanvasArea::DEFAULT, Angle::from_degrees(-179.9999));
        assert!(places_are_the_same(&place, &other));
    }

    fn pointing(grab: Option<Grab>, is_dragging: bool) -> Pointing {
        Pointing {
            grab,
            is_dragging,
            turning: None,
        }
    }

    #[test]
    fn a_video_that_is_all_of_the_picture_is_not_outlined_or_given_a_hand_until_it_is_picked() {
        let layers = vec![layer(0, whole_picture()), layer(1, in_the_middle())];
        let not_picked = content(layers, None);
        let over_the_video = pointing(Some(Grab::Layer(clip(0))), false);
        assert!(!over_the_video.outlines(&not_picked, &not_picked.layers[0]));
        assert_eq!(over_the_video.cursor(&not_picked, &picture()), None);

        let over_the_box = pointing(Some(Grab::Layer(clip(1))), false);
        assert!(over_the_box.outlines(&not_picked, &not_picked.layers[1]));
        assert_eq!(over_the_box.cursor(&not_picked, &picture()), Some(CursorIcon::Grab));

        let picked = content(vec![layer(0, whole_picture())], Some(0));
        assert!(over_the_video.outlines(&picked, &picked.layers[0]));
        assert_eq!(over_the_video.cursor(&picked, &picture()), Some(CursorIcon::Grab));
    }

    #[test]
    fn a_layer_that_is_being_dragged_is_not_outlined_and_the_pointer_is_a_closed_hand() {
        let content = content(vec![layer(1, in_the_middle())], Some(1));
        let dragging = pointing(Some(Grab::Layer(clip(1))), true);
        assert!(!dragging.outlines(&content, &content.layers[0]));
        assert_eq!(dragging.cursor(&content, &picture()), Some(CursorIcon::Grabbing));
    }

    #[test]
    fn only_the_layer_the_pointer_is_over_is_outlined() {
        let content = content(vec![layer(0, in_the_middle()), layer(1, in_the_middle())], None);
        let over_the_second = pointing(Some(Grab::Layer(clip(1))), false);
        assert!(!over_the_second.outlines(&content, &content.layers[0]));
        assert!(over_the_second.outlines(&content, &content.layers[1]));
        assert!(!pointing(None, false).outlines(&content, &content.layers[1]));
    }

    #[test]
    fn the_pointer_over_what_can_be_taken_hold_of_shows_what_it_does() {
        let content = content(vec![layer(1, in_the_middle())], Some(1));
        let picture = picture();
        assert_eq!(
            pointing(
                Some(Grab::PlaceToTurnFrom {
                    layer: clip(1),
                    corner: Corner::TopLeft
                }),
                false
            )
            .cursor(&content, &picture),
            Some(CursorIcon::Crosshair)
        );
        assert_eq!(
            pointing(
                Some(Grab::Handle {
                    layer: clip(1),
                    corner: Corner::BottomRight
                }),
                false
            )
            .cursor(&content, &picture),
            Some(CursorIcon::ResizeNwSe)
        );
        assert_eq!(
            pointing(Some(Grab::Symbol), false).cursor(&content, &picture),
            Some(CursorIcon::Grab)
        );
        assert_eq!(
            pointing(Some(Grab::Symbol), true).cursor(&content, &picture),
            Some(CursorIcon::Grabbing)
        );
        assert_eq!(
            pointing(Some(Grab::SymbolCorner), false).cursor(&content, &picture),
            Some(CursorIcon::ResizeNwSe)
        );
        assert_eq!(pointing(None, false).cursor(&content, &picture), None);
    }

    #[test]
    fn the_symbol_is_outlined_while_the_pointer_is_over_it_or_has_hold_of_it_and_not_otherwise() {
        assert!(pointing(Some(Grab::Symbol), false).emphasises_the_symbol());
        assert!(pointing(Some(Grab::SymbolCorner), true).emphasises_the_symbol());
        assert!(!pointing(Some(Grab::Layer(clip(0))), false).emphasises_the_symbol());
        assert!(!pointing(None, false).emphasises_the_symbol());
    }
}
