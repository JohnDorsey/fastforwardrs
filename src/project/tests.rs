use std::path::PathBuf;

use super::*;
use crate::project::tracks::{ClipId, ClipKind};
use crate::project::{SourceAudioProperties, SourceImageProperties, SourceVideoProperties};

pub(super) fn frame_rate(frames_per_second: u32) -> FrameRate {
    FrameRate::from_whole_frames_per_second(frames_per_second).unwrap()
}

pub(super) fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn speed(factor: u32) -> SpeedFactor {
    SpeedFactor::new(factor).unwrap()
}

fn link(name: &str) -> SourceMediaLink {
    SourceMediaLink::from_absolute_path(PathBuf::from(format!("/files/{name}")))
}

pub(super) fn video_source(identifier: u32, frames_per_second: u32, total_frame_count: u64) -> SourceMedia {
    SourceMedia::video(
        SourceMediaIdentifier::new(identifier),
        link(&format!("{identifier}.mp4")),
        SourceVideoProperties {
            frame_rate: frame_rate(frames_per_second),
            total_frame_count,
            display_width_in_pixels: 1280,
            display_height_in_pixels: 720,
            has_audio: true,
        },
    )
}

pub(super) fn audio_source(identifier: u32, seconds: u64) -> SourceMedia {
    SourceMedia::audio(
        SourceMediaIdentifier::new(identifier),
        link(&format!("{identifier}.mp3")),
        SourceAudioProperties {
            duration_in_microseconds: seconds * 1_000_000,
        },
    )
}

pub(super) fn picture_source(identifier: u32, width: u32, height: u32) -> SourceMedia {
    SourceMedia::image(
        SourceMediaIdentifier::new(identifier),
        link(&format!("{identifier}.png")),
        SourceImageProperties {
            display_width_in_pixels: width,
            display_height_in_pixels: height,
        },
    )
}

fn video_clip(identifier: u32, first: u64, end: u64) -> Clip {
    Clip::Video(
        MediaClip::new(
            SourceMediaIdentifier::new(identifier),
            SourceFrameIndex::new(first),
            SourceFrameIndex::new(end),
            SpeedFactor::NORMAL,
        )
        .unwrap(),
    )
}

fn audio_clip(identifier: u32, first: u64, end: u64) -> Clip {
    Clip::Audio(
        MediaClip::new(
            SourceMediaIdentifier::new(identifier),
            SourceFrameIndex::new(first),
            SourceFrameIndex::new(end),
            SpeedFactor::NORMAL,
        )
        .unwrap(),
    )
}

fn track_with(id: u32, clips: Vec<(u64, Clip)>) -> Track {
    Track::new(TrackId::new(id), format!("track {id}")).with_clips(clips)
}

/// A project with a 30 fps video of `frames` frames imported, so it has one track with one clip.
pub(super) fn project_with_a_video(frames: u64) -> Project {
    let mut project = Project::empty();
    project.import_source(video_source(1, 30, frames), frame(0)).unwrap();
    project
}

pub(super) fn first_clip_location(project: &Project, track_position: usize) -> ClipId {
    let track = &project.tracks()[track_position];
    track.entries()[0].id()
}

// --- a new project ---

#[test]
fn a_new_project_has_one_empty_track_no_files_and_follows_every_global_setting() {
    let project = Project::empty();
    assert_eq!(project.tracks().len(), 1);
    assert_eq!(project.tracks()[0].title(), "track 0");
    assert!(project.tracks()[0].is_empty());
    assert!(project.sources().is_empty());
    assert!(project.has_no_clips());
    assert_eq!(project.total_frame_count(), 0);
    assert_eq!(project.frame_rate(), FRAME_RATE_WITHOUT_A_VIDEO);
    assert_eq!(project.picture_size(), PICTURE_SIZE_WITHOUT_A_VIDEO);
    assert_eq!(project.settings(), ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS);
    assert_eq!(project.speed_indicator_layout(), SpeedIndicatorLayout::DEFAULT);
    assert_eq!(Project::default(), project);
}

#[test]
fn without_a_video_the_project_runs_at_30_frames_per_second_at_full_hd() {
    assert_eq!(FRAME_RATE_WITHOUT_A_VIDEO, frame_rate(30));
    assert_eq!(PICTURE_SIZE_WITHOUT_A_VIDEO, (1920, 1080));
}

#[test]
fn a_track_added_goes_in_front_of_all_the_others_with_the_next_title() {
    let mut project = Project::empty();
    let second = project.add_track_in_front();
    let third = project.add_track_in_front();
    let titles: Vec<&str> = project.tracks().iter().map(Track::title).collect();
    assert_eq!(titles, vec!["track 2", "track 1", "track 0"], "the newest is in front");
    assert_eq!(project.track_position(third), Some(0));
    assert_eq!(project.track_position(second), Some(1));
    let mut identifiers: Vec<_> = project.tracks().iter().map(Track::id).collect();
    identifiers.dedup();
    assert_eq!(identifiers.len(), 3, "each track has a name of its own");
}

// --- importing ---

#[test]
fn the_first_video_goes_on_the_empty_track_that_is_there_and_sets_the_frame_rate_and_picture() {
    let mut project = Project::empty();
    let outcome = project
        .import_source(
            SourceMedia::video(
                SourceMediaIdentifier::new(1),
                link("a.mp4"),
                SourceVideoProperties {
                    frame_rate: FrameRate::new(24_000, 1001).unwrap(),
                    total_frame_count: 900,
                    display_width_in_pixels: 1080,
                    display_height_in_pixels: 1920,
                    has_audio: false,
                },
            ),
            frame(0),
        )
        .unwrap();

    assert!(!outcome.created_track, "the empty track was used");
    assert_eq!(project.tracks().len(), 1);
    assert_eq!(project.frame_rate(), FrameRate::new(24_000, 1001).unwrap());
    assert_eq!(project.picture_size(), (1080, 1920));
    let placement = project.placement_of(outcome.clip.unwrap());
    let placement = placement.unwrap();
    assert_eq!(placement.clip.kind(), ClipKind::Video);
    assert_eq!(
        (
            placement.first_timeline_frame.value(),
            placement.end_timeline_frame.value()
        ),
        (0, 900)
    );
    assert_eq!(project.total_frame_count(), 900);
    assert!(!project.has_no_clips());
}

#[test]
fn a_later_file_goes_on_the_front_most_empty_track_or_on_a_new_one_in_front_starting_at_the_playhead() {
    let mut project = project_with_a_video(300);

    // No track is empty: a new one is made in front.
    let music = project.import_source(audio_source(2, 4), frame(45)).unwrap();
    assert!(music.created_track);
    assert_eq!(project.tracks().len(), 2);
    assert_eq!(project.track_position(music.track), Some(0), "in front of the video");
    let placement = project.placement_of(music.clip.unwrap()).unwrap();
    assert_eq!(placement.clip.kind(), ClipKind::Audio);
    assert_eq!(
        (
            placement.first_timeline_frame.value(),
            placement.end_timeline_frame.value()
        ),
        (45, 45 + 120),
        "4 seconds at 30 fps, from the playhead"
    );

    // An empty track that is there is used, the front-most of them.
    let empty_in_front = project.add_track_in_front();
    let empty_behind = project.add_track_in_front();
    let _ = empty_behind;
    let outcome = project.import_source(audio_source(3, 1), frame(0)).unwrap();
    assert!(!outcome.created_track);
    assert_eq!(outcome.track, project.tracks()[0].id(), "the front-most empty one");
    assert_ne!(outcome.track, empty_in_front, "(the one made first is further back)");
}

#[test]
fn a_video_of_another_frame_rate_is_imported_and_lasts_as_many_frames_of_the_project_as_it_lasts_in_time() {
    let mut project = project_with_a_video(300); // 30 frames per second, ten seconds
    let outcome = project.import_source(video_source(2, 25, 100), frame(0)).unwrap(); // four seconds
    let placement = project.placement_of(outcome.clip.unwrap()).unwrap();
    assert_eq!(
        placement.timeline_frame_count(),
        120,
        "four seconds at 30 frames per second"
    );
    assert_eq!(
        project.frame_rate(),
        frame_rate(30),
        "the project goes on running at its own rate"
    );
    assert_eq!(project.frame_count_of_source(SourceMediaIdentifier::new(2)), Some(120));
    assert_eq!(project.frame_count_of_source(SourceMediaIdentifier::new(1)), Some(300));
}

#[test]
fn a_video_that_comes_to_no_frames_at_the_projects_rate_is_added_but_makes_no_clip() {
    // One picture of a video that runs at 50 frames per second is 1/50 of a second: less than half a
    // frame of a project at 24, which is no picture at all once the video is made to run at 24.
    let mut project = Project::empty();
    project.set_frame_rate(frame_rate(24));
    let outcome = project.import_source(video_source(1, 50, 1), frame(0)).unwrap();
    assert_eq!(outcome.clip, None);
    assert_eq!(project.sources().len(), 1);
}

