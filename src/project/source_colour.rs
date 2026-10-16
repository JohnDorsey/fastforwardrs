// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the user says the colours of a source file are, where the program would otherwise read them from the file's tags or assume them (phase 3, decision C5 of
//! `docs/engine-phase-3-design.md`). Every part is optional, and a part that is not there is **automatic**: what the file says, or what is assumed where it says
//! nothing (see `crate::media::source_colour`).

use super::{Project, SourceMediaIdentifier, SourceMediaKind};
use crate::colour_space::{StandardPrimaries, TransferFunction, VideoMatrix, VideoRange};

/// What is said, for one source file, about the colours of its picture instead of what the file says. A video uses all four parts; a picture file (which has no matrix and
/// no range) uses the primaries and the curve.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputColourOverride {
    /// The matrix that makes red, green and blue of the brightness and the colour differences of a video.
    pub matrix: Option<VideoMatrix>,
    /// The range that the numbers of a video use.
    pub range: Option<VideoRange>,
    /// The primaries of the colours.
    pub primaries: Option<StandardPrimaries>,
    /// The curve that the codes of red, green and blue are made light with.
    pub curve: Option<TransferFunction>,
}

impl InputColourOverride {
    /// Nothing is said: every part is automatic.
    pub const AUTOMATIC: Self = Self {
        matrix: None,
        range: None,
        primaries: None,
        curve: None,
    };

    /// Whether nothing is said (so that nothing about it is saved in a project file).
    pub fn is_automatic(&self) -> bool {
        *self == Self::AUTOMATIC
    }
}

impl Project {
    /// What is said of the colours of the file `identifier` instead of what it says (`None` if there is no such source file).
    pub fn source_colour(&self, identifier: SourceMediaIdentifier) -> Option<InputColourOverride> {
        self.source(identifier).map(|source| source.colour)
    }

    /// Says what the colours of the file `identifier` are (all automatic, to say nothing). `false` if there is no such source file, or it is a sound, which has no
    /// colours.
    pub fn set_source_colour(&mut self, identifier: SourceMediaIdentifier, colour: InputColourOverride) -> bool {
        match self
            .sources
            .iter_mut()
            .find(|source| source.identifier == identifier && source.kind() != SourceMediaKind::Audio)
        {
            Some(source) => {
                source.colour = colour;
                true
            }
            None => false,
        }
    }
}
