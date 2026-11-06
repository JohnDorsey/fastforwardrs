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
use crate::application::mask_graph::{self, place_on_the_screen};
use crate::application::mask_settings_window::{
    CHANNEL_LABEL, NO_TRACK_CHOSEN_LABEL, REAL_BACKDROP_LABEL, STRAIGHT_LINE_AGAIN_LABEL, TRACK_LABEL,
    USE_THE_SLIDER_AGAIN_LABEL, VALUE_WHERE_NOTHING_LABEL, WhichValue, name_of_a_channel,
};
use crate::application::properties_panel::{
    KeyframeButton, keyframe_button_label, label_of_the_thumbnail, mask_button_label,
};
use crate::project::{
    AnimationTarget, ClipId, DrivenValue, EffectInstanceId, EffectKind, GroupId, MaskChannel, MaskMap, Member, TrackId,
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
    window_with_this_shader_on_the_video(the_shader())
}

/// The same with the shader `kind`. (Whoever calls it keeps the installed plugins in peace for as long as the window lives.)
pub(super) fn window_with_this_shader_on_the_video(kind: EffectKind) -> (Window, ClipId, TrackId) {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let mut window = Window::showing(project_with_a_video_and_a_sound(300, 5));
    let project = window.application().project();
    let clip = project.tracks()[1].placement_at_index(0).unwrap().id;
    let the_other = project.tracks()[0].id();
    select(&mut window, clip);
    window.do_this(Command::AddEffect {
        target: AnimationTarget::Clip(clip),
        kind,
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
            Some(DrivenValue::from_a_track(the_other, channel, 0.25))
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
        Some(DrivenValue::from_a_track(the_other, MaskChannel::Brightness, 0.5)),
        "brightness, and the 0.5 that the slider said"
    );
}

#[test]
fn a_clip_group_and_a_track_group_are_offered_as_sources_and_choosing_one_reads_the_number_from_it() {
    use crate::project::{GroupKind, MaskSource};
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let mut project = super::clip_drag_tests::project_of_tracks(&[&[(0, 20)], &[(0, 20)], &[(0, 20)]]);
    let clips = [0, 1, 2].map(|place| project.tracks()[place].placement_at_index(0).unwrap().id);
    let clip_group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(clips[1]), Member::Clip(clips[2])])
        .unwrap();
    project.rename_group(clip_group, "the two behind").unwrap();
    let mut window = Window::showing(project);
    select(&mut window, clips[0]);
    window.do_this(Command::AddEffect {
        target: AnimationTarget::Clip(clips[0]),
        kind: the_shader(),
    });
    window.do_this(Command::ShowTheMaskSettings(which(clips[0])));
    window.settle();

    window.harness.get_by_value(NO_TRACK_CHOSEN_LABEL).click();
    window.settle();
    window
        .harness
        .get_all_by_label("the two behind (clip group)")
        .last()
        .unwrap()
        .click();
    window.settle();

    assert_eq!(
        driven_now(&window, clips[0]).map(|driven| driven.source),
        Some(MaskSource::ClipGroup(clip_group))
    );
    // (The name of the source is on the list's button now, and the thumbnail says it too.)
    assert!(is_shown(&window, &label_of_the_thumbnail("how much")));
    assert_eq!(
        window
            .application()
            .project()
            .name_of_the_source(MaskSource::ClipGroup(clip_group)),
        "the two behind"
    );
}

// ---- the graph that maps the value of the mask (docs/shader-masks-design.md, 8)

/// The square of the graph in the window.
fn the_square_of_the_graph(window: &Window) -> egui::Rect {
    window.rectangle_of(mask_graph::LABEL)
}

/// Where on the screen the point `(place, height)` of a graph is.
fn where_on_the_screen(window: &Window, point: (f32, f32)) -> egui::Pos2 {
    place_on_the_screen(the_square_of_the_graph(window), point)
}

fn the_map_now(window: &Window, clip: ClipId) -> MaskMap {
    driven_now(window, clip).expect("the number is read from a mask").map
}

/// A window with the number read from the other track and the window of the mask open.
fn window_with_the_window_of_the_mask_open() -> (Window, ClipId) {
    let (mut window, clip, the_other) = window_with_the_shader_on_the_video();
    read_from(&mut window, clip, the_other, MaskChannel::Red, 0.25);
    window.do_this(Command::ShowTheMaskSettings(which(clip)));
    window.settle();
    (window, clip)
}

