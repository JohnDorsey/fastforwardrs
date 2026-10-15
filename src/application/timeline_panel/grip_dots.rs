// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Grip dots: a few tiny bumps, like braille, that say that what they are on can be taken hold of and dragged: the
//! top of the box of a track, the top of a clip, the left end of a rack of keyframes.
//!
//! They are not painted. Each is a little hemisphere lit from the upper left, and what is drawn is only the light and
//! the shade that would fall on it (white and black that fade in and out), over whatever is there: a box, a picture of
//! a video, a rack. So they look raised out of it, whatever colour it is.

use std::sync::LazyLock;

use egui::epaint::Vertex;
use egui::{Color32, Mesh, Painter, Pos2, Shape, Vec2, vec2};

/// How far beyond its edge a dot fades out, as a fraction of its radius, so that its edge is not jagged.
const FEATHER: f32 = 0.3;

/// How many corners the edge of a dot has.
const CORNERS: usize = 12;

/// How far from the middle of a dot to its edge each ring of its mesh is, as a fraction of its radius. (The last
/// is the edge of what fades out.)
const RINGS: [f32; 4] = [0.0, 0.55, 1.0, 1.0 + FEATHER];

/// Where the light comes from, as a direction pointing at it: from the upper left (the screen's y is down), and
/// from in front of the screen.
const TOWARDS_THE_LIGHT: [f32; 3] = [-0.5, -0.6, 0.62];

/// How much of the light shows where a dot faces it more than flat ground would, and how much of the shade where it
/// faces it less, and the most that either covers (from 0, nothing, to 1, as white or as black as can be).
const LIGHT_GAIN: f32 = 2.4;
const SHADE_GAIN: f32 = 0.3;
const MOST_LIGHT: f32 = 0.85;
const MOST_SHADE: f32 = 0.5;

/// How many vertices the mesh of one dot has.
#[cfg(test)]
pub const VERTICES_PER_DOT: usize = 1 + (RINGS.len() - 1) * CORNERS;

/// How many dots there are, in how many columns and rows, how big each is and how far they are from one another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arrangement {
    pub columns: u8,
    pub rows: u8,
    /// How far across a dot is, from the middle of it to its edge, in points.
    pub radius: f32,
    /// How far it is from the middle of a dot to the middle of the next, in points.
    pub pitch: f32,
}

impl Arrangement {
    /// On the top of the box of a track: two rows of three.
    pub const FOR_A_TRACK_BOX: Self = Self {
        columns: 3,
        rows: 2,
        radius: 2.0,
        pitch: 6.0,
    };
    /// At the left end of a rack of keyframes: three rows of two. (A rack is not much taller than a keyframe, so these
    /// are smaller than the others to fit.)
    pub const FOR_A_RACK: Self = Self {
        columns: 2,
        rows: 3,
        radius: 1.5,
        pitch: 4.5,
    };
    /// On the top of a clip: two rows of five.
    pub const FOR_A_CLIP: Self = Self {
        columns: 5,
        rows: 2,
        radius: 2.0,
        pitch: 6.0,
    };

    /// How many dots there are.
    #[cfg(test)]
    pub fn count(self) -> usize {
        usize::from(self.columns) * usize::from(self.rows)
    }

    /// How much room the dots take, from the edge of the first to the edge of the last.
    pub fn size(self) -> Vec2 {
        vec2(
            f32::from(self.columns.saturating_sub(1)) * self.pitch + 2.0 * self.radius,
            f32::from(self.rows.saturating_sub(1)) * self.pitch + 2.0 * self.radius,
        )
    }

    /// Where the middles of the dots are, row by row from the top left, when the dots as a whole are centred on `middle`.
    pub fn centres_round(self, middle: Pos2) -> Vec<Pos2> {
        let pitch = self.pitch;
        let first = middle
            - vec2(
                f32::from(self.columns.saturating_sub(1)) * pitch / 2.0,
                f32::from(self.rows.saturating_sub(1)) * pitch / 2.0,
            );
        (0..self.rows)
            .flat_map(|row| {
                (0..self.columns).map(move |column| first + vec2(f32::from(column) * pitch, f32::from(row) * pitch))
            })
            .collect()
    }
}

