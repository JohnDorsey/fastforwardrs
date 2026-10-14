//! The effects of groups, the rows of a chain as the properties panel shows them, moving things in a chain, and which effects are applied to a
//! clip and in what order.

use super::*;
use crate::colour_effects::ColourMatrix;
use crate::project::groups::test_projects::{Laid, lay_out};
use crate::project::{
    ClipEditError, GroupColour, GroupKind, SourceAudioProperties, SourceMedia, SourceMediaIdentifier, SourceMediaLink,
    TimelineFrameIndex, TrackId,
};

const HUE_SHIFT: EffectKind = EffectKind::HueShift;

fn effect_key() -> ChainItemKey {
    ChainItemKey::Effect(HUE_SHIFT)
}

fn tag_key(group: GroupId) -> ChainItemKey {
    ChainItemKey::Badge(Badge::Tag(group))
}

const TRACK_KEY: ChainItemKey = ChainItemKey::Badge(Badge::Track);

/// Puts a hue shift on the clip at place `index` of the track at place `track` in the list of tracks.
fn hue_shift_on_the_clip(laid: &mut Laid, track: usize, index: usize) {
    let clip = laid.clip(track, index);
    laid.project.add_effect(AnimationTarget::Clip(clip), HUE_SHIFT).unwrap();
}

fn hue_shift_on_the_track(laid: &mut Laid, track: usize) {
    laid.project
        .add_effect(AnimationTarget::Track(laid.tracks[track]), HUE_SHIFT)
        .unwrap();
}

/// Who has each of the effects that are applied to the clip, in the order they are applied in.
fn owners(project: &Project, clip: ClipId) -> Vec<Member> {
    project
        .effects_applied_to_the_clip(clip)
        .iter()
        .map(|applied| applied.owner)
        .collect()
}

/// What each row of the chain of `owner` is, in words that say which it is.
fn rows_of(project: &Project, owner: Member) -> Vec<String> {
    project
        .chain_rows_of(owner)
        .iter()
        .map(|row| match row {
            ChainRow::Effect(effect) => format!("effect {}", effect.kind()),
            ChainRow::TagBadge(group) => format!("tag {}", group.name()),
            ChainRow::TrackBadge {
                track,
                effects_that_apply,
            } => format!("track {} ({effects_that_apply})", track.title()),
            ChainRow::ShapeBadge(group) => format!("shape {}", group.name()),
        })
        .collect()
}

/// A project of four tracks with a clip at frames 0 to 10 on each.
fn four_tracks() -> Laid {
    lay_out(&[&[(0, 10)], &[(0, 10)], &[(0, 10)], &[(0, 10)]])
}

// --- the rows of a chain ---

#[test]
fn a_clip_that_has_nothing_in_its_chain_and_nothing_on_its_track_has_no_rows() {
    let laid = four_tracks();
    assert!(laid.project.chain_rows_of(laid.clip_member(0, 0)).is_empty());
    assert!(laid.project.chain_rows_of(laid.track_member(0)).is_empty());
}

#[test]
fn the_rows_of_a_chain_are_its_effects_its_badges_and_the_badge_of_a_track_that_has_effects_in_the_order_they_apply_in()
{
    let mut laid = four_tracks();
    hue_shift_on_the_clip(&mut laid, 0, 0);
    hue_shift_on_the_track(&mut laid, 0);
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    laid.project.rename_group(tag, "titles").unwrap();
    let track_title = laid.project.track(laid.tracks[0]).unwrap().title().to_owned();
    assert_eq!(
        rows_of(&laid.project, laid.clip_member(0, 0)),
        [
            "effect hue shift".to_owned(),
            "tag titles".to_owned(),
            format!("track {track_title} (1)")
        ]
    );
}

#[test]
fn the_badge_of_a_track_is_a_row_only_when_the_track_has_effects_and_says_how_many() {
    let mut laid = four_tracks();
    assert!(
        rows_of(&laid.project, laid.clip_member(1, 0)).is_empty(),
        "a track with no effects: no badge"
    );
    hue_shift_on_the_track(&mut laid, 1);
    let title = laid.project.track(laid.tracks[1]).unwrap().title().to_owned();
    assert_eq!(
        rows_of(&laid.project, laid.clip_member(1, 0)),
        [format!("track {title} (1)")]
    );
    assert!(
        rows_of(&laid.project, laid.clip_member(2, 0)).is_empty(),
        "the clips of other tracks have none"
    );
    laid.project
        .remove_effect(AnimationTarget::Track(laid.tracks[1]), HUE_SHIFT)
        .unwrap();
    assert!(rows_of(&laid.project, laid.clip_member(1, 0)).is_empty());
}

