// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the sound plan makes of the effects of tracks and groups **today**, written down before phase 12 changes it (docs/engine-phase-12-design.md, section 1): an effect of a track is
//! stamped into the chain of **each** clip of the track (until piece 12.2, which gave a run of clips one instance of them: see `buses_tests.rs`, and the sharing *one copy for each clip*, which is
//! still this); and a clip group or a track group has no sound effect. When phase 12 gives groups sound effects (12.3) the second of these tests is the one that changes, on purpose.

use super::*;
use crate::composition::plan_pictures;
use crate::composition::test_projects::{LONG_VIDEO, SOUND_FILE, media_clip, project_of};
use crate::project::tracks::Clip;
use crate::project::{EffectKind, GroupKind, InstalledPlugin, Member, set_installed_plugins};
use crate::test_support::{an_installed_sound_plugin, the_installed_plugins_to_myself};

fn plan(project: &Project) -> SoundPlan {
    plan_sound(project, TimelineFrameIndex::new(0), |_| true)
}

fn clips_of(plan: &SoundPlan) -> Vec<&SoundClip> {
    plan.strips
        .iter()
        .flat_map(|strip| &strip.items)
        .filter_map(|item| match item {
            SoundItem::Clip(clip) => Some(clip),
            SoundItem::Silence { .. } => None,
        })
        .collect()
}

fn a_sound_effect() -> (InstalledPlugin, EffectKind) {
    let plugin = an_installed_sound_plugin("ladspa:a:tails", "Maker", &[("gain", 1.0, Some((0.0, 4.0)))]);
    set_installed_plugins(vec![plugin.clone()]);
    (plugin, EffectKind::installed_plugin_kinds()[0])
}

/// Two sound clips one right after the other on one track (the number before a clip is the gap before it), and a third on a track of its own.
fn a_track_of_two_clips_and_another_of_one() -> Project {
    project_of(vec![
        vec![
            (0, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
            (0, Clip::Audio(media_clip(SOUND_FILE, 0, 30, 1))),
        ],
        vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 60, 1)))],
    ])
}

fn clip_at(project: &Project, track: usize, index: usize) -> crate::project::ClipId {
    project.tracks()[track].placement_at_index(index).unwrap().id
}

#[test]
fn a_clip_group_and_a_track_group_can_have_a_sound_effect_that_is_not_in_the_chain_of_any_clip_and_a_tag_has_a_copy_in_the_chain_of_each_member()
 {
    // (Rewritten on purpose in phase 12, piece 12.3: a sound effect was refused on a clip group and a track group, which made G-15 hold by refusal; now it is done to the sum of what the
    // members make, in a group of the plan: what is locked here is that it is still in the chain of **no clip**, as a tag's is in the chain of each member.)
    let _alone = the_installed_plugins_to_myself();
    let (_, kind) = a_sound_effect();
    let mut project = a_track_of_two_clips_and_another_of_one();
    let (first, second) = (clip_at(&project, 0, 0), clip_at(&project, 0, 1));
    let track = project.tracks()[0].id();
    let clip_group = project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    let track_group = project
        .create_group(GroupKind::TrackGroup, &[Member::Track(track)])
        .unwrap();

    for group in [clip_group, track_group] {
        project
            .add_effect_to_the_group(group, kind)
            .expect("a group is given the sum of its members' sound");
    }
    let with_groups = plan(&project);
    assert!(
        clips_of(&with_groups).iter().all(|clip| clip.effects.is_empty()),
        "the effects of a clip group or a track group are not applied to each member"
    );
    assert_eq!(with_groups.groups.len(), 2, "they are two groups of the plan");

    let tag = project
        .create_group(GroupKind::Tag, &[Member::Clip(first), Member::Clip(second)])
        .unwrap();
    project
        .add_effect_to_the_group(tag, kind)
        .expect("a tag can have any effect");
    let plan = plan(&project);
    let clips = clips_of(&plan);
    assert_eq!(
        (clips[0].effects.len(), clips[1].effects.len(), clips[2].effects.len()),
        (1, 1, 0),
        "each member of the tag has its own copy; the clip that is not in it has none"
    );
}

#[test]
fn nothing_that_is_heard_or_seen_changes_when_a_sharing_or_a_tail_policy_does_yet() {
    let mut project = project_of(vec![
        vec![
            (0, Clip::Video(media_clip(LONG_VIDEO, 0, 60, 1))),
            (0, Clip::Video(media_clip(LONG_VIDEO, 0, 30, 1))),
        ],
        vec![(0, Clip::Audio(media_clip(SOUND_FILE, 0, 90, 1)))],
    ]);
    let (first, second) = (
        project.tracks()[0].placement_at_index(0).unwrap().id,
        project.tracks()[0].placement_at_index(1).unwrap().id,
    );
    let (track, sound_track) = (project.tracks()[0].id(), project.tracks()[1].id());
    let group = project
        .create_group(
            crate::project::GroupKind::ClipGroup,
            &[
                crate::project::Member::Clip(first),
                crate::project::Member::Clip(second),
            ],
        )
        .unwrap();
    let sound_before = format!("{:?}", plan_sound(&project, TimelineFrameIndex::new(0), |_| true));
    let pictures_before = format!("{:?}", plan_pictures(&project, |_| true));

    for sharing in crate::project::TrackSharing::ALL {
        project
            .set_track_sharing(track, crate::project::EffectDomain::Picture, sharing)
            .unwrap();
        project
            .set_track_sharing(sound_track, crate::project::EffectDomain::Sound, sharing)
            .unwrap();
    }
    project
        .set_clip_tail_policy(
            first,
            crate::project::ClipTailPolicy {
                edge: crate::project::ClipTailEdge::Into,
                depth: crate::project::TailDepth::InFront,
            },
        )
        .unwrap();
    project
        .set_group_tail_policy(
            group,
            crate::project::GroupTailPolicy {
                edge: crate::project::GroupTailEdge::Release,
                depth: crate::project::TailDepth::InFront,
            },
        )
        .unwrap();

    assert_eq!(
        format!("{:?}", plan_sound(&project, TimelineFrameIndex::new(0), |_| true)),
        sound_before
    );
    assert_eq!(format!("{:?}", plan_pictures(&project, |_| true)), pictures_before);
}
