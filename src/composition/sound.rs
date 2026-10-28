// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What is heard at every frame: for each track that has any sound, a *strip* that says, from the
//! start (of the timeline, or of the place the sound is wanted from) to the end of its last
//! sound, which clips are heard when. The strips are heard together, mixed.
//!
//! A clip whose mute switch, or whose track's, is turned by keyframes is cut into the stretches over which it is
//! heard and those over which it is silent. A clip whose gain, or whose track's, has keyframes is heard at a
//! loudness that changes with them ([`Loudness::Changing`]), and one that has none at the same loudness throughout.

use std::collections::BTreeSet;

use std::collections::HashMap;

pub use self::music::{Music, NoteConsumer, PlannedControl, PlannedNote, StretchOfMusic, music_of_a_stretch};
use self::tail_routes::{Emitter, GroupGeometry, tail_routes_of, window_routes_of};
pub use self::tail_routes::{TailDestination, TailRoute};
use super::pictures::{LayerEffect, layer_effect_of};
use crate::project::tracks::{ClipId, ClipPlacement, Gain, MediaClip, MidiClip, Track, TrackId};
use crate::project::{
    AnimationTarget, AppliedEffect, EffectDomain, FrameRate, GroupId, Member, Project, PropertyId, ResolvedAnimations,
    SourceMediaIdentifier, SpeedFactor, TimelineFrameIndex, TrackSharing,
};

/// How loud a stretch of sound is at a frame of it: the loudness of the clip and of its track together.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecibelsAt {
    /// How many frames after the first frame of the stretch.
    pub frame: u64,
    pub decibels: f32,
}

/// How loud a stretch of sound is.
#[derive(Clone, Debug, PartialEq)]
pub enum Loudness {
    /// The number the sound's samples are multiplied by, the same at every frame.
    Constant(f64),
    /// Louder at some frames than at others: how loud it is, in decibels, at each of some frames, between which it
    /// changes evenly (in decibels). Before the first and after the last it is as loud as at that one. There are at
    /// least two, the first at frame 0 and the last at the frame after the stretch's last, and not all are the same
    /// (that is [`Self::Constant`]).
    Changing(Vec<DecibelsAt>),
}

impl Loudness {
    /// The number the samples are multiplied by at `frame` frames after the start of the stretch.
    pub fn linear_factor_at(&self, frame: u64) -> f64 {
        match self {
            Self::Constant(factor) => *factor,
            Self::Changing(points) => {
                let after = points.partition_point(|point| point.frame <= frame);
                let decibels = match (after.checked_sub(1).map(|index| points[index]), points.get(after)) {
                    (Some(before), Some(next)) => {
                        let fraction = (frame - before.frame) as f64 / (next.frame - before.frame) as f64;
                        f64::from(before.decibels) + (f64::from(next.decibels) - f64::from(before.decibels)) * fraction
                    }
                    (Some(last), None) => f64::from(last.decibels),
                    (None, Some(first)) => f64::from(first.decibels),
                    (None, None) => 0.0,
                };
                Gain::linear_factor_of_decibels(decibels)
            }
        }
    }
}

/// A stretch of the sound of a file, heard during part of a track.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundClip {
    pub clip: ClipId,
    /// The file whose sound it is. **`None` for a MIDI clip** (phase 13), which has no sound of its own: see `music`.
    pub source: Option<SourceMediaIdentifier>,
    /// The frame of the timeline at which this stretch of the clip is first heard. (The frames of the keyframes of its effects are frames of the timeline.)
    pub first_timeline_frame: u64,
    /// How many frames of the timeline the clip was heard for before this stretch of it begins: the stretches of it that came before, or the part that a seek skipped. **It is
    /// as much of the clip's own sound as there is to be given to the effects that remember** (see `pre_roll_frames_of`): the file has more before the clip's first frame, which
    /// the clip was cut to leave out, and that is not heard.
    pub frames_of_the_clip_before: u64,
    /// The source frame whose sound is heard first. (The sound has no frames: a frame stands for
    /// the stretch of sound as long as a frame of the video.)
    pub first_source_frame: u64,
    /// The frame just after the last source frame heard (the end is exclusive).
    pub end_source_frame: u64,
    /// How much faster than normal it is played: the clip's speed times its track's.
    pub speed: SpeedFactor,
    /// How many frames of the timeline it is heard for. (If the file is too short for that, the
    /// rest is silence.)
    pub timeline_frame_count: u64,
    /// How loud it is: the clip's volume and its track's.
    pub loudness: Loudness,
    /// What is done to the sound of the clip before it is made as loud as `loudness` says, one after another: the sound effects that are applied to the clip, in the order
    /// they are applied in (see `Project::effects_applied_to_the_clip`), with their values and the keyframes of them. An effect that is not installed is here too: it is the
    /// one that makes the sound pass through unchanged, and it is kept in the plan so that the sound knows it was there.
    pub effects: Vec<LayerEffect>,
    /// Where its sound goes once its own effects, its volume and the effects of its track have done what they do (phase 12, piece 12.3): into the sum of the group that is nearest to it of
    /// the groups that have sound effects (`SoundPlan::groups`: the clip groups it is in, one in another, and then the track groups of its track), or, if it is inside none, into
    /// the sum of all the sound.
    pub sent_to: Option<usize>,
    /// Where the sound that its effects go on making after it ends goes, from some frame on (phase 12, piece 12.3d: the tail policies of the clip and of the groups it is in; see
    /// `tail_routes`). Empty: it goes where the clip does. Only for a clip that is not heard by an instance of its track's effects (the instance has the routes then, see `SoundBus`).
    pub tail_routes: Vec<TailRoute>,
    /// For a MIDI clip: the notes and controllers that a synthesizer is to play over this stretch of it, and which synthesizer (phase 13, piece 13.4; the sound of the clip itself is silence, and its effects
    /// and its volume are as for any clip, which is what the synthesizer's sound goes through). `None` for a clip of a file.
    /// (Boxed: a clip of sound is much the more common, and `SoundItem` should not be as large as the music makes it.)
    pub music: Option<Box<Music>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SoundItem {
    /// Nothing is heard for this many frames.
    Silence {
        frame_count: u64,
    },
    Clip(SoundClip),
}

