//! Drawers on clips: a clip has a drawer under it, which slides open when the wide, thin button over the bottom
//! edge of the clip is clicked, and shut when it is clicked again. Every drawer is empty, for now. They are the place
//! where the keyframes of what is on a clip will be shown, each clip's own, which move along with the clip.
//!
//! A drawer is as wide as its clip and hangs from the bottom of it. Its outer two corners are rounded as much as
//! the clip's are, and the side that faces the clip is straight and goes all the way up to the clip, into the room
//! that the rounding of the clip's corners leaves outside them, so that the clip and the drawer look like one thing.
//! The row of a track is as tall as its drawers need while they are open, which makes the rows under it go down
//! as the drawer slides out.
//!
//! Which drawers are open is not part of the project: it is not saved, and opening or shutting one is not an edit
//! that can be undone.

use std::collections::{HashMap, HashSet};

use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Stroke, pos2, vec2};

use crate::application::theme;
use crate::project::{Clip, ClipId, Project, Track};

/// How wide the button is, and how tall: it is five times as wide as it is tall. (At the least height of a track
/// it covers less than half of the clip, which a test checks.)
pub(super) const BUTTON_WIDTH: f32 = 60.0;
pub(super) const BUTTON_HEIGHT: f32 = 12.0;

/// How far the button keeps from the bottom edge of its clip, in points.
const BUTTON_MARGIN_BELOW: f32 = 3.0;

/// How far the button keeps from the sides of a clip that is narrower than it, in points.
const BUTTON_MARGIN_AT_THE_SIDES: f32 = 10.0;

/// A clip that leaves room for a button narrower than this has no button, because it would be too small to take
/// hold of.
const NARROWEST_BUTTON: f32 = 20.0;

/// How tall a drawer is when it is open, in points.
pub(super) const DRAWER_HEIGHT: f32 = 64.0;

/// How long a drawer takes to slide all the way open (or shut), in seconds.
const SECONDS_TO_SLIDE: f32 = 0.18;

/// How the label of the button of a drawer says whose it is, which the tests (and screen readers) know it by.
pub fn drawer_button_label(track_title: &str, clip: ClipId) -> String {
    format!("Drawer of clip {} of {track_title}", clip.number())
}

/// Whether `clip` has a drawer that can be opened. Every clip has one, for now, empty as it is. (When drawers
/// have something in them, the empty ones may be made inaccessible: here.)
pub(super) fn has_an_accessible_drawer(_clip: &Clip) -> bool {
    true
}

/// Where the button of the drawer of a clip is: over the clip, at its bottom edge, in the middle of the part of it
/// that can be seen (`visible_clip`), so that it can be reached however far the clip goes beyond the view. It is
/// [`BUTTON_WIDTH`] wide, or as wide as the clip leaves room for. `None` if that is too narrow to take hold of.
pub(super) fn button_rectangle(visible_clip: Rect) -> Option<Rect> {
    let width = BUTTON_WIDTH.min(visible_clip.width() - 2.0 * BUTTON_MARGIN_AT_THE_SIDES);
    if width < NARROWEST_BUTTON {
        return None;
    }
    Some(Rect::from_center_size(
        pos2(
            visible_clip.center().x,
            visible_clip.bottom() - BUTTON_MARGIN_BELOW - BUTTON_HEIGHT / 2.0,
        ),
        vec2(width, BUTTON_HEIGHT),
    ))
}

/// What a drawer looks like, which is where it is and how its corners are rounded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DrawerShape {
    pub(super) rectangle: Rect,
    pub(super) corners: CornerRadius,
}

