// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::composition::test_projects::{
    LONG_VIDEO, LONG_VIDEO_FRAME_COUNT, SHORT_SILENT_VIDEO, SHORT_VIDEO_FRAME_COUNT, TALL_VIDEO, WIDE_PICTURE,
    media_clip, project_of, random_project_with_places, random_project_with_places_and_keyframes,
};
use crate::project::tracks::{Angle, CanvasArea, ClipId, ImageClip, Placement, TextClip, TrackId};
use crate::project::{AnimationTarget, SourceMediaIdentifier, fitted_placement, with_placement_value};
use crate::test_support::DeterministicRandomNumbers;

pub(super) fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn speed(factor: u32) -> SpeedFactor {
    SpeedFactor::new(factor).unwrap()
}

fn video(source: SourceMediaIdentifier, first: u64, end: u64, factor: u32) -> Clip {
    Clip::Video(media_clip(source, first, end, factor))
}

pub(super) fn long_video(first: u64, end: u64, factor: u32) -> Clip {
    video(LONG_VIDEO, first, end, factor)
}

/// A video that stands upright, so it does not fill the picture, which is wide.
pub(super) fn tall_video(first: u64, end: u64) -> Clip {
    video(TALL_VIDEO, first, end, 1)
}

/// A long video that was put at `placement`.
pub(super) fn long_video_placed(first: u64, end: u64, placement: Placement) -> Clip {
    Clip::Video(media_clip(LONG_VIDEO, first, end, 1).with_placement(Some(placement)))
}

pub(super) fn over_the_whole_picture() -> Placement {
    Placement::upright(CanvasArea::WHOLE_PICTURE)
}

pub(super) fn text(duration: u64) -> Clip {
    Clip::Text(TextClip::new(TextObject::default(), duration).unwrap())
}

pub(super) fn plan(project: &Project) -> PicturePlan {
    plan_pictures(project, |_| true)
}

/// What a layer shows, in short.
#[derive(Debug, PartialEq)]
enum Seen {
    /// A video: the source frames it shows (first, end) and its speed.
    Video(u64, u64, u32),
    Text,
    Picture,
    /// A generator clip.
    Generator,
}

fn seen(layer: &Layer) -> Seen {
    match &layer.content {
        LayerContent::Video(video) => {
            Seen::Video(video.first_source_frame, video.end_source_frame, video.speed.value())
        }
        LayerContent::Text(_) => Seen::Text,
        LayerContent::Image { .. } => Seen::Picture,
        LayerContent::Generator => Seen::Generator,
        LayerContent::Adjustment => unreachable!("an adjustment clip is not a layer of a section"),
    }
}

/// A picture of the wide file, shown for `duration` frames.
pub(super) fn picture(duration: u64) -> Clip {
    Clip::Image(ImageClip::new(WIDE_PICTURE, duration).unwrap())
}

/// A section in short: where it is, and what its layers show, the one at the back first.
type Summary = (u64, u64, Vec<Seen>);

