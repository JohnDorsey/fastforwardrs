// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A number of a shader read from a mask that is a track (`docs/shader-masks-design.md`): which numbers can be, what the project refuses (a track that is not there, a ring of tracks
//! that read each other), and what becomes of the reading when the track is deleted.

use super::tests::{project_of_tracks, video_clip};
use super::*;

/// A shader with a number that has a range (`amount`, place 0), a colour, a switch, a choice and a point.
const TINT: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" }, { \"NAME\": \"amount\", \"TYPE\": \"float\", \"DEFAULT\": 0.25 }, { \"NAME\": \"strength\", \"TYPE\": \"float\", \"MIN\": 2.0, \"MAX\": 6.0, \"DEFAULT\": 3.0 }, { \"NAME\": \"tint\", \"TYPE\": \"color\", \"DEFAULT\": [1.0, 0.0, 0.0, 1.0] }, { \"NAME\": \"flip\", \"TYPE\": \"bool\", \"DEFAULT\": true }, { \"NAME\": \"mode\", \"TYPE\": \"long\", \"VALUES\": [3, 7], \"DEFAULT\": 7 }, { \"NAME\": \"spot\", \"TYPE\": \"point2D\", \"DEFAULT\": [10, 20] } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage); }\n";

const AMOUNT: usize = 0;
const STRENGTH: usize = 1;
const THE_COLOUR: usize = 2;
const THE_SWITCH: usize = 3;
const THE_CHOICE: usize = 4;
const THE_POINT: usize = 5;

fn tint() -> EffectKind {
    let file = crate::isf::read(TINT).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_of(&file, "tint.fs", TINT)).unwrap()
}

fn from(track: TrackId, channel: MaskChannel, nothing: f32) -> Option<DrivenValue> {
    Some(DrivenValue::from_a_track(track, channel, nothing))
}

/// A project of three tracks with a clip each, the track in front first, and the clip on each.
fn three_tracks() -> (Project, [TrackId; 3], [ClipId; 3]) {
    let project = project_of_tracks(vec![
        vec![(0, video_clip(1, 0, 100))],
        vec![(0, video_clip(1, 0, 100))],
        vec![(0, video_clip(1, 0, 100))],
    ]);
    let tracks = [
        project.tracks()[0].id(),
        project.tracks()[1].id(),
        project.tracks()[2].id(),
    ];
    let clips = tracks.map(|track| project.track(track).unwrap().placement_at_index(0).unwrap().id);
    (project, tracks, clips)
}

/// The effect of a shader on the clip `clip`, and what to call it by.
fn with_a_shader_on(project: &mut Project, clip: ClipId) -> (Member, EffectInstanceId) {
    let instance = project.add_effect(AnimationTarget::Clip(clip), tint()).unwrap();
    (Member::Clip(clip), instance)
}

fn driven_of(project: &Project, owner: Member, instance: EffectInstanceId, slot: usize) -> Option<DrivenValue> {
    project
        .chain_of_the_member(owner)
        .unwrap()
        .iter()
        .find(|effect| effect.instance() == instance)
        .unwrap()
        .driven_values()
        .of(slot)
        .copied()
}

#[test]
fn a_number_of_a_shader_is_read_from_a_track_and_from_its_slider_again_when_let_go() {
    let (mut project, [_, middle, _], [clip, ..]) = three_tracks();
    let (owner, instance) = with_a_shader_on(&mut project, clip);
    let slider_before = project
        .chain_of_the_member(owner)
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .values()
        .to_vec();

    project
        .set_driven_value(owner, instance, AMOUNT, from(middle, MaskChannel::Green, 0.75))
        .unwrap();

    assert_eq!(
        driven_of(&project, owner, instance, AMOUNT),
        Some(DrivenValue::from_a_track(middle, MaskChannel::Green, 0.75))
    );
    let effect = project
        .chain_of_the_member(owner)
        .unwrap()
        .iter()
        .next()
        .unwrap()
        .clone();
    assert_eq!(effect.values(), slider_before, "the slider is kept as it was");
    assert_eq!(
        driven_of(&project, owner, instance, STRENGTH),
        None,
        "and no other number is driven"
    );

    // The reading is changed by saying it again, and let go of by saying none.
    project
        .set_driven_value(owner, instance, AMOUNT, from(middle, MaskChannel::Red, 0.1))
        .unwrap();
    assert_eq!(
        driven_of(&project, owner, instance, AMOUNT).map(|driven| (driven.channel, driven.value_where_nothing)),
        Some((MaskChannel::Red, 0.1))
    );
    project.set_driven_value(owner, instance, AMOUNT, None).unwrap();
    assert_eq!(driven_of(&project, owner, instance, AMOUNT), None);
    assert_eq!(
        project
            .chain_of_the_member(owner)
            .unwrap()
            .iter()
            .next()
            .unwrap()
            .values(),
        slider_before,
        "and the slider is still where it was"
    );
}

