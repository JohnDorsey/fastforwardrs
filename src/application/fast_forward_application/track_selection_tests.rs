// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of selecting a track by clicking its title box: the track is selected, and not the clips on it.

use egui::{Key, Modifiers, Rect, Shape};

use super::test_window::{Window, project_with_a_video_and_a_sound};
use crate::application::commands::Command;
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::application::theme;

fn window_with_a_video_and_a_sound() -> Window {
    Window::showing(project_with_a_video_and_a_sound(300, 5))
}

/// Clicks the title of the track at `position`, which is a box that can be typed in: it is selected, and it is
/// being typed in.
fn click_the_title_box_of_track(window: &mut Window, position: usize) {
    window.click(window.title_box_of_track(position).center());
}

/// Clicks the box of the track at `position` at its edge, beside the title (the title is a box of its own,
/// in it), so that the track is selected without the title being typed in.
fn click_beside_the_title_of_track(window: &mut Window, position: usize) {
    window.select_the_track_at(position);
}

/// The outlines in the colour of the selection that were drawn round the boxes at the left of the tracks, as
/// the rectangle each of them is round.
fn outlines_round_title_boxes(window: &Window) -> Vec<Rect> {
    let first_strip = window.strip_of_track(0);
    window
        .harness
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Rect(rectangle)
                if rectangle.stroke.color == theme::SELECTION_OUTLINE
                    && rectangle.fill.a() == 0
                    && rectangle.rect.right() < first_strip.left() =>
            {
                Some(rectangle.rect)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn clicking_the_title_box_of_a_track_selects_that_track_and_not_the_clips_on_it() {
    let mut window = window_with_a_video_and_a_sound();
    let track = window.application().project().tracks()[1].id();

    click_the_title_box_of_track(&mut window, 1);

    assert_eq!(window.application().selected_items(), vec![SelectionItem::Track(track)]);
    assert_eq!(
        window.application().selected_clips(),
        vec![],
        "not the clip that is on it"
    );
    assert_eq!(window.application().selected_clip(), None);
}

#[test]
fn clicking_the_box_of_a_track_beside_its_title_selects_it_too_and_does_not_start_typing_the_title() {
    let mut window = window_with_a_video_and_a_sound();
    let track = window.application().project().tracks()[0].id();

    click_beside_the_title_of_track(&mut window, 0);

    assert_eq!(window.application().selected_items(), vec![SelectionItem::Track(track)]);
    window.press_key(Key::Space);
    assert!(
        window.application().is_playing(),
        "the keys are still the window's: no title is being typed in"
    );
}

#[test]
fn clicking_the_title_of_a_track_selects_it_and_what_is_typed_after_that_is_the_title() {
    let mut window = window_with_a_video_and_a_sound();
    let track = window.application().project().tracks()[0].id();

    click_the_title_box_of_track(&mut window, 0);
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Track(track)]);
    window.harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    window.harness.step();
    window.harness.event(egui::Event::Text("Sounds".to_owned()));
    window.settle();

    assert_eq!(window.application().project().tracks()[0].title(), "Sounds");
    assert_eq!(
        window.application().selected_items(),
        vec![SelectionItem::Track(track)],
        "still selected"
    );
}

#[test]
fn clicking_a_clip_or_another_track_or_nothing_lets_go_of_the_track() {
    let mut window = window_with_a_video_and_a_sound();
    let (front, back) = (
        window.application().project().tracks()[0].id(),
        window.application().project().tracks()[1].id(),
    );

    click_the_title_box_of_track(&mut window, 0);
    click_the_title_box_of_track(&mut window, 1);
    assert_eq!(
        window.application().selected_items(),
        vec![SelectionItem::Track(back)],
        "only the other"
    );

    let clip = window.clip(0, 0);
    window.click(window.middle_of_the_strip_at(0, 20));
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Clip(clip)]);
    assert!(
        !window
            .application()
            .selected_items()
            .contains(&SelectionItem::Track(front))
    );

    click_the_title_box_of_track(&mut window, 0);
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Track(front)]);
    window.click(window.strip_of_track(0).right_center() - egui::vec2(60.0, 0.0)); // an empty part of the strip
    assert_eq!(window.application().selected_items(), vec![], "nothing");
}

