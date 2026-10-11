//! What the timeline remembers between frames of the window: how far it is scrolled and zoomed,
//! which box of settings is open, and the like.

use egui::{Context, Pos2, Rect};

use super::settings_boxes::{OpenSettingsBox, SettingsBoxSubject};
use crate::application::bubble;
use crate::application::timeline_geometry::TimelineGeometry;
use crate::project::{ClipLocation, TimelineFrameIndex, TrackId};

/// Where a click on an empty part of a strip happened, for the menu that puts something new there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NewMenuTarget {
    pub(super) track: TrackId,
    pub(super) frame: TimelineFrameIndex,
}

/// A track that is being made taller or shorter, by the boundary under its title box being dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TrackResize {
    pub(super) track: TrackId,
    /// How tall the track was when the boundary was taken hold of, in points.
    pub(super) height_at_the_start: f32,
    /// Where the pointer was then, up and down the screen.
    pub(super) pointer_y_at_the_start: f32,
    /// How tall the rows were together then, which they stay while the track is being resized
    /// (see [`super::show`]).
    pub(super) rows_area_height: f32,
}

/// Clips that are being dragged along their tracks.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ClipDrag {
    /// All the clips that move: the selected clips if the one that was taken hold of was one of them,
    /// and otherwise that clip alone.
    pub(super) moving: Vec<ClipLocation>,
    /// The clip that was taken hold of, whose place the pointer is kept at.
    pub(super) grabbed: ClipLocation,
    /// The frame that was under the pointer when the clip was taken hold of.
    pub(super) pointer_frame_at_the_start: u64,
    /// The frame that the grabbed clip started at then.
    pub(super) grabbed_start_at_the_start: u64,
}

/// A rectangle being drawn round clips to select them, by dragging from an empty place.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RubberBand {
    /// Where the pointer was when it was pressed: one corner of the rectangle, the other being where it is now.
    pub(super) origin: Pos2,
    /// The clips that were selected then and stay selected (when Ctrl was held), to which those in the rectangle are added.
    pub(super) kept: Vec<ClipLocation>,
}

/// What the timeline remembers between frames of the window.
#[derive(Default)]
pub struct TimelineViewState {
    /// How far along the strips the left edge of the view is, in points.
    pub(in crate::application) scroll_offset: f64,
    /// How many points of the screen a second of the finished video takes up, if the user has
    /// zoomed. (Otherwise it is whatever the timeline starts at, which depends on the shape of the
    /// picture.)
    pub(super) points_per_second: Option<f64>,
    /// The current frame the view last scrolled to follow.
    pub(super) last_followed_frame: Option<TimelineFrameIndex>,
    /// The frame that was picked on the ruler during the previous frame of the window. The user
    /// chose that place in the view, so the view must not scroll to put it in the middle.
    pub(super) frame_picked_on_the_ruler: Option<TimelineFrameIndex>,
    /// How far down the rows are scrolled, in points. (The scroll bar moves them too: what it did
    /// is read back into this each frame.)
    pub(super) rows_scroll_offset: f32,
    /// How tall the rows were together in the last frame, in points.
    pub(super) rows_area_height: f32,
    /// The track that is being resized, if one is.
    pub(super) track_resize: Option<TrackResize>,
    /// The clips that are being dragged along their tracks, if there are some.
    pub(super) clip_drag: Option<ClipDrag>,
    /// The rectangle that is being drawn round clips, if there is one.
    pub(super) rubber_band: Option<RubberBand>,
    /// Where the ruler and the strips were in the last frame, so that the mouse wheel scrolls
    /// them along only while the pointer is over them.
    pub(super) strips_area_in_the_last_frame: Option<Rect>,
    /// Where the whole timeline (the title boxes too) was in the last frame, so that the mouse
    /// wheel with Alt held scrolls the rows up and down only while the pointer is over it.
    pub(super) timeline_area_in_the_last_frame: Option<Rect>,
    /// The settings box that is open, if one is.
    pub(super) settings_box: Option<OpenSettingsBox>,
    /// The box that the press of a mouse button that is still down closed, so that letting go of
    /// the button (which is a click on whatever opens that box) does not open it again at once.
    pub(super) settings_box_closed_by_the_press: Option<SettingsBoxSubject>,
    /// Where the "New" menu was last opened.
    pub(super) new_menu_target: Option<NewMenuTarget>,
}

impl TimelineViewState {
    /// Forgets the scroll position, the zoom and any open box (when a different project is loaded).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The scale the strips are drawn at: `normal`, the scale the timeline starts at, as far as the
    /// user has zoomed it.
    pub fn geometry(&self, normal: TimelineGeometry) -> TimelineGeometry {
        self.points_per_second.map_or(normal, |points_per_second| {
            normal.with_points_per_second(points_per_second)
        })
    }

    /// Whether a settings box is open. (It has sliders and tick boxes of its own, which use the
    /// keys of the keyboard.)
    pub fn settings_box_is_open(&self) -> bool {
        self.settings_box.is_some()
    }

    /// What the open settings box is about, if one is open.
    pub fn settings_box_subject(&self) -> Option<SettingsBoxSubject> {
        self.settings_box.map(|open| open.subject)
    }

    /// Where the open settings box was drawn in the last frame, if one is open and has been drawn.
    #[cfg(test)]
    pub fn settings_box_rectangle(&self) -> Option<Rect> {
        self.settings_box.and_then(|open| open.rect)
    }

    /// Opens the settings box of `subject` (in place of any other), unless a press that is still
    /// going on has just closed it.
    pub(super) fn open_settings_box(&mut self, subject: SettingsBoxSubject) {
        if self.settings_box_closed_by_the_press == Some(subject) {
            return;
        }
        self.settings_box = Some(OpenSettingsBox {
            subject,
            anchor: None,
            shown_anchor: None,
            rect: None,
        });
    }

    pub(super) fn close_settings_box(&mut self) {
        self.settings_box = None;
    }

    /// Says where the settings box of `subject` has to point, if that is the box that is open.
    pub(super) fn point_settings_box_at(&mut self, subject: SettingsBoxSubject, anchor: Pos2) {
        if let Some(open) = self.settings_box.as_mut()
            && open.subject == subject
        {
            open.anchor = Some(anchor);
        }
    }

    /// Closes the open settings box if a mouse button was pressed outside it.
    pub(super) fn close_settings_box_if_pressed_outside(&mut self, context: &Context) {
        if let Some(open) = self.settings_box
            && bubble::pressed_outside(context, open.rect)
        {
            self.settings_box_closed_by_the_press = Some(open.subject);
            self.settings_box = None;
        }
    }
}
