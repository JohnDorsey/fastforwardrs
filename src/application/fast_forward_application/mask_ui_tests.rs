// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window side of reading a number of a shader from a mask (`docs/shader-masks-design.md`): the mask button next to the keyframe buttons of a number that has a range, the thumbnail of
//! the mask in the place of the slider and the keyframe buttons while the number is read from a mask, and the window that the button and the thumbnail open, whose choices are steps of the
//! history. Nothing else of the screen changes for a number that is not read from a mask.

use egui_kittest::kittest::Queryable;

use super::keyframe_tests::window_with_a_video;
use super::preview_layer_tests::select;
use super::test_window::{Window, project_with_a_video_and_a_sound};
use super::undo_tests::undo;
use crate::application::commands::Command;
use crate::application::mask_settings_window::{
    CHANNEL_LABEL, NO_TRACK_CHOSEN_LABEL, TRACK_LABEL, USE_THE_SLIDER_AGAIN_LABEL, VALUE_WHERE_NOTHING_LABEL,
    WhichValue, name_of_a_channel,
};
use crate::application::properties_panel::{
    KeyframeButton, keyframe_button_label, label_of_the_thumbnail, mask_button_label,
};
use crate::project::{
    AnimationTarget, ClipId, DrivenValue, EffectInstanceId, EffectKind, MaskChannel, MaskSource, Member, TrackId,
};

/// A shader with a number that has a range (called *how much*), a colour and a switch.
const TINT: &str = r#"/*{
	"INPUTS": [
		{ "NAME": "inputImage", "TYPE": "image" },
		{ "NAME": "tint", "LABEL": "Tint colour", "TYPE": "color", "DEFAULT": [1.0, 0.0, 0.0, 1.0] },
		{ "NAME": "amount", "LABEL": "How much", "TYPE": "float", "MIN": 0.0, "MAX": 1.0, "DEFAULT": 0.5 },
		{ "NAME": "invert", "LABEL": "Invert it", "TYPE": "bool", "DEFAULT": false }
	]
}*/
void main() { gl_FragColor = IMG_THIS_NORM_PIXEL(inputImage); }
"#;

/// The place of *how much* among the values of the shader.
const HOW_MUCH: usize = 1;

fn the_shader() -> EffectKind {
    let file = crate::isf::read(TINT).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "tint.fs", TINT)).unwrap()
}

/// A window with a video that is selected and has the shader on it: the window, the clip, and the other track (a sound's, in front of the video's), which the number can be read from.
fn window_with_the_shader_on_the_video() -> (Window, ClipId, TrackId) {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let mut window = Window::showing(project_with_a_video_and_a_sound(300, 5));
    let project = window.application().project();
    let clip = project.tracks()[1].placement_at_index(0).unwrap().id;
    let the_other = project.tracks()[0].id();
    select(&mut window, clip);
    window.do_this(Command::AddEffect {
        target: AnimationTarget::Clip(clip),
        kind: the_shader(),
    });
    (window, clip, the_other)
}

fn which(clip: ClipId) -> WhichValue {
    WhichValue {
        owner: Member::Clip(clip),
        instance: EffectInstanceId::FIRST,
        slot: HOW_MUCH,
    }
}

fn read_from(window: &mut Window, clip: ClipId, track: TrackId, channel: MaskChannel, nothing: f32) {
    window.do_this(Command::SetDrivenValue {
        owner: Member::Clip(clip),
        instance: EffectInstanceId::FIRST,
        slot: HOW_MUCH,
        driven: Some(DrivenValue::from_a_track(track, channel, nothing)),
    });
}

fn is_shown(window: &Window, label: &str) -> bool {
    window.harness.query_by_label(label).is_some()
}

fn driven_now(window: &Window, clip: ClipId) -> Option<DrivenValue> {
    window
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .driven_values()
        .of(HOW_MUCH)
        .copied()
}

#[test]
fn a_number_that_has_a_range_has_the_mask_button_and_a_colour_and_a_switch_have_none() {
    let (window, ..) = window_with_the_shader_on_the_video();

    assert!(
        is_shown(&window, &mask_button_label("how much")),
        "the number with a range"
    );
    assert!(
        is_shown(
            &window,
            &keyframe_button_label("how much", KeyframeButton::Stopwatch, false)
        ),
        "and its buttons of keyframes are as they were"
    );
    for name in ["tint colour", "invert it"] {
        assert!(!is_shown(&window, &mask_button_label(name)), "{name}");
    }
}

