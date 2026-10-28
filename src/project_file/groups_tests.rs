// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of how groups, and the badges in the chains of effects, are saved in a project file and read back, and what is refused when a file
//! says something that cannot be.

use super::tests::{
    assert_same_content, edited_project, link_to_missing_video, project_with_everything, saved, version_line,
};
use super::*;
use crate::project::{
    Badge, ChainItemKey, GroupColour, GroupKind, Member, PropertyId, RackDestination, TimelineFrameIndex,
};
use crate::test_support::HUE_SHIFT_AS_THE_FIRST_EFFECT as HUE;

fn turn(degrees: f32) -> PropertyValue {
    PropertyValue::Number(degrees)
}

fn hue_shift() -> ChainItemKey {
    ChainItemKey::Effect(crate::project::EffectInstanceId::FIRST)
}

fn tag_badge(group: crate::project::GroupId) -> ChainItemKey {
    ChainItemKey::Badge(Badge::Tag(group))
}

/// `project_with_everything` (three tracks: the captions, with a text object; the music, with a sound; and the videos, with three clips),
/// with groups of every kind in it, a group in each kind of group, badges that are not where they begin and effects of groups:
///
/// * group 0, a tag called "titles and sound": the text object, the sound, the first video and a keyframe crate; it has a hue shift of 45;
/// * group 1, a clip group: the first two videos; it has a hue shift as it begins;
/// * group 2, a track group: the captions track and the music track;
/// * group 3, a tag that has the other three groups in it (and a hue shift turned back a twelfth).
pub(super) fn a_project_with_groups_of_every_kind() -> Project {
    let mut project = project_with_everything();
    let (captions, music) = (project.tracks()[0].id(), project.tracks()[1].id());
    let text = project.tracks()[0].entries()[0].id();
    let sound = project.tracks()[1].entries()[0].id();
    let (first_video, second_video) = (
        project.tracks()[2].entries()[0].id(),
        project.tracks()[2].entries()[1].id(),
    );
    // (A keyframe crate that has no keyframes in it does not stay, so this one is made as it is when it is made by the person using the
    // editor: by taking a rack out of the drawer of the first video, and it lasts a frame, where its keyframe is, before the text object.)
    let rack = project
        .add_keyframe(
            AnimationTarget::Clip(first_video),
            PropertyId::Rotation,
            TimelineFrameIndex::new(10),
        )
        .unwrap()
        .unwrap()
        .rack;
    let before_the_text = RackDestination::NewCrate {
        track: captions,
        from_frame: 0,
        until_frame: 100,
    };
    project.move_racks(&[rack], before_the_text, 0, None).unwrap();
    let a_crate = project.tracks()[0].entries()[0].id();

    let tag = project
        .create_group(
            GroupKind::Tag,
            &[
                Member::Clip(text),
                Member::Clip(sound),
                Member::Clip(first_video),
                Member::Clip(a_crate),
            ],
        )
        .unwrap();
    project.rename_group(tag, "titles and sound").unwrap();
    project.add_effect_to_the_group(tag, EffectKind::HUE_SHIFT).unwrap();
    project.set_effect_value_of_the_group(tag, HUE, &turn(45.0)).unwrap();

    let clip_group = project
        .create_group(
            GroupKind::ClipGroup,
            &[Member::Clip(first_video), Member::Clip(second_video)],
        )
        .unwrap();
    project.set_group_colour(clip_group, GroupColour::new(1, 2, 3)).unwrap();
    project
        .add_effect_to_the_group(clip_group, EffectKind::HUE_SHIFT)
        .unwrap();

    let track_group = project
        .create_group(GroupKind::TrackGroup, &[Member::Track(captions), Member::Track(music)])
        .unwrap();
    let outer = project
        .create_group(
            GroupKind::Tag,
            &[
                Member::Group(clip_group),
                Member::Group(track_group),
                Member::Group(tag),
            ],
        )
        .unwrap();
    project.add_effect_to_the_group(outer, EffectKind::HUE_SHIFT).unwrap();
    project.set_effect_value_of_the_group(outer, HUE, &turn(-30.0)).unwrap();

    // The first video has its effect and then the badge of the tag: put the badge first. The second video's badge of its track, which
    // begins at the end of its chain, is put first too. The clip group has the badge of the outer tag and its effect: put the effect first.
    project
        .move_chain_item(Member::Clip(first_video), tag_badge(tag), Some(hue_shift()))
        .unwrap();
    project
        .move_chain_item(
            Member::Clip(second_video),
            ChainItemKey::Badge(Badge::Track),
            Some(hue_shift()),
        )
        .unwrap();
    project
        .move_chain_item(Member::Group(clip_group), tag_badge(outer), None)
        .unwrap();
    assert_eq!(project.problems_with_the_groups(), []);
    project
}

