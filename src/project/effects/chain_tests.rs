//! The chain of a clip, a track or a group: its effects, with the badges that say where the effects of something else are applied, and
//! how they are moved.

use super::*;
use crate::project::groups::GroupId;

const HUE_SHIFT: ChainItemKey = ChainItemKey::Effect(EffectKind::HueShift);
const TRACK: ChainItemKey = ChainItemKey::Badge(Badge::Track);

fn tag(number: u32) -> GroupId {
    GroupId::new(number)
}

fn tag_badge(number: u32) -> ChainItemKey {
    ChainItemKey::Badge(Badge::Tag(tag(number)))
}

/// A chain that has the effect and the badges of the tags that are given, put in in that order.
fn chain_of(effect: bool, tags: &[u32]) -> Effects {
    let mut chain = Effects::none();
    if effect {
        chain.add(EffectKind::HueShift);
    }
    for number in tags {
        chain.add_tag_badge(tag(*number));
    }
    chain
}

// --- what a chain is to begin with ---

#[test]
fn a_chain_that_has_nothing_in_it_is_the_chain_of_a_clip_that_has_no_effects_and_is_in_no_tag() {
    let chain = Effects::none();
    assert!(chain.has_nothing_in_it());
    assert!(chain.is_empty());
    assert_eq!(chain.len(), 0);
    assert_eq!(chain, Effects::default());
    assert_eq!(chain.keys(false), Vec::<ChainItemKey>::new());
    assert_eq!(
        chain.keys(true),
        [TRACK],
        "the badge of the track is in the chain of a clip, last, without being kept in it"
    );
    assert!(!chain.keeps_the_badge_of_the_track());
}

#[test]
fn the_badge_of_the_track_is_last_in_the_chain_of_a_clip_until_it_is_moved_and_is_not_in_the_chain_of_anything_else() {
    let chain = chain_of(true, &[1, 2]);
    assert_eq!(chain.keys(true), [HUE_SHIFT, tag_badge(1), tag_badge(2), TRACK]);
    assert_eq!(
        chain.keys(false),
        [HUE_SHIFT, tag_badge(1), tag_badge(2)],
        "a track or a group has none"
    );
    assert!(!chain.keeps_the_badge_of_the_track());
}

#[test]
fn the_badge_of_the_track_is_shown_in_the_chain_of_a_clip_wherever_it_was_put_and_never_in_the_chain_of_a_track_or_a_group()
 {
    let mut chain = chain_of(true, &[1]);
    assert!(chain.move_before(TRACK, Some(HUE_SHIFT), true));
    assert!(chain.keeps_the_badge_of_the_track());

    assert_eq!(chain.keys(true), [TRACK, HUE_SHIFT, tag_badge(1)]);
    assert_eq!(
        chain.keys(false),
        [HUE_SHIFT, tag_badge(1)],
        "what has the chain says whether the badge is in it: a track or a group never shows it, kept or not"
    );
}

#[test]
fn what_is_put_in_the_chain_goes_before_the_badge_of_the_track_while_that_is_at_the_end() {
    let mut chain = chain_of(false, &[1]);
    chain.add(EffectKind::HueShift);
    chain.add_tag_badge(tag(2));
    assert_eq!(
        chain.keys(true),
        [tag_badge(1), HUE_SHIFT, tag_badge(2), TRACK],
        "the effects of the clip and its tags, and then those of its track, as it was before the badge could be moved"
    );
}

// --- the badge of the track ---

#[test]
fn the_badge_of_the_track_that_is_moved_away_from_the_end_is_kept_where_it_was_put_and_what_comes_later_goes_after_it()
{
    let mut chain = chain_of(true, &[1]);
    assert!(chain.move_before(TRACK, Some(HUE_SHIFT), true));
    assert!(chain.keeps_the_badge_of_the_track());
    assert!(!chain.has_nothing_in_it());
    assert_eq!(chain.keys(true), [TRACK, HUE_SHIFT, tag_badge(1)]);

    chain.add_tag_badge(tag(2));
    assert_eq!(
        chain.keys(true),
        [TRACK, HUE_SHIFT, tag_badge(1), tag_badge(2)],
        "it is not at the end, so it does not keep what is put in later before it"
    );
}

#[test]
fn the_badge_of_the_track_that_is_moved_back_to_the_end_is_as_it_began() {
    let began = chain_of(true, &[1]);
    let mut chain = began.clone();
    assert!(chain.move_before(TRACK, Some(HUE_SHIFT), true));
    assert_ne!(chain, began);
    assert!(chain.move_before(TRACK, None, true));
    assert!(!chain.keeps_the_badge_of_the_track());
    assert_eq!(chain, began, "the same chain, not one that says the same");
    assert_eq!(chain.keys(true), [HUE_SHIFT, tag_badge(1), TRACK]);
}

