// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The masks of the sections of a plan (`docs/shader-masks-design.md`): **a mask that is a track is the plan of that track on its own**, made again for the track whether or not its eye is
//! open, and put in the sections where an effect reads it. A section is cut where a mask changes (a clip of the track begins or ends), so that the mask is the same over the whole of every
//! section; a video that is a mask keeps its one range of source frames across the cuts, as a video of a layer does, so that its stream of pictures goes on.
//!
//! A mask can be made of masks (the effects of the track read masks too). A track that is being made is never made again inside itself: where a ring of tracks that read each other closes
//! (which an edit does not allow but a clip that is moved can make), the track that would be made again has nothing.

use std::collections::{BTreeSet, HashMap};

use super::{Layer, LayerContent, MaskPicture, PicturePlan, PictureSection, Scope, VisibleVideo, plan_pictures_of};
use crate::project::tracks::TrackId;
use crate::project::{MaskSource, MaskUse, Project, SourceMediaIdentifier, TimelineFrameIndex};

/// The masks that the effects of the layers and the groups of `section` read, each once, in the order they are first read in.
fn masks_read_by(section: &PictureSection) -> Vec<MaskUse> {
    let mut masks: Vec<MaskUse> = Vec::new();
    let effects = section
        .layers
        .iter()
        .flat_map(|layer| &layer.effects)
        .chain(section.groups.iter().flat_map(|group| &group.effects));
    for effect in effects {
        for used in effect.driven.uses() {
            if !masks.contains(&used) {
                masks.push(used);
            }
        }
    }
    masks
}

/// How many of the layers of `section`, from the back, are behind the source in the frame: those of the tracks that are after all the source's tracks in the project's list (the layers go
/// from the back to the front, and the tracks from the front to the back).
fn layers_behind(project: &Project, section: &PictureSection, source: MaskSource) -> usize {
    let Some(place) = project
        .tracks_of_the_source(source)
        .into_iter()
        .filter_map(|track| project.track_position(track))
        .max()
    else {
        return 0;
    };
    section
        .layers
        .iter()
        .take_while(|layer| project.track_position(layer.track).is_some_and(|other| other > place))
        .count()
}

/// The part `from..until` of `section` (a stretch inside it): the same layers and groups, each video showing the part of its range that the stretch is.
fn piece_of(section: &PictureSection, from: u64, until: u64) -> PictureSection {
    let offset = from - section.first_timeline_frame;
    let layers = section
        .layers
        .iter()
        .map(|layer| match &layer.content {
            LayerContent::Video(video) => {
                let speed = u64::from(video.speed.value());
                let first_source_frame = video.first_source_frame + offset * speed;
                Layer {
                    content: LayerContent::Video(VisibleVideo {
                        first_source_frame,
                        end_source_frame: (first_source_frame + (until - from) * speed).min(video.end_source_frame),
                        ..*video
                    }),
                    ..layer.clone()
                }
            }
            LayerContent::Image { .. } | LayerContent::Text(_) | LayerContent::Generator | LayerContent::Adjustment => {
                layer.clone()
            }
        })
        .collect();
    PictureSection {
        first_timeline_frame: from,
        end_timeline_frame: until,
        layers,
        groups: section.groups.clone(),
        masks: Vec::new(),
    }
}

/// `sections`, cut where the masks that their effects read change, each with those masks. `making` are the tracks whose pictures are being made, one inside another (see the top of
/// this file).
pub(super) fn with_the_masks_attached(
    project: &Project,
    is_usable: &dyn Fn(SourceMediaIdentifier) -> bool,
    sections: Vec<PictureSection>,
    making: &[TrackId],
) -> Vec<PictureSection> {
    // The picture of each source that is read as a mask, made once (whether or not the transitions in it see the real backdrop is not a matter of the plan: it is a matter of what the
    // layers behind are, and those are the section's).
    let mut plans: HashMap<MaskSource, PicturePlan> = HashMap::new();
    for section in &sections {
        for used in masks_read_by(section) {
            let tracks = project.tracks_of_the_source(used.source);
            if plans.contains_key(&used.source)
                || tracks.iter().any(|track| making.contains(track))
                || !project.a_source_is_there(used.source)
            {
                continue;
            }
            let inside: Vec<TrackId> = making.iter().copied().chain(tracks).collect();
            let scope = match used.source {
                MaskSource::Track(track) => Scope::OnlyTrack(track),
                MaskSource::ClipGroup(group) | MaskSource::TrackGroup(group) => Scope::OnlyGroup(group),
            };
            plans.insert(used.source, plan_pictures_of(project, is_usable, scope, &inside));
        }
    }
    if plans.is_empty() {
        return sections;
    }

    let mut cut_up = Vec::with_capacity(sections.len());
    for section in sections {
        let read: Vec<MaskUse> = masks_read_by(&section)
            .into_iter()
            .filter(|used| plans.contains_key(&used.source))
            .collect();
        if read.is_empty() {
            cut_up.push(section);
            continue;
        }
        let cuts: BTreeSet<u64> = read
            .iter()
            .flat_map(|used| &plans[&used.source].sections)
            .flat_map(|mask| [mask.first_timeline_frame, mask.end_timeline_frame])
            .filter(|frame| *frame > section.first_timeline_frame && *frame < section.end_timeline_frame)
            .collect();
        let points: Vec<u64> = std::iter::once(section.first_timeline_frame)
            .chain(cuts)
            .chain([section.end_timeline_frame])
            .collect();
        for pair in points.windows(2) {
            let (from, until) = (pair[0], pair[1]);
            let mut piece = if from == section.first_timeline_frame && until == section.end_timeline_frame {
                section.clone()
            } else {
                piece_of(&section, from, until)
            };
            piece.masks = read
                .iter()
                .filter_map(|used| {
                    plans[&used.source]
                        .section_at(TimelineFrameIndex::new(from))
                        .map(|mask| MaskPicture {
                            which: *used,
                            layers_behind: layers_behind(project, &section, used.source),
                            section: mask.clone(),
                        })
                })
                .collect();
            cut_up.push(piece);
        }
    }
    cut_up
}
