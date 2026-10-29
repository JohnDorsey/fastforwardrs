// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! MIDI clips in the sound plan (phase 13, piece 13.4a; docs/engine-phase-13-midi-design.md, 13.4): a MIDI clip is a clip with no file and the notes of its track of the file for the stretch
//! of it that is heard, and the plan says which synthesizer plays them: the first of the chains they go through that has one.

use std::path::PathBuf;

use super::*;
use crate::composition::test_projects::{SOUND_FILE, media_clip, project_of};
use crate::midi_file::MidiControlKind;
use crate::project::tracks::Clip;
use crate::project::{
    AnimationTarget, EffectKind, GroupKind, Member, MidiClip, PropertyValue, ResourceKind, SourceFrameIndex,
    SourceMediaLink, SpeedFactor, TrackSharing, set_installed_plugins,
};
use crate::test_support::{
    a_midi_file_with_notes, an_installed_sound_plugin, an_installed_synthesizer, make_a_keyframe, put_an_effect_on,
    put_an_effect_on_a_group, the_installed_plugins_to_myself,
};

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn plan_from(project: &Project, from: u64) -> SoundPlan {
    plan_sound(project, frame(from), |_| true)
}

fn plan(project: &Project) -> SoundPlan {
    plan_from(project, 0)
}

fn clips_of(strip: &SoundStrip) -> Vec<&SoundClip> {
    strip
        .items
        .iter()
        .filter_map(|item| match item {
            SoundItem::Clip(clip) => Some(clip),
            SoundItem::Silence { .. } => None,
        })
        .collect()
}

/// The kinds of effect that are installed: a synthesizer, and an effect that changes sound.
fn the_kinds() -> (EffectKind, EffectKind) {
    set_installed_plugins(vec![
        an_installed_synthesizer("clap:test:synth"),
        an_installed_sound_plugin("clap:test:effect", "Maker", &[("mix", 0.5, None)]),
    ]);
    let kinds = EffectKind::installed_plugin_kinds();
    (kinds[0], kinds[1])
}

fn the_midi_clip(first: u64, end: u64, speed: u32) -> Clip {
    Clip::Midi(
        MidiClip::new(
            "tune.mid",
            0,
            SourceFrameIndex::new(first),
            SourceFrameIndex::new(end),
            SpeedFactor::new(speed).unwrap(),
        )
        .unwrap(),
    )
}

fn add_the_file(project: &mut Project, notes: &[(u32, u32, u8)]) {
    project
        .add_resource(
            "tune.mid",
            ResourceKind::Midi,
            a_midi_file_with_notes(notes),
            SourceMediaLink::from_absolute_path(PathBuf::from("/music/tune.mid")),
        )
        .unwrap();
}

/// Two notes: a half second from the start, and a half second from the first second. A project of one track with a MIDI clip of the first three seconds (90 frames).
fn a_track_with_music() -> Project {
    let mut project = project_of(vec![vec![(0, the_midi_clip(0, 90, 1))]]);
    add_the_file(&mut project, &[(0, 500, 60), (1000, 1500, 64)]);
    project
}

fn the_clip_of(project: &Project, track: usize) -> crate::project::ClipId {
    project.tracks()[track].placement_at_index(0).unwrap().id
}

fn notes_of(clip: &SoundClip) -> Vec<(u64, u64, u8)> {
    clip.music
        .as_ref()
        .expect("a MIDI clip has music")
        .notes
        .iter()
        .map(|note| (note.start, note.end, note.key))
        .collect()
}

fn consumers_of(clip: &SoundClip) -> Vec<NoteConsumer> {
    clip.music.as_ref().expect("a MIDI clip has music").consumers.clone()
}

#[test]
fn a_midi_clip_is_a_clip_with_no_file_whose_music_is_its_notes_in_samples_from_where_it_starts() {
    let project = a_track_with_music();

    let plan = plan(&project);

    assert_eq!(plan.strips.len(), 1);
    let clips = clips_of(&plan.strips[0]);
    assert_eq!(clips.len(), 1);
    assert_eq!(clips[0].source, None);
    assert_eq!(clips[0].timeline_frame_count, 90);
    assert_eq!(
        (clips[0].first_timeline_frame, clips[0].frames_of_the_clip_before),
        (0, 0)
    );
    assert_eq!(
        notes_of(clips[0]),
        vec![(0, 24_000, 60), (48_000, 72_000, 64)],
        "a half second is 24000 samples"
    );
    assert_eq!(
        consumers_of(clips[0]),
        Vec::<NoteConsumer>::new(),
        "no synthesizer is on its road: nothing plays them"
    );
    assert_eq!(clips[0].loudness, Loudness::Constant(1.0));
}

