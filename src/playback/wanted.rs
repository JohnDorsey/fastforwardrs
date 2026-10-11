//! Working out which source picture the preview needs for a position on the timeline.

use crate::composition::{PicturePlan, PictureSection, VisibleVideo};
use crate::project::{SourceFrameIndex, SourceMediaIdentifier, SpeedFactor, TimelineFrameIndex};

/// The video that follows the one being shown, so that its pictures can be got ready before they
/// are needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpcomingVideo {
    pub source: SourceMediaIdentifier,
    pub first_source_frame: SourceFrameIndex,
    pub end_source_frame: SourceFrameIndex,
    pub speed_factor: SpeedFactor,
}

/// The picture the preview needs right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WantedPicture {
    pub source: SourceMediaIdentifier,
    /// The source frame to show.
    pub source_frame: SourceFrameIndex,
    pub speed_factor: SpeedFactor,
    /// The end of the stretch of video being shown: a stream of pictures started here need not go
    /// further. (It is the end of the run of sections that show one clip without a break, so that
    /// text that comes and goes over a video does not make the pictures start again.)
    pub range_end_source_frame: SourceFrameIndex,
    /// How many pictures that stretch still has to show from this one on, counting this one.
    pub pictures_left: u64,
    /// The video after this stretch, if one starts right after it.
    pub next_video: Option<UpcomingVideo>,
}

/// Which picture is needed to show `timeline_frame`, or `None` if the timeline has no such frame or
/// shows no video at it.
pub fn wanted_picture(plan: &PicturePlan, timeline_frame: TimelineFrameIndex) -> Option<WantedPicture> {
    let section = plan.section_at(timeline_frame)?;
    let video = section.video?;
    let section_index = plan
        .sections
        .partition_point(|candidate| candidate.end_timeline_frame <= timeline_frame.value());

    // The stretch goes on through the sections that show the next pictures of the same clip.
    let mut range_end_source_frame = video.end_source_frame;
    let mut end_timeline_frame = section.end_timeline_frame;
    let mut last_in_the_stretch = (section, video);
    let mut next_index = section_index + 1;
    while let Some(next) = plan.sections.get(next_index) {
        match next.video {
            Some(next_video) if continues(last_in_the_stretch.0, last_in_the_stretch.1, next, next_video) => {
                range_end_source_frame = next_video.end_source_frame;
                end_timeline_frame = next.end_timeline_frame;
                last_in_the_stretch = (next, next_video);
                next_index += 1;
            }
            _ => break,
        }
    }
    let next_video = plan
        .sections
        .get(next_index)
        .and_then(|next| next.video)
        .map(|next| UpcomingVideo {
            source: next.source,
            first_source_frame: SourceFrameIndex::new(next.first_source_frame),
            end_source_frame: SourceFrameIndex::new(next.end_source_frame),
            speed_factor: next.speed,
        });
    Some(WantedPicture {
        source: video.source,
        source_frame: SourceFrameIndex::new(section.source_frame_at(timeline_frame)?),
        speed_factor: video.speed,
        range_end_source_frame: SourceFrameIndex::new(range_end_source_frame),
        pictures_left: end_timeline_frame - timeline_frame.value(),
        next_video,
    })
}

