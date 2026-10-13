//! The menu that opens when a clip is right-clicked: put a copy of it right beside it, on either side, or delete it.
//!
//! The menu is about the clips that are selected when the clip that was clicked is one of them (right-clicking one of several
//! selected clips leaves all of them selected: see [`super::strip`]), and about the clip that was clicked when it is not.
//! A clip with no room beside it for a copy (something else is there, or the timeline begins there) is not copied. When none of
//! the clips has room the item cannot be used, and says why when it is hovered over; when only some do, it copies those, and
//! says so.
//!
//! The copy of a clip has a copy of everything in its drawer too (see [`crate::project::Project::duplicate_clip`]).
//!
//! A clip that is made from a file (a video, a sound, a picture) has an item to include that file in the project file, which then
//! has it in it (see [`crate::project_file`]), and one to take it out of the project file again, whichever it is now. The items are
//! about the files of the clips the menu is about, one for each file however many of the clips are made from it.

use egui::{Button, Ui};

use crate::application::commands::Command;
use crate::project::{ClipId, ClipSide, Project, SourceMediaIdentifier};

/// What is said of an item that copies a clip when there is no room beside it for the copy.
const NO_ROOM_FOR_ONE: &str = "There is no room beside it for a copy";
/// What is said of an item that copies several clips when there is no room beside any of them for a copy.
const NO_ROOM_FOR_ANY: &str = "There is no room beside any of them for a copy";
/// What is said of an item that copies several clips when there is room beside some of them, but not all.
const NO_ROOM_FOR_SOME: &str = "The clips that have no room beside them are not copied";

/// What the item that deletes `count` clips says.
pub(super) fn delete_label(count: usize) -> String {
    match count {
        1 => "Delete clip".to_owned(),
        _ => format!("Delete {count} clips"),
    }
}

/// What the item that includes `count` files in the project file says.
pub(super) fn include_label(count: usize) -> String {
    match count {
        1 => "Include in project file".to_owned(),
        _ => format!("Include {count} files in project file"),
    }
}

/// What the item that takes `count` files out of the project file says.
pub(super) fn take_out_label(count: usize) -> String {
    match count {
        1 => "Remove from project file".to_owned(),
        _ => format!("Remove {count} files from project file"),
    }
}

/// What the item that puts a copy of each of `count` clips on `side` of it says.
pub(super) fn duplicate_label(count: usize, side: ClipSide) -> String {
    let towards = match side {
        ClipSide::Right => "right",
        ClipSide::Left => "left",
    };
    match count {
        1 => format!("Duplicate to the {towards}"),
        _ => format!("Duplicate {count} clips to the {towards}"),
    }
}

/// What is to be said about the room beside `count` clips for their copies, when `with_room` of them have it: nothing if all
/// have, and otherwise that none have or that some do not.
fn remark_on_the_room(count: usize, with_room: usize) -> Option<&'static str> {
    if with_room == 0 {
        Some(if count == 1 { NO_ROOM_FOR_ONE } else { NO_ROOM_FOR_ANY })
    } else if with_room < count {
        Some(NO_ROOM_FOR_SOME)
    } else {
        None
    }
}

/// The clips the menu of `clicked` is about, when `selected` are the clips that are selected: those, if it is one of them, and
/// otherwise just `clicked`.
pub(super) fn clips_the_menu_is_about(selected: &[ClipId], clicked: ClipId) -> Vec<ClipId> {
    if selected.contains(&clicked) {
        selected.to_vec()
    } else {
        vec![clicked]
    }
}

/// The files that `clips` are made from, each once: those that are not included in the project file, and those that are.
pub(super) fn files_of(
    clips: &[ClipId],
    project: &Project,
) -> (Vec<SourceMediaIdentifier>, Vec<SourceMediaIdentifier>) {
    let mut not_included = Vec::new();
    let mut included = Vec::new();
    for clip in clips {
        let Some(identifier) = project
            .placement_of(*clip)
            .and_then(|placement| placement.clip.source())
        else {
            continue;
        };
        let Some(source) = project.source(identifier) else {
            continue;
        };
        let list = if source.link.is_included() {
            &mut included
        } else {
            &mut not_included
        };
        if !list.contains(&identifier) {
            list.push(identifier);
        }
    }
    (not_included, included)
}

