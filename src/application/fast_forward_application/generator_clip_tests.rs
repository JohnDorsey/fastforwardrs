// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The generator clip in the window (phase 6, piece 6.5): made from the menu that opens on an empty part of a track (New, Generator from file...), from a shader that reads no
//! picture; selected, with its properties (how long it lasts, where it is, and the effect that it makes its picture with) in the panel; undone; refused where a clip group's rectangle is;
//! and what is a filter and what is a generator not mixed up.

use std::path::{Path, PathBuf};

use egui_kittest::kittest::Queryable;

use super::shader_flow::NOT_A_SHADER_TITLE;
use super::test_window::{Window, project_of};
use super::undo_tests::undo;
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::application::file_chooser::ScriptedFileChooser;
use crate::project::{AnimationTarget, ClipId, ClipKind, EffectKind, GroupKind, Member, TimelineFrameIndex};

const SOLID: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"colour\", \"LABEL\": \"Fill colour\", \"TYPE\": \"color\", \"DEFAULT\": [0.2, 0.4, 0.9, 1.0] } ] }*/\nvoid main() { gl_FragColor = colour; }\n";
const A_FILTER: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage); }\n";

fn a_file_in(folder: &Path, name: &str, text: &str) -> PathBuf {
    let path = folder.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

fn window_that_asks() -> (Window, ScriptedFileChooser) {
    let mut window = Window::showing(project_of(90));
    let chooser = ScriptedFileChooser::default();
    window.use_file_chooser(&chooser);
    (window, chooser)
}

fn the_message(window: &Window) -> Option<(String, String)> {
    match &window.application().interface.dialog {
        Some(Dialog::Message { title, text }) => Some((title.clone(), text.clone())),
        _ => None,
    }
}

/// Makes a generator clip the way a person does: right-clicks the empty part of the strip of the track at `position`, where timeline frame `frame` is, and chooses New, then
/// Generator from file..., and the file is the one the chooser is scripted to give.
fn make_a_generator_at(window: &mut Window, chooser: &ScriptedFileChooser, file: &Path, position: usize, frame: u64) {
    chooser.will_answer(Some(file.to_owned()));
    let point = window.middle_of_the_strip_at(position, frame);
    window.right_click(point);
    window.harness.get_by_label_contains("New").click();
    window.settle();
    window.harness.get_by_label("Generator from file...").click();
    window.settle();
}

fn generator_clip(window: &Window, position: usize, index: usize) -> (ClipId, u64) {
    let location = window.clip(position, index);
    let placement = window.application().project().placement_of(location).unwrap();
    assert_eq!(placement.clip.kind(), ClipKind::Generator);
    (location, placement.clip.duration_in_frames_it_says().unwrap())
}

#[test]
fn the_menu_that_opens_on_an_empty_part_of_a_track_has_a_generator_from_a_file_beside_the_text_object() {
    let mut window = Window::showing(project_of(90));
    let empty_part = window.middle_of_the_strip_at(0, 100);
    window.right_click(empty_part);
    window.harness.get_by_label_contains("New").click();
    window.settle();

    assert!(window.harness.query_by_label("Text object").is_some());
    assert!(window.harness.query_by_label("Generator from file...").is_some());
    assert!(
        !window.application().has_unsaved_changes(),
        "opening a menu changes nothing"
    );
}

#[test]
fn choosing_a_generator_puts_one_where_the_menu_was_opened_for_five_seconds_selects_it_and_shows_its_properties() {
    let folder = tempfile::tempdir().unwrap();
    let file = a_file_in(folder.path(), "solid colour.fs", SOLID);
    let (mut window, chooser) = window_that_asks();

    make_a_generator_at(&mut window, &chooser, &file, 0, 100);

    let asked = chooser.questions_asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].title, "Choose a generator");
    assert!(!asked[0].saving);
    assert_eq!(the_message(&window), None);
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
    assert!(
        window.harness.query_by_label("Shown for").is_some(),
        "how long it lasts"
    );
    assert!(window.harness.query_by_label("Position").is_some(), "where it is");
    assert!(
        window.harness.query_by_label("Fill colour").is_some(),
        "and the controls of what it makes, in the box of its first effect"
    );
    assert_eq!(
        window.current_frame(),
        placement.first_timeline_frame.value(),
        "the playhead went to it, so that it shows"
    );
    let content = placement.clip.as_generator().unwrap().content();
    assert!(
        placement
            .effects
            .instance(content)
            .unwrap()
            .kind()
            .shader_source()
            .is_some(),
        "and what it makes carries the shader, which the project keeps"
    );
}