fn summary(plan: &PicturePlan) -> Vec<Summary> {
    plan.sections
        .iter()
        .map(|section| {
            (
                section.first_timeline_frame,
                section.end_timeline_frame,
                section.layers.iter().map(seen).collect(),
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
            (0, 100, vec![Seen::Video(0, 100, 1)]),
            (100, 175, vec![Seen::Video(100, 250, 2)]),
            (175, 225, vec![Seen::Video(250, 300, 1)]),
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
        vec![
            (0, 4, vec![Seen::Video(0, 11, 3)]),
            (4, 29, vec![Seen::Video(11, 111, 4)])
        ]
    );
}

#[test]
fn the_speed_of_a_section_is_the_clips_times_the_tracks() {
    let mut project = project_of(vec![vec![(0, long_video(0, 120, 3))]]);
    let track = project.tracks()[0].id();
    project.set_track_speed(track, speed(2)).unwrap();
    let plan = plan(&project);
    assert_eq!(plan.sections[0].layers[0].video().unwrap().speed, speed(6));
    assert_eq!(summary(&plan), vec![(0, 20, vec![Seen::Video(0, 120, 6)])]);
}

#[test]
fn empty_time_is_a_section_with_nothing_in_it_which_is_black() {
    let project = project_of(vec![vec![(20, long_video(0, 30, 1)), (10, long_video(30, 60, 1))]]);
    assert_eq!(
        summary(&plan(&project)),
        vec![
            (0, 20, vec![]),
            (20, 50, vec![Seen::Video(0, 30, 1)]),
            (50, 60, vec![]),
            (60, 90, vec![Seen::Video(30, 60, 1)]),
        ]
    );
}

#[test]
fn sections_are_found_by_the_frame_they_contain() {
    let project = project_of(vec![vec![(20, long_video(0, 30, 1))]]);
    let plan = plan(&project);
    assert!(plan.section_at(frame(0)).unwrap().layers.is_empty());
    assert!(plan.section_at(frame(19)).unwrap().layers.is_empty());
    assert_eq!(plan.section_at(frame(20)).unwrap().layers.len(), 1);
    assert_eq!(plan.section_at(frame(49)).unwrap().layers.len(), 1);
    assert!(plan.section_at(frame(50)).is_none(), "past the end");
    let section = plan.section_at(frame(25)).unwrap();
    let video = section.layers[0].video().unwrap();
    assert_eq!(section.source_frame_at(video, frame(25)), Some(5));
    assert_eq!(section.source_frame_at(video, frame(0)), None, "not in the section");
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
            (0, 10, vec![Seen::Video(0, 10, 1)]),
            (10, 40, vec![Seen::Video(10, 40, 1), Seen::Text]),
            (40, 100, vec![Seen::Video(40, 100, 1)]),
        ],
        "only the text in front is ever seen, and not when it is not there"
    );
    assert_eq!(plan.sections[1].layers[1].track, TrackId::new(0));
}

#[test]
fn text_over_black_is_all_there_with_the_one_at_the_back_first() {
    let project = project_of(vec![vec![(0, text(50))], vec![(10, text(20))], vec![(0, text(50))]]);
    let plan = plan(&project);
    let tracks_of_texts_in_the_middle: Vec<TrackId> = plan
        .section_at(frame(15))
        .unwrap()
        .layers
        .iter()
        .map(|layer| layer.track)
        .collect();
    assert_eq!(
        tracks_of_texts_in_the_middle,
        vec![TrackId::new(2), TrackId::new(1), TrackId::new(0)],
        "back to front, which is the order they are drawn in"
    );
    assert_eq!(plan.section_at(frame(5)).unwrap().layers.len(), 2);
    assert!(
        plan.sections
            .iter()
            .flat_map(|section| &section.layers)
            .all(|layer| layer.video().is_none())
    );
}

#[test]
fn of_two_videos_at_the_same_time_that_fill_the_picture_only_the_one_in_front_is_seen_and_the_other_comes_back_where_it_would_have_been()
 {
    // Behind: source frames 0..300 from timeline frame 0. In front: from 50 to 100.
    let project = project_of(vec![
        vec![(50, long_video(100, 150, 1))],
        vec![(0, long_video(0, 300, 1))],
    ]);
    assert_eq!(
        summary(&plan(&project)),
        vec![
            (0, 50, vec![Seen::Video(0, 50, 1)]),
            (50, 100, vec![Seen::Video(100, 150, 1)]),
            (100, 300, vec![Seen::Video(100, 300, 1)]),
        ],
        "after the clip in front the video behind shows the frame it would have shown all along"
    );
}

#[test]
fn a_video_behind_that_is_sped_up_picks_up_at_the_right_picture_when_it_is_uncovered() {
    let project = project_of(vec![vec![(40, long_video(0, 40, 1))], vec![(0, long_video(0, 300, 3))]]);
    let plan = plan(&project);
    // The video behind is at 3x: at timeline frame 80 it shows source frame 240.
    let section = plan.section_at(frame(80)).unwrap();
    let video = section.layers[0].video().unwrap();
    assert_eq!(section.source_frame_at(video, frame(80)), Some(240));
    assert_eq!(video.speed, speed(3));
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
    assert_eq!(summary(&plan(&project)), vec![(0, 100, vec![Seen::Video(0, 100, 1)])]);
}

#[test]
fn a_text_object_cuts_the_video_under_it_into_pieces_that_run_on_from_each_other() {
    // A video of 100 frames (200 source frames at 2x) with text over frames 30 up to 50.
    let project = project_of(vec![vec![(30, text(20))], vec![(0, long_video(0, 200, 2))]]);
    assert_eq!(
        summary(&plan(&project)),
        vec![
            (0, 30, vec![Seen::Video(0, 60, 2)]),
            (30, 50, vec![Seen::Video(60, 100, 2), Seen::Text]),
            (50, 100, vec![Seen::Video(100, 200, 2)]),
        ]
    );
}

