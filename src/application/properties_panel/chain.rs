// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The chain of effects of a clip, of a track or of a group in the properties panel, under the box of the thing itself: a box for each effect,
//! and among them the badges (see [`crate::project::Badge`]) that say where the effects of something else are applied: of a tag that the
//! thing is in, and for a clip, of the track it is on. Under them is the plus button that puts another effect on, and last, apart, the
//! clip group or track group that the thing is directly in, whose effects are applied after all of these and so cannot be moved.
//!
//! **Everything in the chain except that is moved by dragging it**: the box of an effect by its heading (which has grip dots over it), a badge
//! anywhere on it. A line shows where it will go, and it goes there when it is let go of. The effects are applied in the order they are in, from the
//! top to the bottom.
//!
//! What a badge stands for is selected with a **double click**, so that taking hold of a badge to move it does not select anything.

use egui::{CornerRadius, Key, Popup, Rect, Response, RichText, ScrollArea, Stroke, TextEdit, Ui, pos2, vec2};

use super::PropertiesContent;
use super::badge::{self, BadgeLook};
use super::effects::{
    ADD_EFFECT_FROM_FILE_LABEL, ADD_EFFECT_LABEL, CLOSE_EDITOR_LABEL, DELETE_EFFECT_LABEL,
    DEPENDS_ON_EARLIER_FRAMES_NOTE, DUPLICATE_EFFECT_LABEL, EDITOR_PROBLEM_NOTE, NO_EFFECT_FOUND_NOTE,
    NOT_INSTALLED_NOTE, NOT_RUNNING_NOTE, OPEN_EDITOR_LABEL, PRESET_HAS_A_STATE_NOTE, PRESET_IS_SET_MARK,
    PRESETS_LABEL, PRESETS_NOTE, SEARCH_EFFECTS_HINT, SWITCH_EFFECT_OFF_LABEL, SWITCH_EFFECT_ON_LABEL,
    SWITCHED_OFF_NOTE, TRANSITION_NOTE, VALUES_ARE_KEPT_NOTE, WHAT_A_GENERATOR_MAKES_NOTE, effects_that_match,
    effects_to_offer, grouped_by_category, grouped_by_source, items_for_where_the_tables_are_kept, show_the_control_of,
    show_the_control_of_a_group_effect,
};
use super::kind_box;
use super::selection_list::select_when_double_clicked;
use crate::application::commands::Command;
use crate::application::naming::{describe_a_group, what_an_effect_in_a_chain_is_called, what_an_effect_is_called};
use crate::application::plus_button;
use crate::application::selection::SelectionItem;
use crate::application::theme;
use crate::application::timeline_panel::{GripDotArrangement, draw_grip_dots};
use crate::project::{
    AnimationTarget, Badge, ChainItemKey, ChainRow, Effect, EffectInstanceId, EffectKind, Effects, Group, GroupKind,
    Member, Track,
};

/// How much room there is between the rows of the chain, in points.
const SPACE_BETWEEN_ROWS: f32 = 8.0;

/// How much room there is round the plus button, above and below it, in points.
const SPACE_ROUND_THE_PLUS_BUTTON: f32 = 12.0;

/// How far under the top of the box of an effect the middle of its grip dots is, in points.
const DOTS_UNDER_THE_TOP_OF_A_BOX: f32 = 9.0;

/// How thick the line is that shows where what is dragged will go, in points.
const THICKNESS_OF_THE_LINE: f32 = 3.0;

/// What hovering over the badge of a tag, and of a clip group or a track group, and of a track says that a double click does.
const SELECTS_THE_TAG: &str = "Double-click to select the tag";
const SELECTS_THE_GROUP: &str = "Double-click to select the group";
const SELECTS_THE_TRACK: &str = "Double-click to select the track";

/// One row of the chain that can be moved, as it was drawn: what it is, where, and how round its corners are.
struct DrawnRow {
    key: ChainItemKey,
    rectangle: Rect,
    corners: CornerRadius,
    /// What the pointer did to what it can be taken hold of by.
    handle: Response,
}

