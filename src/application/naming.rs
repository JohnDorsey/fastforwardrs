// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What things are called to the person editing: a clip by the kind of thing it is and what tells it from the others of its
//! kind (the name of its file, the beginning of its words), a property of it by its name, and a rack of keyframes by what
//! it animates. (An effect, and a value of an effect, that has others of its kind in the same chain is told from them by a number: "Hue
//! shift 2", the second of the hue shifts, counting in the order they are applied in. Where there is only one it has none.)

use std::collections::HashMap;

use super::timeline_panel::SourceLook;
use crate::project::{
    AnimationTarget, Clip, ClipId, ClipKind, Effect, EffectKind, Effects, Group, Member, Project, PropertyId, Rack,
    SourceMediaIdentifier, format_timecode,
};

/// How many letters of the words of a text object are used to tell it from the others.
const MOST_LETTERS_OF_WORDS: usize = 24;

/// What each of the project's source files looks like, which is where their names are.
pub type SourceLooks<'a> = HashMap<SourceMediaIdentifier, SourceLook<'a>>;

/// The kind of thing a clip is.
pub fn kind_name(kind: ClipKind) -> &'static str {
    match kind {
        ClipKind::Video => "Video clip",
        ClipKind::Audio => "Sound clip",
        ClipKind::Image => "Picture clip",
        ClipKind::Text => "Text object",
        ClipKind::Crate => "Keyframe crate",
    }
}

/// The first line of `words`, cut short if it is long.
pub fn beginning_of(words: &str) -> String {
    let first_line = words.lines().next().unwrap_or("").trim();
    if first_line.chars().count() > MOST_LETTERS_OF_WORDS {
        let cut: String = first_line.chars().take(MOST_LETTERS_OF_WORDS).collect();
        format!("{}...", cut.trim_end())
    } else {
        first_line.to_owned()
    }
}

/// What tells `clip` from the others of its kind: the name of its file, or the beginning of its words. (Nothing, for a clip
/// that has neither.)
pub fn detail_of_a_clip(clip: &Clip, source_looks: &SourceLooks<'_>) -> String {
    match clip {
        Clip::Text(text) => beginning_of(&text.object.text),
        other => other
            .source()
            .and_then(|source| source_looks.get(&source))
            .map(|look| look.file_name.clone())
            .unwrap_or_default(),
    }
}

/// `name` with a capital letter at the start.
fn with_a_capital_letter(name: &str) -> String {
    let mut letters = name.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => String::new(),
    }
}

/// `name`, and when `chain` has several effects of the kind of the effect named `instance`, which of them that is: "hue shift 2".
fn with_its_place_among_its_kind(
    name: &str,
    chain: Option<&Effects>,
    instance: Option<crate::project::EffectInstanceId>,
) -> String {
    match instance.and_then(|instance| chain?.place_among_its_kind(instance)) {
        Some((place, of)) if of > 1 => format!("{name} {place}"),
        _ => name.to_owned(),
    }
}

/// What a property of the clip or the track `target` is called, in lower case: its name, and for the value of an effect of which there are
/// several of its kind on `target`, which one that is ("hue shift 2").
pub fn name_of_a_property_of(project: &Project, target: AnimationTarget, property: PropertyId) -> String {
    with_its_place_among_its_kind(property.name(), project.effects_of(target), property.effect_instance())
}

/// What a property of the clip or the track `target` is called: [`name_of_a_property_of`], with a capital letter.
pub fn what_a_property_of_is_called(project: &Project, target: AnimationTarget, property: PropertyId) -> String {
    with_a_capital_letter(&name_of_a_property_of(project, target, property))
}

/// What a kind of effect is called: its name, with a capital letter.
pub fn what_an_effect_is_called(kind: EffectKind) -> String {
    with_a_capital_letter(kind.name())
}

/// What the effect `effect` is called in the chain `chain` (which it is in): [`what_an_effect_is_called`], and when the chain has several of
/// its kind, which one it is ("Hue shift 2").
pub fn what_an_effect_in_a_chain_is_called(chain: &Effects, effect: &Effect) -> String {
    with_its_place_among_its_kind(
        &what_an_effect_is_called(effect.kind()),
        Some(chain),
        Some(effect.instance()),
    )
}

