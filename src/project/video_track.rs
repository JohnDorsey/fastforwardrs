//! The video track: the ordered clips that make up the timeline.

use thiserror::Error;

use super::{Clip, SourceFrameIndex, SourceVideoIdentifier, SpeedFactor, TimelineFrameIndex};

/// The ordered clips that make up the edited video. Every operation that needs to know where a
/// clip sits on the timeline goes through this type, so the arithmetic lives in one place.
///
/// The track keeps a table of where each clip starts on the timeline. It is rebuilt after every
/// edit, which keeps looking up "which clip is at this frame" fast no matter how many clips there
/// are.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VideoTrack {
    clips: Vec<Clip>,
    /// `clip_first_timeline_frames[i]` is the timeline frame at which `clips[i]` starts.
    /// It is strictly increasing, because every clip has at least one frame.
    clip_first_timeline_frames: Vec<u64>,
    total_frame_count: u64,
}

/// Where a clip sits on the timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipPlacement<'track> {
    pub clip_index: usize,
    pub clip: &'track Clip,
    pub first_timeline_frame: TimelineFrameIndex,
    /// The frame just after the clip's last frame (the end is exclusive).
    pub end_timeline_frame: TimelineFrameIndex,
}

/// What is at one position of the timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimelineLocation {
    pub clip_index: usize,
    /// How many frames into the clip this position is (0 is the clip's first frame).
    pub offset_within_clip: u64,
    /// The frame of the source video that is shown at this position.
    pub source_frame: SourceFrameIndex,
}

/// The result of cutting a clip in two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitOutcome {
    pub first_half_clip_index: usize,
    pub second_half_clip_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum SplitError {
    #[error("{timeline_frame} is outside the timeline, which has {total_frame_count} frames")]
    OutsideTimeline {
        timeline_frame: TimelineFrameIndex,
        total_frame_count: u64,
    },
    #[error("the timeline is already split at {timeline_frame}")]
    AlreadyAtClipBoundary { timeline_frame: TimelineFrameIndex },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("there is no clip number {clip_index}; the track has {clip_count} clips")]
pub struct UnknownClip {
    pub clip_index: usize,
    pub clip_count: usize,
}

impl VideoTrack {
    /// A track made of exactly `clips`, in that order.
    pub fn new(clips: Vec<Clip>) -> Self {
        let mut track = Self {
            clips,
            clip_first_timeline_frames: Vec::new(),
            total_frame_count: 0,
        };
        track.rebuild_timeline_positions();
        track
    }

    /// A track holding one clip that covers all `source_frame_count` frames of a video, at normal
    /// speed. The track is empty if the video has no frames.
    pub fn with_single_clip_covering(source_video_identifier: SourceVideoIdentifier, source_frame_count: u64) -> Self {
        let whole_video = Clip::new(
            source_video_identifier,
            SourceFrameIndex::new(0),
            SourceFrameIndex::new(source_frame_count),
            SpeedFactor::NORMAL,
        );
        match whole_video {
            Ok(clip) => Self::new(vec![clip]),
            // Only possible when the video has no frames at all.
            Err(_) => Self::new(Vec::new()),
        }
    }