impl SoundItem {
    pub fn timeline_frame_count(&self) -> u64 {
        match self {
            Self::Silence { frame_count } => *frame_count,
            Self::Clip(clip) => clip.timeline_frame_count,
        }
    }
}

/// What one track sounds like, from the start of the plan on: its items follow one another, with
/// no gaps, until the end of the last sound.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundStrip {
    pub track: TrackId,
    pub items: Vec<SoundItem>,
    /// The instances of the effects of the track that clips share (phase 12, piece 12.2): one for each **run** of clips, in the order of `items`. A clip that is in none has
    /// all of its effects in its own chain.
    pub buses: Vec<SoundBus>,
    /// How loud the track is, **after its effects** (D9 of docs/engine-phase-12-design.md: the volume and the mute of a track are done to what its clips and its effects make, so that
    /// the memory of the effects goes on while it is muted and the effects hear the sound as the clips made it). The frames are counted from the start of the plan.
    pub loudness: Loudness,
    /// The stretches of the plan, from a frame up to a frame (not including it), counted from the start of the plan, where the track is muted: silent, after its effects.
    pub muted: Vec<(u64, u64)>,
}

/// One instance of the sound effects of a track, which hears the clips of a run one after another: they are put together (each as its own effects and its volume make it) and
/// given to the effects, which have one state from the first of them to the last.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundBus {
    /// The first and the last item of the strip that it hears (the items between are heard too).
    pub first_item: usize,
    pub last_item: usize,
    /// The effects of the track, one after another, with their values and the keyframes of them (the frames of which are frames of the timeline).
    pub effects: Vec<LayerEffect>,
    /// Where what the instance makes goes (phase 12, piece 12.3): the group that the clips it hears are sent to, or the sum of all the sound. (For an instance that groups take windows of,
    /// where it goes outside the windows.)
    pub sent_to: Option<usize>,
    /// Where the sound that the instance goes on making after its last clip goes, from some frame on: the policy of the last clip of the run and of the groups it is in (the tail of a run
    /// is its last clip's). Empty: it goes where the clips go. (For an instance that groups take windows of, the windows.)
    pub tail_routes: Vec<TailRoute>,
}

impl SoundStrip {
    /// How many frames the strip lasts.
    pub fn timeline_frame_count(&self) -> u64 {
        self.items.iter().map(SoundItem::timeline_frame_count).sum()
    }
}

/// A clip group or a track group that has sound effects (phase 12, piece 12.3; section 2 of docs/engine-phase-12-design.md): **its effects are done to the sum of what its
/// members make**, each member made as its own effects, its volume and the effects of its track make it (an instance of the track's effects hears only the clips that go to the
/// same place: a group's edge ends a run, see `SoundBus`). A group that has no sound effect that does anything is not one of these: its members are heard as if it was not there.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundGroup {
    pub group: GroupId,
    /// The sound effects of the group, one after another, with their values (a group's effects have no keyframes), and where its chain has the badge of a tag, those of the tag.
    pub effects: Vec<LayerEffect>,
    /// The group that holds this one and has sound effects too (the nearest, in `SoundPlan::groups`): where the sum goes once the effects have done what they do. `None`: into the sum
    /// of all the sound.
    pub parent: Option<usize>,
}

/// The sound at every frame from some frame on.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundPlan {
    pub frame_rate: FrameRate,
    /// How many frames the plan lasts: from where it starts to the end of the timeline. The strips
    /// are no longer than this.
    pub total_frame_count: u64,
    /// How many frames at the start of the plan are made and not heard (phase 12, piece 12.2d): the plan begins that many frames before the place the sound is wanted from, so that
    /// effects that remember have heard what comes before it (see [`plan_sound`]). Part of `total_frame_count`.
    pub lead_in_frames: u64,
    /// The strips of the tracks that have any sound, the one in front first.
    pub strips: Vec<SoundStrip>,
    /// The clip groups and track groups that have sound effects (phase 12, piece 12.3), in the order the project has its groups in.
    pub groups: Vec<SoundGroup>,
}

/// How many samples a second the sound is made of: the sound engine's `SAMPLE_RATE`, which this module cannot name (the engine is built on the plan, and not the plan on the engine);
/// a test holds the two to each other.
pub const SOUND_SAMPLES_A_SECOND: u64 = 48_000;

/// How many samples an effect goes on making sound after its input has ended (phase 12, section 6.1 of docs/engine-phase-12-design.md): what its person said (`None`, or a length), and
/// where they said nothing what its plugin says (the tail of a CLAP plugin, up to two seconds; half a second for an LV2 plugin, which does not say; nothing for a LADSPA plugin). An
/// effect that is not a plugin that is installed makes the sound pass through unchanged, and has none.
pub fn tail_samples_of(effect: &LayerEffect) -> u64 {
    tail_samples(effect.kind, effect.tail_length)
}

/// [`tail_samples_of`] for an effect of this kind that has this tail length.
fn tail_samples(kind: crate::project::EffectKind, tail_length: crate::project::TailLength) -> u64 {
    use crate::project::TailLength;
    let Some(plugin) = kind.installed_plugin() else {
        return 0;
    };
    match tail_length {
        TailLength::Automatic => plugin.pre_roll_samples as u64,
        TailLength::None => 0,
        TailLength::Milliseconds(milliseconds) => u64::from(milliseconds) * SOUND_SAMPLES_A_SECOND / 1000,
    }
}