#[test]
fn a_midi_clip_that_is_later_on_the_timeline_has_its_notes_counted_from_where_it_starts() {
    let mut project = project_of(vec![vec![(30, the_midi_clip(0, 90, 1))]]);
    add_the_file(&mut project, &[(0, 500, 60)]);

    let plan = plan(&project);

    let clip = clips_of(&plan.strips[0])[0].clone();
    assert_eq!(clip.first_timeline_frame, 30);
    assert_eq!(notes_of(&clip), vec![(0, 24_000, 60)]);
    assert!(matches!(
        plan.strips[0].items[0],
        SoundItem::Silence { frame_count: 30 }
    ));
}

#[test]
fn the_speed_of_a_midi_clip_and_the_part_of_the_music_it_covers_say_where_its_notes_are() {
    let mut project = project_of(vec![vec![(0, the_midi_clip(30, 150, 2))]]);
    add_the_file(
        &mut project,
        &[
            (0, 900, 59),
            (1000, 1500, 60),
            (2000, 2500, 64),
            (4000, 4500, 67),
            (5000, 5500, 69),
        ],
    );

    let plan = plan(&project);

    let clip = clips_of(&plan.strips[0])[0].clone();
    // (From the first second to the fifth, at twice the speed: 60 frames, and each note is half as far from the start as it is in the music. The first note is before the clip; the last, at
    // 5.0 s, is after it.)
    assert_eq!(clip.timeline_frame_count, 60);
    assert_eq!(
        notes_of(&clip),
        vec![(0, 12_000, 60), (24_000, 36_000, 64), (72_000, 84_000, 67)]
    );
    assert_eq!((clip.first_source_frame, clip.end_source_frame), (30, 150));
}

#[test]
fn a_clip_whose_file_is_not_music_or_has_not_the_track_or_is_not_in_the_project_is_silence() {
    for what in ["not music", "no track", "the track after the last", "no file"] {
        let mut project = project_of(vec![vec![(
            0,
            Clip::Midi(
                MidiClip::new(
                    "tune.mid",
                    match what {
                        "no track" => 3,
                        "the track after the last" => 1,
                        _ => 0,
                    },
                    SourceFrameIndex::new(0),
                    SourceFrameIndex::new(30),
                    SpeedFactor::NORMAL,
                )
                .unwrap(),
            ),
        )]]);
        match what {
            "not music" => {
                project
                    .add_resource(
                        "tune.mid",
                        ResourceKind::Midi,
                        std::sync::Arc::from(&b"words"[..]),
                        SourceMediaLink::from_absolute_path(PathBuf::from("/music/tune.mid")),
                    )
                    .unwrap();
            }
            "no track" | "the track after the last" => add_the_file(&mut project, &[(0, 500, 60)]),
            _ => {}
        }

        assert_eq!(plan(&project).strips, Vec::new(), "{what}: no sound, so no strip");
    }
}

#[test]
fn a_midi_clip_that_is_muted_makes_no_music_and_one_that_is_not_does() {
    for muted in [true, false] {
        let mut project = project_of(vec![vec![(
            0,
            Clip::Midi(
                MidiClip::new(
                    "tune.mid",
                    0,
                    SourceFrameIndex::new(0),
                    SourceFrameIndex::new(30),
                    SpeedFactor::NORMAL,
                )
                .unwrap()
                .with_muted(muted),
            ),
        )]]);
        add_the_file(&mut project, &[(0, 500, 60)]);

        let plan = plan(&project);

        assert_eq!(plan.strips.len(), usize::from(!muted), "muted: {muted}");
    }
}

// --- who plays the notes ---

