// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The menus that open when a rack of keyframes, or a keyframe, is right-clicked.
//!
//! Each is about what is selected when the one that was clicked is one of what is selected (right-clicking one of several selected
//! racks, or keyframes, leaves them all selected: see [`super::racks`]), and about the one that was clicked when it is not.
//!
//! * The menu of racks has "Merge racks", which makes one rack of them (see [`crate::project::Project::merge_racks`]), and an item
//!   that takes them away. Merging is for racks of one property of one thing, so it can only be used when several are selected,
//!   and then only if they can be merged: the item says why not when it is hovered over.
//! * The menu of keyframes has the items that put a copy of each of them, as a group, beside them on either side (see
//!   [`crate::project::Project::duplicate_keyframes`]), which says why not when the copies would not all have room, and an item
//!   that takes them away.
//!
//! What takes things away is last, apart from the others, so that it is not what a click that has hardly moved from the
//! right-click chooses.

use egui::{Button, Ui};

use crate::application::commands::Command;
use crate::project::{EditError, KeyframeId, Project, RackEditError, RackId, TimeDirection};

/// What is said of the item that merges racks when fewer than two are selected.
const SELECT_MORE_RACKS: &str = "Select two or more racks of the same property (with Ctrl) to merge them";
/// What is said of it when the racks are not all of one property of one thing.
const NOT_THE_SAME_THING: &str = "Racks can only be merged when they are all of the same property of the same thing";
/// What is said of it when no drawer that one of the racks is in has room for all of their keyframes.
const NO_DRAWER_HAS_ROOM: &str = "No drawer that one of the racks is in has room for all of their keyframes";
/// What is said of the item that copies keyframes when the copies would not all be in a drawer that has the frames for them.
const NO_ROOM_FOR_THE_COPIES: &str = "There is no room there for all the copies";
/// What is said of it when a copy would be at the frame of a keyframe that is there.
const A_COPY_WOULD_BE_ON_A_KEYFRAME: &str = "A copy would be on a keyframe that is there";
/// What is said of an item that cannot be used, when it is not known why.
const NOT_POSSIBLE: &str = "That is not possible";

/// The racks the menu of `clicked` is about, when `selected` are the racks that are selected: those, if it is one of them, and
/// otherwise just `clicked`.
pub(super) fn racks_the_menu_is_about(selected: &[RackId], clicked: RackId) -> Vec<RackId> {
    if selected.contains(&clicked) {
        selected.to_vec()
    } else {
        vec![clicked]
    }
}

/// The keyframes the menu of `clicked` is about, when `selected` are the keyframes that are selected: those, if it is one of them,
/// and otherwise just `clicked`.
pub(super) fn keyframes_the_menu_is_about(selected: &[KeyframeId], clicked: KeyframeId) -> Vec<KeyframeId> {
    if selected.contains(&clicked) {
        selected.to_vec()
    } else {
        vec![clicked]
    }
}

/// What the item that takes away `count` racks says.
pub(super) fn delete_racks_label(count: usize) -> String {
    match count {
        1 => "Delete rack".to_owned(),
        _ => format!("Delete {count} racks"),
    }
}

/// What the item that takes away `count` keyframes says.
pub(super) fn delete_keyframes_label(count: usize) -> String {
    match count {
        1 => "Delete keyframe".to_owned(),
        _ => format!("Delete {count} keyframes"),
    }
}

/// What the item that puts a copy of each of `count` keyframes later or earlier, as `towards` says, says.
pub(super) fn duplicate_label(count: usize, towards: TimeDirection) -> String {
    let side = match towards {
        TimeDirection::Later => "right",
        TimeDirection::Earlier => "left",
    };
    match count {
        1 => format!("Duplicate to the {side}"),
        _ => format!("Duplicate {count} keyframes to the {side}"),
    }
}

/// Why racks cannot be merged, when [`Project::check_the_merging_of_racks`] gave `error`, for the item to say when it is hovered
/// over.
pub(super) fn why_racks_cannot_be_merged(error: EditError) -> &'static str {
    match error {
        EditError::TooFewRacksToMerge => SELECT_MORE_RACKS,
        EditError::RacksOfDifferentThingsCannotBeMerged => NOT_THE_SAME_THING,
        EditError::NoRoomForTheKeyframes => NO_DRAWER_HAS_ROOM,
        _ => NOT_POSSIBLE,
    }
}

/// Why keyframes cannot be copied, when [`Project::check_the_duplicating_of_keyframes`] gave `error`.
pub(super) fn why_keyframes_cannot_be_copied(error: EditError) -> &'static str {
    match error {
        EditError::NoRoomForTheKeyframes => NO_ROOM_FOR_THE_COPIES,
        EditError::Rack(RackEditError::FramesWouldCoincide) => A_COPY_WOULD_BE_ON_A_KEYFRAME,
        _ => NOT_POSSIBLE,
    }
}

