// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The racks of keyframes in a project file: the drawers of the tracks and of the clips, as the file has them, and what they are
//! made into when the file is loaded.
//!
//! A rack says which property its keyframes are of and, unless they are of the clip or the track whose drawer the rack is in,
//! which clip or track that is: by where it is in the file (the number of its track in the list of tracks and, for a clip, the
//! number of the clip in the list of its track's clips, both counted from 0), because the files do not have the names that
//! clips and tracks are known by when a project is edited. (Files of versions before 7 had an `animation` object in each clip and
//! track instead, which is the same as a rack of the keyframes of the clip's or the track's own properties, in its own drawer.)

use std::collections::HashMap;

use super::document::{AnimationDocument, KeyframeDocument, MemberDocument, RackDocument, TargetDocument};
use super::legacy_effects;
use super::value_from_document;
use crate::project::{
    AnimationTarget, ClipId, ClipKind, Drawer, EffectInstanceId, EffectKind, Effects, GroupId, Member, PropertyId,
    Rack, RackId, Track, TrackId,
};

/// What is known of what a file has, which is what the racks it has can be checked against.
pub(super) struct FileContents {
    /// The kind of each clip of each track, in the order they are in the file.
    pub kinds_of_the_clips: Vec<Vec<ClipKind>>,
    /// The effects of each clip of each track, in the same order, and those of each track.
    pub effects_of_the_clips: Vec<Vec<Effects>>,
    pub effects_of_the_tracks: Vec<Effects>,
    /// The effect that each clip of each track is made of, if it is made of one (a generator clip, and a text clip, whether it was read as such or made one of by the migration of a file
    /// of format 17 or earlier).
    pub contents_of_the_clips: Vec<Vec<Option<EffectInstanceId>>>,
}

impl FileContents {
    /// The clip or track that `target` says, if the file has it.
    fn target_from(&self, target: TargetDocument) -> Result<AnimationTarget, String> {
        let clips_of_the_track = self.kinds_of_the_clips.get(target.track).ok_or_else(|| {
            format!(
                "there is no track {} in the file to be the target of keyframes",
                target.track
            )
        })?;
        let track = TrackId::new(u32::try_from(target.track).unwrap_or(u32::MAX));
        match target.clip {
            None => Ok(AnimationTarget::Track(track)),
            Some(clip) if clip < clips_of_the_track.len() => Ok(AnimationTarget::Clip(ClipId::new(
                track,
                u32::try_from(clip).unwrap_or(u32::MAX),
            ))),
            Some(clip) => Err(format!(
                "there is no clip {clip} on track {} to be the target of keyframes",
                target.track
            )),
        }
    }

    /// The effects that `target` (which the file has) has.
    fn effects_of(&self, target: AnimationTarget) -> &Effects {
        match target {
            AnimationTarget::Track(track) => &self.effects_of_the_tracks[track.value() as usize],
            AnimationTarget::Clip(clip) => {
                &self.effects_of_the_clips[clip.origin().value() as usize][clip.number() as usize]
            }
        }
    }

    /// The property of `target` (which the file has) that is saved under `name`: if `effect` is `None`, one that a clip or a track has of its
    /// own, or (in a file that was written before the effects were open, which does not say which effect: there was at most one of each kind) the
    /// value of an effect that it has that the old files called `name`; and if the effect is named, the value of that effect, which `name` is
    /// the id of (or what the old files called it).
    fn property_named(
        &self,
        name: &str,
        effect: Option<EffectInstanceId>,
        target: AnimationTarget,
    ) -> Option<PropertyId> {
        let effects = self.effects_of(target);
        match effect {
            None => PropertyId::from_name(name)
                .or_else(|| self.value_of_the_text_that_a_file_of_an_earlier_version_calls(name, target))
                .or_else(|| {
                    effects
                        .iter()
                        .find_map(|effect| legacy_effects::the_value_of_the_effect_called(effect, name))
                }),
            Some(instance) => {
                let effect = effects.instance(instance)?;
                effect
                    .property_with_id(name)
                    .or_else(|| legacy_effects::the_value_of_the_effect_called(effect, name))
            }
        }
    }

