//! Tests of what the user is told when the rules of groups do not let an edit be made: a copy, a track that is moved, a clip that is made to
//! last longer or to play slower, a new text object, a new frame rate. In each of them the project stays as it was and nothing is an unsaved
//! change, and the message says which rule it is and names what is in the way.
//!
//! (What the rules are is in `project::groups`, and the words of the messages are in `application::group_messages`: these are about the edits
//! being refused where they are made, and the user being told.)

use super::clip_drag_tests::project_of_tracks;
use super::test_window::Window;
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::project::groups::test_projects::lay_out;
use crate::project::{
    AnimationTarget, ClipMove, FrameRate, GroupKind, Member, PropertyId, RackDestination, SpeedFactor,
    TimelineFrameIndex,
};

/// What the message that is up says, as its title and its words.
fn the_message_that_is_up(window: &Window) -> (String, String) {
    match &window.application().dialog {
        Some(Dialog::Message { title, text }) => (title.clone(), text.clone()),
        other => panic!("the user is told: {other:?}"),
    }
}

/// A window with a project that is as it is now, with nothing unsaved and nothing to undo.
fn window_showing_what_is_saved(project: crate::project::Project) -> Window {
    let mut window = Window::showing(project);
    window.application_mut().note_everything_is_saved();
    window
}

/// Checks that the message says `title`, and has `words` in it, and that the project is `before` and has nothing unsaved.
fn assert_refused(window: &Window, before: &crate::project::Project, title: &str, words: &[&str]) {
    let (found_title, text) = the_message_that_is_up(window);
    assert_eq!(found_title, title);
    for word in words {
        assert!(text.contains(word), "{word:?} is not in: {text}");
    }
    assert_eq!(window.application().project(), before, "nothing was changed");
    assert!(!window.application().has_unsaved_changes(), "it was not an edit");
}

#[test]
fn a_copy_that_would_make_the_rectangle_of_a_clip_group_take_in_a_clip_that_is_not_in_it_is_refused_and_the_clip_is_named()
 {
    // Track 0 has the clip A at frames 20 to 50 and track 1 has B at 20 to 50 and C at 60 to 90, which is outside the rectangle of the clip
    // group of A and B. A copy of A to its right (at 50 to 80) is in the group, and the rectangle is then 20 to 80: C is inside it.
    let mut project = project_of_tracks(&[&[(20, 30)], &[(20, 30), (10, 30)]]);
    let a = project.tracks()[0].placements().next().unwrap().id;
    let b = project.tracks()[1].placements().next().unwrap().id;
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(a), Member::Clip(b)])
        .unwrap();
    let before = project.clone();
    let mut window = window_showing_what_is_saved(project);

    window.do_this(Command::DuplicateClips {
        clips: vec![a],
        side: crate::project::ClipSide::Right,
    });

    assert_refused(
        &window,
        &before,
        "The copy cannot be made there",
        &[
            "\"Video clip: holiday.mp4 (at 00:00:02.00)\" would be inside the rectangle of the clip group \"Clip group 1\"",
        ],
    );
    assert!(
        window.application().selected_clips().is_empty(),
        "nothing was copied, so there are no copies to select"
    );
}

#[test]
fn a_clip_that_is_made_to_last_longer_where_its_group_would_then_take_in_another_clip_is_refused() {
    // Text clips: A on track 0 at 20 to 30, in a clip group with B on track 1 at 20 to 50; C is on track 1 at 55 to 70, just outside the
    // rectangle. If A lasted until frame 60 the rectangle would reach C.
    // (`lay_out` says where each clip starts, and not how far it is from the one before.)
    let laid = lay_out(&[&[(20, 10)], &[(20, 30), (55, 15)]]);
    let a = laid.clip(0, 0);
    let mut project = laid.project;
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(a), Member::Clip(laid.clips[1][0])])
        .unwrap();
    let before = project.clone();
    let mut window = window_showing_what_is_saved(project);

    window.do_this(Command::SetClipDuration { clip: a, frames: 40 });

    assert_refused(
        &window,
        &before,
        "The length cannot be changed",
        &["would be inside the rectangle of the clip group \"Clip group 1\""],
    );
}

#[test]
fn a_clip_that_is_made_to_play_slower_so_that_it_lasts_longer_than_its_group_has_room_for_is_refused_and_says_so() {
    // The clip A is 60 frames long at normal speed and plays at 4 times, so it lasts 15 frames (20 to 35): inside the rectangle of the clip group
    // that it is in with B (20 to 50), which C at 60 to 70 is outside. At normal speed A would last until frame 80, and the rectangle would
    // take C in.
    let mut project = project_of_tracks(&[&[(20, 60)], &[(20, 30), (10, 10)]]);
    let a = project.tracks()[0].placements().next().unwrap().id;
    let b = project.tracks()[1].placements().next().unwrap().id;
    project.set_clip_speed(a, SpeedFactor::new(4).unwrap()).unwrap();
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(a), Member::Clip(b)])
        .unwrap();
    let before = project.clone();
    let mut window = window_showing_what_is_saved(project);

    window.do_this(Command::SetClipSpeed {
        clip: a,
        speed_factor: SpeedFactor::NORMAL,
    });

    assert_refused(
        &window,
        &before,
        "The speed cannot be changed",
        &[
            "\"Video clip: holiday.mp4 (at 00:00:02.00)\" would be inside the rectangle of the clip group \"Clip group 1\"",
        ],
    );
}