#[test]
fn the_badge_of_a_track_is_where_it_was_put_in_the_chain_of_the_clip() {
    let mut laid = four_tracks();
    hue_shift_on_the_clip(&mut laid, 0, 0);
    hue_shift_on_the_track(&mut laid, 0);
    let title = laid.project.track(laid.tracks[0]).unwrap().title().to_owned();
    laid.project
        .move_chain_item(laid.clip_member(0, 0), TRACK_KEY, Some(effect_key()))
        .unwrap();
    assert_eq!(
        rows_of(&laid.project, laid.clip_member(0, 0)),
        [format!("track {title} (1)"), "effect hue shift".to_owned()]
    );
    // It is still where it was put when the track has no effects for a while and has some again.
    laid.project
        .remove_effect(AnimationTarget::Track(laid.tracks[0]), HUE_SHIFT)
        .unwrap();
    assert_eq!(
        rows_of(&laid.project, laid.clip_member(0, 0)),
        ["effect hue shift".to_owned()]
    );
    hue_shift_on_the_track(&mut laid, 0);
    assert_eq!(
        rows_of(&laid.project, laid.clip_member(0, 0)),
        [format!("track {title} (1)"), "effect hue shift".to_owned()]
    );
}

#[test]
fn the_clip_group_or_track_group_something_is_directly_in_is_its_last_row_and_one_it_is_only_inside_is_none() {
    let mut laid = four_tracks();
    laid.project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    laid.project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(0), laid.track_member(1)])
        .unwrap();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0), laid.clip_member(2, 0)])
        .unwrap();
    laid.project.add_effect_to_the_group(tag, HUE_SHIFT).unwrap();
    hue_shift_on_the_clip(&mut laid, 0, 0);

    assert_eq!(
        rows_of(&laid.project, laid.clip_member(0, 0)),
        [
            "tag Tag 1".to_owned(),
            "effect hue shift".to_owned(),
            "shape Clip group 1".to_owned()
        ],
        "the clip group last: its effects are applied after the clip is made"
    );
    assert_eq!(rows_of(&laid.project, laid.clip_member(2, 0)), ["tag Tag 1".to_owned()]);
    assert_eq!(
        rows_of(&laid.project, laid.clip_member(1, 0)),
        ["shape Clip group 1".to_owned()],
        "the track group is not a row: the clip is only inside it, by being on a track of it"
    );
    assert_eq!(
        rows_of(&laid.project, laid.track_member(0)),
        ["shape Track group 1".to_owned()]
    );
    assert!(rows_of(&laid.project, laid.track_member(2)).is_empty());
}

#[test]
fn a_group_has_rows_for_the_tags_it_is_in_and_the_group_it_is_inside_after_its_own_effects() {
    let mut laid = four_tracks();
    let inner = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let outer = laid
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Group(inner), laid.clip_member(2, 0)])
        .unwrap();
    laid.project
        .create_group(GroupKind::Tag, &[Member::Group(inner)])
        .unwrap();
    laid.project.add_effect_to_the_group(inner, HUE_SHIFT).unwrap();
    assert_eq!(
        rows_of(&laid.project, Member::Group(inner)),
        [
            "tag Tag 1".to_owned(),
            "effect hue shift".to_owned(),
            "shape Clip group 2".to_owned()
        ]
    );
    assert!(rows_of(&laid.project, Member::Group(outer)).is_empty());
}

#[test]
fn a_clip_that_picture_effects_do_not_apply_to_has_no_badge_of_its_track() {
    let mut laid = four_tracks();
    hue_shift_on_the_track(&mut laid, 0);
    let sound = laid
        .project
        .import_source(
            SourceMedia::audio(
                SourceMediaIdentifier::new(5),
                SourceMediaLink::from_absolute_path("sound.mp3".into()),
                SourceAudioProperties {
                    duration_in_microseconds: 1_000_000,
                },
            ),
            TimelineFrameIndex::new(20),
        )
        .unwrap();
    // (The sound went on a track that had nothing on it, which has none of the effects: put the effect on that track.)
    laid.project
        .add_effect(AnimationTarget::Track(sound.track), HUE_SHIFT)
        .unwrap();
    let sound_clip = sound.clip.unwrap();
    assert!(
        laid.project.chain_rows_of(Member::Clip(sound_clip)).is_empty(),
        "a hue shift is not heard"
    );
    assert!(laid.project.effects_applied_to_the_clip(sound_clip).is_empty());
}

