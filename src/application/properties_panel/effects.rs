// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The controls of the values of the effects of a clip, of a track or of a group in the properties panel: the box of each effect (see
//! `chain`, which lays out the boxes with the badges among them and the plus button) has these in it.
//!
//! Every effect is a row of one table (see [`crate::project::EffectKind`]): its box is made from what the row says (its name,
//! its values), and the menu of the plus button lists the effects the thing can have (also those that it has: it may have several of a
//! kind, each with values of its own). What each value is
//! changed with is in [`control_for`], which has a control for each property that is a value of an effect. A change is a command
//! like any other, so it is an edit that is undone with one step, and a value that has keyframes is changed at the playhead. (The values of
//! the effect of a group have no keyframes: they are not animated.)

use egui::{ComboBox, DragValue, RichText, Slider, SliderClamping, TextEdit, Ui};

use super::PropertiesContent;
use super::animatable::{self, Animatable};
use super::controls::labelled_row;
use crate::application::commands::Command;
use crate::project::{
    AnimationTarget, Effect, EffectKind, FontSize, GroupId, Member, ParameterControl, Project, PropertyId,
    PropertyValue, Resource, ValueScale,
};

/// How many lines of words show without scrolling in a box of lines.
pub(super) const LINES_OF_WORDS_SHOWN: usize = 4;

/// The label of the font that is chosen when no font is: the editor's own choice.
pub(super) const DEFAULT_FONT_LABEL: &str = "Default font";

/// What the list of fonts says while the computer has not been asked for its fonts yet.
pub(super) const LOOKING_FOR_THE_FONTS_NOTE: &str = "Looking for the fonts...";

/// What the plus button that puts an effect on is called, which it says when the pointer rests on it, and to screen readers.
pub const ADD_EFFECT_LABEL: &str = "Add an effect";

/// What the button of the box of a plugin that has a window, and the item of its menu, say: open the window, and (while it is open) close it.
pub const OPEN_EDITOR_LABEL: &str = "Open editor";
pub const CLOSE_EDITOR_LABEL: &str = "Close editor";

/// What the box of a plugin says when its window could not be opened or ended: followed by why.
pub const EDITOR_PROBLEM_NOTE: &str = "The plugin's window:";

/// What the smaller menu of the menu of a box of a plugin effect that has presets is called, what it says under them, and what is put before the preset that the effect's values are
/// now (and after the label of one that has a state too, which is not used).
pub const PRESETS_LABEL: &str = "Presets";
pub const PRESETS_NOTE: &str = "A value that has keyframes is kept as it is.";
pub const PRESET_IS_SET_MARK: &str = "\u{2713} ";
pub const PRESET_HAS_A_STATE_NOTE: &str = " (values only)";

/// What the item of the menu of a box of an effect that deletes the effect is called.
pub const DELETE_EFFECT_LABEL: &str = "Delete effect";

/// What the item of the menu of a box of an effect that puts a copy of the effect after it is called.
pub const DUPLICATE_EFFECT_LABEL: &str = "Duplicate effect";

/// What the items of the menu of a box of an effect that switch it off (bypass it) and on say, and what the box says while it is off.
pub const SWITCH_EFFECT_OFF_LABEL: &str = "Switch off";
pub const SWITCH_EFFECT_ON_LABEL: &str = "Switch on";
/// What the menu of the box of the effect that a generator clip makes its picture with says instead of its items: it cannot be switched off, copied or taken off.
pub const WHAT_A_GENERATOR_MAKES_NOTE: &str =
    "What the generator makes: it cannot be switched off, copied or taken off.";
