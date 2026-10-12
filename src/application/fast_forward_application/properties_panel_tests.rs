//! Tests of the properties panel: what it shows for what is selected, and what its controls do.

use egui::{Key, Modifiers, Pos2, Rect, Shape};
use egui_kittest::kittest::{NodeT, Queryable};

use super::test_window::{
    MARGIN_THE_TEST_WINDOW_HAS_ROUND_ITS_EDGES, SOUND, VIDEO, WINDOW_SIZE, Window, project_cut_into_three, project_of,
    project_with_a_video_and_a_sound,
};
use super::{SourceFileState, SourceStatus};
use crate::application::commands::Command;
use crate::application::selection::SelectionItem;
use crate::application::theme;
use crate::project::{Gain, SpeedFactor};

/// A window showing a video of 90 frames cut into three clips of 30, all in view.
fn window_with_three_clips() -> Window {
    let mut window = Window::showing(project_cut_into_three(90));
    window.application_mut().note_everything_is_saved();
    window.settle();
    window
}

fn select_the_clip_at(window: &mut Window, position: usize, frame: u64) {
    let point = window.middle_of_the_strip_at(position, frame);
    window.click(point);
}

fn right_click_the_clip_at(window: &mut Window, position: usize, frame: u64) {
    let point = window.middle_of_the_strip_at(position, frame);
    window.right_click(point);
}

fn slider_value(window: &Window, label: &str) -> f64 {
    window
        .harness
        .get_by_label(label)
        .accesskit_node()
        .numeric_value()
        .unwrap_or_else(|| panic!("{label} is a slider with a value"))
}

fn is_shown(window: &Window, label: &str) -> bool {
    window.harness.query_by_label(label).is_some()
}

const HINT: &str = "Select a clip or a track to see its properties.";

// --- the panels of the window ---

/// The strip across the top of the window that the menus are in: drawn in the colour of panels, and the whole
/// width of the window.
fn the_menu_bar(window: &Window) -> Rect {
    window
        .harness
        .output()
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            Shape::Rect(rectangle)
                if rectangle.fill == theme::PANEL_BACKGROUND
                    && rectangle.rect.width()
                        >= WINDOW_SIZE[0] - 2.0 * MARGIN_THE_TEST_WINDOW_HAS_ROUND_ITS_EDGES - 1.0
                    && rectangle.rect.height() < 60.0 =>
            {
                Some(rectangle.rect)
            }
            _ => None,
        })
        .expect("the menu bar is drawn")
}

#[test]
fn the_window_is_divided_into_three_panels_each_six_points_from_the_edges_of_the_window_and_from_its_neighbours() {
    let window = Window::showing(project_of(300));
    let panels = window.panels();
    assert_eq!(
        panels.len(),
        3,
        "the timeline, the properties and the preview: {panels:?}"
    );
    let screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(WINDOW_SIZE[0], WINDOW_SIZE[1]))
        .shrink(MARGIN_THE_TEST_WINDOW_HAS_ROUND_ITS_EDGES);
    let gap = f32::from(theme::PANEL_GAP);
    assert_eq!(gap, 6.0);

    let timeline = *panels.iter().max_by(|a, b| a.width().total_cmp(&b.width())).unwrap();
    let mut above: Vec<Rect> = panels.iter().copied().filter(|panel| *panel != timeline).collect();
    above.sort_by(|a, b| a.left().total_cmp(&b.left()));
    let (properties, preview) = (above[0], above[1]);

    assert_eq!(
        timeline.left() - screen.left(),
        gap,
        "the timeline is 6 from the left edge of the window"
    );
    assert_eq!(screen.right() - timeline.right(), gap, "and the right");
    assert_eq!(screen.bottom() - timeline.bottom(), gap, "and the bottom");
    assert_eq!(
        properties.left() - screen.left(),
        gap,
        "the properties are 6 from the left"
    );
    assert_eq!(
        preview.left() - properties.right(),
        gap,
        "the preview is 6 from the properties"
    );
    assert_eq!(screen.right() - preview.right(), gap, "and 6 from the right edge");
    assert_eq!(
        timeline.top() - properties.bottom(),
        gap,
        "the timeline is 6 under the properties"
    );
    assert_eq!(timeline.top() - preview.bottom(), gap, "and under the preview");
    assert_eq!(properties.top(), preview.top(), "side by side");
    assert_eq!(
        properties.top() - the_menu_bar(&window).bottom(),
        gap,
        "6 under the menu bar, which is at the top of the window"
    );
}