#[test]
fn the_window_shows_the_graph_which_is_the_straight_line_until_a_point_is_added() {
    let (window, clip) = window_with_the_window_of_the_mask_open();

    assert!(is_shown(&window, mask_graph::LABEL));
    assert_eq!(the_map_now(&window, clip), MaskMap::Straight);
    assert!(
        is_shown(&window, STRAIGHT_LINE_AGAIN_LABEL),
        "the button is there, and dimmed while the map is straight"
    );
}

#[test]
fn a_click_on_the_line_adds_a_point_there_and_the_map_is_a_graph() {
    let (mut window, clip) = window_with_the_window_of_the_mask_open();

    let on_the_line = where_on_the_screen(&window, (0.5, 0.5));
    window.click(on_the_line);

    let MaskMap::Graph(graph) = the_map_now(&window, clip) else {
        panic!("a point was added: {:?}", the_map_now(&window, clip));
    };
    assert_eq!(graph.points().len(), 3);
    let (place, height) = graph.points()[1];
    assert!(
        (place - 0.5).abs() < 0.02 && (height - 0.5).abs() < 0.02,
        "the new point is on the line, at the place clicked: {:?}",
        graph.points()
    );
}

#[test]
fn a_click_off_the_line_adds_nothing() {
    let (mut window, clip) = window_with_the_window_of_the_mask_open();

    let off_the_line = where_on_the_screen(&window, (0.5, 0.9));
    window.click(off_the_line);

    assert_eq!(the_map_now(&window, clip), MaskMap::Straight);
}

#[test]
fn dragging_a_point_moves_it() {
    let (mut window, clip) = window_with_the_window_of_the_mask_open();
    let on_the_line = where_on_the_screen(&window, (0.5, 0.5));
    window.click(on_the_line);

    let point = where_on_the_screen(&window, (0.5, 0.5));
    let higher = where_on_the_screen(&window, (0.5, 0.9));
    window.drag(point, higher, egui::PointerButton::Primary);

    let MaskMap::Graph(graph) = the_map_now(&window, clip) else {
        panic!("still a graph");
    };
    let (place, height) = graph.points()[1];
    assert!(
        (place - 0.5).abs() < 0.03 && (height - 0.9).abs() < 0.03,
        "the point went up: {:?}",
        graph.points()
    );
}

#[test]
fn pressing_on_the_line_and_dragging_adds_a_point_and_carries_it() {
    let (mut window, clip) = window_with_the_window_of_the_mask_open();

    let on_the_line = where_on_the_screen(&window, (0.3, 0.3));
    let somewhere_else = where_on_the_screen(&window, (0.3, 0.8));
    window.drag(on_the_line, somewhere_else, egui::PointerButton::Primary);

    let MaskMap::Graph(graph) = the_map_now(&window, clip) else {
        panic!("a point was added and moved");
    };
    assert_eq!(graph.points().len(), 3, "{:?}", graph.points());
    let (place, height) = graph.points()[1];
    assert!(
        (place - 0.3).abs() < 0.03 && (height - 0.8).abs() < 0.03,
        "{:?}",
        graph.points()
    );
}

#[test]
fn an_end_of_the_graph_moves_up_and_down_only() {
    let (mut window, clip) = window_with_the_window_of_the_mask_open();

    let top_right = where_on_the_screen(&window, (1.0, 1.0));
    let lower_and_to_the_left = where_on_the_screen(&window, (0.4, 0.3));
    window.drag(top_right, lower_and_to_the_left, egui::PointerButton::Primary);

    let MaskMap::Graph(graph) = the_map_now(&window, clip) else {
        panic!("the end moved");
    };
    assert_eq!(graph.points().len(), 2);
    let (place, height) = graph.points()[1];
    assert_eq!(place, 1.0, "the end stays at the place 1");
    assert!((height - 0.3).abs() < 0.03, "{height}");
}

#[test]
fn a_right_click_on_a_point_takes_it_away_and_a_graph_that_is_the_straight_line_again_is_a_straight_map() {
    let (mut window, clip) = window_with_the_window_of_the_mask_open();
    let on_the_line = where_on_the_screen(&window, (0.5, 0.5));
    window.click(on_the_line);
    assert!(matches!(the_map_now(&window, clip), MaskMap::Graph(_)));

    let point = where_on_the_screen(&window, (0.5, 0.5));
    window.right_click(point);

    assert_eq!(the_map_now(&window, clip), MaskMap::Straight);
}

