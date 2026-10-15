// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Including the files a project is made from in the project file, and taking them out again.
//!
//! A project file that is a `.ffrs` archive can have files in it (see [`crate::project_file::archive`]): the ones that the user
//! chose, from the menu of a clip, to include. Which ones is part of the project (see [`crate::project::SourceMediaLink`]) but is
//! not an edit that can be undone, as choosing a file again is not: it is where a file is kept, and not what is done with it. A
//! file that is included is read where it is in the archive.
//!
//! * **Including** a file that is found, in a project whose file is an archive, writes it into the archive now, with the project
//!   (which says that it is included): the files that are in the archive already are not written again. If the project's file is not
//!   an archive (it has none yet, or it is plain JSON), the user is told that it has to be one, and asked where to save it as one;
//!   and when they choose, the files are included and the project is saved there, as one step.
//! * **Taking out** a file puts it back where it was when it was included (the place it had then, and if that has gone, the same
//!   place relative to the folder the project file is in now): where the same file is already, nothing is copied; where another file
//!   is, the place is not used, and where nothing is, the file is put there if the folder is. If there is no place, it stays in the
//!   project file, and the user is told. (Its bytes stay in the archive until the next time the archive is written anew.)

use std::fs;
use std::path::{Path, PathBuf};

use super::{FastForwardApplication, OpeningPurpose, SourceStatus};
use crate::application::dialogs::Dialog;
use crate::application::file_chooser::{FileKind, FileQuestion};
use crate::atomic_file::replace_file_atomically;
use crate::media::MediaLocation;
use crate::project::{SourceMediaIdentifier, SourceMediaLink};
use crate::project_file::archive::{copy_bytes_of, have_the_same_contents, resource_entry_name};
use crate::project_file::{ARCHIVE_FILE_EXTENSION, is_the_name_of_an_archive, relative_path_between};

impl FastForwardApplication {
    /// Whether the project's file is an archive, which is what can have files included in it.
    pub(super) fn project_file_is_an_archive(&self) -> bool {
        self.project_file_path.as_deref().is_some_and(is_the_name_of_an_archive)
    }

    /// The name the file of the source file `identifier` is known by to the user, for saying which it is.
    fn name_of_the_file_of(&self, identifier: SourceMediaIdentifier) -> String {
        self.project
            .source(identifier)
            .and_then(|source| source.link.absolute_path.file_name())
            .map_or_else(
                || format!("file {identifier}"),
                |name| name.to_string_lossy().into_owned(),
            )
    }

    /// Where the bytes of the file of the source file `identifier` are now, if it has been found and opens.
    fn location_of_the_file_of(&self, identifier: SourceMediaIdentifier) -> Option<MediaLocation> {
        self.source_files.get(&identifier)?.location_if_available().cloned()
    }

    /// Puts the files of the source files `identifiers` in the project file: the ones that are found, and are not included
    /// already. If the project's file is not an archive, the user is asked first to save it as one (see
    /// [`Self::save_as_an_archive_with_files_included`]). A file that cannot be found is not included, and is said so. (The project is
    /// saved as the files are put in it, with all else that was changed in it.)
    pub(super) fn include_files_in_the_project_file(&mut self, identifiers: &[SourceMediaIdentifier]) {
        let (to_include, not_found) = self.files_that_can_be_included(identifiers);
        if to_include.is_empty() {
            if !not_found.is_empty() {
                self.show_message_about_files_that_are_not_found(&not_found);
            }
            return;
        }
        if let Some(archive) = self
            .project_file_path
            .clone()
            .filter(|_| self.project_file_is_an_archive())
        {
            let project_directory = self.project_directory();
            let project_before = self.project.clone();
            self.mark_as_included(&to_include, project_directory.as_deref());
            if self.save_the_project(archive, Some(project_before)) && !not_found.is_empty() {
                self.show_message_about_files_that_are_not_found(&not_found);
            }
        } else {
            self.pause_playback_for_dialog();
            self.dialog = Some(Dialog::MustSaveAsAnArchiveToIncludeFiles {
                files: to_include,
                has_a_plain_project_file: self.project_file_path.is_some(),
            });
        }
    }