/// What the box of a shader that keeps something from one frame to the next says (EN-06): its picture is made from the frames before it.
pub const DEPENDS_ON_EARLIER_FRAMES_NOTE: &str = "Depends on earlier frames: it goes on from the frame before, so after a jump in the preview it starts again from nothing. The export is exact.";
/// What the box of a transition says (phase 6.6): what it reads, how it is used, and where it is not run.
pub const TRANSITION_NOTE: &str = "A transition: it reads what is behind the clip. Animate its progress from 0 to 1 to bring the clip in over what is behind it. It is not run on a clip in a group that has effects.";
pub const SWITCHED_OFF_NOTE: &str = "Switched off: it is not done, and its values and keyframes are kept.";

/// What the box of an effect that the program does not have says (a project file can bring one): that it does nothing, and that nothing of it is
/// lost.
pub const NOT_INSTALLED_NOTE: &str = "Not installed: it does nothing here, and its values are kept.";

/// What the placeholder of a sound plugin that is not run begins with, and ends with: between them is why.
pub const NOT_RUNNING_NOTE: &str = "Not running:";
pub const VALUES_ARE_KEPT_NOTE: &str = "It does nothing until then, and its values are kept.";

/// What the search field of the menu of the plus button says while it is empty, and what the item that adds an effect from a file is called.
pub const SEARCH_EFFECTS_HINT: &str = "Search effects";
pub const ADD_EFFECT_FROM_FILE_LABEL: &str = "Add effect from file...";
pub const NO_EFFECT_FOUND_NOTE: &str = "No effect matches";

/// What the items of the menu of a box of a look-up table effect that keep its table in the project file or link it say (RS-04).
pub const INCLUDE_TABLE_LABEL: &str = "Include table in project file";
pub const TAKE_TABLE_OUT_LABEL: &str = "Take table out of project file";

/// What the items that do the same for every table of the project say, when there are several.
pub fn include_all_tables_label(how_many: usize) -> String {
    format!("Include all {how_many} tables in project file")
}

pub fn take_all_tables_out_label(how_many: usize) -> String {
    format!("Take all {how_many} tables out of project file")
}

/// The items of the menu of the box of the look-up table effect `effect` that say where its table is kept, with the command each is: to include the table (if it is linked) or
/// to take it out (if it is included); and when the project has several tables that the same could be done to, to do it to all of them. A table that the project has no bytes
/// of (a linked file that is gone) has none: there is nothing to include or to put back.
pub(super) fn items_for_where_the_tables_are_kept(project: &Project, effect: &Effect) -> Vec<(String, Command)> {
    let mut items = Vec::new();
    let Some(slot) = effect
        .kind()
        .parameters()
        .iter()
        .position(|parameter| matches!(parameter.control, ParameterControl::Resource(_)))
    else {
        return items;
    };
    let Some(PropertyValue::Words(name)) = effect.values().get(slot) else {
        return items;
    };
    let can_be_moved = |name: &String| {
        project
            .resources()
            .get(name)
            .filter(|resource| !resource.is_missing())
            .map(Resource::is_included)
    };
    match can_be_moved(name) {
        Some(false) => items.push((
            INCLUDE_TABLE_LABEL.to_owned(),
            Command::IncludeTablesInTheProjectFile(vec![name.clone()]),
        )),
        Some(true) => items.push((
            TAKE_TABLE_OUT_LABEL.to_owned(),
            Command::TakeTablesOutOfTheProjectFile(vec![name.clone()]),
        )),
        None => {}
    }
    let in_use: Vec<String> = project.resources_in_use().into_iter().collect();
    let linked: Vec<String> = in_use
        .iter()
        .filter(|name| can_be_moved(name) == Some(false))
        .cloned()
        .collect();
    let included: Vec<String> = in_use
        .iter()
        .filter(|name| can_be_moved(name) == Some(true))
        .cloned()
        .collect();
    if linked.len() > 1 {
        items.push((
            include_all_tables_label(linked.len()),
            Command::IncludeTablesInTheProjectFile(linked),
        ));
    }
    if included.len() > 1 {
        items.push((
            take_all_tables_out_label(included.len()),
            Command::TakeTablesOutOfTheProjectFile(included),
        ));
    }
    items
}