// --- nothing selected ---

#[test]
fn with_nothing_selected_the_panel_has_a_hint_and_no_properties() {
    let window = window_with_three_clips();
    assert!(is_shown(&window, HINT));
    for label in ["Speed", "Gain", "Mute", "Video clip", "Track"] {
        assert!(!is_shown(&window, label), "{label}");
    }
}

#[test]
fn the_hint_is_inside_the_properties_panel_and_the_panel_is_above_the_timeline_and_left_of_the_preview() {
    let window = window_with_three_clips();
    let panel = window.the_properties_panel();
    assert!(
        panel.contains_rect(window.rectangle_of(HINT)),
        "the hint is in the panel"
    );
    assert!(
        panel.bottom() < window.rectangle_of("Ruler").top(),
        "above the timeline"
    );
    assert!(
        panel.width() > 250.0 && panel.width() < 400.0,
        "narrower than the preview: {panel:?}"
    );
}

#[test]
fn letting_go_of_what_is_selected_brings_the_hint_back() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 45);
    assert!(!is_shown(&window, HINT));

    window.press_key(Key::Escape);

    assert!(is_shown(&window, HINT));
    assert!(!is_shown(&window, "Speed"));
}

// --- one clip selected ---

#[test]
fn clicking_a_clip_selects_it_and_the_panel_shows_a_box_with_its_speed_its_gain_and_its_mute() {
    let mut window = window_with_three_clips();
    let middle_clip = window.clip(0, 1);

    select_the_clip_at(&mut window, 0, 45);

    assert_eq!(window.application().selected_clip(), Some(middle_clip));
    for label in ["Speed", "Gain", "Mute"] {
        assert!(is_shown(&window, label), "{label}");
    }
    assert!(!is_shown(&window, HINT));
    assert!(
        window
            .the_properties_panel()
            .contains_rect(window.rectangle_of("Speed")),
        "in the properties panel"
    );
    assert!(
        !window.application().has_unsaved_changes(),
        "showing them is not an edit"
    );
}

#[test]
fn the_box_is_headed_with_which_kind_of_clip_it_is_for_and_says_which_file_it_comes_from() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(90, 3));
    select_the_clip_at(&mut window, 1, 45);
    assert!(is_shown(&window, "Video clip"));
    assert!(is_shown(&window, "holiday.mp4"));
    assert!(!is_shown(&window, "Sound clip"));

    select_the_clip_at(&mut window, 0, 45);
    assert!(is_shown(&window, "Sound clip"));
    assert!(is_shown(&window, "song.mp3"));
    assert!(!is_shown(&window, "Video clip"));
}

#[test]
fn the_box_is_a_rounded_one_a_little_lighter_than_the_panel_with_the_kind_at_the_top_of_it() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 45);

    let heading = window.rectangle_of("Video clip");
    let boxes: Vec<(Rect, egui::CornerRadius)> = window
        .harness
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Rect(rectangle) if rectangle.fill == theme::PROPERTIES_BOX_BACKGROUND => {
                Some((rectangle.rect, rectangle.corner_radius))
            }
            _ => None,
        })
        .collect();
    let (the_box, radius) = boxes
        .iter()
        .find(|(rectangle, _)| rectangle.contains_rect(heading))
        .copied()
        .expect("a box that holds the heading");
    assert_eq!(
        radius,
        egui::CornerRadius::same(theme::CLIP_CORNER_RADIUS),
        "as round as the clips on the timeline"
    );
    assert!(
        heading.top() - the_box.top() < 30.0,
        "the kind is at the top: {heading:?} in {the_box:?}"
    );
    assert!(
        the_box.contains_rect(window.rectangle_of("Speed")) && the_box.contains_rect(window.rectangle_of("Mute")),
        "and the properties are in it"
    );
    let lightness = |colour: egui::Color32| u32::from(colour.r()) + u32::from(colour.g()) + u32::from(colour.b());
    assert!(
        lightness(theme::PROPERTIES_BOX_BACKGROUND) > lightness(theme::PANEL_BACKGROUND)
            && lightness(theme::PROPERTIES_BOX_BACKGROUND) < lightness(theme::PANEL_BACKGROUND) + 100,
        "lighter than the panel, but only a little"
    );
}

