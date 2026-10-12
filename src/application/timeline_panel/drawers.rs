//! Drawers on clips and on tracks: a clip has a drawer under it and so does a track, which slides open when the wide, thin
//! button at the bottom edge of the clip (or of the box of the track) is clicked, and shut when it is clicked again. A drawer
//! holds the racks of keyframes that are kept with what it is on, which move along with the clip when it is moved.
//!
//! A drawer of a clip is as wide as the clip and hangs from the bottom of it. Its outer two corners are rounded as much as
//! the clip's are, and the side that faces the clip is straight and goes all the way up to the clip, into the room that the
//! rounding of the clip's corners leaves outside them, so that the clip and the drawer look like one thing. The drawer of a
//! track is as wide as the strip, and hangs from the bottom of the strip, or if drawers of clips are open, from the bottom of
//! the lowest of them, so that it does not cover them. A drawer opens over the rows of the tracks that are under it (see
//! [`super::rows`]): it does not push them down.
//!
//! The button is only there when something says it should be: when the clip (or the box of the track) is hovered over, when
//! its drawer is open and when there is something in the drawer.
//!
//! Which drawers are open is not part of the project: it is not saved, and opening or shutting one is not an edit
//! that can be undone.

use std::collections::{HashMap, HashSet};

use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Sense, Stroke, Ui, WidgetInfo, WidgetType, pos2, vec2};

use crate::application::theme;
use crate::project::{AnimationTarget, Clip, ClipId, Project, Track};

/// How wide the button is, and how tall: it is five times as wide as it is tall. (At the least height of a track
/// it covers less than half of the clip, which a test checks.)
pub(super) const BUTTON_WIDTH: f32 = 60.0;
pub(super) const BUTTON_HEIGHT: f32 = 12.0;

/// How far the button keeps from the sides of a clip that is narrower than it, in points.
const BUTTON_MARGIN_AT_THE_SIDES: f32 = 10.0;

/// A clip that leaves room for a button narrower than this has no button, because it would be too small to take
/// hold of.
const NARROWEST_BUTTON: f32 = 20.0;

/// How long a drawer takes to slide all the way open (or shut), in seconds.
const SECONDS_TO_SLIDE: f32 = 0.18;

/// How the label of the button of a drawer of a clip says whose it is, which the tests (and screen readers) know it by.
pub fn drawer_button_label(track_title: &str, clip: ClipId) -> String {
    format!("Drawer of clip {} of {track_title}", clip.number())
}

/// How the label of the button of the drawer of a track says whose it is.
pub fn track_drawer_button_label(track_title: &str) -> String {
    format!("Drawer of {track_title}")
}

/// Whether `clip` has a drawer that can be opened. Every clip has one but a keyframe crate, which is nothing but a drawer
/// that is always open.
pub(super) fn has_an_accessible_drawer(clip: &Clip) -> bool {
    !matches!(clip, Clip::Crate(_))
}

/// How tall a drawer is when it is open and has `rack_count` racks in it, in points: room above the racks, the racks with
/// room between them, and under the last as much room as the corners of a clip are rounded by, so that a rack that is as
/// wide as the drawer is is not cut off by the rounding of the drawer's lower corners.
pub(super) fn drawer_height(rack_count: usize) -> f32 {
    let racks =
        rack_count as f32 * theme::RACK_HEIGHT + rack_count.saturating_sub(1) as f32 * theme::SPACE_BETWEEN_RACKS;
    theme::SPACE_ABOVE_THE_RACKS + racks + f32::from(theme::CLIP_CORNER_RADIUS)
}

/// How tall the drawer of `owner` is when it is open, by how many racks it has.
pub(super) fn height_when_open(owner: AnimationTarget, project: &Project) -> f32 {
    drawer_height(project.drawer_of(owner).map_or(0, crate::project::Drawer::len))
}

