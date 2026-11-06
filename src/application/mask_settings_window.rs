// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window that says what a number of a shader is read from (`docs/shader-masks-design.md`): **the track whose picture is the mask**, which part of the colour of that picture is the
//! number, and what the number is where the track has nothing. It is opened by the mask button next to the keyframe buttons of the number, or by the thumbnail that is in the place of
//! its slider while it is read from a mask; nothing about it is on the screen otherwise. (The graph of line segments that maps the mask's values to the number's goes here later.)
//!
//! The window changes nothing itself: a choice becomes a command, a step of the history that the choices about the same number are joined to.

use egui::{Align2, ComboBox, Context, DragValue, Grid, Id, Slider, SliderClamping, Ui, Window, vec2};

use super::commands::Command;
use super::mask_graph;
use crate::project::{
    DrivenValue, EffectInstanceId, Graph, GroupKind, LARGEST_DISTANCE_OF_A_SHAPE, MaskChannel, MaskMap, MaskSource,
    MaskSpace, Member, Project, PropertyValue, SMALLEST_SIZE_OF_A_SHAPE, WorldPlacement, WorldShape,
};

/// What the lines of the window are called.
pub const TRACK_LABEL: &str = "Source";
pub const CHANNEL_LABEL: &str = "Channel";
pub const VALUE_WHERE_NOTHING_LABEL: &str = "Where the track has nothing";
pub const USE_THE_SLIDER_AGAIN_LABEL: &str = "Use the slider again";
pub const GRAPH_LABEL: &str = "Graph";
pub const STRAIGHT_LINE_AGAIN_LABEL: &str = "Straight line again";
pub const REAL_BACKDROP_LABEL: &str = "Transitions see the real backdrop";

/// What the lines of the window that are only there for a number that can be read from a mask laid in space are called.
pub const SPACE_LABEL: &str = "Space";
pub const OVER_THE_PICTURE_LABEL: &str = "Over the picture";
pub const IN_SPACE_LABEL: &str = "In space";
pub const SHAPE_LABEL: &str = "Shape";
pub const POSITION_LABEL: &str = "Position";
pub const TURN_LABEL: &str = "Turn";
pub const WIDTH_LABEL: &str = "Width";
pub const HEIGHT_LABEL: &str = "Height";
/// What the boxes of the width and the height of the shape are called to a screen reader and the tests (the rows are called *Width* and *Height*, which are also the names of other things).
pub const WIDTH_NAME: &str = "Shape width";
pub const HEIGHT_NAME: &str = "Shape height";

/// What the box of the track says while no track is chosen.
pub const NO_TRACK_CHOSEN_LABEL: &str = "Choose a track or a group";

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

/// What the shapes are called.
pub fn name_of_a_shape(shape: WorldShape) -> &'static str {
    match shape {
        WorldShape::Plane => "Plane",
        WorldShape::Sphere => "Sphere",
        WorldShape::Cylinder => "Cylinder",
    }
}

/// What a shape does with the picture, in a few words, for under the lines of the window.
pub fn what_a_shape_does(shape: WorldShape) -> &'static str {
    match shape {
        WorldShape::Plane => {
            "The picture is laid flat, facing along z. A point of space reads the picture where it is dropped straight onto the plane; beyond the edges the picture has nothing."
        }
        WorldShape::Sphere => {
            "The picture is wrapped round a ball like a map of the world: its middle faces along z and its top is the pole at +y. A point of space reads the picture in the direction it is from the middle, so the size of the ball does not matter."
        }
        WorldShape::Cylinder => {
            "The picture is wrapped once round a tube round the y axis: its middle faces along z. A point of space reads the picture at its direction round, and at its height up the tube; above and below the tube the picture has nothing."
        }
    }
}

/// How wide the note about what a shape does is, in points: it is wrapped to this.
const WIDTH_OF_THE_NOTE_ABOUT_A_SHAPE: f32 = 340.0;

/// The names of the three numbers of a position, and of a turn, as a screen reader and the tests know them.
pub const POSITION_NAMES: [&str; 3] = ["Position x", "Position y", "Position z"];
pub const TURN_NAMES: [&str; 3] = ["Turn yaw", "Turn pitch", "Turn roll"];

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

/// The sources that a mask can be made of, with what each is called in the list: the tracks, then the clip groups and the track groups.
fn sources_to_choose_from(project: &Project) -> Vec<(MaskSource, String)> {
    let mut sources: Vec<(MaskSource, String)> = project
        .tracks()
        .iter()
        .map(|track| (MaskSource::Track(track.id()), track.title().to_owned()))
        .collect();
    for group in project.groups() {
        match group.kind() {
            GroupKind::ClipGroup => sources.push((
                MaskSource::ClipGroup(group.id()),
                format!("{} (clip group)", group.name()),
            )),
            GroupKind::TrackGroup => {
                sources.push((
                    MaskSource::TrackGroup(group.id()),
                    format!("{} (track group)", group.name()),
                ));
            }
            GroupKind::Tag => {}
        }
    }
    sources
}

/// A box that a number is dragged or typed in, known to screen readers and the tests as `name` (the text next to it only says which of three it is).
fn number_box(ui: &mut Ui, value: &mut f32, name: &str, speed: f64, range: std::ops::RangeInclusive<f32>) -> bool {
    let response = ui.add(DragValue::new(value).speed(speed).range(range));
    let (enabled, shown) = (ui.is_enabled(), *value);
    response.widget_info(|| egui::WidgetInfo {
        label: Some(name.to_owned()),
        value: Some(f64::from(shown)),
        enabled,
        ..egui::WidgetInfo::new(egui::WidgetType::DragValue)
    });
    response.changed()
}