#[test]
fn a_row_is_named_by_what_it_is_to_be_moved_by_and_the_group_it_is_inside_cannot_be_moved() {
    let mut laid = four_tracks();
    hue_shift_on_the_clip(&mut laid, 0, 0);
    hue_shift_on_the_track(&mut laid, 0);
    laid.project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    let keys: Vec<Option<ChainItemKey>> = laid
        .project
        .chain_rows_of(laid.clip_member(0, 0))
        .iter()
        .map(ChainRow::key)
        .collect();
    assert_eq!(keys, [Some(effect_key()), Some(tag_key(tag)), Some(TRACK_KEY), None]);
}

#[test]
fn something_that_is_not_in_the_project_has_no_rows() {
    let laid = four_tracks();
    assert!(laid.project.chain_rows_of(Member::Group(GroupId::new(9))).is_empty());
    assert!(laid.project.chain_rows_of(Member::Track(TrackId::new(99))).is_empty());
    assert!(
        laid.project
            .chain_rows_of(Member::Clip(ClipId::new(TrackId::new(99), 0)))
            .is_empty()
    );
}

// --- moving things in a chain ---

#[test]
fn something_in_the_chain_of_a_clip_a_track_or_a_group_is_moved_before_another_or_to_the_end() {
    let mut laid = four_tracks();
    hue_shift_on_the_clip(&mut laid, 0, 0);
    let (first, second) = (laid.clip_member(0, 0), laid.clip_member(1, 0));
    let tag = laid.project.create_group(GroupKind::Tag, &[first, second]).unwrap();
    let other = laid.project.create_group(GroupKind::Tag, &[first]).unwrap();

    assert_eq!(
        laid.project.chain_of_the_member(first).unwrap().keys(true),
        [effect_key(), tag_key(tag), tag_key(other), TRACK_KEY],
        "the clip has the effect, then the badge of the tag, then of the other one"
    );
    laid.project
        .move_chain_item(first, tag_key(other), Some(effect_key()))
        .unwrap();
    assert_eq!(
        laid.project.chain_of_the_member(first).unwrap().keys(true),
        [tag_key(other), effect_key(), tag_key(tag), TRACK_KEY],
        "now the other one is before the effect"
    );
    laid.project.move_chain_item(first, tag_key(tag), None).unwrap();
    assert_eq!(
        laid.project.chain_of_the_member(first).unwrap().keys(true),
        [tag_key(other), effect_key(), tag_key(tag), TRACK_KEY],
        "to the end of what is shown, and the badge of the track is not shown (its track has no effects): it stays at the end"
    );

    // A group's chain.
    let clip_group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[first, second])
        .unwrap();
    laid.project.add_effect_to_the_group(clip_group, HUE_SHIFT).unwrap();
    let inside_a_tag = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Group(clip_group)])
        .unwrap();
    laid.project
        .move_chain_item(Member::Group(clip_group), tag_key(inside_a_tag), Some(effect_key()))
        .unwrap();
    assert_eq!(
        laid.project
            .chain_of_the_member(Member::Group(clip_group))
            .unwrap()
            .keys(false),
        [tag_key(inside_a_tag), effect_key()]
    );
}

#[test]
fn moving_something_to_the_end_of_what_is_shown_does_not_put_it_after_a_badge_of_a_track_that_is_not_shown() {
    let mut laid = four_tracks();
    let clip = laid.clip_member(0, 0);
    let first = laid.project.create_group(GroupKind::Tag, &[clip]).unwrap();
    let second = laid.project.create_group(GroupKind::Tag, &[clip]).unwrap();
    // The track has no effects, so the badge of it is not shown, and what is last is the second badge: put the first after it.
    assert_eq!(rows_of(&laid.project, clip).len(), 2);
    laid.project.move_chain_item(clip, tag_key(first), None).unwrap();
    assert_eq!(
        laid.project.chain_of_the_member(clip).unwrap().keys(true),
        [tag_key(second), tag_key(first), TRACK_KEY]
    );
    assert!(
        !laid
            .project
            .chain_of_the_member(clip)
            .unwrap()
            .keeps_the_badge_of_the_track(),
        "the badge of the track is where it was, which is at the end: it is not in the way of what is put there"
    );

    // When the track has effects the badge is shown, and "to the end" is after it.
    hue_shift_on_the_track(&mut laid, 0);
    laid.project.move_chain_item(clip, tag_key(second), None).unwrap();
    assert_eq!(
        laid.project.chain_of_the_member(clip).unwrap().keys(true),
        [tag_key(first), TRACK_KEY, tag_key(second)]
    );
    assert!(
        laid.project
            .chain_of_the_member(clip)
            .unwrap()
            .keeps_the_badge_of_the_track()
    );
}