/// How the racks of a keyframe crate are laid out in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CrateLayout {
    /// Where the top of the first rack is, counted down from the top of the crate.
    pub(super) first_rack_top: f32,
    /// How far the crate goes below the strip it is on, which is as far as it needs to go to have room for all of its racks (it
    /// is as tall as the strip when they fit in that): drawn like an open drawer that hangs from it.
    pub(super) reach_below_the_strip: f32,
}

/// How the racks are laid out in a crate that has `rack_count` of them, on a strip that is `strip_height` tall: in the middle
/// of the crate up and down if they fit, and if they do not from just under the top of it, with the crate reaching below the
/// strip as far as they need.
pub(super) fn crate_layout(strip_height: f32, rack_count: usize) -> CrateLayout {
    let stack =
        rack_count as f32 * theme::RACK_HEIGHT + rack_count.saturating_sub(1) as f32 * theme::SPACE_BETWEEN_RACKS;
    if stack + 2.0 * theme::CRATE_LEAST_MARGIN <= strip_height {
        return CrateLayout {
            first_rack_top: (strip_height - stack) / 2.0,
            reach_below_the_strip: 0.0,
        };
    }
    CrateLayout {
        first_rack_top: theme::CRATE_LEAST_MARGIN,
        reach_below_the_strip: theme::CRATE_LEAST_MARGIN + stack + theme::CRATE_MARGIN - strip_height,
    }
}

/// Where the button of a drawer is: at the bottom edge of what the drawer is of (`holder`: the part of a clip that can be
/// seen, so that it can be reached however far the clip goes beyond the view, or the box of a track), in the middle of it,
/// with its bottom edge the bottom edge of `holder`. It is [`BUTTON_WIDTH`] wide, or as wide as the holder leaves room for.
/// `None` if that is too narrow to take hold of.
pub(super) fn button_rectangle(holder: Rect) -> Option<Rect> {
    let width = BUTTON_WIDTH.min(holder.width() - 2.0 * BUTTON_MARGIN_AT_THE_SIDES);
    if width < NARROWEST_BUTTON {
        return None;
    }
    Some(Rect::from_center_size(
        pos2(holder.center().x, holder.bottom() - BUTTON_HEIGHT / 2.0),
        vec2(width, BUTTON_HEIGHT),
    ))
}

/// Whether the button of a drawer is shown: when what the drawer is of is hovered over, when the drawer is open, and when
/// there is something in it. (Otherwise it is not there to be seen, which keeps the clips clear.)
pub(super) fn button_is_shown(holder_is_hovered: bool, drawer_is_open: bool, has_racks: bool) -> bool {
    holder_is_hovered || drawer_is_open || has_racks
}

/// What a drawer looks like, which is where it is and how its corners are rounded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DrawerShape {
    pub(super) rectangle: Rect,
    pub(super) corners: CornerRadius,
}

