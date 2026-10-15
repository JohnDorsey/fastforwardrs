// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The kinds of group, what each can hold, the names of things in groups, and the colours of groups.

use super::*;

fn clip(track: u32, number: u32) -> ClipId {
    ClipId::new(TrackId::new(track), number)
}

// --- the kinds ---

#[test]
fn the_three_kinds_are_called_what_the_people_who_use_the_program_call_them() {
    assert_eq!(
        GroupKind::ALL,
        [GroupKind::Tag, GroupKind::ClipGroup, GroupKind::TrackGroup]
    );
    assert_eq!(GroupKind::Tag.name(), "tag");
    assert_eq!(GroupKind::ClipGroup.name(), "clip group");
    assert_eq!(GroupKind::TrackGroup.name(), "track group");
    assert_eq!(GroupKind::Tag.name_with_a_capital(), "Tag");
    assert_eq!(GroupKind::ClipGroup.name_with_a_capital(), "Clip group");
    assert_eq!(GroupKind::TrackGroup.name_with_a_capital(), "Track group");
    for kind in GroupKind::ALL {
        assert_eq!(kind.to_string(), kind.name());
        assert_eq!(kind.name_with_a_capital().to_lowercase(), kind.name());
    }
}

#[test]
fn a_tag_holds_clips_and_groups_of_any_kind_a_clip_group_clips_and_clip_groups_and_a_track_group_tracks_and_track_groups()
 {
    for kind in GroupKind::ALL {
        let holds_a_clip = kind.can_hold_a_clip();
        let holds_a_track = kind.can_hold_a_track();
        let groups_it_holds: Vec<GroupKind> = GroupKind::ALL
            .into_iter()
            .filter(|member| kind.can_hold_a_group_of_kind(*member))
            .collect();
        match kind {
            GroupKind::Tag => {
                assert!(holds_a_clip && !holds_a_track, "{kind}");
                assert_eq!(groups_it_holds, GroupKind::ALL, "{kind}");
            }
            GroupKind::ClipGroup => {
                assert!(holds_a_clip && !holds_a_track, "{kind}");
                assert_eq!(groups_it_holds, [GroupKind::ClipGroup], "{kind}");
            }
            GroupKind::TrackGroup => {
                assert!(!holds_a_clip && holds_a_track, "{kind}");
                assert_eq!(groups_it_holds, [GroupKind::TrackGroup], "{kind}");
            }
        }
    }
}

#[test]
fn a_clip_group_and_a_track_group_are_shapes_that_what_is_in_them_is_inside_and_a_tag_is_not() {
    assert!(!GroupKind::Tag.is_a_shape());
    assert!(GroupKind::ClipGroup.is_a_shape());
    assert!(GroupKind::TrackGroup.is_a_shape());
}

// --- ids and members ---

#[test]
fn a_group_is_named_by_a_number_and_a_member_is_a_clip_a_track_or_a_group() {
    assert_eq!(GroupId::new(7).value(), 7);
    assert_eq!(GroupId::new(7).to_string(), "group 7");
    assert!(GroupId::new(2) < GroupId::new(3));

    let member_of_a_clip: Member = clip(1, 4).into();
    let member_of_a_track: Member = TrackId::new(2).into();
    let member_of_a_group: Member = GroupId::new(5).into();
    assert_eq!(member_of_a_clip, Member::Clip(clip(1, 4)));
    assert_eq!(member_of_a_track, Member::Track(TrackId::new(2)));
    assert_eq!(member_of_a_group, Member::Group(GroupId::new(5)));
    assert_eq!(member_of_a_clip.to_string(), clip(1, 4).to_string());
    assert_eq!(member_of_a_track.to_string(), "track 2");
    assert_eq!(member_of_a_group.to_string(), "group 5");
}

#[test]
fn a_group_is_made_from_its_parts_and_tells_what_they_are() {
    let colour = GroupColour::new(1, 2, 3);
    let bare = Group::new(GroupId::new(3), GroupKind::ClipGroup, "intro", colour);
    assert_eq!(bare.id(), GroupId::new(3));
    assert_eq!(bare.kind(), GroupKind::ClipGroup);
    assert_eq!(bare.name(), "intro");
    assert_eq!(bare.colour(), colour);
    assert!(bare.members().is_empty());
    assert!(bare.effects().has_nothing_in_it());

    let mut effects = Effects::none();
    effects.add(crate::project::EffectKind::HueShift);
    let group = bare
        .with_members([Member::Clip(clip(0, 1)), Member::Clip(clip(2, 0))])
        .with_effects(effects.clone());
    assert_eq!(group.members(), [Member::Clip(clip(0, 1)), Member::Clip(clip(2, 0))]);
    assert!(group.has_as_a_member(Member::Clip(clip(2, 0))));
    assert!(!group.has_as_a_member(Member::Clip(clip(2, 1))));
    assert!(
        !group.has_as_a_member(Member::Track(TrackId::new(2))),
        "a clip is not a track"
    );
    assert_eq!(group.effects(), &effects);
}