/// The shape of the drawer of the clip that is in `whole_clip` and has its corners rounded by `clip_radius`, when
/// the drawer is `openness` (0 to 1) of the way open. `None` when it is shut.
///
/// The drawer reaches up from the bottom edge of the clip by as much as the corners of the clip are rounded by,
/// into the room outside them (what is of it inside the clip is behind the clip, which is drawn over it), and down
/// from there by as much as it is open. Its two lower corners are rounded like the clip's.
pub(super) fn drawer_shape(whole_clip: Rect, clip_radius: f32, openness: f32) -> Option<DrawerShape> {
    if openness <= 0.0 {
        return None;
    }
    let openness = openness.min(1.0);
    let reach_up = clip_radius * openness;
    let rectangle = Rect::from_min_max(
        pos2(whole_clip.left(), whole_clip.bottom() - reach_up),
        pos2(whole_clip.right(), whole_clip.bottom() + DRAWER_HEIGHT * openness),
    );
    // The lower corners are as round as the clip's, if the drawer is wide enough and tall enough yet for that.
    let radius = clip_radius.min(rectangle.width() / 2.0).max(0.0).round() as u8;
    let radius = radius.min(rectangle.height().floor() as u8);
    Some(DrawerShape {
        rectangle,
        corners: CornerRadius {
            nw: 0,
            ne: 0,
            sw: radius,
            se: radius,
        },
    })
}

/// Which drawers are open, and how far each has slid.
#[derive(Debug, Default)]
pub(super) struct Drawers {
    open: HashSet<ClipId>,
    /// How far open each drawer that is open, or has not finished shutting, is: from 0 (shut) to 1 (open),
    /// evenly with time. (What is seen is [`Self::openness`], which slows down as it arrives.)
    levels: HashMap<ClipId, f32>,
}

impl Drawers {
    pub(super) fn is_open(&self, clip: ClipId) -> bool {
        self.open.contains(&clip)
    }

    /// Opens the drawer of `clip` if it is shut, and shuts it if it is open: from where it is now, if it was sliding.
    pub(super) fn toggle(&mut self, clip: ClipId) {
        if !self.open.remove(&clip) {
            self.open.insert(clip);
        }
        self.levels.entry(clip).or_insert(0.0);
    }

    /// Slides the drawers that are open or shutting on, for a frame of the window that took `seconds`. Returns whether
    /// any of them is still sliding, which is when the window has to be drawn again soon.
    pub(super) fn advance(&mut self, seconds: f32) -> bool {
        let step = seconds / SECONDS_TO_SLIDE;
        let open = &self.open;
        let mut is_sliding = false;
        self.levels.retain(|clip, level| {
            let target = if open.contains(clip) { 1.0 } else { 0.0 };
            *level = if *level < target {
                (*level + step).min(target)
            } else {
                (*level - step).max(target)
            };
            is_sliding |= *level != target;
            // A shut drawer that has finished shutting is no longer remembered.
            *level > 0.0 || target > 0.0
        });
        is_sliding
    }

    /// How far open the drawer of `clip` is as it is seen, from 0 to 1: it starts quickly and slows as it arrives.
    pub(super) fn openness(&self, clip: ClipId) -> f32 {
        ease_out(self.levels.get(&clip).copied().unwrap_or(0.0))
    }

    /// How much taller than the track itself the row of `track` is for the drawers that are open: as much as the
    /// one that is most open needs.
    pub(super) fn extra_height_of(&self, track: &Track) -> f32 {
        if self.levels.is_empty() {
            return 0.0;
        }
        track
            .placements()
            .filter(|placement| has_an_accessible_drawer(placement.clip))
            .map(|placement| self.openness(placement.id) * DRAWER_HEIGHT)
            .fold(0.0, f32::max)
    }

    /// Forgets the drawers of clips that are not in `project` any more.
    pub(super) fn forget_what_is_gone(&mut self, project: &Project) {
        let is_there = |clip: &ClipId| project.placement_of(*clip).is_some();
        self.open.retain(is_there);
        self.levels.retain(|clip, _| is_there(clip));
    }
}

/// A number from 0 to 1 that begins quickly and slows as it gets to 1, as `progress` (from 0 to 1) goes evenly.
fn ease_out(progress: f32) -> f32 {
    let remaining = 1.0 - progress.clamp(0.0, 1.0);
    1.0 - remaining * remaining * remaining
}

