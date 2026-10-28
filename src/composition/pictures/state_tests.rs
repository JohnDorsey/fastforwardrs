// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the picture plan makes of a shader that depends on earlier frames (phase 6.4.2): its operation says which state it goes on with and at which frame of its clock, no two
//! effects of a frame have one state, and a layer that has such an effect is not left out because something in front of it hides it (it would not go on, and would start clear when it
//! is seen again).

use super::tests::{frame, long_video, plan, text};
use super::*;
use crate::composition::test_projects::project_of;
use crate::project::{EffectKind, GroupKind};

/// A shader with a buffer that lasts: it keeps what it is given and gives it back.
const MEMORY: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ], \"PASSES\": [ { \"TARGET\": \"memory\", \"PERSISTENT\": true }, {} ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(memory); }\n";

/// A shader with no memory.
const NO_MEMORY: &str = "/*{ \"INPUTS\": [ { \"NAME\": \"inputImage\", \"TYPE\": \"image\" } ] }*/\nvoid main() { gl_FragColor = IMG_THIS_PIXEL(inputImage); }\n";

fn kind_of(name: &str, source: &str) -> EffectKind {
    let file = crate::isf::read(source).unwrap();
    EffectKind::from_snapshot(crate::isf::descriptor_of(&file, name, source)).unwrap()
}

/// The states, `(id, frame)`, of the shaders among `operations`, in order (`None` for one that has none).
fn states_of(operations: &[PictureOperation]) -> Vec<Option<(u64, u64)>> {
    operations
        .iter()
        .filter_map(|operation| match operation {
            PictureOperation::Shader { state, .. } => Some(*state),
            _ => None,
        })
        .collect()
}

#[test]
fn a_shader_that_depends_on_earlier_frames_says_which_state_it_goes_on_with_and_the_frame_of_its_own_clock() {
    let mut project = project_of(vec![vec![(10, long_video(0, 60, 1))]]);
    let clip = project.tracks()[0].placement_at_index(0).unwrap().id;
    let target = AnimationTarget::Clip(clip);
    let remembers = kind_of("memory.fs", MEMORY);
    assert!(remembers.depends_on_earlier_frames());
    let forgets = kind_of("forgets.fs", NO_MEMORY);
    assert!(!forgets.depends_on_earlier_frames());
    project.add_effect(target, remembers).unwrap();
    project.add_effect(target, forgets).unwrap();
    let plan = plan(&project);
    let layer = plan.sections.iter().flat_map(|section| &section.layers).next().unwrap();

    let at = |timeline_frame: u64| states_of(&layer.operations_at(frame(timeline_frame)));

    let first = at(10);
    let Some((id, 0)) = first[0] else {
        panic!("the frame of its clock is 0 when the clip begins: {first:?}");
    };
    assert_eq!(first[1], None, "a shader with no memory has no state");
    // The same state at every frame, and the frame is the frames since the clip began.
    for since in [1u64, 2, 7, 59] {
        let now = at(10 + since);
        assert_eq!(now[0], Some((id, since)), "{since} frames in");
        assert_eq!(now[1], None);
    }
}

#[test]
fn the_state_of_an_effect_is_not_the_state_of_another_clip_or_place_in_a_chain_or_the_effect_of_a_track() {
    let mut project = project_of(vec![vec![(0, long_video(0, 60, 1))], vec![(0, long_video(0, 60, 1))]]);
    let front = project.tracks()[0].placement_at_index(0).unwrap().id;
    let back = project.tracks()[1].placement_at_index(0).unwrap().id;
    let front_track = project.tracks()[0].id();
    let kind = kind_of("memory.fs", MEMORY);
    // The same effect twice on the front clip, once on the back clip, and once on the front track (which is done to the front clip too).
    project.add_effect(AnimationTarget::Clip(front), kind).unwrap();
    project.add_effect(AnimationTarget::Clip(front), kind).unwrap();
    project.add_effect(AnimationTarget::Clip(back), kind).unwrap();
    project.add_effect(AnimationTarget::Track(front_track), kind).unwrap();
    let plan = plan(&project);
    let layers: Vec<&Layer> = plan.sections.iter().flat_map(|section| &section.layers).collect();
    assert_eq!(layers.len(), 2);

    let ids: Vec<u64> = layers
        .iter()
        .flat_map(|layer| states_of(&layer.operations_at(frame(5))))
        .map(|state| state.expect("every one of them depends on earlier frames").0)
        .collect();

    // (The front layer has the clip's two and the track's one; the back layer has its one.)
    assert_eq!(ids.len(), 4, "{ids:?}");
    for (place, id) in ids.iter().enumerate() {
        for other in &ids[place + 1..] {
            assert_ne!(id, other, "{ids:?}");
        }
    }
}

