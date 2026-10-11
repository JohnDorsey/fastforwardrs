//! The text objects on the preview: drawn where they are in the finished picture, and with the
//! mouse they can be selected (a click), edited (a double click), moved (dragging them), and, for
//! the selected one, resized by dragging any of its four corners.

use egui::{Color32, CursorIcon, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, WidgetInfo, WidgetType, pos2, vec2};

use crate::application::clicks::is_a_double_click;
use crate::application::commands::Command;
use crate::application::textures::{TextPictureKey, Textures};
use crate::application::theme;
use crate::composition::VisibleText;
use crate::project::{CanvasArea, ClipLocation, Corner, TextObject};
use crate::text_rendering::LazyTextRenderer;

/// How big the squares at the corners of the selected text are, in points.
const HANDLE_SIZE: f32 = 12.0;

/// What the text objects on the preview are drawn from.
pub(super) struct TextOverlayContent<'a> {
    /// The text objects that are over the picture now, the one at the back first.
    pub texts: &'a [VisibleText],
    pub selected_clip: Option<ClipLocation>,
    /// How big the finished picture is, in pixels: the text pictures are made for that size.
    pub picture_size_in_pixels: (u32, u32),
    /// Where the finished picture is drawn on the screen.
    pub picture_rectangle: Rect,
}

/// The name a text object on the preview is known by to screen readers (and the tests).
pub fn text_object_label(clip: ClipLocation) -> String {
    format!("Text object, {clip}")
}

/// The name the handle at a corner of the selected text object is known by.
pub fn corner_handle_label(corner: Corner) -> String {
    format!("Resize handle at the {} corner of the text object", corner_name(corner))
}

fn corner_name(corner: Corner) -> &'static str {
    match corner {
        Corner::TopLeft => "top left",
        Corner::TopRight => "top right",
        Corner::BottomLeft => "bottom left",
        Corner::BottomRight => "bottom right",
    }
}

/// Where `area` is on the screen, when the finished picture is drawn in `picture_rectangle`.
pub(super) fn rectangle_of_area(area: &CanvasArea, picture_rectangle: Rect) -> Rect {
    Rect::from_min_size(
        pos2(
            picture_rectangle.left() + area.left() * picture_rectangle.width(),
            picture_rectangle.top() + area.top() * picture_rectangle.height(),
        ),
        vec2(
            area.width() * picture_rectangle.width(),
            area.height() * picture_rectangle.height(),
        ),
    )
}

/// How far across and down the finished picture, as fractions of its size, a point on the screen
/// is, when the picture is drawn in `picture_rectangle`.
pub(super) fn fractions_of_picture_at(point: Pos2, picture_rectangle: Rect) -> (f32, f32) {
    (
        (point.x - picture_rectangle.left()) / picture_rectangle.width().max(1.0),
        (point.y - picture_rectangle.top()) / picture_rectangle.height().max(1.0),
    )
}

/// Draws the text objects and adds a command for each thing the user does to them.
pub(super) fn show(
    ui: &mut Ui,
    content: TextOverlayContent<'_>,
    textures: &mut Textures,
    text_renderer: &mut LazyTextRenderer,
    commands: &mut Vec<Command>,
) {
    let (picture_width, picture_height) = content.picture_size_in_pixels;
    let picture_rectangle = content.picture_rectangle;
    let whole_texture = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    let mut selected: Option<(&VisibleText, Rect)> = None;

    for text in content.texts {
        let rectangle = rectangle_of_area(&text.object.area, picture_rectangle);
        let key = TextPictureKey::of(&text.object, picture_width, picture_height);
        if let Some(texture) = textures.text_picture(ui.ctx(), key, || {
            text_renderer
                .get_if_ready()
                .map(|renderer| renderer.render(&text.object, picture_width, picture_height))
        }) {
            ui.painter()
                .image(texture.id(), rectangle, whole_texture, Color32::WHITE);
        }

        let is_selected = content.selected_clip == Some(text.clip);
        let response = ui.interact(
            rectangle,
            Id::new(("text object on the preview", text.clip)),
            Sense::CLICK | Sense::DRAG,
        );
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, text_object_label(text.clip)));
        if is_a_double_click(&response) {
            commands.push(Command::SelectClip(text.clip));
            commands.push(Command::EditTextObject(text.clip));
        } else if response.clicked() || (response.drag_started() && !is_selected) {
            commands.push(Command::SelectClip(text.clip));
        }
        if response.dragged() {
            let delta = response.drag_delta();
            let moved = text.object.area.moved_by(
                delta.x / picture_rectangle.width().max(1.0),
                delta.y / picture_rectangle.height().max(1.0),
            );
            push_if_changed(commands, text, moved);
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
            ui.painter().rect_stroke(
                rectangle,
                0.0,
                Stroke::new(1.0, theme::INDICATOR_OUTLINE),
                StrokeKind::Outside,
            );
        }
        if is_selected {
            selected = Some((text, rectangle));
        }
    }

    // The selected text has its outline and corners over everything else.
    if let Some((text, rectangle)) = selected {
        ui.painter().rect_stroke(
            rectangle,
            0.0,
            Stroke::new(2.0, theme::SELECTION_OUTLINE),
            StrokeKind::Outside,
        );
        for corner in Corner::ALL {
            show_corner_handle(ui, text, corner, picture_rectangle, commands);
        }
    }
}