#[test]
fn the_synthesizer_among_the_effects_of_the_clip_plays_its_notes() {
    let _alone = the_installed_plugins_to_myself();
    let (synthesizer, _) = the_kinds();
    let mut project = a_track_with_music();
    let clip = the_clip_of(&project, 0);
    put_an_effect_on(
        &mut project,
        AnimationTarget::Clip(clip),
        synthesizer,
        &[PropertyValue::Number(1.0)],
    );

    let plan = plan(&project);

    assert_eq!(consumers_of(clips_of(&plan.strips[0])[0]), vec![NoteConsumer::Clip]);
}

#[test]
fn an_effect_that_changes_sound_does_not_play_notes_and_the_notes_go_on_to_the_synthesizer_after_it() {
    let _alone = the_installed_plugins_to_myself();
    let (synthesizer, effect) = the_kinds();
    let mut project = a_track_with_music();
    let clip = AnimationTarget::Clip(the_clip_of(&project, 0));
    put_an_effect_on(&mut project, clip, effect, &[PropertyValue::Number(0.5)]);
    assert_eq!(
        consumers_of(clips_of(&plan(&project).strips[0])[0]),
        Vec::<NoteConsumer>::new(),
        "an effect takes none"
    );

    put_an_effect_on(&mut project, clip, synthesizer, &[PropertyValue::Number(1.0)]);

    assert_eq!(
        consumers_of(clips_of(&plan(&project).strips[0])[0]),
        vec![NoteConsumer::Clip]
    );
}

#[test]
fn the_synthesizer_among_the_effects_of_the_track_plays_the_notes_of_the_clips_that_its_instance_hears() {
    let _alone = the_installed_plugins_to_myself();
    let (synthesizer, _) = the_kinds();
    let mut project = a_track_with_music();
    let track = AnimationTarget::Track(project.tracks()[0].id());
    put_an_effect_on(&mut project, track, synthesizer, &[PropertyValue::Number(1.0)]);

    let plan = plan(&project);

    assert_eq!(
        plan.strips[0].buses.len(),
        1,
        "the effects of the track are one instance for the run"
    );
    assert_eq!(consumers_of(clips_of(&plan.strips[0])[0]), vec![NoteConsumer::Bus]);
}

#[test]
fn with_one_copy_of_the_tracks_effects_for_each_clip_the_synthesizer_of_the_track_is_in_the_chain_of_the_clip() {
    let _alone = the_installed_plugins_to_myself();
    let (synthesizer, _) = the_kinds();
    let mut project = a_track_with_music();
    let track_id = project.tracks()[0].id();
    put_an_effect_on(
        &mut project,
        AnimationTarget::Track(track_id),
        synthesizer,
        &[PropertyValue::Number(1.0)],
    );
    project
        .set_track_sharing(track_id, crate::project::EffectDomain::Sound, TrackSharing::PerClip)
        .unwrap();

    let plan = plan(&project);

    assert_eq!(plan.strips[0].buses, Vec::new());
    assert_eq!(consumers_of(clips_of(&plan.strips[0])[0]), vec![NoteConsumer::Clip]);
}

#[test]
fn the_notes_of_a_clip_and_the_notes_of_a_sound_clip_after_it_are_heard_by_the_same_instance_of_the_tracks_synthesizer()
{
    let _alone = the_installed_plugins_to_myself();
    let (synthesizer, _) = the_kinds();
    let mut project = project_of(vec![vec![
        (0, the_midi_clip(0, 30, 1)),
        (0, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
        (0, the_midi_clip(30, 60, 1)),
    ]]);
    add_the_file(&mut project, &[(0, 500, 60), (1000, 1500, 64)]);
    let track = AnimationTarget::Track(project.tracks()[0].id());
    put_an_effect_on(&mut project, track, synthesizer, &[PropertyValue::Number(1.0)]);

    let plan = plan(&project);

    assert_eq!(plan.strips[0].buses.len(), 1);
    assert_eq!(
        (plan.strips[0].buses[0].first_item, plan.strips[0].buses[0].last_item),
        (0, 2)
    );
    let consumers: Vec<Option<Vec<NoteConsumer>>> = clips_of(&plan.strips[0])
        .iter()
        .map(|clip| clip.music.as_ref().map(|music| music.consumers.clone()))
        .collect();
    assert_eq!(
        consumers,
        vec![Some(vec![NoteConsumer::Bus]), None, Some(vec![NoteConsumer::Bus])]
    );
}

#[test]
fn the_synthesizer_of_a_group_plays_the_notes_of_the_clips_sent_to_it_and_of_the_groups_inside_it() {
    let _alone = the_installed_plugins_to_myself();
    let (synthesizer, effect) = the_kinds();
    let mut project = project_of(vec![
        vec![(0, the_midi_clip(0, 90, 1))],
        vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 60, 1)))],
    ]);
    add_the_file(&mut project, &[(0, 500, 60)]);
    let (music, sound) = (the_clip_of(&project, 0), the_clip_of(&project, 1));
    let inner = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(music), Member::Clip(sound)])
        .unwrap();
    // (A group with only an effect that changes sound does not play the notes.)
    put_an_effect_on_a_group(&mut project, inner, effect, &[PropertyValue::Number(0.5)]);
    let plan_with_the_effect = plan(&project);
    assert_eq!(
        consumers_of(clips_of(&plan_with_the_effect.strips[0])[0]),
        Vec::<NoteConsumer>::new()
    );

    put_an_effect_on_a_group(&mut project, inner, synthesizer, &[PropertyValue::Number(1.0)]);

    let plan = plan(&project);
    assert_eq!(plan.groups.len(), 1);
    assert_eq!(consumers_of(clips_of(&plan.strips[0])[0]), vec![NoteConsumer::Group(0)]);
}