#[test]
fn the_effect_of_a_tag_has_a_state_for_each_clip_it_is_done_to() {
    let mut project = project_of(vec![vec![(0, long_video(0, 60, 1))], vec![(0, text(60))]]);
    let first = project.tracks()[0].placement_at_index(0).unwrap().id;
    let second = project.tracks()[1].placement_at_index(0).unwrap().id;
    let tag = project
        .create_group(GroupKind::Tag, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    project
        .add_effect_to_the_group(tag, kind_of("memory.fs", MEMORY))
        .unwrap();
    let plan = plan(&project);

    let ids: Vec<u64> = plan
        .sections
        .iter()
        .flat_map(|section| &section.layers)
        .map(|layer| states_of(&layer.operations_at(frame(5)))[0].expect("it has a state").0)
        .collect();

    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1], "one effect of a tag, done to two clips, is two states");
}

#[test]
fn the_state_of_the_effect_of_a_clip_group_is_its_own_and_its_clock_is_the_timelines() {
    let mut project = project_of(vec![vec![(7, long_video(0, 60, 1))], vec![(7, text(60))]]);
    let first = project.tracks()[0].placement_at_index(0).unwrap().id;
    let second = project.tracks()[1].placement_at_index(0).unwrap().id;
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    project
        .add_effect_to_the_group(group, kind_of("memory.fs", MEMORY))
        .unwrap();
    let plan = plan(&project);
    let span = plan.sections.iter().flat_map(|section| &section.groups).next().unwrap();

    let at = |timeline_frame: u64| states_of(&span.operations_at(frame(timeline_frame)));

    let Some((id, 12)) = at(12)[0] else {
        panic!(
            "the frame of a group's clock is the frame of the timeline: {:?}",
            at(12)
        );
    };
    assert_eq!(at(20)[0], Some((id, 20)));
    // (Not the state of anything that a clip's layer has.)
    for layer in plan.sections.iter().flat_map(|section| &section.layers) {
        assert!(states_of(&layer.operations_at(frame(12))).is_empty());
    }
}

#[test]
fn a_layer_that_has_an_effect_that_depends_on_earlier_frames_is_not_left_out_when_what_is_in_front_hides_it() {
    // A video that covers the picture in front of a text object: the text is behind what hides it, and is left out.
    let hidden = |project: &Project| -> usize {
        plan(project)
            .sections
            .iter()
            .map(|section| section.layers.len())
            .max()
            .unwrap()
    };
    let mut project = project_of(vec![vec![(0, long_video(0, 60, 1))], vec![(0, text(60))]]);
    assert_eq!(hidden(&project), 1, "the text is hidden and not a layer");

    let text_clip = project.tracks()[1].placement_at_index(0).unwrap().id;
    project
        .add_effect(AnimationTarget::Clip(text_clip), kind_of("forgets.fs", NO_MEMORY))
        .unwrap();
    assert_eq!(hidden(&project), 1, "an effect with no memory does not keep it");

    project
        .add_effect(AnimationTarget::Clip(text_clip), kind_of("memory.fs", MEMORY))
        .unwrap();
    assert_eq!(hidden(&project), 2, "the text is a layer, that is drawn and covered");
    let layers = &plan(&project).sections[0].layers;
    assert!(matches!(layers[0].content, LayerContent::Text(_)), "behind, as it was");
    assert!(layers[1].video().is_some());
}