#[test]
fn an_effect_of_a_track_is_moved_in_the_chain_of_the_track() {
    let mut laid = four_tracks();
    hue_shift_on_the_track(&mut laid, 2);
    let track = laid.track_member(2);
    laid.project.move_chain_item(track, effect_key(), None).unwrap();
    assert_eq!(
        laid.project.chain_of_the_member(track).unwrap().keys(false),
        [effect_key()]
    );
    assert!(
        laid.project.move_chain_item(track, TRACK_KEY, None).is_err(),
        "a track has no badge of a track, which is the badge of the track that a clip is on"
    );
}

#[test]
fn something_that_is_not_in_the_chain_is_not_moved_and_nothing_changes() {
    let mut laid = four_tracks();
    let clip = laid.clip_member(0, 0);
    let before = laid.project.clone();
    assert_eq!(
        laid.project.move_chain_item(clip, effect_key(), None),
        Err(EditError::NotInTheChain {
            owner: clip,
            item: effect_key()
        }),
        "it has no hue shift"
    );
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(1, 0)])
        .unwrap();
    let after_the_tag = laid.project.clone();
    assert_eq!(
        laid.project.move_chain_item(clip, tag_key(tag), None),
        Err(EditError::NotInTheChain {
            owner: clip,
            item: tag_key(tag)
        })
    );
    assert_eq!(laid.project, after_the_tag);
    assert_ne!(after_the_tag, before);
}

#[test]
fn something_that_is_not_there_has_no_chain_to_move_things_in() {
    let mut laid = four_tracks();
    let none = Member::Group(GroupId::new(9));
    assert_eq!(
        laid.project.move_chain_item(none, effect_key(), None),
        Err(EditError::UnknownGroup { group: GroupId::new(9) })
    );
    assert_eq!(
        laid.project
            .move_chain_item(Member::Track(TrackId::new(99)), effect_key(), None),
        Err(EditError::UnknownTrack {
            track: TrackId::new(99)
        })
    );
    let no_clip = ClipId::new(TrackId::new(99), 0);
    assert_eq!(
        laid.project.move_chain_item(Member::Clip(no_clip), effect_key(), None),
        Err(EditError::Clip(ClipEditError::UnknownClip { clip: no_clip }))
    );
}

// --- the effects of a group ---

#[test]
fn every_kind_of_group_can_have_every_effect_that_there_is_for_now() {
    for effect in EffectKind::ALL {
        for kind in GroupKind::ALL {
            assert!(effect.applies_to_a_group_of_kind(kind), "{effect} on a {kind}");
        }
    }
}

#[test]
fn an_effect_is_put_on_a_group_once_taken_off_and_its_value_is_changed() {
    let mut laid = four_tracks();
    let group = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    laid.project.add_effect_to_the_group(group, HUE_SHIFT).unwrap();
    assert_eq!(
        laid.project.add_effect_to_the_group(group, HUE_SHIFT),
        Err(EditError::TheGroupHasThatEffectAlready { group, kind: HUE_SHIFT })
    );
    laid.project
        .set_effect_value_of_the_group(group, PropertyId::HueShift, &PropertyValue::Number(90.0))
        .unwrap();
    assert_eq!(
        laid.project
            .group(group)
            .unwrap()
            .effects()
            .value_of(PropertyId::HueShift),
        Some(PropertyValue::Number(90.0))
    );
    assert_eq!(
        laid.project
            .set_effect_value_of_the_group(group, PropertyId::HueShift, &PropertyValue::Flag(true)),
        Err(EditError::NotAPropertyOfTheGroup {
            group,
            property: PropertyId::HueShift
        }),
        "not the kind of value it has"
    );
    assert_eq!(
        laid.project
            .set_effect_value_of_the_group(group, PropertyId::Gain, &PropertyValue::Number(1.0)),
        Err(EditError::NotAPropertyOfTheGroup {
            group,
            property: PropertyId::Gain
        })
    );
    laid.project.remove_effect_from_the_group(group, HUE_SHIFT).unwrap();
    assert_eq!(
        laid.project.remove_effect_from_the_group(group, HUE_SHIFT),
        Err(EditError::TheGroupDoesNotHaveThatEffect { group, kind: HUE_SHIFT })
    );
    assert_eq!(
        laid.project
            .set_effect_value_of_the_group(group, PropertyId::HueShift, &PropertyValue::Number(1.0)),
        Err(EditError::NotAPropertyOfTheGroup {
            group,
            property: PropertyId::HueShift
        }),
        "it has no hue shift now"
    );
}