#[test]
fn two_numbers_of_one_effect_may_be_read_from_different_tracks() {
    let (mut project, [_, middle, last], [clip, ..]) = three_tracks();
    let (owner, instance) = with_a_shader_on(&mut project, clip);

    project
        .set_driven_value(owner, instance, AMOUNT, from(middle, MaskChannel::Red, 0.0))
        .unwrap();
    project
        .set_driven_value(owner, instance, STRENGTH, from(last, MaskChannel::Brightness, 4.0))
        .unwrap();

    assert_eq!(
        driven_of(&project, owner, instance, AMOUNT).unwrap().source,
        MaskSource::Track(middle)
    );
    assert_eq!(
        driven_of(&project, owner, instance, STRENGTH).unwrap().source,
        MaskSource::Track(last)
    );
    let effect = project.chain_of_the_member(owner).unwrap().iter().next().unwrap();
    assert_eq!(effect.driven_values().tracks(), [middle, last]);
}

#[test]
fn only_a_number_with_a_range_of_a_shader_can_be_read_from_a_mask() {
    let (mut project, [_, middle, _], [clip, ..]) = three_tracks();
    let (owner, instance) = with_a_shader_on(&mut project, clip);

    for (slot, what) in [
        (THE_COLOUR, "a colour"),
        (THE_SWITCH, "a switch"),
        (THE_CHOICE, "a choice"),
        (THE_POINT, "a point"),
        (40, "a value there is not"),
    ] {
        let result = project.set_driven_value(owner, instance, slot, from(middle, MaskChannel::Red, 0.0));
        assert!(
            matches!(result, Err(EditError::ValueCannotBeDriven { .. })),
            "{what}: {result:?}"
        );
        assert_eq!(driven_of(&project, owner, instance, slot), None, "{what}");
    }

    // An effect that the program has of its own is not a shader, whatever its numbers are.
    let hue = project
        .add_effect(AnimationTarget::Clip(clip), EffectKind::HUE_SHIFT)
        .unwrap();
    let result = project.set_driven_value(owner, hue, 0, from(middle, MaskChannel::Red, 0.0));
    assert!(
        matches!(result, Err(EditError::ValueCannotBeDriven { .. })),
        "{result:?}"
    );
}

#[test]
fn letting_go_of_a_value_that_was_never_read_from_a_mask_changes_nothing_and_is_not_an_error() {
    let (mut project, _, [clip, ..]) = three_tracks();
    let (owner, instance) = with_a_shader_on(&mut project, clip);
    let before = project.clone();

    // (Nor is the colour's place: letting go of what is not read from a mask is the same for every place that is there.)
    assert_eq!(project.set_driven_value(owner, instance, AMOUNT, None), Ok(()));
    assert_eq!(project, before);
}

#[test]
fn an_effect_that_is_not_there_and_a_track_that_is_not_there_are_refused_and_nothing_is_changed() {
    let (mut project, [_, middle, _], [clip, ..]) = three_tracks();
    let (owner, instance) = with_a_shader_on(&mut project, clip);
    let before = project.clone();

    let no_such_effect = project.set_driven_value(
        owner,
        EffectInstanceId::new(99),
        AMOUNT,
        from(middle, MaskChannel::Red, 0.0),
    );
    let no_such_track =
        project.set_driven_value(owner, instance, AMOUNT, from(TrackId::new(999), MaskChannel::Red, 0.0));
    let not_a_number = project.set_driven_value(owner, instance, AMOUNT, from(middle, MaskChannel::Red, f32::NAN));

    assert!(
        matches!(no_such_effect, Err(EditError::NotInTheChain { .. })),
        "{no_such_effect:?}"
    );
    assert_eq!(
        no_such_track,
        Err(EditError::UnknownTrack {
            track: TrackId::new(999)
        })
    );
    assert!(
        matches!(not_a_number, Err(EditError::ValueCannotBeDriven { .. })),
        "{not_a_number:?}"
    );
    assert_eq!(project, before);
}

#[test]
fn a_track_cannot_be_the_mask_of_an_effect_on_its_own_picture() {
    let (mut project, [front, ..], [clip, ..]) = three_tracks();
    let (owner, instance) = with_a_shader_on(&mut project, clip);
    let before = project.clone();

    let result = project.set_driven_value(owner, instance, AMOUNT, from(front, MaskChannel::Red, 0.0));

    assert_eq!(result, Err(EditError::MaskWouldReadItself { track: front }));
    assert_eq!(project, before);
    // An effect of the track itself is done to the picture of each of its clips: the same.
    let on_the_track = project.add_effect(AnimationTarget::Track(front), tint()).unwrap();
    assert_eq!(
        project.set_driven_value(
            Member::Track(front),
            on_the_track,
            AMOUNT,
            from(front, MaskChannel::Red, 0.0)
        ),
        Err(EditError::MaskWouldReadItself { track: front })
    );
}

