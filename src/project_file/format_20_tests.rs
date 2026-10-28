// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What version 20 of the project file added (phase 12, piece 12.1; docs/engine-phase-12-design.md): the `sharing` of a track, the `clip_tails` of a track and the `tail` of a clip group or a
//! track group, all left out when they are the default; and that a file of version 19, which has none of them, is read as a project where everything is the default.

use serde_json::{Value, json};

use super::tests::{saved, version_line};
use super::*;
use crate::project::groups::test_projects::{Laid, lay_out_whole_texts};
use crate::project::{
    ClipTailEdge, ClipTailPolicy, EffectDomain, GroupKind, GroupTailEdge, GroupTailPolicy, Member, TailDepth,
    TrackSharing,
};

fn as_json(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

/// Two tracks, three clips on the first and one on the second, a clip group of the first clip of each and a track group of both tracks: all with the defaults.
fn laid_out_with_groups() -> (Laid, crate::project::GroupId, crate::project::GroupId) {
    let mut laid = lay_out_whole_texts(&[&[(0, 10), (10, 10), (20, 10)], &[(0, 10)]]);
    let clip_group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let track_group = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(0), laid.track_member(1)])
        .unwrap();
    (laid, clip_group, track_group)
}

fn unusual_clip_policy() -> ClipTailPolicy {
    ClipTailPolicy {
        edge: ClipTailEdge::Cut,
        depth: TailDepth::InFront,
    }
}

fn unusual_group_policy() -> GroupTailPolicy {
    GroupTailPolicy {
        edge: GroupTailEdge::Release,
        depth: TailDepth::InFront,
    }
}

#[test]
fn a_project_where_everything_is_the_default_has_none_of_the_fields_of_version_20() {
    let (laid, _, _) = laid_out_with_groups();
    let text = saved(&laid.project);

    let file = as_json(&text);
    assert!(text.contains(&version_line(FILE_FORMAT_VERSION)));
    const {
        assert!(
            FILE_FORMAT_VERSION >= 20,
            "the sharing and the tail policies came with format 20"
        )
    };
    for track in file["tracks"].as_array().unwrap() {
        assert!(
            track.get("sharing").is_none() && track.get("clip_tails").is_none(),
            "{track}"
        );
    }
    for group in file["groups"].as_array().unwrap() {
        assert!(group.get("tail").is_none(), "{group}");
    }
}

#[test]
fn the_sharing_of_a_track_is_written_for_the_medium_that_is_not_the_default_and_read_back() {
    let (mut laid, _, _) = laid_out_with_groups();
    laid.project
        .set_track_sharing(laid.tracks[0], EffectDomain::Sound, TrackSharing::PerTrackWithWindows)
        .unwrap();
    laid.project
        .set_track_sharing(laid.tracks[1], EffectDomain::Picture, TrackSharing::PerClip)
        .unwrap();
    laid.project
        .set_track_sharing(laid.tracks[1], EffectDomain::Sound, TrackSharing::PerTrackIndivisible)
        .unwrap();

    let text = saved(&laid.project);

    let file = as_json(&text);
    assert_eq!(
        file["tracks"][0]["sharing"],
        json!({ "sound": "per_track_with_windows" })
    );
    assert_eq!(
        file["tracks"][1]["sharing"],
        json!({ "picture": "per_clip", "sound": "per_track_indivisible" })
    );
    let loaded = project_from_json_text(&text).unwrap();
    assert_eq!(loaded.tracks()[0].sharing().sound, TrackSharing::PerTrackWithWindows);
    assert_eq!(loaded.tracks()[0].sharing().picture, TrackSharing::PerRun);
    assert_eq!(loaded.tracks()[1].sharing().picture, TrackSharing::PerClip);
    assert_eq!(loaded.tracks()[1].sharing().sound, TrackSharing::PerTrackIndivisible);
    assert_eq!(as_json(&saved(&loaded)), file, "nothing is lost, and nothing is added");
}