#[test]
fn a_project_that_has_things_on_its_tracks_keeps_the_frame_rate_they_are_laid_out_at_when_its_first_video_comes() {
    let mut project = Project::empty();
    project.import_source(audio_source(1, 10), frame(0)).unwrap(); // 300 frames at the 30 of a project with no video
    assert_eq!(
        project.chosen_frame_rate(),
        None,
        "it follows its first video, when there is one"
    );

    let outcome = project.import_source(video_source(2, 25, 100), frame(0)).unwrap();

    assert_eq!(
        project.frame_rate(),
        frame_rate(30),
        "what is on the tracks is not changed in meaning"
    );
    assert_eq!(
        project.chosen_frame_rate(),
        Some(frame_rate(30)),
        "and it will not follow another video"
    );
    let sound = &project.tracks()[1].placements().next().unwrap();
    assert_eq!(sound.timeline_frame_count(), 300);
    let video = project.placement_of(outcome.clip.unwrap()).unwrap();
    assert_eq!(video.timeline_frame_count(), 120);
}

#[test]
fn a_frame_rate_that_was_chosen_is_kept_when_the_first_video_comes_and_the_video_is_made_to_run_at_it() {
    let mut project = Project::empty();
    project.set_frame_rate(frame_rate(24));
    let outcome = project.import_source(video_source(1, 30, 300), frame(0)).unwrap(); // ten seconds
    assert_eq!(project.frame_rate(), frame_rate(24));
    let placement = project.placement_of(outcome.clip.unwrap()).unwrap();
    assert_eq!(placement.timeline_frame_count(), 240);
    assert_eq!(
        project.picture_size(),
        (1280, 720),
        "the picture is still the first video's"
    );
}

#[test]
fn a_project_with_nothing_on_its_tracks_takes_the_frame_rate_of_its_first_video_whatever_it_is() {
    let mut project = Project::empty();
    project.add_track_in_front();
    assert!(project.import_source(video_source(1, 25, 100), frame(0)).is_ok());
    assert_eq!(project.frame_rate(), frame_rate(25));
}

#[test]
fn a_file_cannot_be_imported_under_a_name_that_is_in_use() {
    let mut project = project_with_a_video(100);
    let before = project.clone();
    assert_eq!(
        project.import_source(audio_source(1, 3), frame(0)),
        Err(ImportError::IdentifierInUse {
            identifier: SourceMediaIdentifier::new(1)
        })
    );
    assert_eq!(project, before);
}

#[test]
fn a_video_with_no_frames_is_added_to_the_project_but_makes_no_clip() {
    let mut project = Project::empty();
    let outcome = project.import_source(video_source(1, 30, 0), frame(0)).unwrap();
    assert_eq!(outcome.clip, None);
    assert_eq!(project.sources().len(), 1);
    assert!(project.has_no_clips());
}

#[test]
fn the_next_identifier_follows_the_largest_one_in_use() {
    assert_eq!(Project::empty().next_source_identifier(), SourceMediaIdentifier::new(1));
    let project = Project::from_parts(
        vec![video_source(3, 30, 10), audio_source(7, 1)],
        vec![],
        SpeedIndicatorLayout::DEFAULT,
    )
    .unwrap();
    assert_eq!(project.next_source_identifier(), SourceMediaIdentifier::new(8));
}

#[test]
fn the_length_of_the_project_is_the_end_of_the_last_clip_on_any_track() {
    let mut project = project_with_a_video(300);
    assert_eq!(project.total_frame_count(), 300);
    project.import_source(audio_source(2, 20), frame(100)).unwrap();
    assert_eq!(
        project.total_frame_count(),
        100 + 600,
        "the sound goes on after the video"
    );
}

// --- assembling from parts ---

#[test]
fn parts_that_agree_make_a_project_and_its_next_track_has_a_name_nothing_else_has() {
    let tracks = vec![
        track_with(5, vec![(0, video_clip(1, 0, 100))]),
        track_with(2, vec![(10, audio_clip(2, 0, 50))]),
    ];
    let mut project = Project::from_parts(
        vec![video_source(1, 30, 100), audio_source(2, 5)],
        tracks,
        SpeedIndicatorLayout::DEFAULT,
    )
    .unwrap();
    assert_eq!(project.tracks().len(), 2);
    let new_track = project.add_track_in_front();
    assert!(project.tracks()[1..].iter().all(|track| track.id() != new_track));
    assert_eq!(new_track, TrackId::new(6));
}

#[test]
fn parts_that_contradict_each_other_are_refused() {
    let sources = || vec![video_source(1, 30, 100), audio_source(2, 5)];

    assert_eq!(
        Project::from_parts(
            vec![video_source(1, 30, 100), video_source(1, 30, 50)],
            vec![],
            SpeedIndicatorLayout::DEFAULT
        ),
        Err(InconsistentProject::DuplicateSource {
            identifier: SourceMediaIdentifier::new(1)
        })
    );
    assert_eq!(
        Project::from_parts(
            sources(),
            vec![track_with(1, vec![]), track_with(1, vec![])],
            SpeedIndicatorLayout::DEFAULT
        ),
        Err(InconsistentProject::DuplicateTrack { track: TrackId::new(1) })
    );
    assert_eq!(
        Project::from_parts(
            sources(),
            vec![track_with(1, vec![(0, video_clip(9, 0, 10))])],
            SpeedIndicatorLayout::DEFAULT
        ),
        Err(InconsistentProject::ClipUsesUnknownSource {
            track: TrackId::new(1),
            identifier: SourceMediaIdentifier::new(9)
        })
    );
    assert_eq!(
        Project::from_parts(
            sources(),
            vec![track_with(1, vec![(0, video_clip(2, 0, 10))])],
            SpeedIndicatorLayout::DEFAULT
        ),
        Err(InconsistentProject::ClipUsesSourceOfTheWrongKind {
            track: TrackId::new(1),
            identifier: SourceMediaIdentifier::new(2)
        }),
        "a video clip of a sound file"
    );
    assert_eq!(
        Project::from_parts(
            sources(),
            vec![track_with(1, vec![(0, audio_clip(1, 0, 10))])],
            SpeedIndicatorLayout::DEFAULT
        ),
        Err(InconsistentProject::ClipUsesSourceOfTheWrongKind {
            track: TrackId::new(1),
            identifier: SourceMediaIdentifier::new(1)
        }),
        "a sound clip of a video"
    );
}

#[test]
fn parts_with_videos_of_different_frame_rates_make_a_project() {
    let project = Project::from_parts(
        vec![video_source(1, 30, 100), video_source(2, 25, 50)],
        vec![],
        SpeedIndicatorLayout::DEFAULT,
    )
    .unwrap();
    assert_eq!(
        project.frame_rate(),
        frame_rate(30),
        "the first video's, as nothing else was chosen"
    );
    let with_a_frame_rate = project.with_frame_rate(Some(frame_rate(24)));
    assert_eq!(with_a_frame_rate.frame_rate(), frame_rate(24));
    assert_eq!(with_a_frame_rate.chosen_frame_rate(), Some(frame_rate(24)));
}

// --- pointing a source at another file ---

#[test]
fn relinking_keeps_every_edit_and_replaces_the_link_and_properties() {
    let mut project = project_with_a_video(300);
    project.split_at(frame(100), None);
    let tracks_before = project.tracks().to_vec();

    let new_link = SourceMediaLink {
        absolute_path: PathBuf::from("/elsewhere/copy.mp4"),
        relative_path: Some(PathBuf::from("../elsewhere/copy.mp4")),
        included_as: None,
    };
    let SourceMediaProperties::Video(mut new_properties) = video_source(1, 30, 302).properties else {
        unreachable!()
    };
    new_properties.display_width_in_pixels = 640;
    let outcome = project
        .relink_source(
            SourceMediaIdentifier::new(1),
            new_link.clone(),
            SourceMediaProperties::Video(new_properties),
        )
        .unwrap();

    assert_eq!(
        outcome,
        RelinkOutcome {
            frame_count_before: 300,
            frame_count_after: 302
        }
    );
    assert_eq!(project.tracks(), tracks_before, "relinking must not touch the edits");
    let relinked = project.source(SourceMediaIdentifier::new(1)).unwrap();
    assert_eq!(relinked.link, new_link);
    assert_eq!(relinked.properties, SourceMediaProperties::Video(new_properties));
}

#[test]
fn relinking_to_a_video_with_a_different_frame_rate_is_fine_and_counts_its_frames_at_the_projects_rate() {
    let mut project = project_with_a_video(300);
    // A copy of the same ten seconds, at 25 frames per second, has 250 pictures, which are 300 frames
    // of the project: the edits line up as they did.
    let outcome = project
        .relink_source(
            SourceMediaIdentifier::new(1),
            link("other.mp4"),
            video_source(1, 25, 250).properties,
        )
        .unwrap();
    assert_eq!(
        outcome,
        RelinkOutcome {
            frame_count_before: 300,
            frame_count_after: 300
        }
    );
    assert_eq!(
        project.frame_rate(),
        frame_rate(30),
        "the project is not made to follow the new file"
    );
    assert_eq!(
        project.chosen_frame_rate(),
        Some(frame_rate(30)),
        "it followed the video it was edited on, and now keeps the rate it had"
    );
}