    /// Of the source files `identifiers`, those whose files can be included (they are found, and not included already), and the
    /// names of the files that cannot be because they are not found. Each source file once, in the order given.
    fn files_that_can_be_included(
        &self,
        identifiers: &[SourceMediaIdentifier],
    ) -> (Vec<SourceMediaIdentifier>, Vec<String>) {
        let mut to_include = Vec::new();
        let mut not_found = Vec::new();
        for identifier in identifiers {
            let Some(source) = self.project.source(*identifier) else {
                continue;
            };
            if source.link.is_included() || to_include.contains(identifier) {
                continue;
            }
            if self.location_of_the_file_of(*identifier).is_some() {
                to_include.push(*identifier);
            } else {
                not_found.push(self.name_of_the_file_of(*identifier));
            }
        }
        (to_include, not_found)
    }

    fn show_message_about_files_that_are_not_found(&mut self, names: &[String]) {
        self.show_message(
            if names.len() == 1 {
                "The file cannot be included"
            } else {
                "The files cannot be included"
            },
            format!(
                "A file can only be included in the project file when it can be found and opened, and {} cannot:\n{}",
                if names.len() == 1 { "this one" } else { "these" },
                names.join("\n")
            ),
        );
    }

    /// The folder the project's file is in, as a path from the root.
    fn project_directory(&self) -> Option<PathBuf> {
        let folder = self.project_file_path.as_deref()?.parent()?;
        std::path::absolute(folder).ok()
    }

    /// Says in the project that the files of `identifiers` are included, as the entries that the project file will have them as.
    /// The places they were at are what they are now, which is what they are put back at: where each is found, and where that is
    /// from `project_directory` (the folder of the project file they are to be included in, if it is known).
    fn mark_as_included(&mut self, identifiers: &[SourceMediaIdentifier], project_directory: Option<&Path>) {
        for identifier in identifiers {
            let Some(location) = self.location_of_the_file_of(*identifier) else {
                continue;
            };
            let place = location.file().to_owned();
            let absolute_path = std::path::absolute(&place).unwrap_or(place);
            let relative_path = project_directory.and_then(|folder| relative_path_between(folder, &absolute_path));
            let file_name = absolute_path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            self.project.set_source_link(
                *identifier,
                SourceMediaLink {
                    absolute_path,
                    relative_path,
                    included_as: Some(resource_entry_name(*identifier, &file_name)),
                },
            );
        }
    }