/// Shows the chain of `owner` (a clip, a track or a group), and adds a command for each change made in it.
pub(super) fn show(ui: &mut Ui, content: &PropertiesContent<'_>, owner: Member, commands: &mut Vec<Command>) {
    let rows = content.project.chain_rows_of(owner);
    let chain = content.project.chain_of_the_member(owner);
    let mut drawn: Vec<DrawnRow> = Vec::new();
    for row in &rows {
        let shown = match row {
            // (What a text is made of, its text effect, is not a box of the chain: its rows are in the box of the text itself, where they always were. See `text_properties`.)
            ChainRow::Effect(effect) if is_what_a_text_says(content, owner, effect) => continue,
            ChainRow::Effect(effect) => {
                ui.add_space(SPACE_BETWEEN_ROWS);
                show_an_effect(ui, content, owner, chain, effect, commands)
            }
            ChainRow::TagBadge(tag) => {
                ui.add_space(SPACE_BETWEEN_ROWS);
                show_the_badge_of_a_tag(ui, owner, tag, commands)
            }
            ChainRow::TrackBadge {
                track,
                effects_that_apply,
            } => {
                ui.add_space(SPACE_BETWEEN_ROWS);
                show_the_badge_of_a_track(ui, owner, track, *effects_that_apply, commands)
            }
            ChainRow::ShapeBadge(_) => continue,
        };
        drawn.push(shown);
    }
    follow_what_is_taken_hold_of(ui, owner, &drawn, commands);
    show_the_plus_button(ui, content, owner, commands);
    if let Some(ChainRow::ShapeBadge(group)) = rows.last() {
        ui.add_space(SPACE_BETWEEN_ROWS);
        show_the_badge_of_the_group_it_is_in(ui, group, commands);
    }
}

/// Whether `effect` is the text effect that the text clip `owner` is made of, whose rows are those of the text and not a box of the chain.
fn is_what_a_text_says(content: &PropertiesContent<'_>, owner: Member, effect: &Effect) -> bool {
    content
        .project
        .the_clip_whose_content_is(owner, effect.instance())
        .and_then(|clip| content.project.placement_of(clip))
        .is_some_and(|placement| placement.clip.as_text().is_some())
}

/// The command that puts an effect of kind `kind` on `owner`: a clip, a track, or a group (whose effects are not animated).
fn command_that_puts_on(owner: Member, kind: EffectKind) -> Command {
    match owner {
        Member::Clip(clip) => Command::AddEffect {
            target: AnimationTarget::Clip(clip),
            kind,
        },
        Member::Track(track) => Command::AddEffect {
            target: AnimationTarget::Track(track),
            kind,
        },
        Member::Group(group) => Command::AddGroupEffect { group, kind },
    }
}

/// The command that takes the effect that is named `instance` in the chain of `owner` off it.
fn command_that_takes_off(owner: Member, instance: EffectInstanceId) -> Command {
    match owner {
        Member::Clip(clip) => Command::RemoveEffect {
            target: AnimationTarget::Clip(clip),
            instance,
        },
        Member::Track(track) => Command::RemoveEffect {
            target: AnimationTarget::Track(track),
            instance,
        },
        Member::Group(group) => Command::RemoveGroupEffect { group, instance },
    }
}