/// The headings of the menu of effects, in the order they are listed in; a category that is not here (one that a project file brought) comes after them, in the order its
/// first effect is offered in.
const CATEGORIES_IN_ORDER: [&str; 5] = ["Colour", "Light", "Blur and sharpen", "Transform", "Looks"];

/// The effects among `kinds` that match what was typed in the search field of the menu (nothing typed matches all): those whose name, description or category has it in it,
/// whatever the case of the letters, in the order they are in.
pub(super) fn effects_that_match(kinds: &[EffectKind], typed: &str) -> Vec<EffectKind> {
    let needle = typed.trim().to_lowercase();
    kinds
        .iter()
        .copied()
        .filter(|kind| {
            needle.is_empty()
                || kind.name().to_lowercase().contains(&needle)
                || kind.description().to_lowercase().contains(&needle)
                || kind.descriptor().category.to_lowercase().contains(&needle)
        })
        .collect()
}

/// The sources that effects come from, in the order the menu of effects lists them in; a source that is not here comes after them, in the order its first effect is offered in.
const SOURCES_IN_ORDER: [&str; 5] = ["Built-in", "ISF", "LADSPA", "CLAP", "LV2"];

/// What the menu of effects calls the place the effect `kind` comes from: the part of its key before the colon (`builtin:hue_shift`, `ladspa:caps:Eq`), said as a person says it.
pub(super) fn source_of(kind: EffectKind) -> String {
    let namespace = kind.key().split(':').next().unwrap_or_default();
    match namespace {
        "builtin" => "Built-in".to_owned(),
        "isf" | "ladspa" | "clap" | "lv2" => namespace.to_uppercase(),
        other => other.to_uppercase(),
    }
}

/// The kinds of `kinds` grouped by the place they come from ([`source_of`]), the sources in [`SOURCES_IN_ORDER`] first, each with its effects in the order they are in.
pub(super) fn grouped_by_source(kinds: &[EffectKind]) -> Vec<(String, Vec<EffectKind>)> {
    let mut groups: Vec<(String, Vec<EffectKind>)> = Vec::new();
    for kind in kinds {
        let source = source_of(*kind);
        match groups.iter_mut().find(|(name, _)| *name == source) {
            Some((_, members)) => members.push(*kind),
            None => groups.push((source, vec![*kind])),
        }
    }
    groups.sort_by_key(|(name, _)| {
        SOURCES_IN_ORDER
            .iter()
            .position(|known| known == name)
            .unwrap_or(SOURCES_IN_ORDER.len())
    });
    groups
}

/// The kinds of `kinds` grouped by their categories: the headings in [`CATEGORIES_IN_ORDER`] first, then the others, each with its effects in the order they are in.
pub(super) fn grouped_by_category(kinds: &[EffectKind]) -> Vec<(String, Vec<EffectKind>)> {
    let mut groups: Vec<(String, Vec<EffectKind>)> = Vec::new();
    for kind in kinds {
        let category = kind.descriptor().category.to_string();
        match groups.iter_mut().find(|(heading, _)| *heading == category) {
            Some((_, members)) => members.push(*kind),
            None => groups.push((category, vec![*kind])),
        }
    }
    groups.sort_by_key(|(heading, _)| {
        CATEGORIES_IN_ORDER
            .iter()
            .position(|known| known == heading)
            .unwrap_or(CATEGORIES_IN_ORDER.len())
    });
    groups
}

/// The effects among `kinds` that the menu of the plus button offers: those that `can_have` says the thing can have, in the order they are
/// in `kinds`. (What it has already is offered again: it may have several effects of one kind.)
pub(super) fn effects_to_offer(
    kinds: impl IntoIterator<Item = EffectKind>,
    can_have: impl Fn(EffectKind) -> bool,
) -> Vec<EffectKind> {
    kinds.into_iter().filter(|kind| can_have(*kind)).collect()
}

