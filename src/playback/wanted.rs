//! Working out which source picture the preview needs for a position on the timeline.

use crate::project::{SourceFrameIndex, SourceVideoIdentifier, SpeedFactor, TimelineFrameIndex, VideoTrack};

/// The clip that follows the one being shown, so that its pictures can be got ready before they
/// are needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpcomingClip {
    pub source_video_identifier: SourceVideoIdentifier,
    pub first_source_frame: SourceFrameIndex,
    pub end_source_frame: SourceFrameIndex,
    pub speed_factor: SpeedFactor,
}

/// The picture the preview needs right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WantedPicture {
    pub source_video_identifier: SourceVideoIdentifier,
    /// The source frame to show.
    pub source_frame: SourceFrameIndex,
    pub speed_factor: SpeedFactor,
    /// The end of the clip being shown: a stream of pictures started here need not go further.
    pub clip_end_source_frame: SourceFrameIndex,
    /// How many pictures the clip still has to show from this one on, counting this one.
    pub pictures_left_in_clip: u64,
    /// The clip after this one, if there is one.
    pub next_clip: Option<UpcomingClip>,
}

/// Which picture is needed to show `timeline_frame`, or `None` if the timeline has no such frame.
pub fn wanted_picture(video_track: &VideoTrack, timeline_frame: TimelineFrameIndex) -> Option<WantedPicture> {
    let location = video_track.locate(timeline_frame)?;
    let clip = video_track.clips()[location.clip_index];
    let next_clip = video_track
        .clips()
        .get(location.clip_index + 1)
        .map(|next| UpcomingClip {
            source_video_identifier: next.source_video_identifier(),
            first_source_frame: next.first_source_frame(),
            end_source_frame: next.end_source_frame(),
            speed_factor: next.speed_factor(),
        });
    Some(WantedPicture {
        source_video_identifier: clip.source_video_identifier(),
        source_frame: location.source_frame,
        speed_factor: clip.speed_factor(),
        clip_end_source_frame: clip.end_source_frame(),
        pictures_left_in_clip: clip.timeline_frame_count() - location.offset_within_clip,
        next_clip,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Clip;

    const VIDEO: SourceVideoIdentifier = SourceVideoIdentifier::new(1);

    fn clip(first: u64, end: u64, speed: u32) -> Clip {
        Clip::new(
            VIDEO,
            SourceFrameIndex::new(first),
            SourceFrameIndex::new(end),
            SpeedFactor::new(speed).unwrap(),
        )
        .unwrap()
    }

    fn track() -> VideoTrack {
        // Timeline: clip 0 = frames 0..100 (1x), clip 1 = 100..150 (200 source frames at 4x).
        VideoTrack::new(vec![clip(0, 100, 1), clip(100, 300, 4)])
    }

    fn wanted(timeline_frame: u64) -> Option<WantedPicture> {
        wanted_picture(&track(), TimelineFrameIndex::new(timeline_frame))
    }

    #[test]
    fn a_frame_in_a_normal_clip_wants_its_own_source_frame() {
        let wanted = wanted(40).unwrap();
        assert_eq!(wanted.source_frame, SourceFrameIndex::new(40));
        assert_eq!(wanted.speed_factor, SpeedFactor::NORMAL);
        assert_eq!(wanted.clip_end_source_frame, SourceFrameIndex::new(100));
        assert_eq!(wanted.pictures_left_in_clip, 60);
        assert_eq!(wanted.next_clip.unwrap().first_source_frame, SourceFrameIndex::new(100));
    }

    #[test]
    fn a_frame_in_a_sped_up_clip_wants_every_nth_source_frame() {
        let wanted = wanted(110).unwrap();
        assert_eq!(wanted.source_frame, SourceFrameIndex::new(140));
        assert_eq!(wanted.speed_factor, SpeedFactor::new(4).unwrap());
        assert_eq!(wanted.pictures_left_in_clip, 40);
        assert_eq!(wanted.next_clip, None, "the last clip has no clip after it");
    }

    #[test]
    fn the_first_and_last_pictures_of_a_clip_are_counted_correctly() {
        assert_eq!(wanted(0).unwrap().pictures_left_in_clip, 100);
        assert_eq!(wanted(99).unwrap().pictures_left_in_clip, 1);
        assert_eq!(wanted(100).unwrap().pictures_left_in_clip, 50);
        assert_eq!(wanted(149).unwrap().pictures_left_in_clip, 1);
    }

    #[test]
    fn the_upcoming_clip_is_described_for_getting_ready_in_advance() {
        let upcoming = wanted(95).unwrap().next_clip.unwrap();
        assert_eq!(
            upcoming,
            UpcomingClip {
                source_video_identifier: VIDEO,
                first_source_frame: SourceFrameIndex::new(100),
                end_source_frame: SourceFrameIndex::new(300),
                speed_factor: SpeedFactor::new(4).unwrap(),
            }
        );
    }

    #[test]
    fn a_frame_past_the_end_of_the_timeline_wants_nothing() {
        assert_eq!(wanted(150), None);
        assert_eq!(wanted_picture(&VideoTrack::default(), TimelineFrameIndex::new(0)), None);
    }
}