    pub fn clips(&self) -> &[Clip] {
        &self.clips
    }

    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }

    /// How many frames the finished video has.
    pub fn total_frame_count(&self) -> u64 {
        self.total_frame_count
    }

    pub fn placement_of_clip(&self, clip_index: usize) -> Option<ClipPlacement<'_>> {
        let clip = self.clips.get(clip_index)?;
        let first_timeline_frame = self.clip_first_timeline_frames[clip_index];
        Some(ClipPlacement {
            clip_index,
            clip,
            first_timeline_frame: TimelineFrameIndex::new(first_timeline_frame),
            end_timeline_frame: TimelineFrameIndex::new(first_timeline_frame + clip.timeline_frame_count()),
        })
    }

    /// Every clip with its place on the timeline, in order.
    pub fn placements(&self) -> impl Iterator<Item = ClipPlacement<'_>> {
        (0..self.clips.len()).filter_map(|clip_index| self.placement_of_clip(clip_index))
    }

    /// What is shown at `timeline_frame`, or `None` if that is past the end of the timeline.
    pub fn locate(&self, timeline_frame: TimelineFrameIndex) -> Option<TimelineLocation> {
        if timeline_frame.value() >= self.total_frame_count {
            return None;
        }
        let clips_starting_at_or_before = self
            .clip_first_timeline_frames
            .partition_point(|&first_frame| first_frame <= timeline_frame.value());
        let clip_index = clips_starting_at_or_before - 1;
        let offset_within_clip = timeline_frame.value() - self.clip_first_timeline_frames[clip_index];
        let source_frame = self.clips[clip_index].source_frame_at_timeline_offset(offset_within_clip)?;
        Some(TimelineLocation {
            clip_index,
            offset_within_clip,
            source_frame,
        })
    }

    /// The timeline frame that shows `source_frame` of the given video, or `None` if no clip
    /// contains it. In a sped-up clip several source frames share one timeline frame, and a
    /// source frame that appears in several clips is reported at its first clip.
    pub fn timeline_frame_showing_source_frame(
        &self,
        source_video_identifier: SourceVideoIdentifier,
        source_frame: SourceFrameIndex,
    ) -> Option<TimelineFrameIndex> {
        self.clips.iter().enumerate().find_map(|(clip_index, clip)| {
            if clip.source_video_identifier() != source_video_identifier {
                return None;
            }
            let offset_within_clip = clip.timeline_offset_of_source_frame(source_frame)?;
            Some(TimelineFrameIndex::new(
                self.clip_first_timeline_frames[clip_index] + offset_within_clip,
            ))
        })
    }

    /// Cuts the clip at `timeline_frame` in two, so that the clip's frame at that position becomes
    /// the first frame of the second half. Both halves keep the clip's speed, and the timeline's
    /// total length does not change.
    pub fn split_at(&mut self, timeline_frame: TimelineFrameIndex) -> Result<SplitOutcome, SplitError> {
        let location = self.locate(timeline_frame).ok_or(SplitError::OutsideTimeline {
            timeline_frame,
            total_frame_count: self.total_frame_count,
        })?;
        let (first_half, second_half) = self.clips[location.clip_index]
            .split_before_source_frame(location.source_frame)
            .ok_or(SplitError::AlreadyAtClipBoundary { timeline_frame })?;

        self.clips[location.clip_index] = first_half;
        self.clips.insert(location.clip_index + 1, second_half);
        self.rebuild_timeline_positions();
        Ok(SplitOutcome {
            first_half_clip_index: location.clip_index,
            second_half_clip_index: location.clip_index + 1,
        })
    }

    /// Plays the clip at `clip_index` at a different speed. Everything after it moves along the
    /// timeline to close up (or make room for) the change in the clip's length.
    pub fn set_speed_factor(&mut self, clip_index: usize, speed_factor: SpeedFactor) -> Result<(), UnknownClip> {
        let clip_count = self.clips.len();
        let clip = self
            .clips
            .get_mut(clip_index)
            .ok_or(UnknownClip { clip_index, clip_count })?;
        *clip = clip.with_speed_factor(speed_factor);
        self.rebuild_timeline_positions();
        Ok(())
    }

    fn rebuild_timeline_positions(&mut self) {
        self.clip_first_timeline_frames.clear();
        let mut next_first_frame = 0;
        for clip in &self.clips {
            self.clip_first_timeline_frames.push(next_first_frame);
            next_first_frame += clip.timeline_frame_count();
        }
        self.total_frame_count = next_first_frame;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::DeterministicRandomNumbers;

    const VIDEO: SourceVideoIdentifier = SourceVideoIdentifier::new(1);

    fn timeline_frame(frame_number: u64) -> TimelineFrameIndex {
        TimelineFrameIndex::new(frame_number)
    }

    fn speed(factor: u32) -> SpeedFactor {
        SpeedFactor::new(factor).unwrap()
    }

    fn clip(first: u64, end: u64, factor: u32) -> Clip {
        Clip::new(
            VIDEO,
            SourceFrameIndex::new(first),
            SourceFrameIndex::new(end),
            speed(factor),
        )
        .unwrap()
    }

    fn described(track: &VideoTrack) -> Vec<(u64, u64, u32)> {
        track
            .clips()
            .iter()
            .map(|clip| {
                (
                    clip.first_source_frame().value(),
                    clip.end_source_frame().value(),
                    clip.speed_factor().value(),
                )
            })
            .collect()
    }

    #[test]
    fn a_new_track_is_one_normal_speed_clip_over_the_whole_video() {
        let track = VideoTrack::with_single_clip_covering(VIDEO, 300);
        assert_eq!(described(&track), vec![(0, 300, 1)]);
        assert_eq!(track.total_frame_count(), 300);
    }

    #[test]
    fn a_video_without_frames_gives_an_empty_track() {
        let track = VideoTrack::with_single_clip_covering(VIDEO, 0);
        assert!(track.is_empty());
        assert_eq!(track.total_frame_count(), 0);
        assert_eq!(track.locate(timeline_frame(0)), None);
    }

    #[test]
    fn locating_a_frame_finds_its_clip_and_source_frame() {
        let track = VideoTrack::new(vec![clip(0, 100, 1), clip(100, 300, 4), clip(300, 400, 1)]);
        // Timeline: clip 0 = frames 0..100, clip 1 = 100..150 (200 frames at 4x), clip 2 = 150..250.
        assert_eq!(track.total_frame_count(), 250);
        let at = |frame| track.locate(timeline_frame(frame)).unwrap();
        assert_eq!((at(0).clip_index, at(0).source_frame.value()), (0, 0));
        assert_eq!((at(99).clip_index, at(99).source_frame.value()), (0, 99));
        assert_eq!((at(100).clip_index, at(100).source_frame.value()), (1, 100));
        assert_eq!(
            (
                at(110).clip_index,
                at(110).offset_within_clip,
                at(110).source_frame.value()
            ),
            (1, 10, 140)
        );
        assert_eq!((at(149).clip_index, at(149).source_frame.value()), (1, 296));
        assert_eq!((at(150).clip_index, at(150).source_frame.value()), (2, 300));
        assert_eq!((at(249).clip_index, at(249).source_frame.value()), (2, 399));
        assert_eq!(track.locate(timeline_frame(250)), None);
    }

    #[test]
    fn placements_cover_the_timeline_without_gaps() {
        let track = VideoTrack::new(vec![clip(0, 100, 1), clip(100, 300, 4), clip(300, 400, 1)]);
        let placements: Vec<(usize, u64, u64)> = track
            .placements()
            .map(|placement| {
                (
                    placement.clip_index,
                    placement.first_timeline_frame.value(),
                    placement.end_timeline_frame.value(),
                )
            })
            .collect();
        assert_eq!(placements, vec![(0, 0, 100), (1, 100, 150), (2, 150, 250)]);
    }

    #[test]
    fn splitting_cuts_a_clip_in_two_and_keeps_the_total_length() {
        let mut track = VideoTrack::with_single_clip_covering(VIDEO, 300);
        let outcome = track.split_at(timeline_frame(100)).unwrap();
        assert_eq!(
            outcome,
            SplitOutcome {
                first_half_clip_index: 0,
                second_half_clip_index: 1
            }
        );
        assert_eq!(described(&track), vec![(0, 100, 1), (100, 300, 1)]);
        assert_eq!(track.total_frame_count(), 300);
    }

    #[test]
    fn splitting_at_an_existing_boundary_or_outside_the_timeline_is_refused() {
        let mut track = VideoTrack::with_single_clip_covering(VIDEO, 300);
        track.split_at(timeline_frame(100)).unwrap();
        let before = track.clone();
        assert_eq!(
            track.split_at(timeline_frame(0)),
            Err(SplitError::AlreadyAtClipBoundary {
                timeline_frame: timeline_frame(0)
            })
        );
        assert_eq!(
            track.split_at(timeline_frame(100)),
            Err(SplitError::AlreadyAtClipBoundary {
                timeline_frame: timeline_frame(100)
            })
        );
        assert_eq!(
            track.split_at(timeline_frame(300)),
            Err(SplitError::OutsideTimeline {
                timeline_frame: timeline_frame(300),
                total_frame_count: 300
            })
        );
        assert_eq!(track, before, "refused splits must not change the track");
    }

    #[test]
    fn splitting_inside_a_sped_up_clip_lands_on_one_of_its_shown_frames() {
        let mut track = VideoTrack::new(vec![clip(0, 100, 1), clip(100, 300, 4)]);
        // Timeline frame 107 is clip 1's frame 7, which shows source frame 100 + 7 * 4 = 128.
        track.split_at(timeline_frame(107)).unwrap();
        assert_eq!(described(&track), vec![(0, 100, 1), (100, 128, 4), (128, 300, 4)]);
        // 28 source frames at 4x = 7 timeline frames, and 172 at 4x = 43: total unchanged (50).
        assert_eq!(track.total_frame_count(), 100 + 50);
    }

    #[test]
    fn splitting_a_sped_up_clip_never_changes_the_total_length_even_when_the_speed_does_not_divide_it() {
        for factor in [2u32, 3, 4, 5, 7, 16] {
            let reference = VideoTrack::new(vec![clip(10, 10 + 101, factor)]);
            for split_frame in 1..reference.total_frame_count() {
                let mut track = reference.clone();
                track.split_at(timeline_frame(split_frame)).unwrap();
                assert_eq!(
                    track.total_frame_count(),
                    reference.total_frame_count(),
                    "splitting 101 frames at {factor}x before timeline frame {split_frame}"
                );
            }
        }
    }

    #[test]
    fn changing_a_speed_moves_everything_after_it() {
        let mut track = VideoTrack::with_single_clip_covering(VIDEO, 300);
        track.split_at(timeline_frame(100)).unwrap();
        track.split_at(timeline_frame(200)).unwrap();
        assert_eq!(track.total_frame_count(), 300);

        track.set_speed_factor(1, speed(10)).unwrap();
        assert_eq!(described(&track), vec![(0, 100, 1), (100, 200, 10), (200, 300, 1)]);
        assert_eq!(track.total_frame_count(), 100 + 10 + 100);
        let last_clip = track.placement_of_clip(2).unwrap();
        assert_eq!(last_clip.first_timeline_frame.value(), 110);
        assert_eq!(last_clip.end_timeline_frame.value(), 210);
    }

    #[test]
    fn changing_the_speed_of_a_clip_that_does_not_exist_is_an_error() {
        let mut track = VideoTrack::with_single_clip_covering(VIDEO, 300);
        assert_eq!(
            track.set_speed_factor(1, speed(2)),
            Err(UnknownClip {
                clip_index: 1,
                clip_count: 1
            })
        );
        assert_eq!(described(&track), vec![(0, 300, 1)]);
    }

    #[test]
    fn a_source_frame_is_found_at_the_timeline_frame_that_shows_it() {
        let track = VideoTrack::new(vec![clip(0, 100, 1), clip(100, 300, 4), clip(300, 400, 1)]);
        let find = |frame| {
            track
                .timeline_frame_showing_source_frame(VIDEO, SourceFrameIndex::new(frame))
                .map(TimelineFrameIndex::value)
        };
        assert_eq!(find(0), Some(0));
        assert_eq!(find(99), Some(99));
        assert_eq!(find(100), Some(100));
        assert_eq!(
            find(103),
            Some(100),
            "source frames 100..=103 all belong to timeline frame 100"
        );
        assert_eq!(find(104), Some(101));
        assert_eq!(find(299), Some(149));
        assert_eq!(find(300), Some(150));
        assert_eq!(find(399), Some(249));
        assert_eq!(find(400), None);
        assert_eq!(
            track.timeline_frame_showing_source_frame(SourceVideoIdentifier::new(2), SourceFrameIndex::new(0)),
            None
        );
    }

    /// Checks everything that must always be true of a track made by splitting a video of
    /// `source_frame_count` frames and changing clip speeds.
    fn assert_track_is_consistent(track: &VideoTrack, source_frame_count: u64) {
        // The clips tile the source video exactly once, in order.
        let mut expected_next_source_frame = 0;
        let mut expected_total_frames = 0;
        for clip in track.clips() {
            assert_eq!(clip.first_source_frame().value(), expected_next_source_frame);
            expected_next_source_frame = clip.end_source_frame().value();
            expected_total_frames += clip
                .source_frame_count()
                .div_ceil(u64::from(clip.speed_factor().value()));
        }
        assert_eq!(expected_next_source_frame, source_frame_count);
        assert_eq!(track.total_frame_count(), expected_total_frames);

        // The placements tile the timeline exactly once, in order.
        let mut expected_next_timeline_frame = 0;
        for placement in track.placements() {
            assert_eq!(placement.first_timeline_frame.value(), expected_next_timeline_frame);
            assert!(placement.end_timeline_frame > placement.first_timeline_frame);
            expected_next_timeline_frame = placement.end_timeline_frame.value();
        }
        assert_eq!(expected_next_timeline_frame, track.total_frame_count());

        // Every timeline frame shows a source frame that maps straight back to it.
        for frame_number in 0..track.total_frame_count() {
            let location = track.locate(timeline_frame(frame_number)).unwrap();
            assert_eq!(
                track.timeline_frame_showing_source_frame(VIDEO, location.source_frame),
                Some(timeline_frame(frame_number)),
                "timeline frame {frame_number} shows {}",
                location.source_frame
            );
        }
        assert_eq!(track.locate(timeline_frame(track.total_frame_count())), None);
    }

    #[test]
    fn random_sequences_of_splits_and_speed_changes_keep_the_track_consistent() {
        const SOURCE_FRAME_COUNT: u64 = 700;
        for seed in 1..=20u64 {
            let mut random = DeterministicRandomNumbers::with_seed(seed);
            let mut track = VideoTrack::with_single_clip_covering(VIDEO, SOURCE_FRAME_COUNT);
            for _ in 0..60 {
                if random.next_below(3) == 0 && track.clips().len() > 1 {
                    let clip_index = random.next_below(track.clips().len() as u64) as usize;
                    let factor = [1u32, 2, 3, 4, 5, 8, 16, 100][random.next_below(8) as usize];
                    track.set_speed_factor(clip_index, speed(factor)).unwrap();
                } else {
                    let length_before = track.total_frame_count();
                    let frame = random.next_below(track.total_frame_count());
                    if track.split_at(timeline_frame(frame)).is_ok() {
                        assert_eq!(
                            track.total_frame_count(),
                            length_before,
                            "a split must not change the length"
                        );
                    }
                }
                assert_track_is_consistent(&track, SOURCE_FRAME_COUNT);
            }
        }
    }
}