#[test]
fn tracks_cannot_read_each_other_in_a_ring_of_any_size() {
    let (mut project, [first, second, third], [on_first, on_second, on_third]) = three_tracks();
    let (owner_first, instance_first) = with_a_shader_on(&mut project, on_first);
    let (owner_second, instance_second) = with_a_shader_on(&mut project, on_second);
    let (owner_third, instance_third) = with_a_shader_on(&mut project, on_third);

    // The first is made from the second, and the second from the third: a line, and not a ring.
    project
        .set_driven_value(owner_first, instance_first, AMOUNT, from(second, MaskChannel::Red, 0.0))
        .unwrap();
    project
        .set_driven_value(
            owner_second,
            instance_second,
            AMOUNT,
            from(third, MaskChannel::Red, 0.0),
        )
        .unwrap();

    // The third made from the second is a ring of two; made from the first, a ring of three.
    assert_eq!(
        project.set_driven_value(owner_third, instance_third, AMOUNT, from(second, MaskChannel::Red, 0.0)),
        Err(EditError::MaskWouldReadItself { track: second })
    );
    assert_eq!(
        project.set_driven_value(owner_third, instance_third, AMOUNT, from(first, MaskChannel::Red, 0.0)),
        Err(EditError::MaskWouldReadItself { track: first })
    );
    // The second made from the first is a ring of two as well.
    assert_eq!(
        project.set_driven_value(
            owner_second,
            instance_second,
            STRENGTH,
            from(first, MaskChannel::Red, 0.0)
        ),
        Err(EditError::MaskWouldReadItself { track: first })
    );
}

#[test]
fn two_tracks_may_read_the_same_track_and_a_track_may_be_read_by_many_effects() {
    let (mut project, [first, second, third], [on_first, on_second, _]) = three_tracks();
    let (owner_first, instance_first) = with_a_shader_on(&mut project, on_first);
    let (owner_second, instance_second) = with_a_shader_on(&mut project, on_second);

    assert_eq!(
        project.set_driven_value(owner_first, instance_first, AMOUNT, from(third, MaskChannel::Red, 0.0)),
        Ok(())
    );
    assert_eq!(
        project.set_driven_value(
            owner_second,
            instance_second,
            AMOUNT,
            from(third, MaskChannel::Blue, 0.5)
        ),
        Ok(())
    );
    // (The first reads the second, which reads the third, and so does the first itself: a diamond, which is no ring.)
    assert_eq!(
        project.set_driven_value(
            owner_first,
            instance_first,
            STRENGTH,
            from(second, MaskChannel::Red, 3.0)
        ),
        Ok(())
    );
    assert_eq!(first, project.tracks()[0].id());
}

#[test]
fn an_effect_of_a_track_is_done_to_each_of_its_clips_and_makes_the_ring_too() {
    let (mut project, [first, second, _], [_, on_second, _]) = three_tracks();
    let on_the_first_track = project.add_effect(AnimationTarget::Track(first), tint()).unwrap();
    project
        .set_driven_value(
            Member::Track(first),
            on_the_first_track,
            AMOUNT,
            from(second, MaskChannel::Red, 0.0),
        )
        .unwrap();
    let (owner, instance) = with_a_shader_on(&mut project, on_second);

    // The picture of the first track is made through the second now; the second cannot be made from it.
    assert_eq!(
        project.set_driven_value(owner, instance, AMOUNT, from(first, MaskChannel::Red, 0.0)),
        Err(EditError::MaskWouldReadItself { track: first })
    );
}

#[test]
fn an_effect_that_is_switched_off_still_counts_in_a_ring_so_that_switching_it_on_cannot_make_one() {
    let (mut project, [first, second, _], [on_first, on_second, _]) = three_tracks();
    let (owner_first, instance_first) = with_a_shader_on(&mut project, on_first);
    project
        .set_driven_value(owner_first, instance_first, AMOUNT, from(second, MaskChannel::Red, 0.0))
        .unwrap();
    project.set_effect_enabled(owner_first, instance_first, false).unwrap();
    let (owner_second, instance_second) = with_a_shader_on(&mut project, on_second);

    assert_eq!(
        project.set_driven_value(
            owner_second,
            instance_second,
            AMOUNT,
            from(first, MaskChannel::Red, 0.0)
        ),
        Err(EditError::MaskWouldReadItself { track: first })
    );
}

