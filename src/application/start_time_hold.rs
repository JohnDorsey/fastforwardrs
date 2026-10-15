// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Holding a dragged thing to the time it started at, for as long as it is on another row: a clip that is dragged from one
//! track to another, or a keyframe that is dragged from one rack to another.
//!
//! A person who drags something from one row to another usually wants it to be at the same time on the other, and the pointer
//! never goes straight down. So a thing that is dragged to another row is held to the frame it started at, as a magnet holds
//! iron: while the frame it would otherwise be at is near that (a few points away on the screen, however far in frames that is
//! at the zoom) it is put at it exactly, and further than that it follows the pointer. It is held like that until the button is
//! let go.
//!
//! A person who drags something a long way along its own row first wants it somewhere else in time, though, and then the time it
//! started at means nothing: such a thing has no time to be held to.
//!
//! What a row is does not matter here (a track, a rack): only whether it is the one the thing started on.

/// How near, in points on the screen, a thing must be to the time it is held to for it to be held, which is how far it can be
/// pulled away from the time before it comes loose.
pub const HELD_WITHIN_POINTS: f64 = 24.0;

/// How far, in points on the screen, a thing can be dragged along the row it started on before it has no time to be held to (a
/// decent distance: a few of the frames at the zoom the timeline starts at, and more than half a second).
pub const DECENT_DISTANCE_IN_POINTS: f64 = 96.0;

/// What a drag remembers of the thing that was taken hold of, so that it can be held to the time it started at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartTimeMemory<Row> {
    /// The frame the thing was at when it was taken hold of.
    start: u64,
    /// The row it was on then.
    first_row: Row,
    /// Whether the thing has been on another row since.
    has_been_on_another_row: bool,
    /// Whether the thing was dragged a decent distance along the first row before it was on another: then it has no start time
    /// to be held to for the rest of the drag.
    was_dragged_a_decent_distance: bool,
}

/// A time that a dragged thing is held to when it is near, and what the hold depends on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldTime<Row> {
    /// The frame the thing is held to.
    pub start: u64,
    /// How near to `start`, in frames, the frame the thing would be at if it followed the pointer exactly has to be for it to be
    /// held there.
    pub within_frames: u64,
    /// The row that the thing is not held to `start` on, if there is one.
    pub free_on: Option<Row>,
}

impl<Row: Copy + PartialEq> StartTimeMemory<Row> {
    /// Remembers that a thing was taken hold of where it is at frame `start`, on the row `first_row`.
    pub fn new(start: u64, first_row: Row) -> Self {
        Self {
            start,
            first_row,
            has_been_on_another_row: false,
            was_dragged_a_decent_distance: false,
        }
    }

    /// The frame the thing was at when it was taken hold of.
    pub fn start(&self) -> u64 {
        self.start
    }

    /// Notes where the thing is now (call this once for each frame of the window that the drag lasts, before working out where it
    /// goes), `points_per_frame` being how wide a frame is on the screen.
    pub fn note_where_it_is(&mut self, row: Row, frame_now: u64, points_per_frame: f64) {
        if self.has_been_on_another_row || self.was_dragged_a_decent_distance {
            return;
        }
        if row != self.first_row {
            self.has_been_on_another_row = true;
        } else if frame_now.abs_diff(self.start) as f64 * points_per_frame > DECENT_DISTANCE_IN_POINTS {
            self.was_dragged_a_decent_distance = true;
        }
    }

    /// What holds the thing to the time it started at in the frame that is being worked out, if anything does.
    /// `points_per_frame` says how wide a frame is on the screen.
    pub fn held_time(&self, points_per_frame: f64) -> Option<HeldTime<Row>> {
        if self.was_dragged_a_decent_distance {
            return None;
        }
        Some(HeldTime {
            start: self.start,
            within_frames: frames_that_fit_in(HELD_WITHIN_POINTS, points_per_frame),
            // The thing is free on the row it started on until it has been on another one: then the way back to the time it
            // started at is held too.
            free_on: (!self.has_been_on_another_row).then_some(self.first_row),
        })
    }
}

