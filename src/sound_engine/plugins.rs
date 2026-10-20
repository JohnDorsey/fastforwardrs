// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The effects of clips that are plugins (phase 7, piece 7.6): what the graph asks for them ([`PluginRunner`]), and the [`Insertions`] that ask.
//!
//! **What runs a plugin is not here** (the graph has no `unsafe`, no library and no question to a person): a [`PluginRunner`] is given the effect (its kind, its values and the
//! keyframes of them) and the clip it is on, and gives back what changes the sound of the clip a block at a time, or nothing, which is **the sound passing through unchanged**
//! (the plugin is not installed, nobody has said that it may be run, it could not be made, it is not the plugin that was scanned...). The editor's runner is `plugin_host`.
//!
//! **A track's effects are applied to the sound of each of its clips**: the plan puts the effects of the track in the chain of every clip of it where its badge is (see
//! `composition::sound`), so what is applied after the clips are put together in the design (G9) is applied to each clip before it is. For an effect with no memory that is the
//! same sound; for one that has (a filter, a reverb) a clip ends where the effect's memory of it ends, and the next clip begins with the effect as it was made. (A strip that
//! has its own effects, [`Insertions::for_strip`], is not used yet: a track that is a group of tracks is where it will be.)

use std::sync::{Arc, Mutex};

use super::feed::ClipRequest;
use super::insertions::{BlockProcessor, Insertions};
use crate::composition::LayerEffect;
use crate::media::export::SoundClipToExport;
use crate::project::FrameRate;

/// What the runner says to be told of: lines for the person, which the stream says when it ends (a plugin that was too slow and was left out, a plugin that could not be made).
pub type Notes = Arc<Mutex<Vec<String>>>;

/// What makes the sound of a clip pass through a plugin.
pub trait PluginRunner: Send + Sync {
    /// What does the plugin effect `effect` to the sound of `clip` (whose frames are counted from `frame_rate`), or `None` if the sound is to pass through unchanged. `notes`
    /// is where to say why, if a person should be told. `is_for_the_preview` says that a plugin that is too slow to keep up is to be left out (the export waits for it).
    fn processor_for(
        &self,
        effect: &LayerEffect,
        clip: &SoundClipToExport,
        frame_rate: FrameRate,
        notes: &Notes,
        is_for_the_preview: bool,
    ) -> Option<Box<dyn BlockProcessor>>;

    /// Said when the stream whose effects were asked for with `notes` is being stopped (a seek, a stop): its effects are about to be let go of. A runner that cannot have
    /// plugins of the same library alive at once, one made while another is being let go of, waits for them (see `PluginHost`).
    fn stream_is_stopping(&self, _notes: &Notes) {}

    /// A number that is different whenever what this would do for the same effect has changed (a folder was approved or declined): the preview makes the sound again when it
    /// is, which is how a plugin begins to be heard when a person has said yes. A runner that never changes its mind says 0.
    fn generation(&self) -> u64 {
        0
    }
}

/// The insertions of the graph that are the plugin effects of the clips, made by a [`PluginRunner`]. An effect that is not a plugin (or that the runner says nothing for) is not
/// an insertion: the sound goes through it as it is.
pub struct PluginInsertions {
    runner: Arc<dyn PluginRunner>,
    frame_rate: FrameRate,
    notes: Notes,
    is_for_the_preview: bool,
}

impl PluginInsertions {
    pub fn new(runner: Arc<dyn PluginRunner>, frame_rate: FrameRate, notes: Notes, is_for_the_preview: bool) -> Self {
        Self {
            runner,
            frame_rate,
            notes,
            is_for_the_preview,
        }
    }
}

impl Insertions for PluginInsertions {
    fn for_clip(&mut self, request: &ClipRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        request
            .clip
            .effects
            .iter()
            .filter(|effect| effect.kind.installed_plugin().is_some())
            .filter_map(|effect| {
                self.runner.processor_for(
                    effect,
                    request.clip,
                    self.frame_rate,
                    &self.notes,
                    self.is_for_the_preview,
                )
            })
            .collect()
    }

    fn for_strip(&mut self, _strip: usize) -> Vec<Box<dyn BlockProcessor>> {
        Vec::new()
    }
}