#[test]
fn relinking_to_a_video_of_the_same_frame_rate_leaves_the_project_following_its_first_video() {
    let mut project = project_with_a_video(300);
    project
        .relink_source(
            SourceMediaIdentifier::new(1),
            link("other.mp4"),
            video_source(1, 30, 280).properties,
        )
        .unwrap();
    assert_eq!(project.chosen_frame_rate(), None);
}

#[test]
fn relinking_to_a_different_kind_of_file_is_refused() {
    let mut project = project_with_a_video(300);
    project.import_source(audio_source(2, 5), frame(0)).unwrap();
    let before = project.clone();

    assert!(matches!(
        project.relink_source(
            SourceMediaIdentifier::new(1),
            link("other.mp3"),
            audio_source(1, 5).properties
        ),
        Err(RelinkError::DifferentKind { .. })
    ));
    assert!(matches!(
        project.relink_source(
            SourceMediaIdentifier::new(2),
            link("other.mp4"),
            video_source(2, 30, 5).properties
        ),
        Err(RelinkError::DifferentKind { .. })
    ));
    assert_eq!(
        project.relink_source(
            SourceMediaIdentifier::new(9),
            link("other.mp4"),
            video_source(9, 30, 5).properties
        ),
        Err(RelinkError::UnknownSource {
            identifier: SourceMediaIdentifier::new(9)
        })
    );
    assert_eq!(project, before);
}

#[test]
fn a_sound_file_can_be_relinked_to_another_one_of_a_different_length() {
    let mut project = Project::empty();
    project.import_source(audio_source(1, 10), frame(0)).unwrap();
    let outcome = project
        .relink_source(
            SourceMediaIdentifier::new(1),
            link("shorter.mp3"),
            audio_source(1, 4).properties,
        )
        .unwrap();
    assert_eq!(
        outcome,
        RelinkOutcome {
            frame_count_before: 300,
            frame_count_after: 120
        }
    );
    assert_eq!(project.frame_count_of_source(SourceMediaIdentifier::new(1)), Some(120));
    assert_eq!(project.total_frame_count(), 300, "the clips are as they were");
}

// --- editing through clip locations ---

#[test]
fn cutting_cuts_every_clip_at_the_frame_and_skips_the_tracks_with_nothing_there() {
    let mut project = project_with_a_video(300);
    project.import_source(audio_source(2, 4), frame(60)).unwrap(); // 60..180 on a track in front
    project.add_track_in_front(); // an empty track

    let cut = project.split_at(frame(100), None);
    assert_eq!(cut.len(), 2, "the video and the sound were both cut");
    assert!(
        project
            .tracks()
            .iter()
            .filter(|track| !track.is_empty())
            .all(|track| track.entries().len() == 2)
    );

    // Where only one clip is, only it is cut.
    let cut = project.split_at(frame(250), None);
    assert_eq!(cut.len(), 1);
}

#[test]
fn cutting_with_a_clip_named_cuts_only_that_clip_and_only_where_it_is() {
    let mut project = project_with_a_video(300);
    project.import_source(audio_source(2, 4), frame(60)).unwrap();
    let sound_track = project.tracks()[0].id();
    let video_track = project.tracks()[1].id();
    let sound = project.tracks()[0].entries()[0].id();

    assert!(
        project.split_at(frame(10), Some(&[sound])).is_empty(),
        "the sound is not at frame 10"
    );
    assert_eq!(project.track(video_track).unwrap().entries().len(), 1);

    let cut = project.split_at(frame(100), Some(&[sound]));
    assert_eq!(cut.len(), 1);
    assert_eq!(cut[0].0, sound_track);
    assert_eq!(project.track(sound_track).unwrap().entries().len(), 2);
    assert_eq!(
        project.track(video_track).unwrap().entries().len(),
        1,
        "the video was left alone"
    );
}

#[test]
fn changes_to_a_clip_or_a_track_go_to_the_one_named_and_a_name_that_is_not_there_is_an_error() {
    let mut project = project_with_a_video(300);
    let location = first_clip_location(&project, 0);
    let track = project.track_holding(location).unwrap().id();

    project.set_clip_speed(location, speed(5)).unwrap();
    project.set_clip_gain(location, Gain::from_decibels(-4.0)).unwrap();
    project.set_clip_muted(location, true).unwrap();
    project.set_track_title(track, "the big one").unwrap();
    project.set_track_gain(track, Gain::from_decibels(2.0)).unwrap();
    project.set_track_muted(track, true).unwrap();
    project.set_track_speed(track, speed(2)).unwrap();

    let track = project.track(track).unwrap();
    assert_eq!(track.title(), "the big one");
    assert_eq!(track.gain(), Gain::from_decibels(2.0));
    assert!(track.is_muted());
    assert_eq!(track.speed(), speed(2));
    let media = track.entries()[0].clip().as_media().unwrap();
    assert_eq!(media.speed(), speed(5));
    assert_eq!(media.gain(), Gain::from_decibels(-4.0));
    assert!(media.is_muted());
    assert_eq!(track.total_frame_count(), 30, "300 frames at 5x on a 2x track");

    // A clip that no track has is not found, whichever track it says it was made on.
    let nobody = ClipId::new(TrackId::new(99), 0);
    assert_eq!(
        project.set_clip_speed(nobody, speed(2)),
        Err(EditError::Clip(ClipEditError::UnknownClip { clip: nobody }))
    );
    assert_eq!(
        project.delete_clip(nobody),
        Err(EditError::Clip(ClipEditError::UnknownClip { clip: nobody }))
    );
    assert_eq!(
        project.set_track_muted(TrackId::new(99), true),
        Err(EditError::UnknownTrack {
            track: TrackId::new(99)
        })
    );
    let no_such_clip = ClipId::new(location.origin(), 99);
    assert_eq!(
        project.delete_clip(no_such_clip),
        Err(EditError::Clip(ClipEditError::UnknownClip { clip: no_such_clip }))
    );
}

#[test]
fn a_track_can_be_made_taller_or_shorter_and_one_that_is_not_there_is_an_error() {
    let mut project = project_with_a_video(300);
    let track = project.tracks()[0].id();
    assert_eq!(project.track(track).unwrap().height(), TrackHeight::DEFAULT);
    let others_before = project.clone();

    project.set_track_height(track, TrackHeight::from_points(60.0)).unwrap();
    assert_eq!(project.track(track).unwrap().height().points(), 60.0);
    project
        .set_track_height(track, TrackHeight::from_points(10_000.0))
        .unwrap();
    assert_eq!(
        project.track(track).unwrap().height().points(),
        TrackHeight::LARGEST_IN_POINTS,
        "as tall as a track can be"
    );
    // Nothing but the height changed.
    project.set_track_height(track, TrackHeight::DEFAULT).unwrap();
    assert_eq!(project, others_before);

    assert_eq!(
        project.set_track_height(TrackId::new(99), TrackHeight::DEFAULT),
        Err(EditError::UnknownTrack {
            track: TrackId::new(99)
        })
    );
}

#[test]
fn restoring_an_earlier_copy_brings_back_the_heights_of_the_tracks() {
    let mut project = project_with_a_video(300);
    let track = project.tracks()[0].id();
    let before = project.clone();
    project.set_track_height(track, TrackHeight::from_points(50.0)).unwrap();

    project.restore(before);

    assert_eq!(project.track(track).unwrap().height(), TrackHeight::DEFAULT);
}

#[test]
fn clips_on_different_tracks_can_be_moved_in_one_go() {
    let mut project = project_with_a_video(300);
    project.import_source(audio_source(2, 10), frame(0)).unwrap();
    let on_the_sound_track = first_clip_location(&project, 0);
    let on_the_video_track = first_clip_location(&project, 1);

    project
        .move_clips(&[
            along_its_track(&project, on_the_sound_track, 40),
            along_its_track(&project, on_the_video_track, 25),
        ])
        .unwrap();

    assert_eq!(
        project.placement_of(on_the_sound_track).unwrap().first_timeline_frame,
        frame(40)
    );
    assert_eq!(
        project.placement_of(on_the_video_track).unwrap().first_timeline_frame,
        frame(25)
    );
    assert_eq!(
        project.total_frame_count(),
        340,
        "the 10 second sound, which now starts at frame 40, ends last"
    );
}

