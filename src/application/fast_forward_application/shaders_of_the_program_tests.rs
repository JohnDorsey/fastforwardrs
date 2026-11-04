// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The shaders that come with the program (`assets/shaders`), in the editor: installed with the ones that are found in folders when the editor is told to (and a person's editor is), left
//! out otherwise, so that every other test finds in a folder all that it finds; listed in the plugin manager; the generators among them in the New menu of a track, each a clip in one step, and one
//! of them drawn by the picture engine in the preview of a window.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use super::sound_plugins_flow::PlacesOfAPersonsPlugins;
use super::test_window::{Window, project_of};
use crate::application::commands::Command;
use crate::application::plugin_manager_window::heading_of_the_shaders_that_can_be_used;
use crate::project::{
    AnimationTarget, ClipKind, EffectInstanceId, EffectKind, ParameterControl, PropertyId, PropertyValue,
    TimelineFrameIndex, ValueKind,
};
use crate::shader_files::{SHADERS_OF_THE_PROGRAM, scan_of_the_shaders_of_the_program};
use crate::test_support::the_installed_plugins_to_myself;
use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};

/// Waits until the scan that is going on is over and a frame has taken what it found (the scan is a thread of its own).
fn wait_for_the_scan(window: &mut Window) {
    let started = Instant::now();
    loop {
        window.settle();
        if !window.application().is_scanning_plugins() {
            return;
        }
        assert!(started.elapsed() < Duration::from_secs(120), "the scan did not end");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn names_of(kinds: &[EffectKind]) -> Vec<&'static str> {
    kinds.iter().map(|kind| kind.name()).collect()
}

fn names_of_the_shaders_of_the_program_that(is_wanted: impl Fn(bool) -> bool) -> Vec<&'static str> {
    scan_of_the_shaders_of_the_program()
        .shaders
        .iter()
        .filter(|shader| is_wanted(shader.kind.is_some_and(EffectKind::is_a_generator)))
        .map(|shader| shader.kind.expect("it can be used").name())
        .collect()
}

#[test]
fn the_shaders_of_the_program_are_installed_when_the_editor_is_told_to_and_the_generators_are_not_effects() {
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(project_of(100));
    window.application_mut().install_the_shaders_of_the_program();
    wait_for_the_scan(&mut window);

    assert!(window.application().plugin_scan_problems().is_empty());
    let all: Vec<&str> = names_of_the_shaders_of_the_program_that(|_| true);
    assert_eq!(all.len(), SHADERS_OF_THE_PROGRAM.len(), "every one of them can be used");
    assert_eq!(
        names_of(&EffectKind::installed_shader_kinds()),
        all,
        "in the order they are listed in"
    );
    assert_eq!(
        names_of(&EffectKind::installed_generators()),
        names_of_the_shaders_of_the_program_that(|is_a_generator| is_a_generator)
    );
    let offered = EffectKind::offered();
    for generator in names_of_the_shaders_of_the_program_that(|is_a_generator| is_a_generator) {
        assert!(
            !names_of(&offered).contains(&generator),
            "{generator} is made into a clip, not put on one"
        );
    }
    for filter in names_of_the_shaders_of_the_program_that(|is_a_generator| !is_a_generator) {
        assert!(names_of(&offered).contains(&filter), "{filter} is offered as an effect");
    }
}

#[test]
fn the_editor_of_a_person_installs_them_with_whatever_else_it_finds_where_it_looks() {
    // (The editor of a person is made with a window, which a test has not: this is what it does after that, with no file kept and no scan program.)
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(project_of(100));
    window.application_mut().find_plugins_in(PlacesOfAPersonsPlugins {
        settings_file: None,
        bundled_folders: Vec::new(),
        scanner_program: None,
        cache_file: None,
        breadcrumb_file: None,
    });
    wait_for_the_scan(&mut window);

    // (The folders of this computer are looked in as well, so that what a person has fetched may be installed besides: all of the program's own are there.)
    let installed = names_of(&EffectKind::installed_shader_kinds());
    for name in names_of_the_shaders_of_the_program_that(|_| true) {
        assert!(
            installed.contains(&name),
            "{name} is installed for a person: {installed:?}"
        );
    }
}