/// What a point of a dot is covered by, which is the light or the shade that falls on it: `across` and `down` are how
/// far it is from the middle, as fractions of the radius (so the edge is where they come to 1 together).
fn light_and_shade_at(across: f32, down: f32) -> Color32 {
    let height = (1.0 - across * across - down * down).max(0.0).sqrt();
    let [light_across, light_down, light_height] = TOWARDS_THE_LIGHT;
    let length = (light_across * light_across + light_down * light_down + light_height * light_height).sqrt();
    let facing_the_light = (across * light_across + down * light_down + height * light_height) / length;
    // What flat ground would have, so that the top of the bump, which is flat, is not covered by anything.
    let flat_ground = light_height / length;
    let difference = facing_the_light - flat_ground;
    if difference >= 0.0 {
        let alpha = (difference * LIGHT_GAIN).clamp(0.0, MOST_LIGHT);
        let level = (alpha * 255.0).round() as u8;
        Color32::from_rgba_premultiplied(level, level, level, level)
    } else {
        let alpha = (-difference * SHADE_GAIN).clamp(0.0, MOST_SHADE);
        Color32::from_rgba_premultiplied(0, 0, 0, (alpha * 255.0).round() as u8)
    }
}

/// The vertices of a dot centred on (0, 0) with a radius of 1, and the triangles between them, which are the same for
/// every dot (a bigger one is this made bigger).
static DOT: LazyLock<(Vec<Vertex>, Vec<u32>)> = LazyLock::new(|| {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(Pos2::ZERO, light_and_shade_at(0.0, 0.0));
    for (ring, fraction) in RINGS.iter().copied().enumerate().skip(1) {
        let is_the_feathered_edge = ring == RINGS.len() - 1;
        for corner in 0..CORNERS {
            let angle = std::f32::consts::TAU * corner as f32 / CORNERS as f32;
            let (down, across) = angle.sin_cos();
            // (How far the ring is from the middle is where the shade is worked out, but past the edge there is none.)
            let colour = if is_the_feathered_edge {
                Color32::TRANSPARENT
            } else {
                light_and_shade_at(across * fraction, down * fraction)
            };
            mesh.colored_vertex((vec2(across, down) * fraction).to_pos2(), colour);
        }
    }
    let ring_start = |ring: usize| 1 + (ring - 1) * CORNERS;
    for corner in 0..CORNERS {
        let next = (corner + 1) % CORNERS;
        // The first ring is joined to the middle.
        mesh.add_triangle(0, (ring_start(1) + corner) as u32, (ring_start(1) + next) as u32);
        // And each ring to the one outside it, in two triangles for each corner.
        for ring in 1..RINGS.len() - 1 {
            let (inner, outer) = (ring_start(ring), ring_start(ring + 1));
            mesh.add_triangle((inner + corner) as u32, (outer + corner) as u32, (outer + next) as u32);
            mesh.add_triangle((inner + corner) as u32, (outer + next) as u32, (inner + next) as u32);
        }
    }
    (mesh.vertices, mesh.indices)
});

/// The mesh of the dots of `arrangement` when they are centred on `middle`: all of them in one.
pub fn mesh_of(arrangement: Arrangement, middle: Pos2) -> Mesh {
    let (vertices, indices) = &*DOT;
    let mut mesh = Mesh::default();
    for centre in arrangement.centres_round(middle) {
        let first = mesh.vertices.len() as u32;
        mesh.vertices.extend(vertices.iter().map(|vertex| Vertex {
            pos: centre + vertex.pos.to_vec2() * arrangement.radius,
            ..*vertex
        }));
        mesh.indices.extend(indices.iter().map(|index| first + index));
    }
    mesh
}

/// Draws the dots of `arrangement` centred on `middle`.
pub fn draw(painter: &Painter, arrangement: Arrangement, middle: Pos2) {
    painter.add(Shape::mesh(mesh_of(arrangement, middle)));
}