/// The most seconds of sound before the place it is wanted from that are made so that effects that remember have heard what comes before it. (A tail of a minute is not made again at every
/// jump of the playhead: past this the sound at the start is as it would be if the effects had begun to listen this long before.)
pub const MOST_SECONDS_OF_LEAD_IN: u64 = 10;

/// How many frames before `from` the sound is to begin so that the effects of it that remember have heard what they would have on the way there: the greatest, over the clips, of
/// what all the sound effects that are applied to a clip (its own, its tags' and its track's) ring on for added up (the sound of a clip goes through all of them one after another),
/// and no more than [`MOST_SECONDS_OF_LEAD_IN`] seconds and no more than there is before `from`. 0 where nothing rings on.
fn lead_in_frames_before(project: &Project, effects: &HashMap<ClipId, Vec<AppliedEffect<'_>>>, from: u64) -> u64 {
    if from == 0 {
        return 0;
    }
    let ring = effects
        .values()
        .map(|applied| {
            applied
                .iter()
                .filter(|applied| applied.effect.kind().domain() == EffectDomain::Sound)
                .map(|applied| tail_samples(applied.effect.kind(), applied.effect.tail_length()))
                .sum::<u64>()
        })
        .max()
        .unwrap_or(0);
    let frame_rate = project.frame_rate();
    let most = frames_that_reach(frame_rate, MOST_SECONDS_OF_LEAD_IN * SOUND_SAMPLES_A_SECOND);
    frames_that_reach(frame_rate, ring).min(most).min(from)
}

/// How many samples a chain of effects goes on after its input has ended: the tails of its effects added up (each hears what the one before it makes, tail and all).
pub fn tail_samples_of_a_chain(effects: &[LayerEffect]) -> u64 {
    effects.iter().map(tail_samples_of).sum()
}

/// How many frames of the timeline `samples` samples reach, rounded up.
pub fn frames_that_reach(frame_rate: FrameRate, samples: u64) -> u64 {
    let seconds_times_the_denominator = u128::from(samples) * u128::from(frame_rate.numerator());
    let frames = seconds_times_the_denominator
        .div_ceil(u128::from(frame_rate.denominator()) * u128::from(SOUND_SAMPLES_A_SECOND));
    u64::try_from(frames).unwrap_or(u64::MAX)
}

/// Works out the sound of the project from `from` to the end, as it is when all the sound is
/// on (the setting for what sped-up clips sound like is for whatever turns the plan into sound).
/// The sound of a file that is not `usable` is left out.
pub fn plan_sound(
    project: &Project,
    from: TimelineFrameIndex,
    is_usable: impl Fn(SourceMediaIdentifier) -> bool,
) -> SoundPlan {
    let wanted_from = from.value();
    let animations = project.resolved_animations();
    // The effects of a clip by itself (its own, its track's, its tags'): the effects of the groups it is in are done to the sum that the group's members make (see `SoundGroup`).
    let effects = project.effects_applied_to_every_clip_by_itself();
    let groups_of_the_sound = sound_groups_of(project, &animations);
    // The plan begins before the place the sound is wanted from by as much as the effects ring on for: what is made of that is not heard, and the effects have heard what leads up to it.
    // (The sound of a clip goes through the effects of its groups too, one after another, and they ring on after it.)
    let lead_in_frames = lead_in_frames_before(project, &project.effects_applied_to_every_clip(), wanted_from);
    let from = wanted_from - lead_in_frames;
    let total_frame_count = project.total_frame_count().saturating_sub(from);
    let strips = project
        .tracks()
        .iter()
        .filter_map(|track| {
            strip_of(
                project,
                &animations,
                &effects,
                &groups_of_the_sound,
                track,
                from,
                &is_usable,
            )
        })
        .collect();
    SoundPlan {
        frame_rate: project.frame_rate(),
        total_frame_count,
        lead_in_frames,
        strips,
        groups: groups_of_the_sound.nodes,
    }
}

/// The groups of the sound, where each clip goes, and what the tail policies need to know of each group.
struct GroupsOfTheSound {
    nodes: Vec<SoundGroup>,
    sent_to: HashMap<ClipId, usize>,
    geometry: Vec<GroupGeometry>,
    /// Which place in `nodes` each group that is one has.
    places: HashMap<GroupId, usize>,
}

/// The clip groups and track groups that have a sound effect that is installed (the others are pass-through), with where the sum of each goes, and for each clip that is inside
/// one of them the one it is sent to: the nearest of those round it (the clip groups it is in, one in another, and then the track groups of its track).
fn sound_groups_of(project: &Project, animations: &ResolvedAnimations) -> GroupsOfTheSound {
    let mut places: HashMap<GroupId, usize> = HashMap::new();
    let mut groups: Vec<SoundGroup> = Vec::new();
    let mut geometry: Vec<GroupGeometry> = Vec::new();
    for group in project.groups().iter().filter(|group| group.kind().is_a_shape()) {
        let effects: Vec<LayerEffect> = project
            .effects_applied_to_the_group(group.id())
            .iter()
            .filter(|applied| applied.effect.kind().domain() == EffectDomain::Sound)
            .map(|applied| layer_effect_of(project, animations, applied))
            .collect();
        // (A synthesizer that is not installed makes the group a node too: it takes the notes of what is in the group, and plays nothing.)
        if effects
            .iter()
            .any(|effect| effect.kind.is_installed() || is_a_synthesizer(effect))
        {
            places.insert(group.id(), groups.len());
            groups.push(SoundGroup {
                group: group.id(),
                effects,
                parent: None,
            });
            geometry.push(GroupGeometry {
                rectangle: project
                    .rectangle_of_the_group(group.id())
                    .unwrap_or(crate::project::Rectangle {
                        first_frame: 0,
                        end_frame: 0,
                        first_track: 0,
                        last_track: 0,
                    }),
                tail_edge: group.tail_policy().edge,
                is_a_clip_group: group.kind() == crate::project::GroupKind::ClipGroup,
            });
        }
    }
    let mut sent_to = HashMap::new();
    if groups.is_empty() {
        return GroupsOfTheSound {
            nodes: groups,
            sent_to,
            geometry,
            places,
        };
    }
    for track in project.tracks() {
        for entry in track.entries() {
            let round: Vec<usize> = project
                .groups_round_the_clip(entry.id())
                .into_iter()
                .filter_map(|group| places.get(&group).copied())
                .collect();
            if let Some(&nearest) = round.first() {
                sent_to.insert(entry.id(), nearest);
            }
            // (Each is in the next: the clip groups one in another, and then the track groups.)
            for pair in round.windows(2) {
                groups[pair[0]].parent = Some(pair[1]);
            }
        }
    }
    GroupsOfTheSound {
        nodes: groups,
        sent_to,
        geometry,
        places,
    }
}