#[test]
fn a_shader_that_is_in_a_folder_too_is_one_kind_of_effect_and_the_one_that_comes_with_the_program_is_listed_first() {
    let _alone = the_installed_plugins_to_myself();
    let folder = tempfile::tempdir().unwrap();
    let plasma = SHADERS_OF_THE_PROGRAM
        .iter()
        .find(|shader| shader.file_name == "Plasma Storm.fs")
        .expect("the program has a plasma");
    std::fs::write(folder.path().join("Plasma Storm.fs"), plasma.text).unwrap();
    let mut window = Window::showing(project_of(100));
    window.application_mut().install_the_shaders_of_the_program();
    // (Asked for while the scan of that goes on: another follows it, which has the folder.)
    window.application_mut().add_plugin_folder(folder.path().to_owned());
    wait_for_the_scan(&mut window);

    let found = &window.application().environment.plugin_search.result.shaders.shaders;
    let copies: Vec<bool> = found
        .iter()
        .filter(|shader| shader.file_name == "Plasma Storm.fs")
        .map(|shader| shader.built_into_the_program)
        .collect();
    assert_eq!(
        copies,
        [true, false],
        "both are found, and the one that comes with the program is first"
    );
    assert_eq!(
        names_of(&EffectKind::installed_shader_kinds())
            .iter()
            .filter(|name| **name == "plasma storm")
            .count(),
        1,
        "the same text is the same kind of effect: it is installed once"
    );
}

#[test]
fn an_editor_that_is_not_told_to_has_none_of_them_so_that_what_a_test_finds_in_a_folder_is_all_it_finds() {
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(project_of(100));
    window.application_mut().start_scanning_plugins();
    wait_for_the_scan(&mut window);
    assert!(EffectKind::installed_shader_kinds().is_empty());
    assert!(EffectKind::installed_generators().is_empty());
}

#[test]
fn the_plugin_manager_counts_them_among_the_shaders_that_can_be_used() {
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(project_of(100));
    window.application_mut().install_the_shaders_of_the_program();
    wait_for_the_scan(&mut window);
    window.harness.get_by_label("Edit").click();
    window.settle();
    window.harness.get_by_label("Plugin manager...").click();
    window.settle();
    assert!(
        window
            .harness
            .query_by_label(&heading_of_the_shaders_that_can_be_used(SHADERS_OF_THE_PROGRAM.len()))
            .is_some(),
        "the heading of the list says how many there are"
    );
}

#[test]
fn a_generator_of_the_program_is_made_into_a_clip_whose_first_effect_it_is_with_its_text_in_the_project() {
    let _alone = the_installed_plugins_to_myself();
    let mut window = Window::showing(project_of(90));
    window.application_mut().install_the_shaders_of_the_program();
    wait_for_the_scan(&mut window);
    let kind = EffectKind::installed_generators()
        .into_iter()
        .find(|kind| kind.name() == "mandelbrot explorer")
        .expect("the Mandelbrot explorer is installed");
    let track = window.application().project().tracks().first().expect("a track").id();
    window.do_this(Command::NewGeneratorClipOfKind {
        track,
        frame: TimelineFrameIndex::new(100),
        kind,
    });

    // (The video of the project is the first clip of the track, and the generator was put after it.)
    let clip = window.clip(0, 1);
    let placement = window.application().project().placement_of(clip).unwrap();
    assert_eq!(placement.clip.kind(), ClipKind::Generator);
    let effects: Vec<EffectKind> = window
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .map(|effect| effect.kind())
        .collect();
    assert_eq!(
        effects,
        [kind],
        "what the clip makes is the first effect of its chain, and the only one"
    );
    assert_eq!(
        kind.shader_source().map(|source| source.text.as_ref()),
        SHADERS_OF_THE_PROGRAM
            .iter()
            .find(|shader| shader.file_name == "Mandelbrot Explorer.fs")
            .map(|shader| shader.text),
        "the project has the text of the shader: it runs where the program has another version of it"
    );
}

