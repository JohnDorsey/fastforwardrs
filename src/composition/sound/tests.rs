// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::composition::test_projects::{
    LONG_VIDEO, SHORT_SILENT_VIDEO, SOUND_FILE, media_clip, project_of, random_project,
};
use crate::project::EffectInstanceId;
use crate::project::tracks::{Clip, ClipId, Gain, TextClip};
use crate::test_support::DeterministicRandomNumbers;

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn speed(factor: u32) -> SpeedFactor {
    SpeedFactor::new(factor).unwrap()
}

fn long_video(first: u64, end: u64, factor: u32) -> Clip {
    Clip::Video(media_clip(LONG_VIDEO, first, end, factor))
}

fn silent_video(first: u64, end: u64) -> Clip {
    Clip::Video(media_clip(SHORT_SILENT_VIDEO, first, end, 1))
}

fn sound_file(first: u64, end: u64, factor: u32) -> Clip {
    Clip::Audio(media_clip(SOUND_FILE, first, end, factor))
}

fn text(duration: u64) -> Clip {
    Clip::Text(TextClip::new(duration, EffectInstanceId::FIRST).unwrap())
}

fn plan(project: &Project) -> SoundPlan {
    plan_sound(project, frame(0), |_| true)
}

/// An item in short: `None` for silence, or for a clip its source file, its source range and its speed; and how long it lasts.
type ItemSummary = (u64, Option<(u32, u64, u64, u32)>);

fn items(strip: &SoundStrip) -> Vec<ItemSummary> {
    strip
        .items
        .iter()
        .map(|item| match item {
            SoundItem::Silence { frame_count } => (*frame_count, None),
            SoundItem::Clip(clip) => (
                clip.timeline_frame_count,
                Some((
                    clip.source.value(),
                    clip.first_source_frame,
                    clip.end_source_frame,
                    clip.speed.value(),
                )),
            ),
        })
        .collect()
}

fn first_clip(strip: &SoundStrip) -> SoundClip {
    strip
        .items
        .iter()
        .find_map(|item| match item {
            SoundItem::Clip(clip) => Some(clip.clone()),
            SoundItem::Silence { .. } => None,
        })
        .expect("the strip has a clip")
}

/// The number the sound of `clip` is multiplied by, for a clip that is as loud all the way through.
fn constant_loudness_factor(clip: &SoundClip) -> f64 {
    match clip.loudness {
        Loudness::Constant(factor) => factor,
        Loudness::Changing(_) => panic!("the clip is not as loud all the way through: {clip:?}"),
    }
}

#[test]
fn a_video_with_sound_makes_one_strip_with_one_clip_at_its_normal_loudness() {
    let plan = plan(&project_of(vec![vec![(0, long_video(0, 100, 1))]]));
    assert_eq!(plan.strips.len(), 1);
    assert_eq!(items(&plan.strips[0]), vec![(100, Some((1, 0, 100, 1)))]);
    assert_eq!(
        constant_loudness_factor(&first_clip(&plan.strips[0])),
        1.0,
        "exactly 1, so that nothing needs to be done to the sound"
    );
    assert_eq!(plan.total_frame_count, 100);
}

#[test]
fn a_video_without_sound_and_text_and_an_empty_track_make_no_strip() {
    let plan = plan(&project_of(vec![
        vec![(0, silent_video(0, 50))],
        vec![(0, text(40))],
        vec![],
    ]));
    assert!(plan.strips.is_empty());
    assert_eq!(
        plan.total_frame_count, 50,
        "the plan is as long as the timeline all the same"
    );
}

#[test]
fn empty_time_between_sounds_is_silence_and_so_is_the_time_before_the_first_but_there_is_none_at_the_end() {
    // Sound A is at frames 20..80. Text is at 90..95 and is not heard. Sound B touches the text:
    // 95..125. Then a video without sound at 140..170, which is not heard either.
    let plan = plan(&project_of(vec![vec![
        (20, sound_file(0, 60, 1)),
        (10, text(5)),
        (0, sound_file(60, 90, 1)),
        (15, silent_video(0, 30)),
    ]]));
    assert_eq!(
        items(&plan.strips[0]),
        vec![
            (20, None),
            (60, Some((3, 0, 60, 1))),
            (15, None),
            (30, Some((3, 60, 90, 1))),
        ]
    );
    assert_eq!(plan.strips[0].timeline_frame_count(), 125);
    assert_eq!(plan.total_frame_count, 170);
}