#[test]
fn a_group_that_is_not_there_has_no_effects_put_on_or_taken_off_it() {
    let mut laid = four_tracks();
    let none = GroupId::new(5);
    assert_eq!(
        laid.project.add_effect_to_the_group(none, HUE_SHIFT),
        Err(EditError::UnknownGroup { group: none })
    );
    assert_eq!(
        laid.project.remove_effect_from_the_group(none, HUE_SHIFT),
        Err(EditError::UnknownGroup { group: none })
    );
    assert_eq!(
        laid.project
            .set_effect_value_of_the_group(none, PropertyId::HueShift, &PropertyValue::Number(1.0)),
        Err(EditError::UnknownGroup { group: none })
    );
}

// --- what is applied to a clip ---

#[test]
fn what_is_applied_to_a_clip_that_is_in_no_group_is_its_effects_and_then_those_of_its_track_as_it_always_was() {
    let mut laid = four_tracks();
    hue_shift_on_the_clip(&mut laid, 0, 0);
    hue_shift_on_the_track(&mut laid, 0);
    let (clip, track) = (laid.clip(0, 0), laid.tracks[0]);
    assert_eq!(owners(&laid.project, clip), [Member::Clip(clip), Member::Track(track)]);
    assert!(
        owners(&laid.project, laid.clip(1, 0)).is_empty(),
        "nothing is on it or on its track"
    );
    assert!(
        laid.project
            .effects_applied_to_the_clip(ClipId::new(TrackId::new(99), 0))
            .is_empty(),
        "no such clip"
    );
}

#[test]
fn the_effects_of_the_track_are_applied_where_the_badge_of_the_track_is_in_the_chain_of_the_clip() {
    let mut laid = four_tracks();
    hue_shift_on_the_clip(&mut laid, 0, 0);
    hue_shift_on_the_track(&mut laid, 0);
    let (clip, track) = (laid.clip(0, 0), laid.tracks[0]);
    laid.project
        .move_chain_item(Member::Clip(clip), TRACK_KEY, Some(effect_key()))
        .unwrap();
    assert_eq!(owners(&laid.project, clip), [Member::Track(track), Member::Clip(clip)]);
}

#[test]
fn the_effects_of_a_tag_are_applied_where_its_badge_is_in_the_chain_of_the_clip_and_before_those_of_the_track_to_begin_with()
 {
    let mut laid = four_tracks();
    hue_shift_on_the_clip(&mut laid, 0, 0);
    hue_shift_on_the_track(&mut laid, 0);
    let clip = laid.clip(0, 0);
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Clip(clip)])
        .unwrap();
    laid.project.add_effect_to_the_group(tag, HUE_SHIFT).unwrap();
    let track = laid.tracks[0];
    assert_eq!(
        owners(&laid.project, clip),
        [Member::Clip(clip), Member::Group(tag), Member::Track(track)],
        "the clip's own, the tag's, and then the track's, which is what the chain says"
    );

    laid.project
        .move_chain_item(Member::Clip(clip), tag_key(tag), Some(effect_key()))
        .unwrap();
    assert_eq!(
        owners(&laid.project, clip),
        [Member::Group(tag), Member::Clip(clip), Member::Track(track)]
    );
    laid.project
        .move_chain_item(Member::Clip(clip), TRACK_KEY, Some(tag_key(tag)))
        .unwrap();
    assert_eq!(
        owners(&laid.project, clip),
        [Member::Track(track), Member::Group(tag), Member::Clip(clip)]
    );
}

#[test]
fn a_clip_that_is_in_several_tags_has_their_effects_applied_in_the_order_of_its_badges() {
    let mut laid = four_tracks();
    let clip = laid.clip(0, 0);
    let first = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Clip(clip)])
        .unwrap();
    let second = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Clip(clip)])
        .unwrap();
    laid.project.add_effect_to_the_group(first, HUE_SHIFT).unwrap();
    laid.project.add_effect_to_the_group(second, HUE_SHIFT).unwrap();
    assert_eq!(
        owners(&laid.project, clip),
        [Member::Group(first), Member::Group(second)]
    );
    laid.project
        .move_chain_item(Member::Clip(clip), tag_key(second), Some(tag_key(first)))
        .unwrap();
    assert_eq!(
        owners(&laid.project, clip),
        [Member::Group(second), Member::Group(first)]
    );
}

#[test]
fn the_effects_of_the_clip_groups_a_clip_is_in_come_after_its_own_and_its_track_s_the_nearest_first() {
    let mut laid = four_tracks();
    hue_shift_on_the_clip(&mut laid, 0, 0);
    hue_shift_on_the_track(&mut laid, 0);
    let clip = laid.clip(0, 0);
    let inner = laid
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(clip), laid.clip_member(1, 0)])
        .unwrap();
    let outer = laid
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Group(inner), laid.clip_member(2, 0)])
        .unwrap();
    laid.project.add_effect_to_the_group(outer, HUE_SHIFT).unwrap();
    laid.project.add_effect_to_the_group(inner, HUE_SHIFT).unwrap();
    assert_eq!(
        owners(&laid.project, clip),
        [
            Member::Clip(clip),
            Member::Track(laid.tracks[0]),
            Member::Group(inner),
            Member::Group(outer)
        ]
    );
    assert_eq!(
        owners(&laid.project, laid.clip(2, 0)),
        [Member::Group(outer)],
        "a clip that is only in the outer group has only its effects"
    );
}