#[test]
fn a_generator_of_the_program_is_drawn_by_the_engine_in_the_preview_of_the_window_as_the_picture_it_makes() {
    // (A window whose preview is made by the picture engine, as a person's is, here on the software graphics adapter: a computer that has none has no such window, and the test stops.)
    let Some(mut window) = Window::showing_with_the_engine(project_of(90)) else {
        return;
    };
    // (The kind is made from the text that comes with the program and is not installed: the engine needs its text, and a test that installs takes the plugins of the whole program for itself.)
    let kind = scan_of_the_shaders_of_the_program()
        .shaders
        .iter()
        .find_map(|shader| shader.kind.filter(|kind| kind.name() == "mandelbrot explorer"))
        .expect("the Mandelbrot explorer can be used");
    let track = window.application().project().tracks().first().expect("a track").id();
    window.do_this(Command::NewGeneratorClipOfKind {
        track,
        frame: TimelineFrameIndex::new(100),
        kind,
    });
    // A second into the generator, with no video before or behind it (the video of the project ends at frame 90).
    window.move_current_frame_to(130);
    window.wait_for_the_engine();

    let frame = window
        .engine_preview()
        .expect("the window has an engine")
        .the_frame_shown()
        .expect("a frame is shown");
    let colours: BTreeSet<[u8; 4]> = frame.rgba_pixels.as_chunks::<4>().0.iter().copied().collect();
    assert!(
        colours.len() >= 8,
        "the preview shows the fractal, with many colours in it, and not an empty frame: it has {} colours, {:?}",
        colours.len(),
        colours.iter().take(8).collect::<Vec<_>>()
    );
}

/// How far into the room that the buttons of a row have a box with a number in it may reach. The buttons (the one that reads the number from a mask, and those of its keyframes) come out at the right end of the row while the
/// pointer is on it, and are drawn over whatever is there; the box has a margin of its own of about this much on its right, and what is written in it is to be read while the slider is being dragged, which is when the pointer is on the row.
const MOST_OF_A_BOX_UNDER_THE_BUTTONS_OF_ITS_ROW: f32 = 8.0;

/// The controls of `role` that have the labels `labels`, one for each label in it. (The box of a shader is the lowest in the panel, below those of the clip, which may have a row of the same name: it is the lowest of the
/// controls with the name that is taken.)
fn nodes_with_the_labels_of<'a>(window: &'a Window, role: Role, labels: &[&str]) -> Vec<egui_kittest::Node<'a>> {
    let mut found = Vec::new();
    let distinct: BTreeSet<&str> = labels.iter().copied().collect();
    for label in distinct {
        let wanted = labels.iter().filter(|other| **other == label).count();
        let mut nodes: Vec<_> = window
            .harness
            .query_all_by_role(role)
            .filter(|node| node.accesskit_node().label().as_deref() == Some(label))
            .collect();
        nodes.sort_by(|a, b| a.rect().top().total_cmp(&b.rect().top()));
        found.extend(nodes.split_off(nodes.len().saturating_sub(wanted)));
    }
    found
}

/// The buttons that come out at the right end of the rows of the properties panel while the pointer is on them: those of the keyframes, and the one that reads a number from a mask.
fn buttons_of_the_rows(window: &Window) -> Vec<egui::Rect> {
    window
        .harness
        .query_all_by_role(Role::Button)
        .filter(|node| {
            let label = node.accesskit_node().label().unwrap_or_default();
            label.starts_with("Previous keyframe of ")
                || label.starts_with("Next keyframe of ")
                || label.starts_with("Make a keyframe of ")
                || label.starts_with("Take away the keyframe")
                || (label.starts_with("Read ") && label.ends_with(" from a mask"))
        })
        .map(|node| node.rect())
        .collect()
}

