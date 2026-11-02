// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window that says what a number of a shader is read from (`docs/shader-masks-design.md`): **the track whose picture is the mask**, which part of the colour of that picture is the
//! number, and what the number is where the track has nothing. It is opened by the mask button next to the keyframe buttons of the number, or by the thumbnail that is in the place of
//! its slider while it is read from a mask; nothing about it is on the screen otherwise. (The graph of line segments that maps the mask's values to the number's goes here later.)
//!
//! The window changes nothing itself: a choice becomes a command, a step of the history that the choices about the same number are joined to.

use egui::{Align2, ComboBox, Context, Grid, Id, Slider, SliderClamping, Window, vec2};

use super::commands::Command;
use super::mask_graph;
use crate::project::{
    DrivenValue, EffectInstanceId, Graph, MaskChannel, MaskMap, MaskSource, Member, Project, PropertyValue, TrackId,
};

/// What the lines of the window are called.
pub const TRACK_LABEL: &str = "Track";
pub const CHANNEL_LABEL: &str = "Channel";
pub const VALUE_WHERE_NOTHING_LABEL: &str = "Where the track has nothing";
pub const USE_THE_SLIDER_AGAIN_LABEL: &str = "Use the slider again";
pub const GRAPH_LABEL: &str = "Graph";
pub const STRAIGHT_LINE_AGAIN_LABEL: &str = "Straight line again";
pub const REAL_BACKDROP_LABEL: &str = "Transitions see the real backdrop";

/// What the box of the track says while no track is chosen.
pub const NO_TRACK_CHOSEN_LABEL: &str = "Choose a track";

/// What is said under the lines while the number is not read from a mask.
pub const NOT_READ_FROM_A_MASK_NOTE: &str =
    "The number is on its slider. Choose a track to read it from the picture of the track instead, pixel by pixel.";

/// What is said under the lines while it is.
pub const READ_FROM_A_MASK_NOTE: &str =
    "The slider and its keyframes are kept, and are used again when you use the slider again.";

/// What a track that cannot be chosen says after its name.
pub const WOULD_BE_MADE_FROM_ITSELF_NOTE: &str = "(would be made from itself)";

/// What the channels are called.
pub fn name_of_a_channel(channel: MaskChannel) -> &'static str {
    match channel {
        MaskChannel::Red => "Red",
        MaskChannel::Green => "Green",
        MaskChannel::Blue => "Blue",
        MaskChannel::Brightness => "Brightness",
    }
}

/// Which number of which effect the window is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WhichValue {
    pub owner: Member,
    pub instance: EffectInstanceId,
    /// The place of the number among the values of the effect.
    pub slot: usize,
}

/// The title of the window about the number called `label`.
pub fn title_for(label: &str) -> String {
    format!("Mask for {label}")
}

/// What a track is called in the window.
fn name_of_a_track(project: &Project, track: TrackId) -> String {
    project
        .track(track)
        .map_or_else(|| format!("{track}"), |found| found.title().to_owned())
}