#[test]
fn right_clicking_a_clip_selects_it_too_but_leaves_alone_a_selection_that_it_is_one_of() {
    let mut window = window_with_three_clips();
    right_click_the_clip_at(&mut window, 0, 45);
    assert_eq!(window.application().selected_clip(), Some(window.clip(0, 1)));

    select_the_clip_at(&mut window, 0, 15);
    let last = window.middle_of_the_strip_at(0, 75);
    window.click_with_control(last);
    right_click_the_clip_at(&mut window, 0, 75);
    assert_eq!(
        window.application().selected_clips().len(),
        2,
        "both are still selected"
    );
}

#[test]
fn the_panel_follows_the_selection_from_clip_to_clip() {
    let mut window = window_with_three_clips();
    window.do_this(Command::SetClipSpeed {
        clip: window.clip(0, 2),
        speed_factor: SpeedFactor::new(4).unwrap(),
    });
    select_the_clip_at(&mut window, 0, 15);
    assert_eq!(slider_value(&window, "Speed"), 1.0);

    // The last clip, which now lasts 8 frames from frame 60.
    select_the_clip_at(&mut window, 0, 63);

    assert_eq!(slider_value(&window, "Speed"), 4.0);
}

#[test]
fn the_panel_shows_the_speed_the_gain_and_the_mute_of_the_clip() {
    let mut project = project_cut_into_three(90);
    let middle = project.tracks()[0].placement_at_index(1).unwrap().id;
    project.set_clip_speed(middle, SpeedFactor::new(3).unwrap()).unwrap();
    project.set_clip_gain(middle, Gain::from_decibels(6.0)).unwrap();
    project.set_clip_muted(middle, true).unwrap();
    let mut window = Window::showing(project);

    // The middle clip now lasts 10 frames, starting at frame 30.
    select_the_clip_at(&mut window, 0, 35);

    assert_eq!(slider_value(&window, "Speed"), 3.0);
    assert_eq!(slider_value(&window, "Gain"), 6.0);
    assert!(window.is_checked("Mute"));
}

#[test]
fn the_speed_slider_sets_the_speed_of_the_clip_and_keeps_the_picture_on_screen() {
    let mut window = window_with_three_clips();
    window.move_current_frame_to(75); // the last clip, showing source frame 75
    select_the_clip_at(&mut window, 0, 15); // the first clip

    window.set_slider("Speed", 5.0);

    assert_eq!(window.clips(), vec![(0, 30, 5), (30, 60, 1), (60, 90, 1)]);
    assert!(window.application().has_unsaved_changes());
    // The first clip lasts 6 frames now, so everything after it is 24 frames earlier: the current
    // frame went with the picture it was showing.
    assert_eq!(window.current_frame(), 75 - 24);
    assert_eq!(
        window.application().selected_clip(),
        Some(window.clip(0, 0)),
        "and it is still selected"
    );
}

#[test]
fn the_speed_can_be_set_again_and_again() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 45);
    for speed in [2.0, 10.0, 1.0, 1000.0] {
        window.set_slider("Speed", speed);
        assert_eq!(window.clips()[1].2, speed as u32, "{speed}");
        assert_eq!(slider_value(&window, "Speed"), speed, "the slider shows it");
    }
}

#[test]
fn dragging_the_speed_slider_changes_the_speed_along_the_way_and_is_one_step_of_the_history() {
    // One clip of 10 seconds. Making it play 30 times faster leaves it a third of a second long, so its middle
    // moves a long way to the left while the speed slider is being dragged; the panel does not move.
    let mut window = Window::showing(project_of(300));
    select_the_clip_at(&mut window, 0, 100);
    let slider = window.rectangle_of("Speed");
    let at_the_left_end = Pos2::new(slider.left() + 2.0, slider.center().y);
    let in_the_middle = Pos2::new(slider.center().x, slider.center().y);
    window.application_mut().note_everything_is_saved();

    window.drag(at_the_left_end, in_the_middle, egui::PointerButton::Primary);

    let speed = window.clips()[0].2;
    assert!(
        (10..=100).contains(&speed),
        "the pointer is half way along a slider that goes from 1 to 1000 times, so a speed of about 30, and not {speed}"
    );
    assert_eq!(window.application().selected_clip(), Some(window.clip(0, 0)));
    window.harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    window.settle();
    assert_eq!(window.clips(), vec![(0, 300, 1)], "one undo takes the whole drag back");
    assert!(!window.application().can_undo());
}

