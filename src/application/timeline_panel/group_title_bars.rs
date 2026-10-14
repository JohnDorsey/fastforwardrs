//! The names of groups on the timeline: a bar in the colour of the group, with the name of the group in it, that is shown while the pointer is
//! over the group, and is where the group is clicked (which selects it) and dragged (which moves it, with everything that is in it).
//!
//! It is a **tab**: a capsule that is flat on the side that is against the outline of the group, which is the top side of the outline (a tab
//! that is above the outline has its flat side down, and one that is hanging below it, where there is no room above, has it up). It is as wide
//! as the name and its grip dots need, and is in the middle of the stretch of the top edge that is in view, so that it does not run the whole
//! width of a group that is wider than that, and goes out beyond the sides of one that is not as wide, with its ends round.
//!
//! * A **clip group** has its tab on its one top edge.
//! * A **tag** has its tab on whichever top edge of its outline is the best place for it: the longest that is in view (a tag that is round
//!   clips here and there has a top edge at each of the clips that is not under another).
//! * A **track group** has none: it is not clicked or dragged on the timeline.
//!
//! Of the groups that the pointer is over, the one that has the tab is the **innermost** (the one with the fewest groups in it, and the smallest
//! of those): the pointer comes to the tab of a group that is round it by going out of the groups that are in it, or onto the tab, which is
//! where the group is wherever it is.
//!
//! The group is dragged as its clips are (see [`super::item_dragging`]): along the timeline as far as the pointer goes, and to other rows as
//! many as the pointer goes, as far as the clips next to it and the rules of groups leave room.

use egui::{
    Color32, CornerRadius, CursorIcon, FontId, PointerButton, Pos2, Rect, Sense, Ui, Vec2,
    text::{LayoutJob, TextFormat, TextWrapping},
    vec2,
};

use super::grip_dots::{Arrangement, draw as draw_grip_dots};
use super::group_outlines::OutlineOfAGroup;
use super::item_dragging::{drag_of, row_nearest_to, what_is_dragged};
use super::strip_view::StripView;
use super::view_state::Grabbed;
use super::{TimelineContent, TimelineViewState};
use crate::application::commands::Command;
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::project::{GroupId, GroupKind, Track};

/// How tall a tab is, in points.
pub const HEIGHT: f32 = 17.0;

/// How far from the ends of a tab what is in it is, and how far the dots are from the name.
const PADDING: f32 = 8.0;
const SPACE_AFTER_THE_DOTS: f32 = 6.0;

/// How wide the name in a tab can be, in points: a longer name is cut off, with an ellipsis.
const WIDEST_NAME: f32 = 260.0;

/// How small a stretch of a top edge that is in view is worth putting a tab on: a group of which less than this is in view has no tab.
const LEAST_EDGE_IN_VIEW: f32 = 12.0;

/// The dots of a tab: the same as at the left end of a rack.
const DOTS: Arrangement = Arrangement::FOR_A_RACK;

/// Where a tab goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Placement {
    pub rectangle: Rect,
    /// Whether the tab is under the edge it is on (it hangs from it, inside the group) and not over it.
    pub hangs_below_the_edge: bool,
}

/// A stretch of the top of an outline that goes straight across: where it is, and how far across.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TopEdge {
    from_x: f32,
    to_x: f32,
    y: f32,
}

/// The top edges of the loops of an outline: the stretches that go straight from the left to the right (the loops go round clockwise on the
/// screen, so what goes that way is at the top of the shape, with the inside of it below).
fn top_edges_of(loops: &[Vec<Pos2>]) -> Vec<TopEdge> {
    let mut edges = Vec::new();
    for points in loops {
        for (place, from) in points.iter().enumerate() {
            let to = points[(place + 1) % points.len()];
            if (to.y - from.y).abs() <= 0.01 && to.x - from.x > 0.5 {
                edges.push(TopEdge {
                    from_x: from.x,
                    to_x: to.x,
                    y: from.y,
                });
            }
        }
    }
    edges
}

