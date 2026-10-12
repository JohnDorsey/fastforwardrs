//! The racks of keyframes in a project file: the drawers of the tracks and of the clips, as the file has them, and what they are
//! made into when the file is loaded.
//!
//! A rack says which property its keyframes are of and, unless they are of the clip or the track whose drawer the rack is in,
//! which clip or track that is: by where it is in the file (the number of its track in the list of tracks and, for a clip, the
//! number of the clip in the list of its track's clips, both counted from 0), because the files do not have the names that
//! clips and tracks are known by when a project is edited. (Files of versions before 7 had an `animation` object in each clip and
//! track instead, which is the same as a rack of the keyframes of the clip's or the track's own properties, in its own drawer.)

use std::collections::HashMap;

use super::document::{AnimationDocument, KeyframeDocument, RackDocument, TargetDocument};
use super::value_from_document;
use crate::project::{AnimationTarget, ClipId, ClipKind, Drawer, PropertyId, Rack, RackId, Track, TrackId};

/// What is known of what a file has, which is what the racks it has can be checked against.
pub(super) struct FileContents {
    /// The kind of each clip of each track, in the order they are in the file.
    pub kinds_of_the_clips: Vec<Vec<ClipKind>>,
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

    /// Whether `target` (which the file has) has `property`.
    fn has_the_property(&self, target: AnimationTarget, property: PropertyId) -> bool {
        match target {
            AnimationTarget::Track(_) => property.applies_to_a_track(),
            AnimationTarget::Clip(clip) => {
                let kind = self.kinds_of_the_clips[clip.origin().value() as usize][clip.number() as usize];
                property.applies_to_a_clip_of_kind(kind)
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
            target: None,
            keyframes,
        })
        .collect();
    from_the_old_animation.sort_by_key(|rack| PropertyId::from_name(&rack.property));
    let mut drawer = Drawer::default();
    for (number, document) in from_the_old_animation.into_iter().chain(racks).enumerate() {
        let rack = rack_from_document(document, owner, RackId::new(track, *next_rack_number), length, contents)
            .map_err(|reason| format!("rack number {}: {reason}", number + 1))?;
        *next_rack_number += 1;
        if !drawer.has_room_for(&rack) {
            return Err(format!(
                "rack number {}: the drawer has another rack of the keyframes of \"{}\" of the same thing",
                number + 1,
                rack.property().name()
            ));
        }
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
    let property = PropertyId::from_name(&document.property).ok_or_else(|| {
        format!(
            "the keyframes are of \"{}\", which is not something that can be animated",
            document.property
        )
    })?;
    let target = match document.target {
        None => owner,
        Some(target) => contents.target_from(target)?,
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
        let value = value_from_document(value, property)
            .map_err(|reason| format!("the keyframe of \"{}\" at frame {frame}: {reason}", document.property))?;
        rack.set(frame, value)
            .map_err(|error| format!("the keyframe of \"{}\" at frame {frame}: {error}", document.property))?;
    }
    if rack.is_empty() {
        return Err(format!("it has no keyframes of \"{}\"", document.property));
    }
    Ok(match length {
        Some(length) => rack.truncated_by_its_own_curve(length),
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
                property: rack.property().name().to_owned(),
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