#[test]
fn the_speed_of_a_clip_is_its_own_times_its_tracks() {
    let mut project = project_of(vec![vec![(0, long_video(0, 120, 3))]]);
    let track = project.tracks()[0].id();
    project.set_track_speed(track, speed(2)).unwrap();
    let plan = plan(&project);
    assert_eq!(items(&plan.strips[0]), vec![(20, Some((1, 0, 120, 6)))]);
}

// --- what is silent ---

#[test]
fn a_muted_clip_is_silence_for_as_long_as_it_lasts() {
    let mut project = project_of(vec![vec![
        (0, long_video(0, 50, 1)),
        (0, long_video(50, 100, 1)),
        (0, long_video(100, 150, 1)),
    ]]);
    let middle = project.tracks()[0].entries()[1].id();
    project.set_clip_muted(middle, true).unwrap();
    assert_eq!(
        items(&plan(&project).strips[0]),
        vec![(50, Some((1, 0, 50, 1))), (50, None), (50, Some((1, 100, 150, 1)))]
    );
}

#[test]
fn a_track_that_is_muted_has_no_sound_at_all() {
    let mut project = project_of(vec![vec![(0, long_video(0, 50, 1))], vec![(0, sound_file(0, 50, 1))]]);
    let front = project.tracks()[0].id();
    project.set_track_muted(front, true).unwrap();
    let plan = plan(&project);
    assert_eq!(plan.strips.len(), 1);
    assert_eq!(plan.strips[0].track, project.tracks()[1].id());
}

#[test]
fn the_gain_of_the_clip_and_of_its_track_are_both_applied() {
    let mut project = project_of(vec![vec![(0, sound_file(0, 50, 1))]]);
    let location = project.tracks()[0].entries()[0].id();
    let track = project.tracks()[0].id();
    project.set_clip_gain(location, Gain::from_decibels(6.0)).unwrap();
    project.set_track_gain(track, Gain::from_decibels(-6.0)).unwrap();
    let factor = constant_loudness_factor(&first_clip(&plan(&project).strips[0]));
    assert!((factor - 1.0).abs() < 1e-12, "+6 and -6 decibels cancel: {factor}");

    project.set_track_gain(track, Gain::from_decibels(14.0)).unwrap();
    let factor = constant_loudness_factor(&first_clip(&plan(&project).strips[0]));
    assert!((factor - 10.0).abs() < 1e-9, "+20 decibels is ten times: {factor}");
}

#[test]
fn a_file_that_cannot_be_used_is_silent_and_the_others_are_not() {
    let project = project_of(vec![vec![(0, long_video(0, 30, 1)), (0, sound_file(0, 30, 1))]]);
    let plan = plan_sound(&project, frame(0), |source| source != LONG_VIDEO);
    assert_eq!(items(&plan.strips[0]), vec![(30, None), (30, Some((3, 0, 30, 1)))]);
}

// --- strips in order ---

#[test]
fn strips_are_in_the_order_of_the_tracks_the_one_in_front_first() {
    let project = project_of(vec![
        vec![(0, sound_file(0, 10, 1))],
        vec![(0, long_video(0, 10, 1))],
        vec![(0, sound_file(0, 10, 1))],
    ]);
    let tracks: Vec<TrackId> = plan(&project).strips.iter().map(|strip| strip.track).collect();
    assert_eq!(tracks, vec![TrackId::new(0), TrackId::new(1), TrackId::new(2)]);
}

// --- from some frame on ---

#[test]
fn a_plan_from_a_frame_starts_there_and_is_that_much_shorter() {
    let project = project_of(vec![vec![(20, sound_file(0, 60, 1)), (10, long_video(0, 40, 1))]]);
    // Sound at 20..80, video sound at 90..130.
    let from_nothing = plan(&project);
    let from_the_start_of_the_second = plan_sound(&project, frame(90), |_| true);
    assert_eq!(
        from_the_start_of_the_second.total_frame_count,
        from_nothing.total_frame_count - 90
    );
    assert_eq!(
        items(&from_the_start_of_the_second.strips[0]),
        vec![(40, Some((1, 0, 40, 1)))]
    );
    let from_a_gap = plan_sound(&project, frame(85), |_| true);
    assert_eq!(items(&from_a_gap.strips[0]), vec![(5, None), (40, Some((1, 0, 40, 1)))]);
}

