//! Tests of dragging clips in the window where there are clip groups: what is dragged stops against the rectangle of a group as it stops
//! against another clip, follows the pointer when it comes back, and is not refused with a message when it is let go of.

use egui::PointerButton;

use super::clip_drag_tests::{in_frame, project_of_tracks};
use super::test_window::Window;
use crate::project::{GroupKind, Member};

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

#[test]
fn a_clip_dragged_into_the_rectangle_of_a_clip_group_stops_at_its_edge_and_follows_the_pointer_when_it_comes_back() {
    let mut window = window_with_a_clip_group_and_clips_that_are_not_in_it();
    let from = in_frame(&window, 0, 10);

    // 60 frames to the right: but the rectangle begins at frame 40, 20 frames on. (The clip B, which is in the group, is at 70.)
    window.drag_and_hold(from, in_frame(&window, 0, 70), PointerButton::Primary);
    window.settle();
    assert_eq!(
        starts_on_track(&window, 0),
        vec![20, 70],
        "up against the rectangle, which did not move"
    );

    // The pointer comes back to 5 frames past where it was taken hold of: the clip follows.
    window.move_pointer_to(in_frame(&window, 0, 15));
    window.settle();
    assert_eq!(starts_on_track(&window, 0), vec![5, 70]);

    // And is let go of where it is, which is not refused: it is where it can be.
    window.release(in_frame(&window, 0, 15), PointerButton::Primary);
    window.settle();
    assert_eq!(starts_on_track(&window, 0), vec![5, 70]);
    assert!(
        window.application().dialog.is_none(),
        "stopping at what is in the way is not a refusal that is said"
    );
}

#[test]
fn a_clip_dragged_far_past_a_clip_group_does_not_come_out_on_the_other_side_of_it() {
    let mut window = window_with_a_clip_group_and_clips_that_are_not_in_it();
    let from = in_frame(&window, 0, 10);

    // (100 frames on is where the clip would be after the rectangle, which ends at 90, on the front track.)
    window.drag(from, in_frame(&window, 0, 110), PointerButton::Primary);

    assert_eq!(starts_on_track(&window, 0), vec![20, 70]);
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
