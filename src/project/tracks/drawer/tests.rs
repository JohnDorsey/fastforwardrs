use super::*;
use crate::project::PropertyValue;
use crate::project::tracks::ids::{ClipId, TrackId};

fn track() -> TrackId {
    TrackId::new(0)
}

fn clip(number: u32) -> AnimationTarget {
    AnimationTarget::Clip(ClipId::new(track(), number))
}

fn rack(number: u32, target: AnimationTarget, property: PropertyId, frames: &[u64]) -> Rack {
    let mut rack = Rack::new(RackId::new(track(), number), target, property);
    for frame in frames {
        let value = match property {
            PropertyId::Muted => PropertyValue::Flag(true),
            _ => PropertyValue::Number(0.0),
        };
        rack.set(*frame, value).unwrap();
    }
    rack
}

#[test]
fn a_drawer_is_empty_until_a_rack_is_put_in_it_and_has_its_racks_in_the_order_they_were_put_in() {
    let mut drawer = Drawer::default();
    assert!(drawer.is_empty());
    assert_eq!(drawer.len(), 0);
    drawer.push(rack(0, clip(0), PropertyId::Gain, &[1]));
    drawer.push(rack(1, clip(0), PropertyId::Muted, &[2]));
    drawer.push(rack(2, clip(5), PropertyId::Gain, &[3]));
    assert!(!drawer.is_empty());
    let ids: Vec<u32> = drawer.racks().iter().map(|rack| rack.id().number()).collect();
    assert_eq!(ids, vec![0, 1, 2], "the first is at the top");
}

#[test]
fn a_rack_is_found_by_its_name_and_by_what_it_animates() {
    let drawer = Drawer::with_racks([
        rack(0, clip(0), PropertyId::Gain, &[1]),
        rack(1, clip(0), PropertyId::Muted, &[2]),
        rack(2, clip(5), PropertyId::Gain, &[3]),
    ]);
    assert_eq!(
        drawer.rack(RackId::new(track(), 1)).unwrap().property(),
        PropertyId::Muted
    );
    assert!(drawer.rack(RackId::new(track(), 9)).is_none());
    assert_eq!(
        drawer.first_rack_for(clip(5), PropertyId::Gain).unwrap().id(),
        RackId::new(track(), 2),
        "the loudness of the other clip, which has a rack of its own"
    );
    assert!(drawer.first_rack_for(clip(5), PropertyId::Muted).is_none());
}

#[test]
fn a_drawer_can_have_several_racks_of_the_same_property_of_the_same_thing_and_they_stay_apart() {
    let mut drawer = Drawer::with_racks([
        rack(0, clip(0), PropertyId::Gain, &[1]),
        rack(1, clip(0), PropertyId::Muted, &[2]),
        rack(2, clip(0), PropertyId::Gain, &[3]),
    ]);
    drawer.push(rack(3, clip(0), PropertyId::Gain, &[4]));

    let numbers_of = |racks: Vec<&Rack>| -> Vec<u32> { racks.iter().map(|rack| rack.id().number()).collect() };
    assert_eq!(drawer.len(), 4);
    assert_eq!(
        numbers_of(drawer.racks_for(clip(0), PropertyId::Gain).collect()),
        vec![0, 2, 3],
        "all of them, the one at the top first"
    );
    assert_eq!(
        drawer.first_rack_for(clip(0), PropertyId::Gain).unwrap().id().number(),
        0,
        "the first of them is the one at the top"
    );
    drawer
        .first_rack_for_mut(clip(0), PropertyId::Gain)
        .unwrap()
        .set(9, PropertyValue::Number(1.0))
        .unwrap();
    assert_eq!(
        drawer.rack(RackId::new(track(), 0)).unwrap().len(),
        2,
        "that one was changed"
    );
    assert_eq!(
        drawer.rack(RackId::new(track(), 2)).unwrap().len(),
        1,
        "and not the others"
    );
    assert_eq!(drawer.racks_for(clip(0), PropertyId::Muted).count(), 1);
    assert_eq!(drawer.racks_for(clip(1), PropertyId::Gain).count(), 0);
}

