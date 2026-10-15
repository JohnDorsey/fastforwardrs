// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the question that is asked before a change that would lose keyframes is made: when it is asked, what it says, and what
//! becomes of the change and of the keyframes whichever way it is answered.

use egui::Key;
use egui_kittest::kittest::Queryable;

use super::test_window::{Window, project_of};
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::project::{
    AnimationTarget, ClipId, ClipMove, FrameRate, Project, PropertyId, SpeedFactor, TimelineFrameIndex, TrackId,
};

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn speed(factor: u32) -> SpeedFactor {
    SpeedFactor::new(factor).unwrap()
}

/// A window with a video of 300 frames on the back track and, on a track in front of it, a text object that lasts 90 frames from
/// the first frame, which has a keyframe of how far it is turned at each of `frames`; with nothing unsaved. Gives the text object.
fn window_with_a_text_object_turned_at(frames: &[u64]) -> (Window, ClipId) {
    let mut window = Window::showing(project_of(300));
    window.do_this(Command::AddTrack);
    let front = window.application().project().tracks()[0].id();
    window.do_this(Command::NewTextObject {
        track: front,
        frame: frame(0),
    });
    let text = window.clip(0, 0);
    assert_eq!(duration_of(&window, text), 90);
    for at in frames {
        window.do_this(Command::AddKeyframe {
            target: AnimationTarget::Clip(text),
            property: PropertyId::Rotation,
            frame: frame(*at),
        });
    }
    window.application_mut().note_everything_is_saved();
    (window, text)
}

/// A window like that, with five keyframes of the loudness of the video too, at frames 100 to 104 of it. Gives the video as well.
fn window_with_a_text_object_and_a_video_that_each_have_keyframes() -> (Window, ClipId, ClipId) {
    let (mut window, text) = window_with_a_text_object_turned_at(&[0, 10, 20, 30]);
    let video = window.clip(1, 0);
    for at in 100..105 {
        window.do_this(Command::AddKeyframe {
            target: AnimationTarget::Clip(video),
            property: PropertyId::Gain,
            frame: frame(at),
        });
    }
    window.application_mut().note_everything_is_saved();
    (window, text, video)
}

fn duration_of(window: &Window, clip: ClipId) -> u64 {
    window
        .application()
        .project()
        .placement_of(clip)
        .unwrap()
        .timeline_frame_count()
}

fn keyframes(window: &Window) -> usize {
    window.application().project().keyframe_count()
}

/// The question that is open, and how many keyframes it says would be lost.
fn the_question_open(window: &Window) -> Option<(Command, usize)> {
    match &window.application().dialog {
        Some(Dialog::ConfirmLosingKeyframes { change, how_many }) => Some(((**change).clone(), *how_many)),
        _ => None,
    }
}

fn agree(window: &mut Window) {
    let button = if window.harness.query_by_label("Lose it").is_some() {
        "Lose it"
    } else {
        "Lose them"
    };
    window.harness.get_by_label(button).click();
    window.settle();
}

fn cancel(window: &mut Window) {
    window.harness.get_by_label("Cancel").click();
    window.settle();
}

fn the_project_now(window: &Window) -> Project {
    window.application().project().clone()
}

// --- when the question is asked ---

#[test]
fn a_clip_asked_to_last_less_is_made_to_without_a_question_while_it_has_a_frame_for_each_of_its_keyframes() {
    let (mut window, text) = window_with_a_text_object_turned_at(&[0, 10, 20, 30]);

    window.do_this(Command::SetClipDuration { clip: text, frames: 4 });

    assert_eq!(window.application().dialog, None);
    assert_eq!(duration_of(&window, text), 4);
    assert_eq!(keyframes(&window), 4, "none is lost: they are a frame apart now");
    assert!(window.application().has_unsaved_changes());
}