/// What the button that chooses a file for the table of a look-up table effect says, and what the line that says no table is chosen says.
pub const CHOOSE_A_TABLE_LABEL: &str = "Choose a file...";
pub const NO_TABLE_CHOSEN_NOTE: &str = "No table chosen";
pub const TABLE_NOT_IN_THE_PROJECT_NOTE: &str = "The project does not have this table.";

/// What a control of the value of an effect asks for.
enum Change {
    /// The value is to be this.
    Value(PropertyValue),
    /// A file is to be chosen for the value (a table).
    ChooseAFile,
}

/// The member of the project that the clip or the track `target` is, for the commands that are about the effects of a thing of any kind.
fn member_of(target: AnimationTarget) -> Member {
    match target {
        AnimationTarget::Clip(clip) => Member::Clip(clip),
        AnimationTarget::Track(track) => Member::Track(track),
    }
}

/// The command that a change of the value `property` of an effect of `owner` is.
fn command_for(owner: Member, property: PropertyId, change: Change) -> Option<Command> {
    let PropertyId::EffectValue { instance, .. } = property else {
        return None;
    };
    Some(match (owner, change) {
        (Member::Clip(clip), Change::Value(value)) => Command::SetEffectValue {
            target: AnimationTarget::Clip(clip),
            property,
            value,
        },
        (Member::Track(track), Change::Value(value)) => Command::SetEffectValue {
            target: AnimationTarget::Track(track),
            property,
            value,
        },
        (Member::Group(group), Change::Value(value)) => Command::SetGroupEffectValue { group, property, value },
        (owner, Change::ChooseAFile) => Command::ChooseTableFile { owner, instance },
    })
}

/// Whether the row of the value `property` of an effect of the kind `kind`, which has `shown` for its value, has the stopwatch and the buttons of keyframes. What changes evenly between
/// two keyframes, a number, a pair and a colour, is animated by the stopwatch, and so are the words of a box of lines and a list of fonts (those of the text effect, which hold until the
/// next keyframe: a text can say another thing later). A switch and a choice are not: they have no buttons of keyframes, and hold until they are changed.
fn has_the_buttons_of_keyframes(kind: EffectKind, property: PropertyId, shown: &PropertyValue) -> bool {
    match shown {
        PropertyValue::Number(_) | PropertyValue::Pair(..) | PropertyValue::Colour(_) => true,
        PropertyValue::Words(_) => {
            let PropertyId::EffectValue { slot, .. } = property else {
                return false;
            };
            kind.parameters().get(usize::from(slot)).is_some_and(|parameter| {
                matches!(parameter.control, ParameterControl::Multiline | ParameterControl::Font)
            })
        }
        PropertyValue::Flag(_) => false,
    }
}

/// Shows the row of the control of the value of an effect of a clip or a track that `property` is, with the buttons of its keyframes, and
/// adds a command if it is changed.
pub(super) fn show_the_control_of(
    ui: &mut Ui,
    content: &PropertiesContent<'_>,
    target: AnimationTarget,
    kind: EffectKind,
    property: PropertyId,
    commands: &mut Vec<Command>,
) {
    let Some(shown) = content
        .project
        .property_value_at(target, property, content.current_frame)
    else {
        return;
    };
    if !has_the_buttons_of_keyframes(kind, property, &shown) {
        if let Some(change) = labelled_row(ui, |ui| {
            control_for(ui, content.project, content.font_families, kind, property, &shown)
        }) && let Some(command) = command_for(member_of(target), property, change)
        {
            commands.push(command);
        }
        return;
    }
    let changed = animatable::row(ui, content, Animatable { target, property }, commands, |ui, _| {
        labelled_row(ui, |ui| {
            control_for(ui, content.project, content.font_families, kind, property, &shown)
        })
    });
    if let Some(Change::Value(value)) = changed {
        commands.push(Command::SetEffectValue {
            target,
            property,
            value,
        });
    }
}

