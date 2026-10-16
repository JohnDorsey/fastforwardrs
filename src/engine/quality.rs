// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Adaptive quality (EN-08): when the card cannot make the frames of the preview as fast as the clock goes, the frames are made **smaller** (and the window makes
//! them bigger again on the screen) instead of being dropped; when the picture is still, it is made whole.
//!
//! The governor is a state machine and nothing else: it is told what happened and when, and it says what size the next frame is to be made at, so that it can be
//! tested with times that are made up. What it goes by:
//!
//! * **While the clock is going**, each frame that comes says how long it took from being asked for. A frame is *late* if that was more than [`QualityRules::late_above`]
//!   of the time a frame of the project lasts, and *comfortable* if it was less than [`QualityRules::comfortable_below`] of it. Two late frames in a row make the
//!   picture smaller (whole, half, quarter); ten comfortable ones in a row make it bigger again. A frame that was neither, in the band between, undoes the counting
//!   of both: the band is what keeps it from going up and down between two sizes.
//! * **While the clock is stopped**, a picture that is *still* (what is shown has not changed for [`QualityRules::how_long_before_a_still_picture_is_made_whole`])
//!   is made whole, and a picture that is being changed (the playhead is being dragged over the frames) keeps the size it had, since each frame of it is wanted at once.
//!   The numbers are first guesses, to be tuned with the benchmark.

use std::time::{Duration, Instant};

/// The part of the size of the picture that the engine makes a frame at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PreviewScale {
    /// All of it: one pixel of the frame for each pixel of the picture that is shown (but not more than the picture has).
    Whole,
    /// Half as many across and down, a quarter of the pixels.
    Half,
    /// A quarter as many across and down, a sixteenth of the pixels.
    Quarter,
}

impl PreviewScale {
    /// The part of the width and of the height.
    pub fn as_fraction(self) -> f32 {
        match self {
            Self::Whole => 1.0,
            Self::Half => 0.5,
            Self::Quarter => 0.25,
        }
    }

    fn smaller(self) -> Option<Self> {
        match self {
            Self::Whole => Some(Self::Half),
            Self::Half => Some(Self::Quarter),
            Self::Quarter => None,
        }
    }

    fn bigger(self) -> Option<Self> {
        match self {
            Self::Whole => None,
            Self::Half => Some(Self::Whole),
            Self::Quarter => Some(Self::Half),
        }
    }

    /// The size a frame is made at when the whole picture would be `whole`: each side by the part, rounded, and not less than two pixels.
    pub fn of_the_size(self, whole: (u32, u32)) -> (u32, u32) {
        let part = |side: u32| ((side as f32 * self.as_fraction()).round() as u32).max(2);
        match self {
            Self::Whole => whole,
            _ => (part(whole.0), part(whole.1)),
        }
    }
}

/// The numbers the governor goes by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QualityRules {
    /// A frame is late when it took longer than this part of the time that a frame of the project lasts.
    pub late_above: f32,
    /// A frame is comfortable when it took less than this part of it.
    pub comfortable_below: f32,
    /// How many late frames in a row make the picture smaller.
    pub late_frames_that_make_it_smaller: u32,
    /// How many comfortable frames in a row make it bigger.
    pub comfortable_frames_that_make_it_bigger: u32,
    /// How long what is shown must stay as it is, while the clock is stopped, before the picture is made whole.
    pub how_long_before_a_still_picture_is_made_whole: Duration,
}

impl Default for QualityRules {
    fn default() -> Self {
        Self {
            late_above: 0.8,
            comfortable_below: 0.35,
            late_frames_that_make_it_smaller: 2,
            comfortable_frames_that_make_it_bigger: 10,
            how_long_before_a_still_picture_is_made_whole: Duration::from_millis(120),
        }
    }
}

/// Decides the size the frames of the preview are made at. What it goes by is written at the top of `quality.rs`.
#[derive(Clone, Debug)]
pub struct QualityGovernor {
    rules: QualityRules,
    scale: PreviewScale,
    late_in_a_row: u32,
    comfortable_in_a_row: u32,
    /// Since when what is shown has been as it is, while the clock is stopped (`None` while it is going or what is shown is being changed).
    still_since: Option<Instant>,
}

impl QualityGovernor {
    /// A governor that begins with whole pictures.
    pub fn new(rules: QualityRules) -> Self {
        Self {
            rules,
            scale: PreviewScale::Whole,
            late_in_a_row: 0,
            comfortable_in_a_row: 0,
            still_since: None,
        }
    }

    /// The size that frames are made at now.
    pub fn scale(&self) -> PreviewScale {
        self.scale
    }

    /// A frame that was asked for has come, `latency` after it was asked for, while the clock was going (or not: `is_playing`) and a frame of the project
    /// lasts `frame_duration`. A frame that comes while the clock is stopped says nothing about whether the card can keep up with it.
    pub fn a_frame_came(&mut self, latency: Duration, frame_duration: Duration, is_playing: bool) {
        if !is_playing || frame_duration.is_zero() {
            return;
        }
        let part_of_a_frame = latency.as_secs_f64() / frame_duration.as_secs_f64();
        if part_of_a_frame > f64::from(self.rules.late_above) {
            self.comfortable_in_a_row = 0;
            self.late_in_a_row += 1;
            if self.late_in_a_row >= self.rules.late_frames_that_make_it_smaller {
                self.change_the_scale(PreviewScale::smaller);
            }
        } else if part_of_a_frame < f64::from(self.rules.comfortable_below) {
            self.late_in_a_row = 0;
            self.comfortable_in_a_row += 1;
            if self.comfortable_in_a_row >= self.rules.comfortable_frames_that_make_it_bigger {
                self.change_the_scale(PreviewScale::bigger);
            }
        } else {
            self.late_in_a_row = 0;
            self.comfortable_in_a_row = 0;
        }
    }

    /// The window is drawn at `now`, with the clock going or not (`is_playing`), and what is shown changed since the last time it was drawn or not
    /// (`what_is_shown_changed`). Gives the size that the next frame is to be made at.
    pub fn the_window_is_drawn(&mut self, now: Instant, is_playing: bool, what_is_shown_changed: bool) -> PreviewScale {
        if is_playing || what_is_shown_changed {
            // (Playing: the frames say how it goes. Being changed: each is wanted at once, and the size it had is kept.)
            self.still_since = None;
            return self.scale;
        }
        let since = *self.still_since.get_or_insert(now);
        if self.scale != PreviewScale::Whole
            && now.saturating_duration_since(since) >= self.rules.how_long_before_a_still_picture_is_made_whole
        {
            self.scale = PreviewScale::Whole;
            self.late_in_a_row = 0;
            self.comfortable_in_a_row = 0;
        }
        self.scale
    }

    /// How long from `now` until the window has to be drawn again for the picture that is still to be made whole, if it is waiting to be (the window is not
    /// drawn when nothing changes, so it has to be asked to).
    pub fn how_long_until_the_picture_is_made_whole(&self, now: Instant) -> Option<Duration> {
        if self.scale == PreviewScale::Whole {
            return None;
        }
        self.still_since.map(|since| {
            self.rules
                .how_long_before_a_still_picture_is_made_whole
                .saturating_sub(now.saturating_duration_since(since))
        })
    }

    fn change_the_scale(&mut self, to: fn(PreviewScale) -> Option<PreviewScale>) {
        if let Some(scale) = to(self.scale) {
            self.scale = scale;
        }
        self.late_in_a_row = 0;
        self.comfortable_in_a_row = 0;
    }
}

#[cfg(test)]
mod tests;