#[test]
fn if_one_clip_cannot_be_moved_none_is_and_the_project_is_as_it_was() {
    let mut project = project_with_a_video(300);
    project.split_at(frame(100), None);
    project.import_source(audio_source(2, 10), frame(0)).unwrap();
    let on_the_sound_track = first_clip_location(&project, 0);
    let first_part_of_the_video = first_clip_location(&project, 1);
    let before = project.clone();

    // The sound can go where it is asked to, but the first part of the video would be on the second.
    let result = project.move_clips(&[
        along_its_track(&project, on_the_sound_track, 40),
        along_its_track(&project, first_part_of_the_video, 150),
    ]);

    assert_eq!(
        result,
        Err(EditError::Clip(ClipEditError::WouldOverlap {
            clip: first_part_of_the_video
        })),
        "it is the one that would end up on the other that is named"
    );
    assert_eq!(project, before, "not even the sound moved");

    let nowhere = ClipId::new(TrackId::new(99), 0);
    assert_eq!(
        project.move_clips(&[
            along_its_track(&project, on_the_sound_track, 40),
            move_to(nowhere, project.tracks()[0].id(), 1)
        ]),
        Err(EditError::Clip(ClipEditError::UnknownClip { clip: nowhere }))
    );
    assert_eq!(project, before);
    assert_eq!(project.move_clips(&[]), Ok(()), "moving nothing is allowed");
}

#[test]
fn text_is_added_to_a_track_and_changed_through_its_location() {
    let mut project = project_with_a_video(300);
    let front = project.add_track_in_front();

    let (location, duration) = project
        .insert_text(front, frame(30), TextObject::default(), 90)
        .expect("the track is empty there");
    assert_eq!(duration, 90);
    assert_eq!(project.track_holding(location).unwrap().id(), front);
    let placement = project.placement_of(location).unwrap();
    assert_eq!(
        (
            placement.first_timeline_frame.value(),
            placement.end_timeline_frame.value()
        ),
        (30, 120)
    );

    let object = TextObject {
        text: "Hi".to_owned(),
        ..TextObject::default()
    };
    project.set_text_object(location, object.clone()).unwrap();
    assert_eq!(
        project.placement_of(location).unwrap().clip.as_text().unwrap().object,
        object
    );
    assert_eq!(project.set_clip_duration(location, 10).unwrap(), 10);

    let video_clip = first_clip_location(&project, 1);
    assert_eq!(
        project.set_text_object(video_clip, object),
        Err(EditError::Clip(ClipEditError::NotATextClip { clip: video_clip }))
    );
    assert!(
        project
            .insert_text(TrackId::new(99), frame(0), TextObject::default(), 10)
            .is_none(),
        "no such track"
    );
    assert!(
        project
            .insert_text(
                project.track_holding(video_clip).unwrap().id(),
                frame(10),
                TextObject::default(),
                10
            )
            .is_none(),
        "the video is there"
    );
}

#[test]
fn a_clip_is_put_somewhere_on_the_picture_through_its_location_and_only_that_clip_changes() {
    let mut project = project_with_a_video(300);
    let front = project.add_track_in_front();
    let (text, _) = project
        .insert_text(front, frame(30), TextObject::default(), 90)
        .unwrap();
    let video = first_clip_location(&project, 1);
    let somewhere = Placement::new(CanvasArea::new(0.1, 0.2, 0.3, 0.3), Angle::from_degrees(-30.0));

    project.set_clip_placement(video, somewhere).unwrap();
    assert_eq!(project.placement_of(video).unwrap().clip.placement(), Some(somewhere));
    assert_eq!(
        project.placement_of(text).unwrap().clip.placement(),
        Some(TextObject::default().placement()),
        "the text did not move"
    );
    project.set_clip_placement(text, somewhere).unwrap();
    assert_eq!(project.placement_of(text).unwrap().clip.placement(), Some(somewhere));

    let nobody = ClipId::new(TrackId::new(99), 0);
    assert_eq!(
        project.set_clip_placement(nobody, somewhere),
        Err(EditError::Clip(ClipEditError::UnknownClip { clip: nobody }))
    );
}

#[test]
fn a_sound_cannot_be_put_on_the_picture() {
    let mut project = Project::empty();
    project.import_source(audio_source(1, 10), frame(0)).unwrap();
    let sound = first_clip_location(&project, 0);
    let before = project.clone();
    assert_eq!(
        project.set_clip_placement(sound, Placement::default()),
        Err(EditError::Clip(ClipEditError::NotOnThePicture { clip: sound }))
    );
    assert_eq!(project, before);
}

// --- pictures ---

#[test]
fn a_picture_is_put_on_an_empty_track_shown_for_five_seconds_at_the_frame_asked_for() {
    let mut project = Project::empty();
    let outcome = project.import_source(picture_source(1, 800, 600), frame(10)).unwrap();
    assert!(!outcome.created_track, "the empty track there was");
    let location = outcome.clip.expect("a picture makes a clip");
    let placement = project.placement_of(location).unwrap();
    assert_eq!(placement.clip.kind(), ClipKind::Image);
    assert_eq!(
        (
            placement.first_timeline_frame.value(),
            placement.end_timeline_frame.value()
        ),
        (10, 10 + 150),
        "5 seconds at the 30 frames per second of a project that has no video"
    );
    assert_eq!(
        placement.clip.placement(),
        None,
        "fitted inside the picture until it is put somewhere"
    );
}

#[test]
fn a_picture_is_shown_for_five_seconds_of_the_frame_rate_of_the_project_it_is_in() {
    let mut project = project_with_a_video(1000); // 30 frames per second
    let mut other = Project::empty();
    other
        .import_source(
            SourceMedia::video(
                SourceMediaIdentifier::new(1),
                link("1.mp4"),
                SourceVideoProperties {
                    frame_rate: FrameRate::new(25, 1).unwrap(),
                    total_frame_count: 100,
                    display_width_in_pixels: 640,
                    display_height_in_pixels: 360,
                    has_audio: false,
                },
            ),
            frame(0),
        )
        .unwrap();
    for (project, frames) in [(&mut project, 150), (&mut other, 125)] {
        let outcome = project.import_source(picture_source(7, 100, 100), frame(0)).unwrap();
        assert!(
            outcome.created_track,
            "the video is on the only track, so there is a new one"
        );
        let location = outcome.clip.unwrap();
        assert_eq!(project.placement_of(location).unwrap().timeline_frame_count(), frames);
    }
}

#[test]
fn a_picture_does_not_give_the_project_a_frame_rate_or_a_picture_size() {
    let mut project = Project::empty();
    project.import_source(picture_source(1, 800, 600), frame(0)).unwrap();
    assert_eq!(project.frame_rate(), FRAME_RATE_WITHOUT_A_VIDEO);
    assert_eq!(project.picture_size(), PICTURE_SIZE_WITHOUT_A_VIDEO);
    // A video that comes later still gives the project its own.
    let mut with_video_first = project_with_a_video(100);
    with_video_first
        .import_source(picture_source(2, 10, 10), frame(0))
        .unwrap();
    assert_eq!(with_video_first.picture_size(), (1280, 720));
}

#[test]
fn a_clip_cannot_be_made_from_a_file_of_the_wrong_kind() {
    let sources = vec![picture_source(1, 100, 100), video_source(2, 30, 100)];
    let picture_clip_of = |identifier| Clip::Image(ImageClip::new(SourceMediaIdentifier::new(identifier), 30).unwrap());
    let ok = Project::from_parts(
        sources.clone(),
        vec![track_with(0, vec![(0, picture_clip_of(1))])],
        SpeedIndicatorLayout::DEFAULT,
    );
    assert!(ok.is_ok());
    let wrong = Project::from_parts(
        sources.clone(),
        vec![track_with(0, vec![(0, picture_clip_of(2))])],
        SpeedIndicatorLayout::DEFAULT,
    );
    assert!(matches!(
        wrong,
        Err(InconsistentProject::ClipUsesSourceOfTheWrongKind { .. })
    ));
    let video_from_a_picture = Project::from_parts(
        sources.clone(),
        vec![track_with(0, vec![(0, video_clip(1, 0, 10))])],
        SpeedIndicatorLayout::DEFAULT,
    );
    assert!(matches!(
        video_from_a_picture,
        Err(InconsistentProject::ClipUsesSourceOfTheWrongKind { .. })
    ));
    let unknown = Project::from_parts(
        vec![video_source(2, 30, 100)],
        vec![track_with(0, vec![(0, picture_clip_of(9))])],
        SpeedIndicatorLayout::DEFAULT,
    );
    assert!(matches!(
        unknown,
        Err(InconsistentProject::ClipUsesUnknownSource { .. })
    ));
}