    /// The value of the text effect of `target`, if it is a text clip, that a file of format 17 or earlier calls `name` (what a text says, its font, its size and its colour were properties
    /// of the clip itself then, `"text"`, `"font"`, `"font size"` and `"colour"`; they are the values of its text effect since, and a rack of them says which effect).
    fn value_of_the_text_that_a_file_of_an_earlier_version_calls(
        &self,
        name: &str,
        target: AnimationTarget,
    ) -> Option<PropertyId> {
        let AnimationTarget::Clip(clip) = target else {
            return None;
        };
        let (track, number) = (clip.origin().value() as usize, clip.number() as usize);
        if self.kinds_of_the_clips[track][number] != ClipKind::Text {
            return None;
        }
        let content = self.contents_of_the_clips[track][number]?;
        let slot = match name {
            "text" => 0,
            "font" => 1,
            "font size" => 2,
            "colour" => 3,
            _ => return None,
        };
        Some(PropertyId::effect_value(EffectKind::TEXT, content, slot))
    }

    /// Whether `target` (which the file has) has `property`: a value of an effect only if it has the effect.
    fn has_the_property(&self, target: AnimationTarget, property: PropertyId) -> bool {
        match target {
            AnimationTarget::Track(track) => {
                property.is_had_by_a_track(&self.effects_of_the_tracks[track.value() as usize])
            }
            AnimationTarget::Clip(clip) => {
                let (track, number) = (clip.origin().value() as usize, clip.number() as usize);
                property.is_had_by_a_clip(
                    self.kinds_of_the_clips[track][number],
                    &self.effects_of_the_clips[track][number],
                )
            }
        }
    }
}

/// The drawer that the racks `racks` (and the `legacy` animation of a file of an earlier version) make, which is the drawer of
/// `owner`, a clip or a track of the track number `track_number` (which makes the names of the racks, counting from
/// `*next_rack_number`). The drawer lasts `length` frames if it is a clip's: keyframes after its end are cut off, with the curve
/// kept up to the end.
pub(super) fn drawer_from_documents(
    owner: AnimationTarget,
    track_number: usize,
    racks: Vec<RackDocument>,
    legacy: AnimationDocument,
    length: Option<u64>,
    contents: &FileContents,
    next_rack_number: &mut u32,
) -> Result<Drawer, String> {
    let track = TrackId::new(u32::try_from(track_number).unwrap_or(u32::MAX));
    // (The properties of an old animation object are racks in the order of the table of properties, which is the order they were
    // kept in: a name that is not one of them comes first, to be refused.)
    let mut from_the_old_animation: Vec<RackDocument> = legacy
        .0
        .into_iter()
        .map(|(property, keyframes)| RackDocument {
            property,
            effect: None,
            target: None,
            keyframes,
        })
        .collect();
    from_the_old_animation.sort_by_key(|rack| {
        // (The names that are not names of anything come first; then those of the properties of a clip or a track; then those of values of effects.)
        match PropertyId::from_name(&rack.property) {
            Some(property) => (1, Some(property)),
            None if legacy_effects::the_effect_that_has_a_value_called(&rack.property).is_some() => (2, None),
            None => (0, None),
        }
    });
    let mut drawer = Drawer::default();
    for (number, document) in from_the_old_animation.into_iter().chain(racks).enumerate() {
        let rack = rack_from_document(document, owner, RackId::new(track, *next_rack_number), length, contents)
            .map_err(|reason| format!("rack number {}: {reason}", number + 1))?;
        *next_rack_number += 1;
        drawer.push(rack);
    }
    Ok(drawer)
}