/// Shows the box of the effect `effect` of `owner`, whose chain is `chain`, which can be taken hold of by its heading.
fn show_an_effect(
    ui: &mut Ui,
    content: &PropertiesContent<'_>,
    owner: Member,
    chain: Option<&Effects>,
    effect: &Effect,
    commands: &mut Vec<Command>,
) -> DrawnRow {
    let kind = effect.kind();
    let mut to_be_deleted = false;
    let mut to_be_switched = false;
    let mut to_be_duplicated = false;
    // (A cell: the menu and the box both can say it.)
    let to_toggle_the_editor = std::cell::Cell::new(false);
    let mut from_the_menu = Vec::new();
    // (A plugin that has a window of its own: the box has a button for it, and so does the menu. Only a clip or a track has the window: it is for the effect that makes the sound.)
    let editor_of = match owner {
        Member::Clip(clip) => Some(AnimationTarget::Clip(clip)),
        Member::Track(track) => Some(AnimationTarget::Track(track)),
        Member::Group(_) => None,
    }
    .filter(|_| kind.installed_plugin().is_some_and(|plugin| plugin.has_a_window))
    .map(|target| {
        (
            target,
            (content.plugin_window_is_open)(target, effect.instance()),
            (content.plugin_window_problem)(target, effect.instance()),
        )
    });
    // (The presets of a plugin: a clip or a track has the menu of them, as it has the window; a group does not.)
    let presets_of = match owner {
        Member::Clip(clip) => Some(AnimationTarget::Clip(clip)),
        Member::Track(track) => Some(AnimationTarget::Track(track)),
        Member::Group(_) => None,
    }
    .zip(kind.installed_plugin().filter(|plugin| !plugin.presets.is_empty()));
    let title = chain.map_or_else(
        || what_an_effect_is_called(kind),
        |chain| what_an_effect_in_a_chain_is_called(chain, effect),
    );
    let drawn = kind_box::show_with_a_menu(
        ui,
        &title,
        |ui| {
            let label = if effect.is_enabled() {
                SWITCH_EFFECT_OFF_LABEL
            } else {
                SWITCH_EFFECT_ON_LABEL
            };
            // (What a generator makes is its first effect, which is not taken off, switched off, copied or moved: it has no such items in its menu.)
            let is_what_a_generator_makes = content
                .project
                .the_clip_whose_content_is(owner, effect.instance())
                .is_some();
            if is_what_a_generator_makes {
                ui.weak(WHAT_A_GENERATOR_MAKES_NOTE);
                return;
            }
            if let Some((_, is_open, _)) = &editor_of
                && ui
                    .button(if *is_open {
                        CLOSE_EDITOR_LABEL
                    } else {
                        OPEN_EDITOR_LABEL
                    })
                    .clicked()
            {
                to_toggle_the_editor.set(true);
                ui.close();
            }
            if let Some((target, plugin)) = presets_of {
                ui.menu_button(PRESETS_LABEL, |ui| {
                    for (number, preset) in plugin.presets.iter().enumerate() {
                        // (The preset that the effect is set to is marked: every value that it gives is the value that the effect has.)
                        let is_set = preset
                            .values
                            .iter()
                            .all(|(slot, value)| effect.values().get(*slot) == Some(value));
                        let text = format!(
                            "{}{}{}",
                            if is_set { PRESET_IS_SET_MARK } else { "" },
                            preset.label,
                            if preset.has_state { PRESET_HAS_A_STATE_NOTE } else { "" }
                        );
                        if ui.button(text).clicked() {
                            from_the_menu.push(Command::ApplyEffectPreset {
                                target,
                                instance: effect.instance(),
                                preset: number,
                            });
                            ui.close();
                        }
                    }
                    ui.separator();
                    ui.weak(PRESETS_NOTE);
                });
            }
            if ui.button(label).clicked() {
                to_be_switched = true;
                ui.close();
            }
            if ui.button(DUPLICATE_EFFECT_LABEL).clicked() {
                to_be_duplicated = true;
                ui.close();
            }
            crate::application::tail_menus::show_the_tail_length_menu_of_an_effect(
                ui,
                owner,
                effect.instance(),
                effect.tail_length(),
                &mut from_the_menu,
            );
            for (label, command) in items_for_where_the_tables_are_kept(content.project, effect) {
                if ui.button(label).clicked() {
                    from_the_menu.push(command);
                    ui.close();
                }
            }
            if ui.button(DELETE_EFFECT_LABEL).clicked() {
                to_be_deleted = true;
                ui.close();
            }
        },
        |ui| {
            // A sound plugin that does nothing says why, in words of its own; any other effect that is not installed says that it is not.
            if let Some(why) = (content.why_an_effect_does_nothing)(kind) {
                ui.label(
                    RichText::new(format!("{NOT_RUNNING_NOTE} {why} {VALUES_ARE_KEPT_NOTE}"))
                        .color(ui.visuals().warn_fg_color),
                );
            } else if !kind.is_installed() {
                ui.label(RichText::new(NOT_INSTALLED_NOTE).weak());
            }
            if kind.depends_on_earlier_frames() {
                ui.label(RichText::new(DEPENDS_ON_EARLIER_FRAMES_NOTE).weak());
            }
            if kind.is_a_transition() {
                ui.label(RichText::new(TRANSITION_NOTE).weak());
            }
            if !effect.is_enabled() {
                ui.label(RichText::new(SWITCHED_OFF_NOTE).weak());
            }
            if let Some((_, is_open, problem)) = &editor_of {
                if ui
                    .button(if *is_open {
                        CLOSE_EDITOR_LABEL
                    } else {
                        OPEN_EDITOR_LABEL
                    })
                    .clicked()
                {
                    to_toggle_the_editor.set(true);
                }
                if let Some(problem) = problem {
                    ui.label(
                        RichText::new(format!("{EDITOR_PROBLEM_NOTE} {problem}")).color(ui.visuals().warn_fg_color),
                    );
                }
            }
            for property in effect.properties() {
                match owner {
                    Member::Clip(clip) => {
                        show_the_control_of(ui, content, AnimationTarget::Clip(clip), kind, property, commands);
                    }
                    Member::Track(track) => {
                        show_the_control_of(ui, content, AnimationTarget::Track(track), kind, property, commands);
                    }
                    Member::Group(group) => {
                        show_the_control_of_a_group_effect(ui, content.project, group, effect, property, commands);
                    }
                }
            }
        },
    );
    if to_be_deleted {
        commands.push(command_that_takes_off(owner, effect.instance()));
    }
    if to_toggle_the_editor.get()
        && let Some((target, is_open, _)) = editor_of
    {
        let instance = effect.instance();
        commands.push(if is_open {
            Command::ClosePluginEditor { target, instance }
        } else {
            Command::OpenPluginEditor { target, instance }
        });
    }
    commands.extend(from_the_menu);
    if to_be_duplicated {
        commands.push(Command::DuplicateEffect {
            owner,
            instance: effect.instance(),
        });
    }
    if to_be_switched {
        commands.push(Command::SetEffectEnabled {
            owner,
            instance: effect.instance(),
            enabled: !effect.is_enabled(),
        });
    }
    draw_grip_dots(
        ui.painter(),
        GripDotArrangement::FOR_A_TRACK_BOX,
        pos2(
            drawn.rectangle.center().x,
            drawn.rectangle.top() + DOTS_UNDER_THE_TOP_OF_A_BOX,
        ),
    );
    DrawnRow {
        key: ChainItemKey::Effect(effect.instance()),
        rectangle: drawn.rectangle,
        corners: CornerRadius::same(theme::CLIP_CORNER_RADIUS),
        handle: drawn.heading,
    }
}