#[test]
fn how_long_a_picture_is_shown_for_can_be_set_and_how_long_a_video_lasts_cannot() {
    let mut project = project_with_a_video(300);
    let outcome = project.import_source(picture_source(2, 100, 100), frame(0)).unwrap();
    let picture = outcome.clip.unwrap();
    assert_eq!(project.set_clip_duration(picture, 20).unwrap(), 20);
    assert_eq!(project.placement_of(picture).unwrap().timeline_frame_count(), 20);
    assert_eq!(
        project.set_clip_duration(picture, 9000).unwrap(),
        9000,
        "nothing is after it on its track"
    );

    let video = first_clip_location(&project, 1);
    assert_eq!(
        project.set_clip_duration(video, 20),
        Err(EditError::Clip(ClipEditError::LastsAsLongAsItsFile { clip: video }))
    );
}

#[test]
fn a_picture_can_be_replaced_by_another_picture_but_not_by_a_video() {
    let mut project = Project::empty();
    project.import_source(picture_source(1, 100, 100), frame(0)).unwrap();
    let again = project.relink_source(
        SourceMediaIdentifier::new(1),
        link("elsewhere.png"),
        SourceMediaProperties::Image(SourceImageProperties {
            display_width_in_pixels: 300,
            display_height_in_pixels: 200,
        }),
    );
    assert_eq!(
        again,
        Ok(RelinkOutcome {
            frame_count_before: 1,
            frame_count_after: 1
        })
    );
    assert_eq!(
        project
            .source(SourceMediaIdentifier::new(1))
            .unwrap()
            .properties
            .display_size_in_pixels(),
        Some((300, 200))
    );
    let video = video_source(1, 30, 100);
    assert!(matches!(
        project.relink_source(SourceMediaIdentifier::new(1), video.link, video.properties),
        Err(RelinkError::DifferentKind { .. })
    ));
}

// --- deleting a track ---

#[test]
fn a_track_is_deleted_with_all_its_clips_and_the_others_stay_as_they_were_in_the_same_order() {
    let mut project = project_with_a_video(300);
    let middle = project.add_track_in_front();
    let (text, _) = project
        .insert_text(middle, frame(10), TextObject::default(), 30)
        .unwrap();
    let front = project.add_track_in_front();
    let before: Vec<TrackId> = project.tracks().iter().map(Track::id).collect();
    assert_eq!(before, vec![front, middle, TrackId::new(0)]);

    project.delete_track(middle).unwrap();

    let after: Vec<TrackId> = project.tracks().iter().map(Track::id).collect();
    assert_eq!(after, vec![front, TrackId::new(0)]);
    assert!(project.placement_of(text).is_none(), "its clips went with it");
    assert_eq!(
        project.total_frame_count(),
        300,
        "what is on the other tracks is where it was"
    );
    assert_eq!(project.sources().len(), 1, "the files are still part of the project");
}

#[test]
fn a_track_that_was_deleted_is_not_given_to_a_track_made_afterwards() {
    let mut project = project_with_a_video(300);
    let deleted = project.add_track_in_front();
    project.delete_track(deleted).unwrap();
    let made_afterwards = project.add_track_in_front();
    assert_ne!(
        made_afterwards, deleted,
        "so nothing that remembered the old one finds the new"
    );
}

#[test]
fn the_only_track_and_a_track_that_is_not_there_cannot_be_deleted_and_nothing_changes() {
    let mut project = project_with_a_video(300);
    let before = project.clone();
    let only = project.tracks()[0].id();
    assert_eq!(
        project.delete_track(only),
        Err(EditError::CannotDeleteTheOnlyTrack { track: only })
    );
    assert_eq!(project, before);
    project.add_track_in_front();
    let with_two = project.clone();
    assert_eq!(
        project.delete_track(TrackId::new(99)),
        Err(EditError::UnknownTrack {
            track: TrackId::new(99)
        })
    );
    assert_eq!(project, with_two);
}

// --- moving a track in the list ---

/// A project with four tracks, the front one first, and their ids.
fn project_with_four_tracks() -> (Project, [TrackId; 4]) {
    let mut project = project_with_a_video(300);
    let last = project.tracks()[0].id();
    let third = project.add_track_in_front();
    let second = project.add_track_in_front();
    let first = project.add_track_in_front();
    (project, [first, second, third, last])
}

fn order_of_tracks(project: &Project) -> Vec<TrackId> {
    project.tracks().iter().map(Track::id).collect()
}

#[test]
fn a_track_that_is_moved_goes_to_the_place_asked_for_and_the_others_keep_their_order() {
    let (mut project, [first, second, third, fourth]) = project_with_four_tracks();

    project.move_track(second, 3).unwrap();
    assert_eq!(
        order_of_tracks(&project),
        vec![first, third, fourth, second],
        "to the back"
    );

    project.move_track(fourth, 0).unwrap();
    assert_eq!(
        order_of_tracks(&project),
        vec![fourth, first, third, second],
        "to the front"
    );

    project.move_track(first, 2).unwrap();
    assert_eq!(
        order_of_tracks(&project),
        vec![fourth, third, first, second],
        "backwards, to the middle"
    );

    project.move_track(first, 1).unwrap();
    assert_eq!(
        order_of_tracks(&project),
        vec![fourth, first, third, second],
        "and forwards"
    );
}

#[test]
fn a_track_moved_to_the_place_it_is_at_changes_nothing() {
    let (mut project, tracks) = project_with_four_tracks();
    let before = project.clone();
    for (position, track) in tracks.iter().enumerate() {
        project.move_track(*track, position).unwrap();
        assert_eq!(project, before, "the track at {position}");
    }
}

#[test]
fn a_track_moved_past_the_last_place_goes_to_the_last_place() {
    let (mut project, [first, second, third, fourth]) = project_with_four_tracks();
    project.move_track(first, 99).unwrap();
    assert_eq!(order_of_tracks(&project), vec![second, third, fourth, first]);
}

#[test]
fn a_track_that_is_moved_keeps_everything_that_is_on_it_and_what_the_timeline_lasts() {
    let mut project = project_with_a_video(300);
    let behind = project.tracks()[0].id();
    let in_front = project.add_track_in_front();
    let (text, _) = project
        .insert_text(in_front, frame(10), TextObject::default(), 30)
        .unwrap();
    project.set_track_gain(in_front, Gain::from_decibels(-6.0)).unwrap();
    project
        .set_track_height(in_front, TrackHeight::from_points(70.0))
        .unwrap();
    let length = project.total_frame_count();
    let clips_behind = project.tracks()[1].entries().to_vec();

    project.move_track(in_front, 1).unwrap();

    assert_eq!(order_of_tracks(&project), vec![behind, in_front]);
    assert_eq!(project.total_frame_count(), length);
    assert_eq!(
        project.tracks()[0].entries(),
        &clips_behind[..],
        "the other track is as it was"
    );
    let moved = project.track(in_front).unwrap();
    assert_eq!(moved.gain(), Gain::from_decibels(-6.0));
    assert_eq!(moved.height(), TrackHeight::from_points(70.0));
    assert!(
        project.placement_of(text).is_some(),
        "its clips are still there under their ids"
    );
    assert_eq!(project.track_holding(text).unwrap().id(), in_front);
}

#[test]
fn a_track_that_is_not_there_cannot_be_moved_and_nothing_changes() {
    let (mut project, _) = project_with_four_tracks();
    let before = project.clone();
    assert_eq!(
        project.move_track(TrackId::new(99), 1),
        Err(EditError::UnknownTrack {
            track: TrackId::new(99)
        })
    );
    assert_eq!(project, before);
}

#[test]
fn whatever_moves_are_made_the_project_has_the_same_tracks_and_each_goes_where_it_was_sent() {
    let mut random = crate::test_support::DeterministicRandomNumbers::with_seed(5);
    let (mut project, tracks) = project_with_four_tracks();
    for round in 0..200 {
        let track = tracks[random.next_below(4) as usize];
        let position = random.next_below(6) as usize;
        let others_before: Vec<TrackId> = order_of_tracks(&project)
            .into_iter()
            .filter(|other| *other != track)
            .collect();

        project.move_track(track, position).unwrap();

        let order = order_of_tracks(&project);
        assert_eq!(order.len(), 4, "round {round}");
        assert_eq!(
            order.iter().position(|other| *other == track),
            Some(position.min(3)),
            "round {round}"
        );
        let others_after: Vec<TrackId> = order.into_iter().filter(|other| *other != track).collect();
        assert_eq!(
            others_after, others_before,
            "round {round}: the others keep their order"
        );
    }
}

#[test]
fn deleting_a_clip_leaves_the_project_with_the_rest() {
    let mut project = project_with_a_video(300);
    let location = first_clip_location(&project, 0);
    project.delete_clip(location).unwrap();
    assert!(project.has_no_clips());
    assert_eq!(project.total_frame_count(), 0);
    assert_eq!(project.sources().len(), 1, "the file is still part of the project");
}

#[test]
fn what_a_project_has_set_for_itself_is_kept_when_it_is_changed() {
    let settings = ProjectSettings {
        autosave: Some(true),
        sped_up_clip_sound: None,
    };
    let mut project = Project::empty().with_settings(settings);
    assert_eq!(project.settings(), settings);
    project.set_settings(ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS);
    assert_eq!(project.settings(), ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS);
    let layout = SpeedIndicatorLayout::new(0.5, 0.5, 0.2);
    project.set_speed_indicator_layout(layout);
    assert_eq!(project.speed_indicator_layout(), layout);
}