/// Feeds everything that is heard of the sound effects of the clips of `project` into `hasher`, so that a number made from it is different whenever it is: for each clip that
/// has sound effects, which they are (the kind and version, which plugin of which library stands in for it now, if one does, so that a plugin that is found by the scan is a
/// change), their values, the keyframes of them and the state that their plugin saved. A project with no sound effects feeds nothing.
pub fn hash_the_sound_effects_into(project: &Project, hasher: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    let applied = project.effects_applied_to_every_clip();
    let animations = (!applied.is_empty()).then(|| project.resolved_animations());
    let Some(animations) = animations else {
        return;
    };
    for track in project.tracks() {
        for entry in track.entries() {
            let Some(effects) = applied.get(&entry.id()) else {
                continue;
            };
            for effect in effects
                .iter()
                .filter(|applied| applied.effect.kind().domain() == EffectDomain::Sound)
            {
                let layer = layer_effect_of(project, &animations, effect);
                entry.id().hash(hasher);
                layer.instance.number().hash(hasher);
                // (How much of the track one copy of its effects covers is heard too.)
                track.sharing().sound.hash(hasher);
                layer.kind.key().hash(hasher);
                layer.kind.version().hash(hasher);
                layer
                    .kind
                    .installed_plugin()
                    .map(|plugin| (&plugin.library, &plugin.address))
                    .hash(hasher);
                for value in &layer.values {
                    value.hash_into(hasher);
                }
                layer.animation.hash_properties_into(hasher, |_| true);
                // (What its plugin saved is heard too: a window changed it. A number that was made when it was saved: the bytes are not read at every frame.)
                effect.effect.state_fingerprint().hash(hasher);
                // (Where the notes of a MIDI clip go is heard: a synthesizer that passes them on is a different sound.)
                effect.effect.passes_notes_on().hash(hasher);
            }
        }
    }
    if !applied
        .values()
        .flatten()
        .any(|applied| applied.effect.kind().domain() == EffectDomain::Sound)
    {
        return;
    }
    // What the tail policies say of where the ring of a clip goes depends on where the groups are and on what each says: a change of them is heard.
    for group in project.groups().iter().filter(|group| group.kind().is_a_shape()) {
        let Some(rectangle) = project.rectangle_of_the_group(group.id()) else {
            continue;
        };
        group.id().hash(hasher);
        group.tail_policy().edge.hash(hasher);
        (
            rectangle.first_frame,
            rectangle.end_frame,
            rectangle.first_track,
            rectangle.last_track,
        )
            .hash(hasher);
    }
    for track in project.tracks() {
        for entry in track.entries() {
            if applied.get(&entry.id()).is_some_and(|effects| !effects.is_empty()) {
                entry.id().hash(hasher);
                entry.tail_policy().edge.hash(hasher);
            }
        }
    }
}