#[test]
fn the_synthesizer_of_a_group_that_holds_the_group_a_clip_is_sent_to_plays_its_notes() {
    let _alone = the_installed_plugins_to_myself();
    let (synthesizer, effect) = the_kinds();
    let mut project = project_of(vec![
        vec![(0, the_midi_clip(0, 90, 1))],
        vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 60, 1)))],
    ]);
    add_the_file(&mut project, &[(0, 500, 60)]);
    let (music, sound) = (the_clip_of(&project, 0), the_clip_of(&project, 1));
    let inner = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(music), Member::Clip(sound)])
        .unwrap();
    let outer = project
        .create_group(
            GroupKind::TrackGroup,
            &[
                Member::Track(project.tracks()[0].id()),
                Member::Track(project.tracks()[1].id()),
            ],
        )
        .unwrap();
    put_an_effect_on_a_group(&mut project, inner, effect, &[PropertyValue::Number(0.5)]);
    put_an_effect_on_a_group(&mut project, outer, synthesizer, &[PropertyValue::Number(1.0)]);

    let plan = plan(&project);

    assert_eq!(plan.groups.len(), 2);
    let outer_place = plan.groups.iter().position(|group| group.group == outer).unwrap();
    assert_eq!(
        consumers_of(clips_of(&plan.strips[0])[0]),
        vec![NoteConsumer::Group(outer_place)]
    );
}

#[test]
fn the_chain_nearest_the_clip_that_has_a_synthesizer_plays_the_notes_and_the_others_do_not() {
    let _alone = the_installed_plugins_to_myself();
    let (synthesizer, _) = the_kinds();
    let mut project = project_of(vec![
        vec![(0, the_midi_clip(0, 90, 1))],
        vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 60, 1)))],
    ]);
    add_the_file(&mut project, &[(0, 500, 60)]);
    let (music, sound) = (the_clip_of(&project, 0), the_clip_of(&project, 1));
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(music), Member::Clip(sound)])
        .unwrap();
    put_an_effect_on_a_group(&mut project, group, synthesizer, &[PropertyValue::Number(1.0)]);
    let track = AnimationTarget::Track(project.tracks()[0].id());
    put_an_effect_on(&mut project, track, synthesizer, &[PropertyValue::Number(1.0)]);
    assert_eq!(
        consumers_of(clips_of(&plan(&project).strips[0])[0]),
        vec![NoteConsumer::Bus],
        "the track's before the group's"
    );

    put_an_effect_on(
        &mut project,
        AnimationTarget::Clip(music),
        synthesizer,
        &[PropertyValue::Number(1.0)],
    );

    assert_eq!(
        consumers_of(clips_of(&plan(&project).strips[0])[0]),
        vec![NoteConsumer::Clip],
        "the clip's own before the track's"
    );
}

