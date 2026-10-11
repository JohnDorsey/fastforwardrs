use super::*;
use crate::composition::test_projects::{
    LONG_VIDEO, LONG_VIDEO_FRAME_COUNT, SHORT_SILENT_VIDEO, SHORT_VIDEO_FRAME_COUNT, media_clip, project_of,
    random_project,
};
use crate::project::tracks::{ClipId, TextClip, TrackId};
use crate::test_support::DeterministicRandomNumbers;

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn speed(factor: u32) -> SpeedFactor {
    SpeedFactor::new(factor).unwrap()
}

fn video(source: SourceMediaIdentifier, first: u64, end: u64, factor: u32) -> Clip {
    Clip::Video(media_clip(source, first, end, factor))
}

fn long_video(first: u64, end: u64, factor: u32) -> Clip {
    video(LONG_VIDEO, first, end, factor)
}

fn text(duration: u64) -> Clip {
    Clip::Text(TextClip::new(TextObject::default(), duration).unwrap())
}

fn plan(project: &Project) -> PicturePlan {
    plan_pictures(project, |_| true)
}

/// A section in short: where it is, which source frames it shows (first, end, speed) and how many text objects.
type Summary = (u64, u64, Option<(u64, u64, u32)>, usize);

fn summary(plan: &PicturePlan) -> Vec<Summary> {
    plan.sections
        .iter()
        .map(|section| {
            (
                section.first_timeline_frame,
                section.end_timeline_frame,
                section
                    .video
                    .map(|video| (video.first_source_frame, video.end_source_frame, video.speed.value())),
                section.texts.len(),
            )
        })
        .collect()
}

// --- simple projects ---

#[test]
fn an_empty_project_has_a_plan_with_no_sections() {
    let plan = plan(&Project::empty());
    assert!(plan.sections.is_empty());
    assert_eq!(plan.total_frame_count, 0);
    assert!(plan.section_at(frame(0)).is_none());
}

#[test]
fn the_plan_carries_the_frame_rate_and_the_picture_size_of_the_project() {
    let plan = plan(&project_of(vec![vec![(0, long_video(0, 30, 1))]]));
    assert_eq!(plan.frame_rate, FrameRate::from_whole_frames_per_second(30).unwrap());
    assert_eq!(plan.picture_size, (640, 360));
    assert_eq!(plan.total_frame_count, 30);
}

#[test]
fn one_video_clip_is_one_section_and_clips_that_touch_are_separate_sections() {
    let project = project_of(vec![vec![
        (0, long_video(0, 100, 1)),
        (0, long_video(100, 250, 2)),
        (0, long_video(250, 300, 1)),
    ]]);
    assert_eq!(
        summary(&plan(&project)),
        vec![
            (0, 100, Some((0, 100, 1)), 0),
            (100, 175, Some((100, 250, 2)), 0),
            (175, 225, Some((250, 300, 1)), 0),
        ]
    );
}

/// The source range of a section that is a whole clip is the clip's own range, even when the speed
/// does not divide it, so that the export cuts exactly what the clip is.
#[test]
fn a_whole_clip_keeps_its_own_source_range_even_when_the_speed_does_not_divide_it() {
    let project = project_of(vec![vec![(0, long_video(0, 11, 3)), (0, long_video(11, 111, 4))]]);
    assert_eq!(
        summary(&plan(&project)),
        vec![(0, 4, Some((0, 11, 3)), 0), (4, 29, Some((11, 111, 4)), 0)]
    );
}

#[test]
fn the_speed_of_a_section_is_the_clips_times_the_tracks() {
    let mut project = project_of(vec![vec![(0, long_video(0, 120, 3))]]);
    let track = project.tracks()[0].id();
    project.set_track_speed(track, speed(2)).unwrap();
    let plan = plan(&project);
    assert_eq!(plan.sections[0].video.unwrap().speed, speed(6));
    assert_eq!(summary(&plan), vec![(0, 20, Some((0, 120, 6)), 0)]);
}

#[test]
fn empty_time_is_a_black_section() {
    let project = project_of(vec![vec![(20, long_video(0, 30, 1)), (10, long_video(30, 60, 1))]]);
    assert_eq!(
        summary(&plan(&project)),
        vec![
            (0, 20, None, 0),
            (20, 50, Some((0, 30, 1)), 0),
            (50, 60, None, 0),
            (60, 90, Some((30, 60, 1)), 0),
        ]
    );
}