#[test]
fn a_rack_is_put_in_a_drawer_so_that_it_has_as_many_racks_above_it_as_is_asked_and_not_more_than_there_are() {
    let drawer_with_three = || {
        Drawer::with_racks([
            rack(0, clip(0), PropertyId::Gain, &[1]),
            rack(1, clip(0), PropertyId::Muted, &[2]),
            rack(2, clip(5), PropertyId::Gain, &[3]),
        ])
    };
    let order_after = |slot: usize| -> Vec<u32> {
        let mut drawer = drawer_with_three();
        drawer.insert(slot, rack(9, clip(0), PropertyId::Gain, &[4]));
        drawer.racks().iter().map(|rack| rack.id().number()).collect()
    };
    assert_eq!(order_after(0), vec![9, 0, 1, 2], "at the top");
    assert_eq!(order_after(1), vec![0, 9, 1, 2]);
    assert_eq!(order_after(2), vec![0, 1, 9, 2]);
    assert_eq!(order_after(3), vec![0, 1, 2, 9], "under all of them");
    assert_eq!(order_after(40), vec![0, 1, 2, 9], "not more than there are");
}

#[test]
fn the_place_of_a_rack_in_a_drawer_is_counted_from_the_top_and_a_rack_that_is_not_there_has_none() {
    let drawer = Drawer::with_racks([
        rack(4, clip(0), PropertyId::Gain, &[1]),
        rack(2, clip(0), PropertyId::Muted, &[2]),
    ]);
    assert_eq!(drawer.position_of(RackId::new(track(), 4)), Some(0));
    assert_eq!(drawer.position_of(RackId::new(track(), 2)), Some(1));
    assert_eq!(drawer.position_of(RackId::new(track(), 0)), None);
}

#[test]
fn a_rack_taken_out_of_a_drawer_is_given_and_the_others_stay_in_order() {
    let mut drawer = Drawer::with_racks([
        rack(0, clip(0), PropertyId::Gain, &[1]),
        rack(1, clip(0), PropertyId::Muted, &[2]),
        rack(2, clip(5), PropertyId::Gain, &[3]),
    ]);
    let taken = drawer.take(RackId::new(track(), 1)).unwrap();
    assert_eq!(taken.property(), PropertyId::Muted);
    assert!(drawer.take(RackId::new(track(), 1)).is_none());
    let ids: Vec<u32> = drawer.racks().iter().map(|rack| rack.id().number()).collect();
    assert_eq!(ids, vec![0, 2]);
}

#[test]
fn racks_with_no_keyframes_are_taken_away_and_so_are_the_racks_of_a_thing_that_is_gone() {
    let mut drawer = Drawer::with_racks([
        rack(0, clip(0), PropertyId::Gain, &[1]),
        rack(1, clip(0), PropertyId::Muted, &[]),
        rack(2, clip(5), PropertyId::Gain, &[3]),
        rack(3, clip(5), PropertyId::Muted, &[4]),
    ]);
    drawer.remove_the_racks_with_no_keyframes();
    assert_eq!(drawer.len(), 3);
    assert_eq!(drawer.remove_the_racks_of(clip(5)), 2);
    assert_eq!(drawer.remove_the_racks_of(clip(5)), 0);
    assert_eq!(drawer.racks()[0].id(), RackId::new(track(), 0));
    assert_eq!(drawer.len(), 1);
}

#[test]
fn a_drawer_with_each_rack_changed_has_those_that_are_changed_into_nothing_left_out() {
    let drawer = Drawer::with_racks([
        rack(0, clip(0), PropertyId::Gain, &[1, 50]),
        rack(1, clip(1), PropertyId::Gain, &[60]),
    ]);
    let without_what_is_of_the_second_clip = drawer.with_each_rack_changed(|rack| {
        if rack.target() == clip(1) {
            rack.emptied()
        } else {
            rack.clone()
        }
    });
    assert_eq!(
        without_what_is_of_the_second_clip.len(),
        1,
        "the second was changed into nothing"
    );
    assert_eq!(
        without_what_is_of_the_second_clip.racks()[0].len(),
        2,
        "the first was not changed"
    );
    assert_eq!(drawer.len(), 2, "the drawer it was made from is as it was");
}