fn strip_of(
    project: &Project,
    animations: &ResolvedAnimations,
    effects: &HashMap<ClipId, Vec<AppliedEffect<'_>>>,
    groups: &GroupsOfTheSound,
    track: &Track,
    from: u64,
    is_usable: &impl Fn(SourceMediaIdentifier) -> bool,
) -> Option<SoundStrip> {
    let mut items: Vec<SoundItem> = Vec::new();
    // What each item shares with the other clips of the track: the effects of the track, for a clip that has them at the end of its chain (see `effects_of_a_clip`).
    let mut shared_effects: Vec<Option<Vec<LayerEffect>>> = Vec::new();
    let mut next_frame = from;
    for placement in track.placements() {
        if let Some(midi) = placement.clip.as_midi() {
            let end = placement.end_timeline_frame.value();
            if end <= from {
                continue;
            }
            let start = placement.first_timeline_frame.value().max(from);
            push_silence(&mut items, start - next_frame);
            shared_effects.resize(items.len(), None);
            // (A clip whose file is not in the project, is not music, or has not the track that the clip plays is silent, as a clip of a file that is not there is.)
            let the_music = project
                .resources()
                .get(midi.resource())
                .and_then(|resource| resource.midi_file().ok())
                .filter(|file| usize::from(midi.midi_track()) < file.tracks.len());
            match the_music {
                Some(file) => {
                    let track_of_the_file = &file.tracks[usize::from(midi.midi_track())];
                    for stretch in stretches_heard_or_not(animations, &placement, midi.is_muted(), start, end) {
                        if stretch.is_heard {
                            let (own, shared) = effects_of_a_clip(project, animations, effects, track, placement.id);
                            shared_effects.resize(items.len(), None);
                            items.push(SoundItem::Clip(music_clip_over(
                                project,
                                animations,
                                own,
                                &placement,
                                midi,
                                track_of_the_file,
                                (stretch.first_frame, stretch.end_frame),
                                groups.sent_to.get(&placement.id).copied(),
                            )));
                            shared_effects.push(shared);
                        } else {
                            push_silence(&mut items, stretch.end_frame - stretch.first_frame);
                            shared_effects.resize(items.len(), None);
                        }
                    }
                }
                None => {
                    push_silence(&mut items, end - start);
                    shared_effects.resize(items.len(), None);
                }
            }
            next_frame = end;
            continue;
        }
        let Some(media) = placement.clip.as_media() else {
            continue;
        };
        let end = placement.end_timeline_frame.value();
        if end <= from {
            continue;
        }
        let start = placement.first_timeline_frame.value().max(from);
        push_silence(&mut items, start - next_frame);
        shared_effects.resize(items.len(), None);

        let has_sound = project
            .source(media.source())
            .is_some_and(|source| source.properties.has_sound())
            && is_usable(media.source());
        if has_sound {
            for stretch in stretches_heard_or_not(animations, &placement, media.is_muted(), start, end) {
                if stretch.is_heard {
                    let (own, shared) = effects_of_a_clip(project, animations, effects, track, placement.id);
                    shared_effects.resize(items.len(), None);
                    items.push(SoundItem::Clip(sound_clip_over(
                        animations,
                        own,
                        &placement,
                        media,
                        stretch.first_frame,
                        stretch.end_frame,
                        groups.sent_to.get(&placement.id).copied(),
                    )));
                    shared_effects.push(shared);
                } else {
                    push_silence(&mut items, stretch.end_frame - stretch.first_frame);
                    shared_effects.resize(items.len(), None);
                }
            }
        } else {
            push_silence(&mut items, end - start);
            shared_effects.resize(items.len(), None);
        }
        next_frame = end;
    }
    // Silence at the end is not part of the sound.
    while matches!(items.last(), Some(SoundItem::Silence { .. })) {
        items.pop();
    }
    shared_effects.truncate(items.len());
    shared_effects.resize(items.len(), None);
    let mut buses = runs_of_clips_that_share_the_effects_of_the_track(
        &items,
        &shared_effects,
        project.frame_rate(),
        track.sharing().sound,
    );
    give_the_rings_their_routes(project, groups, track, &mut items, &mut buses);
    give_the_music_its_consumers(groups, &mut items, &buses);
    let strip_frames: u64 = items.iter().map(SoundItem::timeline_frame_count).sum();
    let muted = muted_stretches_of_the_track(animations, track, from, from + strip_frames);
    // (A track that is muted all the way through makes no sound whatever its effects remember: it has no strip.)
    let is_muted_all_the_way = muted.as_slice() == [(0, strip_frames)];
    (!items.is_empty() && !is_muted_all_the_way).then(|| SoundStrip {
        track: track.id(),
        loudness: loudness_of_the_track(animations, track, from, from + strip_frames),
        muted,
        items,
        buses,
    })
}

/// Says where the ring of each clip that is not heard by an instance of its track's effects, and of each instance, goes: see `tail_routes`.
fn give_the_rings_their_routes(
    project: &Project,
    groups: &GroupsOfTheSound,
    track: &Track,
    items: &mut [SoundItem],
    buses: &mut [SoundBus],
) {
    if groups.nodes.is_empty() {
        return;
    }
    let Some(row) = project.track_position(track.id()) else {
        return;
    };
    let frame_rate = project.frame_rate();
    let policy_of = |clip: ClipId| {
        project
            .clip_tail_policy(clip)
            .map_or(crate::project::ClipTailEdge::Around, |policy| policy.edge)
    };
    let mut heard_by_a_bus = vec![false; items.len()];
    for bus in buses.iter_mut() {
        heard_by_a_bus[bus.first_item..=bus.last_item].fill(true);
        let clips: Vec<&SoundClip> = items[bus.first_item..=bus.last_item]
            .iter()
            .filter_map(|item| match item {
                SoundItem::Clip(clip) => Some(clip),
                SoundItem::Silence { .. } => None,
            })
            .collect();
        let Some(last) = clips.last() else {
            continue;
        };
        // (What rings on after the last clip: the longest of the rings of the clips, which are in the instance, and the tail of its effects.)
        let ring_samples = clips
            .iter()
            .map(|clip| tail_samples_of_a_chain(&clip.effects))
            .max()
            .unwrap_or(0)
            + tail_samples_of_a_chain(&bus.effects);
        let ring_from = last.first_timeline_frame + last.timeline_frame_count;
        let ring_frames = frames_that_reach(frame_rate, ring_samples);
        if track.sharing().sound == TrackSharing::PerTrackWithWindows {
            // Groups take windows of the one instance: outside them its sound goes to the track group that holds the track, if one has sound effects, or to the sum.
            let default = track_groups_round(project, track)
                .into_iter()
                .find_map(|group| groups.places.get(&group).copied());
            bus.sent_to = default;
            bus.tail_routes = window_routes_of(&groups.geometry, row, default, ring_from + ring_frames);
            continue;
        }
        bus.tail_routes = tail_routes_of(
            &groups.nodes,
            &groups.geometry,
            &Emitter {
                row,
                ring_from,
                ring_frames,
                sent_to: last.sent_to,
                policy: policy_of(last.clip),
            },
        );
    }
    for (place, item) in items.iter_mut().enumerate() {
        let SoundItem::Clip(clip) = item else {
            continue;
        };
        if heard_by_a_bus[place] {
            continue;
        }
        clip.tail_routes = tail_routes_of(
            &groups.nodes,
            &groups.geometry,
            &Emitter {
                row,
                ring_from: clip.first_timeline_frame + clip.timeline_frame_count,
                ring_frames: frames_that_reach(frame_rate, tail_samples_of_a_chain(&clip.effects)),
                sent_to: clip.sent_to,
                policy: policy_of(clip.clip),
            },
        );
    }
}