fn show_corner_handle(
    ui: &mut Ui,
    text: &VisibleText,
    corner: Corner,
    picture_rectangle: Rect,
    commands: &mut Vec<Command>,
) {
    let (corner_x, corner_y) = text.object.area.corner_position(corner);
    let centre = pos2(
        picture_rectangle.left() + corner_x * picture_rectangle.width(),
        picture_rectangle.top() + corner_y * picture_rectangle.height(),
    );
    let handle_rectangle = Rect::from_center_size(centre, vec2(HANDLE_SIZE, HANDLE_SIZE));
    let response = ui.interact(
        handle_rectangle,
        Id::new(("text object corner", text.clip, corner_name(corner))),
        Sense::DRAG,
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, corner_handle_label(corner)));
    ui.painter()
        .rect_filled(handle_rectangle, 2.0, theme::SELECTION_OUTLINE);
    ui.painter().rect_stroke(
        handle_rectangle,
        2.0,
        Stroke::new(1.0, Color32::BLACK),
        StrokeKind::Inside,
    );

    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(match corner {
            Corner::TopLeft | Corner::BottomRight => CursorIcon::ResizeNwSe,
            Corner::TopRight | Corner::BottomLeft => CursorIcon::ResizeNeSw,
        });
    }
    if response.dragged()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let (x, y) = fractions_of_picture_at(pointer, picture_rectangle);
        push_if_changed(commands, text, text.object.area.with_corner_dragged_to(corner, x, y));
    }
}

/// How far an area's edges have to move (as a fraction of the picture) to count as a change. A
/// pointer that is held still can still make the same corner come out a hair different from where
/// it was, because of rounding, and that is not a change worth saving.
const SMALLEST_CHANGE_OF_AREA: f32 = 1e-5;

fn areas_are_the_same(first: &CanvasArea, second: &CanvasArea) -> bool {
    [
        (first.left(), second.left()),
        (first.top(), second.top()),
        (first.width(), second.width()),
        (first.height(), second.height()),
    ]
    .into_iter()
    .all(|(before, after)| (before - after).abs() < SMALLEST_CHANGE_OF_AREA)
}

/// Adds the command that gives `text` the new `area`, unless it already has it.
fn push_if_changed(commands: &mut Vec<Command>, text: &VisibleText, area: CanvasArea) {
    if !areas_are_the_same(&area, &text.object.area) {
        commands.push(Command::SetTextObject {
            clip: text.clip,
            object: TextObject {
                area,
                ..text.object.clone()
            },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture() -> Rect {
        Rect::from_min_size(pos2(100.0, 50.0), vec2(800.0, 450.0))
    }

    #[test]
    fn an_area_is_where_its_fractions_of_the_picture_are_on_the_screen() {
        let rectangle = rectangle_of_area(&CanvasArea::new(0.25, 0.5, 0.5, 0.2), picture());
        assert_eq!(rectangle.left(), 100.0 + 200.0);
        assert_eq!(rectangle.top(), 50.0 + 225.0);
        assert_eq!(rectangle.width(), 400.0);
        assert_eq!(rectangle.height(), 90.0);
    }

    #[test]
    fn a_point_on_the_screen_is_found_again_as_fractions_of_the_picture() {
        let (x, y) = fractions_of_picture_at(pos2(300.0, 275.0), picture());
        assert_eq!((x, y), (0.25, 0.5));
        // Points outside the picture are outside the fractions 0 to 1, which the area then corrects.
        let (x, y) = fractions_of_picture_at(pos2(0.0, 0.0), picture());
        assert!(x < 0.0 && y < 0.0);
    }

    #[test]
    fn dragging_a_corner_to_where_it_is_on_the_screen_leaves_the_area_as_it_is() {
        let area = CanvasArea::new(0.25, 0.5, 0.5, 0.2);
        let rectangle = rectangle_of_area(&area, picture());
        let (x, y) = fractions_of_picture_at(rectangle.right_bottom(), picture());
        assert_eq!(area.with_corner_dragged_to(Corner::BottomRight, x, y), area);
    }

    #[test]
    fn areas_that_differ_by_rounding_only_are_the_same_and_areas_that_were_moved_are_not() {
        let area = CanvasArea::new(0.25, 0.5, 0.5, 0.2);
        let nudged_by_rounding = CanvasArea::new(0.25 + 1e-7, 0.5 - 1e-7, 0.5, 0.2 + 1e-7);
        assert!(areas_are_the_same(&area, &nudged_by_rounding));
        assert!(!areas_are_the_same(&area, &area.moved_by(0.001, 0.0)));
        assert!(!areas_are_the_same(
            &area,
            &area.with_corner_dragged_to(Corner::BottomRight, 0.9, 0.9)
        ));
    }

    #[test]
    fn every_corner_has_its_own_name() {
        let names: std::collections::HashSet<String> = Corner::ALL.into_iter().map(corner_handle_label).collect();
        assert_eq!(names.len(), 4);
    }
}
