// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The generator clip in the window (phase 5, piece 5.4): made from the menu that opens on an empty part of a track (New, Generator, Solid colour), with its properties (its colour,
//! how long it lasts, where it is, the effects it has) in the panel, undone, and refused where the rectangle of a clip group is; and its colour changed, which is a keyframe if the
//! colour has keyframes.

use egui_kittest::kittest::{NodeT, Queryable};

use super::test_window::{Window, project_of};
use super::undo_tests::undo;
use crate::application::commands::Command;
use crate::project::{
    AnimationTarget, ClipId, ClipKind, EffectKind, GeneratorKind, GroupKind, Member, PropertyId, TextColour,
    TimelineFrameIndex,
};

/// Makes a solid colour the way a person does: right-clicks the empty part of the strip of the track at `position`, where timeline frame `frame` is, and chooses New, then Generator,
/// then Solid colour.
fn make_a_solid_colour_at(window: &mut Window, position: usize, frame: u64) {
    let point = window.middle_of_the_strip_at(position, frame);
    window.right_click(point);
    window.harness.get_by_label_contains("New").click();
    window.settle();
    window.harness.get_by_label_contains("Generator").click();
    window.settle();
    window.harness.get_by_label("Solid colour").click();
    window.settle();
}

/// The generator clip that is the `index`th clip of the track at `position`, and how long it lasts.
fn generator_clip(window: &Window, position: usize, index: usize) -> (ClipId, u64) {
    let location = window.clip(position, index);
    let placement = window.application().project().placement_of(location).unwrap();
    assert_eq!(placement.clip.kind(), ClipKind::Generator);
    (location, placement.clip.duration_in_frames_it_says().unwrap())
}

#[test]
fn the_menu_that_opens_on_an_empty_part_of_a_track_has_a_generator_with_a_solid_colour_in_it() {
    let mut window = Window::showing(project_of(90));
    let empty_part = window.middle_of_the_strip_at(0, 100);
    window.right_click(empty_part);
    window.harness.get_by_label_contains("New").click();
    window.settle();

    assert!(window.harness.query_by_label_contains("Generator").is_some());
    assert!(
        window.harness.query_by_label("Solid colour").is_none(),
        "it is in the menu of the generator, which is not open yet"
    );
    window.harness.get_by_label_contains("Generator").click();
    window.settle();
    assert!(window.harness.query_by_label("Solid colour").is_some());
    assert!(
        !window.application().has_unsaved_changes(),
        "opening menus changes nothing"
    );
}

#[test]
fn choosing_solid_colour_puts_one_where_the_menu_was_opened_for_five_seconds_selects_it_and_shows_its_properties() {
    let mut window = Window::showing(project_of(90));

    make_a_solid_colour_at(&mut window, 0, 100);

    let (location, duration) = generator_clip(&window, 0, 1);
    assert_eq!(duration, 150, "five seconds at 30 frames a second");
    let placement = window.application().project().placement_of(location).unwrap();
    assert!(placement.first_timeline_frame.value().abs_diff(100) <= 1);
    assert_eq!(window.application().selected_clip(), Some(location));
    assert!(window.application().has_unsaved_changes());
    assert!(
        window.harness.query_by_label("Generator clip").is_some(),
        "the box of its properties is in the panel"
    );
    assert!(window.harness.query_by_label("Colour").is_some(), "its colour");
    assert!(
        window.harness.query_by_label("Shown for").is_some(),
        "how long it lasts"
    );
    assert!(window.harness.query_by_label("Position").is_some(), "and where it is");
    assert_eq!(
        window.current_frame(),
        placement.first_timeline_frame.value(),
        "the playhead went to it, so that it shows"
    );
}

#[test]
fn a_solid_colour_that_is_made_is_undone_as_any_edit_is() {
    let mut window = Window::showing(project_of(90));
    make_a_solid_colour_at(&mut window, 0, 100);
    assert_eq!(window.application().project().tracks()[0].entries().len(), 2);

    undo(&mut window);

    assert_eq!(window.application().project().tracks()[0].entries().len(), 1);
}