/// Shows the row of the control of the value of the effect `effect` of the group `group` that `property` is, which has no buttons of
/// keyframes (its values are not animated), and adds a command if it is changed.
pub(super) fn show_the_control_of_a_group_effect(
    ui: &mut Ui,
    project: &Project,
    group: GroupId,
    effect: &Effect,
    property: PropertyId,
    commands: &mut Vec<Command>,
) {
    let Some(shown) = effect.value_of(property) else {
        return;
    };
    if let Some(change) = labelled_row(ui, |ui| control_for(ui, project, None, effect.kind(), property, shown))
        && let Some(command) = command_for(Member::Group(group), property, change)
    {
        commands.push(command);
    }
}

/// The control that changes the value of an effect that `property` is, which shows `shown`: what it asks for if it was changed. A property that is not a value of an
/// effect has none. **What the control looks like is what the effect says of its value** (its label, its control, the range it shows, what it is in): a number has a slider,
/// or a box to drag in if it has no range; a choice is a list; a flag is a switch; a colour is a colour button; a pair is two boxes to drag in; a table is a button that chooses a file,
/// with the name of the table and what is wrong with it if anything is; **a box of lines is for words that may have more than one line, a list of fonts is for the name of a font (from `fonts`,
/// the fonts of the computer, if they are known yet), and a size in points is for a fraction of the picture's height that is shown as the points it would be on a picture 1080 tall** (the three
/// of the text effect, which a text clip is made of).
fn control_for(
    ui: &mut Ui,
    project: &Project,
    fonts: Option<&[String]>,
    kind: EffectKind,
    property: PropertyId,
    shown: &PropertyValue,
) -> Option<Change> {
    let PropertyId::EffectValue { slot, .. } = property else {
        return None;
    };
    let parameter = kind.parameters().get(usize::from(slot))?;
    match (&parameter.control, shown) {
        (ParameterControl::Choice(options), PropertyValue::Words(chosen)) => {
            let label = ui.label(&*parameter.label);
            let selected = options
                .iter()
                .find(|option| option.id == chosen.as_str())
                .map_or(chosen.as_str(), |option| &*option.label);
            let mut picked = None;
            ComboBox::from_id_salt(("the choice of", property))
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for option in options.iter() {
                        if ui
                            .selectable_label(option.id == chosen.as_str(), &*option.label)
                            .clicked()
                        {
                            picked = Some(option.id.to_string());
                        }
                    }
                })
                .response
                .labelled_by(label.id);
            picked
                .filter(|id| id != chosen)
                .map(|id| Change::Value(PropertyValue::Words(id)))
        }
        (ParameterControl::Multiline, PropertyValue::Words(words)) => {
            let label = ui.label(&*parameter.label);
            let mut typed = words.clone();
            let changed = ui
                .add(
                    TextEdit::multiline(&mut typed)
                        .desired_rows(LINES_OF_WORDS_SHOWN)
                        .desired_width(f32::INFINITY),
                )
                .labelled_by(label.id)
                .changed();
            changed.then_some(Change::Value(PropertyValue::Words(typed)))
        }
        (ParameterControl::Font, PropertyValue::Words(family)) => {
            let label = ui.label(&*parameter.label);
            let Some(fonts) = fonts else {
                ui.weak(LOOKING_FOR_THE_FONTS_NOTE);
                return None;
            };
            let shown_name = if family.is_empty() { DEFAULT_FONT_LABEL } else { family };
            let mut chosen = family.clone();
            ComboBox::from_id_salt(("the font of", property))
                .selected_text(shown_name)
                .width((ui.available_width() - 4.0).max(60.0))
                .height(320.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen, String::new(), DEFAULT_FONT_LABEL);
                    for name in fonts {
                        ui.selectable_value(&mut chosen, name.clone(), name);
                    }
                })
                .response
                .labelled_by(label.id);
            (chosen != *family).then_some(Change::Value(PropertyValue::Words(chosen)))
        }
        (ParameterControl::PointsOfThePictureHeight, PropertyValue::Number(fraction)) => {
            let label = ui.label(&*parameter.label);
            let mut points = FontSize::from_fraction_of_picture_height(*fraction).points();
            let smallest = FontSize::SMALLEST_FRACTION * FontSize::REFERENCE_PICTURE_HEIGHT;
            let largest = FontSize::LARGEST_FRACTION * FontSize::REFERENCE_PICTURE_HEIGHT;
            let changed = ui
                .add(
                    DragValue::new(&mut points)
                        .range(smallest..=largest)
                        .speed(0.5)
                        .max_decimals(1)
                        .suffix(" pt"),
                )
                .labelled_by(label.id)
                .changed();
            changed.then(|| {
                Change::Value(PropertyValue::Number(
                    FontSize::from_points(points).fraction_of_picture_height(),
                ))
            })
        }
        (ParameterControl::Resource(_), PropertyValue::Words(name)) => {
            ui.label(&*parameter.label);
            let named = project.resource_named_by(&parameter.control, shown);
            if name.is_empty() {
                ui.label(RichText::new(NO_TABLE_CHOSEN_NOTE).weak());
            } else {
                ui.label(name);
            }
            let clicked = ui.button(CHOOSE_A_TABLE_LABEL).clicked();
            if !name.is_empty() {
                // (What is wrong with the table, under the row it is in: it is not in the way of the row.)
                match named.map(Resource::cube_table) {
                    None => {
                        ui.label(RichText::new(TABLE_NOT_IN_THE_PROJECT_NOTE).weak());
                    }
                    Some(Err(why)) => {
                        ui.label(RichText::new(format!("{name}: {why}")).weak());
                    }
                    Some(Ok(_)) => {}
                }
            }
            clicked.then_some(Change::ChooseAFile)
        }
        (_, PropertyValue::Flag(on)) => {
            let mut on = *on;
            let changed = ui.checkbox(&mut on, &*parameter.label).changed();
            changed.then_some(Change::Value(PropertyValue::Flag(on)))
        }
        (_, PropertyValue::Number(number)) => {
            let mut number = *number;
            let label = ui.label(&*parameter.label);
            // (The range is what the slider shows: the value can be typed to go further, as a turn of the colours is typed to go round more than
            // once, which is how many times they go round between two keyframes.)
            let changed = match parameter.range {
                Some((least, most)) => {
                    let mut slider = Slider::new(&mut number, least..=most)
                        .clamping(SliderClamping::Never)
                        .suffix(&*parameter.unit);
                    if let Some(decimals) = parameter.decimals {
                        slider = slider.max_decimals(usize::from(decimals));
                    }
                    // (Equal steps are equal ratios, for what spans orders of magnitude, such as a frequency: the descriptor only says so for a range that is above zero.)
                    if parameter.scale == ValueScale::Logarithmic {
                        slider = slider.logarithmic(true);
                    }
                    if parameter.control == ParameterControl::Integer {
                        slider = slider.integer();
                    }
                    ui.add(slider).labelled_by(label.id).changed()
                }
                None => {
                    let mut drag = DragValue::new(&mut number).suffix(&*parameter.unit);
                    if let Some(decimals) = parameter.decimals {
                        drag = drag.max_decimals(usize::from(decimals));
                    }
                    if parameter.control == ParameterControl::Integer {
                        drag = drag.fixed_decimals(0).speed(1.0);
                    }
                    ui.add(drag).labelled_by(label.id).changed()
                }
            };
            changed.then_some(Change::Value(PropertyValue::Number(number)))
        }
        (_, PropertyValue::Colour(colour)) => {
            let label = ui.label(&*parameter.label);
            let mut rgba = [colour.red, colour.green, colour.blue, colour.alpha];
            let changed = ui
                .color_edit_button_srgba_unmultiplied(&mut rgba)
                .labelled_by(label.id)
                .changed();
            changed.then(|| {
                Change::Value(PropertyValue::Colour(crate::project::TextColour::new(
                    rgba[0], rgba[1], rgba[2], rgba[3],
                )))
            })
        }
        (_, PropertyValue::Pair(across, down)) => {
            let label = ui.label(&*parameter.label);
            let (mut across, mut down) = (*across, *down);
            let first = ui
                .add(DragValue::new(&mut across).speed(1.0).suffix(&*parameter.unit))
                .labelled_by(label.id)
                .changed();
            let second = ui
                .add(DragValue::new(&mut down).speed(1.0).suffix(&*parameter.unit))
                .labelled_by(label.id)
                .changed();
            (first || second).then_some(Change::Value(PropertyValue::Pair(across, down)))
        }
        _ => {
            ui.label(format!("{}: {}", parameter.label, what_a_value_says(shown)));
            None
        }
    }
}

