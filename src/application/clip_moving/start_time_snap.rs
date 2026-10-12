//! Holding a dragged clip to the time it started at, for as long as it is on another track.
//!
//! A person who drags a clip from one track to another usually wants it to be at the same time on the other,
//! and the pointer never goes straight down. So a clip that is dragged to another track is held to the frame it
//! started at, as a magnet holds iron: while the frame it would otherwise be at is near that (a few points away
//! on the screen, however far in frames that is at the zoom) it is put at it exactly, and further than that it
//! follows the pointer. It is held like that until the button is let go.
//!
//! A person who drags a clip a long way along its own track first wants it somewhere else in time, though, and
//! then the time it started at means nothing: such a clip has no time to be held to.

use super::Snap;
use crate::project::{ClipId, TrackId};

/// How near, in points on the screen, a clip must be to the time it is held to for it to be held, which is
/// how far it can be pulled away from the time before it comes loose.
pub const HELD_WITHIN_POINTS: f64 = 24.0;

/// How far, in points on the screen, a clip can be dragged along the track it started on before it has no
/// time to be held to (a decent distance: a few of the frames at the zoom the timeline starts at, and
/// more than half a second).
pub const DECENT_DISTANCE_IN_POINTS: f64 = 96.0;

/// What a drag remembers of the clip that was taken hold of, so that it can be held to the time it started at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartTimeMemory {
    /// The first frame of the clip when it was taken hold of.
    start: u64,
    /// The track it was on then.
    first_track: TrackId,
    /// Whether the clip has been on another track since.
    has_been_on_another_track: bool,
    /// Whether the clip was dragged a decent distance along the first track before it was on another: then it
    /// has no start time to be held to for the rest of the drag.
    was_dragged_a_decent_distance: bool,
}

impl StartTimeMemory {
    /// Remembers that a clip was taken hold of where it starts at `start`, on the track `first_track`.
    pub fn new(start: u64, first_track: TrackId) -> Self {
        Self {
            start,
            first_track,
            has_been_on_another_track: false,
            was_dragged_a_decent_distance: false,
        }
    }

    /// The first frame of the clip when it was taken hold of.
    #[cfg(test)]
    pub fn start(&self) -> u64 {
        self.start
    }

    /// Notes where the clip is now (call this once for each frame of the window that the drag lasts, before
    /// working out where it goes), `points_per_frame` being how wide a frame is on the screen.
    pub fn note_where_the_clip_is(&mut self, track: TrackId, start_now: u64, points_per_frame: f64) {
        if self.has_been_on_another_track || self.was_dragged_a_decent_distance {
            return;
        }
        if track != self.first_track {
            self.has_been_on_another_track = true;
        } else if start_now.abs_diff(self.start) as f64 * points_per_frame > DECENT_DISTANCE_IN_POINTS {
            self.was_dragged_a_decent_distance = true;
        }
    }

    /// What holds `clip`, the clip that was taken hold of, to the time it started at in the frame that is being
    /// worked out, if anything does. `points_per_frame` says how wide a frame is on the screen.
    pub fn snap(&self, clip: ClipId, points_per_frame: f64) -> Option<Snap> {
        if self.was_dragged_a_decent_distance {
            return None;
        }
        Some(Snap {
            clip,
            start: self.start,
            within_frames: frames_that_fit_in(HELD_WITHIN_POINTS, points_per_frame),
            // The clip is free on the track it started on until it has been on another one: then the way back to
            // the time it started at is held too.
            free_on: (!self.has_been_on_another_track).then_some(self.first_track),
        })
    }
}