// --- where things are on the picture ---

#[test]
fn a_video_that_was_not_placed_is_fitted_inside_the_picture_and_one_that_was_is_where_it_was_put() {
    let somewhere = Placement::new(CanvasArea::new(0.1, 0.2, 0.3, 0.4), Angle::from_degrees(25.0));
    let project = project_of(vec![
        vec![(0, long_video_placed(0, 50, somewhere))],
        vec![(0, tall_video(0, 50))],
        vec![(0, long_video(0, 50, 1))],
    ]);
    let plan = plan(&project);
    let places: Vec<Placement> = plan.sections[0].layers.iter().map(|layer| layer.placement).collect();
    assert_eq!(
        places,
        vec![
            Placement::upright(CanvasArea::WHOLE_PICTURE),
            fitted_placement((360, 640), (640, 360)),
            somewhere,
        ],
        "back to front: the long video is as big as the picture, the tall one is in the middle of it, and the one in front is where it was put"
    );
}

#[test]
fn a_text_layer_is_where_its_text_object_is_and_turned_as_it_is() {
    let object = TextObject::default().with_placement(Placement::new(
        CanvasArea::new(0.2, 0.3, 0.4, 0.2),
        Angle::from_degrees(-40.0),
    ));
    let project = project_of(vec![vec![(0, Clip::Text(TextClip::new(object.clone(), 20).unwrap()))]]);
    let plan = plan(&project);
    let layer = &plan.sections[0].layers[0];
    assert_eq!(layer.placement, object.placement());
    assert_eq!(layer.content, LayerContent::Text(object));
}

#[test]
fn a_video_that_does_not_fill_the_picture_lets_what_is_behind_it_show() {
    let project = project_of(vec![vec![(0, tall_video(0, 60))], vec![(0, long_video(0, 60, 1))]]);
    let plan = plan(&project);
    assert_eq!(
        summary(&plan),
        vec![(0, 60, vec![Seen::Video(0, 60, 1), Seen::Video(0, 60, 1)])]
    );
    let sources: Vec<SourceMediaIdentifier> = plan.sections[0]
        .layers
        .iter()
        .map(|layer| layer.video().unwrap().source)
        .collect();
    assert_eq!(sources, vec![LONG_VIDEO, TALL_VIDEO], "the tall one is in front");
}

#[test]
fn a_video_put_over_the_whole_picture_and_not_turned_hides_what_is_behind_it() {
    let project = project_of(vec![
        vec![(0, text(60))],
        vec![(0, long_video_placed(0, 60, over_the_whole_picture()))],
        vec![(0, tall_video(0, 60))],
        vec![(0, text(60))],
    ]);
    assert_eq!(
        summary(&plan(&project)),
        vec![(0, 60, vec![Seen::Video(0, 60, 1), Seen::Text])]
    );
}

#[test]
fn a_video_over_the_whole_picture_that_is_turned_even_a_little_does_not_hide_what_is_behind_it() {
    let barely_turned = Placement::new(CanvasArea::WHOLE_PICTURE, Angle::from_degrees(0.5));
    let project = project_of(vec![
        vec![(0, long_video_placed(0, 60, barely_turned))],
        vec![(0, tall_video(0, 60))],
    ]);
    assert_eq!(
        summary(&plan(&project)),
        vec![(0, 60, vec![Seen::Video(0, 60, 1), Seen::Video(0, 60, 1)])]
    );
}

#[test]
fn a_video_that_is_smaller_than_the_picture_does_not_hide_the_text_behind_it() {
    let smaller = Placement::upright(CanvasArea::new(0.25, 0.25, 0.5, 0.5));
    let project = project_of(vec![vec![(0, long_video_placed(0, 60, smaller))], vec![(0, text(60))]]);
    assert_eq!(
        summary(&plan(&project)),
        vec![(0, 60, vec![Seen::Text, Seen::Video(0, 60, 1)])]
    );
}