/// The menu of `racks` (at least one), which adds a command when something in it is chosen.
pub(super) fn show_for_racks(ui: &mut Ui, racks: &[RackId], project: &Project, commands: &mut Vec<Command>) {
    let merging = project.check_the_merging_of_racks(racks);
    let mut item = ui.add_enabled(merging.is_ok(), Button::new("Merge racks"));
    if let Err(error) = merging {
        item = item.on_disabled_hover_text(why_racks_cannot_be_merged(error));
    }
    if item.clicked() {
        commands.push(Command::MergeRacks(racks.to_vec()));
        ui.close();
    }
    ui.separator();
    if ui.button(delete_racks_label(racks.len())).clicked() {
        commands.push(Command::DeleteRacks(racks.to_vec()));
        ui.close();
    }
}

/// The menu of `keyframes` (at least one), which adds a command when something in it is chosen. The copies are `gap_in_frames`
/// frames from the keyframes at the least (so many frames as a keyframe is wide on the screen).
pub(super) fn show_for_keyframes(
    ui: &mut Ui,
    keyframes: &[KeyframeId],
    project: &Project,
    gap_in_frames: u64,
    commands: &mut Vec<Command>,
) {
    for towards in [TimeDirection::Later, TimeDirection::Earlier] {
        let copying = project.check_the_duplicating_of_keyframes(keyframes, towards, gap_in_frames);
        let mut item = ui.add_enabled(copying.is_ok(), Button::new(duplicate_label(keyframes.len(), towards)));
        if let Err(error) = copying {
            item = item.on_disabled_hover_text(why_keyframes_cannot_be_copied(error));
        }
        if item.clicked() {
            commands.push(Command::DuplicateKeyframes {
                keyframes: keyframes.to_vec(),
                towards,
                gap_in_frames,
            });
            ui.close();
        }
    }
    ui.separator();
    if ui.button(delete_keyframes_label(keyframes.len())).clicked() {
        commands.push(Command::DeleteKeyframes(keyframes.to_vec()));
        ui.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::TrackId;

    fn rack(number: u32) -> RackId {
        RackId::new(TrackId::new(0), number)
    }

    fn keyframe(number: u32) -> KeyframeId {
        KeyframeId::new(rack(0), number)
    }

    #[test]
    fn the_menu_of_a_rack_that_is_selected_is_about_all_the_selected_racks_and_of_one_that_is_not_about_that_one() {
        let selected = [rack(1), rack(2), rack(5)];
        assert_eq!(racks_the_menu_is_about(&selected, rack(2)), selected);
        assert_eq!(racks_the_menu_is_about(&selected, rack(3)), [rack(3)]);
        assert_eq!(racks_the_menu_is_about(&[], rack(3)), [rack(3)], "nothing is selected");
    }

    #[test]
    fn the_menu_of_a_keyframe_that_is_selected_is_about_all_the_selected_keyframes_and_of_one_that_is_not_about_that_one()
     {
        let selected = [keyframe(1), keyframe(4)];
        assert_eq!(keyframes_the_menu_is_about(&selected, keyframe(4)), selected);
        assert_eq!(keyframes_the_menu_is_about(&selected, keyframe(2)), [keyframe(2)]);
        assert_eq!(keyframes_the_menu_is_about(&[], keyframe(2)), [keyframe(2)]);
    }

    #[test]
    fn the_items_say_how_many_they_are_about_when_it_is_not_one() {
        assert_eq!(delete_racks_label(1), "Delete rack");
        assert_eq!(delete_racks_label(3), "Delete 3 racks");
        assert_eq!(delete_keyframes_label(1), "Delete keyframe");
        assert_eq!(delete_keyframes_label(4), "Delete 4 keyframes");
        assert_eq!(duplicate_label(1, TimeDirection::Later), "Duplicate to the right");
        assert_eq!(duplicate_label(1, TimeDirection::Earlier), "Duplicate to the left");
        assert_eq!(
            duplicate_label(3, TimeDirection::Later),
            "Duplicate 3 keyframes to the right"
        );
        assert_eq!(
            duplicate_label(2, TimeDirection::Earlier),
            "Duplicate 2 keyframes to the left"
        );
    }

    #[test]
    fn what_cannot_be_merged_or_copied_says_why_in_words_of_its_own() {
        let merge_reasons = [
            why_racks_cannot_be_merged(EditError::TooFewRacksToMerge),
            why_racks_cannot_be_merged(EditError::RacksOfDifferentThingsCannotBeMerged),
            why_racks_cannot_be_merged(EditError::NoRoomForTheKeyframes),
        ];
        let copy_reasons = [
            why_keyframes_cannot_be_copied(EditError::NoRoomForTheKeyframes),
            why_keyframes_cannot_be_copied(EditError::Rack(RackEditError::FramesWouldCoincide)),
        ];
        let mut all: Vec<&str> = merge_reasons.iter().chain(&copy_reasons).copied().collect();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), 5, "no two reasons are the same words: {all:?}");
        assert_eq!(
            why_racks_cannot_be_merged(EditError::UnknownRack { rack: rack(1) }),
            NOT_POSSIBLE,
            "what is not one of the usual reasons"
        );
        assert_eq!(
            why_keyframes_cannot_be_copied(EditError::UnknownRack { rack: rack(1) }),
            NOT_POSSIBLE
        );
    }
}
