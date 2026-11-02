// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The editor of the graph that maps the value a mask gives to the value an effect uses (`docs/shader-masks-design.md`, 3 and 8), in the settings window of the mask: a square with the
//! value of the mask's channel along the bottom (nothing of it at the left, all of it at the right) and the number's range up the side (the least of it at the bottom, the most at the top),
//! and the graph of line segments drawn in it.
//!
//! * **Drag a point** to move it. An end moves up and down only; any other point stays between its neighbours.
//! * **Press on the line, and click it**, to add a point there; pressing on the line and dragging adds the point and carries it along.
//! * **Right-click a point** to take it away (not an end).
//!
//! The widget holds nothing but which point is being dragged: it is given the graph, and gives the graph it has become, for the window to make a step of the history of (the choices
//! about one number are one step when they follow each other).

use egui::{
    CornerRadius, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, WidgetInfo, WidgetType, pos2, vec2,
};

use crate::project::{Graph, MaskMap};

/// What the square is called to a screen reader and the tests.
pub const LABEL: &str = "Graph of the number";

/// What is said when the pointer rests on the square.
pub const HINT: &str = "Drag a point to move it. Click the line to add a point. Right-click a point to take it away.";

/// How big the square is, in points, along each side.
pub const SIZE_IN_POINTS: f32 = 168.0;

/// How big the circle of a point is drawn, in points across the radius.
const RADIUS_OF_A_POINT: f32 = 4.5;

/// How near, in points, the pointer has to be to a point of the graph to take hold of it.
const HOW_NEAR_A_POINT_IS_TAKEN_HOLD_OF: f32 = 9.0;

/// How near, in points, the pointer has to be to the line to add a point to it.
const HOW_NEAR_THE_LINE_IS_CLICKED: f32 = 6.0;

/// Where on the screen the point `(place, height)` of a graph is, when the graph is drawn in `rect`: the place 0 is at the left, the height 0 at the bottom.
pub fn place_on_the_screen(rect: Rect, (place, height): (f32, f32)) -> Pos2 {
    pos2(
        rect.left() + place * rect.width(),
        rect.bottom() - height * rect.height(),
    )
}

/// The place and the height that `position` on the screen is, when a graph is drawn in `rect` (not kept inside the square).
pub fn point_under(rect: Rect, position: Pos2) -> (f32, f32) {
    (
        (position.x - rect.left()) / rect.width(),
        (rect.bottom() - position.y) / rect.height(),
    )
}

/// The point of `graph` that `position` is near enough to take hold of, the nearest if there are two.
pub fn handle_at(rect: Rect, graph: &Graph, position: Pos2) -> Option<usize> {
    graph
        .points()
        .iter()
        .enumerate()
        .map(|(number, point)| (number, place_on_the_screen(rect, *point).distance(position)))
        .filter(|(_, distance)| *distance <= HOW_NEAR_A_POINT_IS_TAKEN_HOLD_OF)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(number, _)| number)
}