#[test]
fn the_effects_of_the_track_groups_a_clips_track_is_in_come_last_the_nearest_first() {
    let mut laid = four_tracks();
    let clip = laid.clip(0, 0);
    let clip_group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[Member::Clip(clip), laid.clip_member(1, 0)])
        .unwrap();
    let inner_tracks = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(0), laid.track_member(1)])
        .unwrap();
    let outer_tracks = laid
        .project
        .create_group(
            GroupKind::TrackGroup,
            &[Member::Group(inner_tracks), laid.track_member(2)],
        )
        .unwrap();
    for group in [outer_tracks, inner_tracks, clip_group] {
        laid.project.add_effect_to_the_group(group, HUE_SHIFT).unwrap();
    }
    assert_eq!(
        owners(&laid.project, clip),
        [
            Member::Group(clip_group),
            Member::Group(inner_tracks),
            Member::Group(outer_tracks)
        ]
    );
    assert_eq!(
        owners(&laid.project, laid.clip(2, 0)),
        [Member::Group(outer_tracks)],
        "on a track that is only in the outer group"
    );
    assert!(
        owners(&laid.project, laid.clip(3, 0)).is_empty(),
        "on no track of a track group"
    );
}

#[test]
fn a_tag_that_a_clip_group_is_in_has_its_effects_applied_to_the_group_where_its_badge_is_in_the_chain_of_the_group() {
    let mut laid = four_tracks();
    let (x, y) = (laid.clip_member(0, 0), laid.clip_member(1, 0));
    let clip_group = laid.project.create_group(GroupKind::ClipGroup, &[x, y]).unwrap();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Group(clip_group)])
        .unwrap();
    laid.project.add_effect_to_the_group(tag, HUE_SHIFT).unwrap();
    laid.project.add_effect_to_the_group(clip_group, HUE_SHIFT).unwrap();
    let clip = laid.clip(0, 0);
    assert_eq!(
        owners(&laid.project, clip),
        [Member::Group(tag), Member::Group(clip_group)],
        "the badge of the tag was put in the chain of the group first, and its own effect after"
    );
    laid.project
        .move_chain_item(Member::Group(clip_group), tag_key(tag), None)
        .unwrap();
    assert_eq!(
        owners(&laid.project, clip),
        [Member::Group(clip_group), Member::Group(tag)]
    );
    assert_eq!(x, Member::Clip(clip), "(it is the clip that was asked about)");
}

#[test]
fn a_tag_that_is_in_a_tag_has_the_effects_of_the_outer_one_applied_where_its_badge_is_in_the_chain_of_the_inner_one() {
    let mut laid = four_tracks();
    let x = laid.clip_member(0, 0);
    let inner = laid.project.create_group(GroupKind::Tag, &[x]).unwrap();
    let outer = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Group(inner)])
        .unwrap();
    laid.project.add_effect_to_the_group(inner, HUE_SHIFT).unwrap();
    laid.project.add_effect_to_the_group(outer, HUE_SHIFT).unwrap();
    let clip = laid.clip(0, 0);
    assert_eq!(
        owners(&laid.project, clip),
        [Member::Group(outer), Member::Group(inner)],
        "the badge of the outer tag was put in the chain of the inner one first, and its effect after"
    );
    laid.project
        .move_chain_item(Member::Group(inner), tag_key(outer), None)
        .unwrap();
    assert_eq!(
        owners(&laid.project, clip),
        [Member::Group(inner), Member::Group(outer)]
    );
}

#[test]
fn a_tag_that_a_track_group_is_in_has_its_effects_applied_to_the_clips_on_the_tracks_of_the_group() {
    let mut laid = four_tracks();
    let track_group = laid
        .project
        .create_group(GroupKind::TrackGroup, &[laid.track_member(0), laid.track_member(1)])
        .unwrap();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Group(track_group)])
        .unwrap();
    laid.project.add_effect_to_the_group(tag, HUE_SHIFT).unwrap();
    assert_eq!(owners(&laid.project, laid.clip(0, 0)), [Member::Group(tag)]);
    assert_eq!(owners(&laid.project, laid.clip(1, 0)), [Member::Group(tag)]);
    assert!(owners(&laid.project, laid.clip(2, 0)).is_empty());
}

