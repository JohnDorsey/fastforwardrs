// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Drawers on clips and on tracks: a clip has a drawer under it and so does a track, which slides open when the wide, thin
//! button at the bottom edge of the clip (or of the box of the track) is clicked, and shut when it is clicked again. A drawer
//! holds the racks of keyframes that are kept with what it is on, which move along with the clip when it is moved.
//!
//! A drawer of a clip is as wide as the clip and hangs from the bottom of it. Its outer two corners are rounded as much as
//! the clip's are, and the side that faces the clip is straight and goes all the way up to the clip, into the room that the
//! rounding of the clip's corners leaves outside them, so that the clip and the drawer look like one thing. The drawer of a
//! track is as wide as the strip, and hangs from the bottom of the strip, or if drawers of clips are open, from the bottom of
//! the lowest of them, so that it does not cover them. A drawer opens over the rows of the tracks that are under it (see
//! [`super::rows`]): a drawer pushes the rows under it down to make room for itself, unless it was opened with Shift held,
//! when it lies over them and they stay where they are.
//!
//! The button is only there when something says it should be: when the clip (or the box of the track) is hovered over, when
//! its drawer is open and when there is something in the drawer.
//!
//! Which drawers are open is not part of the project: it is not saved, and opening or shutting one is not an edit
//! that can be undone.
//!
//! A drawer makes a sound as it slides open, and another as it slides shut (the editor plays them: [`Drawers`] only says that
//! a drawer has begun to open or to shut, see [`Drawers::take_the_sounds`]).

use std::collections::HashMap;

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

/// How a drawer opened, which is whether it makes room for itself (the rows under it are pushed down) or lies over them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum OpenMode {
    /// The rows under the drawer are pushed down, as far as it hangs, to make room for it. This is how a drawer opens when
    /// its button is clicked, and when a keyframe is put in it.
    #[default]
    PushingTheRowsUnder,
    /// The rows under the drawer stay where they are, and the drawer is drawn over them. This is how a drawer opens when its
    /// button is clicked with Shift held.
    LyingOverTheRowsUnder,
}

impl OpenMode {
    /// How a drawer opens when its button is clicked while `shift_is_held` or not.
    pub(super) fn when_clicked(shift_is_held: bool) -> Self {
        if shift_is_held {
            Self::LyingOverTheRowsUnder
        } else {
            Self::PushingTheRowsUnder
        }
    }
}

/// A sound that a drawer makes: it begins to slide open or shut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawerSound {
    Opening,
    Closing,
}

/// How far one drawer has slid, and how it opened.
#[derive(Clone, Copy, Debug)]
struct Slide {
    /// How far open the drawer is, from 0 (shut) to 1 (open), evenly with time. (What is seen is [`Drawers::openness`],
    /// which slows down as it arrives.)
    level: f32,
    /// Whether the drawer is open or is opening, and not shut or shutting.
    is_open: bool,
    /// How the drawer opened, which it keeps until it has shut.
    mode: OpenMode,
}

impl Slide {
    fn is_sliding(&self) -> bool {
        self.level != if self.is_open { 1.0 } else { 0.0 }
    }
}

/// Which drawers are open, how far each has slid, and how each opened.
#[derive(Debug, Default)]
pub(super) struct Drawers {
    /// The drawers that are open, or have not finished shutting.
    slides: HashMap<AnimationTarget, Slide>,
    /// The drawer that has just been opened, whose bottom is to be brought into view as it slides (see [`Self::revealing`]).
    revealing: Option<AnimationTarget>,
    /// The sounds that drawers have begun to make since they were last taken (see [`Self::take_the_sounds`]), in order.
    sounds: Vec<DrawerSound>,
}

impl Drawers {
    pub(super) fn is_open(&self, owner: AnimationTarget) -> bool {
        self.slides.get(&owner).is_some_and(|slide| slide.is_open)
    }

    /// How the drawer of `owner` is open (or was, if it is shutting): pushing the rows under it down if it is not known.
    pub(super) fn mode_of(&self, owner: AnimationTarget) -> OpenMode {
        self.slides.get(&owner).map_or(OpenMode::default(), |slide| slide.mode)
    }