#[test]
fn text_never_hides_what_is_behind_it_even_if_its_area_is_the_whole_picture() {
    let covering_text = TextObject::default().with_placement(over_the_whole_picture());
    let project = project_of(vec![
        vec![(0, Clip::Text(TextClip::new(covering_text, 60).unwrap()))],
        vec![(0, long_video(0, 60, 1))],
    ]);
    assert_eq!(
        summary(&plan(&project)),
        vec![(0, 60, vec![Seen::Video(0, 60, 1), Seen::Text])]
    );
}

#[test]
fn only_the_layers_in_front_of_the_front_most_one_that_covers_the_picture_are_kept() {
    // From the front: text, a tall video, the video over the whole picture, a long video, text.
    let project = project_of(vec![
        vec![(0, text(40))],
        vec![(0, tall_video(0, 40))],
        vec![(0, long_video_placed(0, 40, over_the_whole_picture()))],
        vec![(0, long_video(0, 40, 1))],
        vec![(0, text(40))],
    ]);
    let plan = plan(&project);
    let tracks: Vec<TrackId> = plan.sections[0].layers.iter().map(|layer| layer.track).collect();
    assert_eq!(tracks, vec![TrackId::new(2), TrackId::new(1), TrackId::new(0)]);
}

#[test]
fn a_clip_hidden_for_a_while_comes_back_as_a_piece_that_runs_on_where_it_would_have_been() {
    // A long video behind, covered by a full-picture one for frames 30 up to 50, which is placed on top.
    let project = project_of(vec![
        vec![(30, long_video_placed(0, 20, over_the_whole_picture()))],
        vec![(0, long_video(0, 100, 1))],
    ]);
    assert_eq!(
        summary(&plan(&project)),
        vec![
            (0, 30, vec![Seen::Video(0, 30, 1)]),
            (30, 50, vec![Seen::Video(0, 20, 1)]),
            (50, 100, vec![Seen::Video(50, 100, 1)]),
        ]
    );
}

#[test]
fn sections_whose_layers_are_in_other_places_are_not_joined_even_if_they_show_the_same_pictures() {
    // Two clips that follow each other on one track, the second put somewhere: the source frames
    // run on, but it is not where the first was.
    let somewhere = Placement::upright(CanvasArea::new(0.1, 0.1, 0.5, 0.5));
    let project = project_of(vec![vec![
        (0, long_video(0, 50, 1)),
        (0, long_video_placed(50, 100, somewhere)),
    ]]);
    assert_eq!(plan(&project).sections.len(), 2);
}

// --- pictures ---

#[test]
fn a_picture_is_a_layer_fitted_inside_the_picture_until_it_is_put_somewhere() {
    let somewhere = Placement::new(CanvasArea::new(0.1, 0.2, 0.3, 0.4), Angle::from_degrees(25.0));
    let project = project_of(vec![
        vec![(
            0,
            Clip::Image(
                ImageClip::new(WIDE_PICTURE, 40)
                    .unwrap()
                    .with_placement(Some(somewhere)),
            ),
        )],
        vec![(0, picture(40))],
        vec![(0, long_video(0, 40, 1))],
    ]);
    let plan = plan(&project);
    assert_eq!(
        summary(&plan),
        vec![(0, 40, vec![Seen::Video(0, 40, 1), Seen::Picture, Seen::Picture])]
    );
    let places: Vec<Placement> = plan.sections[0].layers.iter().map(|layer| layer.placement).collect();
    assert_eq!(
        places[1],
        fitted_placement((400, 200), (640, 360)),
        "fitted: 640 by 320, in the middle"
    );
    assert_eq!(places[2], somewhere);
    assert_eq!(
        plan.sections[0].layers[1].content,
        LayerContent::Image { source: WIDE_PICTURE }
    );
}

#[test]
fn a_picture_never_hides_what_is_behind_it_even_one_that_is_put_over_the_whole_picture() {
    let over_all = Clip::Image(
        ImageClip::new(WIDE_PICTURE, 40)
            .unwrap()
            .with_placement(Some(over_the_whole_picture())),
    );
    let project = project_of(vec![vec![(0, over_all)], vec![(0, long_video(0, 40, 1))]]);
    assert_eq!(
        summary(&plan(&project)),
        vec![(0, 40, vec![Seen::Video(0, 40, 1), Seen::Picture])],
        "it may have clear places, and a picture file does not say whether it has"
    );
}