#[test]
fn a_generator_that_is_made_is_undone_as_any_edit_is() {
    let folder = tempfile::tempdir().unwrap();
    let file = a_file_in(folder.path(), "solid.fs", SOLID);
    let (mut window, chooser) = window_that_asks();
    make_a_generator_at(&mut window, &chooser, &file, 0, 100);
    assert_eq!(window.application().project().tracks()[0].entries().len(), 2);

    undo(&mut window);

    assert_eq!(window.application().project().tracks()[0].entries().len(), 1);
}

#[test]
fn a_filter_is_refused_as_a_generator_and_a_generator_as_a_filter_each_with_the_way_to_do_what_was_meant() {
    let folder = tempfile::tempdir().unwrap();
    let filter = a_file_in(folder.path(), "keep.fs", A_FILTER);
    let generator = a_file_in(folder.path(), "solid.fs", SOLID);
    let (mut window, chooser) = window_that_asks();
    let before = window.application().project().clone();

    make_a_generator_at(&mut window, &chooser, &filter, 0, 100);

    let (title, text) = the_message(&window).expect("it says why");
    assert_eq!(title, NOT_A_SHADER_TITLE);
    assert!(
        text.starts_with("keep.fs:") && text.contains("filter") && text.contains("Add effect from file..."),
        "{text}"
    );
    assert_eq!(window.application().project(), &before);

    window.application_mut().interface.dialog = None;
    let clip = window.clip(0, 0);
    chooser.will_answer(Some(generator));
    window.do_this(Command::AddEffectFromFile {
        owner: Member::Clip(clip),
    });
    let (title, text) = the_message(&window).expect("it says why");
    assert_eq!(title, NOT_A_SHADER_TITLE);
    assert!(
        text.contains("generator") && text.contains("Generator from file..."),
        "{text}"
    );
    assert_eq!(window.application().project(), &before, "nothing was put on the video");
}

#[test]
fn a_file_that_is_not_a_good_shader_is_refused_with_the_line_that_is_wrong_and_cancelling_changes_nothing() {
    let folder = tempfile::tempdir().unwrap();
    let bad = a_file_in(
        folder.path(),
        "bad.fs",
        include_str!("../../isf/fixtures/bad_syntax.fs"),
    );
    let (mut window, chooser) = window_that_asks();
    let before = window.application().project().clone();

    make_a_generator_at(&mut window, &chooser, &bad, 0, 100);

    let (title, text) = the_message(&window).expect("it says why");
    assert_eq!(title, NOT_A_SHADER_TITLE);
    assert!(text.starts_with("bad.fs: line 12, column"), "{text}");
    assert_eq!(window.application().project(), &before);

    window.application_mut().interface.dialog = None;
    chooser.will_answer(None);
    let track = window.application().project().tracks()[0].id();
    window.do_this(Command::ChooseAGeneratorFile {
        track,
        frame: TimelineFrameIndex::new(100),
    });
    assert_eq!(the_message(&window), None);
    assert_eq!(window.application().project(), &before);
}