#[test]
fn a_synthesizer_that_is_not_installed_still_takes_the_notes_and_so_does_the_group_that_has_only_it() {
    let _alone = the_installed_plugins_to_myself();
    // (A synthesizer of a plugin that is not here: a kind that a file's snapshot made.)
    let mut descriptor = an_installed_synthesizer("clap:test:gone").descriptor;
    descriptor.key = "clap:test:gone".into();
    let gone = EffectKind::from_snapshot(descriptor).unwrap();
    set_installed_plugins(Vec::new());
    assert!(!gone.is_installed() && gone.makes_sound_from_notes());
    let mut project = project_of(vec![
        vec![(0, the_midi_clip(0, 90, 1))],
        vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 60, 1)))],
    ]);
    add_the_file(&mut project, &[(0, 500, 60)]);
    let (music, sound) = (the_clip_of(&project, 0), the_clip_of(&project, 1));

    put_an_effect_on(&mut project, AnimationTarget::Clip(music), gone, &[]);
    assert_eq!(
        consumers_of(clips_of(&plan(&project).strips[0])[0]),
        vec![NoteConsumer::Clip],
        "a missing synthesizer takes the notes and plays nothing: they do not go on to another"
    );

    let mut with_a_group = project_of(vec![
        vec![(0, the_midi_clip(0, 90, 1))],
        vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 60, 1)))],
    ]);
    add_the_file(&mut with_a_group, &[(0, 500, 60)]);
    let group = with_a_group
        .create_group(GroupKind::ClipGroup, &[Member::Clip(music), Member::Clip(sound)])
        .unwrap();
    put_an_effect_on_a_group(&mut with_a_group, group, gone, &[]);
    let plan = plan(&with_a_group);
    assert_eq!(
        plan.groups.len(),
        1,
        "a group that has only a missing synthesizer is a group of the plan"
    );
    assert_eq!(consumers_of(clips_of(&plan.strips[0])[0]), vec![NoteConsumer::Group(0)]);
}

// --- the switch "pass notes on" (13.5) ---

/// A MIDI clip on the first track and a sound clip on the second that are in a clip group, and the synthesizers: one among the effects of the clip, one among those of its track and one among those
/// of the group (all of them off, which is how a synthesizer is put on). Their instances, to switch them by.
struct ARoadWithThreeSynthesizers {
    project: Project,
    clip: crate::project::ClipId,
    clip_synthesizer: crate::project::EffectInstanceId,
    track_synthesizer: crate::project::EffectInstanceId,
    group_synthesizer: crate::project::EffectInstanceId,
    group: crate::project::GroupId,
}

fn a_road_with_three_synthesizers(synthesizer: EffectKind) -> ARoadWithThreeSynthesizers {
    let mut project = project_of(vec![
        vec![(0, the_midi_clip(0, 90, 1))],
        vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 60, 1)))],
    ]);
    add_the_file(&mut project, &[(0, 500, 60)]);
    let (clip, sound) = (the_clip_of(&project, 0), the_clip_of(&project, 1));
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(clip), Member::Clip(sound)])
        .unwrap();
    let clip_synthesizer = put_an_effect_on(
        &mut project,
        AnimationTarget::Clip(clip),
        synthesizer,
        &[PropertyValue::Number(1.0)],
    );
    let track = AnimationTarget::Track(project.tracks()[0].id());
    let track_synthesizer = put_an_effect_on(&mut project, track, synthesizer, &[PropertyValue::Number(1.0)]);
    let group_synthesizer = put_an_effect_on_a_group(&mut project, group, synthesizer, &[PropertyValue::Number(1.0)]);
    ARoadWithThreeSynthesizers {
        project,
        clip,
        clip_synthesizer,
        track_synthesizer,
        group_synthesizer,
        group,
    }
}

impl ARoadWithThreeSynthesizers {
    fn consumers(&self) -> Vec<NoteConsumer> {
        consumers_of(clips_of(&plan(&self.project).strips[0])[0])
    }

    fn pass_on(&mut self, owner: Member, instance: crate::project::EffectInstanceId) {
        self.project.set_effect_passes_notes_on(owner, instance, true).unwrap();
    }

    fn the_track(&self) -> crate::project::TrackId {
        self.project.tracks()[0].id()
    }
}