/// Shows the window of `open_for`, if there is one and the effect is still in the project, and adds a command for a change made in it. The window closes (`open_for` becomes `None`) when
/// it is closed with its cross, or when the effect or its number is not there any more.
pub fn show(context: &Context, open_for: &mut Option<WhichValue>, project: &Project, commands: &mut Vec<Command>) {
    let Some(which) = *open_for else {
        return;
    };
    let found = project.chain_of_the_member(which.owner).and_then(|chain| {
        let effect = chain.iter().find(|effect| effect.instance() == which.instance)?;
        let parameter = effect.kind().parameters().get(which.slot)?;
        Some((effect, parameter))
    });
    let Some((effect, parameter)) = found else {
        *open_for = None;
        return;
    };
    let driven = effect.driven_values().of(which.slot).copied();
    let slider_now = match effect.values().get(which.slot) {
        Some(PropertyValue::Number(number)) => *number,
        _ => 0.0,
    };
    let (least, most) = parameter.range.unwrap_or((0.0, 1.0));
    let mut open = true;
    Window::new(title_for(&parameter.label))
        .id(Id::new(("mask of a value", which.owner, which.instance, which.slot)))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .pivot(Align2::CENTER_CENTER)
        .default_pos(context.content_rect().center() + vec2(0.0, 40.0))
        .show(context, |ui| {
            let set = |driven: DrivenValue| Command::SetDrivenValue {
                owner: which.owner,
                instance: which.instance,
                slot: which.slot,
                driven: Some(driven),
            };
            Grid::new(("mask settings", which.owner, which.instance, which.slot))
                .num_columns(2)
                .spacing([24.0, 8.0])
                .show(ui, |ui| {
                    ui.label(TRACK_LABEL);
                    let chosen = driven.map(|driven| match driven.source {
                        MaskSource::Track(track) => track,
                    });
                    ComboBox::from_id_salt(("the track of a mask", which.instance, which.slot))
                        .selected_text(chosen.map_or_else(
                            || NO_TRACK_CHOSEN_LABEL.to_owned(),
                            |track| name_of_a_track(project, track),
                        ))
                        .show_ui(ui, |ui| {
                            for track in project.tracks() {
                                let id = track.id();
                                let name = track.title().to_owned();
                                let loops = project.a_mask_made_of_this_track_would_loop(which.owner, id);
                                let text = if loops {
                                    format!("{name} {WOULD_BE_MADE_FROM_ITSELF_NOTE}")
                                } else {
                                    name
                                };
                                let selected = chosen == Some(id);
                                if ui
                                    .add_enabled(!loops, egui::Button::selectable(selected, text))
                                    .clicked()
                                    && !selected
                                {
                                    // (The first time a track is chosen the number is read as brightness, and where the track has nothing it is what the slider says now.)
                                    commands.push(set(DrivenValue {
                                        source: MaskSource::Track(id),
                                        channel: driven.map_or(MaskChannel::Brightness, |driven| driven.channel),
                                        value_where_nothing: driven
                                            .map_or(slider_now, |driven| driven.value_where_nothing),
                                        map: driven.map_or(MaskMap::Straight, |driven| driven.map),
                                        real_backdrop: driven.is_none_or(|driven| driven.real_backdrop),
                                    }));
                                }
                            }
                        });
                    ui.end_row();

                    ui.label(CHANNEL_LABEL);
                    ui.horizontal(|ui| {
                        for channel in MaskChannel::ALL {
                            let selected = driven.is_some_and(|driven| driven.channel == channel);
                            let response = ui.add_enabled(
                                driven.is_some(),
                                egui::Button::selectable(selected, name_of_a_channel(channel)),
                            );
                            if response.clicked()
                                && !selected
                                && let Some(driven) = driven
                            {
                                commands.push(set(DrivenValue { channel, ..driven }));
                            }
                        }
                    });
                    ui.end_row();

                    ui.label(GRAPH_LABEL);
                    ui.vertical(|ui| {
                        let no_room = effect.why_the_value_cannot_have_a_graph(which.slot);
                        let graph = driven.map_or_else(Graph::straight, |driven| driven.map.graph_or_straight());
                        if let Some(edited) = mask_graph::show(ui, graph, driven.is_some() && no_room.is_none())
                            && let Some(driven) = driven
                        {
                            commands.push(set(DrivenValue {
                                map: mask_graph::map_of(edited),
                                ..driven
                            }));
                        }
                        if let Some(reason) = no_room
                            && driven.is_some()
                        {
                            ui.label(egui::RichText::new(reason).small());
                        }
                        let has_a_graph = driven.is_some_and(|driven| driven.map != MaskMap::Straight);
                        if ui
                            .add_enabled(has_a_graph, egui::Button::new(STRAIGHT_LINE_AGAIN_LABEL))
                            .clicked()
                            && let Some(driven) = driven
                        {
                            commands.push(set(DrivenValue {
                                map: MaskMap::Straight,
                                ..driven
                            }));
                        }
                    });
                    ui.end_row();

                    ui.label("");
                    let mut sees = driven.is_none_or(|driven| driven.real_backdrop);
                    let changed = ui
                        .add_enabled(driven.is_some(), egui::Checkbox::new(&mut sees, REAL_BACKDROP_LABEL))
                        .on_hover_text(
                            "While a clip of the track has a transition, the mask is the track over the tracks behind it, as the picture shows it. Off: the transition is done over nothing.",
                        )
                        .changed();
                    if changed && let Some(driven) = driven {
                        commands.push(set(driven.with_the_real_backdrop(sees)));
                    }
                    ui.end_row();

                    ui.label(VALUE_WHERE_NOTHING_LABEL);
                    let mut nothing = driven.map_or(slider_now, |driven| driven.value_where_nothing);
                    let changed = ui
                        .add_enabled(
                            driven.is_some(),
                            Slider::new(&mut nothing, least..=most)
                                .clamping(SliderClamping::Never)
                                .suffix(&*parameter.unit),
                        )
                        .changed();
                    if changed && let Some(driven) = driven {
                        commands.push(set(DrivenValue {
                            value_where_nothing: nothing,
                            ..driven
                        }));
                    }
                    ui.end_row();
                });
            ui.add_space(8.0);
            if ui
                .add_enabled(driven.is_some(), egui::Button::new(USE_THE_SLIDER_AGAIN_LABEL))
                .clicked()
            {
                commands.push(Command::SetDrivenValue {
                    owner: which.owner,
                    instance: which.instance,
                    slot: which.slot,
                    driven: None,
                });
            }
            ui.separator();
            ui.label(
                egui::RichText::new(if driven.is_some() {
                    READ_FROM_A_MASK_NOTE
                } else {
                    NOT_READ_FROM_A_MASK_NOTE
                })
                .small(),
            );
        });
    if !open {
        *open_for = None;
    }
}