#[test]
fn sections_are_found_by_the_frame_they_contain() {
    let project = project_of(vec![vec![(20, long_video(0, 30, 1))]]);
    let plan = plan(&project);
    assert_eq!(plan.section_at(frame(0)).unwrap().video, None);
    assert_eq!(plan.section_at(frame(19)).unwrap().video, None);
    assert!(plan.section_at(frame(20)).unwrap().video.is_some());
    assert!(plan.section_at(frame(49)).unwrap().video.is_some());
    assert!(plan.section_at(frame(50)).is_none(), "past the end");
    let section = plan.section_at(frame(25)).unwrap();
    assert_eq!(section.source_frame_at(frame(25)), Some(5));
    assert_eq!(section.source_frame_at(frame(0)), None, "not in the section");
}

// --- layers ---

#[test]
fn text_in_front_of_a_video_is_over_it_and_text_behind_it_is_hidden() {
    let project = project_of(vec![
        vec![(10, text(30))],             // in front
        vec![(0, long_video(0, 100, 1))], // the video
        vec![(0, text(100))],             // behind the video
    ]);
    let plan = plan(&project);
    assert_eq!(
        summary(&plan),
        vec![
            (0, 10, Some((0, 10, 1)), 0),
            (10, 40, Some((10, 40, 1)), 1),
            (40, 100, Some((40, 100, 1)), 0),
        ],
        "only the text in front is ever seen, and not when it is not there"
    );
    assert_eq!(plan.sections[1].texts[0].clip.track, TrackId::new(0));
}

#[test]
fn text_over_black_is_all_there_with_the_one_at_the_back_first() {
    let project = project_of(vec![vec![(0, text(50))], vec![(10, text(20))], vec![(0, text(50))]]);
    let plan = plan(&project);
    let tracks_of_texts_in_the_middle: Vec<TrackId> = plan
        .section_at(frame(15))
        .unwrap()
        .texts
        .iter()
        .map(|text| text.clip.track)
        .collect();
    assert_eq!(
        tracks_of_texts_in_the_middle,
        vec![TrackId::new(2), TrackId::new(1), TrackId::new(0)],
        "back to front, which is the order they are drawn in"
    );
    assert_eq!(plan.section_at(frame(5)).unwrap().texts.len(), 2);
    assert!(plan.sections.iter().all(|section| section.video.is_none()));
}

#[test]
fn of_two_videos_at_the_same_time_only_the_one_in_front_is_seen_and_the_other_comes_back_where_it_would_have_been() {
    // Behind: source frames 0..300 from timeline frame 0. In front: from 50 to 100.
    let project = project_of(vec![
        vec![(50, long_video(100, 150, 1))],
        vec![(0, long_video(0, 300, 1))],
    ]);
    assert_eq!(
        summary(&plan(&project)),
        vec![
            (0, 50, Some((0, 50, 1)), 0),
            (50, 100, Some((100, 150, 1)), 0),
            (100, 300, Some((100, 300, 1)), 0),
        ],
        "after the clip in front the video behind shows the frame it would have shown all along"
    );
}

#[test]
fn a_video_behind_that_is_sped_up_picks_up_at_the_right_picture_when_it_is_uncovered() {
    let project = project_of(vec![vec![(40, long_video(0, 40, 1))], vec![(0, long_video(0, 300, 3))]]);
    let plan = plan(&project);
    // The video behind is at 3x: at timeline frame 80 it shows source frame 240.
    assert_eq!(
        plan.section_at(frame(80)).unwrap().source_frame_at(frame(80)),
        Some(240)
    );
    assert_eq!(plan.section_at(frame(80)).unwrap().video.unwrap().speed, speed(3));
}

#[test]
fn a_clip_that_is_hidden_the_whole_time_makes_no_difference_to_the_plan() {
    let alone = project_of(vec![vec![(0, long_video(0, 100, 1))]]);
    let with_a_hidden_clip = project_of(vec![
        vec![(0, long_video(0, 100, 1))],
        vec![(20, long_video(0, 30, 1)), (5, text(10))],
    ]);
    let section_count = |project: &Project| plan(project).sections.len();
    assert_eq!(section_count(&alone), 1);
    assert_eq!(
        section_count(&with_a_hidden_clip),
        1,
        "no section is made for what cannot be seen"
    );
}

#[test]
fn a_boundary_that_changes_nothing_that_is_seen_does_not_split_a_section() {
    // A sound clip changes nothing in the picture, wherever it begins and ends.
    let project = project_of(vec![
        vec![(
            30,
            Clip::Audio(media_clip(crate::composition::test_projects::SOUND_FILE, 0, 20, 1)),
        )],
        vec![(0, long_video(0, 100, 1))],
    ]);
    assert_eq!(summary(&plan(&project)), vec![(0, 100, Some((0, 100, 1)), 0)]);
}

#[test]
fn a_text_object_cuts_the_video_under_it_into_pieces_that_run_on_from_each_other() {
    // A video of 100 frames (200 source frames at 2x) with text over frames 30 up to 50.
    let project = project_of(vec![vec![(30, text(20))], vec![(0, long_video(0, 200, 2))]]);
    assert_eq!(
        summary(&plan(&project)),
        vec![
            (0, 30, Some((0, 60, 2)), 0),
            (30, 50, Some((60, 100, 2)), 1),
            (50, 100, Some((100, 200, 2)), 0),
        ]
    );
}