#[test]
fn a_synthesizer_that_consumes_the_notes_is_the_last_on_the_road_and_one_that_passes_them_on_lets_them_go_to_the_next_chain()
 {
    let _alone = the_installed_plugins_to_myself();
    let (synthesizer, _) = the_kinds();
    let mut road = a_road_with_three_synthesizers(synthesizer);
    assert_eq!(
        road.consumers(),
        vec![NoteConsumer::Clip],
        "off: the first consumes them"
    );
    let (clip, track, group) = (
        Member::Clip(road.clip),
        Member::Track(road.the_track()),
        Member::Group(road.group),
    );

    // (The track's passes them on, but the clip's, which is before it, does not: they do not get as far.)
    road.pass_on(track, road.track_synthesizer);
    assert_eq!(road.consumers(), vec![NoteConsumer::Clip]);

    road.pass_on(clip, road.clip_synthesizer);
    assert_eq!(
        road.consumers(),
        vec![NoteConsumer::Clip, NoteConsumer::Bus, NoteConsumer::Group(0)],
        "the track's passes them on to the group's, which consumes them"
    );

    // (And the track's, switched off again, is the last.)
    road.project
        .set_effect_passes_notes_on(track, road.track_synthesizer, false)
        .unwrap();
    assert_eq!(road.consumers(), vec![NoteConsumer::Clip, NoteConsumer::Bus]);

    // (The group's passing them on changes nothing: there is no chain after it.)
    road.pass_on(group, road.group_synthesizer);
    assert_eq!(road.consumers(), vec![NoteConsumer::Clip, NoteConsumer::Bus]);
}

#[test]
fn a_chain_with_two_synthesizers_lets_the_notes_go_on_only_if_both_pass_them_on() {
    let _alone = the_installed_plugins_to_myself();
    let (synthesizer, _) = the_kinds();
    let mut road = a_road_with_three_synthesizers(synthesizer);
    let (clip, track) = (Member::Clip(road.clip), Member::Track(road.the_track()));
    let second = put_an_effect_on(
        &mut road.project,
        AnimationTarget::Clip(road.clip),
        synthesizer,
        &[PropertyValue::Number(1.0)],
    );
    road.pass_on(clip, road.clip_synthesizer);
    road.pass_on(track, road.track_synthesizer);
    assert_eq!(
        road.consumers(),
        vec![NoteConsumer::Clip],
        "the second one in the chain consumes them"
    );

    road.pass_on(clip, second);

    assert_eq!(
        road.consumers(),
        vec![NoteConsumer::Clip, NoteConsumer::Bus, NoteConsumer::Group(0)]
    );
}

#[test]
fn a_chain_that_has_no_synthesizer_lets_the_notes_by_and_one_that_is_switched_off_is_not_a_chain_that_has_one() {
    let _alone = the_installed_plugins_to_myself();
    let (synthesizer, effect) = the_kinds();
    let mut road = a_road_with_three_synthesizers(synthesizer);
    let clip = Member::Clip(road.clip);
    road.pass_on(clip, road.clip_synthesizer);
    // (The track has an effect that changes sound, and no synthesizer: the notes go through it.)
    let track = AnimationTarget::Track(road.the_track());
    road.project.remove_effect(track, road.track_synthesizer).unwrap();
    put_an_effect_on(&mut road.project, track, effect, &[PropertyValue::Number(0.5)]);
    assert_eq!(road.consumers(), vec![NoteConsumer::Clip, NoteConsumer::Group(0)]);

    // (The synthesizer of the group, switched off, does not play them or take them: the road has no one else.)
    road.project
        .set_effect_enabled(Member::Group(road.group), road.group_synthesizer, false)
        .unwrap();
    assert_eq!(road.consumers(), vec![NoteConsumer::Clip]);
}