#[test]
fn the_badge_of_the_track_is_not_kept_when_what_came_after_it_is_taken_away() {
    let mut chain = chain_of(true, &[]);
    assert!(chain.move_before(TRACK, Some(HUE_SHIFT), true));
    assert!(chain.keeps_the_badge_of_the_track());
    assert!(chain.remove(EffectKind::HueShift));
    assert!(!chain.keeps_the_badge_of_the_track());
    assert_eq!(chain, Effects::none(), "it is the chain that has nothing in it again");

    let mut chain = chain_of(false, &[3]);
    chain.move_before(TRACK, Some(tag_badge(3)), true);
    assert!(chain.keeps_the_badge_of_the_track());
    assert!(chain.remove_tag_badge(tag(3)));
    assert_eq!(chain, Effects::none());
}

#[test]
fn something_moved_to_the_very_end_goes_after_the_badge_of_the_track() {
    let mut chain = chain_of(true, &[1, 2]);
    assert!(chain.move_before(tag_badge(1), None, true));
    assert_eq!(
        chain.keys(true),
        [HUE_SHIFT, tag_badge(2), TRACK, tag_badge(1)],
        "the end is after the badge of the track, which is not at the end any more"
    );
    assert!(chain.keeps_the_badge_of_the_track());
}

#[test]
fn the_badge_of_the_track_cannot_be_moved_in_the_chain_of_anything_but_a_clip() {
    let mut chain = chain_of(true, &[1]);
    let before = chain.clone();
    assert!(!chain.move_before(TRACK, None, false));
    assert!(!chain.move_before(TRACK, Some(HUE_SHIFT), false));
    assert!(
        !chain.move_before(HUE_SHIFT, Some(TRACK), false),
        "it is not there to go before"
    );
    assert_eq!(chain, before);
}

// --- moving ---

#[test]
fn an_item_is_moved_to_right_before_another_or_to_the_end_and_the_others_keep_their_order() {
    let mut chain = chain_of(true, &[1, 2]);
    assert!(chain.move_before(tag_badge(2), Some(HUE_SHIFT), true));
    assert_eq!(chain.keys(true), [tag_badge(2), HUE_SHIFT, tag_badge(1), TRACK]);

    assert!(chain.move_before(HUE_SHIFT, Some(tag_badge(1)), true));
    assert_eq!(
        chain.keys(true),
        [tag_badge(2), HUE_SHIFT, tag_badge(1), TRACK],
        "already there"
    );

    assert!(chain.move_before(tag_badge(2), Some(TRACK), true));
    assert_eq!(chain.keys(true), [HUE_SHIFT, tag_badge(1), tag_badge(2), TRACK]);
    assert!(!chain.keeps_the_badge_of_the_track());
}

#[test]
fn an_effect_is_moved_among_the_badges_like_a_badge_is() {
    let mut chain = chain_of(true, &[1, 2]);
    assert!(chain.move_before(HUE_SHIFT, None, false));
    assert_eq!(chain.keys(false), [tag_badge(1), tag_badge(2), HUE_SHIFT]);
    assert!(chain.move_before(HUE_SHIFT, Some(tag_badge(2)), false));
    assert_eq!(chain.keys(false), [tag_badge(1), HUE_SHIFT, tag_badge(2)]);
    assert_eq!(
        chain.get(EffectKind::HueShift),
        Some(&Effect::new(EffectKind::HueShift)),
        "it is the same effect"
    );
}

#[test]
fn moving_something_before_itself_changes_nothing_and_moving_what_is_not_there_is_refused() {
    let mut chain = chain_of(true, &[1]);
    let before = chain.clone();
    assert!(chain.move_before(tag_badge(1), Some(tag_badge(1)), true));
    assert_eq!(chain, before);

    assert!(!chain.move_before(tag_badge(9), None, true), "no such badge");
    assert!(!chain.move_before(tag_badge(9), Some(HUE_SHIFT), true));
    assert!(
        !chain.move_before(HUE_SHIFT, Some(tag_badge(9)), true),
        "nothing to go before"
    );
    assert_eq!(chain, before, "refused moves change nothing");
}

#[test]
fn moving_an_effect_keeps_the_values_it_has() {
    let mut chain = chain_of(true, &[1]);
    assert!(chain.set_value(PropertyId::HueShift, &PropertyValue::Number(77.0)));
    assert!(chain.move_before(HUE_SHIFT, None, false));
    assert_eq!(chain.value_of(PropertyId::HueShift), Some(PropertyValue::Number(77.0)));
}

