//! Tests of dragging clips in the window where there are clip groups: what is dragged stops against the rectangle of a group as it stops
//! against another clip, follows the pointer when it comes back, and is not refused with a message when it is let go of.

use egui::PointerButton;

use super::clip_drag_tests::{in_frame, project_of_tracks};
use super::test_window::Window;
use crate::project::{ClipId, GroupKind, Member};

/// A window with two tracks (the front one first), with these clips: on the front track X at frames 0 to 20 and B at 70 to 90, and on the
/// back track A at 40 to 60 and Z at 130 to 150. A and B are in a clip group, which has the rectangle of frames 40 to 90 across both tracks.
/// (Everything is in view: the strips show about 170 frames.)
fn window_with_a_clip_group_and_clips_that_are_not_in_it() -> Window {
    let mut project = project_of_tracks(&[&[(0, 20), (50, 20)], &[(40, 20), (70, 20)]]);
    let b = project.tracks()[0].placements().nth(1).unwrap().id;
    let a = project.tracks()[1].placements().next().unwrap().id;
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(a), Member::Clip(b)])
        .unwrap();
    let mut window = Window::showing(project);
    window.application_mut().note_everything_is_saved();
    window
}

fn starts_on_track(window: &Window, position: usize) -> Vec<u64> {
    window.application().project().tracks()[position]
        .placements()
        .map(|placement| placement.first_timeline_frame.value())
        .collect()
}

/// Whether the clip `clip` is directly in the clip group of the project (there is one).
fn is_in_the_clip_group(window: &Window, clip: ClipId) -> bool {
    let project = window.application().project();
    project
        .groups()
        .iter()
        .any(|group| group.kind() == GroupKind::ClipGroup && group.has_as_a_member(Member::Clip(clip)))
}

#[test]
fn a_clip_dragged_into_the_rectangle_of_a_clip_group_goes_in_as_far_as_the_clip_in_its_way_and_follows_the_pointer_when_it_comes_back()
 {
    let mut window = window_with_a_clip_group_and_clips_that_are_not_in_it();
    let x = window.clip(0, 0);
    let from = in_frame(&window, 0, 10);
    assert!(!is_in_the_clip_group(&window, x));

    // 60 frames to the right: the rectangle begins at frame 40, 20 frames on, and the clip is in the group while it is in it. B, which is in
    // the group, is at 70: that is as far as it goes, 50 to 70.
    window.drag_and_hold(from, in_frame(&window, 0, 70), PointerButton::Primary);
    window.settle();
    assert_eq!(starts_on_track(&window, 0), vec![50, 70], "up against B");
    assert!(
        is_in_the_clip_group(&window, x),
        "it is in the rectangle of the group, and so in the group"
    );

    // The pointer comes back to 5 frames past where it was taken hold of: the clip follows, and is out of the group again.
    window.move_pointer_to(in_frame(&window, 0, 15));
    window.settle();
    assert_eq!(starts_on_track(&window, 0), vec![5, 70]);
    assert!(
        !is_in_the_clip_group(&window, x),
        "it is not in the rectangle, which did not move"
    );

    // And is let go of where it is, which is not refused: it is where it can be.
    window.release(in_frame(&window, 0, 15), PointerButton::Primary);
    window.settle();
    assert_eq!(starts_on_track(&window, 0), vec![5, 70]);
    assert!(!is_in_the_clip_group(&window, x));
    assert!(
        window.application().dialog.is_none(),
        "stopping at what is in the way is not a refusal that is said"
    );
}

#[test]
fn a_clip_let_go_of_in_the_rectangle_of_a_clip_group_is_in_the_group_and_undoing_the_drag_puts_it_back_out_of_it() {
    let mut window = window_with_a_clip_group_and_clips_that_are_not_in_it();
    let x = window.clip(0, 0);
    let before = window.application().project().clone();

    window.drag(
        in_frame(&window, 0, 10),
        in_frame(&window, 0, 40),
        PointerButton::Primary,
    );
    window.settle();

    assert_eq!(
        starts_on_track(&window, 0),
        vec![30, 70],
        "30 to 50, which is in the rectangle that begins at 40"
    );
    assert!(is_in_the_clip_group(&window, x));

    window.do_this(crate::application::commands::Command::Undo);
    assert_eq!(
        window.application().project(),
        &before,
        "one step takes the whole drag back"
    );
    assert!(!is_in_the_clip_group(&window, x));
}

#[test]
fn a_clip_dragged_far_past_a_clip_group_goes_in_as_far_as_the_clip_in_its_way_and_is_in_the_group_there() {
    let mut window = window_with_a_clip_group_and_clips_that_are_not_in_it();
    let x = window.clip(0, 0);
    let from = in_frame(&window, 0, 10);

    // (100 frames on is where the clip would be after the rectangle, which ends at 90, on the front track: B is on the way.)
    window.drag(from, in_frame(&window, 0, 110), PointerButton::Primary);

    assert_eq!(starts_on_track(&window, 0), vec![50, 70]);
    assert!(is_in_the_clip_group(&window, x));
}

#[test]
fn a_clip_of_a_clip_group_dragged_to_where_the_group_would_take_in_another_clip_stops_before_it() {
    let mut window = window_with_a_clip_group_and_clips_that_are_not_in_it();
    // B is at 70 to 90 and the clip Z is on the back track from 130: B can go 40 frames on, to 110 to 130, and no further without the
    // rectangle taking Z in.
    let from = in_frame(&window, 0, 80);

    window.drag(from, in_frame(&window, 0, 140), PointerButton::Primary);

    assert_eq!(
        starts_on_track(&window, 0),
        vec![0, 110],
        "up against Z's frame, and not past it"
    );
    assert_eq!(starts_on_track(&window, 1), vec![40, 130], "nothing else moved");
    assert!(window.application().dialog.is_none());
}