    /// Asks where to save the project as an archive, and saves it there with the files `identifiers` included: if it cannot be saved
    /// there, nothing is included. Nothing happens if no place is chosen.
    pub(super) fn save_as_an_archive_with_files_included(&mut self, identifiers: &[SourceMediaIdentifier]) {
        self.pause_playback_for_dialog();
        let Some(path) = self.ask_where_to_save_an_archive("Save the project with the files included") else {
            return;
        };
        let project_before = self.project.clone();
        let project_directory = std::path::absolute(&path)
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf));
        self.mark_as_included(identifiers, project_directory.as_deref());
        self.save_the_project(path, Some(project_before));
    }

    /// Asks the user for a video, a sound or a picture and puts all of it on the timeline, at the current frame, and in the project
    /// file. The project file has to be an archive for that: if it is not one the user is asked first to save it as one, and
    /// only if they agree (and it is saved) is the file asked for (see [`Self::save_as_an_archive_and_ask_for_a_file_to_embed`]).
    pub(super) fn embed_and_import_file(&mut self) {
        self.pause_playback_for_dialog();
        if self.project_file_is_an_archive() {
            self.ask_for_a_file_to_embed();
        } else {
            self.dialog = Some(Dialog::MustSaveAsAnArchiveToEmbedAFile {
                has_a_plain_project_file: self.project_file_path.is_some(),
            });
        }
    }

    fn ask_for_a_file_to_embed(&mut self) {
        if let Some(path) = self.ask_for_a_media_file("Choose a video, a sound or a picture to embed") {
            self.start_opening(OpeningPurpose::ImportNewSourceAndEmbedIt, vec![path]);
        }
    }

    /// Asks where to save the project as an archive (with nothing included yet, so that it is written at once), and then asks for
    /// the file to embed in it. Nothing more happens if no place is chosen, or the project cannot be saved there (and the user is told
    /// why).
    pub(super) fn save_as_an_archive_and_ask_for_a_file_to_embed(&mut self) {
        self.pause_playback_for_dialog();
        let Some(path) = self.ask_where_to_save_an_archive("Save the project to embed a file in it") else {
            return;
        };
        if self.save_the_project(path, None) && self.project_file_is_an_archive() && self.running_save.is_none() {
            self.ask_for_a_file_to_embed();
        }
    }

    /// Asks where to save the project as an archive: only the kind of file that is one is offered, and it has the ending of one
    /// whatever name is chosen.
    fn ask_where_to_save_an_archive(&self, title: &str) -> Option<PathBuf> {
        let suggested_file_name = self.suggested_name_for_a_project_file(ARCHIVE_FILE_EXTENSION);
        let path = self.file_chooser.choose_where_to_save(&FileQuestion {
            title,
            suggested_file_name: Some(&suggested_file_name),
            starting_directory: self.project_file_path.as_deref().and_then(Path::parent),
            kinds: &[FileKind {
                name: "Project files (.ffrs)",
                extensions: &[ARCHIVE_FILE_EXTENSION],
            }],
        })?;
        Some(if is_the_name_of_an_archive(&path) {
            path
        } else {
            // (A name that ends another way is not one that can have files included, however it was chosen.)
            let mut name = path.into_os_string();
            name.push(format!(".{ARCHIVE_FILE_EXTENSION}"));
            PathBuf::from(name)
        })
    }

    /// Takes the files of the source files `identifiers` out of the project file, and puts each where it was when it was included,
    /// as far as that can be done (see the beginning of this file). The user is told of the files that could not be.
    pub(super) fn take_files_out_of_the_project_file(&mut self, identifiers: &[SourceMediaIdentifier]) {
        let project_directory = self.project_directory();
        let mut problems = Vec::new();
        let mut took_out_any = false;
        let mut done = Vec::new();
        for identifier in identifiers {
            if done.contains(identifier) {
                continue;
            }
            done.push(*identifier);
            let Some(link) = self.project.source(*identifier).map(|source| source.link.clone()) else {
                continue;
            };
            if !link.is_included() {
                continue;
            }
            let name = self.name_of_the_file_of(*identifier);
            let Some(bytes_at) = self.location_of_the_file_of(*identifier) else {
                problems.push(format!("{name}: it cannot be found or opened just now"));
                continue;
            };
            match put_back(&bytes_at, &places_it_was_at(&link, project_directory.as_deref())) {
                Ok(place) => {
                    let relative_path = project_directory
                        .as_deref()
                        .and_then(|folder| relative_path_between(folder, &place))
                        .or(link.relative_path);
                    self.project.set_source_link(
                        *identifier,
                        SourceMediaLink {
                            absolute_path: place.clone(),
                            relative_path,
                            included_as: None,
                        },
                    );
                    // (It is the same file, wherever it is: what was made from it stays.)
                    if let Some(state) = self.source_files.get_mut(identifier) {
                        state.status = SourceStatus::Available {
                            location: MediaLocation::of_a_file(place),
                        };
                    }
                    took_out_any = true;
                }
                Err(problem) => problems.push(format!("{name}: {problem}")),
            }
        }
        if took_out_any {
            self.note_unsaved_change();
        }
        if !problems.is_empty() {
            self.show_message(
                if problems.len() == 1 {
                    "The file could not be taken out of the project file"
                } else {
                    "Some files could not be taken out of the project file"
                },
                problems.join("\n"),
            );
        }
    }
}

/// The places that a file that is included was at, in the order to try them: where it was, and then where the same place is from
/// the folder that the project file is in now (if it is not where it was, because the project was moved).
fn places_it_was_at(link: &SourceMediaLink, project_directory: Option<&Path>) -> Vec<PathBuf> {
    let mut places = vec![link.absolute_path.clone()];
    if let (Some(relative_path), Some(folder)) = (&link.relative_path, project_directory) {
        let resolved = folder.join(relative_path);
        if resolved != link.absolute_path {
            places.push(resolved);
        }
    }
    places
}