    /// Opens the drawer of `owner` if it is shut, the way `mode` says, and shuts it if it is open: from where it is now, if it was
    /// sliding. (A drawer that is still sliding shut and is opened again goes on the way it was open, so that the rows under it do
    /// not jump.) An opening drawer is the one that is to be brought into view.
    pub(super) fn toggle(&mut self, owner: AnimationTarget, mode: OpenMode) {
        let slide = self.slides.entry(owner).or_insert(Slide {
            level: 0.0,
            is_open: false,
            mode,
        });
        slide.is_open = !slide.is_open;
        let is_open = slide.is_open;
        if is_open {
            if slide.level == 0.0 {
                slide.mode = mode;
            }
            self.revealing = Some(owner);
        }
        self.sounds.push(if is_open {
            DrawerSound::Opening
        } else {
            DrawerSound::Closing
        });
    }

    /// The sounds that drawers have begun to make since this was last called, in the order they began, which are then forgotten:
    /// one for each drawer that began to slide open or shut (a drawer that is opened again while it slides shut begins to make
    /// another).
    pub(super) fn take_the_sounds(&mut self) -> Vec<DrawerSound> {
        std::mem::take(&mut self.sounds)
    }

    /// Opens the drawer of `owner` if it is shut (and leaves it alone if it is open), pushing the rows under it down: from where it
    /// is now, if it was sliding shut.
    pub(super) fn open_it(&mut self, owner: AnimationTarget) {
        if !self.is_open(owner) {
            self.toggle(owner, OpenMode::PushingTheRowsUnder);
        }
    }

    /// Slides the drawers that are open or shutting on, for a frame of the window that took `seconds`. Returns whether
    /// any of them is still sliding, which is when the window has to be drawn again soon.
    pub(super) fn advance(&mut self, seconds: f32) -> bool {
        let step = seconds / SECONDS_TO_SLIDE;
        let mut is_sliding = false;
        self.slides.retain(|_, slide| {
            let target = if slide.is_open { 1.0 } else { 0.0 };
            slide.level = if slide.level < target {
                (slide.level + step).min(target)
            } else {
                (slide.level - step).max(target)
            };
            is_sliding |= slide.is_sliding();
            // A shut drawer that has finished shutting is no longer remembered.
            slide.level > 0.0 || slide.is_open
        });
        is_sliding
    }

    /// How far open the drawer of `owner` is as it is seen, from 0 to 1: it starts quickly and slows as it arrives.
    pub(super) fn openness(&self, owner: AnimationTarget) -> f32 {
        ease_out(self.slides.get(&owner).map_or(0.0, |slide| slide.level))
    }

    /// Whether the drawer of `owner` is open, or still sliding shut: whether any of it is to be seen.
    pub(super) fn is_to_be_seen(&self, owner: AnimationTarget) -> bool {
        self.is_open(owner) || self.openness(owner) > 0.0
    }

    /// The drawer whose bottom is to be brought into view while it slides open, if there is one: the one that was opened last, and
    /// only until it has finished sliding (see [`Self::stop_revealing_if_it_has_stopped_sliding`]).
    pub(super) fn revealing(&self) -> Option<AnimationTarget> {
        self.revealing
    }

