//! The marks of the ruler above the tracks, and what the labels on them say.
//!
//! How the marks are spaced depends on how far the timeline is zoomed in. There are up to three
//! kinds: a *labelled* mark every so many whole seconds (the more zoomed out, the more seconds
//! apart, so that the labels never crowd one another), *between* marks that divide the way from one
//! labelled mark to the next into a few equal parts (when they are far enough apart to tell apart),
//! and, when zoomed in far enough, a mark for the start of every frame.

use super::TimelineGeometry;

/// Labelled marks are never closer together than this many points. (Whatever zoom, a label has room.)
const SMALLEST_POINTS_BETWEEN_LABELS: f64 = 70.0;

/// The marks between the labelled ones, and the marks for frames, are left out when they would be
/// closer together than this many points.
const SMALLEST_POINTS_BETWEEN_MARKS: f64 = 8.0;

/// How far apart, in whole seconds, the labelled marks can be, with into how many equal parts the
/// way from one of them to the next is divided by the marks between them. The labelled marks are as
/// close together as the zoom allows, so the first of these that is wide enough is the one used.
const LADDER: [(u64, u64); 18] = [
    (1, 10),
    (2, 4),
    (5, 5),
    (10, 10),
    (15, 3),
    (30, 6),
    (60, 6),
    (120, 4),
    (300, 5),
    (600, 10),
    (900, 3),
    (1800, 6),
    (3600, 6),
    (7200, 4),
    (14400, 4),
    (28800, 4),
    (43200, 6),
    (86400, 4),
];

/// One mark of the ruler above the tracks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RulerTick {
    /// Where the mark is along the strip, in points.
    pub x: f64,
    pub kind: RulerTickKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RulerTickKind {
    /// A mark with a label, at a whole number of seconds into the finished video.
    Labelled { seconds: u64 },
    /// A mark between two labelled ones.
    Between,
    /// The start of a frame, for when frames are wide enough to be told apart.
    Frame,
}

