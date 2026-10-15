// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The slider along the bottom that spans the whole timeline: dragging it moves the current frame.

use crate::project::TimelineFrameIndex;

/// The frame at `fraction` of the way along the slider (0 is the left end, 1 the right end), for
/// a timeline of `total_frame_count` frames. The ends of the slider are the first and last frames.
pub fn frame_at_slider_fraction(fraction: f32, total_frame_count: u64) -> TimelineFrameIndex {
    let last_frame = total_frame_count.saturating_sub(1);
    if fraction.is_nan() {
        return TimelineFrameIndex::new(0);
    }
    let frame = (f64::from(fraction.clamp(0.0, 1.0)) * last_frame as f64).round() as u64;
    TimelineFrameIndex::new(frame.min(last_frame))
}

/// How far along the slider `frame` is, from 0 (the first frame) to 1 (the last).
pub fn slider_fraction_of_frame(frame: TimelineFrameIndex, total_frame_count: u64) -> f32 {
    let last_frame = total_frame_count.saturating_sub(1);
    if last_frame == 0 {
        return 0.0;
    }
    (frame.value().min(last_frame) as f64 / last_frame as f64) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_at(fraction: f32, total: u64) -> u64 {
        frame_at_slider_fraction(fraction, total).value()
    }

    #[test]
    fn the_ends_of_the_slider_are_the_first_and_last_frames() {
        assert_eq!(frame_at(0.0, 600), 0);
        assert_eq!(frame_at(1.0, 600), 599);
        assert_eq!(frame_at(0.5, 601), 300);
    }

    #[test]
    fn positions_outside_the_slider_stay_on_the_nearest_end() {
        assert_eq!(frame_at(-0.5, 600), 0);
        assert_eq!(frame_at(7.0, 600), 599);
        assert_eq!(frame_at(f32::NAN, 600), 0);
        assert_eq!(frame_at(f32::INFINITY, 600), 599);
    }

    #[test]
    fn a_timeline_of_one_frame_or_none_always_gives_the_first_frame() {
        assert_eq!(frame_at(0.7, 1), 0);
        assert_eq!(frame_at(0.7, 0), 0);
        assert_eq!(slider_fraction_of_frame(TimelineFrameIndex::new(0), 1), 0.0);
        assert_eq!(slider_fraction_of_frame(TimelineFrameIndex::new(0), 0), 0.0);
    }

    #[test]
    fn frames_map_to_slider_positions_and_back() {
        for total in [2u64, 3, 100, 600, 5_000, 108_000] {
            for frame in [0, 1, total / 3, total / 2, total - 2, total - 1] {
                let fraction = slider_fraction_of_frame(TimelineFrameIndex::new(frame), total);
                assert!((0.0..=1.0).contains(&fraction));
                assert_eq!(frame_at(fraction, total), frame, "frame {frame} of {total}");
            }
        }
    }

    #[test]
    fn dragging_across_the_slider_visits_frames_in_order() {
        let frames: Vec<u64> = (0..=100).map(|step| frame_at(step as f32 / 100.0, 600)).collect();
        assert!(frames.windows(2).all(|pair| pair[0] <= pair[1]), "{frames:?}");
        assert_eq!(frames.first(), Some(&0));
        assert_eq!(frames.last(), Some(&599));
    }
}