#[test]
fn the_gain_slider_sets_the_gain_of_the_clip_and_stays_within_thirty_decibels_either_way() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 45);
    let clip = window.clip(0, 1);
    let gain = |window: &Window| {
        window
            .application()
            .project()
            .placement_of(clip)
            .unwrap()
            .clip
            .as_media()
            .unwrap()
            .gain()
            .decibels()
    };

    window.set_slider("Gain", -9.5);
    assert_eq!(gain(&window), -9.5);
    window.set_slider("Gain", 500.0);
    assert_eq!(gain(&window), 30.0);
    window.set_slider("Gain", -500.0);
    assert_eq!(gain(&window), -30.0);
    window.set_slider("Gain", 0.0);
    assert_eq!(gain(&window), 0.0, "0 dB is no change, in the middle of the slider");
}

#[test]
fn the_mute_box_mutes_the_clip_and_unmutes_it() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 45);
    let clip = window.clip(0, 1);
    let is_muted = |window: &Window| {
        window
            .application()
            .project()
            .placement_of(clip)
            .unwrap()
            .clip
            .as_media()
            .unwrap()
            .is_muted()
    };

    window.click(window.rectangle_of("Mute").center());
    assert!(is_muted(&window));
    assert!(window.application().has_unsaved_changes());
    window.click(window.rectangle_of("Mute").center());
    assert!(!is_muted(&window));
}

#[test]
fn a_change_that_changes_nothing_is_not_an_edit() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 45);
    window.set_slider("Speed", 1.0);
    window.set_slider("Gain", 0.0);
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn when_the_track_has_its_own_speed_the_panel_says_how_fast_the_clip_really_plays() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 45);
    window.set_slider("Speed", 3.0);
    assert!(
        window.harness.query_by_label_contains("It plays at").is_none(),
        "no note at normal track speed"
    );

    let track = window.application().project().tracks()[0].id();
    window.do_this(Command::SetTrackSpeed {
        track,
        speed_factor: SpeedFactor::new(2).unwrap(),
    });
    window.settle();
    assert!(
        is_shown_containing(&window, "It plays at 6x, because its track plays at 2x"),
        "the clip's 3x and its track's 2x"
    );
}

fn is_shown_containing(window: &Window, text: &str) -> bool {
    window.harness.query_by_label_contains(text).is_some()
}

#[test]
fn the_clip_of_a_file_that_cannot_be_found_says_so_and_offers_to_choose_the_file_again() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(90, 3));
    window.application_mut().source_files.insert(
        SOUND,
        SourceFileState::new(SourceStatus::Unavailable {
            reason: "there is no file there".to_owned(),
        }),
    );
    window.settle();

    select_the_clip_at(&mut window, 0, 45);
    assert!(is_shown_containing(&window, "cannot be found"));
    assert!(is_shown(&window, "Choose the file again"));

    select_the_clip_at(&mut window, 1, 45);
    assert!(
        !is_shown(&window, "Choose the file again"),
        "the video clip is fine: the file it is made from can be used"
    );
    assert!(window.application().source_file_is_available(VIDEO));
}

#[test]
fn nothing_in_the_panel_can_be_changed_while_a_message_is_up() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 45);
    assert!(!window.harness.get_by_label("Speed").accesskit_node().is_disabled());

    window.application_mut().dialog = Some(super::Dialog::Message {
        title: "Note".into(),
        text: "Read me".into(),
    });
    window.settle();

    assert!(window.harness.get_by_label("Speed").accesskit_node().is_disabled());
    assert!(window.harness.get_by_label("Mute").accesskit_node().is_disabled());
}

#[test]
fn when_the_selected_clip_is_deleted_the_panel_shows_the_hint_again() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 45);
    let clip = window.clip(0, 1);

    window.do_this(Command::DeleteClips(vec![clip]));

    assert!(is_shown(&window, HINT));
}

// --- several things selected ---