#[test]
fn a_generator_is_only_as_long_as_there_is_room_for_before_the_next_clip_and_is_not_put_where_a_clip_is() {
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
    let text: std::sync::Arc<str> = std::sync::Arc::from(SOLID);

    window.do_this(Command::NewGeneratorClip {
        track: front,
        frame: TimelineFrameIndex::new(0),
        file_name: "solid.fs".to_owned(),
        text: text.clone(),
        images: Vec::new(),
    });
    let (_, duration) = generator_clip(&window, 0, 0);
    assert_eq!(duration, 30);

    window.application_mut().interface.dialog = None;
    window.do_this(Command::NewGeneratorClip {
        track: front,
        frame: TimelineFrameIndex::new(40),
        file_name: "solid.fs".to_owned(),
        text,
        images: Vec::new(),
    });
    let (title, _) = the_message(&window).expect("it says why");
    assert_eq!(title, "There is no room for a generator clip there");
}

#[test]
fn a_generator_is_not_made_inside_the_rectangle_of_a_clip_group_and_the_message_names_the_group() {
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
        file_name: "solid.fs".to_owned(),
        text: std::sync::Arc::from(SOLID),
        images: Vec::new(),
    });

    let (title, text) = the_message(&window).expect("it says why");
    assert_eq!(title, "A generator clip cannot be put there");
    assert!(
        text.contains("would be inside the rectangle of the clip group"),
        "{text}"
    );
}

#[test]
fn what_a_generator_makes_cannot_be_taken_off_by_a_command_and_an_effect_put_on_it_can() {
    let folder = tempfile::tempdir().unwrap();
    let file = a_file_in(folder.path(), "solid.fs", SOLID);
    let (mut window, chooser) = window_that_asks();
    make_a_generator_at(&mut window, &chooser, &file, 0, 100);
    let (clip, _) = generator_clip(&window, 0, 1);
    let target = AnimationTarget::Clip(clip);
    let content = window
        .application()
        .project()
        .placement_of(clip)
        .unwrap()
        .clip
        .as_generator()
        .unwrap()
        .content();
    let before = window.application().project().clone();

    window.do_this(Command::RemoveEffect {
        target,
        instance: content,
    });
    window.do_this(Command::SetEffectEnabled {
        owner: Member::Clip(clip),
        instance: content,
        enabled: false,
    });
    window.do_this(Command::DuplicateEffect {
        owner: Member::Clip(clip),
        instance: content,
    });
    assert_eq!(
        window.application().project(),
        &before,
        "what it makes was left as it was"
    );

    window.do_this(Command::AddEffect {
        target,
        kind: EffectKind::HUE_SHIFT,
    });
    assert_eq!(
        window.application().project().effects_of(target).unwrap().len(),
        2,
        "an effect can be put on it"
    );
}

#[test]
fn the_menu_of_what_a_generator_makes_says_it_cannot_be_changed_and_has_none_of_the_items_that_would_and_another_effect_has_them()
 {
    let folder = tempfile::tempdir().unwrap();
    let file = a_file_in(folder.path(), "solid colour.fs", SOLID);
    let (mut window, chooser) = window_that_asks();
    make_a_generator_at(&mut window, &chooser, &file, 0, 100);
    let (clip, _) = generator_clip(&window, 0, 1);
    window.do_this(Command::AddEffect {
        target: AnimationTarget::Clip(clip),
        kind: EffectKind::HUE_SHIFT,
    });
    window.settle();
    let shown = |window: &Window, label: &str| window.harness.query_by_label_contains(label).is_some();

    window.right_click(window.rectangle_of("Solid colour").center());
    assert!(
        shown(
            &window,
            crate::application::properties_panel::WHAT_A_GENERATOR_MAKES_NOTE
        ),
        "it says why"
    );
    assert!(!shown(&window, "Switch off") && !shown(&window, "Duplicate") && !shown(&window, "Delete"));

    // Close that menu, and open the one of the hue shift: it has them.
    window.do_this(Command::Deselect);
    select_the_clip(&mut window, clip);
    window.right_click(window.rectangle_of("Hue shift").center());
    assert!(shown(&window, "Switch off") && shown(&window, "Duplicate") && shown(&window, "Delete"));
    assert!(!shown(
        &window,
        crate::application::properties_panel::WHAT_A_GENERATOR_MAKES_NOTE
    ));
}

