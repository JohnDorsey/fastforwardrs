use std::path::PathBuf;

use super::*;
use crate::project::tracks::{ClipKind, ClipLocation};
use crate::project::{SourceAudioProperties, SourceVideoProperties};

fn frame_rate(frames_per_second: u32) -> FrameRate {
    FrameRate::from_whole_frames_per_second(frames_per_second).unwrap()
}

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn speed(factor: u32) -> SpeedFactor {
    SpeedFactor::new(factor).unwrap()
}

fn link(name: &str) -> SourceMediaLink {
    SourceMediaLink::from_absolute_path(PathBuf::from(format!("/files/{name}")))
}

fn video_source(identifier: u32, frames_per_second: u32, total_frame_count: u64) -> SourceMedia {
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

fn audio_source(identifier: u32, seconds: u64) -> SourceMedia {
    SourceMedia::audio(
        SourceMediaIdentifier::new(identifier),
        link(&format!("{identifier}.mp3")),
        SourceAudioProperties {
            duration_in_microseconds: seconds * 1_000_000,
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
fn project_with_a_video(frames: u64) -> Project {
    let mut project = Project::empty();
    project.import_source(video_source(1, 30, frames), frame(0)).unwrap();
    project
}

fn first_clip_location(project: &Project, track_position: usize) -> ClipLocation {
    let track = &project.tracks()[track_position];
    ClipLocation {
        track: track.id(),
        clip: track.entries()[0].id(),
    }
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
    let placement = project.placement_of(ClipLocation {
        track: outcome.track,
        clip: outcome.clip.unwrap(),
    });
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
    let placement = project
        .placement_of(ClipLocation {
            track: music.track,
            clip: music.clip.unwrap(),
        })
        .unwrap();
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
fn a_second_video_at_the_same_frame_rate_is_fine_and_a_different_one_is_refused_and_changes_nothing() {
    let mut project = project_with_a_video(300);
    assert!(project.import_source(video_source(2, 30, 100), frame(300)).is_ok());

    let before = project.clone();
    assert_eq!(
        project.import_source(video_source(3, 25, 100), frame(0)),
        Err(ImportError::DifferentFrameRate {
            project_frame_rate: frame_rate(30),
            chosen_frame_rate: frame_rate(25)
        })
    );
    assert_eq!(project, before);
}

#[test]
fn a_video_at_another_frame_rate_cannot_join_a_project_that_only_has_sound_and_text_on_its_tracks() {
    let mut project = Project::empty();
    project.import_source(audio_source(1, 10), frame(0)).unwrap();
    let before = project.clone();

    let result = project.import_source(video_source(2, 25, 100), frame(0));

    assert_eq!(
        result,
        Err(ImportError::ProjectAlreadyLaidOutAtAnotherFrameRate {
            project_frame_rate: frame_rate(30),
            chosen_frame_rate: frame_rate(25)
        })
    );
    assert_eq!(project, before);
    // But one at the frame rate everything is laid out at is fine,
    assert!(project.import_source(video_source(2, 30, 100), frame(0)).is_ok());
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
    assert!(matches!(
        Project::from_parts(
            vec![video_source(1, 30, 100), video_source(2, 25, 50)],
            vec![],
            SpeedIndicatorLayout::DEFAULT
        ),
        Err(InconsistentProject::MixedFrameRates { .. })
    ));
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
fn relinking_to_a_video_with_a_different_frame_rate_or_to_a_different_kind_of_file_is_refused() {
    let mut project = project_with_a_video(300);
    project.import_source(audio_source(2, 5), frame(0)).unwrap();
    let before = project.clone();

    assert_eq!(
        project.relink_source(
            SourceMediaIdentifier::new(1),
            link("other.mp4"),
            video_source(1, 25, 300).properties
        ),
        Err(RelinkError::DifferentFrameRate {
            project_frame_rate: frame_rate(30),
            chosen_frame_rate: frame_rate(25)
        })
    );
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
    let sound = ClipLocation {
        track: sound_track,
        clip: project.tracks()[0].entries()[0].id(),
    };

    assert!(
        project.split_at(frame(10), Some(sound)).is_empty(),
        "the sound is not at frame 10"
    );
    assert_eq!(project.track(video_track).unwrap().entries().len(), 1);

    let cut = project.split_at(frame(100), Some(sound));
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
    let track = location.track;

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

    let nobody = ClipLocation {
        track: TrackId::new(99),
        clip: ClipId::new(0),
    };
    assert_eq!(
        project.set_clip_speed(nobody, speed(2)),
        Err(EditError::UnknownTrack {
            track: TrackId::new(99)
        })
    );
    assert_eq!(
        project.delete_clip(nobody),
        Err(EditError::UnknownTrack {
            track: TrackId::new(99)
        })
    );
    assert_eq!(
        project.set_track_muted(TrackId::new(99), true),
        Err(EditError::UnknownTrack {
            track: TrackId::new(99)
        })
    );
    let no_such_clip = ClipLocation {
        track: location.track,
        clip: ClipId::new(99),
    };
    assert_eq!(
        project.delete_clip(no_such_clip),
        Err(EditError::Clip(ClipEditError::UnknownClip { clip: ClipId::new(99) }))
    );
}

#[test]
fn text_is_added_to_a_track_and_changed_through_its_location() {
    let mut project = project_with_a_video(300);
    let front = project.add_track_in_front();

    let (location, duration) = project
        .insert_text(front, frame(30), TextObject::default(), 90)
        .expect("the track is empty there");
    assert_eq!(duration, 90);
    assert_eq!(location.track, front);
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
    assert_eq!(project.set_text_duration(location, 10).unwrap(), 10);

    let video_clip = first_clip_location(&project, 1);
    assert_eq!(
        project.set_text_object(video_clip, object),
        Err(EditError::Clip(ClipEditError::NotATextClip { clip: video_clip.clip }))
    );
    assert!(
        project
            .insert_text(TrackId::new(99), frame(0), TextObject::default(), 10)
            .is_none(),
        "no such track"
    );
    assert!(
        project
            .insert_text(video_clip.track, frame(10), TextObject::default(), 10)
            .is_none(),
        "the video is there"
    );
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
