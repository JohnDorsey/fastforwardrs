// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn track(number: u32) -> TrackId {
    TrackId::new(number)
}

fn from(number: u32, channel: MaskChannel, nothing: f32) -> DrivenValue {
    DrivenValue::from_a_track(track(number), channel, nothing)
}

#[test]
fn a_value_that_is_read_from_a_track_says_which_and_how_and_what_it_is_where_the_track_has_nothing() {
    let driven = from(3, MaskChannel::Brightness, 0.25);

    assert_eq!(driven.source, MaskSource::Track(track(3)));
    assert_eq!(driven.channel, MaskChannel::Brightness);
    assert_eq!(driven.value_where_nothing, 0.25);
    assert!(driven.is_well_formed());
    assert_eq!(driven.source, MaskSource::Track(track(3)));
}

#[test]
fn a_number_that_is_not_one_is_not_well_formed() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(!from(1, MaskChannel::Red, bad).is_well_formed(), "{bad}");
    }
    assert!(
        from(1, MaskChannel::Red, -3.5).is_well_formed(),
        "a negative number is a number"
    );
}

#[test]
fn the_driven_values_are_in_the_order_of_their_places_whatever_order_they_were_given_in() {
    let values = DrivenValues::from_places(vec![
        (5, from(2, MaskChannel::Blue, 1.0)),
        (1, from(7, MaskChannel::Red, 0.0)),
        (3, from(7, MaskChannel::Green, 0.5)),
    ])
    .expect("three different places");

    let places: Vec<usize> = values.iter().map(|(place, _)| place).collect();
    assert_eq!(places, [1, 3, 5]);
    assert_eq!(values.of(3).map(|driven| driven.channel), Some(MaskChannel::Green));
    assert_eq!(values.of(4), None, "a place that is not driven");
    assert!(!values.is_empty());
    assert!(DrivenValues::none().is_empty());
}

#[test]
fn a_place_cannot_be_driven_twice_and_a_number_that_is_not_one_cannot_be_kept() {
    assert_eq!(
        DrivenValues::from_places(vec![
            (2, from(1, MaskChannel::Red, 0.0)),
            (2, from(2, MaskChannel::Red, 0.0))
        ]),
        None
    );
    assert_eq!(
        DrivenValues::from_places(vec![(2, from(1, MaskChannel::Red, f32::NAN))]),
        None
    );
    assert_eq!(DrivenValues::from_places(Vec::new()), Some(DrivenValues::none()));
}

#[test]
fn a_value_is_driven_replaced_and_let_go_one_place_at_a_time() {
    let none = DrivenValues::none();
    let one = none.with(4, Some(from(1, MaskChannel::Red, 0.0))).unwrap();
    let two = one.with(2, Some(from(2, MaskChannel::Green, 0.5))).unwrap();
    assert_eq!(
        two.iter().map(|(place, _)| place).collect::<Vec<_>>(),
        [2, 4],
        "kept in the order of the places"
    );

    let replaced = two.with(4, Some(from(9, MaskChannel::Brightness, 1.0))).unwrap();
    assert_eq!(replaced.iter().count(), 2, "a place is driven once");
    assert_eq!(replaced.of(4), Some(&from(9, MaskChannel::Brightness, 1.0)));
    assert_eq!(replaced.of(2), two.of(2), "the other is as it was");

    let let_go = replaced.with(2, None).unwrap();
    assert_eq!(let_go.iter().map(|(place, _)| place).collect::<Vec<_>>(), [4]);
    assert_eq!(
        let_go.with(2, None),
        Some(let_go.clone()),
        "letting go of what is not driven changes nothing"
    );
    assert_eq!(let_go.with(4, None), Some(DrivenValues::none()));
    // (And the one that was changed from is not changed.)
    assert_eq!(two.iter().count(), 2);
}

#[test]
fn a_number_that_is_not_one_is_refused_and_nothing_is_changed() {
    let one = DrivenValues::none()
        .with(1, Some(from(1, MaskChannel::Red, 0.0)))
        .unwrap();

    assert_eq!(one.with(1, Some(from(1, MaskChannel::Red, f32::INFINITY))), None);
    assert_eq!(one.with(3, Some(from(1, MaskChannel::Red, f32::NAN))), None);
    assert_eq!(one.iter().count(), 1);
}

#[test]
fn the_tracks_the_values_are_read_from_are_each_named_once_in_the_order_they_are_first_met_in() {
    let values = DrivenValues::from_places(vec![
        (0, from(5, MaskChannel::Red, 0.0)),
        (1, from(2, MaskChannel::Red, 0.0)),
        (2, from(5, MaskChannel::Green, 0.0)),
        (3, from(9, MaskChannel::Blue, 0.0)),
    ])
    .unwrap();

    assert_eq!(
        values.sources(),
        [
            MaskSource::Track(track(5)),
            MaskSource::Track(track(2)),
            MaskSource::Track(track(9))
        ]
    );
    assert!(DrivenValues::none().sources().is_empty());
}

#[test]
fn a_track_that_is_gone_takes_the_values_that_were_read_from_it_with_it_and_no_others() {
    let values = DrivenValues::from_places(vec![
        (0, from(5, MaskChannel::Red, 0.0)),
        (1, from(2, MaskChannel::Red, 0.1)),
        (2, from(5, MaskChannel::Green, 0.2)),
    ])
    .unwrap();

    let without = values.without_the_track(track(5));

    assert_eq!(without.iter().map(|(place, _)| place).collect::<Vec<_>>(), [1]);
    assert_eq!(without.of(1), values.of(1));
    assert_eq!(
        values.without_the_track(track(77)),
        values,
        "a track that nothing is read from changes nothing"
    );
}

#[test]
fn a_value_sees_the_real_backdrop_until_it_is_said_otherwise_and_two_settings_of_one_track_are_two_masks() {
    let seeing = from(3, MaskChannel::Red, 0.0);
    let not_seeing = from(3, MaskChannel::Green, 0.0).with_the_real_backdrop(false);

    assert!(seeing.real_backdrop, "on when a value is first read from a mask");
    assert_eq!(seeing.mask_use(), MaskUse::of_a_track(track(3)));
    assert_ne!(seeing.mask_use(), not_seeing.mask_use());
    let values =
        DrivenValues::from_places(vec![(0, seeing), (1, not_seeing), (2, from(3, MaskChannel::Blue, 0.0))]).unwrap();
    assert_eq!(
        values.uses(),
        [seeing.mask_use(), not_seeing.mask_use()],
        "the third reads the first's mask: the same track, the same setting"
    );
    assert_eq!(values.sources(), [MaskSource::Track(track(3))], "and it is one source");
}