#[test]
fn two_clips_one_after_the_other_on_a_track_and_two_clip_groups_have_states_of_their_own() {
    // Two clips on one track, one after the other, each with the effect.
    let mut project = project_of(vec![vec![(0, long_video(0, 30, 1)), (0, long_video(30, 60, 1))]]);
    let first = project.tracks()[0].placement_at_index(0).unwrap().id;
    let second = project.tracks()[0].placement_at_index(1).unwrap().id;
    let kind = kind_of("memory.fs", MEMORY);
    project.add_effect(AnimationTarget::Clip(first), kind).unwrap();
    project.add_effect(AnimationTarget::Clip(second), kind).unwrap();
    let plan = plan(&project);
    let ids: Vec<u64> = plan
        .sections
        .iter()
        .flat_map(|section| &section.layers)
        .map(|layer| states_of(&layer.operations_at(frame(5)))[0].unwrap().0)
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(
        ids[0], ids[1],
        "the clips are on one track and the effects are in one place of their chains"
    );

    // Two clip groups, each of two clips on two tracks, each with the effect (the text objects do not hide each other).
    let mut project = project_of(vec![
        vec![(0, text(60))],
        vec![(0, text(60))],
        vec![(0, text(60))],
        vec![(0, text(60))],
    ]);
    let clips: Vec<ClipId> = (0..4)
        .map(|track| project.tracks()[track].placement_at_index(0).unwrap().id)
        .collect();
    for pair in clips.chunks(2) {
        let group = project
            .create_group(GroupKind::ClipGroup, &[Member::Clip(pair[0]), Member::Clip(pair[1])])
            .unwrap();
        project.add_effect_to_the_group(group, kind).unwrap();
    }
    let plan = super::tests::plan(&project);
    let spans: Vec<&GroupSpan> = plan.sections.iter().flat_map(|section| &section.groups).collect();
    assert_eq!(spans.len(), 2);
    let ids: Vec<u64> = spans
        .iter()
        .map(|span| states_of(&span.operations_at(frame(5)))[0].unwrap().0)
        .collect();
    assert_ne!(
        ids[0], ids[1],
        "two groups, each with the effect in the same place of its chain"
    );
}

#[test]
fn a_layer_with_such_an_effect_inside_a_group_that_is_hidden_is_kept_with_its_group() {
    // A video in front that covers the picture, and behind it a clip group of two text objects, which has an effect with no memory (so that the group makes a picture of its
    // own); one of the texts has the effect that remembers.
    let mut project = project_of(vec![
        vec![(0, long_video(0, 60, 1))],
        vec![(0, text(60))],
        vec![(0, text(60))],
    ]);
    let remembering = project.tracks()[1].placement_at_index(0).unwrap().id;
    let other = project.tracks()[2].placement_at_index(0).unwrap().id;
    let group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(remembering), Member::Clip(other)])
        .unwrap();
    project
        .add_effect_to_the_group(group, kind_of("forgets.fs", NO_MEMORY))
        .unwrap();
    let layers = |project: &Project| {
        super::tests::plan(project)
            .sections
            .iter()
            .map(|section| section.layers.len())
            .max()
            .unwrap()
    };
    assert_eq!(
        layers(&project),
        1,
        "the group is behind what covers the picture and is left out"
    );
    project
        .add_effect(AnimationTarget::Clip(remembering), kind_of("memory.fs", MEMORY))
        .unwrap();
    assert_eq!(
        layers(&project),
        3,
        "the group is kept, and what is in it, for the one that remembers"
    );
}

// --- phase 12, piece 12.5: the state of an effect of a track is shared by the clips of a run, or by the whole track ---

/// A state, `(id, frame)`, if the effect has one.
type State = Option<(u64, u64)>;

/// Two clips on one track, one right after the other (frames 0 to 30 and 30 to 60), with the effect that remembers on the track, planned: the state `(id, frame)` that the effect has at the
/// frame `first` of the first clip and at the frame `second` of the second.
fn the_states_of_the_effect_of_a_track_at(
    sharing: crate::project::TrackSharing,
    gap: u64,
    first: u64,
    second: u64,
) -> (State, State) {
    let mut project = project_of(vec![vec![(0, long_video(0, 30, 1)), (gap, long_video(30, 60, 1))]]);
    let track = project.tracks()[0].id();
    project
        .add_effect(AnimationTarget::Track(track), kind_of("memory.fs", MEMORY))
        .unwrap();
    project
        .set_track_sharing(track, crate::project::EffectDomain::Picture, sharing)
        .unwrap();
    states_at_two_frames(&project, first, second)
}