/// The track groups that hold `track`, the nearest first.
fn track_groups_round(project: &Project, track: &Track) -> Vec<GroupId> {
    let Some(entry) = track.entries().first() else {
        return Vec::new();
    };
    project
        .groups_round_the_clip(entry.id())
        .into_iter()
        .filter(|group| {
            project
                .group(*group)
                .is_some_and(|found| found.kind() == crate::project::GroupKind::TrackGroup)
        })
        .collect()
}

/// The runs of clips that share the effects of their track: clips that have the effects of the track at the end of their chain, one after another, **with no more silence between two
/// of them than the effects go on for after their input ends** (their tail, 6 of docs/engine-phase-12-design.md): where the silence is longer the memory of the effects is gone, and
/// the next clip has them as they were made, which is the same sound and costs nothing while it is idle. A clip that does not share them (it has its own copy where its badge is)
/// ends a run too. Each is one instance, with the effects that all of its clips have.
fn runs_of_clips_that_share_the_effects_of_the_track(
    items: &[SoundItem],
    shared: &[Option<Vec<LayerEffect>>],
    frame_rate: FrameRate,
    sharing: TrackSharing,
) -> Vec<SoundBus> {
    let mut buses: Vec<SoundBus> = Vec::new();
    for (place, effects) in shared.iter().enumerate() {
        let Some(effects) = effects else {
            continue;
        };
        let tail_frames = frames_that_reach(frame_rate, tail_samples_of_a_chain(effects));
        let is_one_instance_for_the_track = matches!(
            sharing,
            TrackSharing::PerTrackWithWindows | TrackSharing::PerTrackIndivisible
        );
        match buses.last_mut() {
            // (With one instance for the track, a run goes on across any silence and any edge of a group: only a clip that has a copy of its own ends it. Otherwise a run is clips that
            // go to the same place, with no more silence between them than the tail: the edge of a group ends it, and a copy of the effects that the clips inside it have is not one
            // that hears the clips outside.)
            Some(last)
                if if is_one_instance_for_the_track {
                    items[last.last_item + 1..place]
                        .iter()
                        .all(|item| matches!(item, SoundItem::Silence { .. }))
                } else {
                    only_silence_that_is_not_longer_than(&items[last.last_item + 1..place], tail_frames)
                        && sent_to_by(&items[last.last_item]) == sent_to_by(&items[place])
                } =>
            {
                last.last_item = place;
            }
            _ => buses.push(SoundBus {
                first_item: place,
                last_item: place,
                effects: effects.clone(),
                sent_to: sent_to_by(&items[place]),
                tail_routes: Vec::new(),
            }),
        }
    }
    buses
}

/// Where the sound of `item` goes: the group it is sent to, if it is a clip that is sent to one.
fn sent_to_by(item: &SoundItem) -> Option<usize> {
    match item {
        SoundItem::Clip(clip) => clip.sent_to,
        SoundItem::Silence { .. } => None,
    }
}

/// Whether `items` are all silences, and together last no more than `frames` frames.
fn only_silence_that_is_not_longer_than(items: &[SoundItem], frames: u64) -> bool {
    let mut total = 0_u64;
    for item in items {
        match item {
            SoundItem::Silence { frame_count } => total += frame_count,
            SoundItem::Clip(_) => return false,
        }
    }
    total <= frames
}

/// The sound effects that are applied to the clip `clip` (a clip of `track`), with their values and keyframes, in the order they are applied in, as two lists: **the clip's own**
/// (its own effects and those of its tags), and **the effects of its track, if the track's effects at the end of the chain are to be shared with the clips next to it** (`None`
/// when they are not: they are then part of the clip's own). They are shared when the track's sharing of its sound effects is not one copy for each clip, and the effects of the
/// track come after all the others in the clip's chain (a clip whose badge of its track is moved to the middle of its chain has its own copy of them, where the badge is).
/// (Its picture effects are not here.)
fn effects_of_a_clip(
    project: &Project,
    animations: &ResolvedAnimations,
    effects: &HashMap<ClipId, Vec<AppliedEffect<'_>>>,
    track: &Track,
    clip: ClipId,
) -> (Vec<LayerEffect>, Option<Vec<LayerEffect>>) {
    let applied: Vec<&AppliedEffect<'_>> = effects
        .get(&clip)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter(|applied| applied.effect.kind().domain() == EffectDomain::Sound)
        .collect();
    let is_the_tracks = |applied: &&AppliedEffect<'_>| applied.owner == Member::Track(track.id());
    let those_of_the_track = applied.iter().filter(|applied| is_the_tracks(applied)).count();
    let at_the_end = applied[applied.len() - those_of_the_track..].iter().all(is_the_tracks);
    let layer_of = |applied: &&AppliedEffect<'_>| layer_effect_of(project, animations, applied);
    if those_of_the_track == 0 || !at_the_end || track.sharing().sound == TrackSharing::PerClip {
        return (applied.iter().map(layer_of).collect(), None);
    }
    let (own, track_effects) = applied.split_at(applied.len() - those_of_the_track);
    (
        own.iter().map(layer_of).collect(),
        Some(track_effects.iter().map(layer_of).collect()),
    )
}

/// A stretch of a clip, from `first_frame` up to `end_frame` of the timeline, over which it is heard, or is not
/// because it or its track is muted.
struct Stretch {
    first_frame: u64,
    end_frame: u64,
    is_heard: bool,
}