#[test]
fn the_button_of_the_straight_line_puts_the_map_back_and_keeps_the_track_and_the_channel() {
    let (mut window, clip) = window_with_the_window_of_the_mask_open();
    let point = where_on_the_screen(&window, (0.5, 0.5));
    let higher = where_on_the_screen(&window, (0.5, 0.9));
    window.drag(point, higher, egui::PointerButton::Primary);
    let before = driven_now(&window, clip).unwrap();
    assert!(matches!(before.map, MaskMap::Graph(_)));
    window.settle();

    let button = window.rectangle_of(STRAIGHT_LINE_AGAIN_LABEL).center();
    window.click(button);

    let after = driven_now(&window, clip).unwrap();
    assert_eq!(after.map, MaskMap::Straight);
    assert_eq!(
        (after.source, after.channel, after.value_where_nothing),
        (before.source, before.channel, before.value_where_nothing)
    );
}

#[test]
fn changing_the_channel_keeps_the_graph() {
    let (mut window, clip) = window_with_the_window_of_the_mask_open();
    let on_the_line = where_on_the_screen(&window, (0.5, 0.5));
    window.click(on_the_line);
    let MaskMap::Graph(graph) = the_map_now(&window, clip) else {
        panic!("a point was added");
    };

    let button = window.rectangle_of(name_of_a_channel(MaskChannel::Blue)).center();
    window.click(button);

    let now = driven_now(&window, clip).unwrap();
    assert_eq!(now.channel, MaskChannel::Blue);
    assert_eq!(now.map, MaskMap::Graph(graph));
}

#[test]
fn a_graph_is_a_step_of_the_history_and_undoing_it_takes_the_graph_away() {
    let (mut window, clip) = window_with_the_window_of_the_mask_open();
    let on_the_line = where_on_the_screen(&window, (0.5, 0.5));
    window.click(on_the_line);
    assert!(matches!(the_map_now(&window, clip), MaskMap::Graph(_)));

    undo(&mut window);

    // (The test window joins the edits of one number whenever they are made, so this goes back past the choice of the track too; a person's edits a second apart are steps of their own.)
    assert!(
        !matches!(
            driven_now(&window, clip).map(|driven| driven.map),
            Some(MaskMap::Graph(_))
        ),
        "{:?}",
        driven_now(&window, clip)
    );
}

#[test]
fn while_no_track_is_chosen_the_graph_is_shown_and_cannot_be_changed() {
    let (mut window, clip, _) = window_with_the_shader_on_the_video();
    window.do_this(Command::ShowTheMaskSettings(which(clip)));
    window.settle();
    assert!(is_shown(&window, mask_graph::LABEL));

    let on_the_line = where_on_the_screen(&window, (0.5, 0.5));
    window.click(on_the_line);

    assert_eq!(
        driven_now(&window, clip),
        None,
        "nothing is read from a mask by touching the graph"
    );
}

// ---- whether the transitions of the mask see the real backdrop

#[test]
fn the_window_has_the_checkbox_of_the_real_backdrop_which_is_on_until_it_is_turned_off_and_on_again() {
    let (mut window, clip) = window_with_the_window_of_the_mask_open();
    assert!(driven_now(&window, clip).unwrap().real_backdrop, "on by default");
    assert!(is_shown(&window, REAL_BACKDROP_LABEL));

    let checkbox = window.rectangle_of(REAL_BACKDROP_LABEL).center();
    window.click(checkbox);
    assert!(!driven_now(&window, clip).unwrap().real_backdrop, "turned off");
    window.settle();
    let checkbox = window.rectangle_of(REAL_BACKDROP_LABEL).center();
    window.click(checkbox);
    assert!(driven_now(&window, clip).unwrap().real_backdrop, "and on again");
}

#[test]
fn turning_the_real_backdrop_off_keeps_the_rest_of_what_the_number_is_read_as() {
    let (mut window, clip) = window_with_the_window_of_the_mask_open();
    let before = driven_now(&window, clip).unwrap();

    let checkbox = window.rectangle_of(REAL_BACKDROP_LABEL).center();
    window.click(checkbox);

    let after = driven_now(&window, clip).unwrap();
    assert_eq!(after, before.with_the_real_backdrop(false));
}

#[test]
fn the_checkbox_of_the_real_backdrop_cannot_be_used_while_no_track_is_chosen() {
    let (mut window, clip, _) = window_with_the_shader_on_the_video();
    window.do_this(Command::ShowTheMaskSettings(which(clip)));
    window.settle();

    let checkbox = window.rectangle_of(REAL_BACKDROP_LABEL).center();
    window.click(checkbox);

    assert_eq!(driven_now(&window, clip), None);
}