/// What a value is, in words, for a value that has no control.
fn what_a_value_says(value: &PropertyValue) -> String {
    match value {
        PropertyValue::Number(number) => number.to_string(),
        PropertyValue::Pair(first, second) => format!("{first}, {second}"),
        PropertyValue::Colour(colour) => format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            colour.red, colour.green, colour.blue, colour.alpha
        ),
        PropertyValue::Flag(true) => "yes".to_owned(),
        PropertyValue::Flag(false) => "no".to_owned(),
        PropertyValue::Words(words) => words.clone(),
    }
}

#[cfg(test)]
mod controls_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_are_grouped_by_the_place_they_come_from_with_the_program_first_and_an_unknown_place_last() {
        let _alone = crate::test_support::the_installed_plugins_to_myself();
        let plugin = crate::test_support::an_installed_sound_plugin("ladspa:caps:eq", "C* Audio Plugin Suite", &[]);
        crate::project::set_installed_plugins(vec![plugin]);
        let plugin_kind = EffectKind::installed_plugin_kinds()[0];
        assert_eq!(source_of(EffectKind::HUE_SHIFT), "Built-in");
        assert_eq!(source_of(plugin_kind), "LADSPA");
        // (A plugin comes before the program in the list given, and the groups still begin with the program.)
        let groups = grouped_by_source(&[plugin_kind, EffectKind::HUE_SHIFT, EffectKind::BLUR]);
        assert_eq!(
            groups,
            [
                ("Built-in".to_owned(), vec![EffectKind::HUE_SHIFT, EffectKind::BLUR]),
                ("LADSPA".to_owned(), vec![plugin_kind]),
            ]
        );
        assert!(grouped_by_source(&[]).is_empty());
    }

    #[test]
    fn the_menu_offers_the_effects_the_thing_can_have_in_the_order_they_are_in_and_what_it_has_already_too() {
        // (A thing may have several effects of one kind, so what it has is offered as much as what it has not: the menu is not told what it has.)
        let kinds = EffectKind::BUILT_IN;

        assert_eq!(effects_to_offer(kinds, |_| true), kinds.to_vec(), "all of them");
        assert_eq!(
            effects_to_offer(kinds, |_| false),
            Vec::<EffectKind>::new(),
            "what it cannot have is not offered"
        );
        // (Rewritten on purpose in phase 4: there are five effects now, not one, so what a thing cannot have is left out of the five and the others are still offered.)
        assert_eq!(
            effects_to_offer(kinds, |kind| kind != EffectKind::HUE_SHIFT),
            kinds[1..].to_vec(),
            "whichever it is that it cannot have is left out, and the others are offered in their order"
        );
        assert_eq!(
            effects_to_offer(kinds, |kind| kind == EffectKind::OPACITY),
            vec![EffectKind::OPACITY]
        );
        assert_eq!(
            effects_to_offer([EffectKind::HUE_SHIFT, EffectKind::HUE_SHIFT], |_| true).len(),
            2,
            "(it offers what it is given: the table has each kind once)"
        );
    }
}