/// How far `position` is from the segment between `from` and `to`.
fn distance_to_a_segment(from: Pos2, to: Pos2, position: Pos2) -> f32 {
    let along = to - from;
    let length_squared = along.length_sq();
    let share = if length_squared > 0.0 {
        ((position - from).dot(along) / length_squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (from + along * share).distance(position)
}

/// Whether `position` is near enough to the line of `graph` to add a point to it.
pub fn is_near_the_line(rect: Rect, graph: &Graph, position: Pos2) -> bool {
    graph.points().windows(2).any(|pair| {
        distance_to_a_segment(
            place_on_the_screen(rect, pair[0]),
            place_on_the_screen(rect, pair[1]),
            position,
        ) <= HOW_NEAR_THE_LINE_IS_CLICKED
    })
}

/// The map that `graph` is: the straight map if it is the straight line (so that a graph that has been given back its first shape takes no numbers of the call), and the graph otherwise.
pub fn map_of(graph: Graph) -> MaskMap {
    if graph == Graph::straight() {
        MaskMap::Straight
    } else {
        MaskMap::Graph(graph)
    }
}

/// What the pointer does to `graph` in this frame (`graph` as it is given, if nothing).
fn after_the_pointer(ui: &Ui, response: &Response, rect: Rect, graph: Graph) -> Graph {
    let id = response.id;
    let mut graph = graph;
    if response.drag_started() {
        let origin = ui.input(|input| input.pointer.press_origin()).unwrap_or(rect.center());
        let mut grabbed = handle_at(rect, &graph, origin);
        if grabbed.is_none() && is_near_the_line(rect, &graph, origin) {
            // Pressing on the line and dragging adds a point where the press was, and carries it along.
            let (place, _) = point_under(rect, origin);
            if let Some(added) = graph.with_a_point_added_at(place) {
                grabbed = added.points().iter().position(|point| point.0 == place);
                graph = added;
            }
        }
        ui.data_mut(|data| data.insert_temp(id, grabbed));
    }
    if response.dragged() {
        let grabbed: Option<usize> = ui.data(|data| data.get_temp::<Option<usize>>(id)).flatten();
        if let (Some(index), Some(position)) = (grabbed, response.interact_pointer_pos()) {
            graph = graph.with_a_point_moved(index, point_under(rect, position));
        }
    }
    if response.drag_stopped() {
        ui.data_mut(|data| data.remove::<Option<usize>>(id));
    }
    if response.clicked()
        && let Some(position) = response.interact_pointer_pos()
        && handle_at(rect, &graph, position).is_none()
        && is_near_the_line(rect, &graph, position)
        && let Some(added) = graph.with_a_point_added_at(point_under(rect, position).0)
    {
        graph = added;
    }
    if response.secondary_clicked()
        && let Some(position) = response.interact_pointer_pos()
        && let Some(index) = handle_at(rect, &graph, position)
        && let Some(removed) = graph.with_a_point_removed(index)
    {
        graph = removed;
    }
    graph
}

/// Draws the square, the straight line to compare with, and `graph` with a circle at each point.
fn paint(ui: &Ui, rect: Rect, graph: &Graph, enabled: bool, hovered: Option<usize>) {
    let painter = ui.painter_at(rect.expand(RADIUS_OF_A_POINT + 2.0));
    let visuals = ui.visuals();
    let faint = visuals.widgets.noninteractive.bg_stroke.color;
    painter.rect(
        rect,
        CornerRadius::same(3),
        visuals.extreme_bg_color,
        Stroke::new(1.0, faint),
        StrokeKind::Inside,
    );
    for step in 1..4 {
        let along = step as f32 / 4.0;
        painter.line_segment(
            [
                place_on_the_screen(rect, (along, 0.0)),
                place_on_the_screen(rect, (along, 1.0)),
            ],
            Stroke::new(1.0, faint.gamma_multiply(0.5)),
        );
        painter.line_segment(
            [
                place_on_the_screen(rect, (0.0, along)),
                place_on_the_screen(rect, (1.0, along)),
            ],
            Stroke::new(1.0, faint.gamma_multiply(0.5)),
        );
    }
    // (The straight line, which is what the number is until a graph is drawn, to see what the graph does by.)
    painter.line_segment(
        [
            place_on_the_screen(rect, (0.0, 0.0)),
            place_on_the_screen(rect, (1.0, 1.0)),
        ],
        Stroke::new(1.0, faint),
    );
    let colour = if enabled {
        visuals.selection.bg_fill
    } else {
        visuals.widgets.noninteractive.fg_stroke.color.gamma_multiply(0.4)
    };
    let points: Vec<Pos2> = graph
        .points()
        .iter()
        .map(|point| place_on_the_screen(rect, *point))
        .collect();
    painter.add(Shape::line(points.clone(), Stroke::new(2.0, colour)));
    for (number, position) in points.iter().enumerate() {
        let radius = if hovered == Some(number) {
            RADIUS_OF_A_POINT + 1.5
        } else {
            RADIUS_OF_A_POINT
        };
        painter.circle(*position, radius, visuals.extreme_bg_color, Stroke::new(2.0, colour));
    }
}

/// Shows `graph` in a square, and gives the graph it has become when the person changed it with the pointer. `enabled` is false while there is no track to read (the graph is shown and
/// cannot be changed).
pub fn show(ui: &mut Ui, graph: Graph, enabled: bool) -> Option<Graph> {
    let sense = if enabled {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(SIZE_IN_POINTS, SIZE_IN_POINTS), sense);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, enabled, LABEL));
    let edited = if enabled {
        after_the_pointer(ui, &response, rect, graph)
    } else {
        graph
    };
    let hovered = response
        .hover_pos()
        .filter(|_| enabled)
        .and_then(|position| handle_at(rect, &edited, position));
    paint(ui, rect, &edited, enabled, hovered);
    response.on_hover_text(HINT);
    (edited != graph).then_some(edited)
}

#[cfg(test)]
mod tests;
