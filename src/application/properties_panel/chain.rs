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

use egui::{CornerRadius, Popup, Rect, Response, Stroke, Ui, pos2, vec2};

use super::PropertiesContent;
use super::badge::{self, BadgeLook};
use super::effects::{
    ADD_EFFECT_LABEL, DELETE_EFFECT_LABEL, effects_to_offer, show_the_control_of, show_the_control_of_a_group_effect,
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
    let title = chain.map_or_else(
        || what_an_effect_is_called(kind),
        |chain| what_an_effect_in_a_chain_is_called(chain, effect),
    );
    let drawn = kind_box::show_with_a_menu(
        ui,
        &title,
        |ui| {
            if ui.button(DELETE_EFFECT_LABEL).clicked() {
                to_be_deleted = true;
                ui.close();
            }
        },
        |ui| {
            for property in effect.properties() {
                match owner {
                    Member::Clip(clip) => {
                        show_the_control_of(ui, content, AnimationTarget::Clip(clip), kind, property, commands);
                    }
                    Member::Track(track) => {
                        show_the_control_of(ui, content, AnimationTarget::Track(track), kind, property, commands);
                    }
                    Member::Group(group) => {
                        show_the_control_of_a_group_effect(ui, group, effect, property, commands);
                    }
                }
            }
        },
    );
    if to_be_deleted {
        commands.push(command_that_takes_off(owner, effect.instance()));
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

/// Shows the plus button under the effects, which opens the menu of the effects that can be put on, and adds a command when one is chosen. A
/// thing that no effect can be put on is not given a plus button, as it would have nothing to put on.
fn show_the_plus_button(ui: &mut Ui, content: &PropertiesContent<'_>, owner: Member, commands: &mut Vec<Command>) {
    if content.project.chain_of_the_member(owner).is_none() {
        return;
    }
    let can_have = kinds_that_can_be_put_on(content, owner);
    if !EffectKind::ALL.into_iter().any(&can_have) {
        return;
    }
    let to_offer = effects_to_offer(EffectKind::ALL, &can_have);
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
    Popup::menu(&button).show(|ui| {
        for kind in &to_offer {
            if ui
                .button(what_an_effect_is_called(*kind))
                .on_hover_text(kind.description())
                .clicked()
            {
                commands.push(command_that_puts_on(owner, *kind));
                ui.close();
            }
        }
    });
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
        let hue = EffectKind::HueShift;
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