/// Where the tab of a group goes, on the outline that is the loops `loops`: a tab that is `size` wide and tall, in the area `allowed` (the
/// part of the screen that a tab can be in: the strips, and the ruler over them). It is on the top edge that has the most in view (as much as
/// a tab is wide, at the most), and of those that have as much, the highest, and then the one furthest left; in the middle of the stretch of
/// the edge that is in view, and over the edge, unless there is no room over it in the area, when it hangs under it. `None` if no top edge has
/// enough in view.
pub(super) fn place_a_tab(loops: &[Vec<Pos2>], size: Vec2, allowed: Rect) -> Option<Placement> {
    let mut best: Option<(f32, f32, f32, TopEdge, f32, f32)> = None;
    for edge in top_edges_of(loops) {
        if edge.y < allowed.top() || edge.y > allowed.bottom() {
            continue;
        }
        let (in_view_from, in_view_to) = (edge.from_x.max(allowed.left()), edge.to_x.min(allowed.right()));
        let in_view = in_view_to - in_view_from;
        if in_view < LEAST_EDGE_IN_VIEW {
            continue;
        }
        let worth = in_view.min(size.x);
        let better = match best {
            None => true,
            Some((best_worth, best_y, best_x, ..)) => {
                worth > best_worth + 0.5
                    || ((worth - best_worth).abs() <= 0.5
                        && (edge.y < best_y - 0.5 || ((edge.y - best_y).abs() <= 0.5 && edge.from_x < best_x)))
            }
        };
        if better {
            best = Some((worth, edge.y, edge.from_x, edge, in_view_from, in_view_to));
        }
    }
    let (_, _, _, edge, in_view_from, in_view_to) = best?;
    let middle = (in_view_from + in_view_to) / 2.0;
    // (One that is wider than the stretch goes out at both sides; and wherever it is, it is all in the area, if it can be: one that is wider
    // than the area is at the left end of it, and goes out of it at the right.)
    let left = (middle - size.x / 2.0).clamp(allowed.left(), (allowed.right() - size.x).max(allowed.left()));
    let hangs_below_the_edge = edge.y - size.y < allowed.top();
    let (top, bottom) = if hangs_below_the_edge {
        (edge.y, edge.y + size.y)
    } else {
        (edge.y - size.y, edge.y)
    };
    Some(Placement {
        rectangle: Rect::from_min_max(egui::pos2(left, top), egui::pos2(left + size.x, bottom)),
        hangs_below_the_edge,
    })
}

/// The corners of a tab that are round: the ones away from the edge that it is on (a tab is a capsule that is flat on the side that is
/// against the edge, so that it is the edge that goes on past it).
fn rounding_of_a_tab(hangs_below_the_edge: bool) -> CornerRadius {
    let round = (HEIGHT / 2.0).round() as u8;
    if hangs_below_the_edge {
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: round,
            se: round,
        }
    } else {
        CornerRadius {
            nw: round,
            ne: round,
            sw: 0,
            se: 0,
        }
    }
}

/// Whether `point` is inside the shape that the loops go round (for a point on a straight run of a loop, which side it is on is not worth
/// telling: it is where the outline is drawn).
pub(super) fn outline_has(loops: &[Vec<Pos2>], point: Pos2) -> bool {
    let mut inside = false;
    for points in loops {
        for (place, from) in points.iter().enumerate() {
            let to = points[(place + 1) % points.len()];
            if (from.y > point.y) != (to.y > point.y) {
                let crossing_x = from.x + (point.y - from.y) / (to.y - from.y) * (to.x - from.x);
                if point.x < crossing_x {
                    inside = !inside;
                }
            }
        }
    }
    inside
}

/// How a group is told from the others that the pointer is over: the one with fewer groups in it first, and then the one that is smaller.
fn closeness_of(outline: &OutlineOfAGroup) -> (usize, i64) {
    let points = outline.loops.iter().flatten().copied();
    let bounds = Rect::from_points(&points.collect::<Vec<_>>());
    (outline.height, (bounds.width() * bounds.height()).round() as i64)
}

/// What a tab of a group is made of: how it is laid out.
struct TabContents {
    name: std::sync::Arc<egui::Galley>,
    size: Vec2,
}