#[test]
fn a_group_in_itself_does_not_make_what_is_applied_go_on_for_ever() {
    let mut laid = four_tracks();
    let clip = laid.clip(0, 0);
    let (first, second) = (GroupId::new(0), GroupId::new(1));
    laid.project.groups = vec![
        Group::new(first, GroupKind::Tag, "a", GroupColour::PALETTE[0])
            .with_members([Member::Clip(clip), Member::Group(second)]),
        Group::new(second, GroupKind::Tag, "b", GroupColour::PALETTE[1]).with_members([Member::Group(first)]),
    ];
    for group in [first, second] {
        laid.project
            .put_the_badge_of_a_tag_in_the_chain_of(Member::Clip(clip), group);
        laid.project.add_effect_to_the_group(group, HUE_SHIFT).unwrap();
    }
    laid.project
        .put_the_badge_of_a_tag_in_the_chain_of(Member::Group(second), first);
    laid.project
        .put_the_badge_of_a_tag_in_the_chain_of(Member::Group(first), second);
    let applied = owners(&laid.project, clip);
    assert!(applied.len() < 20, "it ends: {applied:?}");
    assert!(applied.contains(&Member::Group(first)));
}

#[test]
fn effects_that_do_not_apply_to_a_clip_of_its_kind_are_left_out() {
    let mut laid = four_tracks();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    laid.project.add_effect_to_the_group(tag, HUE_SHIFT).unwrap();
    let crate_clip = laid
        .project
        .insert_crate(laid.tracks[1], TimelineFrameIndex::new(20), 10)
        .unwrap();
    laid.project.add_to_the_group(tag, Member::Clip(crate_clip)).unwrap();
    assert_eq!(owners(&laid.project, laid.clip(0, 0)), [Member::Group(tag)]);
    assert!(
        owners(&laid.project, crate_clip).is_empty(),
        "nothing is seen of a crate, so nothing is done to it"
    );
}

#[test]
fn what_is_applied_to_every_clip_is_what_is_applied_to_each() {
    let mut laid = four_tracks();
    hue_shift_on_the_clip(&mut laid, 0, 0);
    hue_shift_on_the_track(&mut laid, 1);
    let clip_group = laid
        .project
        .create_group(GroupKind::ClipGroup, &[laid.clip_member(0, 0), laid.clip_member(1, 0)])
        .unwrap();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[Member::Group(clip_group), laid.clip_member(3, 0)])
        .unwrap();
    laid.project.add_effect_to_the_group(clip_group, HUE_SHIFT).unwrap();
    laid.project.add_effect_to_the_group(tag, HUE_SHIFT).unwrap();
    let all = laid.project.effects_applied_to_every_clip();
    assert_eq!(all.len(), 4);
    for track in 0..4 {
        let clip = laid.clip(track, 0);
        assert_eq!(
            all[&clip],
            laid.project.effects_applied_to_the_clip(clip),
            "the clip on track {track}"
        );
    }
}

#[test]
fn an_applied_effect_is_the_effect_itself_with_its_own_values() {
    let mut laid = four_tracks();
    let tag = laid
        .project
        .create_group(GroupKind::Tag, &[laid.clip_member(0, 0)])
        .unwrap();
    laid.project.add_effect_to_the_group(tag, HUE_SHIFT).unwrap();
    laid.project
        .set_effect_value_of_the_group(tag, PropertyId::HueShift, &PropertyValue::Number(45.0))
        .unwrap();
    let applied = laid.project.effects_applied_to_the_clip(laid.clip(0, 0));
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].effect.values(), [PropertyValue::Number(45.0)]);
    assert_eq!(applied[0].owner, Member::Group(tag));
}

#[test]
fn an_animation_target_is_the_clip_or_the_track_that_it_is_the_member_of() {
    let clip = ClipId::new(TrackId::new(1), 2);
    assert_eq!(Member::from(AnimationTarget::Clip(clip)), Member::Clip(clip));
    assert_eq!(
        Member::from(AnimationTarget::Track(TrackId::new(3))),
        Member::Track(TrackId::new(3))
    );
}

// --- the placeholder: group effects are applied to each member, not to what the members make together ---

/// A pixel with a straight (not multiplied by how clear it is) colour: red, green, blue and how clear it is, each from 0 to 255.
type Pixel = [u8; 4];