// --- colours ---

/// The hue of a colour in degrees.
fn hue_of(colour: GroupColour) -> f64 {
    let [red, green, blue] = [colour.red, colour.green, colour.blue].map(|channel| f64::from(channel) / 255.0);
    let (largest, smallest) = (red.max(green).max(blue), red.min(green).min(blue));
    let spread = largest - smallest;
    if spread == 0.0 {
        0.0
    } else if largest == red {
        60.0 * ((green - blue) / spread).rem_euclid(6.0)
    } else if largest == green {
        60.0 * ((blue - red) / spread + 2.0)
    } else {
        60.0 * ((red - green) / spread + 4.0)
    }
}

fn contrast(first: f64, second: f64) -> f64 {
    (first.max(second) + 0.05) / (first.min(second) + 0.05)
}

fn luminance_of(colour: GroupColour) -> f64 {
    colour.relative_luminance()
}

/// How far apart two hues are on the wheel of colours, in degrees: at most 180.
fn degrees_apart(first: f64, second: f64) -> f64 {
    let difference = (first - second).abs() % 360.0;
    difference.min(360.0 - difference)
}

#[test]
fn the_colours_new_groups_are_given_are_eight_that_can_be_told_apart_and_follow_each_other_far_apart() {
    let palette = GroupColour::PALETTE;
    assert_eq!(palette.len(), 8);
    for (index, first) in palette.iter().enumerate() {
        for second in &palette[index + 1..] {
            assert_ne!(first, second);
            let apart = degrees_apart(hue_of(*first), hue_of(*second));
            assert!(apart >= 33.0, "{first:?} and {second:?} are too much alike: {apart}");
        }
    }
    for pair in palette.windows(2) {
        let apart = degrees_apart(hue_of(pair[0]), hue_of(pair[1]));
        assert!(
            apart >= 60.0,
            "{:?} and {:?} follow each other and are too much alike: {apart}",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn the_colours_are_away_from_the_yellow_that_shows_what_is_selected_the_orange_of_the_keyframe_buttons_and_the_red_of_the_playhead()
 {
    // (The colours of the theme: the selection outline is (255, 204, 51), the keyframe buttons (255, 150, 24), the playhead (255, 92, 72).)
    let mut least = f64::MAX;
    for avoided in [
        GroupColour::new(255, 204, 51),
        GroupColour::new(255, 150, 24),
        GroupColour::new(255, 92, 72),
    ] {
        for colour in GroupColour::PALETTE {
            least = least.min(degrees_apart(hue_of(colour), hue_of(avoided)));
        }
    }
    assert!(least >= 36.0, "the documentation says 36 degrees: {least}");
}

#[test]
fn every_colour_can_be_seen_on_the_dark_of_the_window() {
    let dark = luminance_of(GroupColour::new(34, 36, 40));
    for colour in GroupColour::PALETTE {
        assert!(contrast(luminance_of(colour), dark) >= 3.6, "{colour:?}");
    }
}

#[test]
fn writing_on_a_colour_is_white_or_black_whichever_can_be_read_better() {
    assert!(GroupColour::new(0, 0, 0).writing_on_it_is_white());
    assert!(!GroupColour::new(255, 255, 255).writing_on_it_is_white());
    assert!(GroupColour::new(20, 30, 120).writing_on_it_is_white(), "dark blue");
    assert!(
        !GroupColour::new(250, 240, 120).writing_on_it_is_white(),
        "light yellow"
    );
    for colour in GroupColour::PALETTE {
        let luminance = luminance_of(colour);
        let with_white = contrast(1.0, luminance);
        let with_black = contrast(0.0, luminance);
        assert_eq!(colour.writing_on_it_is_white(), with_white > with_black, "{colour:?}");
        assert!(
            with_white.max(with_black) >= 4.9,
            "the writing can be read on {colour:?}"
        );
    }
}