/// What a clip or a track is called, in a few words: a clip by what tells it from the others of its kind, or if nothing does
/// by its kind, and a track by its title. `None` for what is not in the project.
pub fn what_a_target_is_called(
    project: &Project,
    target: AnimationTarget,
    source_looks: &SourceLooks<'_>,
) -> Option<String> {
    match target {
        AnimationTarget::Track(track) => Some(project.track(track)?.title().to_owned()),
        AnimationTarget::Clip(clip) => {
            let placement = project.placement_of(clip)?;
            let detail = detail_of_a_clip(placement.clip, source_looks);
            Some(if detail.is_empty() {
                kind_name(placement.clip.kind()).to_owned()
            } else {
                detail
            })
        }
    }
}

/// What a clip is called in a list or a message: its kind, then what tells it from the others of its kind (the name of its file, the
/// beginning of its words), and where it starts, which tells apart the pieces of one video ("Video clip: holiday.mp4 (at 0:00)"). `None`
/// for a clip that is not in the project.
pub fn describe_a_clip(project: &Project, source_looks: &SourceLooks<'_>, clip: ClipId) -> Option<String> {
    let placement = project.placement_of(clip)?;
    let detail = detail_of_a_clip(placement.clip, source_looks);
    let kind = kind_name(placement.clip.kind());
    let starts = format_timecode(placement.first_timeline_frame, project.frame_rate());
    Some(if detail.is_empty() {
        format!("{kind} (at {starts})")
    } else {
        format!("{kind}: {detail} (at {starts})")
    })
}

/// What a clip, a track or a group is called in a list or a message: a clip as [`describe_a_clip`] says, a track by "Track" and its
/// title, a group by its kind and its name. `None` for what is not in the project.
pub fn describe_a_member(project: &Project, source_looks: &SourceLooks<'_>, member: Member) -> Option<String> {
    match member {
        Member::Clip(clip) => describe_a_clip(project, source_looks, clip),
        Member::Track(track) => Some(format!("Track: {}", project.track(track)?.title())),
        Member::Group(group) => Some(describe_a_group(project.group(group)?)),
    }
}

/// What a group is called in a list or on a badge: its kind and its name ("Tag: holiday").
pub fn describe_a_group(group: &Group) -> String {
    format!("{}: {}", group.kind().name_with_a_capital(), group.name())
}