#[test]
fn the_notes_go_out_through_the_groups_that_hold_a_group_as_far_as_the_first_that_consumes_them() {
    let _alone = the_installed_plugins_to_myself();
    let (synthesizer, _) = the_kinds();
    let mut project = project_of(vec![
        vec![(0, the_midi_clip(0, 90, 1))],
        vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 60, 1)))],
    ]);
    add_the_file(&mut project, &[(0, 500, 60)]);
    let (music, sound) = (the_clip_of(&project, 0), the_clip_of(&project, 1));
    let inner = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(music), Member::Clip(sound)])
        .unwrap();
    let outer = project
        .create_group(
            GroupKind::TrackGroup,
            &[
                Member::Track(project.tracks()[0].id()),
                Member::Track(project.tracks()[1].id()),
            ],
        )
        .unwrap();
    let in_the_inner = put_an_effect_on_a_group(&mut project, inner, synthesizer, &[PropertyValue::Number(1.0)]);
    put_an_effect_on_a_group(&mut project, outer, synthesizer, &[PropertyValue::Number(1.0)]);
    let places = |project: &Project| {
        let plan = plan(project);
        let place_of = |group| plan.groups.iter().position(|node| node.group == group).unwrap();
        (
            consumers_of(clips_of(&plan.strips[0])[0]),
            place_of(inner),
            place_of(outer),
        )
    };
    let (consumers, inner_place, _) = places(&project);
    assert_eq!(
        consumers,
        vec![NoteConsumer::Group(inner_place)],
        "the inner consumes them"
    );

    project
        .set_effect_passes_notes_on(Member::Group(inner), in_the_inner, true)
        .unwrap();

    let (consumers, inner_place, outer_place) = places(&project);
    assert_eq!(
        consumers,
        vec![NoteConsumer::Group(inner_place), NoteConsumer::Group(outer_place)]
    );
}

#[test]
fn a_synthesizer_that_is_not_installed_passes_the_notes_on_if_it_is_set_to() {
    let _alone = the_installed_plugins_to_myself();
    let mut descriptor = an_installed_synthesizer("clap:test:gone").descriptor;
    descriptor.key = "clap:test:gone".into();
    let gone = EffectKind::from_snapshot(descriptor).unwrap();
    set_installed_plugins(Vec::new());
    let mut project = a_track_with_music();
    let clip = the_clip_of(&project, 0);
    let track = AnimationTarget::Track(project.tracks()[0].id());
    let on_the_clip = put_an_effect_on(&mut project, AnimationTarget::Clip(clip), gone, &[]);
    put_an_effect_on(&mut project, track, gone, &[]);
    let consumers = |project: &Project| consumers_of(clips_of(&plan(project).strips[0])[0]);
    assert_eq!(consumers(&project), vec![NoteConsumer::Clip]);

    project
        .set_effect_passes_notes_on(Member::Clip(clip), on_the_clip, true)
        .unwrap();

    assert_eq!(consumers(&project), vec![NoteConsumer::Clip, NoteConsumer::Bus]);
}

/// **The two ways of asking whether a MIDI clip has anyone to play it must agree**: the window asks the project (`has_a_synthesizer_on_the_road`, for the marker on the clip), the sound asks the
/// plan (`consumers`). A self-check that turns a difference between them into a failure.
#[test]
fn the_window_and_the_plan_agree_on_whether_a_midi_clip_has_a_synthesizer_on_its_road() {
    let _alone = the_installed_plugins_to_myself();
    let (synthesizer, _) = the_kinds();
    for switched_off in 0..8_u8 {
        let mut road = a_road_with_three_synthesizers(synthesizer);
        let track = Member::Track(road.the_track());
        let which = [
            (Member::Clip(road.clip), road.clip_synthesizer),
            (track, road.track_synthesizer),
            (Member::Group(road.group), road.group_synthesizer),
        ];
        for (number, (owner, instance)) in which.into_iter().enumerate() {
            if switched_off & (1 << number) != 0 {
                road.project.set_effect_enabled(owner, instance, false).unwrap();
            }
        }

        let plan_says = !road.consumers().is_empty();
        let window_says = road.project.has_a_synthesizer_on_the_road(road.clip);

        assert_eq!(plan_says, window_says, "switched off: {switched_off:03b}");
        assert_eq!(
            plan_says,
            switched_off != 0b111,
            "only when all three are off is there none"
        );
    }
}

// --- the stretches of a clip ---