/// What the label of a mark `seconds` into the video says: minutes and seconds ("2:05"), with hours
/// first once there are any ("1:02:05").
pub fn time_label(seconds: u64) -> String {
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

impl TimelineGeometry {
    /// How many whole seconds apart the labelled marks are at this zoom, and into how many equal
    /// parts the marks between them divide the way from one to the next.
    fn labelled_interval(&self) -> (u64, u64) {
        LADDER
            .iter()
            .copied()
            .find(|(seconds, _)| *seconds as f64 * self.points_per_second >= SMALLEST_POINTS_BETWEEN_LABELS)
            .unwrap_or(LADDER[LADDER.len() - 1])
    }

    /// The marks of the ruler that are in the view `view_width` wide that starts `scroll_offset`
    /// along the strip, in order from left to right (see the top of this file for what there are).
    /// They follow real time, not frames, so they stay true at frame rates such as 29.97 where a
    /// second is not a whole number of frames; only the marks for frames are where frames are.
    pub fn ruler_ticks(&self, total_frame_count: u64, scroll_offset: f64, view_width: f64) -> Vec<RulerTick> {
        if total_frame_count == 0 || scroll_offset.is_nan() || view_width.is_nan() {
            return Vec::new();
        }
        let view_left = scroll_offset.max(0.0);
        let view_right = (scroll_offset + view_width).min(self.content_width(total_frame_count));
        if view_right < view_left {
            return Vec::new();
        }

        let (labelled_seconds, parts) = self.labelled_interval();
        let labelled_spacing = labelled_seconds as f64 * self.points_per_second;
        let shows_marks_between = labelled_spacing / parts as f64 >= SMALLEST_POINTS_BETWEEN_MARKS;
        // The finest marks that are not for frames: every labelled one, or the ones between too.
        let (spacing, marks_per_label) = if shows_marks_between {
            (labelled_spacing / parts as f64, parts)
        } else {
            (labelled_spacing, 1)
        };
        let first_mark = (view_left / spacing).ceil() as u64;
        let last_mark = (view_right / spacing).floor() as u64;
        let mut ticks: Vec<RulerTick> = (first_mark..=last_mark)
            .map(|mark| RulerTick {
                x: mark as f64 * spacing,
                kind: if mark % marks_per_label == 0 {
                    RulerTickKind::Labelled {
                        seconds: mark / marks_per_label * labelled_seconds,
                    }
                } else {
                    RulerTickKind::Between
                },
            })
            .collect();

        let frame_width = self.x_of_frame(1);
        if frame_width >= SMALLEST_POINTS_BETWEEN_MARKS {
            let seconds_between_marks = spacing / self.points_per_second;
            let first_frame = {
                let frame = self.frame_at_x(view_left).map_or(0, |frame| frame.value());
                if self.x_of_frame(frame) < view_left {
                    frame + 1
                } else {
                    frame
                }
            };
            let last_frame = self.frame_at_x(view_right).map_or(0, |frame| frame.value());
            for frame in first_frame..=last_frame {
                // A frame that starts where another mark is has no mark of its own.
                let seconds = self.frame_rate.seconds_at_frame(frame);
                let nearest = (seconds / seconds_between_marks).round() * seconds_between_marks;
                if (seconds - nearest).abs() <= 1e-9 * seconds.max(1.0) {
                    continue;
                }
                ticks.push(RulerTick {
                    x: self.x_of_frame(frame),
                    kind: RulerTickKind::Frame,
                });
            }
            ticks.sort_by(|first, second| first.x.total_cmp(&second.x));
        }
        ticks
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{assert_close, geometry_at};
    use super::*;

    fn labelled(ticks: &[RulerTick]) -> Vec<(f64, u64)> {
        ticks
            .iter()
            .filter_map(|tick| match tick.kind {
                RulerTickKind::Labelled { seconds } => Some((tick.x, seconds)),
                _ => None,
            })
            .collect()
    }

    fn xs_of(ticks: &[RulerTick], kind: RulerTickKind) -> Vec<f64> {
        ticks
            .iter()
            .filter(|tick| tick.kind == kind)
            .map(|tick| tick.x)
            .collect()
    }

    #[test]
    fn the_ruler_has_a_labelled_mark_for_every_second_and_a_mark_for_every_tenth_between_when_there_is_room() {
        let geometry = geometry_at(30, 1, 100.0);
        // 5 seconds of timeline (150 frames) is 500 points: the view shows all of it.
        let ticks = geometry.ruler_ticks(150, 0.0, 1000.0);

        let seconds = labelled(&ticks);
        assert_eq!(seconds.len(), 6, "0 s, 1 s, ... 5 s: {seconds:?}");
        for (second, (x, labelled_seconds)) in seconds.iter().enumerate() {
            assert_close(*x, second as f64 * 100.0);
            assert_eq!(*labelled_seconds, second as u64, "each says which second it is");
        }
        let tenths = xs_of(&ticks, RulerTickKind::Between);
        assert_eq!(tenths.len(), 5 * 9, "nine between each pair of seconds: {tenths:?}");
        assert_close(tenths[0], 10.0);
        assert_close(tenths[9], 110.0);
        assert!(
            xs_of(&ticks, RulerTickKind::Frame).is_empty(),
            "frames are 3 points wide"
        );
        assert!(ticks.windows(2).all(|pair| pair[0].x < pair[1].x), "in order");
    }

    #[test]
    fn the_marks_between_the_labelled_ones_are_left_out_when_they_would_be_crowded() {
        // Tenths of a second are 7 points apart at 70 points a second.
        let ticks = geometry_at(30, 1, 70.0).ruler_ticks(150, 0.0, 1000.0);
        assert!(xs_of(&ticks, RulerTickKind::Between).is_empty());
        assert_eq!(labelled(&ticks).len(), 6, "a labelled mark for each of 0 s to 5 s");
        // And 8 points apart is enough.
        let ticks = geometry_at(30, 1, 80.0).ruler_ticks(150, 0.0, 1000.0);
        assert_eq!(xs_of(&ticks, RulerTickKind::Between).len(), 45);
    }

    #[test]
    fn the_labelled_marks_are_further_apart_in_time_the_more_the_timeline_is_zoomed_out() {
        // (points a second, whole seconds between labelled marks): the least that leaves 70 points.
        for (points_per_second, seconds) in [
            (1200.0, 1),
            (185.0, 1),
            (70.0, 1),
            (69.9, 2),
            (35.0, 2),
            (34.9, 5),
            (14.0, 5),
            (13.9, 10),
            (7.0, 10),
            (6.9, 15),
            (4.7, 15),
            (4.6, 30),
            (2.34, 30),
            (2.33, 60),
            (2.0, 60),
        ] {
            let ticks = geometry_at(30, 1, points_per_second).ruler_ticks(30 * 3600, 0.0, 100_000.0);
            let marks = labelled(&ticks);
            assert_eq!(
                marks[1].1 - marks[0].1,
                seconds,
                "at {points_per_second} points a second the labelled marks are {seconds} s apart"
            );
        }
    }

    #[test]
    fn labelled_marks_say_the_seconds_they_are_at_which_is_a_whole_number_of_intervals() {
        // 5 s apart at 20 points a second: 100 points.
        let ticks = geometry_at(30, 1, 20.0).ruler_ticks(30 * 60, 0.0, 400.0);
        assert_eq!(
            labelled(&ticks),
            vec![(0.0, 0), (100.0, 5), (200.0, 10), (300.0, 15), (400.0, 20)]
        );
        // The marks between them are a second apart (20 points), four between each pair.
        let between = xs_of(&ticks, RulerTickKind::Between);
        assert_eq!(between.len(), 16);
        assert_close(between[0], 20.0);
        assert_close(between[3], 80.0);
        assert_close(between[4], 120.0);
    }

    #[test]
    fn zoomed_in_far_enough_every_frame_has_a_mark_and_the_frames_that_start_at_another_mark_do_not() {
        // 1200 points a second at 30 frames a second: a frame is 40 points wide, a tenth of a second 120.
        let geometry = geometry_at(30, 1, 1200.0);
        let ticks = geometry.ruler_ticks(300, 0.0, 480.0); // the first 12 frames
        let frames = xs_of(&ticks, RulerTickKind::Frame);
        // Frames 0, 3, 6, 9 and 12 start where a tenth of a second does.
        let expected: Vec<f64> = [1, 2, 4, 5, 7, 8, 10, 11]
            .iter()
            .map(|frame| *frame as f64 * 40.0)
            .collect();
        assert_eq!(frames.len(), expected.len(), "{frames:?}");
        for (actual, expected) in frames.iter().zip(&expected) {
            assert_close(*actual, *expected);
        }
        assert_eq!(xs_of(&ticks, RulerTickKind::Between).len(), 4, "0.1 s to 0.4 s");
        assert_eq!(labelled(&ticks).len(), 1, "only the start: 1 s is 1200 points along");
        assert!(ticks.windows(2).all(|pair| pair[0].x <= pair[1].x), "in order");
    }

    #[test]
    fn frames_get_marks_from_eight_points_wide_on_and_that_depends_on_the_frame_rate() {
        // 30 frames a second: 240 points a second is 8 points a frame (just under it, and just over).
        let at_30 = |points_per_second: f64| {
            xs_of(
                &geometry_at(30, 1, points_per_second).ruler_ticks(300, 0.0, 200.0),
                RulerTickKind::Frame,
            )
        };
        assert!(at_30(239.0).is_empty());
        assert!(!at_30(241.0).is_empty());
        // 60 frames a second needs twice the zoom for the same frame width.
        let at_60 = |points_per_second: f64| {
            xs_of(
                &geometry_at(60, 1, points_per_second).ruler_ticks(600, 0.0, 200.0),
                RulerTickKind::Frame,
            )
        };
        assert!(at_60(479.0).is_empty());
        assert!(!at_60(481.0).is_empty());
    }

    #[test]
    fn the_ruler_only_has_marks_inside_the_view_and_inside_the_strip() {
        let geometry = geometry_at(30, 1, 100.0);
        // 20 seconds of timeline; the view covers 450..850 along the strip.
        let ticks = geometry.ruler_ticks(600, 450.0, 400.0);
        assert!(ticks.iter().all(|tick| (450.0..=850.0).contains(&tick.x)), "{ticks:?}");
        let seconds: Vec<f64> = labelled(&ticks).iter().map(|(x, _)| *x).collect();
        assert_eq!(seconds, vec![500.0, 600.0, 700.0, 800.0]);

        // Past the end of a 5 second strip there is nothing.
        assert!(geometry.ruler_ticks(150, 600.0, 400.0).is_empty());
        assert!(
            geometry.ruler_ticks(0, 0.0, 400.0).is_empty(),
            "an empty timeline has no ruler"
        );
        assert!(geometry.ruler_ticks(600, 0.0, f64::NAN).is_empty());
        assert!(geometry.ruler_ticks(600, f64::NAN, 400.0).is_empty());
        assert!(geometry.ruler_ticks(600, 100.0, -50.0).is_empty());
    }

    #[test]
    fn labelled_marks_follow_real_time_so_they_stay_true_at_ntsc_frame_rates() {
        // At 29.97 frames per second, frame 30 is at 1.001 s: the one second mark is a hair before it.
        let geometry = geometry_at(30000, 1001, 100.0);
        let ticks = geometry.ruler_ticks(300, 0.0, 10_000.0);
        let second_ticks = labelled(&ticks);
        assert_close(second_ticks[1].0, 100.0);
        assert!(geometry.x_of_frame(30) > 100.0);
    }

    #[test]
    fn a_time_is_labelled_with_minutes_and_seconds_and_hours_when_there_are_any() {
        assert_eq!(time_label(0), "0:00");
        assert_eq!(time_label(7), "0:07");
        assert_eq!(time_label(65), "1:05");
        assert_eq!(time_label(600), "10:00");
        assert_eq!(time_label(3599), "59:59");
        assert_eq!(time_label(3600), "1:00:00");
        assert_eq!(time_label(3725), "1:02:05");
        assert_eq!(time_label(36_000), "10:00:00");
    }

    #[test]
    fn at_any_zoom_the_marks_are_where_they_say_inside_the_view_in_order_and_never_crowded() {
        use crate::test_support::DeterministicRandomNumbers;
        let mut random = DeterministicRandomNumbers::with_seed(31);
        for round in 0..300 {
            // Zooms from 2 to 1200 points a second, spread evenly on a logarithmic scale.
            let points_per_second = 2.0 * (600.0f64).powf(random.next_below(1000) as f64 / 1000.0);
            let (numerator, denominator) = [(30, 1), (30000, 1001), (24, 1), (60, 1)][random.next_below(4) as usize];
            let geometry = geometry_at(numerator, denominator, points_per_second);
            let total_frames = 1 + random.next_below(200_000);
            let scroll_offset = random.next_below(100_000) as f64 / 10.0;
            let view_width = 100.0 + random.next_below(30_000) as f64 / 10.0;
            let ticks = geometry.ruler_ticks(total_frames, scroll_offset, view_width);
            let context = format!(
                "round {round}: {points_per_second} points a second, offset {scroll_offset}, width {view_width}"
            );

            let view_right = (scroll_offset + view_width).min(geometry.content_width(total_frames));
            assert!(
                ticks.windows(2).all(|pair| pair[0].x <= pair[1].x),
                "{context}: in order"
            );
            assert!(
                ticks
                    .iter()
                    .all(|tick| tick.x >= scroll_offset - 1e-6 && tick.x <= view_right + 1e-6),
                "{context}: inside the view and the strip"
            );
            assert!(
                ticks.len() as f64 <= 3.0 * (view_width / SMALLEST_POINTS_BETWEEN_MARKS + 3.0),
                "{context}: {} marks are too many",
                ticks.len()
            );

            // Labelled marks: where their seconds say, and with room for a label.
            let marks = labelled(&ticks);
            for (x, seconds) in &marks {
                assert_close(*x, *seconds as f64 * points_per_second);
            }
            for pair in marks.windows(2) {
                assert!(
                    pair[1].0 - pair[0].0 >= SMALLEST_POINTS_BETWEEN_LABELS - 1e-6,
                    "{context}: {pair:?}"
                );
            }
            // Labelled and between marks are never closer than the least allowed (the marks for frames may be).
            let coarse: Vec<f64> = ticks
                .iter()
                .filter(|tick| tick.kind != RulerTickKind::Frame)
                .map(|tick| tick.x)
                .collect();
            for pair in coarse.windows(2) {
                assert!(
                    pair[1] - pair[0] >= SMALLEST_POINTS_BETWEEN_MARKS.min(SMALLEST_POINTS_BETWEEN_LABELS) - 1e-6,
                    "{context}: {pair:?}"
                );
            }
        }
    }
}
