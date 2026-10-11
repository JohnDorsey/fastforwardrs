//! Tests of dragging clips along their tracks, and of drawing a rectangle round clips to select them.

use egui::{CursorIcon, Key, Modifiers, PointerButton, Pos2, Shape, vec2};

use super::test_window::{VIDEO, Window, project_of};
use crate::application::commands::Command;
use crate::application::theme;
use crate::project::{
    Clip, ClipLocation, MediaClip, Project, SourceFrameIndex, SpeedFactor, SpeedIndicatorLayout, TextClip, TextObject,
    TimelineFrameIndex, Track, TrackId,
};

/// A video clip of `frames` frames, from the video the test windows have.
fn video_clip(frames: u64) -> Clip {
    Clip::Video(
        MediaClip::new(
            VIDEO,
            SourceFrameIndex::new(0),
            SourceFrameIndex::new(frames),
            SpeedFactor::NORMAL,
        )
        .unwrap(),
    )
}

/// A project whose tracks (the front one first) have clips of these lengths, each with the number
/// of empty frames before it.
fn project_of_tracks(tracks: &[&[(u64, u64)]]) -> Project {
    let built: Vec<Track> = tracks
        .iter()
        .enumerate()
        .map(|(number, clips)| {
            Track::new(TrackId::new(number as u32), format!("track {number}"))
                .with_clips(clips.iter().map(|&(gap, frames)| (gap, video_clip(frames))))
        })
        .collect();
    let source = super::test_window::video_source(1000);
    Project::from_parts(vec![source], built, SpeedIndicatorLayout::DEFAULT).unwrap()
}

/// One track with clips at frames 20..50, 65..95 and 110..140: all in view at the normal zoom.
fn window_with_three_clips() -> Window {
    Window::showing(project_of_tracks(&[&[(20, 30), (15, 30), (15, 30)]]))
}

fn starts_on_track(window: &Window, position: usize) -> Vec<u64> {
    window.application().project().tracks()[position]
        .placements()
        .map(|placement| placement.first_timeline_frame.value())
        .collect()
}

/// A point on the strip of the track at `position`, in the middle of timeline frame `frame` (so that
/// moving the point by the width of some frames moves it by as many).
fn in_frame(window: &Window, position: usize, frame: u64) -> Pos2 {
    let start = window.x_of_frame_on_the_strip(position, frame);
    let width = window.x_of_frame_on_the_strip(position, frame + 1) - start;
    Pos2::new(start + width / 2.0, window.strip_of_track(position).center().y)
}

fn drag_by_frames(window: &mut Window, position: usize, from_frame: u64, frames: i64) {
    let from = in_frame(window, position, from_frame);
    let to = in_frame(window, position, (from_frame as i64 + frames) as u64);
    window.drag(from, to, PointerButton::Primary);
}

fn select_with_control(window: &mut Window, position: usize, frame: u64) {
    let point = in_frame(window, position, frame);
    window.click_with_control(point);
}

// --- moving a clip ---

#[test]
fn dragging_a_clip_along_its_track_moves_it_as_many_frames_as_the_pointer_goes() {
    let mut window = window_with_three_clips();

    drag_by_frames(&mut window, 0, 80, 10); // the middle clip, which is at 65..95

    assert_eq!(starts_on_track(&window, 0), vec![20, 75, 110]);
    let moved = window.application().project().tracks()[0].placements().nth(1).unwrap();
    assert_eq!(moved.timeline_frame_count(), 30, "it is as long as it was");
    drag_by_frames(&mut window, 0, 90, -14);
    assert_eq!(
        starts_on_track(&window, 0),
        vec![20, 61, 110],
        "and back and a little more"
    );
}

#[test]
fn a_clip_stops_against_the_clip_next_to_it_and_follows_the_pointer_when_it_comes_back() {
    let mut window = window_with_three_clips();
    let from = in_frame(&window, 0, 80);

    // 40 frames to the right: but there are only 15 until the clip at 110.
    window.drag_and_hold(from, in_frame(&window, 0, 120), PointerButton::Primary);
    window.settle();
    assert_eq!(
        starts_on_track(&window, 0),
        vec![20, 80, 110],
        "up against the next clip, which did not move"
    );

    // The pointer comes back to 5 frames past where it was taken hold of: the clip follows.
    window.move_pointer_to(in_frame(&window, 0, 85));
    window.settle();
    assert_eq!(starts_on_track(&window, 0), vec![20, 70, 110]);
    window.release(in_frame(&window, 0, 85), PointerButton::Primary);
}

#[test]
fn a_clip_stops_at_the_start_of_the_timeline() {
    let mut window = window_with_three_clips();
    drag_by_frames(&mut window, 0, 35, -30); // the first clip, at 20..50
    assert_eq!(starts_on_track(&window, 0), vec![0, 65, 110]);
}