#[test]
fn a_picture_behind_a_video_that_fills_the_picture_is_hidden_like_text_is() {
    let project = project_of(vec![vec![(0, long_video(0, 40, 1))], vec![(0, picture(40))]]);
    assert_eq!(summary(&plan(&project)), vec![(0, 40, vec![Seen::Video(0, 40, 1)])]);
}

#[test]
fn a_picture_over_black_is_the_only_layer_and_shows_for_exactly_as_long_as_it_is_shown_for() {
    let project = project_of(vec![vec![(10, picture(30))]]);
    assert_eq!(
        summary(&plan(&project)),
        vec![(0, 10, vec![]), (10, 40, vec![Seen::Picture])]
    );
}

#[test]
fn a_picture_whose_file_cannot_be_used_is_not_a_layer() {
    let project = project_of(vec![vec![(0, picture(30))], vec![(0, long_video(0, 30, 1))]]);
    let plan = plan_pictures(&project, |source| source != WIDE_PICTURE);
    assert_eq!(summary(&plan), vec![(0, 30, vec![Seen::Video(0, 30, 1)])]);
    assert_eq!(
        plan.frames_missing_a_picture, 0,
        "that is counted for the pictures of videos only"
    );
}

#[test]
fn a_picture_that_a_video_starting_and_stopping_behind_it_cuts_into_sections_is_still_one_picture_that_runs_on() {
    // The picture is in front of a video that is cut in two.
    let project = project_of(vec![
        vec![(0, picture(100))],
        vec![(0, long_video(0, 50, 1)), (0, long_video(50, 100, 1))],
    ]);
    let plan = plan(&project);
    assert_eq!(
        summary(&plan),
        vec![
            (0, 50, vec![Seen::Video(0, 50, 1), Seen::Picture]),
            (50, 100, vec![Seen::Video(50, 100, 1), Seen::Picture]),
        ],
        "the video clips are two, so there are two sections; the picture is in both"
    );
    // A cut that changes nothing in the picture is not made: a text clip that starts and ends
    // behind the covering video is not seen, so it does not split anything.
    let hidden = project_of(vec![
        vec![(0, picture(100))],
        vec![(0, long_video(0, 100, 1))],
        vec![(30, text(20))],
    ]);
    assert_eq!(super::plan_pictures(&hidden, |_| true).sections.len(), 1);
}

// --- files that run out ---

#[test]
fn a_clip_that_reaches_past_the_end_of_its_file_has_no_picture_for_the_rest_and_the_timeline_keeps_its_length() {
    // The short video has 90 frames; the clip asks for 90..150 - none exist at all - and 60..120.
    let project = project_of(vec![vec![(0, video(SHORT_SILENT_VIDEO, 60, 120, 1))]]);
    assert_eq!(SHORT_VIDEO_FRAME_COUNT, 90);
    let plan = plan(&project);
    assert_eq!(
        summary(&plan),
        vec![(0, 30, vec![Seen::Video(60, 90, 1)]), (30, 60, vec![])],
        "30 pictures exist, then 30 frames with nothing to show"
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
        vec![(0, 10, vec![Seen::Video(50, 90, 4)]), (10, 25, vec![])]
    );
}

/// What a video that has no more pictures leaves of the picture is what there would be if it were
/// not there: what is behind it shows, which is the text.
#[test]
fn where_a_video_has_run_out_it_is_as_if_it_were_not_there_so_what_was_behind_it_shows() {
    let project = project_of(vec![
        vec![(0, text(40))],
        vec![(0, video(SHORT_SILENT_VIDEO, 80, 130, 1))],
        vec![(0, text(40))],
    ]);
    let plan = plan(&project);
    let tracks_at = |frame_number: u64| -> Vec<TrackId> {
        plan.section_at(frame(frame_number))
            .unwrap()
            .layers
            .iter()
            .map(|layer| layer.track)
            .collect()
    };
    assert_eq!(
        tracks_at(5),
        vec![TrackId::new(1), TrackId::new(0)],
        "the video with the text in front of it, and no text behind it"
    );
    assert_eq!(
        tracks_at(30),
        vec![TrackId::new(2), TrackId::new(0)],
        "both texts, the one behind first"
    );
}

#[test]
fn a_video_that_has_run_out_lets_the_video_behind_it_show() {
    // In front: 30 pictures from the short video; behind: the long video for 60 frames.
    let project = project_of(vec![
        vec![(0, video(SHORT_SILENT_VIDEO, 60, 120, 1))],
        vec![(0, long_video(0, 60, 1))],
    ]);
    assert_eq!(
        summary(&plan(&project)),
        vec![
            (0, 30, vec![Seen::Video(60, 90, 1)]),
            (30, 60, vec![Seen::Video(30, 60, 1)])
        ]
    );
}