#[test]
fn a_plan_from_the_middle_of_a_clip_starts_the_clip_at_the_source_frame_that_is_there() {
    // 300 source frames at 3x last 100 frames, from 10 to 110. At frame 40 the clip is 30 frames in,
    // which shows source frame 100 + 30 * 3 = 190.
    let project = project_of(vec![vec![(10, long_video(100, 400, 3))]]);
    let plan = plan_sound(&project, frame(40), |_| true);
    assert_eq!(items(&plan.strips[0]), vec![(70, Some((1, 190, 400, 3)))]);
    assert_eq!(plan.total_frame_count, 70);
}

#[test]
fn a_plan_from_after_everything_is_empty() {
    let project = project_of(vec![vec![(0, sound_file(0, 50, 1))]]);
    for from in [50, 51, 5000] {
        let plan = plan_sound(&project, frame(from), |_| true);
        assert!(plan.strips.is_empty(), "from {from}");
        assert_eq!(plan.total_frame_count, 0, "from {from}");
    }
}

#[test]
fn a_clip_that_has_ended_before_the_start_is_not_part_of_the_plan() {
    let project = project_of(vec![vec![(0, sound_file(0, 30, 1)), (0, long_video(0, 30, 1))]]);
    let plan = plan_sound(&project, frame(30), |_| true);
    assert_eq!(items(&plan.strips[0]), vec![(30, Some((1, 0, 30, 1)))]);
}

// --- checked against the plain answer ---

/// What track `strip` plays at `offset` frames from its start: the clip and its source frame.
fn heard_at(strip: &SoundStrip, offset: u64) -> Option<(ClipId, u64)> {
    let mut item_start = 0;
    for item in &strip.items {
        let length = item.timeline_frame_count();
        if offset < item_start + length {
            return match item {
                SoundItem::Silence { .. } => None,
                SoundItem::Clip(clip) => Some((
                    clip.clip,
                    clip.first_source_frame + (offset - item_start) * u64::from(clip.speed.value()),
                )),
            };
        }
        item_start += length;
    }
    None
}

/// What track `track` plays at `timeline_frame`, worked out the plain way: look at its clip there.
fn plainly_heard(
    project: &Project,
    track: &crate::project::tracks::Track,
    timeline_frame: u64,
) -> Option<(ClipId, u64)> {
    let placement = track.placement_covering(frame(timeline_frame))?;
    let media = placement.clip.as_media()?;
    let has_sound = project.source(media.source())?.properties.has_sound();
    (has_sound && !track.is_muted() && !media.is_muted()).then(|| {
        (
            placement.id,
            placement.source_frame_at(frame(timeline_frame)).unwrap().value(),
        )
    })
}

#[test]
fn the_plan_has_at_every_frame_exactly_the_sound_that_looking_at_the_tracks_one_frame_at_a_time_gives() {
    for seed in 1..=300u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let project = random_project(&mut random);
        let from = random.next_below(project.total_frame_count() + 20);
        let plan = plan_sound(&project, frame(from), |_| true);
        let describe = || format!("seed {seed}, from {from}: tracks {:?}", project.tracks());

        assert_eq!(
            plan.total_frame_count,
            project.total_frame_count().saturating_sub(from),
            "{}",
            describe()
        );
        for strip in &plan.strips {
            assert!(strip.timeline_frame_count() <= plan.total_frame_count, "{}", describe());
            assert!(
                matches!(strip.items.last(), Some(SoundItem::Clip(_))),
                "a strip ends with sound, not silence: {}",
                describe()
            );
        }
        for track in project.tracks() {
            let strip = plan.strips.iter().find(|strip| strip.track == track.id());
            for offset in 0..plan.total_frame_count {
                let expected = plainly_heard(&project, track, from + offset);
                let heard = strip.and_then(|strip| heard_at(strip, offset));
                assert_eq!(
                    heard,
                    expected,
                    "{} at frame {} of {}: {}",
                    track.title(),
                    from + offset,
                    from,
                    describe()
                );
            }
            if strip.is_none() {
                assert!(
                    (0..plan.total_frame_count).all(|offset| plainly_heard(&project, track, from + offset).is_none()),
                    "a track with sound has a strip: {}",
                    describe()
                );
            }
        }
    }
}

#[test]
fn clips_keep_the_names_they_have_on_their_track() {
    let project = project_of(vec![vec![(0, long_video(0, 10, 1))]]);
    let clip = first_clip(&plan(&project).strips[0]);
    assert_eq!(clip.clip, ClipId::new(TrackId::new(0), 0));
}

// --- keyframe crates ---

#[test]
fn a_keyframe_crate_makes_no_sound_and_has_no_strip_of_its_own() {
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
        plan_sound(&with, frame(0), |_| true),
        plan_sound(&without, frame(0), |_| true),
        "the sound is the video's alone"
    );
}