#[test]
fn deleting_a_track_lets_go_of_every_value_that_was_read_from_it_and_of_no_other() {
    let (mut project, [first, second, third], [on_first, on_second, _]) = three_tracks();
    let (owner_first, instance_first) = with_a_shader_on(&mut project, on_first);
    let (owner_second, instance_second) = with_a_shader_on(&mut project, on_second);
    let on_the_second_track = project.add_effect(AnimationTarget::Track(second), tint()).unwrap();
    project
        .set_driven_value(owner_first, instance_first, AMOUNT, from(third, MaskChannel::Red, 0.1))
        .unwrap();
    project
        .set_driven_value(
            owner_first,
            instance_first,
            STRENGTH,
            from(second, MaskChannel::Green, 4.0),
        )
        .unwrap();
    project
        .set_driven_value(
            owner_second,
            instance_second,
            AMOUNT,
            from(third, MaskChannel::Blue, 0.2),
        )
        .unwrap();
    project
        .set_driven_value(
            Member::Track(second),
            on_the_second_track,
            STRENGTH,
            from(third, MaskChannel::Red, 5.0),
        )
        .unwrap();
    let _ = first;

    project.delete_track(third).unwrap();

    assert_eq!(
        driven_of(&project, owner_first, instance_first, AMOUNT),
        None,
        "read from the third"
    );
    assert_eq!(
        driven_of(&project, owner_first, instance_first, STRENGTH).map(|driven| driven.source),
        Some(MaskSource::Track(second)),
        "read from the second, which is still there"
    );
    assert_eq!(
        driven_of(&project, owner_second, instance_second, AMOUNT),
        None,
        "read from the third"
    );
    assert_eq!(
        driven_of(&project, Member::Track(second), on_the_second_track, STRENGTH),
        None,
        "the effect of a track is let go of too"
    );
}

#[test]
fn a_clip_that_is_copied_keeps_the_values_that_are_read_from_masks() {
    let (mut project, [_, middle, _], [clip, ..]) = three_tracks();
    let (owner, instance) = with_a_shader_on(&mut project, clip);
    project
        .set_driven_value(owner, instance, STRENGTH, from(middle, MaskChannel::Brightness, 5.5))
        .unwrap();

    let copy = project.chain_of_the_member(owner).unwrap().clone();

    let effect = copy.iter().next().unwrap();
    assert_eq!(
        effect.driven_values().of(STRENGTH).copied(),
        Some(DrivenValue::from_a_track(middle, MaskChannel::Brightness, 5.5))
    );
}

// --- a project that is made from its parts (as when a file is loaded) ---

/// The chain of a clip that has a shader with its first number read from the track `track`.
fn a_chain_reading(track: TrackId) -> Effects {
    let driven = DrivenValues::from_places(vec![(0, DrivenValue::from_a_track(track, MaskChannel::Red, 0.0))]).unwrap();
    Effects::from_entries([ChainEntry::Effect(Effect::new(tint()).with_driven_values(driven))]).unwrap()
}

fn two_tracks_from_parts(
    first_reads: Option<TrackId>,
    second_reads: Option<TrackId>,
) -> Result<Project, InconsistentProject> {
    let with_effects = |id: u32, reads: Option<TrackId>| {
        let track = super::tests::track_with(id, vec![(0, video_clip(1, 0, 100))]);
        match reads {
            Some(reads) => track.with_effects_on_its_clips([a_chain_reading(reads)]),
            None => track,
        }
    };
    Project::from_parts(
        vec![super::tests::video_source(1, 30, 1000)],
        vec![with_effects(0, first_reads), with_effects(1, second_reads)],
        SpeedIndicatorLayout::DEFAULT,
    )
}

#[test]
fn a_project_whose_values_are_read_from_tracks_it_has_and_not_from_each_other_is_consistent() {
    assert!(two_tracks_from_parts(None, None).is_ok());
    assert!(two_tracks_from_parts(Some(TrackId::new(1)), None).is_ok());
    assert!(two_tracks_from_parts(None, Some(TrackId::new(0))).is_ok());
}

#[test]
fn a_value_read_from_a_track_that_the_project_does_not_have_is_refused_when_a_project_is_made_from_parts() {
    assert_eq!(
        two_tracks_from_parts(Some(TrackId::new(9)), None).unwrap_err(),
        InconsistentProject::ValueReadFromAnUnknownTrack { track: TrackId::new(9) }
    );
}

#[test]
fn tracks_that_read_each_other_are_refused_when_a_project_is_made_from_parts() {
    // A track that reads itself, and two that read each other.
    assert!(matches!(
        two_tracks_from_parts(Some(TrackId::new(0)), None).unwrap_err(),
        InconsistentProject::TracksAreMadeFromEachOther { track } if track == TrackId::new(0)
    ));
    assert!(matches!(
        two_tracks_from_parts(Some(TrackId::new(1)), Some(TrackId::new(0))).unwrap_err(),
        InconsistentProject::TracksAreMadeFromEachOther { .. }
    ));
}