#[test]
fn the_tail_policy_of_a_clip_is_written_by_its_place_and_only_for_the_clips_that_are_not_the_default() {
    let (mut laid, _, _) = laid_out_with_groups();
    laid.project
        .set_clip_tail_policy(laid.clip(0, 2), unusual_clip_policy())
        .unwrap();
    laid.project
        .set_clip_tail_policy(
            laid.clip(0, 0),
            ClipTailPolicy {
                edge: ClipTailEdge::Into,
                depth: TailDepth::Behind,
            },
        )
        .unwrap();

    let text = saved(&laid.project);

    let file = as_json(&text);
    assert_eq!(
        file["tracks"][0]["clip_tails"],
        json!([{ "clip": 0, "edge": "into" }, { "clip": 2, "edge": "cut", "depth": "in_front" }])
    );
    assert!(file["tracks"][1].get("clip_tails").is_none());
    let loaded = project_from_json_text(&text).unwrap();
    let policy_of = |track: usize, clip: usize| loaded.tracks()[track].entries()[clip].tail_policy();
    assert_eq!(policy_of(0, 2), unusual_clip_policy());
    assert_eq!(policy_of(0, 0).edge, ClipTailEdge::Into);
    assert_eq!(policy_of(0, 1), ClipTailPolicy::default());
    assert_eq!(policy_of(1, 0), ClipTailPolicy::default());
    assert_eq!(as_json(&saved(&loaded)), file);
}

#[test]
fn the_tail_policy_of_a_clip_group_and_of_a_track_group_is_written_and_read_back_and_a_tag_has_none() {
    let (mut laid, clip_group, track_group) = laid_out_with_groups();
    laid.project
        .create_group(GroupKind::Tag, &[Member::Clip(laid.clip(0, 1))])
        .unwrap();
    laid.project
        .set_group_tail_policy(clip_group, unusual_group_policy())
        .unwrap();
    laid.project
        .set_group_tail_policy(
            track_group,
            GroupTailPolicy {
                edge: GroupTailEdge::Cut,
                depth: TailDepth::Behind,
            },
        )
        .unwrap();

    let text = saved(&laid.project);

    let file = as_json(&text);
    assert_eq!(
        file["groups"][0]["tail"],
        json!({ "edge": "release", "depth": "in_front" })
    );
    assert_eq!(file["groups"][1]["tail"], json!({ "edge": "cut" }));
    assert!(file["groups"][2].get("tail").is_none(), "a tag has no edge");
    let loaded = project_from_json_text(&text).unwrap();
    assert_eq!(loaded.groups()[0].tail_policy(), unusual_group_policy());
    assert_eq!(loaded.groups()[1].tail_policy().edge, GroupTailEdge::Cut);
    assert_eq!(loaded.groups()[2].kind(), GroupKind::Tag);
    assert_eq!(loaded.groups()[2].tail_policy(), GroupTailPolicy::default());
    assert_eq!(as_json(&saved(&loaded)), file);

    let mut with_a_tag_that_has_one = file.clone();
    with_a_tag_that_has_one["groups"][2]["tail"] = json!({ "edge": "cut" });
    let error = project_from_json_text(&with_a_tag_that_has_one.to_string()).unwrap_err();
    assert!(error.to_string().contains("a tag has no edge"), "{error}");
}

#[test]
fn a_file_of_version_19_has_none_of_it_is_read_as_the_defaults_and_saved_as_the_version_there_is() {
    let (laid, _, _) = laid_out_with_groups();
    let text = saved(&laid.project);
    let nineteen = text.replace(&version_line(FILE_FORMAT_VERSION), &version_line(19));

    let loaded = project_from_json_text(&nineteen).unwrap();

    assert!(
        loaded
            .tracks()
            .iter()
            .all(|track| track.sharing() == crate::project::EffectSharing::default()
                && track.entries().iter().all(|entry| entry.tail_policy().is_the_default()))
    );
    assert!(loaded.groups().iter().all(|group| group.tail_policy().is_the_default()));
    assert_eq!(as_json(&saved(&loaded)), as_json(&text));
}

