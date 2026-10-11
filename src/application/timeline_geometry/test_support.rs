//! What the tests of the parts of the geometry have in common.

use super::TimelineGeometry;
use crate::project::{
    Clip, ClipId, FrameRate, MediaClip, SourceFrameIndex, SourceMediaIdentifier, SpeedFactor, Track, TrackId,
};

pub(super) fn geometry_at(numerator: u32, denominator: u32, points_per_second: f64) -> TimelineGeometry {
    TimelineGeometry {
        frame_rate: FrameRate::new(numerator, denominator).unwrap(),
        points_per_second,
    }
}

pub(super) fn media(first: u64, end: u64, speed: u32) -> MediaClip {
    MediaClip::new(
        SourceMediaIdentifier::new(1),
        SourceFrameIndex::new(first),
        SourceFrameIndex::new(end),
        SpeedFactor::new(speed).unwrap(),
    )
    .unwrap()
}

pub(super) fn clip(first: u64, end: u64, speed: u32) -> Clip {
    Clip::Video(media(first, end, speed))
}

/// A track whose clips touch one another, the first one starting at the start of the timeline.
pub(super) fn track_of(clips: Vec<Clip>) -> Track {
    Track::new(TrackId::new(0), "track 0").with_clips(clips.into_iter().map(|clip| (0, clip)))
}

pub(super) fn assert_close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} is not {expected}");
}

/// Timeline at 30 fps: clip 0 is 100 frames (3.333 s), clip 1 is 200 source frames at 4x = 50 frames (1.667 s).
pub(super) fn two_clip_track() -> Track {
    track_of(vec![clip(0, 100, 1), clip(100, 300, 4)])
}

pub(super) fn clip_id(number: u32) -> ClipId {
    ClipId::new(number)
}