/// The stretches, one after another, that the frames `start` up to `end` of the clip `placement` are cut into by turning the mute switch of the clip on and off with keyframes. A clip
/// that is not muted by keyframes is one stretch. (The mute of its **track** is not here: it is done after the effects of the track, to the sound of the whole track: see
/// [`muted_stretches_of_the_track`].)
fn stretches_heard_or_not(
    animations: &ResolvedAnimations,
    placement: &ClipPlacement<'_>,
    is_muted: bool,
    start: u64,
    end: u64,
) -> Vec<Stretch> {
    let clip_animation = animations.of(AnimationTarget::Clip(placement.id));
    let is_heard_at = |frame: u64| !clip_animation.is_muted_at(is_muted, frame);
    // The switch only turns at a keyframe, and what is between two is what the earlier one says.
    let frames_it_may_turn_at: BTreeSet<u64> = clip_animation
        .keyframes_of(PropertyId::Muted)
        .into_iter()
        .flat_map(|keyframes| keyframes.iter().map(|keyframe| keyframe.frame))
        .filter(|frame| start < *frame && *frame < end)
        .collect();

    let mut stretches: Vec<Stretch> = Vec::new();
    for first_frame in std::iter::once(start).chain(frames_it_may_turn_at) {
        let is_heard = is_heard_at(first_frame);
        match stretches.last_mut() {
            Some(last) if last.is_heard == is_heard => last.end_frame = end,
            _ => {
                if let Some(last) = stretches.last_mut() {
                    last.end_frame = first_frame;
                }
                stretches.push(Stretch {
                    first_frame,
                    end_frame: end,
                    is_heard,
                });
            }
        }
    }
    stretches
}

/// The sound of the clip `placement` that is heard from `first_frame` up to `end_frame` of the timeline.
fn sound_clip_over(
    animations: &ResolvedAnimations,
    effects: Vec<LayerEffect>,
    placement: &ClipPlacement<'_>,
    media: &MediaClip,
    first_frame: u64,
    end_frame: u64,
    sent_to: Option<usize>,
) -> SoundClip {
    let speed = u64::from(placement.effective_speed.value());
    let frames_cut_off_the_start = first_frame - placement.first_timeline_frame.value();
    let first_source_frame = media.first_source_frame().value() + frames_cut_off_the_start * speed;
    SoundClip {
        clip: placement.id,
        source: Some(media.source()),
        first_timeline_frame: first_frame,
        frames_of_the_clip_before: frames_cut_off_the_start,
        first_source_frame,
        end_source_frame: media
            .end_source_frame()
            .value()
            .min(first_source_frame + (end_frame - first_frame) * speed),
        speed: placement.effective_speed,
        timeline_frame_count: end_frame - first_frame,
        loudness: loudness_over(animations, placement, media.gain(), first_frame, end_frame),
        effects,
        sent_to,
        tail_routes: Vec::new(),
        music: None,
    }
}

/// The sound of the MIDI clip `placement` that is heard from `first_frame` up to `end_frame` of the timeline: no sound of its own, and the notes of its track of the file in that stretch for the
/// synthesizer that plays them (which it is is told by [`give_the_music_its_consumers`], once the buses and the groups are known).
#[allow(clippy::too_many_arguments)]
fn music_clip_over(
    project: &Project,
    animations: &ResolvedAnimations,
    effects: Vec<LayerEffect>,
    placement: &ClipPlacement<'_>,
    midi: &MidiClip,
    track_of_the_file: &crate::midi_file::MidiTrack,
    (first_frame, end_frame): (u64, u64),
    sent_to: Option<usize>,
) -> SoundClip {
    let speed = u64::from(placement.effective_speed.value());
    let frames_cut_off_the_start = first_frame - placement.first_timeline_frame.value();
    let first_source_frame = midi.first_source_frame().value() + frames_cut_off_the_start * speed;
    let end_source_frame = midi
        .end_source_frame()
        .value()
        .min(first_source_frame + (end_frame - first_frame) * speed);
    let (notes, controls) = music_of_a_stretch(
        track_of_the_file,
        StretchOfMusic {
            frame_rate: project.frame_rate(),
            clip_first_source_frame: midi.first_source_frame().value(),
            first_source_frame,
            end_source_frame,
            speed: placement.effective_speed.value(),
        },
    );
    SoundClip {
        clip: placement.id,
        source: None,
        first_timeline_frame: first_frame,
        frames_of_the_clip_before: frames_cut_off_the_start,
        first_source_frame,
        end_source_frame,
        speed: placement.effective_speed,
        timeline_frame_count: end_frame - first_frame,
        loudness: loudness_over(animations, placement, midi.gain(), first_frame, end_frame),
        effects,
        sent_to,
        tail_routes: Vec::new(),
        music: Some(Box::new(Music {
            notes,
            controls,
            consumers: Vec::new(),
        })),
    }
}

/// Whether the effect is a synthesizer: **it plays the notes that reach it, installed or not** (one that is not installed makes no sound and still takes the notes, so that a project does not
/// play its notes with another synthesizer where this one is missing).
fn is_a_synthesizer(effect: &LayerEffect) -> bool {
    effect.kind.makes_sound_from_notes()
}