#[test]
fn a_clip_asked_to_last_so_little_that_keyframes_would_be_lost_is_not_changed_and_the_user_is_asked() {
    let (mut window, text) = window_with_a_text_object_turned_at(&[0, 10, 20, 30]);
    let before = the_project_now(&window);

    window.do_this(Command::SetClipDuration { clip: text, frames: 3 });

    assert_eq!(
        the_question_open(&window),
        Some((Command::SetClipDuration { clip: text, frames: 3 }, 1)),
        "four keyframes in three frames: one would be lost"
    );
    assert_eq!(
        window.application().project(),
        &before,
        "nothing is changed until the user says"
    );
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn the_question_says_what_would_be_done_and_how_many_keyframes_would_be_lost() {
    let (mut window, text) = window_with_a_text_object_turned_at(&[0, 10, 20, 30]);

    window.do_this(Command::SetClipDuration { clip: text, frames: 3 });
    assert!(window.harness.query_by_label("Lose 1 keyframe?").is_some());
    assert!(
        window
            .harness
            .query_by_label_contains("Making this clip last that long leaves no room for 1 keyframe in the drawers")
            .is_some()
    );
    assert!(window.harness.query_by_label_contains("so it would be lost").is_some());
    assert!(window.harness.query_by_label("Lose it").is_some());
    cancel(&mut window);

    window.do_this(Command::SetClipDuration { clip: text, frames: 2 });
    assert!(window.harness.query_by_label("Lose 2 keyframes?").is_some());
    assert!(
        window
            .harness
            .query_by_label_contains("so they would be lost")
            .is_some()
    );
    assert!(window.harness.query_by_label("Lose them").is_some());
}

#[test]
fn the_user_is_asked_when_the_text_clip_is_made_to_last_less_in_the_properties_panel_too() {
    let (mut window, text) = window_with_a_text_object_turned_at(&[0, 10, 20, 30]);
    assert_eq!(
        window.application().selected_clip(),
        Some(text),
        "its properties are showing"
    );

    window.set_slider("Shown for", 0.1);

    assert_eq!(
        the_question_open(&window),
        Some((Command::SetClipDuration { clip: text, frames: 3 }, 1)),
        "a tenth of a second is three frames"
    );
    assert_eq!(duration_of(&window, text), 90);
    agree(&mut window);
    assert_eq!(duration_of(&window, text), 3);
    assert_eq!(keyframes(&window), 3);
}

#[test]
fn dragging_the_time_a_text_is_shown_for_down_squeezes_the_keyframes_and_stops_to_ask_where_the_next_step_would_lose_some()
 {
    let (mut window, text) = window_with_a_text_object_turned_at(&[0, 10, 20, 30]);
    let field = window.harness.get_by_label("Shown for").rect().center();

    // Seven tenths of a second a step, from three seconds to well below what four keyframes need.
    window.drag(field, field - egui::vec2(120.0, 0.0), egui::PointerButton::Primary);

    let (change, how_many) =
        the_question_open(&window).expect("a question is asked where a step would lose a keyframe");
    let Command::SetClipDuration { clip, frames } = change else {
        panic!("{change:?}");
    };
    assert_eq!(clip, text);
    assert!(frames < 4, "{frames} frames for four keyframes");
    assert_eq!(how_many, 4 - frames as usize);
    let lasts = duration_of(&window, text);
    assert!(
        lasts >= 4,
        "the steps before it were made, and none was lost: it lasts {lasts}"
    );
    assert_eq!(
        keyframes(&window),
        4,
        "the keyframes are still all there, a frame apart where they have to be"
    );

    agree(&mut window);
    assert_eq!(duration_of(&window, text), frames);
    assert_eq!(keyframes(&window), frames as usize);
}

#[test]
fn a_drag_that_squeezed_the_keyframes_is_one_step_of_the_history_which_undoing_takes_back_to_where_they_were() {
    let (mut window, text) = window_with_a_text_object_turned_at(&[0, 10, 20, 30]);
    let before = the_project_now(&window);
    let field = window.harness.get_by_label("Shown for").rect().center();
    window.drag(field, field - egui::vec2(120.0, 0.0), egui::PointerButton::Primary);
    assert!(
        duration_of(&window, text) < 90,
        "the steps before the question were made"
    );
    cancel(&mut window);
    assert_ne!(window.application().project(), &before);

    window.do_this(Command::Undo);

    assert_eq!(
        window.application().project(),
        &before,
        "one undo takes back the whole drag: how long the clip lasted, and where each keyframe was"
    );
}

#[test]
fn a_change_that_does_not_lose_anything_is_never_asked_about_whatever_it_is() {
    let (mut window, text, video) = window_with_a_text_object_and_a_video_that_each_have_keyframes();
    let back = window.application().project().tracks()[1].id();

    // Longer, the same, a speed that leaves room, a change that cannot be made, and edits that are not about how long things last.
    for command in [
        Command::SetClipDuration {
            clip: text,
            frames: 120,
        },
        Command::SetClipDuration {
            clip: text,
            frames: 120,
        },
        Command::SetClipSpeed {
            clip: video,
            speed_factor: speed(4),
        },
        Command::SetClipSpeed {
            clip: video,
            speed_factor: speed(4),
        },
        Command::SetTrackSpeed {
            track: back,
            speed_factor: speed(2),
        },
        Command::SetClipDuration {
            clip: ClipId::new(TrackId::new(99), 0),
            frames: 1,
        },
        Command::SetProjectFrameRate(FrameRate::new(24, 1).unwrap()),
        Command::AddKeyframe {
            target: AnimationTarget::Clip(text),
            property: PropertyId::Rotation,
            frame: frame(50),
        },
    ] {
        window.do_this(command.clone());
        assert_eq!(window.application().dialog, None, "{command:?}");
    }
    assert_eq!(
        keyframes(&window),
        10,
        "all nine that there were, and the one that was added"
    );
}

// --- the answers ---

#[test]
fn agreeing_makes_the_change_and_loses_the_keyframes_and_undoing_it_brings_them_back() {
    let (mut window, text) = window_with_a_text_object_turned_at(&[0, 10, 20, 30]);
    let before = the_project_now(&window);
    window.do_this(Command::SetClipDuration { clip: text, frames: 3 });

    agree(&mut window);

    assert_eq!(window.application().dialog, None);
    assert_eq!(duration_of(&window, text), 3);
    assert_eq!(keyframes(&window), 3);
    assert!(window.application().has_unsaved_changes());
    assert_eq!(
        window.application().project().problems_with_the_racks(),
        Vec::<String>::new()
    );

    window.do_this(Command::Undo);
    assert_eq!(
        window.application().project(),
        &before,
        "the keyframes and how long it lasts are back"
    );
}

#[test]
fn cancelling_changes_nothing_and_neither_does_closing_the_question_with_escape() {
    let (mut window, text) = window_with_a_text_object_turned_at(&[0, 10, 20, 30]);
    let before = the_project_now(&window);

    window.do_this(Command::SetClipDuration { clip: text, frames: 3 });
    cancel(&mut window);
    assert_eq!(window.application().dialog, None);
    assert_eq!(window.application().project(), &before);

    window.do_this(Command::SetClipDuration { clip: text, frames: 3 });
    assert!(the_question_open(&window).is_some());
    window.press_key(Key::Escape);
    assert_eq!(window.application().dialog, None, "closed");
    assert_eq!(window.application().project(), &before);
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn agreeing_is_for_that_change_only_and_the_next_that_would_lose_keyframes_is_asked_about_too() {
    let (mut window, text) = window_with_a_text_object_turned_at(&[0, 10, 20, 30]);
    window.do_this(Command::SetClipDuration { clip: text, frames: 3 });
    agree(&mut window);
    assert_eq!(keyframes(&window), 3);

    window.do_this(Command::SetClipDuration { clip: text, frames: 2 });

    assert_eq!(
        the_question_open(&window),
        Some((Command::SetClipDuration { clip: text, frames: 2 }, 1)),
        "3 keyframes, 2 frames"
    );
    assert_eq!(keyframes(&window), 3, "until they say");
}

#[test]
fn while_the_question_is_open_what_is_asked_for_by_a_gesture_that_is_still_going_on_is_not_done() {
    let (mut window, text) = window_with_a_text_object_turned_at(&[0, 10, 20, 30]);
    window.do_this(Command::SetClipDuration { clip: text, frames: 3 });
    let before = the_project_now(&window);

    // A mouse button that is held down goes on sending commands, to the lengths the pointer is dragged to.
    for frames in [2, 1, 80, 3] {
        window.do_this(Command::SetClipDuration { clip: text, frames });
    }

    assert_eq!(
        the_question_open(&window),
        Some((Command::SetClipDuration { clip: text, frames: 3 }, 1)),
        "the question is about the first, and is not replaced"
    );
    assert_eq!(window.application().project(), &before, "and none of them was made");
}

#[test]
fn a_question_that_is_open_already_is_not_replaced_by_this_one_and_nothing_is_lost_behind_it() {
    let (mut window, text) = window_with_a_text_object_turned_at(&[0, 10, 20, 30]);
    window.application_mut().dialog = Some(Dialog::Message {
        title: "A message".to_owned(),
        text: "Something to read".to_owned(),
    });
    let before = the_project_now(&window);

    window.do_this(Command::SetClipDuration { clip: text, frames: 3 });

    assert!(
        matches!(window.application().dialog, Some(Dialog::Message { .. })),
        "it is still there"
    );
    assert_eq!(window.application().project(), &before);
}

#[test]
fn the_question_stops_the_playback_so_that_it_does_not_go_on_unseen_behind_it() {
    let (mut window, text) = window_with_a_text_object_turned_at(&[0, 10, 20, 30]);
    let total = window.application().project().total_frame_count();
    window.application_mut().clock.play(std::time::Instant::now(), total);
    assert!(window.application().clock.is_playing());

    window.do_this(Command::SetClipDuration { clip: text, frames: 3 });

    assert!(!window.application().clock.is_playing());
}

#[test]
fn only_one_question_is_asked_for_the_commands_of_one_frame_the_first_that_would_lose_keyframes() {
    let (mut window, text, video) = window_with_a_text_object_and_a_video_that_each_have_keyframes();
    let before = the_project_now(&window);
    let context = egui::Context::default();

    window.application_mut().execute_all(
        vec![
            Command::SetClipDuration { clip: text, frames: 2 },
            Command::SetClipSpeed {
                clip: video,
                speed_factor: speed(100),
            },
        ],
        std::time::Instant::now(),
        &context,
    );

    assert_eq!(
        the_question_open(&window),
        Some((Command::SetClipDuration { clip: text, frames: 2 }, 2))
    );
    assert_eq!(window.application().project(), &before, "neither was made");
}

// --- every way of making something last less ---

#[test]
fn playing_a_clip_so_fast_that_it_has_fewer_frames_than_keyframes_asks_and_losing_some_leaves_a_frame_for_each_of_the_rest()
 {
    let (mut window, _, video) = window_with_a_text_object_and_a_video_that_each_have_keyframes();
    let before = the_project_now(&window);
    assert_eq!(duration_of(&window, video), 300);

    window.do_this(Command::SetClipSpeed {
        clip: video,
        speed_factor: speed(100),
    });

    assert_eq!(
        the_question_open(&window),
        Some((
            Command::SetClipSpeed {
                clip: video,
                speed_factor: speed(100)
            },
            2
        )),
        "300 frames at 100 times the speed are 3 frames: for 5 keyframes"
    );
    assert_eq!(window.application().project(), &before);
    agree(&mut window);
    assert_eq!(duration_of(&window, video), 3);
    assert_eq!(
        keyframes(&window),
        4 + 3,
        "the four of the text, and three of the five of the video"
    );
    assert_eq!(
        window.application().project().problems_with_the_racks(),
        Vec::<String>::new()
    );
}

#[test]
fn playing_a_track_so_fast_that_its_clips_have_fewer_frames_than_keyframes_asks() {
    let (mut window, _, video) = window_with_a_text_object_and_a_video_that_each_have_keyframes();
    let back = window.application().project().tracks()[1].id();
    let before = the_project_now(&window);

    window.do_this(Command::SetTrackSpeed {
        track: back,
        speed_factor: speed(100),
    });

    assert_eq!(
        the_question_open(&window),
        Some((
            Command::SetTrackSpeed {
                track: back,
                speed_factor: speed(100)
            },
            2
        ))
    );
    assert_eq!(window.application().project(), &before);
    agree(&mut window);
    assert_eq!(duration_of(&window, video), 3);
    assert_eq!(window.application().project().track(back).unwrap().speed(), speed(100));
}

#[test]
fn counting_the_project_again_at_a_frame_rate_so_low_that_a_clip_has_fewer_frames_than_keyframes_asks() {
    let (mut window, text) = window_with_a_text_object_turned_at(&[0, 10, 20, 30]);
    let before = the_project_now(&window);
    let one_a_second = FrameRate::new(1, 1).unwrap();

    window.do_this(Command::SetProjectFrameRate(one_a_second));

    assert_eq!(
        the_question_open(&window),
        Some((Command::SetProjectFrameRate(one_a_second), 1)),
        "three seconds at one frame a second are three frames: for four keyframes"
    );
    assert_eq!(window.application().project(), &before);
    assert_eq!(
        window.application().project().frame_rate(),
        FrameRate::new(30, 1).unwrap()
    );
    agree(&mut window);
    assert_eq!(window.application().project().frame_rate(), one_a_second);
    assert_eq!(duration_of(&window, text), 3);
    assert_eq!(keyframes(&window), 3);
}

#[test]
fn putting_a_clip_on_a_track_that_plays_so_fast_that_it_has_fewer_frames_than_keyframes_asks() {
    let (mut window, _, video) = window_with_a_text_object_and_a_video_that_each_have_keyframes();
    window.do_this(Command::AddTrack);
    let fast = window.application().project().tracks()[0].id();
    window.do_this(Command::SetTrackSpeed {
        track: fast,
        speed_factor: speed(100),
    });
    assert_eq!(window.application().dialog, None, "an empty track loses nothing");
    let before = the_project_now(&window);
    let moves = vec![ClipMove {
        clip: video,
        to_track: fast,
        start: frame(0),
    }];

    window.do_this(Command::move_clips(moves.clone()));

    assert_eq!(the_question_open(&window), Some((Command::move_clips(moves), 2)));
    assert_eq!(window.application().project(), &before, "the clip stays where it is");
    agree(&mut window);
    assert_eq!(duration_of(&window, video), 3);
    assert_eq!(
        window.application().project().track_holding(video).unwrap().id(),
        fast,
        "it is on the fast track now"
    );
}

#[test]
fn moving_a_clip_to_a_track_that_plays_at_the_same_speed_is_not_asked_about_however_many_keyframes_it_has() {
    let (mut window, _, video) = window_with_a_text_object_and_a_video_that_each_have_keyframes();
    window.do_this(Command::AddTrack);
    let other = window.application().project().tracks()[0].id();

    window.do_this(Command::move_clips(vec![ClipMove {
        clip: video,
        to_track: other,
        start: frame(10),
    }]));

    assert_eq!(window.application().dialog, None);
    assert_eq!(window.application().project().track_holding(video).unwrap().id(), other);
    assert_eq!(keyframes(&window), 9);
}

// --- what makes a command one of those ---

#[test]
fn the_commands_that_change_how_long_things_last_are_the_ones_that_may_lose_keyframes_and_each_says_what_it_is() {
    let clip = ClipId::new(TrackId::new(0), 0);
    let track = TrackId::new(0);
    let those = [
        Command::SetClipSpeed {
            clip,
            speed_factor: speed(2),
        },
        Command::SetTrackSpeed {
            track,
            speed_factor: speed(2),
        },
        Command::SetClipDuration { clip, frames: 5 },
        Command::SetProjectFrameRate(FrameRate::new(24, 1).unwrap()),
        Command::move_clips(Vec::new()),
    ];
    let words: Vec<&str> = those
        .iter()
        .map(|command| {
            assert!(command.may_lose_keyframes(), "{command:?}");
            command.name_in_a_question_about_losing_keyframes().unwrap()
        })
        .collect();
    let mut different = words.clone();
    different.sort_unstable();
    different.dedup();
    assert_eq!(different.len(), words.len(), "each is said differently: {words:?}");
    for not in [
        Command::SetClipGain {
            clip,
            gain: crate::project::Gain::UNCHANGED,
        },
        Command::DeleteClips(vec![clip]),
        Command::SplitAt {
            frame: frame(3),
            only: None,
        },
        Command::AddTrack,
        Command::Undo,
    ] {
        assert!(!not.may_lose_keyframes(), "{not:?}");
        assert_eq!(not.name_in_a_question_about_losing_keyframes(), None);
    }
}