#[cfg(test)]
mod tests {
    use egui::{Rect, pos2};

    use super::*;

    #[test]
    fn the_dots_of_a_track_box_a_clip_and_a_rack_are_six_ten_and_six_in_rows_and_columns() {
        assert_eq!(
            (Arrangement::FOR_A_TRACK_BOX.rows, Arrangement::FOR_A_TRACK_BOX.columns),
            (2, 3),
            "two rows of three"
        );
        assert_eq!(
            (Arrangement::FOR_A_RACK.rows, Arrangement::FOR_A_RACK.columns),
            (3, 2),
            "three rows of two"
        );
        assert_eq!(
            (Arrangement::FOR_A_CLIP.rows, Arrangement::FOR_A_CLIP.columns),
            (2, 5),
            "two rows of five"
        );
        assert_eq!(Arrangement::FOR_A_TRACK_BOX.count(), 6);
        assert_eq!(Arrangement::FOR_A_RACK.count(), 6);
        assert_eq!(Arrangement::FOR_A_CLIP.count(), 10);
    }

    #[test]
    fn the_dots_are_in_rows_and_columns_a_pitch_apart_centred_on_the_middle_given() {
        let middle = pos2(100.0, 50.0);
        let centres = Arrangement::FOR_A_CLIP.centres_round(middle);
        assert_eq!(centres.len(), 10);
        // Row by row from the top left: five in the first row, then five in the second.
        let pitch = Arrangement::FOR_A_CLIP.pitch;
        assert_eq!(centres[0], pos2(100.0 - 2.0 * pitch, 50.0 - pitch / 2.0));
        assert_eq!(centres[4], pos2(100.0 + 2.0 * pitch, 50.0 - pitch / 2.0));
        assert_eq!(centres[5], pos2(100.0 - 2.0 * pitch, 50.0 + pitch / 2.0));
        assert_eq!(centres[9], pos2(100.0 + 2.0 * pitch, 50.0 + pitch / 2.0));
        let sum = centres.iter().fold(Vec2::ZERO, |sum, centre| sum + centre.to_vec2());
        assert_eq!(
            sum / 10.0,
            middle.to_vec2(),
            "the middle of all of them is the middle given"
        );
        for pair in centres[..5].windows(2) {
            assert_eq!(pair[1].x - pair[0].x, pitch);
            assert_eq!(pair[1].y, pair[0].y);
        }
    }

    #[test]
    fn the_room_the_dots_take_reaches_from_the_edge_of_the_first_to_the_edge_of_the_last() {
        let Arrangement { pitch, radius, .. } = Arrangement::FOR_A_CLIP;
        let size = Arrangement::FOR_A_CLIP.size();
        assert_eq!(size, vec2(4.0 * pitch + 2.0 * radius, pitch + 2.0 * radius));
        let centres = Arrangement::FOR_A_CLIP.centres_round(Pos2::ZERO);
        let (first, last) = (centres[0], centres[9]);
        assert_eq!(last.x - first.x + 2.0 * radius, size.x);
        assert_eq!(last.y - first.y + 2.0 * radius, size.y);
    }

