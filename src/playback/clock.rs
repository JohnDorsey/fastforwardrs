// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The playback clock: which frame of the timeline is the current one, while playing or paused.

use std::time::{Duration, Instant};

use crate::project::{FrameRate, TimelineFrameIndex};

const NANOSECONDS_PER_SECOND: u128 = 1_000_000_000;

#[derive(Clone, Copy, Debug)]
enum ClockState {
    Paused {
        position: u64,
    },
    /// Playing: `started_position` was the current frame at the moment `started_at`.
    Playing {
        started_at: Instant,
        started_position: u64,
    },
}

/// Keeps track of the current frame of the timeline.
///
/// While playing, the current frame follows the real time: a 30 frames per second video advances
/// 30 frames in every second, however fast or slowly the screen is drawn. The clock never looks at
/// the time itself; it is told what time it is, which makes it simple to test.
#[derive(Clone, Copy, Debug)]
pub struct PlaybackClock {
    frame_rate: FrameRate,
    state: ClockState,
    /// How many times the current frame has jumped (see [`Self::jumps`]).
    jumps: u64,
}

impl PlaybackClock {
    /// A paused clock at the first frame.
    pub fn new(frame_rate: FrameRate) -> Self {
        Self {
            frame_rate,
            state: ClockState::Paused { position: 0 },
            jumps: 0,
        }
    }

    /// How many times the current frame has **jumped**: gone to a frame that is not the one that it was at or the one that playing would have brought it to (a seek to another frame; stopping, which
    /// goes back to the first; playing again from the last frame, which starts at the first). Playing, pausing and the time going by are not jumps. What depends on earlier frames (the states of
    /// effects) is no longer what the frame asks for after one, and the preview looks at this number to know when that has happened.
    pub fn jumps(&self) -> u64 {
        self.jumps
    }

    pub fn is_playing(&self) -> bool {
        matches!(self.state, ClockState::Playing { .. })
    }

    /// The current frame at time `now`, for a timeline of `total_frame_count` frames. It is never
    /// beyond the last frame (and is the first frame when the timeline is empty).
    pub fn position(&self, now: Instant, total_frame_count: u64) -> TimelineFrameIndex {
        let last_frame = total_frame_count.saturating_sub(1);
        let position = match self.state {
            ClockState::Paused { position } => position,
            ClockState::Playing {
                started_at,
                started_position,
            } => started_position.saturating_add(frames_elapsed(
                self.frame_rate,
                now.saturating_duration_since(started_at),
            )),
        };
        TimelineFrameIndex::new(position.min(last_frame))
    }

    /// The time into the timeline at `now`, in seconds, more exactly than the whole frame that
    /// [`Self::position`] gives (which is where that frame begins). While playing it moves on
    /// smoothly with the real time, up to the start of the last frame, where playing stops. This
    /// is what the sound is kept in step with.
    pub fn position_in_seconds(&self, now: Instant, total_frame_count: u64) -> f64 {
        let last_frame_seconds = self.frame_rate.seconds_at_frame(total_frame_count.saturating_sub(1));
        let seconds = match self.state {
            ClockState::Paused { position } => self.frame_rate.seconds_at_frame(position),
            ClockState::Playing {
                started_at,
                started_position,
            } => {
                self.frame_rate.seconds_at_frame(started_position)
                    + now.saturating_duration_since(started_at).as_secs_f64()
            }
        };
        seconds.min(last_frame_seconds)
    }

    /// Brings the clock up to date: if playing has gone past the last frame, playing stops there.
    /// Returns whether it stopped just now.
    pub fn stop_at_the_end(&mut self, now: Instant, total_frame_count: u64) -> bool {
        let ClockState::Playing {
            started_at,
            started_position,
        } = self.state
        else {
            return false;
        };
        let wanted_position = started_position.saturating_add(frames_elapsed(
            self.frame_rate,
            now.saturating_duration_since(started_at),
        ));
        let last_frame = total_frame_count.saturating_sub(1);
        if wanted_position >= last_frame {
            self.state = ClockState::Paused { position: last_frame };
            return true;
        }
        false
    }