/// What a rack of keyframes is called: the property it animates, and if the rack is not in the drawer of what it animates
/// (`in_the_drawer_of` is whose drawer it is in), what that is.
pub fn what_a_rack_is_called(
    project: &Project,
    rack: &Rack,
    in_the_drawer_of: AnimationTarget,
    source_looks: &SourceLooks<'_>,
) -> String {
    let property = what_a_property_of_is_called(project, rack.target(), rack.property());
    if rack.target() == in_the_drawer_of {
        return property;
    }
    match what_a_target_is_called(project, rack.target(), source_looks) {
        Some(target) => format!("{property} of {target}"),
        None => property,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::project::{
        EffectInstanceId, FrameRate, RackId, SourceMedia, SourceMediaLink, SourceVideoProperties, TextObject,
        TimelineFrameIndex, TrackId,
    };

    /// A project whose one track has a text object, and on it as many hue shifts as `effects_on_the_clip` says. (And the target that is the
    /// clip.)
    fn project_with_hue_shifts(effects_on_the_clip: usize) -> (Project, AnimationTarget) {
        let mut project = Project::empty();
        let track = project.tracks()[0].id();
        let (clip, _) = project
            .insert_text(track, TimelineFrameIndex::new(0), TextObject::default(), 30)
            .unwrap();
        for _ in 0..effects_on_the_clip {
            project
                .add_effect(AnimationTarget::Clip(clip), EffectKind::HueShift)
                .unwrap();
        }
        (project, AnimationTarget::Clip(clip))
    }

    fn hue_of(number: u32) -> PropertyId {
        PropertyId::effect_value(EffectKind::HueShift, EffectInstanceId::new(number), 0)
    }

    #[test]
    fn a_property_is_called_by_its_name_with_a_capital() {
        let (project, target) = project_with_hue_shifts(0);
        let called = |property| what_a_property_of_is_called(&project, target, property);
        assert_eq!(called(PropertyId::Gain), "Gain");
        assert_eq!(called(PropertyId::FontSize), "Font size");
        assert_eq!(called(PropertyId::Rotation), "Rotation");
    }

    #[test]
    fn every_property_has_a_name_that_starts_with_a_capital_and_is_otherwise_its_own_when_there_is_one_effect_of_each_kind()
     {
        let (project, target) = project_with_hue_shifts(1);
        for property in crate::test_support::properties_of_every_kind() {
            let called = what_a_property_of_is_called(&project, target, property);
            assert!(called.chars().next().unwrap().is_uppercase(), "{called}");
            assert_eq!(called.to_lowercase(), property.name());
            assert_eq!(name_of_a_property_of(&project, target, property), property.name());
        }
    }

    #[test]
    fn the_values_of_several_effects_of_one_kind_are_told_apart_by_which_of_them_they_are_counting_from_one() {
        let (project, target) = project_with_hue_shifts(3);

        assert_eq!(name_of_a_property_of(&project, target, hue_of(0)), "hue shift 1");
        assert_eq!(name_of_a_property_of(&project, target, hue_of(1)), "hue shift 2");
        assert_eq!(name_of_a_property_of(&project, target, hue_of(2)), "hue shift 3");
        assert_eq!(what_a_property_of_is_called(&project, target, hue_of(1)), "Hue shift 2");
        assert_eq!(
            name_of_a_property_of(&project, target, hue_of(7)),
            "hue shift",
            "one that is not in the chain has no place in it to say"
        );
        assert_eq!(
            name_of_a_property_of(&project, target, PropertyId::Rotation),
            "rotation",
            "what is not a value of an effect has no number"
        );
    }

    #[test]
    fn when_the_other_effects_of_its_kind_are_gone_an_effect_is_called_by_its_name_alone_again() {
        let (mut project, target) = project_with_hue_shifts(2);
        assert_eq!(name_of_a_property_of(&project, target, hue_of(1)), "hue shift 2");

        project.remove_effect(target, EffectInstanceId::FIRST).unwrap();

        assert_eq!(
            name_of_a_property_of(&project, target, hue_of(1)),
            "hue shift",
            "the first is gone: it is the only hue shift, and the second is not counted from where it was"
        );
    }

    #[test]
    fn a_rack_is_told_from_the_others_by_the_chain_of_what_it_is_of_and_not_by_that_of_the_drawer_it_is_in() {
        let (mut project, with_two) = project_with_hue_shifts(2);
        let track = project.tracks()[0].id();
        let (without_any, _) = project
            .insert_text(track, TimelineFrameIndex::new(100), TextObject::default(), 30)
            .unwrap();
        let without_any = AnimationTarget::Clip(without_any);
        let rack = Rack::new(RackId::new(track, 0), with_two, hue_of(1));
        let looks = SourceLooks::new();

        assert_eq!(
            what_a_rack_is_called(&project, &rack, with_two, &looks),
            "Hue shift 2",
            "in the drawer of what it is of"
        );
        let in_another_drawer = what_a_rack_is_called(&project, &rack, without_any, &looks);
        assert!(
            in_another_drawer.starts_with("Hue shift 2 of "),
            "it is the second hue shift of what it is of, whatever the drawer has: {in_another_drawer}"
        );
    }

    #[test]
    fn an_effect_in_a_chain_is_called_by_its_kind_and_which_of_its_kind_it_is_when_the_chain_has_several() {
        let (project, target) = project_with_hue_shifts(2);
        let chain = project.effects_of(target).unwrap();
        let called: Vec<String> = chain
            .iter()
            .map(|effect| what_an_effect_in_a_chain_is_called(chain, effect))
            .collect();
        assert_eq!(called, ["Hue shift 1", "Hue shift 2"]);

        let (alone, target) = project_with_hue_shifts(1);
        let chain = alone.effects_of(target).unwrap();
        assert_eq!(
            what_an_effect_in_a_chain_is_called(chain, chain.iter().next().unwrap()),
            "Hue shift"
        );
        assert_eq!(
            what_an_effect_is_called(EffectKind::HueShift),
            "Hue shift",
            "the menu has no number"
        );
    }

    #[test]
    fn the_beginning_of_words_is_their_first_line_and_a_long_one_is_cut() {
        assert_eq!(beginning_of("Hello\nthere"), "Hello");
        assert_eq!(beginning_of("  padded  "), "padded");
        assert_eq!(beginning_of(""), "");
        let long = "abcdefghijklmnopqrstuvwxyz and then some more";
        assert_eq!(beginning_of(long), "abcdefghijklmnopqrstuvwx...");
        assert_eq!(
            beginning_of("abcdefghijklmnopqrstuvwx"),
            "abcdefghijklmnopqrstuvwx",
            "exactly as long as it may be"
        );
    }

    fn project_with_a_video_and_a_text() -> (Project, AnimationTarget, AnimationTarget, TrackId) {
        let mut project = Project::empty();
        let outcome = project
            .import_source(
                SourceMedia::video(
                    SourceMediaIdentifier::new(1),
                    SourceMediaLink::from_absolute_path(PathBuf::from("holiday.mp4")),
                    SourceVideoProperties {
                        frame_rate: FrameRate::new(30, 1).unwrap(),
                        total_frame_count: 100,
                        display_width_in_pixels: 320,
                        display_height_in_pixels: 180,
                        has_audio: true,
                    },
                ),
                TimelineFrameIndex::new(0),
            )
            .unwrap();
        let video = AnimationTarget::Clip(outcome.clip.unwrap());
        let in_front = project.add_track_in_front();
        let (text, _) = project
            .insert_text(
                in_front,
                TimelineFrameIndex::new(10),
                TextObject {
                    text: "Happy birthday\nto you".to_owned(),
                    ..TextObject::default()
                },
                30,
            )
            .unwrap();
        (project, video, AnimationTarget::Clip(text), in_front)
    }

    fn looks_of<'a>() -> SourceLooks<'a> {
        let mut looks = HashMap::new();
        looks.insert(
            SourceMediaIdentifier::new(1),
            SourceLook {
                is_available: true,
                thumbnails: None,
                picture_size_in_pixels: (320, 180),
                picture: None,
                waveform: None,
                file_name: "holiday.mp4".to_owned(),
            },
        );
        looks
    }

    #[test]
    fn a_clip_is_called_by_its_file_or_its_words_and_a_track_by_its_title() {
        let (project, video, text, track) = project_with_a_video_and_a_text();
        let looks = looks_of();
        assert_eq!(
            what_a_target_is_called(&project, video, &looks).as_deref(),
            Some("holiday.mp4")
        );
        assert_eq!(
            what_a_target_is_called(&project, text, &looks).as_deref(),
            Some("Happy birthday")
        );
        assert_eq!(
            what_a_target_is_called(&project, AnimationTarget::Track(track), &looks).as_deref(),
            Some(project.track(track).unwrap().title())
        );
    }

    #[test]
    fn a_clip_that_has_nothing_to_tell_it_by_is_called_by_its_kind_and_what_is_not_there_has_no_name() {
        let (project, video, _, _) = project_with_a_video_and_a_text();
        let no_looks = SourceLooks::new();
        assert_eq!(
            what_a_target_is_called(&project, video, &no_looks).as_deref(),
            Some("Video clip"),
            "the file's name is not known"
        );
        let nobody = AnimationTarget::Clip(crate::project::ClipId::new(TrackId::new(99), 0));
        assert_eq!(what_a_target_is_called(&project, nobody, &no_looks), None);
    }

    #[test]
    fn a_rack_in_the_drawer_of_what_it_animates_is_called_by_its_property_and_in_any_other_drawer_by_what_it_animates_too()
     {
        let (project, video, text, _) = project_with_a_video_and_a_text();
        let looks = looks_of();
        let rack = Rack::new(crate::project::RackId::new(TrackId::new(0), 0), video, PropertyId::Gain);

        assert_eq!(what_a_rack_is_called(&project, &rack, video, &looks), "Gain");
        assert_eq!(
            what_a_rack_is_called(&project, &rack, text, &looks),
            "Gain of holiday.mp4"
        );
        let track_rack = Rack::new(
            crate::project::RackId::new(TrackId::new(0), 1),
            AnimationTarget::Track(project.tracks()[0].id()),
            PropertyId::Muted,
        );
        assert_eq!(
            what_a_rack_is_called(&project, &track_rack, video, &looks),
            format!("Muted of {}", project.tracks()[0].title())
        );
    }
}