#[test]
fn with_several_clips_selected_the_panel_lists_them_and_has_no_properties_to_change() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 15);
    let last = window.middle_of_the_strip_at(0, 75);
    window.click_with_control(last);

    assert_eq!(window.application().selected_clips().len(), 2);
    for label in ["Speed", "Gain", "Mute", HINT] {
        assert!(!is_shown(&window, label), "{label}");
    }
    let entries: Vec<String> = window
        .harness
        .query_all_by_label_contains("Video clip: holiday.mp4")
        .map(|node| node.accesskit_node().label().unwrap_or_default())
        .collect();
    assert_eq!(
        entries,
        vec![
            "Video clip: holiday.mp4 (at 00:00:00.00)".to_owned(),
            "Video clip: holiday.mp4 (at 00:00:02.00)".to_owned()
        ],
        "each with where it starts, to tell them apart"
    );
}

#[test]
fn the_list_is_inside_a_rounded_yellow_outline_in_the_properties_panel() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 15);
    let last = window.middle_of_the_strip_at(0, 75);
    window.click_with_control(last);

    let panel = window.the_properties_panel();
    let outlines: Vec<(Rect, egui::CornerRadius)> = window
        .harness
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            Shape::Rect(rectangle)
                if rectangle.stroke.color == theme::SELECTION_OUTLINE
                    && rectangle.fill.a() == 0
                    && panel.contains_rect(rectangle.rect) =>
            {
                Some((rectangle.rect, rectangle.corner_radius))
            }
            _ => None,
        })
        .collect();
    assert_eq!(outlines.len(), 1, "{outlines:?}");
    let (outline, radius) = outlines[0];
    assert_eq!(radius, egui::CornerRadius::same(theme::CLIP_CORNER_RADIUS));
    for node in window.harness.query_all_by_label_contains("Video clip: holiday.mp4") {
        assert!(outline.contains_rect(node.rect()), "an entry is inside it");
    }
}

#[test]
fn clicking_an_entry_of_the_list_selects_just_that_one_and_its_properties_are_shown() {
    let mut window = window_with_three_clips();
    select_the_clip_at(&mut window, 0, 15);
    let last = window.middle_of_the_strip_at(0, 75);
    window.click_with_control(last);
    let (first, third) = (window.clip(0, 0), window.clip(0, 2));
    assert_eq!(window.application().selected_clips(), vec![first, third]);

    window
        .harness
        .get_by_label("Video clip: holiday.mp4 (at 00:00:02.00)")
        .click();
    window.settle();

    assert_eq!(window.application().selected_clips(), vec![third], "just that one");
    assert!(is_shown(&window, "Speed"), "and now its properties can be changed");
    assert!(!window.application().has_unsaved_changes(), "picking is not an edit");
}

#[test]
fn a_track_and_clips_can_be_in_the_list_together_and_a_track_can_be_picked_from_it() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(300, 5));
    select_the_clip_at(&mut window, 1, 100);
    window.click_with_control(window.title_box_of_track(0).center());
    let sound_track = window.application().project().tracks()[0].id();
    assert_eq!(window.application().selected_items().len(), 2);

    assert!(is_shown(&window, "Video clip: holiday.mp4 (at 00:00:00.00)"));
    assert!(is_shown(&window, "Track: track 1"));
    window.harness.get_by_label("Track: track 1").click();
    window.settle();

    assert_eq!(
        window.application().selected_items(),
        vec![SelectionItem::Track(sound_track)]
    );
    assert!(is_shown(&window, "Track"));
}

// --- a track selected ---

#[test]
fn selecting_a_track_shows_its_properties_in_a_box_headed_track_and_not_those_of_a_clip() {
    let mut window = Window::showing(project_of(300));

    window.select_the_track_at(0);

    assert!(is_shown(&window, "Track"));
    assert!(is_shown(&window, "track 0"), "with its title");
    for label in ["Speed", "Gain", "Mute"] {
        assert!(is_shown(&window, label), "{label}");
    }
    assert!(!is_shown(&window, "Video clip"));
    assert!(!is_shown(&window, HINT));
}

#[test]
fn the_box_of_a_track_shows_what_the_track_has_now() {
    let mut project = project_of(300);
    let track = project.tracks()[0].id();
    project.set_track_speed(track, SpeedFactor::new(4).unwrap()).unwrap();
    project.set_track_gain(track, Gain::from_decibels(-6.0)).unwrap();
    project.set_track_muted(track, true).unwrap();
    let mut window = Window::showing(project);
    window.select_the_track_at(0);

    assert!(window.is_checked("Mute"));
    assert_eq!(slider_value(&window, "Speed"), 4.0);
    assert_eq!(slider_value(&window, "Gain"), -6.0);
}