    /// Starts playing from the current frame. If the current frame is the last one, playing starts
    /// again from the first.
    pub fn play(&mut self, now: Instant, total_frame_count: u64) {
        if self.is_playing() {
            return;
        }
        let position = self.position(now, total_frame_count).value();
        let start = if position >= total_frame_count.saturating_sub(1) {
            0
        } else {
            position
        };
        if start != position {
            self.jumps = self.jumps.saturating_add(1);
        }
        self.state = ClockState::Playing {
            started_at: now,
            started_position: start,
        };
    }

    /// Stops playing and stays on the current frame.
    pub fn pause(&mut self, now: Instant, total_frame_count: u64) {
        let position = self.position(now, total_frame_count).value();
        self.state = ClockState::Paused { position };
    }

    /// Stops playing and goes back to the first frame.
    pub fn stop(&mut self) {
        self.go_back_to_the_first_frame();
    }

    /// Paused on the first frame. (It is a jump unless the clock was paused on it: while playing nobody knows without the time that it is on it.)
    fn go_back_to_the_first_frame(&mut self) {
        if !matches!(self.state, ClockState::Paused { position: 0 }) {
            self.jumps = self.jumps.saturating_add(1);
        }
        self.state = ClockState::Paused { position: 0 };
    }

    /// Jumps to `position`. If playing, playing carries on from there.
    pub fn seek(&mut self, now: Instant, position: TimelineFrameIndex, total_frame_count: u64) {
        let position = position.value().min(total_frame_count.saturating_sub(1));
        if position != self.position(now, total_frame_count).value() {
            self.jumps = self.jumps.saturating_add(1);
        }
        self.state = if self.is_playing() {
            ClockState::Playing {
                started_at: now,
                started_position: position,
            }
        } else {
            ClockState::Paused { position }
        };
    }

    /// Changes the frame rate (when a different video is loaded) and goes back to the start.
    pub fn reset_for_frame_rate(&mut self, frame_rate: FrameRate) {
        self.frame_rate = frame_rate;
        self.go_back_to_the_first_frame();
    }
}

