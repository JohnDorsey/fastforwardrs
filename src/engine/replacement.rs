// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! When the engine starts over with a new graphics device, and when it gives up: the decisions of the watchdog, without a graphics card in them.
//!
//! A frame that the card does not finish in time (a shader that is only slow, or one that does not end where the guard's limit on loops did not help), and a
//! device that was lost (the card was reset, the driver was updated), are answered the same way: the device is thrown away and a new one is made, and the
//! engine goes on. What was running on the old device is stopped if the card is a real one; on a software adapter it goes on using the processor until the
//! program ends (the findings of phase 0.5, S2), which is why the engine does not do this for ever: **when a new device is needed again and again, it stops**
//! (the window then makes the preview the old way).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::frame_renderer::RenderProblem;

/// Remembers when the device was replaced, and says when it has been replaced too often.
#[derive(Debug)]
pub struct ReplacementLimiter {
    /// The number of replacements that stops the engine: when this many are needed within `how_long`, the last of them is not made.
    stopping_number: usize,
    how_long: Duration,
    made: VecDeque<Instant>,
}

impl ReplacementLimiter {
    pub fn new(stopping_number: usize, how_long: Duration) -> Self {
        Self {
            stopping_number,
            how_long,
            made: VecDeque::new(),
        }
    }

    /// Whether the device may be replaced at `now`: not if it was replaced `stopping_number - 1` times within `how_long` before it already. A replacement that is
    /// allowed is remembered.
    pub fn allow_another(&mut self, now: Instant) -> bool {
        while self
            .made
            .front()
            .is_some_and(|oldest| now.saturating_duration_since(*oldest) >= self.how_long)
        {
            self.made.pop_front();
        }
        if self.made.len() + 1 >= self.stopping_number {
            return false;
        }
        self.made.push_back(now);
        true
    }

    /// How many replacements are remembered (made within `how_long` of the last time this was asked of it).
    #[cfg(test)]
    pub fn remembered(&self) -> usize {
        self.made.len()
    }
}

/// How the engine says why it is replacing a device that was lost, whose own words for it are `reason`.
pub fn why_the_device_was_lost(reason: &str) -> String {
    format!("the graphics device was lost ({reason})")
}

/// What the engine does about a frame that could not be made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AfterAFailedFrame {
    /// The frame is told to have failed, and that is all: the card is all right and what was asked of it was not.
    GoOn,
    /// The device is replaced, and the effects that were in the frame are not trusted again.
    ReplaceTheDevice { why: String },
}