// --- going back to an earlier copy ---

#[test]
fn restoring_an_earlier_copy_brings_back_its_tracks_clips_and_files() {
    let mut project = project_with_a_video(300);
    let before = project.clone();
    project.split_at(frame(100), None);
    project.add_track_in_front();
    project.import_source(audio_source(2, 4), frame(0)).unwrap();
    assert_ne!(project, before);

    project.restore(before.clone());

    assert_eq!(project, before);
    assert_eq!(
        project.sources().len(),
        1,
        "the sound file is no longer one of the project's"
    );
}

#[test]
fn restoring_leaves_the_settings_of_the_project_as_they_are_now() {
    let mut project = project_with_a_video(300);
    let before = project.clone();
    project.split_at(frame(100), None);
    let changed_settings = ProjectSettings {
        autosave: Some(true),
        ..ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
    };
    project.set_settings(changed_settings);

    project.restore(before);

    assert_eq!(
        project.settings(),
        changed_settings,
        "undoing an edit does not undo a setting"
    );
    assert_eq!(project.tracks()[0].entries().len(), 1, "but the cut is undone");
}

#[test]
fn restoring_keeps_where_a_file_is_now_when_the_earlier_copy_knew_it_somewhere_else() {
    let mut project = project_with_a_video(300);
    let before = project.clone();
    project.split_at(frame(100), None);
    // The file was chosen again after the cut: it is somewhere else, and is longer.
    let properties = SourceMediaProperties::Video(SourceVideoProperties {
        total_frame_count: 400,
        ..*video_source(1, 30, 300).properties.video().unwrap()
    });
    project
        .relink_source(SourceMediaIdentifier::new(1), link("moved/1.mp4"), properties)
        .unwrap();

    project.restore(before);

    let source = project.source(SourceMediaIdentifier::new(1)).unwrap();
    assert_eq!(source.link, link("moved/1.mp4"), "the file is where it was found");
    assert_eq!(source.properties, properties);
    assert_eq!(project.tracks()[0].entries().len(), 1, "and the cut is undone");
}

#[test]
fn restoring_a_copy_with_a_file_the_project_has_since_lost_brings_it_back_where_it_was() {
    let mut project = Project::empty();
    let before_the_import = project.clone();
    project.import_source(video_source(1, 30, 300), frame(0)).unwrap();
    let with_the_import = project.clone();

    project.restore(before_the_import);
    assert!(project.sources().is_empty());
    project.restore(with_the_import.clone());

    assert_eq!(
        project, with_the_import,
        "undoing the import and redoing it gives it back as it was"
    );
}

// --- the frame rate of the project ---

/// The ten second 30 frames per second video cut into pieces at the frames given, and a sound, a text
/// object and a picture on tracks of their own, with gaps before some of them.
fn project_of_every_kind_at_30_frames_per_second() -> Project {
    let mut project = project_with_a_video(300);
    for at in [100, 175, 240] {
        project.split_at(frame(at), None);
    }
    let second_piece = project.tracks()[0].entries()[1].id();
    project.set_clip_speed(second_piece, speed(5)).unwrap();
    project.import_source(audio_source(2, 4), frame(45)).unwrap();
    let text_track = project.add_track_in_front();
    project
        .insert_text(text_track, frame(60), TextObject::default(), 90)
        .unwrap();
    project.import_source(picture_source(3, 800, 400), frame(200)).unwrap();
    project
}

/// The track that has a clip of the kind `kind` on it (the first, from the front).
fn track_holding(project: &Project, kind: ClipKind) -> &Track {
    project
        .tracks()
        .iter()
        .find(|track| track.entries().iter().any(|entry| entry.clip().kind() == kind))
        .expect("a track with a clip of that kind")
}

/// Where every clip starts and how long it lasts, as (kind, start, length) in seconds, track by track.
fn clip_times_in_seconds(project: &Project) -> Vec<Vec<(ClipKind, f64, f64)>> {
    let rate = project.frame_rate();
    project
        .tracks()
        .iter()
        .map(|track| {
            track
                .placements()
                .map(|placement| {
                    (
                        placement.clip.kind(),
                        rate.seconds_at_frame(placement.first_timeline_frame.value()),
                        rate.seconds_at_frame(placement.timeline_frame_count()),
                    )
                })
                .collect()
        })
        .collect()
}

#[test]
fn a_new_frame_rate_is_chosen_and_the_project_follows_it_even_when_it_is_the_one_it_had() {
    let mut project = project_with_a_video(300);
    assert_eq!(project.chosen_frame_rate(), None);
    project.set_frame_rate(frame_rate(30));
    assert_eq!(project.chosen_frame_rate(), Some(frame_rate(30)));
    assert_eq!(project.total_frame_count(), 300, "nothing was changed by that");
}

#[test]
fn changing_the_frame_rate_keeps_what_each_clip_lasts_and_where_it_starts_in_time() {
    let mut project = project_of_every_kind_at_30_frames_per_second();
    let before = clip_times_in_seconds(&project);
    for new_rate in [
        frame_rate(60),
        frame_rate(24),
        FrameRate::new(30000, 1001).unwrap(),
        frame_rate(25),
        frame_rate(12),
    ] {
        let mut changed = project.clone();
        changed.set_frame_rate(new_rate);
        assert_eq!(changed.frame_rate(), new_rate);
        let after = clip_times_in_seconds(&changed);
        let one_frame = 1.0 / new_rate.frames_per_second();
        for (track_before, track_after) in before.iter().zip(&after) {
            assert_eq!(track_before.len(), track_after.len(), "{new_rate}: no clip is lost");
            for (index, (clip_before, clip_after)) in track_before.iter().zip(track_after).enumerate() {
                assert_eq!(clip_before.0, clip_after.0);
                // (A clip made from pieces that touch can be a frame off for each piece before it: the pieces
                // are counted again one at a time, and a piece that is sped up lasts a whole number of frames.)
                let allowed = one_frame * (2.0 + index as f64);
                assert!(
                    (clip_before.1 - clip_after.1).abs() <= allowed,
                    "{new_rate}: {clip_before:?} starts as {clip_after:?}"
                );
                assert!(
                    (clip_before.2 - clip_after.2).abs() <= allowed,
                    "{new_rate}: {clip_before:?} lasts as {clip_after:?}"
                );
            }
        }
    }
    project.set_frame_rate(frame_rate(30));
}

#[test]
fn clips_that_touch_still_touch_after_the_frame_rate_changes_and_clips_with_a_gap_between_them_still_have_one() {
    let mut project = project_of_every_kind_at_30_frames_per_second();
    let touching = |project: &Project| -> Vec<Vec<bool>> {
        project
            .tracks()
            .iter()
            .map(|track| {
                track
                    .entries()
                    .iter()
                    .map(|entry| entry.gap_before_frames() == 0)
                    .collect()
            })
            .collect()
    };
    let before = touching(&project);
    for new_rate in [
        frame_rate(60),
        frame_rate(24),
        FrameRate::new(24000, 1001).unwrap(),
        frame_rate(15),
    ] {
        let mut changed = project.clone();
        changed.set_frame_rate(new_rate);
        assert_eq!(touching(&changed), before, "{new_rate}");
    }
    project.set_frame_rate(frame_rate(30));
}

#[test]
fn the_pieces_a_video_was_cut_into_still_fit_together_in_the_source_and_in_time_at_a_new_rate() {
    // A video of 300 frames at 30 cut at 100 and 175 (normal speed), made to run at 24 and at 25.
    for new_rate in [frame_rate(24), frame_rate(25), frame_rate(60)] {
        let mut project = project_with_a_video(300);
        project.split_at(frame(100), None);
        project.split_at(frame(175), None);
        project.set_frame_rate(new_rate);
        let track = &project.tracks()[0];
        let pieces: Vec<_> = track.placements().collect();
        let ranges: Vec<(u64, u64)> = pieces
            .iter()
            .map(|piece| {
                let media = piece.clip.as_media().unwrap();
                (media.first_source_frame().value(), media.end_source_frame().value())
            })
            .collect();
        assert_eq!(ranges[0].0, 0);
        assert_eq!(
            ranges[0].1, ranges[1].0,
            "{new_rate}: no source frame is skipped or shown twice at the cut"
        );
        assert_eq!(ranges[1].1, ranges[2].0);
        assert_eq!(ranges[2].1, frame_rate(30).convert_frame_count_to(300, new_rate));
        assert_eq!(
            pieces[2].end_timeline_frame.value(),
            ranges[2].1,
            "{new_rate}: the end of the last piece is where all ten seconds end, with no drift"
        );
    }
}