#[test]
fn a_tail_policy_of_a_clip_that_the_track_does_not_have_and_a_word_that_is_not_one_are_refused_and_say_where() {
    let (laid, _, _) = laid_out_with_groups();
    let file = as_json(&saved(&laid.project));

    let mut past_the_clips = file.clone();
    past_the_clips["tracks"][1]["clip_tails"] = json!([{ "clip": 5, "edge": "cut" }]);
    let error = project_from_json_text(&past_the_clips.to_string()).unwrap_err();
    assert!(error.to_string().contains("track number 2"), "{error}");
    assert!(error.to_string().contains("clip number 5"), "{error}");

    let mut not_a_word = file.clone();
    not_a_word["tracks"][0]["sharing"] = json!({ "sound": "divided_up" });
    assert!(project_from_json_text(&not_a_word.to_string()).is_err());
    let mut not_an_edge = file;
    not_an_edge["groups"][0]["tail"] = json!({ "edge": "around" });
    assert!(
        project_from_json_text(&not_an_edge.to_string()).is_err(),
        "the words of a clip are not the words of a group"
    );
}

#[test]
fn a_file_that_breaks_the_rules_of_sharing_is_refused_when_it_is_read_and_says_which() {
    let (mut laid, clip_group, _) = laid_out_with_groups();
    laid.project
        .add_effect_to_the_group(clip_group, crate::project::EffectKind::HUE_SHIFT)
        .unwrap();
    let file = as_json(&saved(&laid.project));
    assert!(
        project_from_json_text(&file.to_string()).is_ok(),
        "the file as it is, is right"
    );

    // (The first track has three clips, and the clip group has one of them: a track that is never divided cannot be like that.)
    let mut broken = file;
    broken["tracks"][0]["sharing"] = json!({ "picture": "per_track_indivisible" });
    let error = project_from_json_text(&broken.to_string()).unwrap_err();

    assert!(error.to_string().contains("never divided"), "{error}");
}

#[test]
fn the_tail_length_of_an_effect_is_written_when_it_is_not_automatic_and_read_back() {
    use crate::project::{AnimationTarget, EffectKind, TailLength};

    let (mut laid, _, _) = laid_out_with_groups();
    let clip = AnimationTarget::Clip(laid.clip(0, 0));
    let with_a_length = laid.project.add_effect(clip, EffectKind::HUE_SHIFT).unwrap();
    let with_none = laid.project.add_effect(clip, EffectKind::HUE_SHIFT).unwrap();
    laid.project.add_effect(clip, EffectKind::HUE_SHIFT).unwrap();
    laid.project
        .set_effect_tail_length(laid.clip_member(0, 0), with_a_length, TailLength::Milliseconds(1500))
        .unwrap();
    laid.project
        .set_effect_tail_length(laid.clip_member(0, 0), with_none, TailLength::None)
        .unwrap();

    let text = saved(&laid.project);

    let file = as_json(&text);
    // (The first effect of a text is its text effect.)
    let effects = file["tracks"][0]["clips"][0]["effects"].as_array().unwrap();
    assert_eq!(effects[1]["tail"], json!({ "milliseconds": 1500 }));
    assert_eq!(effects[2]["tail"], json!("none"));
    assert!(
        effects[3].get("tail").is_none(),
        "an effect with the automatic tail has no field for it"
    );
    let loaded = project_from_json_text(&text).unwrap();
    let lengths: Vec<TailLength> = loaded.tracks()[0].entries()[0]
        .effects()
        .iter()
        .skip(1)
        .map(|effect| effect.tail_length())
        .collect();
    assert_eq!(
        lengths,
        [TailLength::Milliseconds(1500), TailLength::None, TailLength::Automatic]
    );
    assert_eq!(as_json(&saved(&loaded)), file);
}

#[test]
fn the_edge_fade_a_project_sets_for_itself_is_written_and_read_back_and_one_that_follows_the_global_setting_is_not() {
    use crate::project::{EdgeFade, ProjectSettings};

    let (mut laid, _, _) = laid_out_with_groups();
    assert!(
        as_json(&saved(&laid.project)).get("settings").is_none()
            || as_json(&saved(&laid.project))["settings"].get("edge_fade").is_none()
    );

    laid.project.set_settings(ProjectSettings {
        edge_fade: Some(EdgeFade::Milliseconds20),
        ..ProjectSettings::FOLLOW_ALL_GLOBAL_SETTINGS
    });
    let text = saved(&laid.project);

    let file = as_json(&text);
    assert_eq!(file["settings"]["edge_fade"], json!("20_ms"));
    let loaded = project_from_json_text(&text).unwrap();
    assert_eq!(loaded.settings().edge_fade, Some(EdgeFade::Milliseconds20));
    assert_eq!(as_json(&saved(&loaded)), file);
}