/// Says which chains have the synthesizers that play the notes of each MIDI clip of a strip. **The notes go along the road of the clip: its own effects, then the instance of the track's effects
/// that hears the clip, then the effects of the groups the clip is sent to, from the nearest out** (see 13.4 of docs/engine-phase-13-midi-design.md). A chain that has no synthesizer lets them by.
/// **A chain that has some plays them, and they go on to the next chain only if every synthesizer of it passes notes on** (13.5: the one that is off consumes them). A clip with no synthesizer
/// on its road has no one to play it.
fn give_the_music_its_consumers(groups: &GroupsOfTheSound, items: &mut [SoundItem], buses: &[SoundBus]) {
    for (place, item) in items.iter_mut().enumerate() {
        let SoundItem::Clip(clip) = item else {
            continue;
        };
        if clip.music.is_none() {
            continue;
        }
        let mut road: Vec<(NoteConsumer, &[LayerEffect])> = vec![(NoteConsumer::Clip, &clip.effects)];
        if let Some(bus) = buses
            .iter()
            .find(|bus| bus.first_item <= place && place <= bus.last_item)
        {
            road.push((NoteConsumer::Bus, &bus.effects));
        }
        let mut group = clip.sent_to;
        while let Some(number) = group {
            road.push((NoteConsumer::Group(number), &groups.nodes[number].effects));
            group = groups.nodes[number].parent;
        }
        let mut consumers = Vec::new();
        for (consumer, effects) in road {
            let mut synthesizers = effects.iter().filter(|effect| is_a_synthesizer(effect)).peekable();
            if synthesizers.peek().is_none() {
                continue;
            }
            consumers.push(consumer);
            if !synthesizers.all(|effect| effect.passes_notes_on) {
                break;
            }
        }
        if let Some(music) = &mut clip.music {
            music.consumers = consumers;
        }
    }
}

/// How loud the clip `placement` is from `first_frame` up to `end_frame` of the timeline (its own volume: its track's is done to the sound of the whole track, after the effects of the
/// track: see [`loudness_of_the_track`]).
fn loudness_over(
    animations: &ResolvedAnimations,
    placement: &ClipPlacement<'_>,
    gain: Gain,
    first_frame: u64,
    end_frame: u64,
) -> Loudness {
    loudness_between(
        animations.of(AnimationTarget::Clip(placement.id)),
        gain,
        first_frame,
        end_frame,
        first_frame,
    )
}

/// How loud `track` is from the frame `from` (the start of the plan) up to `end_frame`, counted from `from`: its volume, which is done after the effects of the track.
fn loudness_of_the_track(animations: &ResolvedAnimations, track: &Track, from: u64, end_frame: u64) -> Loudness {
    loudness_between(
        animations.of(AnimationTarget::Track(track.id())),
        track.gain(),
        from,
        end_frame,
        from,
    )
}

/// How loud something that is animated by `animation` and has the volume `gain` when it is not animated is, from `first_frame` up to `end_frame`: a number if it does not change, and
/// otherwise how loud, in decibels, at each frame it bends at, counted from `counted_from`.
fn loudness_between(
    animation: &crate::project::Animation,
    gain: Gain,
    first_frame: u64,
    end_frame: u64,
    counted_from: u64,
) -> Loudness {
    let gain_keyframes = animation.keyframes_of(PropertyId::Gain);
    if gain_keyframes.is_none() {
        return Loudness::Constant(gain.linear_factor());
    }
    let decibels_at = |frame: u64| animation.gain_at(gain, frame).decibels();
    // It changes evenly between its keyframes.
    let frames_it_may_bend_at: BTreeSet<u64> = gain_keyframes
        .into_iter()
        .flat_map(|keyframes| keyframes.iter().map(|keyframe| keyframe.frame))
        .filter(|frame| first_frame < *frame && *frame < end_frame)
        .chain([first_frame, end_frame])
        .collect();
    let points: Vec<DecibelsAt> = frames_it_may_bend_at
        .into_iter()
        .map(|frame| DecibelsAt {
            frame: frame - counted_from,
            decibels: decibels_at(frame),
        })
        .collect();
    if points.iter().all(|point| point.decibels == points[0].decibels) {
        Loudness::Constant(Gain::linear_factor_of_decibels(f64::from(points[0].decibels)))
    } else {
        Loudness::Changing(points)
    }
}

/// The stretches from the frame `from` (the start of the plan) up to `end_frame` where `track` is muted, counted from `from`: each from a frame up to a frame (not including it). The
/// switch only turns at a keyframe, and what is between two keyframes is what the earlier one says. The sound of the track is silent there, **after its effects**.
fn muted_stretches_of_the_track(
    animations: &ResolvedAnimations,
    track: &Track,
    from: u64,
    end_frame: u64,
) -> Vec<(u64, u64)> {
    let animation = animations.of(AnimationTarget::Track(track.id()));
    let frames_it_may_turn_at: BTreeSet<u64> = animation
        .keyframes_of(PropertyId::Muted)
        .into_iter()
        .flat_map(|keyframes| keyframes.iter().map(|keyframe| keyframe.frame))
        .filter(|frame| from < *frame && *frame < end_frame)
        .collect();
    let mut stretches: Vec<(u64, u64)> = Vec::new();
    let mut turns: Vec<u64> = std::iter::once(from).chain(frames_it_may_turn_at).collect();
    turns.push(end_frame);
    for window in turns.windows(2) {
        if !animation.is_muted_at(track.is_muted(), window[0]) {
            continue;
        }
        let (first, end) = (window[0] - from, window[1] - from);
        match stretches.last_mut() {
            Some(last) if last.1 == first => last.1 = end,
            _ => stretches.push((first, end)),
        }
    }
    stretches
}

/// Adds `frame_count` frames of silence, joined to the silence before if there is any.
fn push_silence(items: &mut Vec<SoundItem>, frame_count: u64) {
    if frame_count == 0 {
        return;
    }
    if let Some(SoundItem::Silence { frame_count: before }) = items.last_mut() {
        *before += frame_count;
    } else {
        items.push(SoundItem::Silence { frame_count });
    }
}

mod music;
mod tail_routes;

#[cfg(test)]
mod animation_tests;
#[cfg(test)]
mod buses_tests;
#[cfg(test)]
mod effects_tests;
#[cfg(test)]
mod groups_tests;
#[cfg(test)]
mod music_clip_tests;
#[cfg(test)]
mod sharing_tests;
#[cfg(test)]
mod tail_routes_tests;
#[cfg(test)]
mod tails_as_they_are_tests;
#[cfg(test)]
mod tests;