#[test]
fn the_frames_missing_a_picture_are_counted_and_other_frames_with_nothing_to_show_are_not() {
    let nothing_missing = project_of(vec![vec![(20, long_video(0, 30, 1))], vec![(0, text(10))]]);
    assert_eq!(
        plan(&nothing_missing).frames_missing_a_picture,
        0,
        "gaps and text over black are not missing"
    );

    // 30 pictures and then 30 frames without; and at 4x, ten pictures and fifteen frames without.
    let short = project_of(vec![vec![
        (0, video(SHORT_SILENT_VIDEO, 60, 120, 1)),
        (0, video(SHORT_SILENT_VIDEO, 50, 150, 4)),
    ]]);
    assert_eq!(plan(&short).frames_missing_a_picture, 30 + 15);

    // A file that cannot be used has no pictures: every frame of its clip is missing one.
    let unusable = plan_pictures(&project_of(vec![vec![(0, long_video(0, 60, 2))]]), |_| false);
    assert_eq!(unusable.frames_missing_a_picture, 30);
}

#[test]
fn a_video_that_lacks_pictures_counts_as_missing_even_where_another_video_shows_in_its_place() {
    let project = project_of(vec![
        vec![(0, video(SHORT_SILENT_VIDEO, 60, 120, 1))],
        vec![(0, long_video(0, 60, 1))],
    ]);
    assert_eq!(plan(&project).frames_missing_a_picture, 30);
}

#[test]
fn a_video_that_lacks_pictures_behind_one_that_hides_it_is_not_missed() {
    let project = project_of(vec![
        vec![(0, long_video(0, 60, 1))],
        vec![(0, video(SHORT_SILENT_VIDEO, 60, 120, 1))],
    ]);
    assert_eq!(
        plan(&project).frames_missing_a_picture,
        0,
        "the long video covers the picture, so it makes no difference what is behind it"
    );
}

#[test]
fn a_clip_starting_after_the_end_of_its_file_has_no_picture_at_all() {
    let project = project_of(vec![vec![(0, video(SHORT_SILENT_VIDEO, 95, 120, 1))]]);
    assert_eq!(summary(&plan(&project)), vec![(0, 25, vec![])]);
}

#[test]
fn a_file_that_cannot_be_used_shows_nothing_wherever_it_is_used() {
    let project = project_of(vec![vec![
        (0, long_video(0, 60, 1)),
        (0, video(SHORT_SILENT_VIDEO, 0, 30, 1)),
    ]]);
    let plan = plan_pictures(&project, |source| source != LONG_VIDEO);
    assert_eq!(
        summary(&plan),
        vec![(0, 60, vec![]), (60, 90, vec![Seen::Video(0, 30, 1)])]
    );
}

// --- checked against the plain answer ---