impl<Row: Copy + PartialEq> HeldTime<Row> {
    /// How many frames later (earlier, if negative) than `now` the thing is at the time it is held to, if it is held when it goes
    /// to the row `destination` and the pointer wants it `wanted_frames` frames later than it is: it is, unless the row is one
    /// that it is free on, or what the pointer wants of it would put it too far from that time.
    pub fn shift_to_it(&self, destination: Row, now: u64, wanted_frames: i64) -> Option<i64> {
        if self.free_on == Some(destination) {
            return None;
        }
        let to_the_time = i64::try_from(self.start).unwrap_or(i64::MAX) - i64::try_from(now).unwrap_or(i64::MAX);
        ((wanted_frames - to_the_time).unsigned_abs() <= self.within_frames).then_some(to_the_time)
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
    use crate::project::TrackId;

    const FIRST: TrackId = TrackId::new(0);
    const OTHER: TrackId = TrackId::new(1);

    fn memory() -> StartTimeMemory<TrackId> {
        StartTimeMemory::new(100, FIRST)
    }

    #[test]
    fn a_thing_taken_hold_of_is_held_to_its_start_on_other_rows_but_is_free_on_its_own() {
        let held = memory().held_time(4.0).expect("it has a start time");
        assert_eq!(held.start, 100);
        assert_eq!(held.free_on, Some(FIRST));
    }

    #[test]
    fn once_the_thing_has_been_on_another_row_it_is_held_on_every_row_including_the_first() {
        let mut memory = memory();
        memory.note_where_it_is(OTHER, 100, 4.0);
        assert_eq!(memory.held_time(4.0).unwrap().free_on, None);
        // And it stays that way when it comes back.
        memory.note_where_it_is(FIRST, 103, 4.0);
        assert_eq!(memory.held_time(4.0).unwrap().free_on, None);
    }

    #[test]
    fn moving_a_little_along_its_own_row_does_not_take_the_start_time_away() {
        let mut memory = memory();
        // 23 frames of 4 points is 92 points, which is less than a decent distance.
        for frame in [100, 90, 123, 77, 100] {
            memory.note_where_it_is(FIRST, frame, 4.0);
        }
        assert!(memory.held_time(4.0).is_some());
    }

    #[test]
    fn a_thing_dragged_a_decent_distance_along_its_own_row_has_no_start_time_to_be_held_to() {
        let mut memory = memory();
        memory.note_where_it_is(FIRST, 124, 4.0); // 96 points: not more than a decent distance
        assert!(memory.held_time(4.0).is_some(), "just short of it");
        memory.note_where_it_is(FIRST, 125, 4.0); // 100 points
        assert_eq!(memory.held_time(4.0), None);
        // Nor does it get one back when it comes back to where it started, or goes to another row.
        memory.note_where_it_is(FIRST, 100, 4.0);
        assert_eq!(memory.held_time(4.0), None, "back where it started");
        memory.note_where_it_is(OTHER, 100, 4.0);
        assert_eq!(memory.held_time(4.0), None, "and on another row");
    }

    #[test]
    fn a_thing_dragged_a_decent_distance_to_the_left_has_none_either() {
        let mut memory = memory();
        memory.note_where_it_is(FIRST, 70, 4.0); // 120 points to the left
        assert_eq!(memory.held_time(4.0), None);
    }

    #[test]
    fn how_far_along_another_row_the_thing_is_dragged_does_not_matter_it_is_held_all_the_same() {
        let mut memory = memory();
        memory.note_where_it_is(OTHER, 100, 4.0);
        memory.note_where_it_is(OTHER, 5000, 4.0);
        memory.note_where_it_is(FIRST, 100, 4.0);
        memory.note_where_it_is(FIRST, 5000, 4.0);
        assert!(memory.held_time(4.0).is_some());
    }

    #[test]
    fn a_thing_that_is_found_on_another_row_the_first_time_it_is_looked_at_was_not_dragged_along_its_own() {
        let mut memory = memory();
        memory.note_where_it_is(OTHER, 5000, 4.0);
        assert!(memory.held_time(4.0).is_some());
    }

    #[test]
    fn a_decent_distance_is_so_many_points_which_is_fewer_frames_when_the_timeline_is_zoomed_in() {
        let mut zoomed_in = memory();
        zoomed_in.note_where_it_is(FIRST, 110, 10.0); // 100 points
        assert_eq!(zoomed_in.held_time(10.0), None);
        let mut zoomed_out = memory();
        zoomed_out.note_where_it_is(FIRST, 110, 1.0); // 10 points
        assert!(zoomed_out.held_time(1.0).is_some());
    }

    #[test]
    fn how_near_is_near_enough_to_be_held_is_so_many_points_and_so_fewer_frames_when_zoomed_in() {
        let within = |points_per_frame: f64| memory().held_time(points_per_frame).unwrap().within_frames;
        assert_eq!(within(6.0), 4, "24 points is 4 frames of 6");
        assert_eq!(within(5.0), 4, "and not 5, which would be 25 points");
        assert_eq!(within(1.0), 24);
        assert_eq!(within(0.5), 48);
        assert_eq!(within(30.0), 0, "a frame wider than that is only held to exactly");
        assert_eq!(within(0.0), 0, "a scale that is nonsense holds nothing");
        assert_eq!(within(f64::NAN), 0);
    }

    #[test]
    fn the_memory_says_where_the_thing_started() {
        assert_eq!(memory().start(), 100);
    }

    #[test]
    fn a_row_can_be_anything_that_can_be_compared() {
        let mut memory = StartTimeMemory::new(10, "a rack");
        memory.note_where_it_is("another rack", 10, 4.0);
        assert_eq!(memory.held_time(4.0).unwrap().free_on, None);
    }

    // --- how far it goes to be at the time ---

    fn held_at_100_within_4_frames() -> HeldTime<TrackId> {
        HeldTime {
            start: 100,
            within_frames: 4,
            free_on: Some(FIRST),
        }
    }

    #[test]
    fn a_thing_that_would_be_near_the_time_it_started_at_goes_to_it_and_one_that_would_be_further_follows_the_pointer()
    {
        let held = held_at_100_within_4_frames();
        // It is at 90 now. The pointer wants it 10 later, which is exactly the time.
        assert_eq!(held.shift_to_it(OTHER, 90, 10), Some(10));
        assert_eq!(held.shift_to_it(OTHER, 90, 14), Some(10), "4 frames beyond: still near");
        assert_eq!(held.shift_to_it(OTHER, 90, 6), Some(10), "4 frames short: still near");
        assert_eq!(held.shift_to_it(OTHER, 90, 15), None, "5 beyond is further");
        assert_eq!(held.shift_to_it(OTHER, 90, 5), None, "5 short is further");
        // And when it is at 110 (the shift is the other way).
        assert_eq!(held.shift_to_it(OTHER, 110, -10), Some(-10));
    }

    #[test]
    fn a_thing_is_not_held_on_the_row_that_it_is_free_on() {
        let held = held_at_100_within_4_frames();
        assert_eq!(held.shift_to_it(FIRST, 90, 10), None);
        assert_eq!(held.shift_to_it(OTHER, 90, 10), Some(10));
        let held_everywhere = HeldTime { free_on: None, ..held };
        assert_eq!(held_everywhere.shift_to_it(FIRST, 90, 10), Some(10));
    }

    #[test]
    fn a_time_that_is_held_to_exactly_holds_only_what_is_exactly_there() {
        let exactly = HeldTime {
            within_frames: 0,
            ..held_at_100_within_4_frames()
        };
        assert_eq!(exactly.shift_to_it(OTHER, 90, 10), Some(10));
        assert_eq!(exactly.shift_to_it(OTHER, 90, 11), None);
    }
}