/// The shape of the drawer of the clip that is in `whole_clip` and has its corners rounded by `clip_radius`, when the drawer
/// is `openness` (0 to 1) of the way open and `height` tall when it is open all the way. `None` when it is shut.
///
/// The drawer reaches up from the bottom edge of the clip by as much as the corners of the clip are rounded by,
/// into the room outside them (what is of it inside the clip is behind the clip, which is drawn over it), and down
/// from there by as much as it is open. Its two lower corners are rounded like the clip's.
pub(super) fn drawer_shape(whole_clip: Rect, clip_radius: f32, openness: f32, height: f32) -> Option<DrawerShape> {
    if openness <= 0.0 {
        return None;
    }
    let openness = openness.min(1.0);
    let reach_up = clip_radius * openness;
    let rectangle = Rect::from_min_max(
        pos2(whole_clip.left(), whole_clip.bottom() - reach_up),
        pos2(whole_clip.right(), whole_clip.bottom() + height * openness),
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

/// The shape of the drawer of a track whose strip is `strip`, `openness` of the way open and `height` tall when it is open
/// all the way: as wide as the strip, with square corners (the strip is a view of a track that goes on beyond what is in the
/// window at either end) and hanging from `below_the_strip` points under the bottom of it, which is where the drawers of
/// its clips that are open end. `None` when it is shut.
pub(super) fn track_drawer_shape(strip: Rect, below_the_strip: f32, openness: f32, height: f32) -> Option<DrawerShape> {
    if openness <= 0.0 {
        return None;
    }
    let top = strip.bottom() + below_the_strip;
    Some(DrawerShape {
        rectangle: Rect::from_min_max(
            pos2(strip.left(), top),
            pos2(strip.right(), top + height * openness.min(1.0)),
        ),
        corners: CornerRadius::ZERO,
    })
}

/// Which drawers are open, and how far each has slid.
#[derive(Debug, Default)]
pub(super) struct Drawers {
    open: HashSet<AnimationTarget>,
    /// How far open each drawer that is open, or has not finished shutting, is: from 0 (shut) to 1 (open),
    /// evenly with time. (What is seen is [`Self::openness`], which slows down as it arrives.)
    levels: HashMap<AnimationTarget, f32>,
}

impl Drawers {
    pub(super) fn is_open(&self, owner: AnimationTarget) -> bool {
        self.open.contains(&owner)
    }

    /// Opens the drawer of `owner` if it is shut, and shuts it if it is open: from where it is now, if it was sliding.
    pub(super) fn toggle(&mut self, owner: AnimationTarget) {
        if !self.open.remove(&owner) {
            self.open.insert(owner);
        }
        self.levels.entry(owner).or_insert(0.0);
    }

    /// Opens the drawer of `owner` if it is shut (and leaves it alone if it is open): from where it is now, if it was
    /// sliding shut.
    pub(super) fn open_it(&mut self, owner: AnimationTarget) {
        if !self.is_open(owner) {
            self.toggle(owner);
        }
    }

    /// Slides the drawers that are open or shutting on, for a frame of the window that took `seconds`. Returns whether
    /// any of them is still sliding, which is when the window has to be drawn again soon.
    pub(super) fn advance(&mut self, seconds: f32) -> bool {
        let step = seconds / SECONDS_TO_SLIDE;
        let open = &self.open;
        let mut is_sliding = false;
        self.levels.retain(|owner, level| {
            let target = if open.contains(owner) { 1.0 } else { 0.0 };
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

    /// How far open the drawer of `owner` is as it is seen, from 0 to 1: it starts quickly and slows as it arrives.
    pub(super) fn openness(&self, owner: AnimationTarget) -> f32 {
        ease_out(self.levels.get(&owner).copied().unwrap_or(0.0))
    }

    /// Whether the drawer of `owner` is open, or still sliding shut: whether any of it is to be seen.
    pub(super) fn is_to_be_seen(&self, owner: AnimationTarget) -> bool {
        self.is_open(owner) || self.openness(owner) > 0.0
    }

    /// How far down from the bottom of what the drawer of `owner` is of it reaches now, as it slides: as tall as it is when it
    /// is open, which is as many racks as it has make it, times how far open it is.
    pub(super) fn hanging_height_of(&self, owner: AnimationTarget, project: &Project) -> f32 {
        let openness = self.openness(owner);
        if openness <= 0.0 {
            return 0.0;
        }
        openness * height_when_open(owner, project)
    }

    /// How far the drawers of the clips of `track` reach under the bottom of its row, as far as the one that reaches
    /// furthest does. (A keyframe crate has no drawer to open, but it reaches below the strip when it has more racks than fit in
    /// it: see [`crate_layout`].)
    pub(super) fn reach_of_the_drawers_of_the_clips_of(&self, track: &Track, project: &Project) -> f32 {
        track
            .placements()
            .map(|placement| match placement.clip {
                Clip::Crate(_) => crate_layout(track.height().points(), placement.drawer.len()).reach_below_the_strip,
                other if has_an_accessible_drawer(other) => {
                    self.hanging_height_of(AnimationTarget::Clip(placement.id), project)
                }
                _ => 0.0,
            })
            .fold(0.0, f32::max)
    }

    /// How far the drawers hanging from the row of `track` (its own and its clips') reach under the bottom of the row,
    /// altogether: the drawer of the track hangs from the end of the drawers of its clips.
    pub(super) fn reach_under_the_row_of(&self, track: &Track, project: &Project) -> f32 {
        self.reach_of_the_drawers_of_the_clips_of(track, project)
            + self.hanging_height_of(AnimationTarget::Track(track.id()), project)
    }

    /// Forgets the drawers of clips and tracks that are not in `project` any more.
    pub(super) fn forget_what_is_gone(&mut self, project: &Project) {
        let is_there = |owner: &AnimationTarget| project.drawer_of(*owner).is_some();
        self.open.retain(is_there);
        self.levels.retain(|owner, _| is_there(owner));
    }
}

/// A number from 0 to 1 that begins quickly and slows as it gets to 1, as `progress` (from 0 to 1) goes evenly.
fn ease_out(progress: f32) -> f32 {
    let remaining = 1.0 - progress.clamp(0.0, 1.0);
    1.0 - remaining * remaining * remaining
}

/// Draws a drawer in `shape`, filled with `colour`, which is behind what it is of when that is a clip (it is drawn before the
/// clip is).
pub(super) fn draw_drawer(painter: &Painter, shape: DrawerShape, colour: egui::Color32) {
    painter.rect_filled(shape.rectangle, shape.corners, colour);
}

/// Takes care of the button of the drawer of `track`, which is at the bottom edge of the box of the track (`track_box`):
/// opens or shuts the drawer when it is clicked, and draws it when it is to be seen (when `box_is_hovered`, or the drawer is
/// open, or has something in it).
pub(super) fn operate_the_button_of_a_track(
    ui: &Ui,
    track_box: Rect,
    track: &Track,
    box_is_hovered: bool,
    drawers: &mut Drawers,
) {
    let Some(button) = button_rectangle(track_box) else {
        return;
    };
    let owner = AnimationTarget::Track(track.id());
    let response = ui.interact(
        button,
        ui.id().with(("button of the drawer of track", track.id())),
        Sense::CLICK | Sense::DRAG,
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, track_drawer_button_label(track.title())));
    if response.clicked() {
        drawers.toggle(owner);
    }
    if button_is_shown(
        box_is_hovered || response.hovered(),
        drawers.is_to_be_seen(owner),
        !track.drawer().is_empty(),
    ) {
        draw_button(ui.painter(), button, drawers.is_open(owner), response.hovered());
    }
}

/// How round the two top corners of the button are, in points: the bottom two are square, so that the button sits on the
/// bottom edge of what it is on like a tab.
fn button_corners() -> CornerRadius {
    let radius = (BUTTON_HEIGHT / 2.0).round() as u8;
    CornerRadius {
        nw: radius,
        ne: radius,
        sw: 0,
        se: 0,
    }
}

/// Draws the button in `rectangle`: a tab that shows what is behind it dimmed, with an arrow in it that points down
/// when the drawer is shut (it is pulled down to open it) and up when it is open.
pub(super) fn draw_button(painter: &Painter, rectangle: Rect, is_open: bool, is_hovered: bool) {
    let (fill, outline) = if is_hovered {
        (theme::DRAWER_BUTTON_FILL_HOVERED, theme::DRAWER_BUTTON_OUTLINE_HOVERED)
    } else {
        (theme::DRAWER_BUTTON_FILL, theme::DRAWER_BUTTON_OUTLINE)
    };
    painter.rect_filled(rectangle, button_corners(), fill);
    painter.rect_stroke(
        rectangle,
        button_corners(),
        Stroke::new(1.0, outline),
        egui::StrokeKind::Inside,
    );
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
    use crate::project::{TrackHeight, TrackId};

    fn clip(number: u32) -> AnimationTarget {
        AnimationTarget::Clip(ClipId::new(TrackId::new(0), number))
    }

    fn track(number: u32) -> AnimationTarget {
        AnimationTarget::Track(TrackId::new(number))
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
    fn the_button_is_over_the_clip_in_the_middle_with_its_bottom_edge_the_bottom_edge_of_the_clip() {
        let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0));
        let button = button_rectangle(clip_rectangle).unwrap();
        assert_eq!(button.size(), vec2(BUTTON_WIDTH, BUTTON_HEIGHT));
        assert_eq!(button.center().x, 250.0, "in the middle");
        assert!(clip_rectangle.contains_rect(button), "over the clip");
        assert_eq!(button.bottom(), clip_rectangle.bottom(), "the same bottom edge");
    }

    #[test]
    fn the_two_bottom_corners_of_the_button_are_square_and_the_two_at_the_top_are_round() {
        let corners = button_corners();
        assert_eq!((corners.sw, corners.se), (0, 0));
        assert!(corners.nw > 0 && corners.nw == corners.ne);
        assert!(
            f32::from(corners.nw) <= BUTTON_HEIGHT,
            "no rounder than the button is tall"
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

    #[test]
    fn the_button_is_shown_when_what_it_is_on_is_hovered_when_the_drawer_is_open_and_when_it_has_something_in_it() {
        assert!(button_is_shown(true, false, false));
        assert!(button_is_shown(false, true, false));
        assert!(button_is_shown(false, false, true));
        assert!(button_is_shown(true, true, true));
        assert!(!button_is_shown(false, false, false), "otherwise it is not there");
    }

    // --- the drawer ---

    /// How tall the drawers of the shape tests are.
    const HEIGHT: f32 = 71.0;

    #[test]
    fn a_drawer_that_is_shut_is_not_there() {
        let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0));
        assert_eq!(drawer_shape(clip_rectangle, 27.0, 0.0, HEIGHT), None);
        assert_eq!(drawer_shape(clip_rectangle, 27.0, -0.5, HEIGHT), None);
        assert_eq!(track_drawer_shape(clip_rectangle, 0.0, 0.0, HEIGHT), None);
    }

    #[test]
    fn an_open_drawer_is_as_wide_as_its_clip_and_hangs_from_the_bottom_of_it_reaching_up_into_the_room_by_its_corners()
    {
        let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0));
        let shape = drawer_shape(clip_rectangle, 27.0, 1.0, HEIGHT).unwrap();
        assert_eq!(shape.rectangle.left(), 100.0);
        assert_eq!(shape.rectangle.right(), 400.0);
        assert_eq!(
            shape.rectangle.top(),
            154.0 - 27.0,
            "up to where the corners of the clip are rounded away"
        );
        assert_eq!(shape.rectangle.bottom(), 154.0 + HEIGHT);
    }

    #[test]
    fn the_two_lower_corners_of_a_drawer_are_rounded_like_the_clip_and_the_two_that_face_the_clip_are_square() {
        let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0));
        let corners = drawer_shape(clip_rectangle, 27.0, 1.0, HEIGHT).unwrap().corners;
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
        let shape = drawer_shape(clip_rectangle, 27.0, 0.05, HEIGHT).unwrap();
        assert!(shape.rectangle.height() < 10.0);
        assert!(f32::from(shape.corners.sw) <= shape.rectangle.height());
        assert_eq!(shape.corners.sw, shape.corners.se);
    }

    #[test]
    fn a_drawer_grows_as_it_opens_in_height_and_in_how_far_it_reaches_up() {
        let clip_rectangle = Rect::from_min_size(pos2(100.0, 50.0), vec2(300.0, 104.0));
        let mut last = drawer_shape(clip_rectangle, 27.0, 0.01, HEIGHT).unwrap().rectangle;
        for step in 2..=20 {
            let now = drawer_shape(clip_rectangle, 27.0, step as f32 / 20.0, HEIGHT)
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
        let shape = drawer_shape(narrow, 15.0, 1.0, HEIGHT).unwrap();
        assert_eq!(shape.corners.sw, 15);
        let very_narrow = Rect::from_min_size(pos2(100.0, 50.0), vec2(20.0, 104.0));
        assert_eq!(drawer_shape(very_narrow, 27.0, 1.0, HEIGHT).unwrap().corners.sw, 10);
    }

    #[test]
    fn the_drawer_of_a_track_is_as_wide_as_the_strip_hangs_from_the_bottom_of_it_and_has_square_corners() {
        let strip = Rect::from_min_size(pos2(180.0, 50.0), vec2(1000.0, 104.0));
        let shape = track_drawer_shape(strip, 0.0, 1.0, HEIGHT).unwrap();
        assert_eq!(shape.rectangle.left(), 180.0);
        assert_eq!(shape.rectangle.right(), 1180.0);
        assert_eq!(shape.rectangle.top(), 154.0, "it does not reach up into the strip");
        assert_eq!(shape.rectangle.bottom(), 154.0 + HEIGHT);
        assert_eq!(shape.corners, CornerRadius::ZERO);
        let half = track_drawer_shape(strip, 0.0, 0.5, HEIGHT).unwrap();
        assert_eq!(half.rectangle.bottom(), 154.0 + HEIGHT / 2.0);
    }

    #[test]
    fn the_drawer_of_a_track_hangs_from_where_the_drawers_of_its_clips_end() {
        let strip = Rect::from_min_size(pos2(180.0, 50.0), vec2(1000.0, 104.0));
        let shape = track_drawer_shape(strip, 40.0, 1.0, HEIGHT).unwrap();
        assert_eq!(shape.rectangle.top(), 154.0 + 40.0);
        assert_eq!(shape.rectangle.bottom(), 154.0 + 40.0 + HEIGHT);
        assert_eq!(
            track_drawer_shape(strip, 40.0, 0.0, HEIGHT),
            None,
            "shut it is not there"
        );
    }

    #[test]
    fn the_racks_of_a_crate_are_in_the_middle_of_it_up_and_down_when_they_fit_and_it_is_as_tall_as_the_strip() {
        let rack = theme::RACK_HEIGHT;
        let one = crate_layout(104.0, 1);
        assert_eq!(one.first_rack_top, (104.0 - rack) / 2.0);
        assert_eq!(one.reach_below_the_strip, 0.0);
        let three = crate_layout(104.0, 3);
        let stack = 3.0 * rack + 2.0 * theme::SPACE_BETWEEN_RACKS;
        assert_eq!(three.first_rack_top, (104.0 - stack) / 2.0);
        assert!(three.first_rack_top >= theme::CRATE_LEAST_MARGIN);
        assert_eq!(three.reach_below_the_strip, 0.0);
        assert_eq!(
            crate_layout(104.0, 0).reach_below_the_strip,
            0.0,
            "nothing to make room for"
        );
    }

    #[test]
    fn a_crate_with_more_racks_than_fit_has_them_from_near_its_top_and_reaches_below_the_strip_as_far_as_they_need() {
        let rack = theme::RACK_HEIGHT;
        let gap = theme::SPACE_BETWEEN_RACKS;
        let strip = 104.0;
        let stack = |count: usize| count as f32 * rack + count.saturating_sub(1) as f32 * gap;
        let mut count = 1;
        while stack(count) + 2.0 * theme::CRATE_LEAST_MARGIN <= strip {
            count += 1;
        }
        let too_many = crate_layout(strip, count);
        assert_eq!(too_many.first_rack_top, theme::CRATE_LEAST_MARGIN);
        assert_eq!(
            too_many.reach_below_the_strip,
            theme::CRATE_LEAST_MARGIN + stack(count) + theme::CRATE_MARGIN - strip
        );
        assert!(too_many.reach_below_the_strip > 0.0);
        // With one more it reaches that much further: another rack and the room between.
        let one_more = crate_layout(strip, count + 1);
        assert_eq!(
            one_more.reach_below_the_strip - too_many.reach_below_the_strip,
            rack + gap
        );
        // The racks all end inside the crate, with the margin under the last.
        let end_of_the_last = one_more.first_rack_top + stack(count + 1);
        assert_eq!(
            strip + one_more.reach_below_the_strip - end_of_the_last,
            theme::CRATE_MARGIN
        );
    }

    #[test]
    fn a_short_strip_has_a_crate_with_even_one_rack_in_the_middle_of_it_if_it_just_fits() {
        let strip = theme::RACK_HEIGHT + 2.0 * theme::CRATE_LEAST_MARGIN;
        assert_eq!(crate_layout(strip, 1).reach_below_the_strip, 0.0);
        assert!(crate_layout(strip - 1.0, 1).reach_below_the_strip > 0.0);
    }

    #[test]
    fn a_drawer_is_as_tall_as_its_racks_and_the_room_round_them_need() {
        let rack = theme::RACK_HEIGHT;
        let radius = f32::from(theme::CLIP_CORNER_RADIUS);
        assert_eq!(
            drawer_height(0),
            theme::SPACE_ABOVE_THE_RACKS + radius,
            "empty: no racks"
        );
        assert_eq!(drawer_height(1), theme::SPACE_ABOVE_THE_RACKS + rack + radius);
        assert_eq!(
            drawer_height(3),
            theme::SPACE_ABOVE_THE_RACKS + 3.0 * rack + 2.0 * theme::SPACE_BETWEEN_RACKS + radius,
            "room between the racks, but none after the last but the room for the corners"
        );
        let heights: Vec<f32> = (0..6).map(drawer_height).collect();
        assert!(heights.windows(2).all(|pair| pair[1] > pair[0]), "{heights:?}");
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
    fn a_drawer_that_is_opened_opens_once_whether_or_not_it_was_already_and_is_not_shut_by_it() {
        let mut drawers = Drawers::default();
        drawers.open_it(clip(1));
        assert!(drawers.is_open(clip(1)));
        drawers.open_it(clip(1));
        assert!(drawers.is_open(clip(1)), "opening what is open leaves it open");
        drawers.toggle(clip(1));
        assert!(!drawers.is_open(clip(1)));
        drawers.open_it(clip(1));
        assert!(drawers.is_open(clip(1)), "and what was shut is opened");
        assert!(!drawers.is_open(clip(2)));
    }

    #[test]
    fn the_drawer_of_a_track_is_not_the_drawer_of_a_clip_whatever_their_numbers() {
        let mut drawers = Drawers::default();
        drawers.toggle(track(1));
        assert!(drawers.is_open(track(1)));
        assert!(!drawers.is_open(clip(1)));
        assert!(!drawers.is_open(track(2)));
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
        assert!(
            drawers.is_to_be_seen(clip(1)),
            "some of it is still to be seen as it slides"
        );
        advance_for(&mut drawers, 0.5);
        assert_eq!(drawers.openness(clip(1)), 0.0);
        assert!(!drawers.is_to_be_seen(clip(1)));
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
    fn the_drawers_of_clips_and_tracks_that_are_gone_are_forgotten_and_those_that_are_there_are_not() {
        let project = Project::empty();
        let track_that_is_there = AnimationTarget::Track(project.tracks()[0].id());
        let mut drawers = Drawers::default();
        drawers.toggle(clip(1));
        drawers.toggle(track(77));
        drawers.toggle(track_that_is_there);
        drawers.advance(0.05);
        drawers.forget_what_is_gone(&project);
        assert!(!drawers.is_open(clip(1)));
        assert!(!drawers.is_open(track(77)));
        assert!(drawers.is_open(track_that_is_there));
        assert_eq!(drawers.levels.len(), 1);
    }

    #[test]
    fn how_far_a_drawer_hangs_is_how_open_it_is_times_its_height_and_a_shut_one_hangs_not_at_all() {
        let project = Project::empty();
        let owner = AnimationTarget::Track(project.tracks()[0].id());
        let mut drawers = Drawers::default();
        assert_eq!(drawers.hanging_height_of(owner, &project), 0.0);
        drawers.toggle(owner);
        advance_for(&mut drawers, 0.5);
        assert_eq!(drawers.hanging_height_of(owner, &project), drawer_height(0));
        assert_eq!(
            drawers.reach_under_the_row_of(&project.tracks()[0], &project),
            drawer_height(0)
        );
    }

    #[test]
    fn the_drawer_of_a_track_reaches_as_far_under_its_row_as_its_clips_drawers_and_its_own_height_together() {
        let mut project = Project::empty();
        let track_id = project.tracks()[0].id();
        let (text, _) = project
            .insert_text(
                track_id,
                crate::project::TimelineFrameIndex::new(20),
                crate::project::TextObject::default(),
                10,
            )
            .unwrap();
        let mut drawers = Drawers::default();
        drawers.toggle(AnimationTarget::Clip(text));
        advance_for(&mut drawers, 0.5);
        let by_the_clips = drawers.reach_of_the_drawers_of_the_clips_of(&project.tracks()[0], &project);
        assert_eq!(by_the_clips, drawer_height(0));
        drawers.toggle(AnimationTarget::Track(track_id));
        advance_for(&mut drawers, 0.5);
        assert_eq!(
            drawers.reach_under_the_row_of(&project.tracks()[0], &project),
            2.0 * drawer_height(0)
        );
        assert_eq!(
            drawers.reach_of_the_drawers_of_the_clips_of(&project.tracks()[0], &project),
            by_the_clips,
            "the track's own drawer is not one of its clips'"
        );
    }

    #[test]
    fn a_keyframe_crate_has_no_drawer_to_reach_with_even_if_it_has_racks_that_would_make_it_reach_further() {
        use crate::project::{PropertyId, RackDestination, TextObject, TimelineFrameIndex};

        let mut project = Project::empty();
        let front = project.tracks()[0].id();
        let behind = project.add_track_in_front();
        let (text, _) = project
            .insert_text(front, TimelineFrameIndex::new(20), TextObject::default(), 10)
            .unwrap();
        // A crate on the track that is in front, over the same frames, with a rack in it (which makes it taller than a
        // drawer that has none).
        let crate_clip = project.insert_crate(behind, TimelineFrameIndex::new(20), 10).unwrap();
        let made = project
            .add_keyframe(
                AnimationTarget::Clip(text),
                PropertyId::Rotation,
                TimelineFrameIndex::new(22),
            )
            .unwrap()
            .unwrap();
        project
            .move_rack(made.rack, RackDestination::Drawer(AnimationTarget::Clip(crate_clip)), 0)
            .unwrap();
        let mut drawers = Drawers::default();
        drawers.toggle(AnimationTarget::Clip(crate_clip));
        advance_for(&mut drawers, 0.5);

        assert_eq!(
            drawers.reach_of_the_drawers_of_the_clips_of(project.track(behind).unwrap(), &project),
            0.0,
            "a crate is nothing but a drawer that is always open: it has no other to open, and one rack fits in it"
        );
        assert!(!has_an_accessible_drawer(
            project.placement_of(crate_clip).unwrap().clip
        ));
        assert!(has_an_accessible_drawer(project.placement_of(text).unwrap().clip));
    }
}