/// Draws the drawer of a clip in `shape`, which is behind the clip (it is drawn before the clip is).
pub(super) fn draw_drawer(painter: &Painter, shape: DrawerShape) {
    painter.rect_filled(shape.rectangle, shape.corners, theme::DRAWER_BACKGROUND);
}

/// Draws the button in `rectangle`: a pill that shows what is behind it dimmed, with an arrow in it that points down
/// when the drawer is shut (it is pulled down to open it) and up when it is open.
pub(super) fn draw_button(painter: &Painter, rectangle: Rect, is_open: bool, is_hovered: bool) {
    let (fill, outline) = if is_hovered {
        (theme::DRAWER_BUTTON_FILL_HOVERED, theme::DRAWER_BUTTON_OUTLINE_HOVERED)
    } else {
        (theme::DRAWER_BUTTON_FILL, theme::DRAWER_BUTTON_OUTLINE)
    };
    let radius = CornerRadius::same((rectangle.height() / 2.0).round() as u8);
    painter.rect_filled(rectangle, radius, fill);
    painter.rect_stroke(rectangle, radius, Stroke::new(1.0, outline), egui::StrokeKind::Inside);
    draw_arrow(painter, rectangle.center(), is_open, theme::DRAWER_BUTTON_ARROW);
}

/// Draws a small arrowhead at `centre` that points up if `points_up` and down if not.
fn draw_arrow(painter: &Painter, centre: Pos2, points_up: bool, colour: Color32) {
    let (half_width, half_height) = (4.0, 1.75);
    let side = if points_up { half_height } else { -half_height };
    let stroke = Stroke::new(1.5, colour);
    painter.line_segment(
        [
            pos2(centre.x - half_width, centre.y + side),
            pos2(centre.x, centre.y - side),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(centre.x, centre.y - side),
            pos2(centre.x + half_width, centre.y + side),
        ],
        stroke,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::TrackHeight;

    fn clip(number: u32) -> ClipId {
        ClipId::new(crate::project::TrackId::new(0), number)
    }

    // --- the button ---

    #[test]
    fn the_button_is_five_times_as_wide_as_it_is_tall() {
        assert_eq!(BUTTON_WIDTH / BUTTON_HEIGHT, 5.0);
    }

    #[test]
    fn the_button_covers_less_than_half_of_a_clip_even_at_the_least_height_of_a_track() {
        let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, TrackHeight::SMALLEST_IN_POINTS));
        let button = button_rectangle(clip_rectangle).unwrap();
        assert!(
            button.height() < clip_rectangle.height() / 2.0,
            "{button:?} on {clip_rectangle:?}"
        );
    }

    #[test]
    fn the_button_is_over_the_clip_at_its_bottom_edge_in_the_middle() {
        let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0));
        let button = button_rectangle(clip_rectangle).unwrap();
        assert_eq!(button.size(), vec2(BUTTON_WIDTH, BUTTON_HEIGHT));
        assert_eq!(button.center().x, 250.0, "in the middle");
        assert!(clip_rectangle.contains_rect(button), "over the clip");
        assert_eq!(
            clip_rectangle.bottom() - button.bottom(),
            BUTTON_MARGIN_BELOW,
            "at its bottom edge"
        );
    }

    #[test]
    fn a_clip_that_is_too_narrow_for_the_whole_button_has_a_smaller_one_and_a_very_narrow_clip_has_none() {
        let narrow = Rect::from_min_size(pos2(0.0, 0.0), vec2(50.0, 104.0));
        let button = button_rectangle(narrow).unwrap();
        assert_eq!(button.width(), 30.0, "room is kept at both sides");
        assert!(narrow.contains_rect(button));
        let very_narrow = Rect::from_min_size(pos2(0.0, 0.0), vec2(35.0, 104.0));
        assert_eq!(button_rectangle(very_narrow), None);
        assert_eq!(
            button_rectangle(Rect::from_min_size(pos2(0.0, 0.0), vec2(0.0, 104.0))),
            None
        );
    }

    // --- the drawer ---

    #[test]
    fn a_drawer_that_is_shut_is_not_there() {
        let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0));
        assert_eq!(drawer_shape(clip_rectangle, 27.0, 0.0), None);
        assert_eq!(drawer_shape(clip_rectangle, 27.0, -0.5), None);
    }

    #[test]
    fn an_open_drawer_is_as_wide_as_its_clip_and_hangs_from_the_bottom_of_it_reaching_up_into_the_room_by_its_corners()
    {
        let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0));
        let shape = drawer_shape(clip_rectangle, 27.0, 1.0).unwrap();
        assert_eq!(shape.rectangle.left(), 100.0);
        assert_eq!(shape.rectangle.right(), 400.0);
        assert_eq!(
            shape.rectangle.top(),
            154.0 - 27.0,
            "up to where the corners of the clip are rounded away"
        );
        assert_eq!(shape.rectangle.bottom(), 154.0 + DRAWER_HEIGHT);
    }

    #[test]
    fn the_two_lower_corners_of_a_drawer_are_rounded_like_the_clip_and_the_two_that_face_the_clip_are_square() {
        let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0));
        let corners = drawer_shape(clip_rectangle, 27.0, 1.0).unwrap().corners;
        assert_eq!(
            corners,
            CornerRadius {
                nw: 0,
                ne: 0,
                sw: 27,
                se: 27
            }
        );
    }

    #[test]
    fn a_drawer_that_has_hardly_opened_has_corners_no_rounder_than_it_is_tall() {
        let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0));
        let shape = drawer_shape(clip_rectangle, 27.0, 0.05).unwrap();
        assert!(shape.rectangle.height() < 10.0);
        assert!(f32::from(shape.corners.sw) <= shape.rectangle.height());
        assert_eq!(shape.corners.sw, shape.corners.se);
    }

    #[test]
    fn a_drawer_grows_as_it_opens_in_height_and_in_how_far_it_reaches_up() {
        let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0));
        let mut last = drawer_shape(clip_rectangle, 27.0, 0.01).unwrap().rectangle;
        for step in 2..=20 {
            let now = drawer_shape(clip_rectangle, 27.0, step as f32 / 20.0)
                .unwrap()
                .rectangle;
            assert!(
                now.bottom() > last.bottom() && now.top() < last.top(),
                "{last:?} then {now:?}"
            );
            last = now;
        }
    }

    #[test]
    fn the_corners_of_a_drawer_of_a_narrow_clip_are_no_rounder_than_half_its_width() {
        let narrow = Rect::from_min_size(pos2(100.0, 50.0), vec2(30.0, 104.0));
        let shape = drawer_shape(narrow, 15.0, 1.0).unwrap();
        assert_eq!(shape.corners.sw, 15);
        let very_narrow = Rect::from_min_size(pos2(100.0, 50.0), vec2(20.0, 104.0));
        assert_eq!(drawer_shape(very_narrow, 27.0, 1.0).unwrap().corners.sw, 10);
    }

    // --- which are open ---

    fn advance_for(drawers: &mut Drawers, seconds: f32) -> bool {
        let frames = (seconds * 60.0).round() as usize;
        (0..frames).fold(false, |_, _| drawers.advance(1.0 / 60.0))
    }

    #[test]
    fn a_drawer_is_shut_to_begin_with_and_toggling_opens_it_and_shuts_it_again() {
        let mut drawers = Drawers::default();
        assert!(!drawers.is_open(clip(1)));
        assert_eq!(drawers.openness(clip(1)), 0.0);
        drawers.toggle(clip(1));
        assert!(drawers.is_open(clip(1)));
        drawers.toggle(clip(1));
        assert!(!drawers.is_open(clip(1)));
        assert!(!drawers.is_open(clip(2)), "the drawer of another clip is not touched");
    }

    #[test]
    fn a_drawer_slides_open_in_a_quick_animation_and_is_then_still() {
        let mut drawers = Drawers::default();
        drawers.toggle(clip(1));
        let mut seen = vec![drawers.openness(clip(1))];
        let mut frames_sliding = 0;
        loop {
            let is_sliding = drawers.advance(1.0 / 60.0);
            seen.push(drawers.openness(clip(1)));
            if !is_sliding {
                break;
            }
            frames_sliding += 1;
            assert!(frames_sliding < 60, "it never arrives: {seen:?}");
        }
        assert_eq!(seen[0], 0.0);
        assert_eq!(*seen.last().unwrap(), 1.0);
        assert!(
            seen.windows(2).all(|pair| pair[1] >= pair[0]),
            "it only opens: {seen:?}"
        );
        assert!(seen[1] > 0.0 && seen[1] < 0.5, "it is not open at once: {seen:?}");
        assert!((6..=20).contains(&frames_sliding), "quick: {frames_sliding} frames");
        assert!(!drawers.advance(1.0 / 60.0), "still, once it is open");
    }

    #[test]
    fn it_starts_quickly_and_slows_as_it_arrives() {
        let mut drawers = Drawers::default();
        drawers.toggle(clip(1));
        let mut openness = vec![0.0];
        for _ in 0..11 {
            drawers.advance(1.0 / 60.0);
            openness.push(drawers.openness(clip(1)));
        }
        let first_step = openness[1] - openness[0];
        let last_step = openness[11] - openness[10];
        assert!(first_step > 2.0 * last_step, "{openness:?}");
    }

    #[test]
    fn a_drawer_shuts_the_same_way_and_is_forgotten_when_it_has_shut() {
        let mut drawers = Drawers::default();
        drawers.toggle(clip(1));
        advance_for(&mut drawers, 0.5);
        assert_eq!(drawers.openness(clip(1)), 1.0);
        drawers.toggle(clip(1));
        assert!(drawers.advance(1.0 / 60.0), "sliding shut");
        assert!(drawers.openness(clip(1)) < 1.0);
        advance_for(&mut drawers, 0.5);
        assert_eq!(drawers.openness(clip(1)), 0.0);
        assert!(
            drawers.levels.is_empty(),
            "nothing is remembered of a drawer that is shut"
        );
        assert!(!drawers.advance(1.0 / 60.0));
    }

    #[test]
    fn a_drawer_toggled_while_it_slides_goes_back_from_where_it_is() {
        let mut drawers = Drawers::default();
        drawers.toggle(clip(1));
        advance_for(&mut drawers, 0.09);
        let half_way = drawers.openness(clip(1));
        assert!(half_way > 0.3 && half_way < 1.0, "{half_way}");
        drawers.toggle(clip(1));
        drawers.advance(1.0 / 60.0);
        let after = drawers.openness(clip(1));
        assert!(
            after < half_way && after > 0.0,
            "it turns round where it is: {half_way} then {after}"
        );
    }

    #[test]
    fn a_long_frame_goes_as_far_as_the_same_time_in_short_ones() {
        let mut one_long = Drawers::default();
        one_long.toggle(clip(1));
        one_long.advance(0.05);
        let mut short_ones = Drawers::default();
        short_ones.toggle(clip(1));
        advance_for(&mut short_ones, 0.05);
        assert!((one_long.openness(clip(1)) - short_ones.openness(clip(1))).abs() < 1e-5);
    }

    #[test]
    fn the_drawers_of_clips_that_are_gone_are_forgotten() {
        let project = Project::empty();
        let mut drawers = Drawers::default();
        drawers.toggle(clip(1));
        drawers.advance(0.05);
        drawers.forget_what_is_gone(&project);
        assert!(!drawers.is_open(clip(1)));
        assert!(drawers.levels.is_empty());
    }
}