/// The menu of `clips` (at least one), which adds a command when something in it is chosen. What deletes them is last, apart
/// from the others, so that it is not what a click that has hardly moved from the right-click chooses.
pub(super) fn show(ui: &mut Ui, clips: &[ClipId], project: &Project, commands: &mut Vec<Command>) {
    let count = clips.len();
    for side in [ClipSide::Right, ClipSide::Left] {
        let with_room = clips
            .iter()
            .filter(|clip| project.room_for_a_copy_of(**clip, side))
            .count();
        let mut item = ui.add_enabled(with_room > 0, Button::new(duplicate_label(count, side)));
        if let Some(remark) = remark_on_the_room(count, with_room) {
            item = if with_room > 0 {
                item.on_hover_text(remark)
            } else {
                item.on_disabled_hover_text(remark)
            };
        }
        if item.clicked() {
            commands.push(Command::DuplicateClips {
                clips: clips.to_vec(),
                side,
            });
            ui.close();
        }
    }
    let (not_included, included) = files_of(clips, project);
    if !not_included.is_empty() || !included.is_empty() {
        ui.separator();
    }
    if !not_included.is_empty() && ui.button(include_label(not_included.len())).clicked() {
        commands.push(Command::IncludeFilesInTheProjectFile(not_included));
        ui.close();
    }
    if !included.is_empty() && ui.button(take_out_label(included.len())).clicked() {
        commands.push(Command::TakeFilesOutOfTheProjectFile(included));
        ui.close();
    }
    ui.separator();
    if ui.button(delete_label(count)).clicked() {
        commands.push(Command::DeleteClips(clips.to_vec()));
        ui.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::TrackId;

    fn clip(number: u32) -> ClipId {
        ClipId::new(TrackId::new(0), number)
    }

    #[test]
    fn the_items_say_how_many_clips_they_are_about_when_it_is_not_one() {
        assert_eq!(delete_label(1), "Delete clip");
        assert_eq!(delete_label(3), "Delete 3 clips");
        assert_eq!(duplicate_label(1, ClipSide::Right), "Duplicate to the right");
        assert_eq!(duplicate_label(1, ClipSide::Left), "Duplicate to the left");
        assert_eq!(duplicate_label(2, ClipSide::Right), "Duplicate 2 clips to the right");
        assert_eq!(duplicate_label(5, ClipSide::Left), "Duplicate 5 clips to the left");
    }

    #[test]
    fn the_items_that_include_a_file_and_take_it_out_say_how_many_files_they_are_about_when_it_is_not_one() {
        assert_eq!(include_label(1), "Include in project file");
        assert_eq!(include_label(3), "Include 3 files in project file");
        assert_eq!(take_out_label(1), "Remove from project file");
        assert_eq!(take_out_label(2), "Remove 2 files from project file");
    }

    /// A project with a video cut in two clips and a text object in front of them, and the clips of each.
    fn a_video_in_two_pieces_and_a_text() -> (Project, [ClipId; 2], ClipId) {
        use crate::project::{FrameRate, SourceMedia, SourceVideoProperties, TextObject, TimelineFrameIndex};

        let mut project = Project::empty();
        project
            .import_source(
                SourceMedia::video(
                    SourceMediaIdentifier::new(1),
                    crate::project::SourceMediaLink::from_absolute_path("holiday.mp4".into()),
                    SourceVideoProperties {
                        frame_rate: FrameRate::new(30, 1).unwrap(),
                        total_frame_count: 60,
                        display_width_in_pixels: 320,
                        display_height_in_pixels: 180,
                        has_audio: false,
                    },
                ),
                TimelineFrameIndex::new(0),
            )
            .unwrap();
        project.split_at(TimelineFrameIndex::new(30), None);
        let pieces: Vec<ClipId> = project.tracks()[0].placements().map(|placement| placement.id).collect();
        let in_front = project.add_track_in_front();
        let (text, _) = project
            .insert_text(in_front, TimelineFrameIndex::new(0), TextObject::default(), 30)
            .unwrap();
        (project, [pieces[0], pieces[1]], text)
    }

    #[test]
    fn clips_made_from_one_file_are_one_file_to_include_and_a_text_object_is_made_from_none() {
        let (project, pieces, text) = a_video_in_two_pieces_and_a_text();
        let video = SourceMediaIdentifier::new(1);

        assert_eq!(files_of(&pieces, &project), (vec![video], vec![]), "one file, not two");
        assert_eq!(files_of(&pieces[..1], &project), (vec![video], vec![]));
        assert_eq!(files_of(&[text], &project), (vec![], vec![]));
        assert_eq!(files_of(&[pieces[0], text, pieces[1]], &project), (vec![video], vec![]));
        assert_eq!(files_of(&[], &project), (vec![], vec![]));
    }

    #[test]
    fn a_file_that_is_included_is_listed_apart_from_those_that_are_not() {
        let (mut project, pieces, _) = a_video_in_two_pieces_and_a_text();
        let video = SourceMediaIdentifier::new(1);
        let mut link = project.source(video).unwrap().link.clone();
        link.included_as = Some("resources/1-holiday.mp4".to_owned());
        project.set_source_link(video, link);

        assert_eq!(files_of(&pieces, &project), (vec![], vec![video]));
    }

    #[test]
    fn the_room_is_remarked_on_only_when_some_clip_has_none() {
        assert_eq!(remark_on_the_room(1, 1), None);
        assert_eq!(remark_on_the_room(4, 4), None);
        assert_eq!(remark_on_the_room(1, 0), Some(NO_ROOM_FOR_ONE));
        assert_eq!(remark_on_the_room(3, 0), Some(NO_ROOM_FOR_ANY));
        assert_eq!(remark_on_the_room(3, 1), Some(NO_ROOM_FOR_SOME));
        assert_eq!(remark_on_the_room(3, 2), Some(NO_ROOM_FOR_SOME));
        assert_ne!(NO_ROOM_FOR_ONE, NO_ROOM_FOR_ANY, "one clip is not several");
    }

    #[test]
    fn the_menu_of_a_clip_that_is_selected_is_about_all_the_selected_clips_and_of_one_that_is_not_about_that_one() {
        let selected = [clip(1), clip(2), clip(5)];
        assert_eq!(clips_the_menu_is_about(&selected, clip(2)), selected);
        assert_eq!(clips_the_menu_is_about(&selected, clip(3)), [clip(3)]);
        assert_eq!(clips_the_menu_is_about(&[], clip(3)), [clip(3)], "nothing is selected");
    }
}