/// How many whole frames are as wide as `points` or less, when a frame is `points_per_frame` wide.
fn frames_that_fit_in(points: f64, points_per_frame: f64) -> u64 {
    if points_per_frame <= 0.0 || !points_per_frame.is_finite() {
        return 0;
    }
    (points / points_per_frame).floor() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: TrackId = TrackId::new(0);
    const OTHER: TrackId = TrackId::new(1);

    fn clip() -> ClipId {
        ClipId::new(FIRST, 1)
    }

    fn memory() -> StartTimeMemory {
        StartTimeMemory::new(100, FIRST)
    }

    #[test]
    fn a_clip_taken_hold_of_is_held_to_its_start_on_other_tracks_but_is_free_on_its_own() {
        let snap = memory().snap(clip(), 4.0).expect("it has a start time");
        assert_eq!(snap.clip, clip());
        assert_eq!(snap.start, 100);
        assert_eq!(snap.free_on, Some(FIRST));
    }

    #[test]
    fn once_the_clip_has_been_on_another_track_it_is_held_on_every_track_including_the_first() {
        let mut memory = memory();
        memory.note_where_the_clip_is(OTHER, 100, 4.0);
        assert_eq!(memory.snap(clip(), 4.0).unwrap().free_on, None);
        // And it stays that way when it comes back.
        memory.note_where_the_clip_is(FIRST, 103, 4.0);
        assert_eq!(memory.snap(clip(), 4.0).unwrap().free_on, None);
    }

    #[test]
    fn moving_a_little_along_its_own_track_does_not_take_the_start_time_away() {
        let mut memory = memory();
        // 23 frames of 4 points is 92 points, which is less than a decent distance.
        for start in [100, 90, 123, 77, 100] {
            memory.note_where_the_clip_is(FIRST, start, 4.0);
        }
        assert!(memory.snap(clip(), 4.0).is_some());
    }

    #[test]
    fn a_clip_dragged_a_decent_distance_along_its_own_track_has_no_start_time_to_be_held_to() {
        let mut memory = memory();
        memory.note_where_the_clip_is(FIRST, 124, 4.0); // 96 points: not more than a decent distance
        assert!(memory.snap(clip(), 4.0).is_some(), "just short of it");
        memory.note_where_the_clip_is(FIRST, 125, 4.0); // 100 points
        assert_eq!(memory.snap(clip(), 4.0), None);
        // Nor does it get one back when it comes back to where it started, or goes to another track.
        memory.note_where_the_clip_is(FIRST, 100, 4.0);
        assert_eq!(memory.snap(clip(), 4.0), None, "back where it started");
        memory.note_where_the_clip_is(OTHER, 100, 4.0);
        assert_eq!(memory.snap(clip(), 4.0), None, "and on another track");
    }

    #[test]
    fn a_clip_dragged_a_decent_distance_to_the_left_has_none_either() {
        let mut memory = memory();
        memory.note_where_the_clip_is(FIRST, 70, 4.0); // 120 points to the left
        assert_eq!(memory.snap(clip(), 4.0), None);
    }

    #[test]
    fn how_far_along_another_track_the_clip_is_dragged_does_not_matter_it_is_held_all_the_same() {
        let mut memory = memory();
        memory.note_where_the_clip_is(OTHER, 100, 4.0);
        memory.note_where_the_clip_is(OTHER, 5000, 4.0);
        memory.note_where_the_clip_is(FIRST, 100, 4.0);
        memory.note_where_the_clip_is(FIRST, 5000, 4.0);
        assert!(memory.snap(clip(), 4.0).is_some());
    }

    #[test]
    fn a_clip_that_is_found_on_another_track_the_first_time_it_is_looked_at_was_not_dragged_along_its_own() {
        let mut memory = memory();
        memory.note_where_the_clip_is(OTHER, 5000, 4.0);
        assert!(memory.snap(clip(), 4.0).is_some());
    }

    #[test]
    fn a_decent_distance_is_so_many_points_which_is_fewer_frames_when_the_timeline_is_zoomed_in() {
        let mut zoomed_in = memory();
        zoomed_in.note_where_the_clip_is(FIRST, 110, 10.0); // 100 points
        assert_eq!(zoomed_in.snap(clip(), 10.0), None);
        let mut zoomed_out = memory();
        zoomed_out.note_where_the_clip_is(FIRST, 110, 1.0); // 10 points
        assert!(zoomed_out.snap(clip(), 1.0).is_some());
    }

    #[test]
    fn how_near_is_near_enough_to_be_held_is_so_many_points_and_so_fewer_frames_when_zoomed_in() {
        let within = |points_per_frame: f64| memory().snap(clip(), points_per_frame).unwrap().within_frames;
        assert_eq!(within(6.0), 4, "24 points is 4 frames of 6");
        assert_eq!(within(5.0), 4, "and not 5, which would be 25 points");
        assert_eq!(within(1.0), 24);
        assert_eq!(within(0.5), 48);
        assert_eq!(within(30.0), 0, "a frame wider than that is only held to exactly");
        assert_eq!(within(0.0), 0, "a scale that is nonsense holds nothing");
        assert_eq!(within(f64::NAN), 0);
    }

    #[test]
    fn the_memory_says_where_the_clip_started() {
        assert_eq!(memory().start(), 100);
    }
}
