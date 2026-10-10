//! The vertical slider that appears while the right mouse button is dragged on a clip, and sets
//! how many times faster the clip plays.
//!
//! The slider is logarithmic: each tenfold increase in speed takes the same height. That keeps
//! the small factors (2x to 10x), which are the ones used most, easy to hit exactly, while the
//! whole range up to 1000x still fits on the screen. While dragging, the arrow keys (and the
//! mouse wheel) change the speed by one for the factors in between.

use crate::project::SpeedFactor;

/// The largest speed factor the slider offers. (Project files may hold larger ones.)
pub const LARGEST_SPEED_FACTOR_ON_THE_SLIDER: u32 = 1000;

/// How many times the speed grows from the bottom to the top of the slider, in tenfolds: 1 to
/// 1000 is three of them.
const TENFOLDS_ON_THE_SLIDER: f32 = 3.0;

/// The slider is never made shorter than this per tenfold, so that small factors stay easy to hit...
const SMALLEST_POINTS_PER_TENFOLD: f32 = 90.0;

/// ...nor taller than this, so that it stays a comfortable slider on a large screen.
const LARGEST_POINTS_PER_TENFOLD: f32 = 200.0;

/// How the height of the slider relates to the speed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpeedSliderScale {
    points_per_tenfold: f32,
}

impl SpeedSliderScale {
    /// A scale that makes the whole slider as tall as `available_height` points allows, within
    /// limits that keep it usable.
    pub fn fitting(available_height: f32) -> Self {
        let wanted = if available_height.is_nan() {
            SMALLEST_POINTS_PER_TENFOLD
        } else {
            available_height / TENFOLDS_ON_THE_SLIDER
        };
        Self {
            points_per_tenfold: wanted.clamp(SMALLEST_POINTS_PER_TENFOLD, LARGEST_POINTS_PER_TENFOLD),
        }
    }

    /// How tall the whole slider is.
    pub fn full_height(&self) -> f32 {
        self.height_of(SpeedFactor::new(LARGEST_SPEED_FACTOR_ON_THE_SLIDER).expect("the largest factor is not zero"))
    }

    /// How far above the bottom of the slider `factor` is.
    pub fn height_of(&self, factor: SpeedFactor) -> f32 {
        self.points_per_tenfold * (factor.value() as f32).log10()
    }

    /// The speed at `height` points above the bottom of the slider: the nearest whole factor,
    /// never below 1 or above the slider's largest.
    pub fn factor_at(&self, height: f32) -> SpeedFactor {
        if height.is_nan() || height <= 0.0 {
            return SpeedFactor::NORMAL;
        }
        let factor = 10f32.powf(height / self.points_per_tenfold).round();
        let factor = factor.clamp(1.0, LARGEST_SPEED_FACTOR_ON_THE_SLIDER as f32) as u32;
        SpeedFactor::new(factor).unwrap_or(SpeedFactor::NORMAL)
    }
}

/// One drag of the slider, from the moment the button goes down until it is let go.
///
/// The speed starts at the clip's current speed, as if the knob were already there, so a click
/// without any dragging changes nothing. Dragging up raises the speed and dragging down lowers it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpeedSliderDrag {
    scale: SpeedSliderScale,
    /// The speed the drag is measured from, and where the pointer was at that moment.
    base_factor: SpeedFactor,
    base_pointer_y: f32,
    current: SpeedFactor,
}

impl SpeedSliderDrag {
    pub fn begin(scale: SpeedSliderScale, initial_factor: SpeedFactor, pointer_y: f32) -> Self {
        Self {
            scale,
            base_factor: initial_factor,
            base_pointer_y: pointer_y,
            current: initial_factor,
        }
    }

    pub fn scale(&self) -> SpeedSliderScale {
        self.scale
    }

    /// The speed the drag has reached.
    pub fn current(&self) -> SpeedFactor {
        self.current
    }

    /// Follows the pointer to `pointer_y` (screen coordinates grow downwards, so moving up
    /// is a smaller number) and returns the speed it has reached.
    pub fn move_pointer(&mut self, pointer_y: f32) -> SpeedFactor {
        let height_moved_up = self.base_pointer_y - pointer_y;
        self.current = self
            .scale
            .factor_at(self.scale.height_of(self.base_factor) + height_moved_up);
        self.current
    }