#[test]
fn a_track_that_is_moved_into_the_middle_of_a_track_group_does_not_go_there_and_the_user_is_told() {
    let laid = lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)]]);
    let tracks = laid.tracks.clone();
    let mut project = laid.project;
    // A track group of the first and the third track cannot be made, so it is of the first two; the third is moved in between them.
    project
        .create_group(
            GroupKind::TrackGroup,
            &[Member::Track(tracks[0]), Member::Track(tracks[1])],
        )
        .unwrap();
    let before = project.clone();
    let mut window = window_showing_what_is_saved(project);

    window.do_this(Command::MoveTrack {
        track: tracks[2],
        to_position: 1,
    });

    assert_refused(
        &window,
        &before,
        "The track cannot go there",
        &["is between the tracks of the track group \"Track group 1\", and is not in it"],
    );
}

#[test]
fn a_new_text_object_where_nothing_is_but_the_rectangle_of_a_clip_group_is_refused_and_the_message_says_so_and_not_that_there_is_no_room()
 {
    let laid = lay_out(&[&[(20, 10)], &[(20, 30)]]);
    let (a, b) = (laid.clip(0, 0), laid.clip(1, 0));
    let tracks = laid.tracks.clone();
    let mut project = laid.project;
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(a), Member::Clip(b)])
        .unwrap();
    let before = project.clone();
    let mut window = window_showing_what_is_saved(project);

    // Frame 40 of the front track is free, and is inside the rectangle (frames 20 to 50 across both tracks).
    window.do_this(Command::NewTextObject {
        track: tracks[0],
        frame: TimelineFrameIndex::new(40),
    });
    let (title, text) = the_message_that_is_up(&window);
    assert_eq!(title, "A text object cannot be put there");
    assert!(
        text.contains("would be inside the rectangle of the clip group \"Clip group 1\""),
        "{text}"
    );
    assert_eq!(window.application().project(), &before);

    // Past the end of the rectangle it is made, as ever.
    window.application_mut().dialog = None;
    window.do_this(Command::NewTextObject {
        track: tracks[0],
        frame: TimelineFrameIndex::new(60),
    });
    assert!(window.application().dialog.is_none());
    assert_eq!(window.application().project().tracks()[0].entries().len(), 2);
}

#[test]
fn a_new_text_object_where_a_clip_is_still_says_that_there_is_no_room() {
    let laid = lay_out(&[&[(20, 10)]]);
    let track = laid.tracks[0];
    let mut window = window_showing_what_is_saved(laid.project);

    window.do_this(Command::NewTextObject {
        track,
        frame: TimelineFrameIndex::new(25),
    });

    let (title, _) = the_message_that_is_up(&window);
    assert_eq!(title, "There is no room for a text object there");
}

#[test]
fn a_frame_rate_that_would_put_a_clip_a_frame_inside_the_rectangle_of_a_clip_group_is_refused_and_the_rate_stays() {
    // (The numbers are those of the test of the project that counting everything again at 24 frames a second puts a clip a frame inside the
    // rectangle.)
    let laid = lay_out(&[&[(1, 2)], &[(0, 1), (3, 1)]]);
    let (a, b) = (laid.clip(0, 0), laid.clip(1, 0));
    let mut project = laid.project;
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(a), Member::Clip(b)])
        .unwrap();
    let before = project.clone();
    let mut window = window_showing_what_is_saved(project);
    window.move_current_frame_to(3);

    window.do_this(Command::SetProjectFrameRate(FrameRate::new(24, 1).unwrap()));

    assert_refused(
        &window,
        &before,
        "The frame rate cannot be changed",
        &["would be inside the rectangle of the clip group \"Clip group 1\""],
    );
    assert_eq!(window.application().project().frame_rate(), before.frame_rate());
    assert_eq!(
        window.current_frame(),
        3,
        "the playhead stays where it was: nothing was counted again, so it is not taken to the same moment at another rate"
    );
}

#[test]
fn an_edge_that_is_asked_to_go_where_it_would_cut_across_a_clip_is_refused_and_the_clip_is_named() {
    // The clip group has the clips at 20 to 50 and 65 to 95; the third, at 110 to 140, is cut across by an edge at 120.
    let laid = lay_out(&[&[(20, 30), (65, 30), (110, 30)]]);
    let (a, b) = (laid.clip(0, 0), laid.clip(0, 1));
    let mut project = laid.project;
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(a), Member::Clip(b)])
        .unwrap();
    let before = project.clone();
    let mut window = window_showing_what_is_saved(project);

    window.do_this(Command::SetTheFramesOfAClipGroup {
        group,
        first_frame: 20,
        end_frame: 120,
    });

    assert_refused(
        &window,
        &before,
        "The group cannot be changed that way",
        &["The edge cannot go there: it would cut across \"Text object: Text (at 00:00:03.20)\"."],
    );
}

