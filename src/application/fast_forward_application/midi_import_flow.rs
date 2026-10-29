// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Importing a MIDI file from the window (phase 13, piece 13.6; docs/engine-phase-13-midi-design.md, question 4). The two items of the File menu that import a video, a sound or a picture take a file
//! whose name ends in `.mid` or `.midi` too: it is **read by the program at once, with no ffmpeg**, kept as a resource of the project (linked, or included in the project file as the item says), and each
//! of its tracks that has notes becomes a MIDI clip on a track of its own at the current frame. All of it is one step of undo. A file that is not music, or has no notes, is refused with the reason, and
//! the project is left as it was.

use std::path::Path;
use std::time::Instant;

use super::FastForwardApplication;
use super::source_media_flow::MIDI_FILE_EXTENSIONS;
use crate::project_file::{link_of_a_new_resource, read_a_resource_file};

/// What the message about a file that cannot be used says.
pub const NOT_A_MIDI_FILE_TITLE: &str = "This is not a MIDI file that can be used";
pub const NOT_A_FILE_TITLE: &str = "This file cannot be used";
pub const LINKED_BECAUSE_THE_PROJECT_FILE_IS_NOT_AN_ARCHIVE: &str = "The MIDI file is linked and not included: the project file is not a .ffrs file, which is the kind that can hold it. Save the project as a .ffrs file (File, Save project as) and import the file again to keep it inside the project file.";

/// Whether the name of the file says that it is a MIDI file.
pub(super) fn is_a_midi_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        MIDI_FILE_EXTENSIONS
            .iter()
            .any(|midi| extension.eq_ignore_ascii_case(midi))
    })
}

impl FastForwardApplication {
    /// Reads the MIDI file at `path` and puts its tracks on the timeline at the current frame, as one step of undo. It is included in the project file if `include_it` and the project file is one that
    /// can hold it, and linked otherwise. The user is told why not if it cannot be done.
    pub(super) fn import_a_midi_file_from(&mut self, path: &Path, include_it: bool) {
        let name = path
            .file_name()
            .map_or_else(|| "music.mid".to_owned(), |name| name.to_string_lossy().into_owned());
        let bytes = match read_a_resource_file(path) {
            Ok(bytes) => bytes,
            Err(problem) => {
                self.show_message(NOT_A_FILE_TITLE, format!("{name}: {problem}"));
                return;
            }
        };
        let is_included = include_it && self.project_file_is_an_archive();
        let link = link_of_a_new_resource(path, &name, &bytes, is_included);
        let now = Instant::now();
        let total_frames = self.document.project.total_frame_count();
        let current_frame = self.playback.clock.position(now, total_frames);
        let project_before = self.document.project.clone();
        match self
            .document
            .project
            .import_a_midi_file(&name, bytes, link, current_frame)
        {
            Ok(_) => {
                // Importing is an edit like any other: undoing it takes the clips, their tracks and the file away again.
                self.document.history.record_change(
                    project_before,
                    self.interface.selection.to_items(),
                    None,
                    now,
                    false,
                );
                self.note_the_project_was_edited(now);
                if include_it && !is_included {
                    self.show_message(
                        "The MIDI file is linked",
                        LINKED_BECAUSE_THE_PROJECT_FILE_IS_NOT_AN_ARCHIVE,
                    );
                }
            }
            Err(error) => self.show_message(NOT_A_MIDI_FILE_TITLE, format!("{name}: {error}")),
        }
    }
}
