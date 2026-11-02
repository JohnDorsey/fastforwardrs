// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The command that reads a number of a shader from a mask (`docs/shader-masks-design.md`): it is one step of the history, it keeps the slider as it was, and when the project refuses
//! (a track that is made from the picture the effect is done to, a number that is not one a mask can give) the person is told why and nothing is changed.

use super::test_window::{Window, project_with_a_video_and_a_sound};
use crate::application::commands::Command;
use crate::application::dialogs::Dialog;
use crate::project::{
    AnimationTarget, ClipId, DrivenValue, EffectInstanceId, EffectKind, MaskChannel, MaskSource, Member, TrackId,
};

/// A shader with a number that has a range, `amount`, and a colour, `tint`.
const TINT: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"amount\", \"TYPE\": \"float\", \"DEFAULT\": 0.25 }, { \"NAME\": \"tint\", \"TYPE\": \"color\", \"DEFAULT\": [1.0, 0.0, 0.0, 1.0] } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage); }\n";

fn tint() -> EffectKind {
    let file = crate::isf::read(TINT).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "tint.fs", TINT)).unwrap()
}

/// A window on a project of two tracks (a sound in front, and the video behind it), with the shader on the video's clip: the window, the clip, and the id of the other track (the sound's).
fn window_with_a_shader_on_the_video() -> (Window, ClipId, TrackId) {
    let _no_plugin_is_being_installed = crate::test_support::the_installed_plugins_in_peace();
    let mut window = Window::showing(project_with_a_video_and_a_sound(300, 5));
    let project = window.application().project();
    let clip = project.tracks()[1].placement_at_index(0).unwrap().id;
    let the_other_track = project.tracks()[0].id();
    window.do_this(Command::AddEffect {
        target: AnimationTarget::Clip(clip),
        kind: tint(),
    });
    (window, clip, the_other_track)
}

fn the_first_effect_of(window: &Window, clip: ClipId) -> crate::project::Effect {
    window
        .application()
        .project()
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .clone()
}

#[test]
fn reading_a_number_from_a_track_is_one_step_of_the_history_and_undoing_it_reads_the_slider_again() {
    let (mut window, clip, the_other_track) = window_with_a_shader_on_the_video();
    let before = window.application().project().clone();

    window.do_this(Command::SetDrivenValue {
        owner: Member::Clip(clip),
        instance: EffectInstanceId::FIRST,
        slot: 0,
        driven: Some(DrivenValue::from_a_track(the_other_track, MaskChannel::Green, 0.5)),
    });

    let effect = the_first_effect_of(&window, clip);
    assert_eq!(
        effect
            .driven_values()
            .of(0)
            .map(|driven| (driven.source, driven.channel, driven.value_where_nothing)),
        Some((MaskSource::Track(the_other_track), MaskChannel::Green, 0.5))
    );
    assert_eq!(
        effect.values(),
        the_first_effect_of_the(&before, clip).values(),
        "the slider is as it was"
    );

    window.do_this(Command::Undo);

    assert_eq!(window.application().project(), &before);
}

fn the_first_effect_of_the(project: &crate::project::Project, clip: ClipId) -> crate::project::Effect {
    project
        .effects_of(AnimationTarget::Clip(clip))
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .clone()
}

#[test]
fn a_number_a_mask_cannot_give_is_refused_with_the_reason_and_nothing_is_changed() {
    let (mut window, clip, the_other_track) = window_with_a_shader_on_the_video();
    window.application_mut().note_everything_is_saved();
    let before = window.application().project().clone();

    // The colour is place 1.
    window.do_this(Command::SetDrivenValue {
        owner: Member::Clip(clip),
        instance: EffectInstanceId::FIRST,
        slot: 1,
        driven: Some(DrivenValue::from_a_track(the_other_track, MaskChannel::Red, 0.0)),
    });

    match &window.application().interface.dialog {
        Some(Dialog::Message { title, text }) => {
            assert_eq!(title, "The value cannot be read from a mask");
            assert!(text.contains("only a number with a range"), "{text}");
        }
        other => panic!("the person is told: {other:?}"),
    }
    assert_eq!(window.application().project(), &before);
    assert!(!window.application().has_unsaved_changes());
}

#[test]
fn a_track_that_is_made_from_the_picture_of_the_effect_is_refused_with_the_reason() {
    let (mut window, clip, _) = window_with_a_shader_on_the_video();
    window.application_mut().note_everything_is_saved();
    let before = window.application().project().clone();
    let its_own_track = before.tracks()[1].id();

    window.do_this(Command::SetDrivenValue {
        owner: Member::Clip(clip),
        instance: EffectInstanceId::FIRST,
        slot: 0,
        driven: Some(DrivenValue::from_a_track(its_own_track, MaskChannel::Red, 0.0)),
    });

    match &window.application().interface.dialog {
        Some(Dialog::Message { title, text }) => {
            assert_eq!(title, "The value cannot be read from a mask");
            assert!(text.contains("made from the picture"), "{text}");
        }
        other => panic!("the person is told: {other:?}"),
    }
    assert_eq!(window.application().project(), &before);
}