#[test]
fn edges_of_a_group_that_is_not_a_clip_group_and_edges_that_would_leave_nothing_in_it_are_refused_and_the_user_is_told()
{
    let laid = lay_out(&[&[(20, 30), (65, 30)]]);
    let (a, b) = (laid.clip(0, 0), laid.clip(0, 1));
    let mut project = laid.project;
    let tag = project
        .create_group(GroupKind::Tag, &[Member::Clip(a), Member::Clip(b)])
        .unwrap();
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(a), Member::Clip(b)])
        .unwrap();
    let before = project.clone();
    let mut window = window_showing_what_is_saved(project);

    window.do_this(Command::SetTheFramesOfAClipGroup {
        group: tag,
        first_frame: 20,
        end_frame: 95,
    });
    assert_refused(
        &window,
        &before,
        "The group cannot be changed that way",
        &["the tag \"Tag 1\" has no edges to move: it is not a clip group."],
    );

    window.application_mut().dialog = None;
    window.do_this(Command::SetTheFramesOfAClipGroup {
        group,
        first_frame: 200,
        end_frame: 300,
    });
    assert_refused(
        &window,
        &before,
        "The group cannot be changed that way",
        &["Nothing would be left in the clip group \"Clip group 1\"."],
    );
}

#[test]
fn clips_that_are_asked_to_go_where_their_group_would_take_in_another_clip_are_refused_and_the_user_is_told() {
    // A and B are in a clip group on the first track (20 to 50 and 65 to 95); D is on the second track at 60 to 70, outside its rectangle. A
    // moved to the second track at frames 20 to 50 would make the rectangle reach across both tracks, and D is inside it.
    let laid = lay_out(&[&[(20, 30), (65, 30)], &[(60, 10)]]);
    let (a, b, d) = (laid.clip(0, 0), laid.clip(0, 1), laid.clip(1, 0));
    let tracks = laid.tracks.clone();
    let mut project = laid.project;
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(a), Member::Clip(b)])
        .unwrap();
    let before = project.clone();
    let mut window = window_showing_what_is_saved(project);
    let _ = d;

    window.do_this(Command::move_clips(vec![ClipMove {
        clip: a,
        to_track: tracks[1],
        start: TimelineFrameIndex::new(20),
    }]));

    assert_refused(
        &window,
        &before,
        "The clips cannot be moved there",
        &["would be inside the rectangle of the clip group \"Clip group 1\""],
    );
}

#[test]
fn racks_that_are_asked_to_go_to_a_new_crate_inside_the_rectangle_of_a_clip_group_are_refused_and_the_user_is_told() {
    // The clip group has A on the first track at 20 to 50 and B on the second at 80 to 95, so its rectangle is 20 to 95 across both tracks. The
    // clip T on the third track has a keyframe. A new crate on the second track at frames 30 to 60 would be inside the rectangle.
    let laid = lay_out(&[&[(20, 30)], &[(80, 15)], &[(20, 30)]]);
    let (a, b, t) = (laid.clip(0, 0), laid.clip(1, 0), laid.clip(2, 0));
    let tracks = laid.tracks.clone();
    let mut project = laid.project;
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(a), Member::Clip(b)])
        .unwrap();
    let rack = project
        .add_keyframe(
            AnimationTarget::Clip(t),
            PropertyId::Rotation,
            TimelineFrameIndex::new(30),
        )
        .unwrap()
        .unwrap()
        .rack;
    let before = project.clone();
    let mut window = window_showing_what_is_saved(project);

    window.do_this(Command::MoveRacks {
        racks: vec![rack],
        to: RackDestination::NewCrate {
            track: tracks[1],
            from_frame: 30,
            until_frame: 60,
        },
        shift_in_frames: 0,
        slot: None,
    });

    assert_refused(
        &window,
        &before,
        "The rack cannot go there",
        &["would be inside the rectangle of the clip group \"Clip group 1\""],
    );
}

#[test]
fn a_message_about_a_refusal_is_not_given_for_what_has_nothing_to_do_with_groups() {
    // A track that is moved in a project that has no groups is moved, or not, in silence, as it always was.
    let laid = lay_out(&[&[(0, 10)], &[(0, 10)]]);
    let tracks: Vec<_> = laid.tracks.clone();
    let mut window = window_showing_what_is_saved(laid.project);

    window.do_this(Command::MoveTrack {
        track: tracks[1],
        to_position: 0,
    });

    assert!(window.application().dialog.is_none());
    assert_eq!(window.application().project().tracks()[0].id(), tracks[1], "it went");
}