fn rack_from_document(
    document: RackDocument,
    owner: AnimationTarget,
    id: RackId,
    length: Option<u64>,
    contents: &FileContents,
) -> Result<Rack, String> {
    let target = match document.target {
        None => owner,
        Some(target) => contents.target_from(target)?,
    };
    let Some(property) =
        contents.property_named(&document.property, document.effect.map(EffectInstanceId::new), target)
    else {
        return Err(if let Some(instance) = document.effect {
            format!(
                "\"{}\" cannot be animated for what the keyframes are of: it does not have {} that has a value of that name",
                document.property,
                EffectInstanceId::new(instance)
            )
        } else if let Some(effect) = legacy_effects::the_effect_that_has_a_value_called(&document.property) {
            format!(
                "\"{}\" cannot be animated for what the keyframes are of: it does not have the {effect} effect",
                document.property
            )
        } else {
            format!(
                "the keyframes are of \"{}\", which is not something that can be animated",
                document.property
            )
        });
    };
    if !contents.has_the_property(target, property) {
        return Err(format!(
            "\"{}\" cannot be animated for what the keyframes are of: it has none",
            document.property
        ));
    }
    let mut rack = Rack::new(id, target, property);
    for keyframe in document.keyframes {
        let KeyframeDocument { frame, value } = keyframe;
        let value = value_from_document(value, property.kind())
            .map_err(|reason| format!("the keyframe of \"{}\" at frame {frame}: {reason}", document.property))?;
        rack.set(frame, value)
            .map_err(|error| format!("the keyframe of \"{}\" at frame {frame}: {error}", document.property))?;
    }
    if rack.is_empty() {
        return Err(format!("it has no keyframes of \"{}\"", document.property));
    }
    Ok(match length {
        Some(length) => rack.truncated_to(length),
        None => rack,
    })
}

/// Where the clips and the tracks of a project are in the list of tracks and the lists of clips, for saying what racks are of.
pub(super) struct Positions {
    tracks: HashMap<TrackId, usize>,
    clips: HashMap<ClipId, (usize, usize)>,
}

impl Positions {
    pub(super) fn of(tracks: &[Track]) -> Self {
        let mut positions = Self {
            tracks: HashMap::new(),
            clips: HashMap::new(),
        };
        for (track_position, track) in tracks.iter().enumerate() {
            positions.tracks.insert(track.id(), track_position);
            for (clip_position, entry) in track.entries().iter().enumerate() {
                positions.clips.insert(entry.id(), (track_position, clip_position));
            }
        }
        positions
    }

    /// What something that is in a group is, as the file says it: a clip by its track's place and its own place on the track, a track by its
    /// place, a group by its place in the list of groups (`numbers` says what that is for each).
    pub(super) fn member_document_of(
        &self,
        member: Member,
        numbers: &HashMap<GroupId, usize>,
    ) -> Result<MemberDocument, String> {
        match member {
            Member::Clip(clip) => self
                .clips
                .get(&clip)
                .map(|(track, clip)| MemberDocument::Clip {
                    track: *track,
                    clip: *clip,
                })
                .ok_or_else(|| format!("a group has {clip} in it, which is not in the project")),
            Member::Track(track) => self
                .tracks
                .get(&track)
                .map(|position| MemberDocument::Track(*position))
                .ok_or_else(|| format!("a group has {track} in it, which is not in the project")),
            Member::Group(group) => numbers
                .get(&group)
                .map(|number| MemberDocument::Group(*number))
                .ok_or_else(|| format!("a group has {group} in it, which is not in the project")),
        }
    }

    fn target_document_of(&self, target: AnimationTarget) -> Result<TargetDocument, String> {
        match target {
            AnimationTarget::Track(track) => self
                .tracks
                .get(&track)
                .map(|position| TargetDocument {
                    track: *position,
                    clip: None,
                })
                .ok_or_else(|| format!("a rack is of the keyframes of {track}, which is not in the project")),
            AnimationTarget::Clip(clip) => self
                .clips
                .get(&clip)
                .map(|(track, clip)| TargetDocument {
                    track: *track,
                    clip: Some(*clip),
                })
                .ok_or_else(|| format!("a rack is of the keyframes of {clip}, which is not in the project")),
        }
    }
}

/// The documents of the racks in `drawer`, which is the drawer of `owner`.
pub(super) fn rack_documents_of(
    owner: AnimationTarget,
    drawer: &Drawer,
    positions: &Positions,
) -> Result<Vec<RackDocument>, String> {
    drawer
        .racks()
        .iter()
        .map(|rack| {
            Ok(RackDocument {
                property: rack.property().name_in_a_file().to_owned(),
                effect: rack.property().effect_instance().map(EffectInstanceId::number),
                target: (rack.target() != owner)
                    .then(|| positions.target_document_of(rack.target()))
                    .transpose()?,
                keyframes: rack
                    .keyframes()
                    .iter()
                    .map(|keyframe| KeyframeDocument {
                        frame: keyframe.frame,
                        value: super::value_document_of(&keyframe.value),
                    })
                    .collect(),
            })
        })
        .collect()
}
