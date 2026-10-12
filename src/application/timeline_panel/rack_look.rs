//! What a rack of keyframes and a keyframe look like.
//!
//! A rack is a long, shallow tray sunk into the drawer it is in: textured with a fine dark noise like grain, shaded as if it
//! were concave (the top edge is in shadow and the bottom edge catches the light), with corners rounded a little (not so
//! much that it is a capsule: its ends have a straight edge). A keyframe is a bright orange diamond (a square turned a
//! quarter of a turn) on it.

use egui::epaint::Vertex;
use egui::{
    Color32, ColorImage, CornerRadius, Mesh, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, TextureId, pos2, vec2,
};

use crate::application::theme;

/// How round the corners of a rack are, in points.
pub const RACK_CORNER_RADIUS: u8 = 3;

/// How tall a keyframe is (the diamond, from its top corner to its bottom corner), in points: about as tall as the
/// stopwatch in the properties panel.
pub const KEYFRAME_HEIGHT: f32 = 12.0;

/// How wide and tall the place is that takes hold of a keyframe, in points: a little more than the keyframe.
pub const KEYFRAME_GRIP_SIZE: f32 = 14.0;

/// How many points one side of the picture of the grain is.
pub const GRAIN_SIZE: usize = 64;

/// How dark the grain is, as the least and most light in a grain of it (of 255): dark grey to very dark grey, nearly black.
const DARKEST_GRAIN: u8 = 8;
const LIGHTEST_GRAIN: u8 = 46;

/// The picture the grain of a rack is made of, which is repeated across it: noise that is the same whenever it is made,
/// and goes on across the place where one repeat meets the next (every point is as random as the others).
pub fn grain_image() -> ColorImage {
    let mut state: u32 = 0x9E37_79B9;
    let mut next_grey = || {
        // (A small generator of numbers that look random: xorshift32.)
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let fraction = (state >> 8) as f32 / (1u32 << 24) as f32;
        // Mostly dark, now and then lighter: squaring the fraction makes the lighter specks fewer.
        let grey = f32::from(DARKEST_GRAIN) + (fraction * fraction) * f32::from(LIGHTEST_GRAIN - DARKEST_GRAIN);
        grey.round() as u8
    };
    let pixels = (0..GRAIN_SIZE * GRAIN_SIZE)
        .map(|_| {
            let grey = next_grey();
            Color32::from_rgb(grey, grey, grey.saturating_add(2))
        })
        .collect();
    ColorImage {
        size: [GRAIN_SIZE, GRAIN_SIZE],
        source_size: vec2(GRAIN_SIZE as f32, GRAIN_SIZE as f32),
        pixels,
    }
}

/// Which corners of a rack are round: its ends are round where they are the ends of the drawer it is in (`left_end_is_cut`
/// and `right_end_is_cut` say that they are not: the drawer goes on beyond what can be seen of it, and so the rack does).
pub fn corners(left_end_is_cut: bool, right_end_is_cut: bool) -> CornerRadius {
    let (left, right) = (
        if left_end_is_cut { 0 } else { RACK_CORNER_RADIUS },
        if right_end_is_cut { 0 } else { RACK_CORNER_RADIUS },
    );
    CornerRadius {
        nw: left,
        sw: left,
        ne: right,
        se: right,
    }
}