#[test]
fn counting_at_another_rate_and_back_gives_the_project_it_was_when_the_other_rate_is_a_whole_multiple() {
    let original = project_of_every_kind_at_30_frames_per_second();
    for multiple in [2, 4] {
        let mut changed = original.clone();
        changed.set_frame_rate(frame_rate(30 * multiple));
        assert_ne!(changed.total_frame_count(), original.total_frame_count());
        changed.set_frame_rate(frame_rate(30));
        assert_eq!(changed.tracks(), original.tracks(), "x{multiple} and back");
        assert_eq!(changed.frame_rate(), frame_rate(30));
    }
}

#[test]
fn changing_the_frame_rate_changes_nothing_else_about_the_clips() {
    let mut project = project_of_every_kind_at_30_frames_per_second();
    let video_track = track_holding(&project, ClipKind::Video).id();
    let video = project.track(video_track).unwrap().entries()[0].id();
    project.set_clip_gain(video, Gain::from_decibels(6.0)).unwrap();
    project.set_clip_muted(video, true).unwrap();
    let placed = Placement::upright(CanvasArea::new(0.1, 0.2, 0.5, 0.4));
    project.set_clip_placement(video, placed).unwrap();
    project.set_track_gain(video_track, Gain::from_decibels(-3.0)).unwrap();
    let sound_track = track_holding(&project, ClipKind::Audio).id();
    project.set_track_speed(sound_track, speed(2)).unwrap();
    let titles: Vec<String> = project.tracks().iter().map(|track| track.title().to_owned()).collect();
    let sources = project.sources().to_vec();

    project.set_frame_rate(frame_rate(48));

    assert_eq!(project.sources(), sources, "the files are what they were");
    assert_eq!(
        project
            .tracks()
            .iter()
            .map(|track| track.title().to_owned())
            .collect::<Vec<_>>(),
        titles
    );
    let media = project.placement_of(video).unwrap().clip.as_media().unwrap();
    assert_eq!(media.gain(), Gain::from_decibels(6.0));
    assert!(media.is_muted());
    assert_eq!(media.placement(), Some(placed));
    assert_eq!(project.track(video_track).unwrap().gain(), Gain::from_decibels(-3.0));
    assert_eq!(project.track(sound_track).unwrap().speed(), speed(2));
}

#[test]
fn text_and_pictures_last_as_long_in_time_at_the_new_rate_and_a_sped_up_clip_plays_at_the_same_speed() {
    let mut project = project_of_every_kind_at_30_frames_per_second();
    project.set_frame_rate(frame_rate(60));

    let text = track_holding(&project, ClipKind::Text).placements().next().unwrap();
    assert_eq!(
        (text.first_timeline_frame.value(), text.timeline_frame_count()),
        (120, 180),
        "three seconds from the second second"
    );
    let picture = track_holding(&project, ClipKind::Image).placements().next().unwrap();
    assert_eq!(
        (picture.first_timeline_frame.value(), picture.timeline_frame_count()),
        (400, 300),
        "five seconds from the seventh second"
    );
    let video_pieces: Vec<_> = track_holding(&project, ClipKind::Video).placements().collect();
    let sped_up = &video_pieces[1];
    assert_eq!(
        sped_up.effective_speed,
        speed(5),
        "it is still played at five times the speed"
    );
    assert_eq!(
        (sped_up.first_timeline_frame.value(), sped_up.timeline_frame_count()),
        (200, 30),
        "the 75 pictures of it, 150 now, are shown 5 at a time: for half a second at 60 a second"
    );
}

// --- which clip is which ---

#[test]
fn clips_made_on_different_tracks_have_different_ids_even_when_each_is_the_first_its_track_made() {
    let mut project = project_with_a_video(300);
    let front = project.add_track_in_front();
    let (text, _) = project.insert_text(front, frame(0), TextObject::default(), 30).unwrap();
    let video = first_clip_location(&project, 1);

    assert_ne!(text, video);
    assert_eq!(
        (text.number(), video.number()),
        (0, 0),
        "each is the first its track made"
    );
    assert_eq!(text.origin(), front);
    assert_eq!(video.origin(), project.tracks()[1].id());
}

#[test]
fn a_clip_is_found_on_the_track_that_has_it_and_a_clip_that_no_track_has_is_not_found() {
    let mut project = project_with_a_video(300);
    project.split_at(frame(100), None);
    let front = project.add_track_in_front();
    project.insert_text(front, frame(0), TextObject::default(), 30).unwrap();

    for track in project.tracks() {
        for placement in track.placements() {
            assert_eq!(project.track_holding(placement.id).map(Track::id), Some(track.id()));
            assert_eq!(
                project
                    .placement_of(placement.id)
                    .map(|found| found.first_timeline_frame),
                Some(placement.first_timeline_frame)
            );
        }
    }
    let on_no_track = ClipId::new(TrackId::new(99), 0);
    assert!(project.track_holding(on_no_track).is_none());
    assert!(project.placement_of(on_no_track).is_none());
    let never_made = ClipId::new(project.tracks()[1].id(), 99);
    assert!(project.track_holding(never_made).is_none());
    assert!(project.placement_of(never_made).is_none());
}

// --- moving clips to other tracks ---

/// A project with these tracks, the one in front first, each a list of the clips on it with the empty frames
/// before each. The files are a video of 1000 frames (file number 1) and a sound of 100 seconds (number 2).
fn project_of_tracks(tracks: Vec<Vec<(u64, Clip)>>) -> Project {
    let tracks = tracks
        .into_iter()
        .enumerate()
        .map(|(id, clips)| track_with(id as u32, clips))
        .collect();
    let files = vec![video_source(1, 30, 1000), audio_source(2, 100)];
    Project::from_parts(files, tracks, SpeedIndicatorLayout::DEFAULT).unwrap()
}

fn text_clip(frames: u64) -> Clip {
    Clip::Text(TextClip::new(TextObject::default(), frames).unwrap())
}

fn move_to(clip: ClipId, to_track: TrackId, start: u64) -> ClipMove {
    ClipMove {
        clip,
        to_track,
        start: frame(start),
    }
}

/// The move that puts `clip` at the frame `start` on the track it is on.
fn along_its_track(project: &Project, clip: ClipId, start: u64) -> ClipMove {
    move_to(clip, project.track_holding(clip).unwrap().id(), start)
}

/// Where the clips of the track at `position` in the list of tracks are: the first frame of each, and the
/// frame after its last.
fn spans_of(project: &Project, position: usize) -> Vec<(u64, u64)> {
    project.tracks()[position]
        .placements()
        .map(|placement| {
            (
                placement.first_timeline_frame.value(),
                placement.end_timeline_frame.value(),
            )
        })
        .collect()
}

#[test]
fn a_clip_moved_to_another_track_is_there_from_then_on_with_the_id_it_had_and_nothing_else_moves() {
    let mut project = project_of_tracks(vec![
        vec![],
        vec![(0, video_clip(1, 0, 100)), (0, video_clip(1, 100, 300))],
    ]);
    let (front, back) = (project.tracks()[0].id(), project.tracks()[1].id());
    let first = project.tracks()[1].entries()[0].id();
    let second = project.tracks()[1].entries()[1].id();

    project.move_clips(&[move_to(first, front, 20)]).unwrap();

    assert_eq!(project.track_holding(first).map(Track::id), Some(front));
    assert_eq!(project.track_holding(second).map(Track::id), Some(back));
    assert_eq!(spans_of(&project, 0), vec![(20, 120)]);
    assert_eq!(
        spans_of(&project, 1),
        vec![(100, 300)],
        "the clip that touched it stays where it was"
    );
    assert_eq!(
        project.tracks()[1].entries()[0].gap_before_frames(),
        100,
        "with room before it now"
    );
    assert_eq!(first.origin(), back, "it is still the clip the back track made");
    assert_eq!(project.tracks()[0].entries()[0].id(), first);
}

#[test]
fn a_clip_moved_back_to_the_track_it_was_made_on_is_the_clip_it_was() {
    let mut project = project_of_tracks(vec![vec![], vec![(0, video_clip(1, 0, 100))]]);
    let (front, back) = (project.tracks()[0].id(), project.tracks()[1].id());
    let clip = project.tracks()[1].entries()[0].id();
    let before = project.clone();

    project.move_clips(&[move_to(clip, front, 7)]).unwrap();
    project.move_clips(&[move_to(clip, back, 0)]).unwrap();

    assert_eq!(project, before);
}

#[test]
fn a_clip_moved_to_a_faster_track_is_shorter_there_and_on_a_slower_one_longer() {
    let mut project = project_of_tracks(vec![vec![], vec![(0, video_clip(1, 0, 100))]]);
    let (front, back) = (project.tracks()[0].id(), project.tracks()[1].id());
    let clip = project.tracks()[1].entries()[0].id();
    project.set_track_speed(front, speed(2)).unwrap();

    project.move_clips(&[move_to(clip, front, 10)]).unwrap();
    assert_eq!(spans_of(&project, 0), vec![(10, 60)], "100 frames at twice the speed");

    project.move_clips(&[move_to(clip, back, 30)]).unwrap();
    assert_eq!(spans_of(&project, 1), vec![(30, 130)]);
}