/// Shows the badge of the tag `tag` in the chain of `owner`, which is taken hold of to be moved, and selects the tag when it is double-clicked.
fn show_the_badge_of_a_tag(ui: &mut Ui, owner: Member, tag: &Group, commands: &mut Vec<Command>) -> DrawnRow {
    let key = ChainItemKey::Badge(Badge::Tag(tag.id()));
    let response = badge::show(
        ui,
        (owner, key),
        &describe_a_group(tag),
        BadgeLook::of_a_group(tag),
        true,
        SELECTS_THE_TAG,
    );
    select_when_double_clicked(&response, SelectionItem::Group(tag.id()), commands);
    DrawnRow {
        key,
        rectangle: response.rect,
        corners: CornerRadius::same(badge::CORNER_RADIUS),
        handle: response,
    }
}

/// What the badge of a track says: its title, and how many of the effects of the track apply to the clip ("Track: Music (1 effect)").
fn label_of_the_badge_of_a_track(title: &str, effects_that_apply: usize) -> String {
    let effects = if effects_that_apply == 1 { "effect" } else { "effects" };
    format!("Track: {title} ({effects_that_apply} {effects})")
}

/// Shows the badge of the track a clip is on, with how many of its effects apply to the clip.
fn show_the_badge_of_a_track(
    ui: &mut Ui,
    owner: Member,
    track: &Track,
    effects_that_apply: usize,
    commands: &mut Vec<Command>,
) -> DrawnRow {
    let key = ChainItemKey::Badge(Badge::Track);
    let response = badge::show(
        ui,
        (owner, key),
        &label_of_the_badge_of_a_track(track.title(), effects_that_apply),
        BadgeLook {
            fill: theme::TRACK_BADGE_BACKGROUND,
            writing: theme::TRACK_BADGE_TEXT,
        },
        true,
        SELECTS_THE_TRACK,
    );
    select_when_double_clicked(&response, SelectionItem::Track(track.id()), commands);
    DrawnRow {
        key,
        rectangle: response.rect,
        corners: CornerRadius::same(badge::CORNER_RADIUS),
        handle: response,
    }
}