#[test]
fn with_control_held_a_track_is_added_to_what_is_selected_and_let_go_of_again() {
    let mut window = window_with_a_video_and_a_sound();
    let clip = window.clip(1, 0);
    window.click(window.middle_of_the_strip_at(1, 20));
    let (front, back) = (
        window.application().project().tracks()[0].id(),
        window.application().project().tracks()[1].id(),
    );

    window.click_with_control(window.title_box_of_track(0).center());
    assert_eq!(
        window.application().selected_items(),
        vec![SelectionItem::Clip(clip), SelectionItem::Track(front)]
    );
    window.click_with_control(window.title_box_of_track(1).center());
    assert_eq!(window.application().selected_items().len(), 3);
    assert!(
        window
            .application()
            .selected_items()
            .contains(&SelectionItem::Track(back))
    );
    window.click_with_control(window.title_box_of_track(0).center());
    assert_eq!(
        window.application().selected_items(),
        vec![SelectionItem::Clip(clip), SelectionItem::Track(back)]
    );
}

#[test]
fn the_box_of_a_selected_track_has_the_outline_of_a_selection_and_the_box_of_another_has_none() {
    let mut window = window_with_a_video_and_a_sound();
    assert_eq!(outlines_round_title_boxes(&window), vec![]);

    click_the_title_box_of_track(&mut window, 1);

    let outlines = outlines_round_title_boxes(&window);
    assert_eq!(outlines.len(), 1);
    assert!(
        outlines[0].contains(window.title_box_of_track(1).center())
            && !outlines[0].contains(window.title_box_of_track(0).center()),
        "round the box of the second track: {outlines:?}"
    );
    window.press_key(Key::Escape);
    assert_eq!(outlines_round_title_boxes(&window), vec![], "gone with Escape");
}

#[test]
fn selecting_a_track_is_not_an_edit_and_there_is_nothing_to_undo_after_it() {
    let mut window = window_with_a_video_and_a_sound();
    window.application_mut().note_everything_is_saved();

    click_the_title_box_of_track(&mut window, 0);

    assert!(!window.application().can_undo());
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn deleting_a_track_lets_go_of_it_and_so_does_undoing_what_made_it() {
    let mut window = window_with_a_video_and_a_sound();
    let track = window.application().project().tracks()[0].id();
    click_the_title_box_of_track(&mut window, 0);

    window.do_this(Command::DeleteTrack(track));

    assert_eq!(window.application().selected_items(), vec![]);
}

#[test]
fn the_delete_key_does_not_take_a_selected_track_away_and_s_cuts_nothing_but_shift_and_s_cuts_everywhere() {
    let mut window = window_with_a_video_and_a_sound();
    window.move_current_frame_to(40);
    click_beside_the_title_of_track(&mut window, 1);
    assert_eq!(window.application().selected_items().len(), 1, "the track is selected");

    window.press_key(Key::Delete);
    assert_eq!(
        window.application().project().tracks().len(),
        2,
        "the track is still there"
    );
    assert_eq!(
        window.application().project().tracks()[1].entries().len(),
        1,
        "and so is its clip"
    );

    window.press_key(Key::S);
    assert_eq!(
        window.application().project().tracks()[1].entries().len(),
        1,
        "S cuts the selected clips, and a track is not one: nothing was cut"
    );

    window.press_key_with(Modifiers::SHIFT, Key::S);
    assert_eq!(
        window.application().project().tracks()[0].entries().len(),
        2,
        "the sound was cut"
    );
    assert_eq!(
        window.application().project().tracks()[1].entries().len(),
        2,
        "and the video"
    );
}

#[test]
fn a_track_is_selected_by_a_command_too_and_one_that_is_not_there_is_not() {
    let mut window = window_with_a_video_and_a_sound();
    let track = window.application().project().tracks()[1].id();

    window.do_this(Command::select_track(track, SelectionChange::Replace));
    assert_eq!(window.application().selected_items(), vec![SelectionItem::Track(track)]);

    window.do_this(Command::select_track(
        crate::project::TrackId::new(99),
        SelectionChange::Replace,
    ));
    assert_eq!(
        window.application().selected_items(),
        vec![SelectionItem::Track(track)],
        "as it was"
    );

    window.do_this(Command::SelectItems {
        items: vec![],
        change: SelectionChange::Replace,
    });
    assert_eq!(
        window.application().selected_items(),
        vec![],
        "asking for nothing is how a rectangle that goes round nothing lets go of what it had"
    );
}