#[test]
fn text_moves_to_other_tracks_too_and_lasts_as_long_as_it_did() {
    let mut project = project_of_tracks(vec![vec![], vec![(5, text_clip(40))], vec![]]);
    let (front, back) = (project.tracks()[0].id(), project.tracks()[2].id());
    project.set_track_speed(front, speed(4)).unwrap();
    let text = project.tracks()[1].entries()[0].id();

    project.move_clips(&[move_to(text, front, 0)]).unwrap();
    assert_eq!(
        spans_of(&project, 0),
        vec![(0, 40)],
        "a text object does not play faster"
    );
    project.move_clips(&[move_to(text, back, 3)]).unwrap();
    assert_eq!(spans_of(&project, 2), vec![(3, 43)]);
    assert!(project.tracks()[0].is_empty() && project.tracks()[1].is_empty());
}

#[test]
fn clips_cannot_be_moved_onto_a_clip_of_the_track_they_go_to_and_then_nothing_moves() {
    let mut project = project_of_tracks(vec![
        vec![(50, video_clip(1, 0, 100))],
        vec![(0, video_clip(1, 0, 100))],
    ]);
    let front = project.tracks()[0].id();
    let in_front = project.tracks()[0].entries()[0].id();
    let behind = project.tracks()[1].entries()[0].id();
    let before = project.clone();

    // From 120 the clip behind would be over the clip in front (50 to 150): the one that starts inside the other is named.
    assert_eq!(
        project.move_clips(&[move_to(behind, front, 120)]),
        Err(EditError::Clip(ClipEditError::WouldOverlap { clip: behind }))
    );
    assert_eq!(
        project.move_clips(&[move_to(behind, front, 20)]),
        Err(EditError::Clip(ClipEditError::WouldOverlap { clip: in_front })),
        "from 20 it would end at 120, over the start of the one in front"
    );
    assert_eq!(project, before);
    // And at 150 it fits, right after it.
    project.move_clips(&[move_to(behind, front, 150)]).unwrap();
    assert_eq!(spans_of(&project, 0), vec![(50, 150), (150, 250)]);
    assert!(project.tracks()[1].is_empty());
}

#[test]
fn clips_that_swap_tracks_do_so_because_they_all_leave_before_any_arrives() {
    let mut project = project_of_tracks(vec![vec![(0, video_clip(1, 0, 50))], vec![(0, video_clip(1, 0, 80))]]);
    let (front, back) = (project.tracks()[0].id(), project.tracks()[1].id());
    let short = project.tracks()[0].entries()[0].id();
    let long = project.tracks()[1].entries()[0].id();

    project
        .move_clips(&[move_to(short, back, 0), move_to(long, front, 0)])
        .unwrap();

    assert_eq!(project.track_holding(short).map(Track::id), Some(back));
    assert_eq!(project.track_holding(long).map(Track::id), Some(front));
    assert_eq!(spans_of(&project, 0), vec![(0, 80)]);
    assert_eq!(spans_of(&project, 1), vec![(0, 50)]);
}

#[test]
fn a_clip_that_is_named_twice_goes_where_it_is_told_last_and_a_move_of_nothing_is_allowed() {
    let mut project = project_of_tracks(vec![vec![], vec![], vec![(0, video_clip(1, 0, 10))]]);
    let tracks: Vec<TrackId> = project.tracks().iter().map(Track::id).collect();
    let clip = project.tracks()[2].entries()[0].id();

    project
        .move_clips(&[move_to(clip, tracks[0], 5), move_to(clip, tracks[1], 9)])
        .unwrap();

    assert_eq!(project.track_holding(clip).map(Track::id), Some(tracks[1]));
    assert_eq!(spans_of(&project, 1), vec![(9, 19)]);
    assert!(project.tracks()[0].is_empty());
    assert_eq!(project.move_clips(&[]), Ok(()));
}

#[test]
fn moving_to_a_track_that_is_not_there_or_a_clip_that_is_not_there_is_an_error_and_nothing_moves() {
    let mut project = project_of_tracks(vec![vec![], vec![(0, video_clip(1, 0, 100))]]);
    let front = project.tracks()[0].id();
    let clip = project.tracks()[1].entries()[0].id();
    let before = project.clone();

    assert_eq!(
        project.move_clips(&[move_to(clip, TrackId::new(99), 0)]),
        Err(EditError::UnknownTrack {
            track: TrackId::new(99)
        })
    );
    let nobody = ClipId::new(front, 7);
    assert_eq!(
        project.move_clips(&[move_to(clip, front, 0), move_to(nobody, front, 500)]),
        Err(EditError::Clip(ClipEditError::UnknownClip { clip: nobody }))
    );
    assert_eq!(project, before);
}

/// What the clips of a project are: each clip's id, the track it is on, where it starts and how long it lasts,
/// in the order of the ids.
fn where_the_clips_are(project: &Project) -> Vec<(ClipId, TrackId, u64, u64)> {
    let mut clips: Vec<(ClipId, TrackId, u64, u64)> = project
        .tracks()
        .iter()
        .flat_map(|track| {
            track.placements().map(|placement| {
                (
                    placement.id,
                    track.id(),
                    placement.first_timeline_frame.value(),
                    placement.timeline_frame_count(),
                )
            })
        })
        .collect();
    clips.sort();
    clips
}

#[test]
fn random_moves_to_random_tracks_do_exactly_what_a_plain_calculation_says_or_are_refused_and_change_nothing() {
    let mut random = crate::test_support::DeterministicRandomNumbers::with_seed(5);
    for round in 0..60 {
        // Three tracks with clips of every kind of length, some on tracks that play faster.
        let mut project = project_of_tracks(
            (0..3)
                .map(|_| {
                    let mut clips = Vec::new();
                    for _ in 0..random.next_below(5) {
                        let gap = random.next_below(40);
                        let length = 1 + random.next_below(60);
                        let clip = match random.next_below(3) {
                            0 => text_clip(length),
                            1 => video_clip(1, 0, length * (1 + random.next_below(3))),
                            _ => audio_clip(2, 0, length),
                        };
                        clips.push((gap, clip));
                    }
                    clips
                })
                .collect(),
        );
        let track_ids: Vec<TrackId> = project.tracks().iter().map(Track::id).collect();
        for &track in &track_ids {
            project
                .set_track_speed(track, speed(1 + random.next_below(3) as u32))
                .unwrap();
        }
        for step in 0..10 {
            let everywhere = where_the_clips_are(&project);
            if everywhere.is_empty() {
                break;
            }
            let moves: Vec<ClipMove> = (0..1 + random.next_below(3))
                .map(|_| {
                    let (clip, ..) = everywhere[random.next_below(everywhere.len() as u64) as usize];
                    move_to(clip, track_ids[random.next_below(3) as usize], random.next_below(300))
                })
                .collect();

            // The plain calculation: every clip is where it was except the ones moved (the last word for each),
            // and then no two clips of one track may be at the same time.
            let mut wanted: Vec<(ClipId, TrackId, u64)> = everywhere
                .iter()
                .map(|&(clip, track, start, _)| (clip, track, start))
                .collect();
            for requested in &moves {
                let entry = wanted.iter_mut().find(|(clip, ..)| *clip == requested.clip).unwrap();
                *entry = (requested.clip, requested.to_track, requested.start.value());
            }
            let length_on = |clip: ClipId, track: TrackId| {
                let placement = project.placement_of(clip).unwrap();
                placement
                    .clip
                    .timeline_frame_count(project.track(track).unwrap().speed())
            };
            let mut would_overlap = false;
            for &track in &track_ids {
                let mut on_it: Vec<(u64, u64)> = wanted
                    .iter()
                    .filter(|(_, on, _)| *on == track)
                    .map(|&(clip, on, start)| (start, start + length_on(clip, on)))
                    .collect();
                on_it.sort();
                would_overlap |= on_it.windows(2).any(|pair| pair[1].0 < pair[0].1);
            }
            let mut expected: Vec<(ClipId, TrackId, u64, u64)> = wanted
                .iter()
                .map(|&(clip, track, start)| (clip, track, start, length_on(clip, track)))
                .collect();
            expected.sort();

            let before = project.clone();
            let result = project.move_clips(&moves);
            let context = format!("round {round}, step {step}: {moves:?} of {everywhere:?}");
            if would_overlap {
                assert!(
                    matches!(result, Err(EditError::Clip(ClipEditError::WouldOverlap { .. }))),
                    "{context}: {result:?}"
                );
                assert_eq!(project, before, "{context}");
            } else {
                assert_eq!(result, Ok(()), "{context}");
                assert_eq!(where_the_clips_are(&project), expected, "{context}");
            }
        }
    }
}