fn select_the_clip(window: &mut Window, clip: ClipId) {
    window.do_this(Command::select_clips(
        vec![clip],
        crate::application::selection::SelectionChange::Replace,
    ));
}

#[test]
fn a_generator_on_the_preview_takes_any_shape_that_a_corner_is_dragged_to_and_is_moved_by_dragging_it() {
    use crate::application::preview_panel::corner_handle_label;
    use crate::project::{CanvasArea, Corner, Placement};

    let file = crate::isf::read(SOLID).unwrap();
    let kind = EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "solid.fs", SOLID)).unwrap();
    let mut project = project_of(300);
    let front = project.add_track_in_front();
    let (clip, _) = project
        .try_insert_generator_clip(front, TimelineFrameIndex::new(0), kind, 200)
        .unwrap();
    project
        .set_clip_placement(clip, Placement::upright(CanvasArea::new(0.3, 0.4, 0.4, 0.2)))
        .unwrap();
    let mut window = Window::showing(project);
    select_the_clip(&mut window, clip);
    let area = |window: &Window| {
        window
            .application()
            .project()
            .placement_of(clip)
            .unwrap()
            .clip
            .placement()
            .unwrap()
            .area
    };
    let before = area(&window);
    let corner = window.rectangle_of(&corner_handle_label(Corner::BottomRight)).center();

    window.drag(corner, corner + egui::vec2(-40.0, 0.0), egui::PointerButton::Primary);

    let after = area(&window);
    assert!(after.width() < before.width() - 0.02, "{before:?} then {after:?}");
    assert!(
        (after.height() - before.height()).abs() < 1e-3,
        "only one way changed, which a picture would not: {after:?}"
    );
}

#[test]
fn a_generator_that_brings_an_image_is_made_into_a_clip_with_it() {
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(
        folder.path().join("tex.png"),
        crate::rgba_picture::RgbaPicture {
            width_in_pixels: 2,
            height_in_pixels: 2,
            rgba_pixels: [255, 128, 0, 255].repeat(4),
        }
        .to_png_bytes()
        .unwrap(),
    )
    .unwrap();
    let source = "/*{ \"IMPORTED\": { \"tex\": { \"PATH\": \"tex.png\" } } }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(tex); }\n";
    let file = a_file_in(folder.path(), "pattern.fs", source);
    let (mut window, chooser) = window_that_asks();

    make_a_generator_at(&mut window, &chooser, &file, 0, 100);

    assert_eq!(the_message(&window), None);
    let (location, _) = generator_clip(&window, 0, 1);
    let project = window.application().project();
    let content = project
        .placement_of(location)
        .unwrap()
        .clip
        .as_generator()
        .unwrap()
        .content();
    let kind = project
        .effects_of(AnimationTarget::Clip(location))
        .unwrap()
        .iter()
        .find(|effect| effect.instance() == content)
        .unwrap()
        .kind();
    assert_eq!(
        kind.shader_source().unwrap().images.len(),
        1,
        "the image came with the shader"
    );
    assert!(kind.is_installed());

    // And one whose image is not there is not made into a clip, and says why.
    let (mut other, other_chooser) = window_that_asks();
    let missing = a_file_in(
        folder.path(),
        "missing.fs",
        "/*{ \"IMPORTED\": { \"tex\": { \"PATH\": \"nowhere.png\" } } }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(tex); }\n",
    );
    make_a_generator_at(&mut other, &other_chooser, &missing, 0, 100);
    let (title, text) = the_message(&other).expect("it was refused");
    assert_eq!(title, NOT_A_SHADER_TITLE);
    assert!(text.contains("the image tex (nowhere.png)"), "{text}");
}