fn file_of_the_project_with_groups() -> serde_json::Value {
    serde_json::from_str(&saved(&a_project_with_groups_of_every_kind())).unwrap()
}

fn text_of_the_file_with(change: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut file = file_of_the_project_with_groups();
    change(&mut file);
    serde_json::to_string_pretty(&file).unwrap()
}

fn reason_it_is_refused(text: &str) -> String {
    match project_from_json_text(text) {
        Err(error) => error.to_string(),
        Ok(_) => panic!("it was not refused"),
    }
}

// --- what the file says ---

#[test]
fn a_group_is_saved_with_its_kind_name_and_colour_and_what_is_in_it_by_where_it_is() {
    let file = file_of_the_project_with_groups();
    let groups = file["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 4);

    assert_eq!(groups[0]["kind"], "tag");
    assert_eq!(groups[0]["name"], "titles and sound");
    assert_eq!(
        groups[0]["members"],
        serde_json::json!([
            {"clip": {"track": 0, "clip": 1}},
            {"clip": {"track": 1, "clip": 0}},
            {"clip": {"track": 2, "clip": 0}},
            {"clip": {"track": 0, "clip": 0}}
        ]),
        "the text object is the second clip of its track (the crate is before it), and the crate is the first"
    );
    assert_eq!(
        groups[0]["effects"],
        serde_json::json!([{"effect": "builtin:hue_shift", "version": [1, 1], "instance": 0, "values": {"degrees": 45.0}}])
    );

    assert_eq!(groups[1]["kind"], "clip_group");
    assert_eq!(groups[1]["colour"], "#010203");
    assert_eq!(
        groups[1]["members"],
        serde_json::json!([{"clip": {"track": 2, "clip": 0}}, {"clip": {"track": 2, "clip": 1}}])
    );
    assert_eq!(
        groups[1]["effects"],
        serde_json::json!([{"effect": "builtin:hue_shift", "version": [1, 1], "instance": 0}])
    );

    assert_eq!(groups[2]["kind"], "track_group");
    assert_eq!(groups[2]["members"], serde_json::json!([{"track": 0}, {"track": 1}]));
    assert!(groups[2].get("effects").is_none(), "it has no effects");

    assert_eq!(groups[3]["kind"], "tag");
    assert_eq!(
        groups[3]["members"],
        serde_json::json!([{"group": 1}, {"group": 2}, {"group": 0}])
    );
}

#[test]
fn a_badge_is_saved_with_how_many_effects_of_its_chain_come_before_it_and_which_it_is_the_badge_of() {
    let file = file_of_the_project_with_groups();
    // The first video: the badge of the tag put before its effect, then the clip group is not a badge (it is inside the clip, not a tag).
    assert_eq!(
        file["tracks"][2]["clips"][0]["badges"],
        serde_json::json!([{"of": {"tag": 0}}]),
        "before the effect, so before none of them"
    );
    // The second video: the badge of its track put first, before its one effect.
    assert_eq!(
        file["tracks"][2]["clips"][1]["badges"],
        serde_json::json!([{"of": "track"}])
    );
    // The sound has no effects, and the tag's badge.
    assert_eq!(
        file["tracks"][1]["clips"][0]["badges"],
        serde_json::json!([{"of": {"tag": 0}}])
    );
    // The text object has its two effects (the text effect that it is made of, and the hue shift), and then the badge of the tag, which is after both of them. (It was after one of them
    // before phase 9.4, when the text was not an effect.)
    assert_eq!(
        file["tracks"][0]["clips"][1]["badges"],
        serde_json::json!([{"of": {"tag": 0}, "effects_before": 2}])
    );
    // The crate is in the tag too.
    assert_eq!(
        file["tracks"][0]["clips"][0]["badges"],
        serde_json::json!([{"of": {"tag": 0}}])
    );
    // The clip group has the badge of the outer tag after its effect; the tag and the track group have the badge of the outer tag.
    assert_eq!(
        file["groups"][1]["badges"],
        serde_json::json!([{"of": {"tag": 3}, "effects_before": 1}])
    );
    assert_eq!(
        file["groups"][0]["badges"],
        serde_json::json!([{"of": {"tag": 3}, "effects_before": 1}])
    );
    assert_eq!(file["groups"][2]["badges"], serde_json::json!([{"of": {"tag": 3}}]));
    assert!(file["groups"][3].get("badges").is_none(), "the outer tag is in no tag");
}

#[test]
fn a_project_with_no_groups_has_no_groups_and_no_badges_in_its_file_and_the_badge_of_a_track_at_the_end_is_not_written()
{
    let text = saved(&project_with_everything());
    assert!(!text.contains("groups"), "{text}");
    assert!(!text.contains("badges"), "{text}");
    assert!(text.contains(&version_line(FILE_FORMAT_VERSION)));
    // (Rewritten on purpose in phase 13.3: 21, the version that has the MIDI clip; 20 in phase 12.1, the version that has the sharing of a track, the tail policies and the length of an effect's tail; it was 19 in phase 10.4, the version that has the state of an effect's plugin; 18 for a text made of an effect (phase 9.4), 17 for the images of a shader, 16 for the shader of an effect, 15 for the adjustment clip, 14 for the plugin libraries, and 13 for the shown-ness of the effects.)
    assert_eq!(FILE_FORMAT_VERSION, 21);
}

// --- the trip through the file ---

#[test]
fn groups_with_their_effects_and_the_badges_in_the_chains_survive_a_trip_through_the_text() {
    let project = a_project_with_groups_of_every_kind();
    let loaded = project_from_json_text(&saved(&project)).unwrap();
    assert_same_content(&loaded, &project);
    assert_eq!(loaded.problems_with_the_groups(), []);

    let groups = loaded.groups();
    assert_eq!(
        groups
            .iter()
            .map(|group| (group.kind(), group.name().to_owned()))
            .collect::<Vec<_>>(),
        [
            (GroupKind::Tag, "titles and sound".to_owned()),
            (GroupKind::ClipGroup, "Clip group 1".to_owned()),
            (GroupKind::TrackGroup, "Track group 1".to_owned()),
            (GroupKind::Tag, "Tag 1".to_owned()),
        ]
    );
    assert_eq!(groups[1].colour(), GroupColour::new(1, 2, 3));
    assert_eq!(groups[0].effects().value_of(HUE), Some(turn(45.0)));
    assert_eq!(groups[3].effects().value_of(HUE), Some(turn(-30.0)));
    assert_eq!(groups[0].members().len(), 4);

    // The chains: the first video has the badge of the tag, and then its effect; the second has the badge of its track first.
    let first_video = Member::Clip(loaded.tracks()[2].entries()[0].id());
    let second_video = Member::Clip(loaded.tracks()[2].entries()[1].id());
    let tag = groups[0].id();
    assert_eq!(
        loaded.chain_of_the_member(first_video).unwrap().keys(true),
        [tag_badge(tag), hue_shift(), ChainItemKey::Badge(Badge::Track)]
    );
    assert_eq!(
        loaded.chain_of_the_member(second_video).unwrap().keys(true),
        [ChainItemKey::Badge(Badge::Track), hue_shift()]
    );
}

#[test]
fn what_is_applied_to_each_clip_is_the_same_after_a_trip_through_the_text() {
    let project = a_project_with_groups_of_every_kind();
    let loaded = project_from_json_text(&saved(&project)).unwrap();
    // For each clip, in the order of the tracks and then the clips, what is applied to it: who has the effect, and its values.
    let applied_to_each_clip = |project: &Project| -> Vec<Vec<(usize, Vec<PropertyValue>)>> {
        let kinds_of_owners = |owner: Member| match owner {
            Member::Clip(_) => 0,
            Member::Track(_) => 1,
            Member::Group(_) => 2,
        };
        project
            .tracks()
            .iter()
            .flat_map(|track| track.entries())
            .map(|entry| {
                project
                    .effects_applied_to_the_clip(entry.id())
                    .iter()
                    .map(|applied| (kinds_of_owners(applied.owner), applied.effect.values().to_vec()))
                    .collect()
            })
            .collect()
    };
    assert_eq!(applied_to_each_clip(&loaded), applied_to_each_clip(&project));
    assert!(
        applied_to_each_clip(&loaded).iter().map(Vec::len).sum::<usize>() > 4,
        "there is something to compare"
    );
}

#[test]
fn a_project_with_groups_is_saved_the_same_every_time_it_is_loaded_and_saved_again() {
    let first = saved(&a_project_with_groups_of_every_kind());
    let second = saved(&project_from_json_text(&first).unwrap());
    let third = saved(&project_from_json_text(&second).unwrap());
    assert_eq!(first, second);
    assert_eq!(second, third);
}

/// Projects of text objects with groups of every kind in them, in every state that random edits bring them to (clips cut and copied and
/// deleted, tracks moved and copied, groups made and taken apart, chains moved about): each is saved, loaded and saved again.
#[test]
fn projects_that_have_had_random_edits_made_to_them_survive_a_trip_through_the_text() {
    let mut projects_with_groups = 0;
    let mut most_groups = 0;
    for seed in 1..=60 {
        let project = crate::project::groups::random_edits_tests::project_after_random_edits_of_whole_texts(seed, 120);
        let text = saved(&project);
        let loaded = project_from_json_text(&text).unwrap_or_else(|error| panic!("seed {seed}: {error}\n{text}"));
        assert_eq!(saved(&loaded), text, "seed {seed}");
        assert_eq!(loaded.problems_with_the_groups(), [], "seed {seed}");
        assert_eq!(loaded.groups().len(), project.groups().len(), "seed {seed}");
        assert_eq!(
            loaded
                .groups()
                .iter()
                .map(|group| (group.kind(), group.name().to_owned(), group.members().len()))
                .collect::<Vec<_>>(),
            project
                .groups()
                .iter()
                .map(|group| (group.kind(), group.name().to_owned(), group.members().len()))
                .collect::<Vec<_>>(),
            "seed {seed}"
        );
        projects_with_groups += usize::from(!project.groups().is_empty());
        most_groups = most_groups.max(project.groups().len());
    }
    assert!(
        projects_with_groups >= 50,
        "most of the projects have groups: {projects_with_groups}"
    );
    assert!(most_groups >= 6, "some have many: {most_groups}");
}

#[test]
fn groups_that_were_made_and_taken_apart_save_by_their_places_in_the_list_and_not_by_the_numbers_they_had() {
    let mut project = a_project_with_groups_of_every_kind();
    // Take the first group apart: the others are numbered from 0 again in the file, and the badges of them with them.
    let first = project.groups()[0].id();
    project.disband_group(first).unwrap();
    let loaded = project_from_json_text(&saved(&project)).unwrap();
    assert_same_content(&loaded, &project);
    assert_eq!(loaded.groups().len(), 3);
    assert_eq!(loaded.groups()[0].id().value(), 0);
    assert_eq!(loaded.problems_with_the_groups(), []);
}

#[test]
fn a_file_of_version_9_loads_as_a_project_with_no_groups_and_the_effects_of_the_track_after_those_of_each_clip() {
    let mut project = project_with_everything();
    let text = saved(&project).replace(&version_line(10), &version_line(9));
    let loaded = project_from_json_text(&text).unwrap();
    assert!(loaded.groups().is_empty());
    assert_same_content(&loaded, &project);
    let video = loaded.tracks()[2].entries()[1].id();
    assert!(
        loaded
            .chain_of_the_member(Member::Clip(video))
            .unwrap()
            .has_nothing_in_it()
            || loaded.tracks()[2].effects().is_empty()
    );
    // (The clip has its own effect, and its track none: put one on the track, and the layer has the clip's and then the track's, as it did.)
    let track = loaded.tracks()[2].id();
    let mut loaded = loaded;
    loaded
        .add_effect(AnimationTarget::Track(track), EffectKind::HUE_SHIFT)
        .unwrap();
    let applied = loaded.effects_applied_to_the_clip(video);
    assert_eq!(
        applied.iter().map(|applied| applied.owner).collect::<Vec<_>>(),
        [Member::Clip(video), Member::Track(track)]
    );
    let _ = &mut project;
}

// --- what is refused ---

#[test]
fn a_group_that_has_what_the_file_does_not_have_in_it_is_refused_and_the_group_is_named() {
    let no_clip = text_of_the_file_with(|file| {
        file["groups"][0]["members"][0] = serde_json::json!({"clip": {"track": 2, "clip": 9}});
    });
    let reason = reason_it_is_refused(&no_clip);
    assert!(
        reason.contains("group number 1") && reason.contains("no clip 9 on track 2"),
        "{reason}"
    );

    let no_track = text_of_the_file_with(|file| {
        file["groups"][2]["members"][1] = serde_json::json!({"track": 7});
    });
    let reason = reason_it_is_refused(&no_track);
    assert!(
        reason.contains("group number 3") && reason.contains("no track 7"),
        "{reason}"
    );

    let no_group = text_of_the_file_with(|file| {
        file["groups"][3]["members"][0] = serde_json::json!({"group": 4});
    });
    let reason = reason_it_is_refused(&no_group);
    assert!(
        reason.contains("group number 4") && reason.contains("no group 4"),
        "{reason}"
    );

    let no_track_for_a_clip = text_of_the_file_with(|file| {
        file["groups"][0]["members"][1] = serde_json::json!({"clip": {"track": 8, "clip": 0}});
    });
    assert!(reason_it_is_refused(&no_track_for_a_clip).contains("no track 8"));
}

#[test]
fn a_badge_that_cannot_be_is_refused() {
    let of_no_tag = text_of_the_file_with(|file| {
        file["tracks"][1]["clips"][0]["badges"] = serde_json::json!([{"of": {"tag": 12}}]);
    });
    let reason = reason_it_is_refused(&of_no_tag);
    assert!(
        reason.contains("clip number 1 of track number 2") && reason.contains("tag number 12"),
        "{reason}"
    );

    let after_more_effects_than_there_are = text_of_the_file_with(|file| {
        file["tracks"][2]["clips"][0]["badges"] = serde_json::json!([{"of": {"tag": 0}, "effects_before": 5}]);
    });
    let reason = reason_it_is_refused(&after_more_effects_than_there_are);
    assert!(reason.contains("after 5 of the effects, and there are 1"), "{reason}");

    let twice = text_of_the_file_with(|file| {
        file["tracks"][2]["clips"][0]["badges"] =
            serde_json::json!([{"of": {"tag": 0}}, {"of": {"tag": 0}, "effects_before": 1}]);
    });
    assert!(reason_it_is_refused(&twice).contains("the same badge twice"));
}

#[test]
fn what_is_one_past_what_the_file_has_is_refused_in_a_member_a_badge_and_the_effects_before_a_badge() {
    // (The first number that is too much, which is where a mistake of one is seen.)
    // The videos are the third track, and it has three clips.
    let one_past_the_clips = text_of_the_file_with(|file| {
        file["groups"][0]["members"][0] = serde_json::json!({"clip": {"track": 2, "clip": 3}});
    });
    assert!(reason_it_is_refused(&one_past_the_clips).contains("no clip 3 on track 2"));

    // There are three tracks, which are 0 to 2.
    let one_past_the_tracks = text_of_the_file_with(|file| {
        file["groups"][2]["members"][1] = serde_json::json!({"track": 3});
    });
    assert!(reason_it_is_refused(&one_past_the_tracks).contains("no track 3"));

    // There are four groups, which are 0 to 3.
    let one_past_the_groups = text_of_the_file_with(|file| {
        file["tracks"][1]["clips"][0]["badges"] = serde_json::json!([{"of": {"tag": 4}}]);
    });
    assert!(reason_it_is_refused(&one_past_the_groups).contains("tag number 4"));

    // The first video has one effect.
    let one_past_the_effects = text_of_the_file_with(|file| {
        file["tracks"][2]["clips"][0]["badges"] = serde_json::json!([{"of": {"tag": 0}, "effects_before": 2}]);
    });
    assert!(reason_it_is_refused(&one_past_the_effects).contains("after 2 of the effects, and there are 1"));
}

#[test]
fn a_group_that_breaks_a_rule_of_groups_is_refused_with_the_rule() {
    // A member of a tag that has no badge of it.
    let without_the_badge = text_of_the_file_with(|file| {
        file["tracks"][1]["clips"][0].as_object_mut().unwrap().remove("badges");
    });
    let reason = reason_it_is_refused(&without_the_badge);
    assert!(reason.contains("no badge"), "{reason}");

    // A badge of a tag that the clip is not in.
    let not_in_it = text_of_the_file_with(|file| {
        file["tracks"][2]["clips"][2]["badges"] = serde_json::json!([{"of": {"tag": 0}}]);
    });
    assert!(reason_it_is_refused(&not_in_it).contains("is not in it"));

    // A clip group that has a clip in its rectangle that is not in it: drop the second video from it, the first is before it on the track,
    // and the track's third clip is after: take the first video out and put the third in, with the second between.
    let with_an_outsider = text_of_the_file_with(|file| {
        file["groups"][1]["members"] =
            serde_json::json!([{"clip": {"track": 2, "clip": 0}}, {"clip": {"track": 2, "clip": 2}}]);
    });
    let reason = reason_it_is_refused(&with_an_outsider);
    assert!(reason.contains("is inside the rectangle of group"), "{reason}");

    // A group with nothing in it.
    let empty = text_of_the_file_with(|file| {
        file["groups"][2]["members"] = serde_json::json!([]);
    });
    assert!(reason_it_is_refused(&empty).contains("has nothing in it"));

    // A tag that has a track in it.
    let a_track_in_a_tag = text_of_the_file_with(|file| {
        file["groups"][0]["members"] = serde_json::json!([{"track": 0}]);
    });
    assert!(reason_it_is_refused(&a_track_in_a_tag).contains("cannot have"));

    // The badge of a track in the chain of a group, put before the badge of the outer tag (a badge of a track that is last is not kept).
    let track_badge_on_a_group = text_of_the_file_with(|file| {
        file["groups"][2]["badges"] = serde_json::json!([{"of": "track"}, {"of": {"tag": 3}}]);
    });
    let reason = reason_it_is_refused(&track_badge_on_a_group);
    assert!(reason.contains("badge of its track"), "{reason}");

    // At the end it is not kept, and so there is nothing to refuse: it is what a chain that has none is.
    let track_badge_at_the_end = text_of_the_file_with(|file| {
        file["groups"][2]["badges"] = serde_json::json!([{"of": {"tag": 3}}, {"of": "track"}]);
    });
    assert!(project_from_json_text(&track_badge_at_the_end).is_ok());
}

#[test]
fn an_effect_that_a_group_cannot_have_and_a_bad_colour_and_an_unknown_kind_are_refused() {
    let unknown_effect = text_of_the_file_with(|file| {
        file["groups"][0]["effects"] = serde_json::json!([{"effect": "blur"}]);
    });
    let reason = reason_it_is_refused(&unknown_effect);
    assert!(
        reason.contains("group number 1") && reason.contains("\"blur\" is not an effect there is"),
        "{reason}"
    );

    let same_name = text_of_the_file_with(|file| {
        file["groups"][0]["effects"] = serde_json::json!([
            {"effect": "hue shift", "instance": 4},
            {"effect": "hue shift", "instance": 4, "values": [9.0]}
        ]);
    });
    let reason = reason_it_is_refused(&same_name);
    assert!(
        reason.contains("group number 1") && reason.contains("two effects that are named effect 4"),
        "{reason}"
    );

    // (Two of one kind under names of their own are two effects, which a group may have.)
    let two_hue_shifts = text_of_the_file_with(|file| {
        file["groups"][0]["effects"] = serde_json::json!([
            {"effect": "hue shift", "instance": 4},
            {"effect": "hue shift", "instance": 7, "values": [9.0]}
        ]);
    });
    let project = project_from_json_text(&two_hue_shifts).unwrap();
    let group = project.groups().first().unwrap();
    assert_eq!(group.effects().len(), 2);
    assert!(
        group
            .effects()
            .has_the_instance(crate::project::EffectInstanceId::new(4))
    );
    assert!(
        group
            .effects()
            .has_the_instance(crate::project::EffectInstanceId::new(7))
    );

    for bad in ["red", "#FF00", "#GG0000", "#FF0000FF", "FF0000"] {
        let text = text_of_the_file_with(|file| {
            file["groups"][1]["colour"] = serde_json::json!(bad);
        });
        let reason = reason_it_is_refused(&text);
        assert!(reason.contains("#RRGGBB"), "{bad}: {reason}");
    }

    let unknown_kind = text_of_the_file_with(|file| {
        file["groups"][1]["kind"] = serde_json::json!("pile");
    });
    assert!(matches!(
        project_from_json_text(&unknown_kind),
        Err(ProjectFileError::NotAProjectFile { .. })
    ));
    let unknown_field = text_of_the_file_with(|file| {
        file["groups"][1]["weight"] = serde_json::json!(3);
    });
    assert!(matches!(
        project_from_json_text(&unknown_field),
        Err(ProjectFileError::NotAProjectFile { .. })
    ));
}

#[test]
fn a_project_file_that_is_a_plain_project_with_groups_is_read_in_a_project_that_has_them() {
    // (Groups need no archive: only files that are included in a project file do.)
    let text = saved(&a_project_with_groups_of_every_kind());
    assert!(project_from_json_text(&text).is_ok());
    let _ = edited_project(link_to_missing_video());
}