/// Shows the badge of the clip group or the track group that the thing is directly in, which comes last in the chain and is not moved.
fn show_the_badge_of_the_group_it_is_in(ui: &mut Ui, group: &Group, commands: &mut Vec<Command>) {
    let response = badge::show(
        ui,
        ("the group it is in", group.id()),
        &describe_a_group(group),
        BadgeLook::of_a_group(group),
        false,
        SELECTS_THE_GROUP,
    );
    select_when_double_clicked(&response, SelectionItem::Group(group.id()), commands);
}

/// Where a row that is let go of with the pointer at `pointer_y` goes: before the first row that has its middle under the pointer, or last if
/// there is none. The answer is the number of the row it goes before (they are numbered from 0, from the top), or the number of rows for
/// last. (`middles` are the middles of the rows as they are now, the one that is dragged among them: it is where it is until it is let go of.)
fn slot_at(middles: &[f32], pointer_y: f32) -> usize {
    middles.iter().filter(|middle| **middle < pointer_y).count()
}

/// Whether letting go of the row `taken` at the slot `slot` leaves it where it is: it goes before itself, or before the row after it.
fn leaves_it_where_it_is(taken: usize, slot: usize) -> bool {
    slot == taken || slot == taken + 1
}

/// While a row is being taken hold of, dims it and shows by a line where it will go (between rows, where the pointer is, which is where it goes
/// if it is let go of there), and when it is let go of, makes it go.
fn follow_what_is_taken_hold_of(ui: &mut Ui, owner: Member, drawn: &[DrawnRow], commands: &mut Vec<Command>) {
    let Some(taken) = drawn
        .iter()
        .position(|row| row.handle.dragged() || row.handle.drag_stopped())
    else {
        return;
    };
    let row = &drawn[taken];
    ui.painter()
        .rect_filled(row.rectangle, row.corners, theme::BEING_DRAGGED_VEIL);
    let Some(pointer) = row
        .handle
        .interact_pointer_pos()
        .or_else(|| ui.input(|input| input.pointer.interact_pos()))
    else {
        return;
    };
    let middles: Vec<f32> = drawn.iter().map(|other| other.rectangle.center().y).collect();
    let place = slot_at(&middles, pointer.y);
    if leaves_it_where_it_is(taken, place) {
        return;
    }
    let (left, right) = (row.rectangle.left(), row.rectangle.right());
    let y = match drawn.get(place) {
        Some(next) => next.rectangle.top() - SPACE_BETWEEN_ROWS / 2.0,
        None => drawn
            .last()
            .map_or(0.0, |last| last.rectangle.bottom() + SPACE_BETWEEN_ROWS / 2.0),
    };
    ui.painter().line_segment(
        [pos2(left, y), pos2(right, y)],
        Stroke::new(THICKNESS_OF_THE_LINE, theme::WHERE_IT_WILL_GO),
    );
    if row.handle.drag_stopped() {
        commands.push(Command::MoveChainItem {
            owner,
            item: row.key,
            before: drawn.get(place).map(|next| next.key),
        });
    }
}

/// The kinds of effect that a thing of the kind `owner` is can have, which the plus button offers.
fn kinds_that_can_be_put_on(content: &PropertiesContent<'_>, owner: Member) -> impl Fn(EffectKind) -> bool {
    let clip_kind = match owner {
        Member::Clip(clip) => content
            .project
            .placement_of(clip)
            .map(|placement| placement.clip.kind()),
        Member::Track(_) | Member::Group(_) => None,
    };
    let group_kind: Option<GroupKind> = match owner {
        Member::Group(group) => content.project.group(group).map(Group::kind),
        Member::Clip(_) | Member::Track(_) => None,
    };
    move |kind: EffectKind| match (owner, clip_kind, group_kind) {
        (Member::Clip(_), Some(clip_kind), _) => kind.applies_to_a_clip_of_kind(clip_kind),
        (Member::Group(_), _, Some(group_kind)) => kind.applies_to_a_group_of_kind(group_kind),
        (Member::Track(_), _, _) => kind.applies_to_a_track(),
        _ => false,
    }
}