    /// Lets go of the drawer that was being brought into view if it has stopped sliding, or is not there any more.
    pub(super) fn stop_revealing_if_it_has_stopped_sliding(&mut self) {
        if let Some(owner) = self.revealing
            && !self.slides.get(&owner).is_some_and(Slide::is_sliding)
        {
            self.revealing = None;
        }
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
    /// furthest does, however they opened. (A keyframe crate has no drawer to open, but it reaches below the strip when it has
    /// more racks than fit in it: see [`crate_layout`], and `strip_height`, which is how tall the strip of the track is drawn:
    /// it is the only thing that the height of the strip has to do with how far a drawer reaches.)
    pub(super) fn reach_of_the_drawers_of_the_clips_of(
        &self,
        track: &Track,
        strip_height: f32,
        project: &Project,
    ) -> f32 {
        self.furthest_reach_of_the_clips_of(track, strip_height, project, |_| true)
    }

    /// How far the drawers of the clips of `track` that pushed the rows under them to open reach under the bottom of its row, as
    /// far as the one that reaches furthest does. (A keyframe crate does not push: it lies over what is under it.)
    fn reach_of_the_drawers_of_the_clips_of_that_push(
        &self,
        track: &Track,
        strip_height: f32,
        project: &Project,
    ) -> f32 {
        self.furthest_reach_of_the_clips_of(track, strip_height, project, |mode| {
            mode == Some(OpenMode::PushingTheRowsUnder)
        })
    }

    /// How far under the bottom of its row the drawers of the clips of `track` reach, as far as the one that reaches furthest
    /// does, of those that `counts` says count (given how each opened, or `None` for a keyframe crate, which does not open).
    fn furthest_reach_of_the_clips_of(
        &self,
        track: &Track,
        strip_height: f32,
        project: &Project,
        counts: impl Fn(Option<OpenMode>) -> bool,
    ) -> f32 {
        track
            .placements()
            .map(|placement| match placement.clip {
                Clip::Crate(_) if counts(None) => {
                    crate_layout(strip_height, placement.drawer.len()).reach_below_the_strip
                }
                other if has_an_accessible_drawer(other) => {
                    let owner = AnimationTarget::Clip(placement.id);
                    if counts(Some(self.mode_of(owner))) {
                        self.hanging_height_of(owner, project)
                    } else {
                        0.0
                    }
                }
                _ => 0.0,
            })
            .fold(0.0, f32::max)
    }

    /// How far the drawers hanging from the row of `track` (its own and its clips') reach under the bottom of the row,
    /// altogether: the drawer of the track hangs from the end of the drawers of its clips.
    pub(super) fn reach_under_the_row_of(&self, track: &Track, strip_height: f32, project: &Project) -> f32 {
        self.reach_of_the_drawers_of_the_clips_of(track, strip_height, project)
            + self.hanging_height_of(AnimationTarget::Track(track.id()), project)
    }

    /// How far the row of `track` is made taller by the drawers that hang from it, for them: the rows under it are pushed down
    /// by that much. The drawers of its clips that pushed to open are in it, as far as the lowest of them reaches; and so is
    /// everything above the drawer of the track if that pushed to open, because it hangs from the end of them. What lies over
    /// the rows under it, whichever it is, is not.
    pub(super) fn extra_height_of(&self, track: &Track, strip_height: f32, project: &Project) -> f32 {
        let of_the_track = AnimationTarget::Track(track.id());
        let hanging = self.hanging_height_of(of_the_track, project);
        if hanging > 0.0 && self.mode_of(of_the_track) == OpenMode::PushingTheRowsUnder {
            return self.reach_of_the_drawers_of_the_clips_of(track, strip_height, project) + hanging;
        }
        self.reach_of_the_drawers_of_the_clips_of_that_push(track, strip_height, project)
    }

    /// How far the drawers that hang from the row of `track` reach under the row as the rows are laid out (which is the track and
    /// [`Self::extra_height_of`] it): what lies over the rows under it.
    pub(super) fn reach_over_the_rows_under_the_row_of(
        &self,
        track: &Track,
        strip_height: f32,
        project: &Project,
    ) -> f32 {
        (self.reach_under_the_row_of(track, strip_height, project) - self.extra_height_of(track, strip_height, project))
            .max(0.0)
    }

    /// Forgets the drawers of clips and tracks that are not in `project` any more.
    pub(super) fn forget_what_is_gone(&mut self, project: &Project) {
        let is_there = |owner: &AnimationTarget| project.drawer_of(*owner).is_some();
        self.slides.retain(|owner, _| is_there(owner));
        if self.revealing.is_some_and(|owner| !is_there(&owner)) {
            self.revealing = None;
        }
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
        drawers.toggle(owner, OpenMode::when_clicked(ui.input(|input| input.modifiers.shift)));
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
mod tests;