/// Where the rows end: the right edge of the last of the buttons. (Everything in the panel is as wide as its widest row: a row that is too wide for the panel makes all of them go on past its edge, where they are cut off.)
fn the_end_of_the_rows(window: &Window) -> f32 {
    let buttons = buttons_of_the_rows(window);
    assert!(!buttons.is_empty(), "the rows have buttons");
    buttons.iter().map(|button| button.right()).fold(f32::MIN, f32::max)
}

/// The box of `kind` in the properties panel of a window, as a person has it: a generator is a clip of its own, a filter is put on the video of the project, and the clip is selected. With `longest_words`, every choice has the longest of
/// its words chosen (a list that is shut is as wide as the words that are in it).
fn window_showing_the_box_of(kind: EffectKind, longest_words: bool) -> Window {
    let mut window = Window::showing(project_of(90));
    let clip = if kind.is_a_generator() {
        let track = window.application().project().tracks().first().expect("a track").id();
        window.do_this(Command::NewGeneratorClipOfKind {
            track,
            frame: TimelineFrameIndex::new(100),
            kind,
        });
        window.clip(0, 1)
    } else {
        let clip = window.clip(0, 0);
        window.do_this(Command::AddEffect {
            target: AnimationTarget::Clip(clip),
            kind,
        });
        clip
    };
    if longest_words {
        for (slot, parameter) in kind.parameters().iter().enumerate() {
            if let ParameterControl::Choice(options) = &parameter.control {
                let longest = options
                    .iter()
                    .max_by_key(|option| option.label.chars().count())
                    .expect("a choice has words");
                window.do_this(Command::SetEffectValue {
                    target: AnimationTarget::Clip(clip),
                    property: PropertyId::effect_value(kind, EffectInstanceId::FIRST, slot as u16),
                    value: PropertyValue::Words(longest.id.to_string()),
                });
            }
        }
    }
    super::preview_layer_tests::select(&mut window, clip);
    window.settle();
    window
}