#[test]
fn an_effect_that_is_not_a_shader_has_no_mask_button() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, clip) = window_with_a_video();
    select(&mut window, clip);
    window.do_this(Command::AddEffect {
        target: AnimationTarget::Clip(clip),
        kind: EffectKind::HUE_SHIFT,
    });

    assert!(is_shown(
        &window,
        &keyframe_button_label(EffectKind::HUE_SHIFT.name(), KeyframeButton::Stopwatch, false)
    ));
    assert!(!is_shown(&window, &mask_button_label(EffectKind::HUE_SHIFT.name())));
}

#[test]
fn the_mask_button_opens_the_window_of_the_number_which_offers_the_track_and_has_the_channel_and_the_rest_dimmed() {
    let (mut window, clip, _) = window_with_the_shader_on_the_video();
    assert_eq!(window.application().interface.mask_settings_window_open_for, None);

    let button = window.rectangle_of(&mask_button_label("how much")).center();
    window.click(button);

    assert_eq!(
        window.application().interface.mask_settings_window_open_for,
        Some(which(clip))
    );
    for label in [
        TRACK_LABEL,
        CHANNEL_LABEL,
        VALUE_WHERE_NOTHING_LABEL,
        USE_THE_SLIDER_AGAIN_LABEL,
    ] {
        assert!(is_shown(&window, label), "{label}");
    }
    assert!(
        window.harness.query_by_value(NO_TRACK_CHOSEN_LABEL).is_some(),
        "no track is chosen yet"
    );
    assert!(
        window
            .application()
            .project()
            .effects_of(AnimationTarget::Clip(clip))
            .is_some()
    );
    assert_eq!(
        driven_now(&window, clip),
        None,
        "opening the window reads nothing from anywhere"
    );
}

#[test]
fn a_number_that_is_read_from_a_mask_has_the_thumbnail_in_the_place_of_its_slider_and_no_buttons() {
    let (mut window, clip, the_other) = window_with_the_shader_on_the_video();
    assert!(is_shown(
        &window,
        &keyframe_button_label("how much", KeyframeButton::Stopwatch, false)
    ));

    read_from(&mut window, clip, the_other, MaskChannel::Green, 0.25);
    window.settle();

    assert!(is_shown(&window, &label_of_the_thumbnail("how much")), "the thumbnail");
    assert!(
        !is_shown(
            &window,
            &keyframe_button_label("how much", KeyframeButton::Stopwatch, false)
        ),
        "no stopwatch: the slider and its keyframes are kept and not used"
    );
    assert!(
        !is_shown(&window, &mask_button_label("how much")),
        "and no mask button: the thumbnail is the way in"
    );
    // The rows of the other values are as they were.
    assert!(is_shown(
        &window,
        &keyframe_button_label("tint colour", KeyframeButton::Stopwatch, false)
    ));
}

#[test]
fn the_thumbnail_opens_the_window_of_the_number() {
    let (mut window, clip, the_other) = window_with_the_shader_on_the_video();
    read_from(&mut window, clip, the_other, MaskChannel::Red, 0.0);
    window.settle();

    let thumbnail = window.rectangle_of(&label_of_the_thumbnail("how much")).center();
    window.click(thumbnail);

    assert_eq!(
        window.application().interface.mask_settings_window_open_for,
        Some(which(clip))
    );
}

#[test]
fn the_buttons_of_the_channels_in_the_window_change_what_the_number_is_read_as_and_use_the_slider_again_lets_go() {
    let (mut window, clip, the_other) = window_with_the_shader_on_the_video();
    read_from(&mut window, clip, the_other, MaskChannel::Green, 0.25);
    window.do_this(Command::ShowTheMaskSettings(which(clip)));
    window.settle();

    for channel in [MaskChannel::Blue, MaskChannel::Brightness] {
        let button = window.rectangle_of(name_of_a_channel(channel)).center();
        window.click(button);
        assert_eq!(
            driven_now(&window, clip),
            Some(DrivenValue {
                source: MaskSource::Track(the_other),
                channel,
                value_where_nothing: 0.25
            })
        );
    }
    let button = window.rectangle_of(USE_THE_SLIDER_AGAIN_LABEL).center();
    window.click(button);

    assert_eq!(driven_now(&window, clip), None);
    window.settle();
    assert!(
        is_shown(
            &window,
            &keyframe_button_label("how much", KeyframeButton::Stopwatch, false)
        ),
        "the slider is back, with its buttons"
    );
}