#[test]
fn a_solid_colour_is_only_as_long_as_there_is_room_for_before_the_next_clip_and_is_not_put_where_a_clip_is() {
    let mut project = project_of(90);
    let front = project.add_track_in_front();
    project
        .try_insert_text(
            front,
            TimelineFrameIndex::new(30),
            crate::project::TextObject::default(),
            30,
        )
        .unwrap();
    let mut window = Window::showing(project);

    window.do_this(Command::NewGeneratorClip {
        track: front,
        frame: TimelineFrameIndex::new(0),
        kind: GeneratorKind::SolidColour,
    });
    let (_, duration) = generator_clip(&window, 0, 0);
    assert_eq!(duration, 30);

    window.application_mut().interface.dialog = None;
    window.do_this(Command::NewGeneratorClip {
        track: front,
        frame: TimelineFrameIndex::new(40),
        kind: GeneratorKind::SolidColour,
    });
    let (title, _) = window.application().message_showing().expect("it says why");
    assert_eq!(title, "There is no room for a generator clip there");
}

#[test]
fn a_solid_colour_is_not_made_inside_the_rectangle_of_a_clip_group_and_the_message_names_the_group() {
    let mut project = project_of(90);
    let front = project.add_track_in_front();
    let video = project.tracks()[1].placement_at_index(0).unwrap().id;
    let (text, _) = project
        .try_insert_text(
            front,
            TimelineFrameIndex::new(10),
            crate::project::TextObject::default(),
            20,
        )
        .unwrap();
    project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(video), Member::Clip(text)])
        .unwrap();
    let mut window = Window::showing(project);

    window.do_this(Command::NewGeneratorClip {
        track: front,
        frame: TimelineFrameIndex::new(50),
        kind: GeneratorKind::SolidColour,
    });

    let (title, text) = window.application().message_showing().expect("it says why");
    assert_eq!(title, "A generator clip cannot be put there");
    assert!(
        text.contains("would be inside the rectangle of the clip group"),
        "{text}"
    );
}

fn the_colour_at(window: &Window, clip: ClipId, frame: u64) -> TextColour {
    window
        .application()
        .project()
        .generator_colour_at(clip, TimelineFrameIndex::new(frame))
        .unwrap()
}

#[test]
fn the_colour_of_a_solid_colour_is_changed_by_a_command_and_stays_so_when_it_has_no_keyframes() {
    let mut window = Window::showing(project_of(90));
    make_a_solid_colour_at(&mut window, 0, 100);
    let (clip, _) = generator_clip(&window, 0, 1);
    let red = TextColour::new(250, 10, 10, 255);

    window.do_this(Command::SetGeneratorColour { clip, colour: red });

    assert_eq!(the_colour_at(&window, clip, 100), red);
    assert_eq!(the_colour_at(&window, clip, 200), red, "the same all through");
    assert!(
        !window
            .application()
            .project()
            .is_animated(AnimationTarget::Clip(clip), PropertyId::Colour),
        "no keyframe was made"
    );
}

#[test]
fn the_colour_of_a_solid_colour_that_has_keyframes_is_changed_at_the_playhead_by_changing_a_keyframe_there() {
    let mut window = Window::showing(project_of(90));
    make_a_solid_colour_at(&mut window, 0, 100);
    let (clip, _) = generator_clip(&window, 0, 1);
    let target = AnimationTarget::Clip(clip);
    let (grey, blue) = (the_colour_at(&window, clip, 100), TextColour::new(10, 10, 250, 255));
    for frame in [100, 140] {
        window
            .application_mut()
            .document
            .project
            .add_keyframe(target, PropertyId::Colour, TimelineFrameIndex::new(frame))
            .unwrap();
    }
    window.move_current_frame_to(140);

    window.do_this(Command::SetGeneratorColour { clip, colour: blue });

    assert_eq!(the_colour_at(&window, clip, 140), blue, "the keyframe at the playhead");
    assert_eq!(
        the_colour_at(&window, clip, 100),
        grey,
        "the other keyframe is as it was"
    );
    assert!(window.application().project().is_animated(target, PropertyId::Colour));
}

#[test]
fn an_effect_put_on_a_solid_colour_is_one_of_its_properties_and_shown_in_the_panel() {
    let mut window = Window::showing(project_of(90));
    make_a_solid_colour_at(&mut window, 0, 100);
    let (clip, _) = generator_clip(&window, 0, 1);

    window.do_this(Command::AddEffect {
        target: AnimationTarget::Clip(clip),
        kind: EffectKind::HUE_SHIFT,
    });
    window.settle();

    assert_eq!(
        window
            .application()
            .project()
            .effects_of(AnimationTarget::Clip(clip))
            .unwrap()
            .len(),
        1
    );
    assert!(
        window.harness.query_by_label_contains("Hue shift").is_some(),
        "the box of the effect is in the panel with the box of the clip"
    );
}