/// Shows the menu of the plus button: a field to search the effects (which has the keyboard at once, and Enter adds the first that matches), a smaller menu for each place the
/// effects that match come from (*Built-in*, *LADSPA*...), in which they are listed under the heading of their categories, and at the bottom *Add effect from file...*. A
/// smaller menu that is taller than the window has a scroll bar.
fn show_the_menu_of_effects(
    ui: &mut Ui,
    to_offer: &[EffectKind],
    offers_a_file: bool,
    owner: Member,
    commands: &mut Vec<Command>,
) {
    let id = ui.id().with("the search of the effects");
    let mut typed: String = ui.data(|data| data.get_temp(id)).unwrap_or_default();
    // (A menu that was not drawn in the pass before this one has just been opened: it begins empty, and the field has the keyboard.)
    let this_pass = ui.ctx().cumulative_pass_nr();
    let just_opened = ui
        .data(|data| data.get_temp::<u64>(id.with("pass")))
        .is_none_or(|last| this_pass > last + 1);
    ui.data_mut(|data| data.insert_temp(id.with("pass"), this_pass));
    if just_opened {
        typed.clear();
    }
    ui.set_min_width(MENU_OF_EFFECTS_WIDTH);
    let search = ui.add(
        TextEdit::singleline(&mut typed)
            .hint_text(SEARCH_EFFECTS_HINT)
            .desired_width(f32::INFINITY),
    );
    if just_opened {
        search.request_focus();
    }
    let matching = effects_that_match(to_offer, &typed);
    let enter_was_pressed = search.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
    let mut chosen = None;
    if enter_was_pressed {
        chosen = matching.first().copied();
    }
    if matching.is_empty() {
        ui.label(RichText::new(NO_EFFECT_FOUND_NOTE).weak());
    }
    for (source, from_there) in grouped_by_source(&matching) {
        ui.menu_button(source, |ui| {
            ui.set_min_width(MENU_OF_EFFECTS_WIDTH);
            let most_tall = (ui.ctx().content_rect().height() - ROOM_LEFT_ROUND_A_SMALLER_MENU)
                .max(MOST_TALL_A_SMALLER_MENU_IS_AT_LEAST);
            ScrollArea::vertical().max_height(most_tall).show(ui, |ui| {
                for (heading, members) in grouped_by_category(&from_there) {
                    ui.label(RichText::new(heading).weak().small());
                    for kind in members {
                        if ui
                            .button(what_an_effect_is_called(kind))
                            .on_hover_text(kind.description())
                            .clicked()
                        {
                            chosen = Some(kind);
                        }
                    }
                }
            });
        });
    }
    let mut from_a_file = false;
    if offers_a_file {
        ui.separator();
        from_a_file = ui.button(ADD_EFFECT_FROM_FILE_LABEL).clicked();
    }
    if let Some(kind) = chosen {
        commands.push(command_that_puts_on(owner, kind));
    }
    if from_a_file {
        commands.push(Command::AddEffectFromFile { owner });
    }
    if chosen.is_some() || from_a_file {
        typed.clear();
        ui.close();
    }
    ui.data_mut(|data| data.insert_temp(id, typed));
}

/// How wide the menu of effects is at least, in points.
const MENU_OF_EFFECTS_WIDTH: f32 = 220.0;

/// How much of the height of the window a smaller menu of effects leaves for the room round it, in points, and how tall it may be at the least (a window that is very low).
const ROOM_LEFT_ROUND_A_SMALLER_MENU: f32 = 60.0;
const MOST_TALL_A_SMALLER_MENU_IS_AT_LEAST: f32 = 120.0;