/// The pieces that the grain on `area` is drawn in, which go with the repeats of the picture of it that begin at `origin`:
/// where each piece is on the screen, and which part of the picture goes in it (as fractions of its width and height). Each
/// piece is a part of one repeat of the picture, so that it does not matter how the texture is told to go on past its edges.
pub fn grain_pieces(area: Rect, origin: Pos2) -> Vec<(Rect, Rect)> {
    let size = GRAIN_SIZE as f32;
    let mut pieces = Vec::new();
    if area.width() <= 0.0 || area.height() <= 0.0 {
        return pieces;
    }
    let first_column = ((area.left() - origin.x) / size).floor();
    let first_row = ((area.top() - origin.y) / size).floor();
    let mut row = first_row;
    while origin.y + row * size < area.bottom() {
        let (top, bottom) = (
            (origin.y + row * size).max(area.top()),
            (origin.y + (row + 1.0) * size).min(area.bottom()),
        );
        let mut column = first_column;
        while origin.x + column * size < area.right() {
            let (left, right) = (
                (origin.x + column * size).max(area.left()),
                (origin.x + (column + 1.0) * size).min(area.right()),
            );
            let repeat = pos2(origin.x + column * size, origin.y + row * size);
            pieces.push((
                Rect::from_min_max(pos2(left, top), pos2(right, bottom)),
                Rect::from_min_max(
                    pos2((left - repeat.x) / size, (top - repeat.y) / size),
                    pos2((right - repeat.x) / size, (bottom - repeat.y) / size),
                ),
            ));
            column += 1.0;
        }
        row += 1.0;
    }
    pieces
}