/// What to do about the failed frame whose trouble was `problem`, when `device_lost` is what the device said about itself since (the reason, if it was lost).
pub fn what_to_do_after_a_failed_frame(
    problem: &RenderProblem,
    device_lost: Option<String>,
    frame_timeout: Duration,
) -> AfterAFailedFrame {
    if let Some(reason) = device_lost {
        return AfterAFailedFrame::ReplaceTheDevice {
            why: why_the_device_was_lost(&reason),
        };
    }
    match problem {
        RenderProblem::TimedOut => AfterAFailedFrame::ReplaceTheDevice {
            why: format!(
                "the graphics card did not finish a frame in {:.1} seconds",
                frame_timeout.as_secs_f32()
            ),
        },
        // The rest are mistakes in what the engine was asked or what it asked: the card did what it was told and found it wrong, and it is as good as it was.
        RenderProblem::BadSize { .. }
        | RenderProblem::GraphicsMistake(_)
        | RenderProblem::ReadbackFailed(_)
        | RenderProblem::PicturesMissing(_)
        | RenderProblem::PicturesOfAnotherWorkingSpace(_)
        | RenderProblem::BadPicture(_)
        | RenderProblem::BadEffectShader(_) => AfterAFailedFrame::GoOn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_MINUTE: Duration = Duration::from_secs(60);

    #[test]
    fn the_first_replacements_are_allowed_and_the_one_that_makes_it_the_stopping_number_is_not() {
        let start = Instant::now();
        let mut limiter = ReplacementLimiter::new(3, A_MINUTE);
        assert!(limiter.allow_another(start));
        assert!(limiter.allow_another(start + Duration::from_secs(1)));
        assert!(
            !limiter.allow_another(start + Duration::from_secs(2)),
            "the third within the minute stops it"
        );
        assert!(
            !limiter.allow_another(start + Duration::from_secs(3)),
            "and so does every one after it while the first two are still within the minute"
        );
        assert_eq!(
            limiter.remembered(),
            2,
            "a replacement that was not made is not remembered"
        );
    }

    #[test]
    fn replacements_that_are_a_minute_old_are_forgotten() {
        let start = Instant::now();
        let mut limiter = ReplacementLimiter::new(3, A_MINUTE);
        assert!(limiter.allow_another(start));
        assert!(limiter.allow_another(start + Duration::from_secs(10)));
        // The first is not yet a minute old (59.999 seconds), the second is 49 seconds old: refused.
        assert!(!limiter.allow_another(start + Duration::from_millis(59_999)));
        // At exactly a minute the first is old enough to be forgotten: allowed, and then the second and this one are what is remembered.
        assert!(limiter.allow_another(start + A_MINUTE));
        assert_eq!(limiter.remembered(), 2);
        // And the second is forgotten ten seconds later, while the one just made is not.
        assert!(limiter.allow_another(start + A_MINUTE + Duration::from_secs(10)));
        assert!(!limiter.allow_another(start + A_MINUTE + Duration::from_secs(11)));
    }

    #[test]
    fn a_device_that_is_replaced_slowly_is_replaced_for_ever() {
        let start = Instant::now();
        let mut limiter = ReplacementLimiter::new(3, A_MINUTE);
        for number in 0..50 {
            assert!(
                limiter.allow_another(start + Duration::from_secs(40 * number)),
                "every 40 seconds is only one or two within a minute: number {number}"
            );
        }
    }

    #[test]
    fn a_stopping_number_of_one_never_allows_a_replacement_and_of_two_allows_one_in_a_minute() {
        let start = Instant::now();
        let mut never = ReplacementLimiter::new(1, A_MINUTE);
        assert!(!never.allow_another(start));
        let mut one = ReplacementLimiter::new(2, A_MINUTE);
        assert!(one.allow_another(start));
        assert!(!one.allow_another(start + Duration::from_secs(1)));
        assert!(one.allow_another(start + A_MINUTE));
    }

    const THE_TIMEOUT: Duration = Duration::from_millis(2500);

    #[test]
    fn a_frame_that_did_not_finish_makes_a_new_device_and_the_words_say_how_long_it_was_given() {
        let after = what_to_do_after_a_failed_frame(&RenderProblem::TimedOut, None, THE_TIMEOUT);
        let AfterAFailedFrame::ReplaceTheDevice { why } = after else {
            panic!("{after:?}")
        };
        assert!(why.contains("2.5 seconds"), "{why}");
    }

    #[test]
    fn a_device_that_was_lost_makes_a_new_device_whatever_the_frame_came_to() {
        let problems = [
            RenderProblem::TimedOut,
            RenderProblem::GraphicsMistake("a mistake".to_owned()),
            RenderProblem::ReadbackFailed("not read".to_owned()),
            RenderProblem::BadSize {
                width: 0,
                height: 1,
                largest: 8192,
            },
            RenderProblem::PicturesMissing(Vec::new()),
        ];
        for problem in problems {
            let after = what_to_do_after_a_failed_frame(&problem, Some("Destroyed: it was".to_owned()), THE_TIMEOUT);
            let AfterAFailedFrame::ReplaceTheDevice { why } = after else {
                panic!("{problem:?}: {after:?}")
            };
            assert!(why.contains("lost") && why.contains("Destroyed: it was"), "{why}");
        }
    }

    #[test]
    fn a_frame_that_failed_because_of_what_it_was_asked_does_not_make_a_new_device() {
        let problems = [
            RenderProblem::BadSize {
                width: 0,
                height: 1,
                largest: 8192,
            },
            RenderProblem::GraphicsMistake("a mistake".to_owned()),
            RenderProblem::ReadbackFailed("not read".to_owned()),
            RenderProblem::PicturesMissing(Vec::new()),
            RenderProblem::BadPicture("a picture".to_owned()),
            RenderProblem::BadEffectShader("a shader".to_owned()),
        ];
        for problem in problems {
            assert_eq!(
                what_to_do_after_a_failed_frame(&problem, None, THE_TIMEOUT),
                AfterAFailedFrame::GoOn,
                "{problem:?}"
            );
        }
    }
}