/// How many whole frames fit in `elapsed` at `frame_rate`. Exact integer arithmetic.
fn frames_elapsed(frame_rate: FrameRate, elapsed: Duration) -> u64 {
    let numerator = elapsed.as_nanos() * u128::from(frame_rate.numerator());
    let denominator = NANOSECONDS_PER_SECOND * u128::from(frame_rate.denominator());
    u64::try_from(numerator / denominator).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOTAL_FRAMES: u64 = 900;

    fn clock(numerator: u32, denominator: u32) -> PlaybackClock {
        PlaybackClock::new(FrameRate::new(numerator, denominator).unwrap())
    }

    fn after(start: Instant, milliseconds: u64) -> Instant {
        start + Duration::from_millis(milliseconds)
    }

    fn position(clock: &PlaybackClock, now: Instant) -> u64 {
        clock.position(now, TOTAL_FRAMES).value()
    }

    #[test]
    fn a_new_clock_is_paused_at_the_first_frame_and_stays_there() {
        let start = Instant::now();
        let clock = clock(30, 1);
        assert!(!clock.is_playing());
        assert_eq!(position(&clock, start), 0);
        assert_eq!(position(&clock, after(start, 10_000)), 0);
    }

    #[test]
    fn playing_advances_in_real_time() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.play(start, TOTAL_FRAMES);
        assert!(clock.is_playing());
        assert_eq!(position(&clock, start), 0);
        assert_eq!(position(&clock, after(start, 32)), 0, "just under one frame");
        assert_eq!(position(&clock, after(start, 34)), 1);
        assert_eq!(position(&clock, after(start, 1000)), 30);
        assert_eq!(position(&clock, after(start, 10_000)), 300);
    }

    #[test]
    fn the_time_in_seconds_is_exact_where_the_frame_is_only_whole() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.play(start, TOTAL_FRAMES);
        assert_eq!(clock.position_in_seconds(start, TOTAL_FRAMES), 0.0);
        let at_half_a_frame = clock.position_in_seconds(after(start, 16), TOTAL_FRAMES);
        assert!((at_half_a_frame - 0.016).abs() < 1e-9, "{at_half_a_frame}");
        assert_eq!(
            position(&clock, after(start, 16)),
            0,
            "while the frame is still the first"
        );
        let after_a_second = clock.position_in_seconds(after(start, 1000), TOTAL_FRAMES);
        assert!((after_a_second - 1.0).abs() < 1e-9, "{after_a_second}");
    }

    #[test]
    fn the_time_in_seconds_starts_from_wherever_playing_started_and_a_paused_clock_stands_still() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.seek(start, TimelineFrameIndex::new(90), TOTAL_FRAMES);
        assert_eq!(
            clock.position_in_seconds(after(start, 5000), TOTAL_FRAMES),
            3.0,
            "paused at 3 s"
        );
        clock.play(start, TOTAL_FRAMES);
        let later = clock.position_in_seconds(after(start, 500), TOTAL_FRAMES);
        assert!((later - 3.5).abs() < 1e-9, "{later}");
        clock.pause(after(start, 500), TOTAL_FRAMES);
        let paused_at = clock.position_in_seconds(after(start, 9000), TOTAL_FRAMES);
        assert_eq!(paused_at, clock.position_in_seconds(after(start, 500), TOTAL_FRAMES));
        assert_eq!(
            paused_at,
            3.5 - (3.5 * 30.0 % 1.0) / 30.0,
            "a paused clock is on a whole frame"
        );
    }

    #[test]
    fn the_time_in_seconds_never_goes_past_the_start_of_the_last_frame() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.play(start, 90);
        let seconds = clock.position_in_seconds(after(start, 60_000), 90);
        assert!((seconds - 89.0 / 30.0).abs() < 1e-9, "{seconds}");
        assert_eq!(
            clock.position_in_seconds(start, 0),
            0.0,
            "with nothing to play it is zero"
        );
    }

    #[test]
    fn the_time_in_seconds_agrees_with_the_frame_at_a_fractional_frame_rate() {
        let start = Instant::now();
        let mut clock = clock(30000, 1001);
        clock.play(start, 1_000_000);
        let seconds = clock.position_in_seconds(after(start, 1001), 1_000_000);
        assert!((seconds - 1.001).abs() < 1e-9);
        assert_eq!(clock.position(after(start, 1001), 1_000_000).value(), 30);
    }

    #[test]
    fn fractional_frame_rates_stay_exact() {
        let start = Instant::now();
        let mut clock = clock(30000, 1001);
        clock.play(start, 1_000_000);
        // 29.97 fps: exactly 30 frames pass in 1.001 seconds, and 30000 in 1001 seconds.
        assert_eq!(clock.position(after(start, 1000), 1_000_000).value(), 29);
        assert_eq!(clock.position(after(start, 1001), 1_000_000).value(), 30);
        assert_eq!(clock.position(after(start, 1_001_000), 1_000_000).value(), 30000);
    }

    #[test]
    fn pausing_freezes_the_current_frame_and_playing_again_continues_from_it() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.play(start, TOTAL_FRAMES);
        clock.pause(after(start, 1000), TOTAL_FRAMES);
        assert!(!clock.is_playing());
        assert_eq!(position(&clock, after(start, 1000)), 30);
        assert_eq!(position(&clock, after(start, 5000)), 30, "paused time does not count");

        clock.play(after(start, 5000), TOTAL_FRAMES);
        assert_eq!(position(&clock, after(start, 5000)), 30);
        assert_eq!(position(&clock, after(start, 6000)), 60);
    }

    #[test]
    fn stopping_goes_back_to_the_first_frame_and_stays_paused() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.play(start, TOTAL_FRAMES);
        clock.stop();
        assert!(!clock.is_playing());
        assert_eq!(position(&clock, after(start, 3000)), 0);
    }

    #[test]
    fn seeking_while_paused_moves_the_frame_and_stays_paused() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.seek(start, TimelineFrameIndex::new(400), TOTAL_FRAMES);
        assert!(!clock.is_playing());
        assert_eq!(position(&clock, after(start, 2000)), 400);
    }

    #[test]
    fn seeking_while_playing_carries_on_playing_from_the_new_frame() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.play(start, TOTAL_FRAMES);
        clock.seek(after(start, 500), TimelineFrameIndex::new(400), TOTAL_FRAMES);
        assert!(clock.is_playing());
        assert_eq!(position(&clock, after(start, 500)), 400);
        assert_eq!(position(&clock, after(start, 1500)), 430);
    }

    #[test]
    fn seeking_beyond_the_end_goes_to_the_last_frame() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.seek(start, TimelineFrameIndex::new(5000), TOTAL_FRAMES);
        assert_eq!(position(&clock, start), TOTAL_FRAMES - 1);
    }

    #[test]
    fn the_position_never_goes_past_the_last_frame_and_playing_stops_there() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.seek(start, TimelineFrameIndex::new(880), TOTAL_FRAMES);
        clock.play(start, TOTAL_FRAMES);
        assert_eq!(position(&clock, after(start, 10_000)), TOTAL_FRAMES - 1);
        assert!(
            !clock.stop_at_the_end(after(start, 100), TOTAL_FRAMES),
            "frame 883 is not the end"
        );
        assert!(
            clock.stop_at_the_end(after(start, 10_000), TOTAL_FRAMES),
            "it stops just now"
        );
        assert!(!clock.is_playing());
        assert_eq!(position(&clock, after(start, 20_000)), TOTAL_FRAMES - 1);
        assert!(
            !clock.stop_at_the_end(after(start, 20_000), TOTAL_FRAMES),
            "already stopped"
        );
    }

    #[test]
    fn playing_from_the_last_frame_starts_again_from_the_first() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.seek(start, TimelineFrameIndex::new(TOTAL_FRAMES - 1), TOTAL_FRAMES);
        clock.play(start, TOTAL_FRAMES);
        assert_eq!(position(&clock, start), 0);
        assert_eq!(position(&clock, after(start, 1000)), 30);
    }

    #[test]
    fn a_new_clock_has_not_jumped_and_playing_pausing_and_the_time_going_by_are_not_jumps() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        assert_eq!(clock.jumps(), 0);
        clock.play(start, TOTAL_FRAMES);
        let _ = clock.position(after(start, 5000), TOTAL_FRAMES);
        clock.pause(after(start, 5000), TOTAL_FRAMES);
        clock.play(after(start, 6000), TOTAL_FRAMES);
        clock.stop_at_the_end(after(start, 100_000), TOTAL_FRAMES);
        assert_eq!(
            clock.jumps(),
            0,
            "the frame went on by itself, and the end is not a jump"
        );
    }

    #[test]
    fn a_seek_to_another_frame_is_a_jump_while_paused_and_while_playing() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.seek(start, TimelineFrameIndex::new(400), TOTAL_FRAMES);
        assert_eq!(clock.jumps(), 1);
        clock.seek(start, TimelineFrameIndex::new(100), TOTAL_FRAMES);
        assert_eq!(clock.jumps(), 2);
        clock.play(start, TOTAL_FRAMES);
        clock.seek(after(start, 500), TimelineFrameIndex::new(400), TOTAL_FRAMES);
        assert_eq!(clock.jumps(), 3, "while playing, from frame 115 to frame 400");
    }

    #[test]
    fn a_seek_to_the_frame_that_the_clock_is_on_is_not_a_jump() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.seek(start, TimelineFrameIndex::new(400), TOTAL_FRAMES);
        assert_eq!(clock.jumps(), 1);
        clock.seek(start, TimelineFrameIndex::new(400), TOTAL_FRAMES);
        assert_eq!(clock.jumps(), 1, "paused on frame 400, and it stays there");
        clock.play(start, TOTAL_FRAMES);
        // (An edit that makes the window seek to the frame it is playing at again moves nothing.)
        let at_frame_430 = after(start, 1000);
        clock.seek(at_frame_430, TimelineFrameIndex::new(430), TOTAL_FRAMES);
        assert_eq!(clock.jumps(), 1, "playing, and the frame is the one it is at");
        clock.seek(at_frame_430, TimelineFrameIndex::new(431), TOTAL_FRAMES);
        assert_eq!(clock.jumps(), 2, "one frame ahead is another frame");
    }

    #[test]
    fn a_seek_beyond_the_end_is_a_jump_to_the_last_frame_and_not_a_jump_when_it_is_on_it() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.seek(start, TimelineFrameIndex::new(5000), TOTAL_FRAMES);
        assert_eq!(clock.jumps(), 1);
        clock.seek(start, TimelineFrameIndex::new(6000), TOTAL_FRAMES);
        assert_eq!(clock.jumps(), 1, "it is on the last frame already");
    }

    #[test]
    fn stopping_and_changing_the_frame_rate_are_jumps_to_the_first_frame_unless_it_is_there() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.stop();
        assert_eq!(clock.jumps(), 0, "paused on the first frame already");
        clock.seek(start, TimelineFrameIndex::new(50), TOTAL_FRAMES);
        clock.stop();
        assert_eq!(clock.jumps(), 2, "the seek and the stop");
        clock.reset_for_frame_rate(FrameRate::new(60, 1).unwrap());
        assert_eq!(clock.jumps(), 2, "on the first frame already");
        clock.play(start, TOTAL_FRAMES);
        clock.stop();
        assert_eq!(
            clock.jumps(),
            3,
            "while playing it is not known to be on the first frame"
        );
        clock.play(start, TOTAL_FRAMES);
        clock.reset_for_frame_rate(FrameRate::new(30, 1).unwrap());
        assert_eq!(clock.jumps(), 4);
    }

    #[test]
    fn playing_again_from_the_last_frame_is_a_jump_to_the_first() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.seek(start, TimelineFrameIndex::new(TOTAL_FRAMES - 1), TOTAL_FRAMES);
        assert_eq!(clock.jumps(), 1);
        clock.play(start, TOTAL_FRAMES);
        assert_eq!(clock.jumps(), 2, "from the last frame to the first");
        // (With one frame the last is the first, and nothing moves.)
        let mut one_frame = clock;
        one_frame.pause(start, 1);
        let before = one_frame.jumps();
        one_frame.play(start, 1);
        assert_eq!(one_frame.jumps(), before);
    }

    #[test]
    fn an_empty_timeline_always_stays_on_the_first_frame() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.play(start, 0);
        assert_eq!(clock.position(after(start, 5000), 0).value(), 0);
        assert!(clock.stop_at_the_end(after(start, 5000), 0));
    }

    #[test]
    fn a_very_long_time_does_not_overflow() {
        let start = Instant::now();
        let mut clock = clock(60000, 1001);
        clock.play(start, u64::MAX);
        let ten_hours = Duration::from_secs(10 * 3600);
        let frames = clock.position(start + ten_hours, u64::MAX).value();
        // 36000 seconds at 60000/1001 frames per second: 2,160,000,000 / 1001 = 2,157,842.157...
        assert_eq!(frames, 2_157_842);
    }

    #[test]
    fn a_time_before_the_start_counts_as_no_time_at_all() {
        let start = Instant::now() + Duration::from_secs(5);
        let mut clock = clock(30, 1);
        clock.play(start, TOTAL_FRAMES);
        assert_eq!(clock.position(start - Duration::from_secs(1), TOTAL_FRAMES).value(), 0);
    }

    #[test]
    fn changing_the_frame_rate_resets_the_clock() {
        let start = Instant::now();
        let mut clock = clock(30, 1);
        clock.play(start, TOTAL_FRAMES);
        clock.reset_for_frame_rate(FrameRate::new(60, 1).unwrap());
        assert!(!clock.is_playing());
        assert_eq!(position(&clock, after(start, 1000)), 0);
        clock.play(start, TOTAL_FRAMES);
        assert_eq!(position(&clock, after(start, 1000)), 60);
    }
}