/// What is seen at `timeline_frame`, worked out the plain way: look at each track from the front,
/// and see what its clip at that frame is. Gives the layers (the clip, the source frame of a
/// video, where it is on the picture), the one at the back first, and whether some video that is
/// in front of what is seen has no picture to show.
///
/// A video that fills the picture, is not turned and is not moved about by keyframes hides everything behind it, and a
/// video that has no picture to show is not there.
fn plainly_seen(project: &Project, timeline_frame: u64) -> (Vec<(ClipId, Option<u64>, Placement)>, bool) {
    let mut front_to_back = Vec::new();
    let mut a_picture_is_missing = false;
    for track in project.tracks() {
        let Some(placement) = track.placement_covering(frame(timeline_frame)) else {
            continue;
        };
        let location = placement.id;
        match placement.clip {
            Clip::Text(text) => front_to_back.push((
                location,
                None,
                plain_place(project, location, text.object.placement(), timeline_frame),
            )),
            // (What a generator makes is not known to the plan, so it hides nothing.)
            Clip::Generator(generator) => front_to_back.push((
                location,
                None,
                plain_place(project, location, generator.placement(), timeline_frame),
            )),
            Clip::Image(image) => {
                let shown_size = project
                    .source(image.source())
                    .unwrap()
                    .properties
                    .display_size_in_pixels()
                    .unwrap();
                let place = image
                    .placement()
                    .unwrap_or_else(|| fitted_placement(shown_size, project.picture_size()));
                front_to_back.push((location, None, plain_place(project, location, place, timeline_frame)));
            }
            Clip::Video(media) => {
                let source_frame = placement.source_frame_at(frame(timeline_frame)).unwrap().value();
                let file_frames = project.frame_count_of_source(media.source()).unwrap();
                if source_frame >= media.end_source_frame().value() || source_frame >= file_frames {
                    a_picture_is_missing = true;
                    continue;
                }
                let shown_size = project
                    .source(media.source())
                    .unwrap()
                    .properties
                    .video()
                    .map(|video| (video.display_width_in_pixels, video.display_height_in_pixels))
                    .unwrap();
                let place = media
                    .placement()
                    .unwrap_or_else(|| fitted_placement(shown_size, project.picture_size()));
                front_to_back.push((
                    location,
                    Some(source_frame),
                    plain_place(project, location, place, timeline_frame),
                ));
                if place.rotation.is_zero()
                    && place.area == CanvasArea::WHOLE_PICTURE
                    && !PROPERTIES_OF_WHERE_A_LAYER_IS
                        .into_iter()
                        .any(|property| project.is_animated(AnimationTarget::Clip(location), property))
                {
                    break;
                }
            }
            // (An adjustment clip has no picture of its own: it is a group of what is behind it, which these projects do not have.)
            Clip::Audio(_) | Clip::Crate(_) | Clip::Adjustment(_) => {}
        }
    }
    front_to_back.reverse();
    (front_to_back, a_picture_is_missing)
}

/// Where the clip `clip` is on the picture at `timeline_frame`, worked out the plain way: where it says it is, with what
/// each of the properties of where it is that has keyframes says (the project knows) put in, one after another.
fn plain_place(project: &Project, clip: ClipId, says: Placement, timeline_frame: u64) -> Placement {
    let target = AnimationTarget::Clip(clip);
    PROPERTIES_OF_WHERE_A_LAYER_IS
        .into_iter()
        .filter(|property| project.is_animated(target, *property))
        .fold(says, |place, property| {
            let value = project
                .property_value_at(target, property, frame(timeline_frame))
                .expect("a property that is animated has a value");
            with_placement_value(place, property, &value)
        })
}

#[test]
fn the_plan_shows_at_every_frame_exactly_what_looking_at_the_tracks_one_frame_at_a_time_shows() {
    check_that_the_plan_shows_what_looking_at_one_frame_at_a_time_shows(0, random_project_with_places);
}

#[test]
fn the_plan_shows_at_every_frame_exactly_what_looking_at_the_tracks_one_frame_at_a_time_shows_when_keyframes_move_things_about()
 {
    check_that_the_plan_shows_what_looking_at_one_frame_at_a_time_shows(7000, random_project_with_places_and_keyframes);
}

/// Checks the plans of 300 projects made at random, whose seeds begin after `first_seed_after`, against what looking at
/// each frame of the tracks shows.
fn check_that_the_plan_shows_what_looking_at_one_frame_at_a_time_shows(
    first_seed_after: u64,
    make_project: fn(&mut DeterministicRandomNumbers) -> Project,
) {
    for seed in 1..=300u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed + first_seed_after);
        let project = make_project(&mut random);
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

        let mut frames_missing_a_picture = 0;
        for timeline_frame in 0..plan.total_frame_count {
            let section = plan.section_at(frame(timeline_frame)).unwrap();
            let (expected_layers, a_picture_is_missing) = plainly_seen(&project, timeline_frame);
            frames_missing_a_picture += u64::from(a_picture_is_missing);
            let seen_layers: Vec<(ClipId, Option<u64>, Placement)> = section
                .layers
                .iter()
                .map(|layer| {
                    (
                        layer.clip,
                        layer
                            .video()
                            .map(|video| section.source_frame_at(video, frame(timeline_frame)).unwrap()),
                        layer.placement_at(frame(timeline_frame)),
                    )
                })
                .collect();
            assert_eq!(seen_layers, expected_layers, "frame {timeline_frame}, {}", describe());
        }
        assert_eq!(
            plan.frames_missing_a_picture,
            frames_missing_a_picture,
            "the count of frames missing a picture: {}",
            describe()
        );
    }
}