#[test]
fn a_clip_dragged_where_it_cannot_go_at_all_is_not_an_edit() {
    let mut window = window_with_three_clips();
    window.application_mut().note_everything_is_saved();
    // The middle clip, to the left: 15 frames of room, so go the other way after using it up... first
    // the first clip to the right: it has 15 frames of room.
    drag_by_frames(&mut window, 0, 35, 15);
    assert_eq!(starts_on_track(&window, 0), vec![35, 65, 110]);
    window.application_mut().note_everything_is_saved();

    drag_by_frames(&mut window, 0, 45, 10); // against the next clip: no room at all
    assert_eq!(starts_on_track(&window, 0), vec![35, 65, 110]);
    assert!(
        !window.application().has_unsaved_changes(),
        "nothing moved, so nothing was changed"
    );
}

#[test]
fn a_plain_click_on_a_clip_selects_it_and_does_not_move_it() {
    let mut window = window_with_three_clips();
    window.application_mut().note_everything_is_saved();

    window.click(in_frame(&window, 0, 80));

    assert_eq!(window.application().selected_clips(), vec![window.clip(0, 1)]);
    assert_eq!(starts_on_track(&window, 0), vec![20, 65, 110]);
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn one_drag_is_one_step_of_the_history_which_undoing_takes_back_whole() {
    let mut window = window_with_three_clips();
    window.application_mut().note_everything_is_saved();

    drag_by_frames(&mut window, 0, 80, 10);
    assert_eq!(starts_on_track(&window, 0), vec![20, 75, 110]);

    window.harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    window.harness.step();
    window.settle();
    assert_eq!(
        starts_on_track(&window, 0),
        vec![20, 65, 110],
        "back where it was, in one step"
    );
    assert!(!window.application().can_undo());
}

#[test]
fn the_pointer_is_a_grabbing_hand_while_a_clip_is_dragged_and_not_after() {
    let mut window = window_with_three_clips();
    let from = in_frame(&window, 0, 80);
    window.drag_and_hold(from, in_frame(&window, 0, 85), PointerButton::Primary);
    window.harness.step();
    assert_eq!(
        window.harness.output().platform_output.cursor_icon,
        CursorIcon::Grabbing
    );

    window.release(in_frame(&window, 0, 85), PointerButton::Primary);
    window.harness.step();
    assert_ne!(
        window.harness.output().platform_output.cursor_icon,
        CursorIcon::Grabbing
    );
}

// --- what is selected while dragging ---

#[test]
fn dragging_a_clip_that_is_not_selected_selects_it_and_moves_only_it() {
    let mut window = window_with_three_clips();
    window.click(in_frame(&window, 0, 35)); // the first clip is selected

    drag_by_frames(&mut window, 0, 80, 10); // and the middle one is dragged

    assert_eq!(
        window.application().selected_clips(),
        vec![window.clip(0, 1)],
        "the one dragged"
    );
    assert_eq!(
        starts_on_track(&window, 0),
        vec![20, 75, 110],
        "and only that one moved"
    );
}

#[test]
fn dragging_one_of_the_selected_clips_moves_all_of_them_by_the_same_amount() {
    let mut window = window_with_three_clips();
    window.click(in_frame(&window, 0, 35));
    select_with_control(&mut window, 0, 125); // the first and the third are selected

    drag_by_frames(&mut window, 0, 125, 10);

    assert_eq!(starts_on_track(&window, 0), vec![30, 65, 120]);
    assert_eq!(window.application().selected_clips().len(), 2, "they stay selected");
}

#[test]
fn selected_clips_that_move_together_stop_when_the_first_of_them_is_stopped() {
    let mut window = window_with_three_clips();
    window.click(in_frame(&window, 0, 35));
    select_with_control(&mut window, 0, 125); // the first (20..50) and the third (110..140)

    // The first has 15 frames of room to the right, the third no limit: 40 frames asked for.
    drag_by_frames(&mut window, 0, 125, 40);

    assert_eq!(
        starts_on_track(&window, 0),
        vec![35, 65, 125],
        "both went the 15 frames that the first could"
    );
}

#[test]
fn selected_clips_that_move_together_also_stop_together_at_the_start_of_the_timeline() {
    // Clips at 20..50 and 130..160, with plenty of room between them.
    let mut window = Window::showing(project_of_tracks(&[&[(20, 30), (80, 30)]]));
    window.click(in_frame(&window, 0, 35));
    select_with_control(&mut window, 0, 145);

    drag_by_frames(&mut window, 0, 145, -60);

    assert_eq!(
        starts_on_track(&window, 0),
        vec![0, 110],
        "the first went the 20 frames it could and the second only as far with it"
    );
}

#[test]
fn dragging_a_clip_that_is_not_selected_with_control_held_adds_it_and_moves_it_with_the_others() {
    let mut window = window_with_three_clips();
    window.click(in_frame(&window, 0, 35)); // the first clip is selected
    window.harness.event(egui::Event::ModifiersChanged(Modifiers::COMMAND));
    window.harness.step();

    // Dragged with Control held: the third clip joins the first and goes with it.
    let from = in_frame(&window, 0, 125);
    window.drag(from, in_frame(&window, 0, 130), PointerButton::Primary);
    window.harness.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    window.harness.step();

    assert_eq!(starts_on_track(&window, 0), vec![25, 65, 115]);
    assert_eq!(window.application().selected_clips().len(), 2);
}

#[test]
fn clips_on_different_tracks_that_are_selected_move_together() {
    let mut window = Window::showing(project_of_tracks(&[&[(10, 30)], &[(40, 30), (10, 30)]]));
    window.click(in_frame(&window, 0, 20)); // the clip on the front track
    select_with_control(&mut window, 1, 60); // and the first on the one behind

    drag_by_frames(&mut window, 1, 60, 8);

    assert_eq!(starts_on_track(&window, 0), vec![18]);
    assert_eq!(
        starts_on_track(&window, 1),
        vec![48, 80],
        "the second clip behind did not move"
    );

    // Dragged on, the clip behind stops against that one (it has 2 frames to go) and the front one with it.
    drag_by_frames(&mut window, 1, 70, 12);
    assert_eq!(starts_on_track(&window, 0), vec![20]);
    assert_eq!(starts_on_track(&window, 1), vec![50, 80]);
}

#[test]
fn a_text_object_is_dragged_like_any_clip() {
    let mut window = Window::showing(project_of(60));
    window.do_this(Command::NewTextObject {
        track: window.application().project().tracks()[0].id(),
        frame: TimelineFrameIndex::new(100),
    });
    window.press_key(Key::Escape); // closes the text window if it is open: the keys are the window's
    let text_start = starts_on_track(&window, 0)[1];
    assert_eq!(text_start, 100);

    drag_by_frames(&mut window, 0, 120, 25);

    assert_eq!(starts_on_track(&window, 0), vec![0, 125]);
    let text = window.application().project().tracks()[0].placements().nth(1).unwrap();
    assert!(
        text.clip
            .as_text()
            .is_some_and(|text: &TextClip| text.object == TextObject::default())
    );
}

// --- the rectangle ---

/// Two tracks, each with a clip at 0..30, and the one behind with another at 45..75: empty beyond.
fn window_with_clips_on_two_tracks() -> Window {
    Window::showing(project_of_tracks(&[&[(0, 30)], &[(0, 30), (15, 30)]]))
}

fn drag_with_control(window: &mut Window, from: Pos2, to: Pos2) {
    window.harness.event(egui::Event::ModifiersChanged(Modifiers::COMMAND));
    window.harness.step();
    window.drag(from, to, PointerButton::Primary);
    window.harness.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    window.harness.step();
}

fn selected_places(window: &Window) -> Vec<(usize, usize)> {
    let selected = window.application().selected_clips();
    let mut places = Vec::new();
    for (position, track) in window.application().project().tracks().iter().enumerate() {
        for (index, placement) in track.placements().enumerate() {
            let location = ClipLocation {
                track: track.id(),
                clip: placement.id,
            };
            if selected.contains(&location) {
                places.push((position, index));
            }
        }
    }
    places
}

#[test]
fn a_rectangle_dragged_from_an_empty_place_selects_the_clips_it_touches_on_any_track() {
    let mut window = window_with_clips_on_two_tracks();
    // From beyond the end of the clips (frame 100) up and to the left, to the middle of the second
    // clip on the front track: it takes in the second clip behind and the first of the front, and the
    // first behind too (which is below the rectangle: no).
    let from = in_frame(&window, 1, 100);
    let to = in_frame(&window, 0, 20);

    window.drag(from, to, PointerButton::Primary);

    assert_eq!(selected_places(&window), vec![(0, 0), (1, 0), (1, 1)]);
}

#[test]
fn a_rectangle_over_only_part_of_the_clips_selects_only_those() {
    let mut window = window_with_clips_on_two_tracks();
    // Along the track behind only, from frame 100 back to frame 60 (the second clip there is 45..75).
    let from = in_frame(&window, 1, 100);
    let to = Pos2::new(in_frame(&window, 1, 60).x, from.y + 10.0);

    window.drag(from, to, PointerButton::Primary);

    assert_eq!(selected_places(&window), vec![(1, 1)]);
}

#[test]
fn a_rectangle_round_nothing_selects_nothing_and_lets_go_of_what_was_selected() {
    let mut window = window_with_clips_on_two_tracks();
    window.click(in_frame(&window, 0, 10));
    assert_eq!(selected_places(&window), vec![(0, 0)]);

    let from = in_frame(&window, 1, 100);
    window.drag(from, from + vec2(40.0, 5.0), PointerButton::Primary);

    assert_eq!(selected_places(&window), vec![]);
}

#[test]
fn with_control_held_the_clips_that_were_selected_stay_selected() {
    let mut window = window_with_clips_on_two_tracks();
    window.click(in_frame(&window, 0, 10));
    let from = in_frame(&window, 1, 100);
    let to = Pos2::new(in_frame(&window, 1, 60).x, from.y + 10.0);

    drag_with_control(&mut window, from, to);

    assert_eq!(selected_places(&window), vec![(0, 0), (1, 1)]);
}

#[test]
fn a_rectangle_can_be_started_in_the_gap_between_two_tracks() {
    let mut window = window_with_clips_on_two_tracks();
    let in_the_gap = Pos2::new(
        in_frame(&window, 1, 100).x,
        (window.strip_of_track(0).bottom() + window.strip_of_track(1).top()) / 2.0,
    );
    let to = in_frame(&window, 0, 10);

    window.drag(in_the_gap, to, PointerButton::Primary);

    assert!(
        selected_places(&window).contains(&(0, 0)),
        "{:?}",
        selected_places(&window)
    );
}

#[test]
fn a_rectangle_goes_on_selecting_when_the_pointer_leaves_the_tracks() {
    let mut window = window_with_clips_on_two_tracks();
    let from = in_frame(&window, 1, 100);
    let above_the_timeline = Pos2::new(in_frame(&window, 0, 10).x, window.rectangle_of("Ruler").top() - 30.0);

    window.drag(from, above_the_timeline, PointerButton::Primary);

    assert_eq!(selected_places(&window), vec![(0, 0), (1, 0), (1, 1)]);
}

#[test]
fn the_rectangle_is_drawn_while_it_is_dragged_and_is_gone_when_the_button_is_let_go() {
    let mut window = window_with_clips_on_two_tracks();
    let from = in_frame(&window, 1, 100);
    let to = in_frame(&window, 0, 20);
    let band_is_drawn = |window: &Window| {
        window.shapes_drawn().into_iter().any(|shape| match shape {
            Shape::Rect(rectangle) => rectangle.fill == theme::RUBBER_BAND_FILL,
            _ => false,
        })
    };
    assert!(!band_is_drawn(&window));

    window.drag_and_hold(from, to, PointerButton::Primary);
    window.harness.step();
    assert!(band_is_drawn(&window), "drawn while dragging");
    let band = window
        .shapes_drawn()
        .into_iter()
        .find_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.fill == theme::RUBBER_BAND_FILL => Some(rectangle.rect),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        (band.left_bottom(), band.right_top()),
        (Pos2::new(to.x, from.y), Pos2::new(from.x, to.y)),
        "from where the button was pressed to where the pointer is, though the drag began a few points on"
    );

    window.release(to, PointerButton::Primary);
    window.settle();
    assert!(!band_is_drawn(&window), "and gone after");
}