// --- files that run out ---

#[test]
fn a_clip_that_reaches_past_the_end_of_its_file_is_black_for_the_rest_and_the_timeline_keeps_its_length() {
    // The short video has 90 frames; the clip asks for 90..150 - none exist at all - and 60..120.
    let project = project_of(vec![vec![(0, video(SHORT_SILENT_VIDEO, 60, 120, 1))]]);
    assert_eq!(SHORT_VIDEO_FRAME_COUNT, 90);
    let plan = plan(&project);
    assert_eq!(
        summary(&plan),
        vec![(0, 30, Some((60, 90, 1)), 0), (30, 60, None, 0)],
        "30 pictures exist, then 30 frames of black"
    );
    assert_eq!(plan.total_frame_count, 60);
}

#[test]
fn a_sped_up_clip_runs_out_of_pictures_where_its_file_runs_out_of_frames() {
    // 100 frames from 50 at 4x would be 25 pictures, but the file has 90 frames: 50, 54, ..., 86, 90?
    // No: 90 is not a frame of the file. The pictures are source frames 50, 54, ..., 86: ten of them.
    let project = project_of(vec![vec![(0, video(SHORT_SILENT_VIDEO, 50, 150, 4))]]);
    assert_eq!(
        summary(&plan(&project)),
        vec![(0, 10, Some((50, 90, 4)), 0), (10, 25, None, 0)]
    );
}

#[test]
fn text_stays_over_the_black_where_a_video_has_run_out_and_text_behind_it_stays_hidden() {
    let project = project_of(vec![
        vec![(0, text(40))],
        vec![(0, video(SHORT_SILENT_VIDEO, 80, 130, 1))],
        vec![(0, text(40))],
    ]);
    let plan = plan(&project);
    let last = plan.section_at(frame(30)).unwrap();
    assert_eq!(last.video, None);
    assert_eq!(
        last.texts.len(),
        1,
        "only the text in front of the video that was there"
    );
    assert_eq!(last.texts[0].clip.track, TrackId::new(0));
}

#[test]
fn the_frames_that_are_black_for_lack_of_pictures_are_counted_and_other_black_frames_are_not() {
    let nothing_missing = project_of(vec![vec![(20, long_video(0, 30, 1))], vec![(0, text(10))]]);
    assert_eq!(
        plan(&nothing_missing).frames_black_for_lack_of_pictures,
        0,
        "gaps and text over black are not missing"
    );

    // 30 pictures and then 30 black frames; and at 4x, ten pictures and fifteen black.
    let short = project_of(vec![vec![
        (0, video(SHORT_SILENT_VIDEO, 60, 120, 1)),
        (0, video(SHORT_SILENT_VIDEO, 50, 150, 4)),
    ]]);
    assert_eq!(plan(&short).frames_black_for_lack_of_pictures, 30 + 15);

    // A file that cannot be used is all black: every frame of its clip.
    let unusable = plan_pictures(&project_of(vec![vec![(0, long_video(0, 60, 2))]]), |_| false);
    assert_eq!(unusable.frames_black_for_lack_of_pictures, 30);
}

#[test]
fn a_clip_starting_after_the_end_of_its_file_is_all_black() {
    let project = project_of(vec![vec![(0, video(SHORT_SILENT_VIDEO, 95, 120, 1))]]);
    assert_eq!(summary(&plan(&project)), vec![(0, 25, None, 0)]);
}

#[test]
fn a_file_that_cannot_be_used_is_black_wherever_it_is_used() {
    let project = project_of(vec![vec![
        (0, long_video(0, 60, 1)),
        (0, video(SHORT_SILENT_VIDEO, 0, 30, 1)),
    ]]);
    let plan = plan_pictures(&project, |source| source != LONG_VIDEO);
    assert_eq!(summary(&plan), vec![(0, 60, None, 0), (60, 90, Some((0, 30, 1)), 0)]);
}

// --- checked against the plain answer ---