// --- badges of tags ---

#[test]
fn the_badge_of_a_tag_is_put_in_the_chain_once_and_taken_out_when_it_is_there() {
    let mut chain = Effects::none();
    assert!(chain.add_tag_badge(tag(4)));
    assert!(chain.add_tag_badge(tag(2)));
    assert!(!chain.add_tag_badge(tag(4)), "it is there");
    assert_eq!(
        chain.tag_badges().collect::<Vec<_>>(),
        [tag(4), tag(2)],
        "in the order they are in the chain"
    );
    assert!(chain.has_the_tag_badge(tag(2)) && !chain.has_the_tag_badge(tag(3)));

    assert!(chain.remove_tag_badge(tag(4)));
    assert!(!chain.remove_tag_badge(tag(4)), "it is not there");
    assert_eq!(chain.tag_badges().collect::<Vec<_>>(), [tag(2)]);
}

#[test]
fn a_badge_is_replaced_by_the_badges_of_other_tags_in_the_place_it_was() {
    let mut chain = chain_of(true, &[1, 2]);
    assert!(chain.replace_tag_badge(tag(1), &[tag(5), tag(6)]));
    assert_eq!(chain.keys(false), [HUE_SHIFT, tag_badge(5), tag_badge(6), tag_badge(2)]);

    assert!(
        chain.replace_tag_badge(tag(2), &[tag(5)]),
        "one that is there already stays where it is"
    );
    assert_eq!(chain.keys(false), [HUE_SHIFT, tag_badge(5), tag_badge(6)]);

    assert!(chain.replace_tag_badge(tag(6), &[]), "replaced by none is taken out");
    assert_eq!(chain.keys(false), [HUE_SHIFT, tag_badge(5)]);

    let before = chain.clone();
    assert!(!chain.replace_tag_badge(tag(9), &[tag(1)]), "there was none to replace");
    assert_eq!(chain, before);
}

#[test]
fn a_badge_is_not_replaced_by_itself_twice_over() {
    let mut chain = chain_of(false, &[1, 2]);
    assert!(chain.replace_tag_badge(tag(1), &[tag(1), tag(7)]));
    assert_eq!(
        chain.keys(false),
        [tag_badge(7), tag_badge(2)],
        "the badge that was replaced is not put back"
    );
}

// --- the effects among the badges ---

#[test]
fn the_effects_are_counted_and_listed_without_the_badges() {
    let chain = chain_of(false, &[1, 2]);
    assert!(chain.is_empty(), "there is no effect, though there are badges");
    assert_eq!(chain.len(), 0);
    assert_eq!(chain.iter().len(), 0);
    assert!(!chain.has_nothing_in_it(), "but there is something in it");

    let chain = chain_of(true, &[1, 2]);
    assert!(!chain.is_empty());
    assert_eq!(chain.len(), 1);
    assert_eq!(chain.iter().len(), 1, "the iterator knows how many there are");
    assert_eq!(
        chain.iter().map(Effect::kind).collect::<Vec<_>>(),
        [EffectKind::HueShift]
    );
}

#[test]
fn the_items_of_a_chain_are_the_effects_themselves_and_the_badges_in_the_order_they_are_applied_in() {
    let chain = chain_of(true, &[3]);
    let items = chain.items(true);
    assert_eq!(items.len(), 3);
    assert_eq!(items[0], ChainItem::Effect(&Effect::new(EffectKind::HueShift)));
    assert_eq!(items[1], ChainItem::Badge(Badge::Tag(tag(3))));
    assert_eq!(items[2], ChainItem::Badge(Badge::Track));
    assert_eq!(items.iter().map(ChainItem::key).collect::<Vec<_>>(), chain.keys(true));
}

#[test]
fn an_effect_that_is_taken_off_leaves_the_badges_where_they_were() {
    let mut chain = chain_of(true, &[1, 2]);
    assert!(chain.remove(EffectKind::HueShift));
    assert_eq!(chain.keys(false), [tag_badge(1), tag_badge(2)]);
    assert!(!chain.remove(EffectKind::HueShift), "there was none");
}

#[test]
fn chains_with_the_same_things_in_the_same_order_are_equal_and_those_in_another_order_are_not() {
    assert_eq!(chain_of(true, &[1, 2]), chain_of(true, &[1, 2]));
    assert_ne!(chain_of(true, &[1, 2]), chain_of(true, &[2, 1]));
    let mut moved = chain_of(true, &[1, 2]);
    moved.move_before(HUE_SHIFT, None, false);
    assert_ne!(moved, chain_of(true, &[1, 2]), "the same things, in another order");
    assert_ne!(chain_of(true, &[]), chain_of(false, &[]));
}