#[test]
fn dragging_from_a_clip_moves_the_clip_and_draws_no_rectangle() {
    let mut window = window_with_clips_on_two_tracks();
    let from = in_frame(&window, 1, 60);
    window.drag_and_hold(from, in_frame(&window, 1, 70), PointerButton::Primary);
    window.harness.step();
    assert!(
        !window.shapes_drawn().into_iter().any(|shape| matches!(
            shape,
            Shape::Rect(rectangle) if rectangle.fill == theme::RUBBER_BAND_FILL
        )),
        "no rectangle"
    );
    window.release(in_frame(&window, 1, 70), PointerButton::Primary);
    window.settle();
    assert_eq!(starts_on_track(&window, 1), vec![0, 55]);
}

#[test]
fn clicking_an_empty_place_selects_nothing_in_a_gap_between_tracks_too() {
    let mut window = window_with_clips_on_two_tracks();
    window.click(in_frame(&window, 0, 10));
    assert_eq!(selected_places(&window).len(), 1);
    let in_the_gap = Pos2::new(
        in_frame(&window, 1, 100).x,
        (window.strip_of_track(0).bottom() + window.strip_of_track(1).top()) / 2.0,
    );

    window.click(in_the_gap);

    assert_eq!(selected_places(&window), vec![]);
}

#[test]
fn selecting_with_a_rectangle_is_not_an_edit_and_not_something_to_undo() {
    let mut window = window_with_clips_on_two_tracks();
    window.application_mut().note_everything_is_saved();

    window.drag(
        in_frame(&window, 1, 100),
        in_frame(&window, 0, 20),
        PointerButton::Primary,
    );

    assert!(!window.application().can_undo());
    assert!(!window.application().has_unsaved_changes());
}