/// What is seen at `timeline_frame`, worked out the plain way: look at each track from the front,
/// and see what its clip at that frame is. The answer is the video picture there is (the clip
/// and the source frame), the text over it (the one at the back first), and whether a video clip
/// is there that has no picture to show.
fn plainly_seen(project: &Project, timeline_frame: u64) -> (Option<(ClipLocation, u64)>, Vec<ClipLocation>, bool) {
    let mut texts_in_front = Vec::new();
    for track in project.tracks() {
        let Some(placement) = track.placement_covering(frame(timeline_frame)) else {
            continue;
        };
        let location = ClipLocation {
            track: track.id(),
            clip: placement.id,
        };
        match placement.clip {
            Clip::Text(_) => texts_in_front.push(location),
            Clip::Video(media) => {
                let source_frame = placement.source_frame_at(frame(timeline_frame)).unwrap().value();
                let file_frames = project.frame_count_of_source(media.source()).unwrap();
                let has_picture = source_frame < media.end_source_frame().value() && source_frame < file_frames;
                texts_in_front.reverse(); // from the back
                return (
                    has_picture.then_some((location, source_frame)),
                    texts_in_front,
                    !has_picture,
                );
            }
            Clip::Audio(_) => {}
        }
    }
    texts_in_front.reverse();
    (None, texts_in_front, false)
}

#[test]
fn the_plan_shows_at_every_frame_exactly_what_looking_at_the_tracks_one_frame_at_a_time_shows() {
    for seed in 1..=300u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let project = random_project(&mut random);
        let plan = plan(&project);
        let describe = || format!("seed {seed}: tracks {:?}", project.tracks());

        assert_eq!(plan.total_frame_count, project.total_frame_count(), "{}", describe());
        // The sections follow one another from the first frame to the last, and each is worth having.
        let mut next_frame = 0;
        for section in &plan.sections {
            assert_eq!(section.first_timeline_frame, next_frame, "{}", describe());
            assert!(
                section.end_timeline_frame > section.first_timeline_frame,
                "{}",
                describe()
            );
            next_frame = section.end_timeline_frame;
        }
        assert_eq!(next_frame, plan.total_frame_count, "{}", describe());

        let mut frames_without_pictures = 0;
        for timeline_frame in 0..plan.total_frame_count {
            let section = plan.section_at(frame(timeline_frame)).unwrap();
            let (expected_video, expected_texts, video_without_picture) = plainly_seen(&project, timeline_frame);
            frames_without_pictures += u64::from(video_without_picture);
            let seen_video = section
                .video
                .map(|video| (video.clip, section.source_frame_at(frame(timeline_frame)).unwrap()));
            assert_eq!(seen_video, expected_video, "frame {timeline_frame}, {}", describe());
            let seen_texts: Vec<ClipLocation> = section.texts.iter().map(|text| text.clip).collect();
            assert_eq!(seen_texts, expected_texts, "frame {timeline_frame}, {}", describe());
        }
        assert_eq!(
            plan.frames_black_for_lack_of_pictures,
            frames_without_pictures,
            "the count of black frames: {}",
            describe()
        );
    }
}

#[test]
fn every_picture_a_section_asks_a_file_for_is_there_and_the_range_it_asks_for_gives_exactly_that_many() {
    for seed in 1..=300u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed + 1000);
        let project = random_project(&mut random);
        for section in plan(&project).sections {
            let Some(video) = section.video else { continue };
            let file_frames = project.frame_count_of_source(video.source).unwrap();
            let speed = u64::from(video.speed.value());
            let pictures = section.timeline_frame_count();
            let last_wanted = video.first_source_frame + (pictures - 1) * speed;
            assert!(
                last_wanted < file_frames,
                "seed {seed}: the last picture of {section:?} is past the file's end"
            );
            // What the export asks for: the frames from the first up to the end, every speed-th one.
            let pictures_in_range = (video.end_source_frame - video.first_source_frame).div_ceil(speed);
            assert_eq!(pictures_in_range, pictures, "seed {seed}: {section:?}");
            assert!(video.end_source_frame <= file_frames, "seed {seed}: {section:?}");
        }
    }
}

#[test]
fn nothing_is_split_that_does_not_need_to_be() {
    for seed in 1..=300u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed + 5000);
        let plan = plan(&random_project(&mut random));
        for pair in plan.sections.windows(2) {
            let (before, after) = (&pair[0], &pair[1]);
            let could_have_been_one = before.texts == after.texts
                && match (before.video, after.video) {
                    (None, None) => true,
                    (Some(first), Some(second)) => {
                        first.clip == second.clip
                            && first.speed == second.speed
                            && second.first_source_frame
                                == first.first_source_frame
                                    + before.timeline_frame_count() * u64::from(first.speed.value())
                    }
                    _ => false,
                };
            assert!(
                !could_have_been_one,
                "seed {seed}: {before:?} and {after:?} should be one section"
            );
        }
    }
}

#[test]
fn clip_numbers_are_the_clips_own() {
    let project = project_of(vec![vec![(0, long_video(0, 10, 1))]]);
    let section = &plan(&project).sections[0];
    assert_eq!(
        section.video.unwrap().clip,
        ClipLocation {
            track: TrackId::new(0),
            clip: ClipId::new(0)
        }
    );
    assert_eq!(section.video.unwrap().source, LONG_VIDEO);
    assert_eq!(LONG_VIDEO_FRAME_COUNT, 300);
}