#[test]
fn changing_the_colour_of_a_solid_colour_that_has_a_keyframe_makes_a_keyframe_at_the_playhead_and_undo_takes_it_back() {
    let mut window = Window::showing(project_of(90));
    make_a_solid_colour_at(&mut window, 0, 100);
    let (clip, _) = generator_clip(&window, 0, 1);
    let target = AnimationTarget::Clip(clip);
    let (grey, blue) = (the_colour_at(&window, clip, 100), TextColour::new(10, 10, 250, 255));
    window
        .application_mut()
        .document
        .project
        .add_keyframe(target, PropertyId::Colour, TimelineFrameIndex::new(100))
        .unwrap();
    window.move_current_frame_to(130);

    window.do_this(Command::SetGeneratorColour { clip, colour: blue });

    let project = window.application().project();
    assert!(project.has_keyframe_at(target, PropertyId::Colour, TimelineFrameIndex::new(130)));
    assert_eq!(the_colour_at(&window, clip, 130), blue);
    assert_eq!(
        the_colour_at(&window, clip, 100),
        grey,
        "the first keyframe is as it was"
    );

    undo(&mut window);

    assert!(
        !window
            .application()
            .project()
            .has_keyframe_at(target, PropertyId::Colour, TimelineFrameIndex::new(130))
    );
}

#[test]
fn colours_made_at_two_frames_soon_after_each_other_are_two_steps_of_the_history() {
    let mut window = Window::showing(project_of(90));
    make_a_solid_colour_at(&mut window, 0, 100);
    let (clip, _) = generator_clip(&window, 0, 1);
    let target = AnimationTarget::Clip(clip);
    let (blue, red) = (TextColour::new(10, 10, 250, 255), TextColour::new(250, 10, 10, 255));
    window
        .application_mut()
        .document
        .project
        .add_keyframe(target, PropertyId::Colour, TimelineFrameIndex::new(100))
        .unwrap();
    window.move_current_frame_to(120);
    window.do_this(Command::SetGeneratorColour { clip, colour: blue });
    window.move_current_frame_to(150);
    window.do_this(Command::SetGeneratorColour { clip, colour: red });
    let has_keyframe_at = |window: &Window, frame: u64| {
        window
            .application()
            .project()
            .has_keyframe_at(target, PropertyId::Colour, TimelineFrameIndex::new(frame))
    };
    assert!(has_keyframe_at(&window, 120) && has_keyframe_at(&window, 150));

    undo(&mut window);

    assert!(has_keyframe_at(&window, 120), "the keyframe at frame 120 stays");
    assert!(
        !has_keyframe_at(&window, 150),
        "only the keyframe at frame 150 is taken back"
    );
}

#[test]
fn the_menu_of_generators_is_called_generator_and_nothing_else() {
    let mut window = Window::showing(project_of(90));
    let empty_part = window.middle_of_the_strip_at(0, 100);
    window.right_click(empty_part);
    window.harness.get_by_label_contains("New").click();
    window.settle();

    let label = window
        .harness
        .get_by_label_contains("Generator")
        .accesskit_node()
        .label()
        .unwrap_or_default();

    assert_eq!(
        label.trim_end_matches(|letter: char| !letter.is_alphanumeric()),
        "Generator",
        "{label:?}"
    );
}

#[test]
fn a_solid_colour_is_drawn_on_the_timeline_in_the_colour_it_says_without_how_clear_it_is() {
    let mut window = Window::showing(project_of(90));
    make_a_solid_colour_at(&mut window, 0, 100);
    let (clip, _) = generator_clip(&window, 0, 1);

    window.do_this(Command::SetGeneratorColour {
        clip,
        colour: TextColour::new(10, 200, 30, 128),
    });

    let wanted = egui::Color32::from_rgb(10, 200, 30);
    assert!(
        window
            .shapes_drawn()
            .iter()
            .any(|shape| matches!(shape, egui::Shape::Rect(rectangle) if rectangle.fill == wanted)),
        "a rectangle of the colour is drawn"
    );
}
