// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Showing a timeline position as `hh:mm:ss.ff`, where `ff` is the frame within the second.

use super::{FrameRate, TimelineFrameIndex};

/// Formats `position` as `hh:mm:ss.ff`.
///
/// The seconds are real seconds (so they stay in step with a clock even for a rate such as
/// 29.97 frames per second), and `ff` counts the frames inside that second, starting at 0. `ff`
/// has two digits, or more when the frame rate needs it (a 120 frames-per-second video counts
/// frames 000 to 119).
pub fn format_timecode(position: TimelineFrameIndex, frame_rate: FrameRate) -> String {
    let frame_number = position.value();
    let whole_seconds = frame_rate.whole_second_containing_frame(frame_number);
    let frame_within_second = frame_number - frame_rate.first_frame_in_whole_second(whole_seconds);

    let hours = whole_seconds / 3600;
    let minutes = whole_seconds / 60 % 60;
    let seconds = whole_seconds % 60;
    let frame_digit_count = digit_count(frame_rate.most_frames_in_one_second() - 1).max(2);

    format!("{hours:02}:{minutes:02}:{seconds:02}.{frame_within_second:0frame_digit_count$}")
}

fn digit_count(number: u32) -> usize {
    number.to_string().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timecode(frame_number: u64, numerator: u32, denominator: u32) -> String {
        format_timecode(
            TimelineFrameIndex::new(frame_number),
            FrameRate::new(numerator, denominator).unwrap(),
        )
    }

    #[test]
    fn counts_frames_within_each_second_at_thirty_frames_per_second() {
        assert_eq!(timecode(0, 30, 1), "00:00:00.00");
        assert_eq!(timecode(1, 30, 1), "00:00:00.01");
        assert_eq!(timecode(29, 30, 1), "00:00:00.29");
        assert_eq!(timecode(30, 30, 1), "00:00:01.00");
        assert_eq!(timecode(59, 30, 1), "00:00:01.29");
        assert_eq!(timecode(30 * 60, 30, 1), "00:01:00.00");
        assert_eq!(timecode(30 * 3600, 30, 1), "01:00:00.00");
        assert_eq!(timecode(30 * 3600 + 30 * 61 + 7, 30, 1), "01:01:01.07");
    }

    #[test]
    fn hours_beyond_two_digits_are_not_truncated() {
        assert_eq!(timecode(30 * 3600 * 123, 30, 1), "123:00:00.00");
    }

    #[test]
    fn other_whole_frame_rates_use_their_own_frame_counts() {
        assert_eq!(timecode(23, 24, 1), "00:00:00.23");
        assert_eq!(timecode(24, 24, 1), "00:00:01.00");
        assert_eq!(timecode(59, 60, 1), "00:00:00.59");
        assert_eq!(timecode(60, 60, 1), "00:00:01.00");
    }

    #[test]
    fn a_rate_above_one_hundred_gets_three_frame_digits() {
        assert_eq!(timecode(5, 120, 1), "00:00:00.005");
        assert_eq!(timecode(119, 120, 1), "00:00:00.119");
        assert_eq!(timecode(120, 120, 1), "00:00:01.000");
    }

    #[test]
    fn fractional_rates_keep_seconds_in_step_with_the_clock() {
        // 29.97 fps: the first second has 30 frames (0..=29), and the clock reaches 1.001 s at
        // frame 30, so second 1 starts there.
        assert_eq!(timecode(29, 30000, 1001), "00:00:00.29");
        assert_eq!(timecode(30, 30000, 1001), "00:00:01.00");
        // 30000 frames are exactly 1001 s = 16 min 41 s.
        assert_eq!(timecode(30000, 30000, 1001), "00:16:41.00");
    }

    #[test]
    fn every_frame_gets_a_distinct_strictly_increasing_timecode() {
        for (numerator, denominator) in [
            (24, 1),
            (24000, 1001),
            (30, 1),
            (30000, 1001),
            (60, 1),
            (60000, 1001),
            (25, 1),
        ] {
            let frame_rate = FrameRate::new(numerator, denominator).unwrap();
            let mut previous: Option<(u64, u32)> = None; // (whole seconds, frame within second)
            for frame_number in 0..200_000u64 {
                let whole_seconds = frame_rate.whole_second_containing_frame(frame_number);
                let frame_within_second = (frame_number - frame_rate.first_frame_in_whole_second(whole_seconds)) as u32;
                assert!(
                    frame_within_second < frame_rate.most_frames_in_one_second(),
                    "frame {frame_number} at {frame_rate} shows ff={frame_within_second}"
                );
                if let Some((previous_seconds, previous_frame)) = previous {
                    let is_next_frame_in_same_second =
                        whole_seconds == previous_seconds && frame_within_second == previous_frame + 1;
                    let is_first_frame_of_next_second =
                        whole_seconds == previous_seconds + 1 && frame_within_second == 0;
                    assert!(
                        is_next_frame_in_same_second || is_first_frame_of_next_second,
                        "frame {frame_number} at {frame_rate}: ({previous_seconds}, {previous_frame}) -> ({whole_seconds}, {frame_within_second})"
                    );
                }
                previous = Some((whole_seconds, frame_within_second));
            }
        }
    }
}
