// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Working out which source pictures the preview needs for a position on the timeline.
//!
//! Every video that is seen at a frame needs a picture, and each track's video has a stream of
//! pictures of its own (see [`super::PreviewPictures`]), so what is wanted is one picture for each
//! track that has a video that is seen.

use crate::composition::{Layer, PicturePlan, PictureSection, VisibleVideo};
use crate::project::{ClipId, SourceFrameIndex, SourceMediaIdentifier, SpeedFactor, TimelineFrameIndex, TrackId};

/// The video that follows the one being shown on a track, so that its pictures can be got ready
/// before they are needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpcomingVideo {
    pub source: SourceMediaIdentifier,
    pub first_source_frame: SourceFrameIndex,
    pub end_source_frame: SourceFrameIndex,
    pub speed_factor: SpeedFactor,
}

/// The picture the preview needs of the video of one track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WantedPicture {
    /// The track the video is on.
    pub track: TrackId,
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
    /// The video on the same track after this stretch, if one starts right after it.
    pub next_video: Option<UpcomingVideo>,
}

/// Which pictures are needed to show `timeline_frame`: one for each video that is seen at that
/// frame (none if the timeline has no such frame or shows no video at it).
///
/// With `look_ahead_frames`, a video on a track that shows none now but will within that many
/// frames is wanted too, as its first picture, so that its pictures are ready when it comes.
pub fn wanted_pictures(
    plan: &PicturePlan,
    timeline_frame: TimelineFrameIndex,
    look_ahead_frames: u64,
) -> Vec<WantedPicture> {
    let Some(section_index) = plan.section_index_at(timeline_frame) else {
        return Vec::new();
    };
    let mut wanted: Vec<WantedPicture> = plan.sections[section_index]
        .video_layers()
        .filter_map(|(layer, video)| wanted_picture_of(plan, section_index, timeline_frame.value(), layer, video))
        .collect();

    let horizon = timeline_frame.value().saturating_add(look_ahead_frames);
    for (index, section) in plan.sections.iter().enumerate().skip(section_index + 1) {
        if section.first_timeline_frame >= horizon {
            break;
        }
        for (layer, video) in section.video_layers() {
            if wanted.iter().any(|already| already.track == layer.track) {
                continue;
            }
            wanted.extend(wanted_picture_of(
                plan,
                index,
                section.first_timeline_frame,
                layer,
                video,
            ));
        }
    }
    wanted
}