/// **What a person reads and reaches in the box of a shader that comes with the program stays in the panel.** The labels of the inputs and the words of the choices are written by whoever writes the shader, and the panel is 340 points wide:
/// a number whose label is long has its box under the buttons that come out of the end of its row (which hide what the number is while the slider is dragged), and a choice whose words are long makes the whole box wider than the panel,
/// so that the rows go on past its edge and are cut off (both were seen, in the pictures of the box of the Mandelbrot explorer: `write_screenshots_of_a_generator_that_comes_with_the_program`). This looks at the boxes as they are drawn, for every
/// shader that there is, as it begins and with the longest words of every choice chosen.
#[test]
fn nothing_in_the_box_of_a_shader_of_the_program_is_under_the_buttons_of_its_row_or_makes_the_box_wider_than_the_panel()
{
    let end_of_the_rows = {
        let mut window = Window::showing(project_of(90));
        let video = window.clip(0, 0);
        super::preview_layer_tests::select(&mut window, video);
        window.settle();
        the_end_of_the_rows(&window)
    };
    let mut problems = Vec::new();
    let (mut numbers_checked, mut choices_checked, mut switches_checked) = (0, 0, 0);
    for shader in scan_of_the_shaders_of_the_program().shaders {
        let kind = shader.kind.expect("it can be used");
        let name = shader.file_name.as_str();
        let labels_of = |wanted: ValueKind, control_is: fn(&ParameterControl) -> bool| -> Vec<&str> {
            kind.parameters()
                .iter()
                .filter(|parameter| parameter.value_kind() == wanted && control_is(&parameter.control))
                .map(|parameter| parameter.label.as_ref())
                .collect()
        };
        let number_labels = labels_of(ValueKind::Number, |control| {
            matches!(control, ParameterControl::Automatic | ParameterControl::Slider)
        });
        let choice_labels = labels_of(ValueKind::Words, |control| {
            matches!(control, ParameterControl::Choice(_))
        });
        let switch_labels = labels_of(ValueKind::Flag, |_| true);

        // As it begins: the numbers, whose boxes are not to be under the buttons of their rows, and the switches.
        let window = window_showing_the_box_of(kind, false);
        let wider = the_end_of_the_rows(&window) - end_of_the_rows;
        if wider > 0.5 {
            problems.push(format!("{name}: the box begins {wider:.0} points wider than the panel"));
        }
        let buttons = buttons_of_the_rows(&window);
        let value_boxes: Vec<egui::Rect> = window
            .harness
            .query_all_by_role(Role::SpinButton)
            .map(|node| node.rect())
            .collect();
        let sliders = nodes_with_the_labels_of(&window, Role::Slider, &number_labels);
        assert_eq!(sliders.len(), number_labels.len(), "{name}: every number has a slider");
        for slider in &sliders {
            let slider_rectangle = slider.rect();
            let label = slider.accesskit_node().label().unwrap_or_default();
            let on_the_row = |rectangle: &&egui::Rect| {
                (rectangle.top() - slider_rectangle.top()).abs() <= 4.0
                    && rectangle.left() >= slider_rectangle.right() - 1.0
            };
            let value_box = value_boxes
                .iter()
                .filter(on_the_row)
                .min_by(|a, b| a.left().total_cmp(&b.left()))
                .unwrap_or_else(|| panic!("{name}: the number {label:?} has a box to read it in"));
            let room_begins = buttons
                .iter()
                .filter(on_the_row)
                .map(|button| button.left())
                .fold(f32::MAX, f32::min);
            assert!(room_begins < f32::MAX, "{name}: the number {label:?} has buttons");
            let under_them = value_box.right() - room_begins;
            if under_them > MOST_OF_A_BOX_UNDER_THE_BUTTONS_OF_ITS_ROW {
                problems.push(format!(
                    "{name}: the box of the number {label:?} is {under_them:.0} points under the buttons of its row (it ends at {:.0}, and they begin at {room_begins:.0})",
                    value_box.right()
                ));
            }
            numbers_checked += 1;
        }
        let switches = nodes_with_the_labels_of(&window, Role::CheckBox, &switch_labels);
        assert_eq!(switches.len(), switch_labels.len(), "{name}: every switch is there");
        for switch in &switches {
            let past_the_end = switch.rect().right() - end_of_the_rows;
            if past_the_end > 0.0 {
                problems.push(format!(
                    "{name}: the switch {:?} reaches {past_the_end:.0} points past the end of the rows",
                    switch.accesskit_node().label().unwrap_or_default()
                ));
            }
            switches_checked += 1;
        }

        // With the longest words of every choice chosen: no list is wider than there is room for.
        if !choice_labels.is_empty() {
            let window = window_showing_the_box_of(kind, true);
            let lists = nodes_with_the_labels_of(&window, Role::ComboBox, &choice_labels);
            assert_eq!(lists.len(), choice_labels.len(), "{name}: every choice is there");
            for list in &lists {
                let past_the_end = list.rect().right() - end_of_the_rows;
                if past_the_end > 0.0 {
                    problems.push(format!(
                        "{name}: the choice {:?}, with {:?} chosen, reaches {past_the_end:.0} points past the end of the rows, which makes the box wider than the panel",
                        list.accesskit_node().label().unwrap_or_default(),
                        list.accesskit_node().value().unwrap_or_default()
                    ));
                }
                choices_checked += 1;
            }
        }
    }
    assert!(
        numbers_checked > 100 && choices_checked > 30 && switches_checked >= 1,
        "the test looks at the controls: {numbers_checked} numbers, {choices_checked} choices, {switches_checked} switches"
    );
    assert!(
        problems.is_empty(),
        "{} problem(s):\n{}",
        problems.len(),
        problems.join("\n")
    );
}
