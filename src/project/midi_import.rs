// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Putting a Standard MIDI File in the project (phase 13, piece 13.6; docs/engine-phase-13-midi-design.md, question 4): the file becomes a resource, and **each track of it that has notes becomes a
//! MIDI clip of all of it, on a track of its own**, from the frame it is put at. A track that has only the tempo, or only controllers, has nothing to play and is left out.

use std::sync::Arc;

use thiserror::Error;

use super::{
    ClipId, MidiClip, Project, ResourceProblem, SourceFrameIndex, SourceMediaLink, SpeedFactor, TimelineFrameIndex,
    TrackId,
};
use crate::midi_file::{MidiFileProblem, parse_midi_file};
use crate::project::ResourceKind;

/// What a file that was put in the project made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MidiImport {
    /// The name the file has among the resources of the project (the wanted one, unless the file was already there under another).
    pub resource: String,
    /// The clips, in the order of the tracks of the file that they play.
    pub clips: Vec<ClipId>,
}

/// Why a file could not be put in the project.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum MidiImportError {
    #[error("it is not a MIDI file that can be read: {0}")]
    NotMusic(MidiFileProblem),
    #[error("it has no notes to play")]
    NoNotes,
    #[error("it cannot be kept in the project: {0}")]
    Resource(ResourceProblem),
    #[error("there is no room for the clips")]
    NoRoom,
}

impl Project {
    /// Puts the MIDI file `bytes` in the project as a resource called `wanted_name` (kept as `link` says) and makes a clip of each of its tracks that has notes, from `at_frame`: the last on the front-most
    /// track that has nothing on it, the others on tracks of their own, new ones in front, so that they lie one above the other in the order of the file. **All or nothing**: if it cannot be done the project is as it was.
    pub fn import_a_midi_file(
        &mut self,
        wanted_name: &str,
        bytes: Arc<[u8]>,
        link: SourceMediaLink,
        at_frame: TimelineFrameIndex,
    ) -> Result<MidiImport, MidiImportError> {
        let file = parse_midi_file(&bytes).map_err(MidiImportError::NotMusic)?;
        let tracks_to_play: Vec<usize> = file
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, track)| !track.notes.is_empty())
            .map(|(place, _)| place)
            .collect();
        if tracks_to_play.is_empty() {
            return Err(MidiImportError::NoNotes);
        }
        let mut attempt = self.clone();
        let resource = attempt
            .add_resource(wanted_name, ResourceKind::Midi, bytes, link)
            .map_err(MidiImportError::Resource)?;
        let frame_rate = attempt.frame_rate();
        // (The length of the file in frames of the project, rounded up, and at least one.)
        let microseconds_times_the_rate = u128::from(file.length_us()) * u128::from(frame_rate.numerator());
        let frames_times_a_million = u128::from(frame_rate.denominator()) * 1_000_000;
        let frames = u64::try_from(microseconds_times_the_rate.div_ceil(frames_times_a_million))
            .unwrap_or(u64::MAX)
            .max(1);
        // (The last track of the file goes on the front-most track that has nothing on it, if there is one, and the others on new tracks in front, made from the last to the first: the tracks of the file are
        // one above the other in the order that the file has them.)
        let mut empty_track: Option<TrackId> = attempt
            .tracks()
            .iter()
            .find(|track| track.is_empty())
            .map(|track| track.id());
        let mut clips: Vec<Option<ClipId>> = vec![None; tracks_to_play.len()];
        for (number, place) in tracks_to_play.iter().copied().enumerate().rev() {
            let clip = MidiClip::new(
                resource.as_str(),
                u16::try_from(place).unwrap_or(u16::MAX),
                SourceFrameIndex::new(0),
                SourceFrameIndex::new(frames),
                SpeedFactor::NORMAL,
            )
            .map_err(|_| MidiImportError::NoRoom)?;
            let first_choice = if number + 1 == tracks_to_play.len() {
                empty_track.take()
            } else {
                None
            };
            let clip_id = match first_choice.map(|track| attempt.try_insert_midi_clip(track, at_frame, clip.clone())) {
                Some(Ok(clip_id)) => clip_id,
                // (No empty track, or one that cannot have it where groups are: a new track in front, which is in no group.)
                Some(Err(_)) | None => {
                    let track = attempt.add_track_in_front();
                    attempt
                        .try_insert_midi_clip(track, at_frame, clip)
                        .map_err(|_| MidiImportError::NoRoom)?
                }
            };
            clips[number] = Some(clip_id);
        }
        let clips: Vec<ClipId> = clips.into_iter().flatten().collect();
        *self = attempt;
        Ok(MidiImport { resource, clips })
    }
}

#[cfg(test)]
mod tests;
