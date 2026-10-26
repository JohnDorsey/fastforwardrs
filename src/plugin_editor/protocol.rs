// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the editor and the editor-window program say to each other (phase 10, P4 of `docs/engine-phase-10-editors-design.md`): lines of JSON, one message on each.
//!
//! **Nothing in it names a kind of plugin**: a window is for a library, an id and a set of values; VST3 will be a second kind in the program and not a second protocol. The
//! program says its lines on its standard output with a fixed word first, because a plugin library is free to print to it too (the others are noise and are thrown away, as the scan
//! does); the editor says its lines on the program's standard input, which nothing else writes to.

use serde::{Deserialize, Serialize};

pub use clap_host::ParameterEdit;

/// What every line that the program says begins with.
pub const LINE_PREFIX: &str = "ffrs-editor: ";

/// What the editor says to the program.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ToProgram {
    /// **The first thing said**: the values that the project says the parameters have (`(id, value)`), and the plugin's state if the project has one. The window is made after this,
    /// so that it shows what the project says.
    Begin {
        values: Vec<(u32, f64)>,
        state: Option<String>,
    },
    /// The project says that these parameters have these values now (a change in the panel, an undo, keyframes while the project plays).
    Values { values: Vec<(u32, f64)> },
    /// The state of the plugin is this (base 64): the project has another now (an undo).
    LoadState { state: String },
    /// Say the plugin's state (answered with [`FromProgram::State`]).
    SaveState,
    /// Close the window and end.
    Close,
}

/// What the program says to the editor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FromProgram {
    /// The window is up. `embedded` says that it is in a window that the program made (and not a window of the plugin's own); the size is what the plugin said.
    Ready {
        width: u32,
        height: u32,
        can_resize: bool,
        embedded: bool,
        has_state: bool,
    },
    /// What a person does in the window: a step of a gesture on a parameter.
    Edit { edit: ParameterEdit },
    /// The plugin's state is not what was last said (ask for it with [`ToProgram::SaveState`]).
    StateChanged,
    /// The state of the plugin, saved (base 64): the answer to [`ToProgram::SaveState`].
    State { state: String },
    /// The plugin's parameters are not what they were (the editor makes its rows again).
    ParametersChanged,
    /// The window is closed, by a person or by the plugin: the program ends.
    Closed,
    /// The window could not be made (or the plugin could not): why. The program ends.
    Failed { reason: String },
}

/// A message as the line that carries it (no new line at its end).
pub fn line_of<T: Serialize>(message: &T) -> String {
    format!(
        "{LINE_PREFIX}{}",
        serde_json::to_string(message).expect("a message can be written")
    )
}

/// The message that a line says, if it is one of ours and a good one. (A line that is not, a plugin's noise, is `None`.)
pub fn message_in<T: for<'a> Deserialize<'a>>(line: &str) -> Option<T> {
    serde_json::from_str(line.strip_prefix(LINE_PREFIX)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_comes_back_as_it_was_said_and_a_line_is_one_of_ours_by_its_first_word() {
        let said = FromProgram::Edit {
            edit: ParameterEdit::Value { id: 7, value: 0.25 },
        };

        let line = line_of(&said);

        assert!(line.starts_with("ffrs-editor: "), "{line}");
        assert!(!line.contains('\n'));
        assert_eq!(message_in::<FromProgram>(&line), Some(said));
        let to = ToProgram::Begin {
            values: vec![(1, 2.0)],
            state: Some("AAAA".to_owned()),
        };
        assert_eq!(message_in::<ToProgram>(&format!("{}\r\n", line_of(&to))), Some(to));
    }

    #[test]
    fn what_a_plugin_prints_and_what_is_not_a_message_is_noise() {
        assert_eq!(message_in::<FromProgram>("hello from a plugin"), None);
        assert_eq!(message_in::<FromProgram>(""), None);
        assert_eq!(message_in::<FromProgram>("ffrs-editor: {not json"), None);
        assert_eq!(message_in::<FromProgram>("ffrs-editor: \"NotAMessage\""), None);
        // (A message that is good but is not preceded by the first word is noise too: a plugin may print anything.)
        assert_eq!(message_in::<FromProgram>("\"Closed\""), None);
        assert_eq!(message_in::<ToProgram>("\"Close\""), None);
        // (A line of the editor's kind is not read as one of the program's.)
        assert_eq!(message_in::<FromProgram>(&line_of(&ToProgram::Close)), None);
    }
}