/// The rows of the window about where the mask is laid: over the picture or in space, and (in space) the shape and how it is put. A choice becomes `set(...)`, a command.
fn show_the_space(
    ui: &mut Ui,
    effect: &crate::project::Effect,
    slot: usize,
    driven: Option<DrivenValue>,
    set: &dyn Fn(DrivenValue) -> Command,
    commands: &mut Vec<Command>,
) {
    let no_room = effect.why_the_value_cannot_be_in_space(slot);
    ui.label(SPACE_LABEL);
    ui.horizontal(|ui| {
        for (label, in_space) in [(OVER_THE_PICTURE_LABEL, false), (IN_SPACE_LABEL, true)] {
            let selected = driven.is_some_and(|driven| driven.space.is_in_space() == in_space);
            let can_be_chosen = driven.is_some() && (!in_space || no_room.is_none());
            let mut response = ui.add_enabled(can_be_chosen, egui::Button::selectable(selected, label));
            if in_space && let (Some(reason), true) = (no_room, driven.is_some()) {
                response = response.on_disabled_hover_text(reason);
            }
            if response.clicked()
                && !selected
                && let Some(driven) = driven
            {
                let space = if in_space {
                    MaskSpace::World(driven.space.placement_or(effect.placement_in_space_to_begin_with()))
                } else {
                    MaskSpace::Screen
                };
                commands.push(set(driven.with_the_space(space)));
            }
        }
    });
    ui.end_row();

    let Some(driven) = driven else {
        return;
    };
    let MaskSpace::World(placement) = driven.space else {
        return;
    };
    let put = |placement: WorldPlacement| set(driven.with_the_space(MaskSpace::World(placement)));

    ui.label(SHAPE_LABEL);
    ui.horizontal(|ui| {
        for shape in WorldShape::ALL {
            let selected = placement.shape == shape;
            if ui
                .add(egui::Button::selectable(selected, name_of_a_shape(shape)))
                .clicked()
                && !selected
            {
                commands.push(put(placement.with_the_shape(shape)));
            }
        }
    });
    ui.end_row();

    ui.label("");
    // (Wrapped in a column of its own: a line of this length would make the whole window as wide as the line.)
    ui.vertical(|ui| {
        ui.set_max_width(WIDTH_OF_THE_NOTE_ABOUT_A_SHAPE);
        ui.add(egui::Label::new(egui::RichText::new(what_a_shape_does(placement.shape)).small()).wrap());
    });
    ui.end_row();

    let far = LARGEST_DISTANCE_OF_A_SHAPE;
    ui.label(POSITION_LABEL);
    ui.horizontal(|ui| {
        let mut position = placement.position;
        let mut changed = false;
        for (number, (axis, name)) in position.iter_mut().zip(["x", "y", "z"].into_iter().zip(POSITION_NAMES)) {
            ui.label(axis);
            changed |= number_box(ui, number, name, 0.02, -far..=far);
        }
        if changed {
            commands.push(put(WorldPlacement { position, ..placement }));
        }
    });
    ui.end_row();

    ui.label(TURN_LABEL);
    ui.horizontal(|ui| {
        let mut turn = placement.yaw_pitch_roll_degrees;
        let mut changed = false;
        for (number, (word, name)) in turn
            .iter_mut()
            .zip(["yaw", "pitch", "roll"].into_iter().zip(TURN_NAMES))
        {
            ui.label(word);
            changed |= number_box(ui, number, name, 0.5, -360.0..=360.0);
        }
        if changed {
            commands.push(put(WorldPlacement {
                yaw_pitch_roll_degrees: turn,
                ..placement
            }));
        }
    });
    ui.end_row();

    // (A sphere has no use for a size, and a cylinder only for its height.)
    if placement.shape == WorldShape::Plane {
        ui.label(WIDTH_LABEL);
        let mut width = placement.width;
        if number_box(ui, &mut width, WIDTH_NAME, 0.02, SMALLEST_SIZE_OF_A_SHAPE..=far) {
            commands.push(put(WorldPlacement { width, ..placement }));
        }
        ui.end_row();
    }
    if placement.shape != WorldShape::Sphere {
        ui.label(HEIGHT_LABEL);
        let mut height = placement.height;
        if number_box(ui, &mut height, HEIGHT_NAME, 0.02, SMALLEST_SIZE_OF_A_SHAPE..=far) {
            commands.push(put(WorldPlacement { height, ..placement }));
        }
        ui.end_row();
    }
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
                    let chosen = driven.map(|driven| driven.source);
                    ComboBox::from_id_salt(("the source of a mask", which.instance, which.slot))
                        .selected_text(chosen.map_or_else(
                            || NO_TRACK_CHOSEN_LABEL.to_owned(),
                            |source| project.name_of_the_source(source),
                        ))
                        .show_ui(ui, |ui| {
                            for (id, name) in sources_to_choose_from(project) {
                                let loops = project
                                    .tracks_of_the_source(id)
                                    .into_iter()
                                    .any(|track| project.a_mask_made_of_this_track_would_loop(which.owner, track));
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
                                        source: id,
                                        channel: driven.map_or(MaskChannel::Brightness, |driven| driven.channel),
                                        value_where_nothing: driven
                                            .map_or(slider_now, |driven| driven.value_where_nothing),
                                        map: driven.map_or(MaskMap::Straight, |driven| driven.map),
                                        space: driven.map_or(MaskSpace::Screen, |driven| driven.space),
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

                    // (Only for a number that its shader says can be read in space: for every other number the window is as it was.)
                    if effect.can_the_value_be_laid_in_space(which.slot) {
                        show_the_space(ui, effect, which.slot, driven, &set, commands);
                    }

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