/// `top` laid over `bottom`, as straight colours are laid over one another, rounded to the nearest of 0 to 255.
fn over(top: Pixel, bottom: Pixel) -> Pixel {
    let (top_opacity, bottom_opacity) = (f64::from(top[3]) / 255.0, f64::from(bottom[3]) / 255.0);
    let opacity = top_opacity + bottom_opacity * (1.0 - top_opacity);
    if opacity == 0.0 {
        return [0; 4];
    }
    let colour = |index: usize| {
        let mixed = (f64::from(top[index]) * top_opacity
            + f64::from(bottom[index]) * bottom_opacity * (1.0 - top_opacity))
            / opacity;
        (mixed + 0.5) as u8
    };
    [colour(0), colour(1), colour(2), (opacity * 255.0 + 0.5) as u8]
}

fn changed_by(matrix: &ColourMatrix, pixel: Pixel) -> Pixel {
    let mut changed = pixel;
    matrix.apply_to_rgba_pixels(&mut changed);
    changed
}

/// How many of the 255 steps of a colour are between what the effect makes of the two layers laid over one another (what a clip group's
/// effect is meant to do) and what it makes of each layer, laid over one another (what is done until the pictures of groups can be made).
fn how_far_off_the_placeholder_is(degrees: f32, top: Pixel, bottom: Pixel) -> u8 {
    let matrix = ColourMatrix::hue_rotation(degrees);
    let meant = changed_by(&matrix, over(top, bottom));
    let placeholder = over(changed_by(&matrix, top), changed_by(&matrix, bottom));
    (0..3)
        .map(|index| meant[index].abs_diff(placeholder[index]))
        .max()
        .unwrap()
}

#[test]
fn the_placeholder_makes_the_picture_that_the_effect_is_meant_to_make_when_the_members_are_not_clear_at_all() {
    // The effect that is applied to what the members make together, and to each of them, makes the same picture where nothing is
    // partly clear: the one in front is what is seen, and what is done to it is what is done to the picture.
    let levels = [0u8, 85, 170, 255];
    let mut tried = 0;
    for red in levels {
        for green in levels {
            for blue in levels {
                for other in levels {
                    for degrees in (-180..=180).step_by(15) {
                        let top = [red, green, blue, 255];
                        let bottom = [other, red, green, 255];
                        assert_eq!(
                            how_far_off_the_placeholder_is(degrees as f32, top, bottom),
                            0,
                            "{top:?} over {bottom:?}, {degrees}"
                        );
                        tried += 1;
                    }
                }
            }
        }
    }
    assert_eq!(tried, 4 * 4 * 4 * 4 * 25);
}

#[test]
fn the_placeholder_makes_the_same_picture_where_no_colour_is_cut_off_even_when_the_members_are_partly_clear() {
    // Greys do not change at all, and these mild colours stay inside the limits for the turns tried: the sum of a change is the change
    // of a sum, and nothing is cut off to spoil that (but for a step of rounding).
    for degrees in (-180..=180).step_by(5) {
        assert_eq!(
            how_far_off_the_placeholder_is(degrees as f32, [128, 128, 128, 128], [90, 90, 90, 102]),
            0,
            "{degrees}"
        );
    }
    let worst = (-180..=180)
        .map(|degrees| how_far_off_the_placeholder_is(degrees as f32, [160, 120, 100, 128], [100, 120, 160, 102]))
        .max()
        .unwrap();
    assert!(worst <= 1, "{worst}");
}

/// What the placeholder gets wrong, written down so that it is known: this test is meant to stop passing when the pictures of groups can be
/// made, and the placeholder goes. It is *not* a limit that the program promises to keep: the difference depends on how vivid the colours are
/// and how clear the one in front is, and is larger for more vivid colours (a random search over colours, clearness and turns found
/// differences of up to about 100 of the 255 steps; between layers that are not clear at all there is none).
#[test]
fn the_placeholder_is_off_where_a_colour_is_cut_off_and_the_members_are_partly_clear() {
    // Two layers, a blue one 40% clear over an orange one 50% clear, and a turn of 120 degrees: what ffmpeg (the export) measured, for
    // the same effect applied to the composite and to each layer, was 14 steps in one colour. The arithmetic here is the same.
    let blue_over_orange =
        |degrees: f32| how_far_off_the_placeholder_is(degrees, [32, 128, 255, 102], [255, 128, 64, 128]);
    assert_eq!(blue_over_orange(120.0), 14);
    let worst = (-180..=180)
        .map(|degrees| blue_over_orange(degrees as f32))
        .max()
        .unwrap();
    assert!(
        (14..=25).contains(&worst),
        "the worst turn for these two colours: {worst}"
    );

    // A layer that is half clear over one that is not clear at all.
    let worst_over_opaque = (-180..=180)
        .map(|degrees| how_far_off_the_placeholder_is(degrees as f32, [255, 128, 64, 128], [32, 128, 255, 255]))
        .max()
        .unwrap();
    assert!((10..=25).contains(&worst_over_opaque), "{worst_over_opaque}");
}