/// The picture that the video of `layer` (a layer of the section number `section_index`) needs at
/// `timeline_frame`, a frame of that section.
fn wanted_picture_of(
    plan: &PicturePlan,
    section_index: usize,
    timeline_frame: u64,
    layer: &Layer,
    video: &VisibleVideo,
) -> Option<WantedPicture> {
    let section = &plan.sections[section_index];
    let source_frame = section.source_frame_at(video, TimelineFrameIndex::new(timeline_frame))?;

    // The stretch goes on through the sections that show the next pictures of the same clip.
    let mut range_end_source_frame = video.end_source_frame;
    let mut end_timeline_frame = section.end_timeline_frame;
    let mut last_in_the_stretch = (section, *video);
    let mut next_index = section_index + 1;
    while let Some(next) = plan.sections.get(next_index) {
        match video_of_clip(next, layer.clip) {
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
        .and_then(|next| {
            next.video_layers()
                .find(|(next_layer, _)| next_layer.track == layer.track)
        })
        .map(|(_, next)| UpcomingVideo {
            source: next.source,
            first_source_frame: SourceFrameIndex::new(next.first_source_frame),
            end_source_frame: SourceFrameIndex::new(next.end_source_frame),
            speed_factor: next.speed,
        });
    Some(WantedPicture {
        track: layer.track,
        source: video.source,
        source_frame: SourceFrameIndex::new(source_frame),
        speed_factor: video.speed,
        range_end_source_frame: SourceFrameIndex::new(range_end_source_frame),
        pictures_left: end_timeline_frame - timeline_frame,
        next_video,
    })
}

/// The video that `clip` shows in `section`, if it is a layer of it.
fn video_of_clip(section: &PictureSection, clip: ClipId) -> Option<VisibleVideo> {
    section
        .video_layers()
        .find(|(layer, _)| layer.clip == clip)
        .map(|(_, video)| *video)
}

/// Whether `next` shows the pictures that come right after those of `section`, of the same clip.
fn continues(section: &PictureSection, video: VisibleVideo, next: &PictureSection, next_video: VisibleVideo) -> bool {
    next_video.speed == video.speed
        && next.first_timeline_frame == section.end_timeline_frame
        && next_video.first_source_frame
            == video.first_source_frame + section.timeline_frame_count() * u64::from(video.speed.value())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::composition::plan_pictures;
    use crate::project::EffectInstanceId;
    use crate::project::{
        CanvasArea, Clip, FrameRate, MediaClip, Placement, Project, SourceMedia, SourceMediaLink,
        SourceVideoProperties, SpeedIndicatorLayout, TextClip, Track,
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

    /// A video that is only a quarter of the picture, so that what is behind it shows.
    fn small_video_clip(first: u64, end: u64) -> Clip {
        match video_clip(first, end, 1) {
            Clip::Video(media) => {
                Clip::Video(media.with_placement(Some(Placement::upright(CanvasArea::new(0.25, 0.25, 0.5, 0.5)))))
            }
            other => other,
        }
    }

    fn text(duration: u64) -> Clip {
        Clip::Text(TextClip::new(duration, EffectInstanceId::FIRST).unwrap())
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
            .map(|(number, clips)| {
                crate::test_support::with_the_text_effect_on_every_text(
                    Track::new(TrackId::new(number as u32), "track").with_clips(clips),
                )
            })
            .collect();
        let project = Project::from_parts(vec![source], tracks, SpeedIndicatorLayout::DEFAULT).unwrap();
        plan_pictures(&project, |_| true)
    }

    /// What is wanted at a frame of a plan with one video at a time, which is the one picture, if any.
    fn wanted(plan: &PicturePlan, timeline_frame: u64) -> Option<WantedPicture> {
        let mut all = wanted_pictures(plan, frame(timeline_frame), 0);
        assert!(all.len() <= 1, "one video at a time: {all:?}");
        all.pop()
    }

    /// Timeline: clip 0 = frames 0..100 (1x), clip 1 = 100..150 (200 source frames at 4x).
    fn two_clips() -> PicturePlan {
        plan_of(vec![vec![(0, video_clip(0, 100, 1)), (0, video_clip(100, 300, 4))]])
    }

    #[test]
    fn a_frame_in_a_normal_clip_wants_its_own_source_frame() {
        let wanted = wanted(&two_clips(), 40).unwrap();
        assert_eq!(wanted.track, TrackId::new(0));
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
        assert_eq!(wanted(&plan_of(vec![vec![]]), 0), None);
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

    // --- several videos at once ---

    #[test]
    fn every_video_that_is_seen_wants_a_picture_of_its_own_on_its_own_track() {
        // A small video in front of a big one: both are seen.
        let plan = plan_of(vec![
            vec![(10, small_video_clip(200, 260))],
            vec![(0, video_clip(0, 100, 1))],
        ]);
        let mut all = wanted_pictures(&plan, frame(20), 0);
        all.sort_by_key(|wanted| wanted.track);
        assert_eq!(all.len(), 2);
        assert_eq!(
            (all[0].track, all[0].source_frame, all[0].pictures_left),
            (TrackId::new(0), SourceFrameIndex::new(210), 50)
        );
        assert_eq!(
            (all[1].track, all[1].source_frame, all[1].pictures_left),
            (TrackId::new(1), SourceFrameIndex::new(20), 80)
        );
    }

    #[test]
    fn a_video_that_is_hidden_behind_a_video_over_the_whole_picture_wants_nothing() {
        let plan = plan_of(vec![
            vec![(0, video_clip(0, 100, 1))],
            vec![(0, video_clip(500, 600, 1))],
        ]);
        let all = wanted_pictures(&plan, frame(20), 0);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].track, TrackId::new(0));
    }

    #[test]
    fn the_stretch_of_a_video_goes_on_while_another_video_over_it_comes_and_goes() {
        // The long video behind is cut into three sections by a small one that is in front of it for a while.
        let plan = plan_of(vec![
            vec![(30, small_video_clip(300, 330))],
            vec![(0, video_clip(0, 100, 1))],
        ]);
        let behind = |frame_number: u64| {
            wanted_pictures(&plan, frame(frame_number), 0)
                .into_iter()
                .find(|wanted| wanted.track == TrackId::new(1))
                .unwrap()
        };
        assert_eq!(behind(10).pictures_left, 90);
        assert_eq!(behind(10).range_end_source_frame, SourceFrameIndex::new(100));
        assert_eq!(behind(40).pictures_left, 60);
        assert_eq!(behind(70).pictures_left, 30);
    }

    #[test]
    fn the_next_video_is_the_next_one_on_the_same_track_not_one_that_begins_on_another_track() {
        // On track 1 two clips follow each other. A small video starts on track 0 right where the first ends.
        let plan = plan_of(vec![
            vec![(50, small_video_clip(300, 330))],
            vec![(0, video_clip(0, 50, 1)), (0, video_clip(50, 100, 1))],
        ]);
        let behind = wanted_pictures(&plan, frame(10), 0)
            .into_iter()
            .find(|wanted| wanted.track == TrackId::new(1))
            .unwrap();
        assert_eq!(
            behind.next_video.unwrap().first_source_frame,
            SourceFrameIndex::new(50),
            "the clip that follows on its own track"
        );
    }

    #[test]
    fn the_next_video_of_a_track_in_front_is_its_own_and_not_the_one_of_a_track_behind_it() {
        // Track 0, in front, has two small videos one after the other. Track 1, behind, has one video the
        // whole time, so when the second small video begins the video behind is a layer too, and the one
        // that comes first in the layers (they go from the back).
        let plan = plan_of(vec![
            vec![(0, small_video_clip(300, 330)), (0, small_video_clip(400, 430))],
            vec![(0, video_clip(0, 200, 1))],
        ]);
        let in_front = wanted_pictures(&plan, frame(10), 0)
            .into_iter()
            .find(|wanted| wanted.track == TrackId::new(0))
            .unwrap();
        assert_eq!(
            in_front.next_video.unwrap().first_source_frame,
            SourceFrameIndex::new(400),
            "the video that follows on its own track, and not the video behind that goes on"
        );
    }

    // --- looking ahead ---

    #[test]
    fn a_video_that_comes_within_the_look_ahead_on_a_track_that_has_none_is_wanted_as_its_first_picture() {
        let plan = plan_of(vec![
            vec![(60, small_video_clip(300, 360))],
            vec![(0, video_clip(0, 200, 1))],
        ]);
        let mut all = wanted_pictures(&plan, frame(50), 20);
        all.sort_by_key(|wanted| wanted.track);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].track, TrackId::new(0));
        assert_eq!(all[0].source_frame, SourceFrameIndex::new(300), "its first picture");
        assert_eq!(all[0].pictures_left, 60, "counted from where it begins");
        assert_eq!(all[1].source_frame, SourceFrameIndex::new(50));
    }

    #[test]
    fn a_video_further_away_than_the_look_ahead_or_when_not_looking_ahead_is_not_wanted_yet() {
        let plan = plan_of(vec![
            vec![(60, small_video_clip(300, 360))],
            vec![(0, video_clip(0, 200, 1))],
        ]);
        assert_eq!(wanted_pictures(&plan, frame(30), 20).len(), 1, "it is 30 frames away");
        assert_eq!(wanted_pictures(&plan, frame(50), 0).len(), 1, "not looking ahead");
        assert_eq!(
            wanted_pictures(&plan, frame(50), 10).len(),
            1,
            "it begins at frame 60, exactly 10 frames on, which is not within 10"
        );
        assert_eq!(wanted_pictures(&plan, frame(50), 11).len(), 2);
    }

    #[test]
    fn a_track_that_already_shows_a_video_is_not_looked_ahead_for_it_has_its_next_video() {
        let plan = plan_of(vec![vec![(0, video_clip(0, 50, 1)), (0, video_clip(50, 100, 1))]]);
        let all = wanted_pictures(&plan, frame(40), 20);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].source_frame, SourceFrameIndex::new(40));
        assert!(all[0].next_video.is_some());
    }

    #[test]
    fn looking_ahead_wants_nothing_past_the_end_of_the_timeline() {
        let plan = plan_of(vec![vec![(0, video_clip(0, 50, 1))]]);
        assert!(wanted_pictures(&plan, frame(50), 20).is_empty());
        assert!(wanted_pictures(&plan, frame(500), 20).is_empty());
    }
}