// ---- the rows of the effects of a group

/// A window with a tag of the first two clips selected, and the shader on it: the window, the tag, and a track that its number can be read from.
fn window_with_the_shader_on_a_selected_group() -> (Window, GroupId, TrackId) {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, group, _) = super::group_box_tests::window_with_a_tag_of_the_first_two_clips_selected();
    window.do_this(Command::AddGroupEffect {
        group,
        kind: the_shader(),
    });
    // (A track of its own: the one that the clips of the group are on would be a ring.)
    let a_track = window.application_mut().document.project.add_track_in_front();
    window.settle();
    (window, group, a_track)
}

fn the_group_s_value(group: GroupId) -> WhichValue {
    WhichValue {
        owner: Member::Group(group),
        instance: EffectInstanceId::FIRST,
        slot: HOW_MUCH,
    }
}

#[test]
fn the_number_of_a_shader_on_a_group_has_the_mask_button_and_no_buttons_of_keyframes() {
    let (window, ..) = window_with_the_shader_on_a_selected_group();

    assert!(is_shown(&window, &mask_button_label("how much")));
    assert!(
        window.harness.query_all_by_label_contains("stopwatch").next().is_none(),
        "the values of the effects of a group have no keyframes"
    );
    for name in ["tint colour", "invert it"] {
        assert!(!is_shown(&window, &mask_button_label(name)), "{name}");
    }
}

#[test]
fn the_mask_button_of_a_group_opens_the_window_of_the_number_of_that_group() {
    let (mut window, group, _) = window_with_the_shader_on_a_selected_group();

    let button = window.rectangle_of(&mask_button_label("how much")).center();
    window.click(button);

    assert_eq!(
        window.application().interface.mask_settings_window_open_for,
        Some(the_group_s_value(group))
    );
    assert!(is_shown(&window, TRACK_LABEL));
}

#[test]
fn a_number_of_a_group_that_is_read_from_a_mask_has_the_thumbnail_and_the_thumbnail_opens_the_window() {
    let (mut window, group, a_track) = window_with_the_shader_on_a_selected_group();
    window.do_this(Command::SetDrivenValue {
        owner: Member::Group(group),
        instance: EffectInstanceId::FIRST,
        slot: HOW_MUCH,
        driven: Some(DrivenValue::from_a_track(a_track, MaskChannel::Red, 0.25)),
    });
    window.settle();

    assert!(is_shown(&window, &label_of_the_thumbnail("how much")));
    assert!(
        !is_shown(&window, &mask_button_label("how much")),
        "the thumbnail is in its place"
    );
    let thumbnail = window.rectangle_of(&label_of_the_thumbnail("how much")).center();
    window.click(thumbnail);

    assert_eq!(
        window.application().interface.mask_settings_window_open_for,
        Some(the_group_s_value(group))
    );
}

#[test]
fn an_effect_of_a_group_that_is_not_a_shader_has_no_mask_button() {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let (mut window, group, _) = super::group_box_tests::window_with_a_tag_of_the_first_two_clips_selected();
    window.do_this(Command::AddGroupEffect {
        group,
        kind: EffectKind::HUE_SHIFT,
    });
    window.settle();

    assert!(is_shown(&window, "Turn"));
    assert!(!is_shown(&window, &mask_button_label("turn")));
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

    // A graph drawn in the window: a point added on the line and dragged up, and another added and dragged down; the thumbnail's bar follows the graph.
    window.do_this(Command::ShowTheMaskSettings(which(clip)));
    window.settle();
    let on_the_line = where_on_the_screen(&window, (0.3, 0.3));
    window.drag(
        on_the_line,
        where_on_the_screen(&window, (0.3, 0.8)),
        egui::PointerButton::Primary,
    );
    // (Where the line is now at the place 0.7, which is not where it was.)
    let line_height = the_map_now(&window, clip).graph_or_straight().height_at(0.7);
    let on_the_line = where_on_the_screen(&window, (0.7, line_height));
    window.drag(
        on_the_line,
        where_on_the_screen(&window, (0.7, 0.2)),
        egui::PointerButton::Primary,
    );
    window.move_pointer_to(egui::pos2(5.0, 5.0));
    save(&mut window, "mask_5_a_graph_drawn_in_the_window");
}
