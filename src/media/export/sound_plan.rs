// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the graph of the sound of a project is made from, worked out from the sound plan: for the finished video, and for the sound that the live preview plays.

use std::collections::BTreeSet;

use super::filter_graph::{
    LoudnessToExport, SoundBusToExport, SoundClipToExport, SoundGraphSpecification, SoundItemToExport,
    SoundStripToExport,
};
use crate::composition::{Loudness, SoundItem, SoundPlan};
use crate::project::{SourceMediaIdentifier, SpedUpClipSound};

/// The files that the sound of `plan` is made from.
pub fn sources_of_sound(plan: &SoundPlan) -> BTreeSet<SourceMediaIdentifier> {
    plan.strips
        .iter()
        .flat_map(|strip| &strip.items)
        .filter_map(|item| match item {
            SoundItem::Clip(clip) => Some(clip.source),
            SoundItem::Silence { .. } => None,
        })
        .collect()
}

/// What the graph of the sound of `plan` is made from, where `source_files` are the files that
/// are the graph's inputs 0, 1, ... in this order (every file the sound is made from must be one
/// of them). The export and the live preview make their sound from this.
pub fn sound_graph_specification(
    plan: &SoundPlan,
    source_files: &[SourceMediaIdentifier],
    sped_up_clip_sound: SpedUpClipSound,
) -> SoundGraphSpecification {
    let input_of_source = |identifier: SourceMediaIdentifier| {
        source_files
            .iter()
            .position(|candidate| *candidate == identifier)
            .expect("every file the sound is made from is one of the inputs")
    };
    SoundGraphSpecification {
        frame_rate: plan.frame_rate,
        total_frame_count: plan.total_frame_count,
        strips: plan
            .strips
            .iter()
            .map(|strip| SoundStripToExport {
                items: strip
                    .items
                    .iter()
                    .map(|item| match item {
                        SoundItem::Silence { frame_count } => SoundItemToExport::Silence {
                            frame_count: *frame_count,
                        },
                        SoundItem::Clip(clip) => SoundItemToExport::Clip(SoundClipToExport {
                            source_input: input_of_source(clip.source),
                            first_source_frame: clip.first_source_frame,
                            end_source_frame: clip.end_source_frame,
                            speed_factor: clip.speed,
                            timeline_frame_count: clip.timeline_frame_count,
                            loudness: match &clip.loudness {
                                Loudness::Constant(factor) => LoudnessToExport::Constant(*factor),
                                Loudness::Changing(points) => LoudnessToExport::Changing(
                                    points.iter().map(|point| (point.frame, point.decibels)).collect(),
                                ),
                            },
                            first_timeline_frame: clip.first_timeline_frame,
                            frames_of_the_clip_before: clip.frames_of_the_clip_before,
                            effects: clip.effects.clone(),
                        }),
                    })
                    .collect(),
                buses: strip
                    .buses
                    .iter()
                    .map(|bus| SoundBusToExport {
                        first_item: bus.first_item,
                        last_item: bus.last_item,
                        effects: bus.effects.clone(),
                    })
                    .collect(),
            })
            .collect(),
        sped_up_clip_sound,
    }
}

#[cfg(test)]
mod tests;