fn states_at_two_frames(project: &Project, first: u64, second: u64) -> (State, State) {
    let plan = plan(project);
    let at = |timeline_frame: u64| {
        let section = plan
            .section_at(frame(timeline_frame))
            .expect("the frame is in the project");
        // (The layer of the clip that has the effect: the others have none.)
        section.layers.iter().find_map(|layer| {
            states_of(&layer.operations_at(frame(timeline_frame)))
                .first()
                .copied()
                .flatten()
        })
    };
    (at(first), at(second))
}

#[test]
fn the_state_of_an_effect_of_a_track_goes_on_from_a_clip_to_the_one_that_follows_it_at_once() {
    use crate::project::TrackSharing;
    let (last_of_the_first, first_of_the_second) =
        the_states_of_the_effect_of_a_track_at(TrackSharing::PerRun, 0, 29, 30);
    let (id, 29) = last_of_the_first.expect("it has a state") else {
        panic!("the frame of the clock of an effect of a track is the frame of the timeline: {last_of_the_first:?}");
    };
    assert_eq!(
        first_of_the_second,
        Some((id, 30)),
        "the next frame of the same state, so it goes on"
    );
}

#[test]
fn after_a_gap_the_next_clip_begins_a_run_of_its_own_and_so_it_does_where_there_is_one_copy_for_each_clip() {
    use crate::project::TrackSharing;
    for (sharing, gap) in [
        (TrackSharing::PerRun, 5),
        (TrackSharing::PerClip, 0),
        (TrackSharing::PerClip, 5),
    ] {
        let (first, second) = the_states_of_the_effect_of_a_track_at(sharing, gap, 5, 40);
        assert_ne!(
            first.expect("it has a state").0,
            second.expect("it has a state").0,
            "{sharing:?}, a gap of {gap}: two states"
        );
    }
}

#[test]
fn with_one_instance_for_the_track_all_its_clips_share_the_state_whatever_is_between_them() {
    use crate::project::TrackSharing;
    for sharing in [TrackSharing::PerTrackWithWindows, TrackSharing::PerTrackIndivisible] {
        let (first, second) = the_states_of_the_effect_of_a_track_at(sharing, 5, 5, 40);
        assert_eq!(
            first.expect("it has a state").0,
            second.expect("it has a state").0,
            "{sharing:?}"
        );
    }
}

#[test]
fn the_edge_of_a_group_that_makes_a_picture_of_its_own_ends_a_run_and_that_of_one_that_does_not_does_not() {
    use crate::project::TrackSharing;
    // The track has two clips one right after the other, the first of which is in a clip group with a clip of another track (and the other is not).
    let build = |group_has_an_effect: bool| {
        let mut project = project_of(vec![
            vec![(0, long_video(0, 30, 1)), (0, long_video(30, 60, 1))],
            vec![(0, text(30))],
        ]);
        let track = project.tracks()[0].id();
        project
            .add_effect(AnimationTarget::Track(track), kind_of("memory.fs", MEMORY))
            .unwrap();
        assert_eq!(
            project.tracks()[0].sharing().picture,
            TrackSharing::PerRun,
            "a premise: the default"
        );
        let (first, below) = (
            project.tracks()[0].placement_at_index(0).unwrap().id,
            project.tracks()[1].placement_at_index(0).unwrap().id,
        );
        let group = project
            .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(below)])
            .unwrap();
        if group_has_an_effect {
            project
                .add_effect_to_the_group(group, kind_of("forgets.fs", NO_MEMORY))
                .unwrap();
        }
        states_at_two_frames(&project, 5, 40)
    };

    let (first, second) = build(true);
    assert_ne!(
        first.expect("it has a state").0,
        second.expect("it has a state").0,
        "the group makes a picture of its own: where it ends the track's effect has a copy that begins"
    );
    let (first, second) = build(false);
    assert_eq!(
        first.expect("it has a state").0,
        second.expect("it has a state").0,
        "a group that has no effect is not there, as far as the picture goes"
    );
}