#[test]
fn the_back_most_layer_is_the_only_one_that_can_cover_the_picture() {
    for seed in 1..=300u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed + 3000);
        let plan = plan(&random_project_with_places(&mut random));
        for section in &plan.sections {
            for layer in section.layers.iter().skip(1) {
                assert!(
                    !layer.covers_the_picture(),
                    "seed {seed}: {layer:?} covers the picture, so what is behind it should be left out of {section:?}"
                );
            }
        }
    }
}

#[test]
fn every_picture_a_section_asks_a_file_for_is_there_and_the_range_it_asks_for_gives_exactly_that_many() {
    for seed in 1..=300u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed + 1000);
        let project = random_project_with_places(&mut random);
        for section in plan(&project).sections {
            for video in section.layers.iter().filter_map(Layer::video) {
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
}

/// Whether the layers of `after` are those of `before` carrying on, so that the two sections are
/// one section that was cut for no reason.
fn could_have_been_one(before: &PictureSection, after: &PictureSection) -> bool {
    before.layers.len() == after.layers.len()
        && before.layers.iter().zip(&after.layers).all(|(first, second)| {
            first.clip == second.clip
                && first.placement == second.placement
                && match (&first.content, &second.content) {
                    (LayerContent::Video(first_video), LayerContent::Video(second_video)) => {
                        first_video.speed == second_video.speed
                            && second_video.first_source_frame
                                == first_video.first_source_frame
                                    + before.timeline_frame_count() * u64::from(first_video.speed.value())
                    }
                    (LayerContent::Text(first_text), LayerContent::Text(second_text)) => first_text == second_text,
                    (LayerContent::Image { source: first_source }, LayerContent::Image { source: second_source }) => {
                        first_source == second_source
                    }
                    _ => false,
                }
        })
}

#[test]
fn nothing_is_split_that_does_not_need_to_be() {
    for seed in 1..=300u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed + 5000);
        let plan = plan(&random_project_with_places(&mut random));
        for pair in plan.sections.windows(2) {
            let (before, after) = (&pair[0], &pair[1]);
            assert!(
                !could_have_been_one(before, after),
                "seed {seed}: {before:?} and {after:?} should be one section"
            );
        }
    }
}

#[test]
fn clip_numbers_are_the_clips_own() {
    let project = project_of(vec![vec![(0, long_video(0, 10, 1))]]);
    let section = &plan(&project).sections[0];
    assert_eq!(section.layers[0].clip, ClipId::new(TrackId::new(0), 0));
    assert_eq!(section.layers[0].video().unwrap().source, LONG_VIDEO);
    assert_eq!(LONG_VIDEO_FRAME_COUNT, 300);
}

#[test]
fn the_front_most_video_is_the_last_of_the_video_layers_and_there_is_none_without_a_video() {
    let project = project_of(vec![
        vec![(0, text(40))],
        vec![(0, tall_video(0, 40))],
        vec![(0, long_video(0, 40, 2))],
    ]);
    let planned = plan(&project);
    let (layer, video) = planned.sections[0].front_video().unwrap();
    assert_eq!(video.source, TALL_VIDEO);
    assert_eq!(layer.track, TrackId::new(1));
    assert_eq!(planned.sections[0].layers.len(), 3);
    assert_eq!(planned.sections[0].video_layers().count(), 2);

    let only_text = plan(&project_of(vec![vec![(0, text(10))]]));
    assert!(only_text.sections[0].front_video().is_none());
}

// --- keyframe crates ---

#[test]
fn a_keyframe_crate_is_not_in_the_picture_and_does_not_cut_a_section_of_it_in_two() {
    let without = project_of(vec![vec![], vec![(0, long_video(0, 100, 1))]]);
    let mut with = without.clone();
    let in_front = with.tracks()[0].id();
    // (A crate has a rack in it, which animates the loudness of its own track here, so that nothing in the picture or the sound is changed.)
    crate::test_support::make_a_crate(
        &mut with,
        in_front,
        30,
        69,
        AnimationTarget::Track(in_front),
        PropertyId::Gain,
    );
    assert_eq!(
        with.tracks()[0].entries().len(),
        1,
        "the crate is on the track in front"
    );

    assert_eq!(
        plan_pictures(&with, |_| true),
        plan_pictures(&without, |_| true),
        "the picture is made of the video alone, in one piece"
    );
}