#[test]
fn what_the_window_does_is_one_step_of_the_history_each_and_undoing_it_reads_the_slider_again() {
    let (mut window, clip, the_other) = window_with_the_shader_on_the_video();
    let before = window.application().project().clone();
    read_from(&mut window, clip, the_other, MaskChannel::Green, 0.25);
    assert!(driven_now(&window, clip).is_some());

    undo(&mut window);

    assert_eq!(window.application().project(), &before);
}

#[test]
fn the_window_goes_away_when_the_effect_does() {
    let (mut window, clip, _) = window_with_the_shader_on_the_video();
    window.do_this(Command::ShowTheMaskSettings(which(clip)));
    window.settle();
    assert_eq!(
        window.application().interface.mask_settings_window_open_for,
        Some(which(clip))
    );

    window.do_this(Command::RemoveEffect {
        target: AnimationTarget::Clip(clip),
        instance: EffectInstanceId::FIRST,
    });
    window.settle();

    assert_eq!(window.application().interface.mask_settings_window_open_for, None);
    assert!(!is_shown(&window, TRACK_LABEL));
}

#[test]
fn choosing_a_track_in_the_window_reads_the_number_from_it_as_brightness_and_where_it_has_nothing_the_slider_is_what_it_was()
 {
    let (mut window, clip, the_other) = window_with_the_shader_on_the_video();
    window.do_this(Command::ShowTheMaskSettings(which(clip)));
    window.settle();
    let title = window
        .application()
        .project()
        .track(the_other)
        .unwrap()
        .title()
        .to_owned();

    window.harness.get_by_value(NO_TRACK_CHOSEN_LABEL).click();
    window.settle();
    // (The title is also the title of the track in the timeline: the item of the list is the one that is on top of the others, the last.)
    window.harness.get_all_by_label(&title).last().unwrap().click();
    window.settle();

    assert_eq!(
        driven_now(&window, clip),
        Some(DrivenValue {
            source: MaskSource::Track(the_other),
            channel: MaskChannel::Brightness,
            value_where_nothing: 0.5
        }),
        "brightness, and the 0.5 that the slider said"
    );
}

/// Makes pictures for looking at: `cargo test --lib --release write_pictures_of_the_mask -- --ignored` with `FASTFORWARDRS_SCREENSHOT_FOLDER` set to the folder they go in.
#[test]
#[ignore = "makes pictures for looking at; run with --ignored and the screenshot folder variable set"]
fn write_pictures_of_the_mask_button_the_thumbnail_and_the_window() {
    let folder = std::path::PathBuf::from(
        std::env::var_os("FASTFORWARDRS_SCREENSHOT_FOLDER")
            .expect("set FASTFORWARDRS_SCREENSHOT_FOLDER to the folder the pictures go in"),
    );
    std::fs::create_dir_all(&folder).unwrap();
    let (mut window, clip, the_other) = window_with_the_shader_on_the_video();
    let save = |window: &mut Window, name: &str| {
        window.settle();
        let picture = window.harness.render().expect("the window could be drawn");
        picture.save(folder.join(format!("{name}.png"))).unwrap();
    };

    // The row of the number with the pointer over it: the mask button is next to the buttons of keyframes.
    let row = window.rectangle_of(&mask_button_label("how much"));
    window.move_pointer_to(row.center());
    save(&mut window, "mask_1_the_row_of_a_number_with_the_mask_button");

    // The window, before a track is chosen.
    window.click(row.center());
    save(&mut window, "mask_2_the_window_before_a_track_is_chosen");

    // A track is chosen: the thumbnail is in the place of the slider, and the window has its controls.
    read_from(&mut window, clip, the_other, MaskChannel::Brightness, 0.5);
    window.move_pointer_to(egui::pos2(5.0, 5.0));
    save(&mut window, "mask_3_the_thumbnail_and_the_window_with_a_track");

    // The window closed, the thumbnail alone.
    window.application_mut().interface.mask_settings_window_open_for = None;
    save(&mut window, "mask_4_the_thumbnail_alone");
}