/// Puts the file at `bytes_at` (a file, or a stretch of one: where it is in the archive) at the first of `places` that it can be, and
/// says which. Where a file is that has just what `bytes_at` has in it, that is where it is (nothing is copied); where there is another
/// file, or anything else, it is left alone, and that place is not used; and where there is nothing, the file is put there, if the
/// folder it is to be in is there.
fn put_back(bytes_at: impl Into<MediaLocation>, places: &[PathBuf]) -> Result<PathBuf, String> {
    let bytes_at = bytes_at.into();
    for place in places {
        if place.is_file() && have_the_same_contents(&bytes_at, place.as_path()).unwrap_or(false) {
            return Ok(place.clone());
        }
    }
    for place in places {
        let folder_is_there = place
            .parent()
            .is_some_and(|folder| folder.as_os_str().is_empty() || folder.is_dir());
        if fs::symlink_metadata(place).is_ok() || !folder_is_there {
            continue;
        }
        let copied = replace_file_atomically(place, |file| copy_bytes_of(&bytes_at, file).map(|_| ()));
        match copied {
            Ok(()) => return Ok(place.clone()),
            Err(error) => return Err(format!("it could not be put back at {}: {error}", place.display())),
        }
    }
    Err(format!(
        "there is no place to put it back at: {}",
        places
            .iter()
            .map(|place| format!(
                "{} is taken by something else, or its folder is not there",
                place.display()
            ))
            .collect::<Vec<_>>()
            .join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(absolute_path: &Path, relative_path: Option<&str>) -> SourceMediaLink {
        SourceMediaLink {
            absolute_path: absolute_path.to_owned(),
            relative_path: relative_path.map(PathBuf::from),
            included_as: Some("resources/1-v.mp4".to_owned()),
        }
    }

    #[test]
    fn the_places_a_file_was_at_are_where_it_was_and_then_that_place_from_the_folder_the_project_is_in() {
        let was = link(Path::new("/old/media/v.mp4"), Some("media/v.mp4"));

        assert_eq!(
            places_it_was_at(&was, Some(Path::new("/new/project"))),
            vec![
                PathBuf::from("/old/media/v.mp4"),
                PathBuf::from("/new/project/media/v.mp4")
            ]
        );
        assert_eq!(
            places_it_was_at(&was, None),
            vec![PathBuf::from("/old/media/v.mp4")],
            "without the folder of the project there is only the first"
        );
        assert_eq!(
            places_it_was_at(
                &link(Path::new("/old/media/v.mp4"), None),
                Some(Path::new("/new/project"))
            ),
            vec![PathBuf::from("/old/media/v.mp4")],
            "and with no relative place there is only the first too"
        );
        assert_eq!(
            places_it_was_at(
                &link(Path::new("/p/media/v.mp4"), Some("media/v.mp4")),
                Some(Path::new("/p"))
            ),
            vec![PathBuf::from("/p/media/v.mp4")],
            "two names for one place are one place"
        );
    }

    #[test]
    fn a_file_is_put_back_where_it_was_when_nothing_is_there() {
        let folder = tempfile::tempdir().unwrap();
        let bytes_at = folder.path().join("in the cache");
        fs::write(&bytes_at, b"the video").unwrap();
        let place = folder.path().join("v.mp4");

        let put_at = put_back(&bytes_at, std::slice::from_ref(&place)).unwrap();

        assert_eq!(put_at, place);
        assert_eq!(fs::read(&place).unwrap(), b"the video");
        assert_eq!(
            fs::read(&bytes_at).unwrap(),
            b"the video",
            "the one it was copied from stays"
        );
    }

    #[test]
    fn where_the_same_file_is_already_nothing_is_copied_and_that_is_where_it_is() {
        let folder = tempfile::tempdir().unwrap();
        let bytes_at = folder.path().join("in the cache");
        fs::write(&bytes_at, b"the video").unwrap();
        let place = folder.path().join("v.mp4");
        fs::write(&place, b"the video").unwrap();
        let modified = fs::metadata(&place).unwrap().modified().unwrap();

        assert_eq!(put_back(&bytes_at, std::slice::from_ref(&place)).unwrap(), place);

        assert_eq!(
            fs::metadata(&place).unwrap().modified().unwrap(),
            modified,
            "it was not written"
        );
    }

    #[test]
    fn a_file_that_is_where_it_is_to_go_is_the_same_file_and_so_is_not_copied_over_itself() {
        let folder = tempfile::tempdir().unwrap();
        let place = folder.path().join("v.mp4");
        fs::write(&place, b"the video").unwrap();

        assert_eq!(put_back(&place, std::slice::from_ref(&place)).unwrap(), place);
        assert_eq!(fs::read(&place).unwrap(), b"the video");
    }

    #[test]
    fn where_another_file_is_it_is_left_alone_and_the_next_place_is_used() {
        let folder = tempfile::tempdir().unwrap();
        let bytes_at = folder.path().join("in the cache");
        fs::write(&bytes_at, b"the video").unwrap();
        let taken = folder.path().join("v.mp4");
        fs::write(&taken, b"something else that was put there").unwrap();
        let free = folder.path().join("elsewhere.mp4");

        let put_at = put_back(&bytes_at, &[taken.clone(), free.clone()]).unwrap();

        assert_eq!(put_at, free);
        assert_eq!(fs::read(&taken).unwrap(), b"something else that was put there");
        assert_eq!(fs::read(&free).unwrap(), b"the video");
    }

    #[test]
    fn the_same_file_at_a_later_place_is_used_before_a_free_place_that_comes_earlier() {
        let folder = tempfile::tempdir().unwrap();
        let bytes_at = folder.path().join("in the cache");
        fs::write(&bytes_at, b"the video").unwrap();
        let free_but_first = folder.path().join("first.mp4");
        let has_it = folder.path().join("second.mp4");
        fs::write(&has_it, b"the video").unwrap();

        let put_at = put_back(&bytes_at, &[free_but_first.clone(), has_it.clone()]).unwrap();

        assert_eq!(put_at, has_it, "it is where it is, and no second copy is made");
        assert!(!free_but_first.exists());
    }

    #[test]
    fn a_place_whose_folder_is_not_there_is_not_used_and_no_folder_is_made_for_it() {
        let folder = tempfile::tempdir().unwrap();
        let bytes_at = folder.path().join("in the cache");
        fs::write(&bytes_at, b"the video").unwrap();
        let no_folder = folder.path().join("gone").join("v.mp4");
        let free = folder.path().join("v.mp4");

        let put_at = put_back(&bytes_at, &[no_folder.clone(), free.clone()]).unwrap();

        assert_eq!(put_at, free);
        assert!(!folder.path().join("gone").exists());
    }

    #[test]
    fn with_no_place_to_use_it_says_so_and_changes_nothing() {
        let folder = tempfile::tempdir().unwrap();
        let bytes_at = folder.path().join("in the cache");
        fs::write(&bytes_at, b"the video").unwrap();
        let taken = folder.path().join("v.mp4");
        fs::write(&taken, b"other").unwrap();
        let no_folder = folder.path().join("gone").join("v.mp4");

        let problem = put_back(&bytes_at, &[taken.clone(), no_folder]).unwrap_err();

        assert!(problem.contains("no place to put it back"), "{problem}");
        assert!(problem.contains("v.mp4"), "{problem}");
        assert_eq!(fs::read(&taken).unwrap(), b"other");
        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 2, "nothing was made");
    }

    #[test]
    fn a_place_that_is_a_folder_is_not_a_file_to_put_the_file_in_the_place_of() {
        let folder = tempfile::tempdir().unwrap();
        let bytes_at = folder.path().join("in the cache");
        fs::write(&bytes_at, b"the video").unwrap();
        let folder_in_the_way = folder.path().join("v.mp4");
        fs::create_dir(&folder_in_the_way).unwrap();

        assert!(put_back(&bytes_at, std::slice::from_ref(&folder_in_the_way)).is_err());
        assert!(folder_in_the_way.is_dir());
    }
}