#[test]
fn a_clip_that_is_muted_in_the_middle_is_two_clips_and_a_note_that_is_held_across_the_mute_is_begun_again_where_the_mute_ends()
 {
    let mut project = project_of(vec![vec![(0, the_midi_clip(0, 90, 1))]]);
    // (A note from 0.5 s to 2.5 s; the clip is muted from frame 30 (1.0 s) to frame 60 (2.0 s).)
    add_the_file(&mut project, &[(500, 2500, 60), (2600, 2800, 62)]);
    let clip = AnimationTarget::Clip(the_clip_of(&project, 0));
    make_a_keyframe(
        &mut project,
        clip,
        crate::project::PropertyId::Muted,
        30,
        PropertyValue::Flag(true),
    );
    make_a_keyframe(
        &mut project,
        clip,
        crate::project::PropertyId::Muted,
        60,
        PropertyValue::Flag(false),
    );
    // (A switch before its first keyframe is as the first says: the clip is heard from the start only if a keyframe there says so.)
    make_a_keyframe(
        &mut project,
        clip,
        crate::project::PropertyId::Muted,
        0,
        PropertyValue::Flag(false),
    );

    let plan = plan(&project);

    let items = &plan.strips[0].items;
    assert!(matches!(items[0], SoundItem::Clip(_)));
    assert!(matches!(items[1], SoundItem::Silence { frame_count: 30 }));
    assert!(matches!(items[2], SoundItem::Clip(_)));
    let clips = clips_of(&plan.strips[0]);
    assert_eq!(
        notes_of(clips[0]),
        vec![(24_000, 48_000, 60)],
        "from half a second to the end of the stretch, where its note off is"
    );
    assert_eq!(
        notes_of(clips[1]),
        vec![(0, 24_000, 60), (28_800, 38_400, 62)],
        "the held note is begun where the sound comes back, and ends at 2.5 s; the next is at 2.6 s"
    );
    assert_eq!(clips[1].frames_of_the_clip_before, 60);
}

#[test]
fn a_plan_that_begins_in_the_middle_of_a_clip_has_the_notes_that_are_sounding_there_begun_where_it_begins() {
    let mut project = project_of(vec![vec![(0, the_midi_clip(0, 90, 1))]]);
    add_the_file(&mut project, &[(0, 500, 60), (500, 2500, 64), (2000, 2200, 67)]);

    // (From frame 30, one second in: the first note is over, the second is sounding, and the third begins at 2.0 s.)
    let plan = plan_from(&project, 30);

    let clip = clips_of(&plan.strips[0])[0].clone();
    assert_eq!(clip.first_timeline_frame, 30);
    assert_eq!(
        notes_of(&clip),
        vec![(0, 72_000, 64), (48_000, 57_600, 67)],
        "the held note from the start of the plan to 2.5 s; the third a second after it"
    );
}

#[test]
fn a_pedal_and_a_wheel_are_where_the_file_says_and_are_let_go_of_at_the_end_of_the_stretch() {
    // (A file with a pedal and a pitch wheel: made here, since the builder of notes has none.)
    let mut project = project_of(vec![vec![(0, the_midi_clip(0, 30, 1))]]);
    let mut file = b"MThd\0\0\0\x06\0\0\0\x01\x03\xE8".to_vec();
    let body: Vec<u8> = vec![
        0x00, 0xFF, 0x51, 0x03, 0x0F, 0x42, 0x40, // a second a quarter note
        0x83, 0x60, 0xB0, 64, 127, // 480 ms: the pedal down
        0x64, 0xE0, 0x00, 0x60, // 100 ms later: the wheel
        0x00, 0xFF, 0x2F, 0x00,
    ];
    file.extend_from_slice(b"MTrk");
    file.extend_from_slice(&(body.len() as u32).to_be_bytes());
    file.extend_from_slice(&body);
    project
        .add_resource(
            "tune.mid",
            ResourceKind::Midi,
            std::sync::Arc::from(file),
            SourceMediaLink::from_absolute_path(PathBuf::from("/music/tune.mid")),
        )
        .unwrap();

    let plan = plan(&project);

    let controls = &clips_of(&plan.strips[0])[0].music.as_ref().unwrap().controls;
    assert_eq!(
        controls
            .iter()
            .map(|control| (control.at, control.what))
            .collect::<Vec<_>>(),
        vec![
            (23_040, MidiControlKind::Controller { number: 64, value: 127 }),
            (27_840, MidiControlKind::PitchBend { value: 12_288 }),
            (48_000, MidiControlKind::Controller { number: 64, value: 0 }),
            (48_000, MidiControlKind::PitchBend { value: 8_192 }),
        ]
    );
}