/// Shows the plus button under the effects, which opens the menu of the effects that can be put on, and adds a command when one is chosen. A
/// thing that no effect can be put on is not given a plus button, as it would have nothing to put on.
fn show_the_plus_button(ui: &mut Ui, content: &PropertiesContent<'_>, owner: Member, commands: &mut Vec<Command>) {
    if content.project.chain_of_the_member(owner).is_none() {
        return;
    }
    let can_have = kinds_that_can_be_put_on(content, owner);
    let offered = EffectKind::offered();
    if !offered.iter().copied().any(&can_have) {
        return;
    }
    let to_offer = effects_to_offer(offered, &can_have);
    let room = vec2(
        ui.available_width(),
        plus_button::DIAMETER + 2.0 * SPACE_ROUND_THE_PLUS_BUTTON,
    );
    let (row, _) = ui.allocate_exact_size(room, egui::Sense::hover());
    let button = plus_button::show(
        ui,
        pos2(row.center().x, row.center().y),
        ("add an effect to", owner),
        ADD_EFFECT_LABEL,
        ui.is_enabled() && !to_offer.is_empty(),
        theme::PROPERTIES_BOX_BACKGROUND,
    );
    let offers_a_file = to_offer.contains(&EffectKind::LUT);
    Popup::menu(&button).show(|ui| show_the_menu_of_effects(ui, &to_offer, offers_a_file, owner, commands));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_that_is_let_go_of_goes_before_the_first_row_that_has_its_middle_under_the_pointer_and_last_if_there_is_none()
     {
        let middles = [10.0, 40.0, 70.0];
        assert_eq!(slot_at(&middles, 0.0), 0, "above them all");
        assert_eq!(slot_at(&middles, 25.0), 1, "between the first and the second");
        assert_eq!(slot_at(&middles, 55.0), 2);
        assert_eq!(slot_at(&middles, 100.0), 3, "under them all: last");
        assert_eq!(slot_at(&middles, 40.0), 1, "exactly on a middle is not past it");
        assert_eq!(slot_at(&middles, 40.5), 2, "and just past it is");
        assert_eq!(slot_at(&[], 5.0), 0, "no rows, no place but the first");
    }

    #[test]
    fn letting_go_before_itself_or_before_the_row_after_it_leaves_a_row_where_it_is_and_anything_else_moves_it() {
        assert!(leaves_it_where_it_is(1, 1));
        assert!(leaves_it_where_it_is(1, 2));
        assert!(!leaves_it_where_it_is(1, 0));
        assert!(!leaves_it_where_it_is(1, 3));
        assert!(leaves_it_where_it_is(0, 0));
        assert!(!leaves_it_where_it_is(0, 2), "past the row after it is a move");
    }

    #[test]
    fn the_badge_of_a_track_says_how_many_of_its_effects_apply_with_the_word_in_the_singular_for_one() {
        assert_eq!(label_of_the_badge_of_a_track("Music", 1), "Track: Music (1 effect)");
        assert_eq!(label_of_the_badge_of_a_track("Music", 2), "Track: Music (2 effects)");
        assert_eq!(label_of_the_badge_of_a_track("", 3), "Track:  (3 effects)");
    }

    #[test]
    fn the_command_that_puts_an_effect_on_and_the_one_that_takes_it_off_are_those_of_what_has_the_effect() {
        use crate::project::{ClipId, GroupId, TrackId};

        let track = TrackId::new(2);
        let clip = ClipId::new(track, 5);
        let group = GroupId::new(1);
        let hue = EffectKind::HUE_SHIFT;
        assert_eq!(
            command_that_puts_on(Member::Clip(clip), hue),
            Command::AddEffect {
                target: AnimationTarget::Clip(clip),
                kind: hue
            }
        );
        assert_eq!(
            command_that_puts_on(Member::Track(track), hue),
            Command::AddEffect {
                target: AnimationTarget::Track(track),
                kind: hue
            }
        );
        assert_eq!(
            command_that_puts_on(Member::Group(group), hue),
            Command::AddGroupEffect { group, kind: hue }
        );
        let second = EffectInstanceId::new(1);
        assert_eq!(
            command_that_takes_off(Member::Clip(clip), second),
            Command::RemoveEffect {
                target: AnimationTarget::Clip(clip),
                instance: second
            }
        );
        assert_eq!(
            command_that_takes_off(Member::Track(track), second),
            Command::RemoveEffect {
                target: AnimationTarget::Track(track),
                instance: second
            }
        );
        assert_eq!(
            command_that_takes_off(Member::Group(group), second),
            Command::RemoveGroupEffect {
                group,
                instance: second
            }
        );
    }
}