fn contents_of(ui: &Ui, name: &str, colour: Color32) -> TabContents {
    let mut job = LayoutJob::single_section(name.to_owned(), TextFormat::simple(FontId::proportional(12.0), colour));
    job.wrap = TextWrapping {
        max_width: WIDEST_NAME,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let name = ui.painter().layout_job(job);
    let width = PADDING + DOTS.size().x + SPACE_AFTER_THE_DOTS + name.size().x + PADDING;
    TabContents {
        name,
        size: vec2(width, HEIGHT),
    }
}

/// The group whose tab is shown, and where it is, if a tab is shown: that of the group that is being dragged by it, or else that of the
/// innermost group (a clip group or a tag) that the pointer is over, or over the tab of.
fn the_group_that_shows_a_tab(
    ui: &Ui,
    content: &TimelineContent<'_>,
    state: &TimelineViewState,
    outlines: &[OutlineOfAGroup],
    allowed: Rect,
    pointer: Option<Pos2>,
) -> Option<(GroupId, TabContents, Placement)> {
    let being_dragged = match state.item_drag.as_ref().map(|drag| drag.grabbed) {
        Some(Grabbed::Group { group, .. }) => Some(group),
        _ => None,
    };
    let candidates = outlines
        .iter()
        .filter(|outline| outline.kind != GroupKind::TrackGroup)
        .filter(|outline| being_dragged.is_none_or(|group| group == outline.group));
    let mut chosen: Option<(&OutlineOfAGroup, (usize, i64), TabContents, Placement)> = None;
    for outline in candidates {
        let Some(group) = content.project.group(outline.group) else {
            continue;
        };
        let wanted = being_dragged.is_some();
        if !wanted {
            let Some(pointer) = pointer else {
                continue;
            };
            if !allowed.contains(pointer) {
                continue;
            }
            let bounds = Rect::from_points(&outline.loops.iter().flatten().copied().collect::<Vec<_>>());
            if !bounds.expand2(vec2(0.0, HEIGHT)).contains(pointer) {
                continue;
            }
        }
        let colours = group.colour();
        let writing = if colours.writing_on_it_is_white() {
            Color32::WHITE
        } else {
            Color32::BLACK
        };
        let contents = contents_of(ui, group.name(), writing);
        let Some(placement) = place_a_tab(&outline.loops, contents.size, allowed) else {
            continue;
        };
        let is_over_it = wanted
            || pointer
                .is_some_and(|pointer| outline_has(&outline.loops, pointer) || placement.rectangle.contains(pointer));
        if !is_over_it {
            continue;
        }
        let closeness = closeness_of(outline);
        if chosen.as_ref().is_none_or(|(_, found, ..)| closeness < *found) {
            chosen = Some((outline, closeness, contents, placement));
        }
    }
    chosen.map(|(outline, _, contents, placement)| (outline.group, contents, placement))
}

/// Draws the tab of the group that the pointer is over, if there is one, and adds a command for what the user does to it: clicking it selects
/// the group, and dragging it moves the group. (`outlines` are those of [`super::group_outlines::outlines_of_the_groups`]; `allowed` is where a
/// tab can be, `rows` and `visible_rows` are the strips and the part of the screen that the rows can be seen in, for dragging.)
pub(super) fn show(
    ui: &mut Ui,
    content: &TimelineContent<'_>,
    state: &mut TimelineViewState,
    outlines: &[OutlineOfAGroup],
    allowed: Rect,
    (rows, visible_rows): (&[(&Track, StripView)], Rect),
    commands: &mut Vec<Command>,
) {
    if !content.can_edit || outlines.is_empty() {
        return;
    }
    // (While something else is being dragged, or a track is being made taller, the pointer is not over a group to take hold of it.)
    let something_else_is_going_on = state.is_in_a_gesture_other_than_item_dragging()
        || state
            .item_drag
            .as_ref()
            .is_some_and(|drag| !matches!(drag.grabbed, Grabbed::Group { .. }));
    if something_else_is_going_on {
        return;
    }
    let pointer = ui.input(|input| input.pointer.hover_pos());
    let Some((group, contents, placement)) = the_group_that_shows_a_tab(ui, content, state, outlines, allowed, pointer)
    else {
        return;
    };
    let Some(found) = content.project.group(group) else {
        return;
    };
    let fill = Color32::from_rgb(found.colour().red, found.colour().green, found.colour().blue);
    let painter = ui.painter().with_clip_rect(allowed.intersect(ui.clip_rect()));
    painter.rect_filled(
        placement.rectangle,
        rounding_of_a_tab(placement.hangs_below_the_edge),
        fill,
    );
    let middle_of_the_dots = egui::pos2(
        placement.rectangle.left() + PADDING + DOTS.size().x / 2.0,
        placement.rectangle.center().y,
    );
    draw_grip_dots(&painter, DOTS, middle_of_the_dots);
    let text_at = egui::pos2(
        placement.rectangle.left() + PADDING + DOTS.size().x + SPACE_AFTER_THE_DOTS,
        placement.rectangle.center().y - contents.name.size().y / 2.0,
    );
    painter.galley(text_at, contents.name.clone(), Color32::WHITE);

    let response = ui.interact(
        placement.rectangle,
        ui.id().with(("the title bar of a group", group)),
        Sense::click_and_drag(),
    );
    if state.item_drag.is_none() && (response.hovered() || response.is_pointer_button_down_on()) {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    }
    let adding_to_the_selection = ui.input(|input| input.modifiers.command);
    if response.drag_started_by(PointerButton::Primary) {
        if let (Some(origin), Some(rectangle)) = (
            ui.input(|input| input.pointer.press_origin()),
            content.project.rectangle_of_the_group(group),
        ) {
            let items = what_is_dragged(content, SelectionItem::Group(group), adding_to_the_selection, commands);
            let (Some((_, view)), Some(row)) = (rows.first(), row_nearest_to(rows, visible_rows, origin.y)) else {
                return;
            };
            let frame_under = |screen_x: f32| view.frame_at(screen_x).map_or(0, |frame| frame.value());
            state.item_drag = Some(drag_of(
                content.project,
                &items,
                Grabbed::Group {
                    group,
                    first_frame: rectangle.first_frame,
                    row,
                },
                frame_under(origin.x),
            ));
        }
    } else if response.clicked_by(PointerButton::Primary) {
        commands.push(Command::SelectItems {
            items: vec![SelectionItem::Group(group)],
            change: if adding_to_the_selection {
                SelectionChange::Toggle
            } else {
                SelectionChange::Replace
            },
        });
    }
}

#[cfg(test)]
mod tests;