    #[test]
    fn all_the_dots_are_one_mesh_of_as_many_dots_as_there_are() {
        let mesh = mesh_of(Arrangement::FOR_A_TRACK_BOX, pos2(30.0, 20.0));
        assert_eq!(mesh.vertices.len(), 6 * VERTICES_PER_DOT);
        assert!(mesh.indices.iter().all(|index| (*index as usize) < mesh.vertices.len()));
        assert_eq!(mesh.indices.len() % 3, 0);
        // Every triangle has some area: none of them is a line.
        for triangle in mesh.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|corner| mesh.vertices[triangle[corner] as usize].pos);
            let area = ((b - a).x * (c - a).y - (b - a).y * (c - a).x).abs() / 2.0;
            assert!(area > 1e-4, "{a:?} {b:?} {c:?}");
        }
    }

    #[test]
    fn the_dots_stay_inside_the_room_they_take_and_the_little_that_fades_out_beyond_it() {
        let middle = pos2(30.0, 20.0);
        for arrangement in [
            Arrangement::FOR_A_CLIP,
            Arrangement::FOR_A_TRACK_BOX,
            Arrangement::FOR_A_RACK,
        ] {
            let mesh = mesh_of(arrangement, middle);
            let room = Rect::from_center_size(middle, arrangement.size()).expand(FEATHER * arrangement.radius + 0.01);
            for vertex in &mesh.vertices {
                assert!(room.contains(vertex.pos), "{:?} is outside {room:?}", vertex.pos);
            }
        }
    }

    #[test]
    fn the_dots_of_a_rack_are_smaller_than_the_others_and_fit_in_it_with_room_to_spare() {
        let rack = Arrangement::FOR_A_RACK;
        assert!(rack.radius < Arrangement::FOR_A_CLIP.radius && rack.pitch < Arrangement::FOR_A_CLIP.pitch);
        assert!(
            rack.size().y + 2.0 * 1.5 <= crate::application::theme::RACK_HEIGHT,
            "{:?} in a rack {} points tall",
            rack.size(),
            crate::application::theme::RACK_HEIGHT
        );
    }

    /// The colour of the vertex of the dot (centred on (0, 0)) that is nearest to a point `across` and `down` from its middle.
    fn colour_near(across: f32, down: f32) -> Color32 {
        let (vertices, _) = &*DOT;
        let target = pos2(across, down);
        vertices
            .iter()
            .min_by(|first, second| {
                first
                    .pos
                    .distance(target)
                    .partial_cmp(&second.pos.distance(target))
                    .unwrap()
            })
            .unwrap()
            .color
    }

    #[test]
    fn a_dot_has_no_colour_of_its_own_only_the_light_and_the_shade_that_fall_on_it() {
        let (vertices, _) = &*DOT;
        for vertex in vertices {
            let colour = vertex.color;
            let is_light = colour.r() == colour.a() && colour.g() == colour.a() && colour.b() == colour.a();
            let is_shade = colour.r() == 0 && colour.g() == 0 && colour.b() == 0;
            assert!(
                is_light || is_shade,
                "{colour:?} is neither white nor black that is let through"
            );
        }
    }

    #[test]
    fn the_top_of_a_dot_is_flat_so_nothing_covers_it() {
        assert_eq!(colour_near(0.0, 0.0), Color32::TRANSPARENT);
    }

    #[test]
    fn a_dot_is_lit_on_the_upper_left_and_shaded_on_the_lower_right() {
        let lit = colour_near(-0.5, -0.5);
        let shaded = colour_near(0.7, 0.7);
        assert!(lit.a() > 40 && lit.r() == lit.a(), "{lit:?} is light");
        assert!(shaded.a() > 40 && shaded.r() == 0, "{shaded:?} is shade");
    }

    #[test]
    fn what_faces_the_light_is_lighter_the_nearer_it_is_to_facing_it_straight_on() {
        // The light is at (-0.5, -0.6) of a sphere, so a point there is lighter than one a little way from it.
        let (across, down) = (-0.5 / 0.997, -0.6 / 0.997);
        let on_it = light_and_shade_at(across, down);
        let beside = light_and_shade_at(across + 0.3, down + 0.3);
        assert!(on_it.a() > beside.a(), "{on_it:?} against {beside:?}");
    }

    #[test]
    fn the_edge_of_a_dot_fades_out_so_that_it_is_not_jagged() {
        let (vertices, _) = &*DOT;
        let farthest = vertices
            .iter()
            .map(|vertex| vertex.pos.to_vec2().length())
            .fold(0.0, f32::max);
        assert!((farthest - (1.0 + FEATHER)).abs() < 1e-4);
        for vertex in vertices {
            if vertex.pos.to_vec2().length() > 1.0 + 1e-4 {
                assert_eq!(vertex.color, Color32::TRANSPARENT, "beyond the edge: {:?}", vertex.pos);
            }
        }
    }
}
