//! How tall a track is on the screen.
//!
//! This is not something a video is made of, but it is something a person arranges to suit
//! themselves (a track with pictures to look at is taller than one that only has a sound on it), so
//! it is kept with the track and saved with the project.

/// The height of a track's row in the timeline, in points. It is never less than
/// [`TrackHeight::SMALLEST_IN_POINTS`] nor more than [`TrackHeight::LARGEST_IN_POINTS`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackHeight {
    points: f32,
}

impl TrackHeight {
    /// The height of a new track.
    pub const DEFAULT_IN_POINTS: f32 = 104.0;

    /// The least a track can be: just taller than what the box at its left needs to hold the grip dots on its top and
    /// the title of the track. (A test of the application checks that it does fit.)
    pub const SMALLEST_IN_POINTS: f32 = 44.0;

    /// The most a track can be: half as tall again as a new track.
    pub const LARGEST_IN_POINTS: f32 = 156.0;

    pub const DEFAULT: TrackHeight = TrackHeight {
        points: Self::DEFAULT_IN_POINTS,
    };

    /// A height of `points` points, or as near to it as is allowed. (Something that is not a
    /// number is the height of a new track.)
    pub fn from_points(points: f32) -> Self {
        if points.is_nan() {
            return Self::DEFAULT;
        }
        Self {
            points: points.clamp(Self::SMALLEST_IN_POINTS, Self::LARGEST_IN_POINTS),
        }
    }

    pub fn points(self) -> f32 {
        self.points
    }

    pub fn is_default(self) -> bool {
        self == Self::DEFAULT
    }
}

impl Default for TrackHeight {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_track_is_a_hundred_and_four_points_tall_and_the_most_it_can_be_is_half_as_much_again() {
        assert_eq!(TrackHeight::default().points(), 104.0);
        assert!(TrackHeight::default().is_default());
        assert_eq!(TrackHeight::LARGEST_IN_POINTS, 1.5 * TrackHeight::DEFAULT_IN_POINTS);
    }

    #[test]
    fn a_height_between_the_limits_is_kept_exactly() {
        for points in [44.0, 45.5, 60.0, 104.0, 155.9, 156.0] {
            assert_eq!(TrackHeight::from_points(points).points(), points);
        }
    }

    #[test]
    fn a_height_outside_the_limits_is_brought_to_the_nearest_limit() {
        assert_eq!(TrackHeight::from_points(0.0).points(), TrackHeight::SMALLEST_IN_POINTS);
        assert_eq!(
            TrackHeight::from_points(-500.0).points(),
            TrackHeight::SMALLEST_IN_POINTS
        );
        assert_eq!(TrackHeight::from_points(157.0).points(), TrackHeight::LARGEST_IN_POINTS);
        assert_eq!(
            TrackHeight::from_points(f32::INFINITY).points(),
            TrackHeight::LARGEST_IN_POINTS
        );
        assert_eq!(
            TrackHeight::from_points(f32::NEG_INFINITY).points(),
            TrackHeight::SMALLEST_IN_POINTS
        );
    }

    #[test]
    fn something_that_is_not_a_number_is_the_height_of_a_new_track() {
        assert_eq!(TrackHeight::from_points(f32::NAN), TrackHeight::DEFAULT);
    }
}