#[test]
fn the_speed_gain_and_mute_of_a_track_can_all_be_changed_in_the_panel() {
    let mut window = Window::showing(project_of(300));
    window.select_the_track_at(0);
    let track = window.application().project().tracks()[0].id();

    window.set_slider("Speed", 2.0);
    assert_eq!(
        window.application().project().track(track).unwrap().speed(),
        SpeedFactor::new(2).unwrap()
    );
    assert_eq!(
        window.application().project().total_frame_count(),
        150,
        "everything on the track plays twice as fast, so it is half as long"
    );

    window.set_slider("Gain", -12.0);
    assert_eq!(
        window.application().project().track(track).unwrap().gain(),
        Gain::from_decibels(-12.0)
    );

    window.harness.get_by_label("Mute").click();
    window.settle();
    assert!(window.application().project().track(track).unwrap().is_muted());
    assert!(window.application().has_unsaved_changes());
}

#[test]
fn the_gain_slider_of_a_track_goes_as_far_quieter_as_louder_with_no_change_in_the_middle() {
    let mut window = Window::showing(project_of(300));
    window.select_the_track_at(0);
    let track = window.application().project().tracks()[0].id();
    let gain = |window: &Window| window.application().project().track(track).unwrap().gain().decibels();

    window.set_slider("Gain", 1000.0);
    assert_eq!(gain(&window), 30.0, "no louder than 30 dB");
    window.set_slider("Gain", -1000.0);
    assert_eq!(gain(&window), -30.0, "and no quieter than minus 30");
    assert_eq!(
        Gain::SMALLEST_DECIBELS,
        -Gain::LARGEST_DECIBELS,
        "so 0 dB is the middle of the slider"
    );
    window.set_slider("Gain", 0.0);
    assert_eq!(gain(&window), 0.0);
}

#[test]
fn the_speed_slider_of_a_track_goes_from_normal_speed_to_a_thousand_times() {
    let mut window = Window::showing(project_of(1000));
    window.select_the_track_at(0);
    let track = window.application().project().tracks()[0].id();
    let speed = |window: &Window| window.application().project().track(track).unwrap().speed().value();

    window.set_slider("Speed", 5000.0);
    assert_eq!(speed(&window), 1000);
    window.set_slider("Speed", 0.0);
    assert_eq!(speed(&window), 1, "never below normal speed");
}

#[test]
fn each_track_has_its_own_properties_and_selecting_another_shows_those() {
    let mut project = project_with_a_video_and_a_sound(300, 10);
    let sound_track = project.tracks()[0].id();
    project
        .set_track_speed(sound_track, SpeedFactor::new(3).unwrap())
        .unwrap();
    let mut window = Window::showing(project);

    window.select_the_track_at(0);
    assert_eq!(slider_value(&window, "Speed"), 3.0);
    window.select_the_track_at(1);
    assert_eq!(slider_value(&window, "Speed"), 1.0);
}

#[test]
fn deleting_the_selected_track_or_starting_another_project_brings_the_hint_back() {
    let mut window = Window::showing(project_with_a_video_and_a_sound(300, 10));
    window.select_the_track_at(0);
    let track = window.application().project().tracks()[0].id();
    window.do_this(Command::DeleteTrack(track));
    assert!(is_shown(&window, HINT));

    window.select_the_track_at(0);
    assert!(is_shown(&window, "Track"));
    window.application_mut().start_empty_project();
    window.settle();
    assert!(is_shown(&window, HINT));
}

// --- the keys ---

#[test]
fn the_keys_are_still_shortcuts_after_a_slider_of_the_panel_has_been_used() {
    let mut window = Window::showing(project_of(300));
    select_the_clip_at(&mut window, 0, 100);
    let slider = window.rectangle_of("Speed");
    window.drag(
        Pos2::new(slider.left() + 2.0, slider.center().y),
        Pos2::new(slider.left() + 40.0, slider.center().y),
        egui::PointerButton::Primary,
    );

    window.press_key(Key::Space);

    assert!(window.application().is_playing(), "space plays");
}