/// Whether `next` shows the pictures that come right after those of `section`, of the same clip.
fn continues(section: &PictureSection, video: VisibleVideo, next: &PictureSection, next_video: VisibleVideo) -> bool {
    next_video.clip == video.clip
        && next_video.speed == video.speed
        && next.first_timeline_frame == section.end_timeline_frame
        && next_video.first_source_frame
            == video.first_source_frame + section.timeline_frame_count() * u64::from(video.speed.value())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::composition::plan_pictures;
    use crate::project::{
        Clip, FrameRate, MediaClip, Project, SourceMedia, SourceMediaLink, SourceVideoProperties, SpeedIndicatorLayout,
        TextClip, TextObject, Track, TrackId,
    };

    const VIDEO: SourceMediaIdentifier = SourceMediaIdentifier::new(1);

    fn frame(number: u64) -> TimelineFrameIndex {
        TimelineFrameIndex::new(number)
    }

    fn video_clip(first: u64, end: u64, speed: u32) -> Clip {
        Clip::Video(
            MediaClip::new(
                VIDEO,
                SourceFrameIndex::new(first),
                SourceFrameIndex::new(end),
                SpeedFactor::new(speed).unwrap(),
            )
            .unwrap(),
        )
    }

    fn text(duration: u64) -> Clip {
        Clip::Text(TextClip::new(TextObject::default(), duration).unwrap())
    }

    /// A project whose tracks (the front one first) have these clips, each with the empty frames before it.
    fn plan_of(tracks: Vec<Vec<(u64, Clip)>>) -> PicturePlan {
        let source = SourceMedia::video(
            VIDEO,
            SourceMediaLink::from_absolute_path(PathBuf::from("/videos/holiday.mp4")),
            SourceVideoProperties {
                frame_rate: FrameRate::from_whole_frames_per_second(30).unwrap(),
                total_frame_count: 1000,
                display_width_in_pixels: 640,
                display_height_in_pixels: 360,
                has_audio: false,
            },
        );
        let tracks = tracks
            .into_iter()
            .enumerate()
            .map(|(number, clips)| Track::new(TrackId::new(number as u32), "track").with_clips(clips))
            .collect();
        let project = Project::from_parts(vec![source], tracks, SpeedIndicatorLayout::DEFAULT).unwrap();
        plan_pictures(&project, |_| true)
    }

    fn wanted(plan: &PicturePlan, timeline_frame: u64) -> Option<WantedPicture> {
        wanted_picture(plan, frame(timeline_frame))
    }

    /// Timeline: clip 0 = frames 0..100 (1x), clip 1 = 100..150 (200 source frames at 4x).
    fn two_clips() -> PicturePlan {
        plan_of(vec![vec![(0, video_clip(0, 100, 1)), (0, video_clip(100, 300, 4))]])
    }

    #[test]
    fn a_frame_in_a_normal_clip_wants_its_own_source_frame() {
        let wanted = wanted(&two_clips(), 40).unwrap();
        assert_eq!(wanted.source_frame, SourceFrameIndex::new(40));
        assert_eq!(wanted.speed_factor, SpeedFactor::NORMAL);
        assert_eq!(wanted.range_end_source_frame, SourceFrameIndex::new(100));
        assert_eq!(wanted.pictures_left, 60);
        assert_eq!(
            wanted.next_video.unwrap().first_source_frame,
            SourceFrameIndex::new(100)
        );
    }

    #[test]
    fn a_frame_in_a_sped_up_clip_wants_every_nth_source_frame() {
        let wanted = wanted(&two_clips(), 110).unwrap();
        assert_eq!(wanted.source_frame, SourceFrameIndex::new(140));
        assert_eq!(wanted.speed_factor, SpeedFactor::new(4).unwrap());
        assert_eq!(wanted.pictures_left, 40);
        assert_eq!(wanted.next_video, None, "the last clip has nothing after it");
    }

    #[test]
    fn the_first_and_last_pictures_of_a_clip_are_counted_correctly() {
        let plan = two_clips();
        assert_eq!(wanted(&plan, 0).unwrap().pictures_left, 100);
        assert_eq!(wanted(&plan, 99).unwrap().pictures_left, 1);
        assert_eq!(wanted(&plan, 100).unwrap().pictures_left, 50);
        assert_eq!(wanted(&plan, 149).unwrap().pictures_left, 1);
    }

    #[test]
    fn the_upcoming_video_is_described_for_getting_ready_in_advance() {
        let upcoming = wanted(&two_clips(), 95).unwrap().next_video.unwrap();
        assert_eq!(
            upcoming,
            UpcomingVideo {
                source: VIDEO,
                first_source_frame: SourceFrameIndex::new(100),
                end_source_frame: SourceFrameIndex::new(300),
                speed_factor: SpeedFactor::new(4).unwrap(),
            }
        );
    }

    #[test]
    fn a_frame_past_the_end_of_the_timeline_or_where_there_is_no_video_wants_nothing() {
        let plan = two_clips();
        assert_eq!(wanted(&plan, 150), None);
        assert_eq!(wanted_picture(&plan_of(vec![vec![]]), frame(0)), None);
        // A gap between two clips shows black.
        let with_a_gap = plan_of(vec![vec![(0, video_clip(0, 50, 1)), (30, video_clip(50, 100, 1))]]);
        assert_eq!(wanted(&with_a_gap, 60), None);
        assert!(
            wanted(&with_a_gap, 49).unwrap().next_video.is_none(),
            "the next video does not start right after"
        );
        assert!(wanted(&with_a_gap, 80).is_some());
    }

    #[test]
    fn text_that_comes_and_goes_over_a_video_does_not_cut_its_stretch_of_pictures_short() {
        // One clip of 100 frames with a text over frames 30..60: three sections, one stretch.
        let plan = plan_of(vec![vec![(30, text(30))], vec![(0, video_clip(0, 100, 1))]]);
        assert_eq!(plan.sections.len(), 3);
        let before_the_text = wanted(&plan, 10).unwrap();
        assert_eq!(before_the_text.pictures_left, 90);
        assert_eq!(before_the_text.range_end_source_frame, SourceFrameIndex::new(100));
        assert_eq!(before_the_text.next_video, None);
        let under_the_text = wanted(&plan, 40).unwrap();
        assert_eq!(under_the_text.source_frame, SourceFrameIndex::new(40));
        assert_eq!(under_the_text.pictures_left, 60);
        assert_eq!(wanted(&plan, 70).unwrap().pictures_left, 30);
    }

    #[test]
    fn a_video_from_another_clip_even_of_the_same_file_is_the_next_video_and_not_part_of_the_stretch() {
        let plan = plan_of(vec![vec![(0, video_clip(0, 50, 1)), (0, video_clip(50, 100, 1))]]);
        let wanted = wanted(&plan, 10).unwrap();
        assert_eq!(wanted.pictures_left, 40);
        assert_eq!(wanted.next_video.unwrap().first_source_frame, SourceFrameIndex::new(50));
    }
}