    /// Changes the speed by `steps` whole factors (negative to lower it), then carries on
    /// measuring the drag from the pointer's current position.
    pub fn nudge(&mut self, steps: i32, pointer_y: f32) -> SpeedFactor {
        let nudged = i64::from(self.current.value()) + i64::from(steps);
        let nudged = nudged.clamp(1, i64::from(LARGEST_SPEED_FACTOR_ON_THE_SLIDER)) as u32;
        self.current = SpeedFactor::new(nudged).unwrap_or(SpeedFactor::NORMAL);
        self.base_factor = self.current;
        self.base_pointer_y = pointer_y;
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn factor(value: u32) -> SpeedFactor {
        SpeedFactor::new(value).unwrap()
    }

    fn scale() -> SpeedSliderScale {
        // 540 points tall in all: 180 points per tenfold.
        SpeedSliderScale::fitting(540.0)
    }

    #[test]
    fn each_tenfold_takes_the_same_height() {
        let scale = scale();
        assert_eq!(scale.height_of(factor(1)), 0.0);
        assert!((scale.height_of(factor(10)) - 180.0).abs() < 1e-3);
        assert!((scale.height_of(factor(100)) - 360.0).abs() < 1e-3);
        assert!((scale.height_of(factor(1000)) - 540.0).abs() < 1e-3);
        assert!((scale.full_height() - 540.0).abs() < 1e-3);
    }

    #[test]
    fn the_scale_fits_the_space_available_within_limits() {
        assert!((SpeedSliderScale::fitting(450.0).full_height() - 450.0).abs() < 1e-3);
        assert!(
            (SpeedSliderScale::fitting(100.0).full_height() - 270.0).abs() < 1e-3,
            "never shorter than 90 per tenfold"
        );
        assert!(
            (SpeedSliderScale::fitting(5000.0).full_height() - 600.0).abs() < 1e-3,
            "never taller than 200 per tenfold"
        );
        assert!((SpeedSliderScale::fitting(f32::NAN).full_height() - 270.0).abs() < 1e-3);
    }

    #[test]
    fn every_factor_comes_back_from_its_own_height() {
        for points in [90.0, 180.0, 200.0] {
            let scale = SpeedSliderScale {
                points_per_tenfold: points,
            };
            for value in 1..=LARGEST_SPEED_FACTOR_ON_THE_SLIDER {
                assert_eq!(
                    scale.factor_at(scale.height_of(factor(value))),
                    factor(value),
                    "factor {value} at {points} per tenfold"
                );
            }
        }
    }

    #[test]
    fn higher_up_the_slider_is_never_slower() {
        let scale = scale();
        let mut previous = 1;
        for step in 0..=1200 {
            let current = scale.factor_at(step as f32 * 0.5).value();
            assert!(
                current >= previous,
                "height {} gave {current} after {previous}",
                step as f32 * 0.5
            );
            previous = current;
        }
    }

    #[test]
    fn the_bottom_is_normal_speed_and_the_ends_are_limited() {
        let scale = scale();
        assert_eq!(scale.factor_at(0.0), SpeedFactor::NORMAL);
        assert_eq!(scale.factor_at(-300.0), SpeedFactor::NORMAL);
        assert_eq!(scale.factor_at(f32::NAN), SpeedFactor::NORMAL);
        assert_eq!(scale.factor_at(10_000.0), factor(1000));
        assert_eq!(scale.factor_at(f32::INFINITY), factor(1000));
    }

    #[test]
    fn small_factors_are_far_enough_apart_to_hit_with_a_mouse() {
        let scale = scale();
        for value in 1..=100 {
            let gap = scale.height_of(factor(value + 1)) - scale.height_of(factor(value));
            assert!(
                gap >= 0.75,
                "factors {value} and {} are only {gap} points apart",
                value + 1
            );
        }
        assert!(
            scale.height_of(factor(3)) - scale.height_of(factor(2)) > 30.0,
            "2x and 3x are well apart"
        );
    }

    #[test]
    fn a_drag_that_does_not_move_keeps_the_speed_the_clip_already_had() {
        for initial in [1, 2, 4, 37, 100, 999, 1000] {
            let mut drag = SpeedSliderDrag::begin(scale(), factor(initial), 400.0);
            assert_eq!(drag.current(), factor(initial));
            assert_eq!(drag.move_pointer(400.0), factor(initial), "pointer still at the start");
        }
    }

    #[test]
    fn dragging_up_raises_the_speed_and_dragging_down_lowers_it() {
        let mut drag = SpeedSliderDrag::begin(scale(), factor(1), 600.0);
        let up_a_tenfold = drag.move_pointer(600.0 - 180.0);
        assert_eq!(up_a_tenfold, factor(10));
        assert_eq!(drag.current(), factor(10));
        let up_two_tenfolds = drag.move_pointer(600.0 - 360.0);
        assert_eq!(up_two_tenfolds, factor(100));
        let back_down = drag.move_pointer(600.0 - 180.0);
        assert_eq!(back_down, factor(10));
        assert_eq!(
            drag.move_pointer(700.0),
            SpeedFactor::NORMAL,
            "below the start is still normal speed"
        );
    }

    #[test]
    fn dragging_starts_from_the_clips_own_speed() {
        // A clip at 10x, dragged up one more tenfold, becomes 100x.
        let mut drag = SpeedSliderDrag::begin(scale(), factor(10), 600.0);
        assert_eq!(drag.move_pointer(600.0 - 180.0), factor(100));
        assert_eq!(drag.move_pointer(600.0 + 180.0), SpeedFactor::NORMAL);
    }

    #[test]
    fn nudging_changes_the_speed_by_whole_factors_and_the_drag_carries_on_from_there() {
        let mut drag = SpeedSliderDrag::begin(scale(), factor(1), 600.0);
        drag.move_pointer(600.0 - 180.0); // 10x
        assert_eq!(drag.nudge(1, 420.0), factor(11));
        assert_eq!(drag.nudge(-2, 420.0), factor(9));
        // The pointer has not moved since the nudge: still 9x, not back to 10x.
        assert_eq!(drag.move_pointer(420.0), factor(9));
        // And moving from there is measured from 9x.
        assert_eq!(drag.move_pointer(420.0 - 180.0), factor(90));
    }

    #[test]
    fn nudging_stays_within_the_slider() {
        let mut drag = SpeedSliderDrag::begin(scale(), factor(1), 600.0);
        assert_eq!(drag.nudge(-5, 600.0), SpeedFactor::NORMAL);
        let mut drag = SpeedSliderDrag::begin(scale(), factor(999), 600.0);
        assert_eq!(drag.nudge(1, 600.0), factor(1000));
        assert_eq!(drag.nudge(5, 600.0), factor(1000));
    }
}