/// Draws a rack in `visible`, with the grain texture `grain` that begins at `grain_origin` (so that the grain goes along
/// with the rack when the timeline is scrolled) and has the shading of a tray that is sunk in.
pub fn draw_rack(painter: &Painter, visible: Rect, corners: CornerRadius, grain: TextureId, grain_origin: Pos2) {
    // The ends, which are round, are of the colour that the grain has on average, and between them the grain is as it is.
    painter.rect_filled(visible, corners, theme::RACK_BACKGROUND);
    let inset = f32::from(corners.nw.max(corners.ne).max(corners.sw).max(corners.se));
    let between_the_ends = Rect::from_min_max(
        pos2(visible.left() + f32::from(corners.nw.max(corners.sw)), visible.top()),
        pos2(
            visible.right() - f32::from(corners.ne.max(corners.se)),
            visible.bottom(),
        ),
    );
    let mut grained = Mesh::with_texture(grain);
    for (piece, uv) in grain_pieces(between_the_ends, grain_origin) {
        grained.add_rect_with_uv(piece, uv, Color32::WHITE);
    }
    painter.add(Shape::mesh(grained));
    // The shade of the top edge, which is sunk in, and the light on the bottom edge that faces it: between the corners,
    // so that nothing is drawn outside them.
    let across = (visible.left() + inset)..=(visible.right() - inset);
    if across.start() < across.end() {
        let (left, right) = (*across.start(), *across.end());
        let mut mesh = Mesh::default();
        let shade_height = (visible.height() * 0.5).min(7.0);
        let top = visible.top();
        let (dark, clear) = (Color32::from_black_alpha(150), Color32::TRANSPARENT);
        for (x, colour_top, colour_bottom) in [(left, dark, clear), (right, dark, clear)] {
            mesh.vertices.push(Vertex {
                pos: pos2(x, top),
                uv: egui::epaint::WHITE_UV,
                color: colour_top,
            });
            mesh.vertices.push(Vertex {
                pos: pos2(x, top + shade_height),
                uv: egui::epaint::WHITE_UV,
                color: colour_bottom,
            });
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(1, 2, 3);
        painter.add(Shape::mesh(mesh));
        let bottom = visible.bottom() - 0.5;
        painter.line_segment(
            [pos2(left, bottom), pos2(right, bottom)],
            Stroke::new(1.0, Color32::from_white_alpha(34)),
        );
    }
    painter.rect_stroke(
        visible,
        corners,
        Stroke::new(1.0, Color32::from_black_alpha(170)),
        StrokeKind::Inside,
    );
}

/// Draws the outline of a rack that is selected, in the colour that the outline of a selected clip is, inside the rack's edge
/// (`visible` is where the rack is, and `corners` how round its corners are).
pub fn draw_the_outline_of_a_selected_rack(painter: &Painter, visible: Rect, corners: CornerRadius) {
    painter.rect_stroke(
        visible,
        corners,
        Stroke::new(SELECTED_RACK_OUTLINE_WIDTH, theme::SELECTION_OUTLINE),
        StrokeKind::Inside,
    );
}

/// How thick the outline of a selected rack is, in points: a rack is only as tall as a keyframe, so it is thinner than the
/// outline of a clip.
pub const SELECTED_RACK_OUTLINE_WIDTH: f32 = 2.0;

/// The four corners of the diamond of a keyframe whose middle is at `centre`: a square turned a quarter of a turn, as tall
/// as [`KEYFRAME_HEIGHT`], from the top one clockwise.
pub fn diamond_corners(centre: Pos2) -> [Pos2; 4] {
    let half = KEYFRAME_HEIGHT / 2.0;
    [
        pos2(centre.x, centre.y - half),
        pos2(centre.x + half, centre.y),
        pos2(centre.x, centre.y + half),
        pos2(centre.x - half, centre.y),
    ]
}

/// Draws the diamond of a keyframe whose middle is at `centre`: bright orange, with its upper left side lighter and its lower
/// right side darker, so that it stands out of the rack like a gem. A selected one has an outline like that of a selected
/// clip round it; one that the pointer is over is brighter.
pub fn draw_keyframe(painter: &Painter, centre: Pos2, is_selected: bool, is_hovered: bool) {
    let [top, right, bottom, left] = diamond_corners(centre);
    if is_selected {
        let halo = diamond_corners_grown(centre, 2.5);
        painter.add(Shape::convex_polygon(
            halo.to_vec(),
            theme::SELECTION_OUTLINE,
            Stroke::NONE,
        ));
    }
    let base = if is_hovered {
        brighter(theme::KEYFRAME_BUTTON_ACTIVE)
    } else {
        theme::KEYFRAME_BUTTON_ACTIVE
    };
    painter.add(Shape::convex_polygon(
        vec![top, right, bottom, left],
        base,
        Stroke::new(1.0, Color32::from_rgb(120, 56, 0)),
    ));
    // The upper left half catches the light and the lower right half is in the shade of it.
    painter.add(Shape::convex_polygon(
        vec![top, centre, left],
        Color32::from_white_alpha(70),
        Stroke::NONE,
    ));
    painter.add(Shape::convex_polygon(
        vec![right, bottom, centre],
        Color32::from_black_alpha(60),
        Stroke::NONE,
    ));
}

/// The corners of a diamond like that of [`diamond_corners`] that is `by` points more across from the middle.
fn diamond_corners_grown(centre: Pos2, by: f32) -> [Pos2; 4] {
    let half = KEYFRAME_HEIGHT / 2.0 + by;
    [
        pos2(centre.x, centre.y - half),
        pos2(centre.x + half, centre.y),
        pos2(centre.x, centre.y + half),
        pos2(centre.x - half, centre.y),
    ]
}

fn brighter(colour: Color32) -> Color32 {
    let lift = |channel: u8| channel.saturating_add(50);
    Color32::from_rgb(lift(colour.r()), lift(colour.g()), lift(colour.b()))
}

/// The place that takes hold of the keyframe whose middle is at `centre`.
pub fn keyframe_grip(centre: Pos2) -> Rect {
    Rect::from_center_size(centre, vec2(KEYFRAME_GRIP_SIZE, KEYFRAME_GRIP_SIZE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grain_is_always_the_same_and_dark_and_not_all_one_grey() {
        let first = grain_image();
        let second = grain_image();
        assert_eq!(first.pixels, second.pixels, "the same every time");
        assert_eq!(first.size, [GRAIN_SIZE, GRAIN_SIZE]);
        let greys: Vec<u8> = first.pixels.iter().map(|pixel| pixel.r()).collect();
        assert!(
            greys.iter().all(|grey| (DARKEST_GRAIN..=LIGHTEST_GRAIN).contains(grey)),
            "dark grey to nearly black"
        );
        let mut distinct = greys.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert!(
            distinct.len() > 15,
            "noise, not a flat colour: {} greys",
            distinct.len()
        );
        let average = greys.iter().map(|grey| f32::from(*grey)).sum::<f32>() / greys.len() as f32;
        assert!(average < 25.0, "mostly dark: {average}");
    }

    #[test]
    fn the_grain_has_no_seam_where_one_repeat_meets_the_next() {
        // The columns and rows at the two edges of the picture are as different from each other as any two are from
        // their neighbours: no picture of this kind could have a row that is a copy of the one beside it, but a
        // picture made to join up badly would.
        let image = grain_image();
        let at = |x: usize, y: usize| i32::from(image.pixels[y * GRAIN_SIZE + x].r());
        let difference_between = |first: usize, second: usize| -> f32 {
            (0..GRAIN_SIZE)
                .map(|y| (at(first, y) - at(second, y)).abs() as f32)
                .sum::<f32>()
                / GRAIN_SIZE as f32
        };
        let across_the_seam = difference_between(GRAIN_SIZE - 1, 0);
        let inside = difference_between(10, 11);
        assert!(
            (across_the_seam - inside).abs() < 4.0,
            "across the seam {across_the_seam}, inside {inside}"
        );
    }

    #[test]
    fn the_grain_pieces_cover_the_area_exactly_once_and_each_is_a_part_of_one_repeat_of_the_picture() {
        let origin = pos2(-1500.3, 100.0);
        let area = Rect::from_min_max(pos2(180.0, 100.0), pos2(1246.0, 116.0));

        let pieces = grain_pieces(area, origin);

        let total: f32 = pieces
            .iter()
            .map(|(rectangle, _)| rectangle.width() * rectangle.height())
            .sum();
        assert!(
            (total - area.width() * area.height()).abs() < 0.5,
            "{total} against {}",
            area.width() * area.height()
        );
        for (rectangle, uv) in &pieces {
            assert!(
                area.expand(1e-3).contains_rect(*rectangle),
                "{rectangle:?} sticks out of {area:?}"
            );
            assert!(
                uv.min.x >= -1e-4 && uv.min.y >= -1e-4 && uv.max.x <= 1.0 + 1e-4 && uv.max.y <= 1.0 + 1e-4,
                "{uv:?} is not inside one repeat"
            );
            let size = GRAIN_SIZE as f32;
            assert!(
                (uv.width() * size - rectangle.width()).abs() < 1e-2,
                "a point of the screen is a point of the picture"
            );
            assert!((uv.height() * size - rectangle.height()).abs() < 1e-2);
        }
        for pair in pieces.windows(2) {
            assert!(
                (pair[0].0.right() - pair[1].0.left()).abs() < 1e-3,
                "one after another with no gap and no overlap"
            );
        }
        // The picture goes on from where one piece ends to where the next begins.
        let joint = pieces[0].1.max.x;
        assert!(
            (joint - 1.0).abs() < 1e-3,
            "the first piece goes to the end of its repeat: {joint}"
        );
        assert!(
            pieces[1].1.min.x.abs() < 1e-3,
            "and the next begins at the start of the next"
        );
    }

    #[test]
    fn grain_that_goes_with_the_rack_stays_with_it_when_the_strip_is_scrolled() {
        let origin = pos2(-1500.0, 100.0);
        let area = Rect::from_min_max(pos2(180.0, 100.0), pos2(300.0, 116.0));
        let moved_by = 64.0 * 3.0;
        let scrolled_origin = pos2(origin.x - moved_by, origin.y);
        let before: Vec<Rect> = grain_pieces(area, origin).into_iter().map(|(_, uv)| uv).collect();
        let after: Vec<Rect> = grain_pieces(area.translate(vec2(0.0, 0.0)), scrolled_origin)
            .into_iter()
            .map(|(_, uv)| uv)
            .collect();
        assert_eq!(before, after, "whole repeats away, it is the same grain");
    }

    #[test]
    fn nothing_is_drawn_for_an_area_with_no_room() {
        assert!(grain_pieces(Rect::from_min_max(pos2(5.0, 5.0), pos2(5.0, 20.0)), Pos2::ZERO).is_empty());
        assert!(grain_pieces(Rect::NOTHING, Pos2::ZERO).is_empty());
    }

    #[test]
    fn the_ends_of_a_rack_are_round_where_the_drawer_ends_and_square_where_it_is_cut_by_the_view() {
        let round = corners(false, false);
        assert_eq!(
            (round.nw, round.sw, round.ne, round.se),
            (
                RACK_CORNER_RADIUS,
                RACK_CORNER_RADIUS,
                RACK_CORNER_RADIUS,
                RACK_CORNER_RADIUS
            )
        );
        let cut_at_the_left = corners(true, false);
        assert_eq!((cut_at_the_left.nw, cut_at_the_left.sw), (0, 0));
        assert_eq!(
            (cut_at_the_left.ne, cut_at_the_left.se),
            (RACK_CORNER_RADIUS, RACK_CORNER_RADIUS)
        );
        let cut_at_the_right = corners(false, true);
        assert_eq!((cut_at_the_right.ne, cut_at_the_right.se), (0, 0));
    }

    #[test]
    fn a_rack_is_not_a_capsule_it_has_a_straight_edge_at_each_end() {
        let rack_height = theme::RACK_HEIGHT;
        assert!(
            f32::from(RACK_CORNER_RADIUS) * 2.0 < rack_height,
            "less than half of its height each way"
        );
        assert!(
            rack_height - 2.0 * f32::from(RACK_CORNER_RADIUS) >= 6.0,
            "some straight edge is left at each end"
        );
    }

    #[test]
    fn a_keyframe_is_a_square_turned_a_quarter_as_tall_as_the_stopwatch_and_centred_where_it_is() {
        let centre = pos2(100.0, 50.0);
        let [top, right, bottom, left] = diamond_corners(centre);
        assert_eq!((top.x, bottom.x), (100.0, 100.0));
        assert_eq!((left.y, right.y), (50.0, 50.0));
        assert_eq!(bottom.y - top.y, KEYFRAME_HEIGHT);
        assert_eq!(right.x - left.x, KEYFRAME_HEIGHT, "as wide as it is tall");
        // All four sides are as long as each other (a square), and the two diagonals as long (a right angle).
        let side = |from: Pos2, to: Pos2| from.distance(to);
        let sides = [
            side(top, right),
            side(right, bottom),
            side(bottom, left),
            side(left, top),
        ];
        assert!(sides.iter().all(|length| (length - sides[0]).abs() < 1e-4), "{sides:?}");
        assert!((sides[0] - KEYFRAME_HEIGHT / 2.0 * 2.0_f32.sqrt()).abs() < 1e-4);
        assert!((top.distance(bottom) - left.distance(right)).abs() < 1e-4);
    }

    #[test]
    fn the_place_that_takes_hold_of_a_keyframe_is_all_of_it_and_a_little_more() {
        let centre = pos2(100.0, 50.0);
        let grip = keyframe_grip(centre);
        assert_eq!(grip.center(), centre);
        assert!(grip.width() >= KEYFRAME_HEIGHT && grip.height() >= KEYFRAME_HEIGHT);
        for corner in diamond_corners(centre) {
            assert!(grip.contains(corner));
        }
    }

    #[test]
    fn a_keyframe_is_about_as_tall_as_a_rack_but_a_rack_is_only_barely_taller() {
        // (They are constants, so this is checked when the program is built.)
        const {
            assert!(KEYFRAME_HEIGHT < theme::RACK_HEIGHT);
            assert!(theme::RACK_HEIGHT - KEYFRAME_HEIGHT <= 6.0, "only barely taller");
        }
    }
}
