use super::*;
use crate::project::{AnimationTarget, PropertyId, PropertyValue, RackDestination, TextObject, TimelineFrameIndex};
use crate::test_support::{DeterministicRandomNumbers, make_a_keyframe};

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn turn(degrees: f32) -> PropertyValue {
    PropertyValue::Number(degrees)
}

/// A text object from frame 0 for 100 frames, whose turn has three racks, in the order they are one under another on the
/// screen: `top`, in the drawer of the text, with keyframes at 50 and 60; and `middle`, with keyframes at 10 and 20, and
/// `bottom`, with keyframes at 30 and 40, which are in the drawer of the track (which has no end).
struct ThreeRacks {
    project: Project,
    text: AnimationTarget,
    top: RackId,
    middle: RackId,
    bottom: RackId,
}

fn three_racks() -> ThreeRacks {
    let mut project = Project::empty();
    let track = project.tracks()[0].id();
    let (clip, _) = project
        .insert_text(track, frame(0), TextObject::default(), 100)
        .unwrap();
    let text = AnimationTarget::Clip(clip);
    // (The keyframes are made in the drawer of the text, in a rack that is moved to the drawer of the track when it is to stay
    // there, so that the next keyframes are made in a rack of their own.)
    let the_rack_of_the_text = |project: &Project| {
        project
            .drawer_of(text)
            .unwrap()
            .first_rack_for(text, PropertyId::Rotation)
            .unwrap()
            .id()
    };
    let to_the_track = RackDestination::Drawer(AnimationTarget::Track(track));
    for at in [10, 20] {
        make_a_keyframe(&mut project, text, PropertyId::Rotation, at, turn(at as f32));
    }
    let middle = the_rack_of_the_text(&project);
    project.move_rack(middle, to_the_track, 0, None).unwrap();
    for at in [30, 40] {
        make_a_keyframe(&mut project, text, PropertyId::Rotation, at, turn(at as f32));
    }
    let bottom = the_rack_of_the_text(&project);
    project.move_rack(bottom, to_the_track, 0, None).unwrap();
    for at in [50, 60] {
        make_a_keyframe(&mut project, text, PropertyId::Rotation, at, turn(at as f32));
    }
    let top = the_rack_of_the_text(&project);
    ThreeRacks {
        project,
        text,
        top,
        middle,
        bottom,
    }
}

impl ThreeRacks {
    fn racks(&self) -> Vec<RackId> {
        vec![self.top, self.middle, self.bottom]
    }

    /// The `number`th keyframe (from 0) of `rack`.
    fn keyframe(&self, rack: RackId, number: usize) -> KeyframeId {
        self.project.located_rack(rack).unwrap().rack.keyframes()[number].id()
    }

    /// Puts a keyframe at frame `at` of `rack`, which has none there, and gives its name.
    fn put_a_keyframe_in(&mut self, rack: RackId, at: u64) -> KeyframeId {
        // (It is made in the top rack at a frame that is none of the frames the keyframes of the tests are at, and moved.)
        const STAGING_FRAME: u64 = 99;
        make_a_keyframe(
            &mut self.project,
            self.text,
            PropertyId::Rotation,
            STAGING_FRAME,
            turn(1.0),
        );
        let made = self.keyframe(self.top, self.project.located_rack(self.top).unwrap().rack.len() - 1);
        self.project
            .move_keyframes(&[KeyframeMove {
                keyframe: made,
                to_rack: rack,
                to_frame: at,
            }])
            .unwrap();
        made
    }
}

/// Every keyframe can go to each of these racks, whichever of them it is in.
fn every_one_of(racks: Vec<RackId>) -> impl Fn(RackId) -> Vec<RackId> {
    move |_| racks.clone()
}

/// Where the moves say the keyframes go: the rack and the frame of its drawer, for each in order.
fn as_places(moves: &[KeyframeMove]) -> Vec<(RackId, u64)> {
    moves.iter().map(|moved| (moved.to_rack, moved.to_frame)).collect()
}

fn wanting(racks_down: i64, frames: i64) -> MoveBy {
    MoveBy { racks_down, frames }
}

// --- down and up the racks ---

#[test]
fn a_keyframe_goes_down_as_many_racks_as_are_wanted_when_it_fits_in_them_and_not_past_the_last() {
    let three = three_racks();
    let moving = [three.keyframe(three.top, 0)]; // at 50
    let racks = every_one_of(three.racks());
    let allowed = |racks_down| allowed_move(&three.project, &moving, &racks, wanting(racks_down, 0));

    assert_eq!(allowed(0).by, wanting(0, 0));
    assert_eq!(
        as_places(&allowed(0).moves),
        vec![(three.top, 50)],
        "it stays where it is"
    );
    assert_eq!(as_places(&allowed(1).moves), vec![(three.middle, 50)]);
    assert_eq!(as_places(&allowed(2).moves), vec![(three.bottom, 50)]);
    assert_eq!(allowed(5).by, wanting(2, 0), "not past the last rack");
    assert_eq!(allowed(-1).by, wanting(0, 0), "there is none above the first");
}

#[test]
fn a_keyframe_goes_up_as_many_racks_as_are_wanted() {
    let three = three_racks();
    let moving = [three.keyframe(three.bottom, 1)]; // at 40
    let racks = every_one_of(three.racks());
    let allowed = |racks_down| allowed_move(&three.project, &moving, &racks, wanting(racks_down, 0));

    assert_eq!(as_places(&allowed(-1).moves), vec![(three.middle, 40)]);
    assert_eq!(as_places(&allowed(-2).moves), vec![(three.top, 40)]);
    assert_eq!(allowed(-9).by, wanting(-2, 0));
    assert_eq!(allowed(1).by, wanting(0, 0), "there is none below the last");
}

#[test]
fn a_keyframe_does_not_go_to_a_rack_that_has_a_keyframe_at_the_frame_it_is_at_but_may_go_on_to_one_that_has_not() {
    let mut three = three_racks();
    three.put_a_keyframe_in(three.middle, 50);
    let moving = [three.keyframe(three.top, 0)]; // at 50
    let racks = every_one_of(three.racks());
    let allowed = |racks_down| allowed_move(&three.project, &moving, &racks, wanting(racks_down, 0));

    assert_eq!(allowed(1).by.racks_down, 0, "50 is taken in the middle rack: it stays");
    assert_eq!(
        as_places(&allowed(2).moves),
        vec![(three.bottom, 50)],
        "but it can go past that rack to one that has room"
    );
}

#[test]
fn keyframes_that_are_dragged_together_all_go_to_other_racks_or_none_does() {
    let mut three = three_racks();
    three.put_a_keyframe_in(three.middle, 60);
    let moving = [three.keyframe(three.top, 0), three.keyframe(three.top, 1)]; // at 50 and 60
    let racks = every_one_of(three.racks());
    let allowed = |racks_down| allowed_move(&three.project, &moving, &racks, wanting(racks_down, 0));

    assert_eq!(
        allowed(1).by.racks_down,
        0,
        "the one at 60 would be on a keyframe, and the one at 50 does not go alone"
    );
    assert_eq!(
        as_places(&allowed(2).moves),
        vec![(three.bottom, 50), (three.bottom, 60)],
        "both fit in the bottom rack"
    );
}

#[test]
fn keyframes_from_different_racks_go_the_same_number_of_racks_each_in_its_own_list_of_racks() {
    let three = three_racks();
    let moving = [three.keyframe(three.middle, 1), three.keyframe(three.bottom, 0)]; // at 20 and 30
    let racks = every_one_of(three.racks());

    let up = allowed_move(&three.project, &moving, &racks, wanting(-1, 0));
    assert_eq!(up.by.racks_down, -1);
    assert_eq!(as_places(&up.moves), vec![(three.top, 20), (three.middle, 30)]);
    let down = allowed_move(&three.project, &moving, &racks, wanting(1, 0));
    assert_eq!(
        down.by.racks_down, 0,
        "the one in the bottom rack has none below it, so none of them goes down"
    );
}

#[test]
fn a_keyframe_that_has_nowhere_else_to_go_keeps_the_others_where_they_are_in_the_racks() {
    let three = three_racks();
    let top = three.top;
    let racks = three.racks();
    // The keyframes in the middle rack can only be in it.
    let lists = move |rack: RackId| if rack == top { racks.clone() } else { vec![rack] };
    let moving = [three.keyframe(three.top, 0), three.keyframe(three.middle, 0)];

    let allowed = allowed_move(&three.project, &moving, &lists, wanting(1, 4));

    assert_eq!(
        allowed.by,
        wanting(0, 4),
        "none goes down, and they go along the timeline together"
    );
}

#[test]
fn a_keyframe_in_a_rack_that_is_not_among_the_racks_it_can_go_to_stays_in_it() {
    let three = three_racks();
    let moving = [three.keyframe(three.top, 0)];
    let nowhere_listed = |_: RackId| Vec::new();

    let allowed = allowed_move(&three.project, &moving, &nowhere_listed, wanting(2, 3));

    assert_eq!(allowed.by, wanting(0, 3));
    assert_eq!(as_places(&allowed.moves), vec![(three.top, 53)]);
}

// --- along the timeline ---

#[test]
fn the_frames_are_as_many_as_the_keyframes_can_go_in_the_rack_they_go_to_up_to_the_keyframes_that_are_in_the_way() {
    let three = three_racks();
    let moving = [three.keyframe(three.top, 0)]; // at 50
    let racks = every_one_of(three.racks());
    let allowed = |racks_down, frames| allowed_move(&three.project, &moving, &racks, wanting(racks_down, frames));

    assert_eq!(
        allowed(0, 100).by.frames,
        9,
        "staying in its rack, it stops before the keyframe at 60"
    );
    assert_eq!(allowed(0, -100).by.frames, -50, "or at the start of the drawer");
    assert_eq!(
        allowed(2, 100).by,
        wanting(2, 100),
        "the bottom rack is in the drawer of the track, which has no end, and its keyframes are before"
    );
    assert_eq!(
        allowed(2, -100).by,
        wanting(2, -9),
        "going back it stops after the keyframe at 40"
    );
}

#[test]
fn a_keyframe_in_the_way_at_the_frame_the_keyframe_is_at_stops_it_going_neither_way() {
    let mut three = three_racks();
    // The middle rack has a keyframe at 50, where the one that is dragged is. It cannot be put there, but if it goes along at the
    // same time it is not stopped by that one.
    three.put_a_keyframe_in(three.middle, 50);
    let to_the_middle = [KeyframeDestination {
        keyframe: three.keyframe(three.top, 0),
        rack: three.middle,
    }];

    assert_eq!(three.project.allowed_shift_of_keyframes(&to_the_middle, -7), -7);
    assert_eq!(three.project.allowed_shift_of_keyframes(&to_the_middle, 7), 7);
    assert_eq!(
        three.project.allowed_shift_of_keyframes(&to_the_middle, -100),
        -29,
        "it stops after 20"
    );
}

// --- held to the frame it started at ---

#[test]
fn a_keyframe_that_goes_to_another_rack_and_is_near_the_frame_it_started_at_is_put_at_that_frame() {
    let three = three_racks();
    let grabbed = three.keyframe(three.top, 0); // it was at 50 when it was taken hold of, and is at 50 still
    let hold = Hold {
        keyframe: grabbed,
        time: HeldTime {
            start: 50,
            within_frames: 3,
            free_on: Some(three.top),
        },
    };
    let racks = every_one_of(three.racks());
    let allowed = |racks_down, frames| {
        allowed_move_holding(
            &three.project,
            &[grabbed],
            &racks,
            wanting(racks_down, frames),
            Some(hold),
        )
    };

    assert_eq!(
        allowed(0, 2).by,
        wanting(0, 2),
        "staying in the rack it started in it is free"
    );
    assert_eq!(
        allowed(1, 3).by,
        wanting(1, 0),
        "3 frames from the frame it started at is near it"
    );
    assert_eq!(allowed(1, 2).by, wanting(1, 0));
    assert_eq!(allowed(1, 4).by, wanting(1, 4), "4 frames from it is not near");
}

#[test]
fn a_keyframe_that_is_held_goes_to_a_rack_that_it_does_not_fit_in_where_it_is_if_it_fits_at_the_frame_it_is_held_to() {
    let mut three = three_racks();
    // The keyframe that was at 50 has been dragged along its rack to 62 (past the one at 60), and the middle rack has a keyframe
    // at 62. Where it is, it does not fit in the middle rack; at 55, which it was held to, it does.
    let grabbed = three.keyframe(three.top, 0);
    three
        .project
        .move_keyframes(&[KeyframeMove {
            keyframe: grabbed,
            to_rack: three.top,
            to_frame: 62,
        }])
        .unwrap();
    three.put_a_keyframe_in(three.middle, 62);
    let hold = Hold {
        keyframe: grabbed,
        time: HeldTime {
            start: 55,
            within_frames: 4,
            free_on: None,
        },
    };
    let racks = every_one_of(three.racks());
    let allowed = |hold| allowed_move_holding(&three.project, &[grabbed], &racks, wanting(1, -6), hold);

    assert_eq!(allowed(None).by.racks_down, 0, "62 is taken in the middle rack");
    let held = allowed(Some(hold));
    assert_eq!(held.by, wanting(1, -7), "the pointer wants 56, which is near 55");
    assert_eq!(as_places(&held.moves), vec![(three.middle, 55)]);
}

#[test]
fn a_keyframe_that_is_held_stays_in_its_rack_when_it_does_not_fit_in_the_other_at_the_frame_it_is_held_to_either() {
    let mut three = three_racks();
    // As in the test above, the keyframe has been dragged along its rack to 62, where it does not fit in the middle rack; but now
    // the frame it is held to, 55, is taken in the middle rack too.
    let grabbed = three.keyframe(three.top, 0);
    three
        .project
        .move_keyframes(&[KeyframeMove {
            keyframe: grabbed,
            to_rack: three.top,
            to_frame: 62,
        }])
        .unwrap();
    three.put_a_keyframe_in(three.middle, 62);
    three.put_a_keyframe_in(three.middle, 55);
    let hold = Hold {
        keyframe: grabbed,
        time: HeldTime {
            start: 55,
            within_frames: 4,
            free_on: None,
        },
    };
    let racks = every_one_of(three.racks());

    let held = allowed_move_holding(&three.project, &[grabbed], &racks, wanting(1, -6), Some(hold));

    assert_eq!(
        held.by.racks_down, 0,
        "it fits in the middle rack neither where it is nor at 55"
    );
    assert!(
        as_places(&held.moves).iter().all(|(rack, _)| *rack == three.top),
        "it stays in the top rack: {:?}",
        held.moves
    );
}

// --- nothing, and whatever is wanted ---

#[test]
fn keyframes_that_are_not_there_are_not_moved_and_nothing_moving_is_stopped_by_anything() {
    let three = three_racks();
    let racks = every_one_of(three.racks());
    let nobody = KeyframeId::new(three.top, 77);

    let allowed = allowed_move(&three.project, &[nobody], &racks, wanting(2, 7));
    assert_eq!(allowed.by, wanting(0, 7));
    assert_eq!(allowed.moves, vec![]);
    let nothing = allowed_move(&three.project, &[], &racks, wanting(1, -3));
    assert_eq!(nothing.by, wanting(0, -3));
}

#[test]
fn moves_by_puts_each_keyframe_the_racks_and_frames_given_from_where_it_is() {
    let three = three_racks();
    let racks = every_one_of(three.racks());
    let moving = [three.keyframe(three.top, 0), three.keyframe(three.top, 1)]; // at 50 and 60

    let moves = moves_by(&three.project, &moving, &racks, wanting(2, -7));

    assert_eq!(as_places(&moves), vec![(three.bottom, 43), (three.bottom, 53)]);
    assert_eq!(
        moves_by(&three.project, &moving, &racks, wanting(3, 0)),
        vec![],
        "a rack that is not there is not gone to"
    );
}

#[test]
fn whatever_is_wanted_the_moves_that_are_given_can_be_made_are_not_more_than_was_wanted_and_are_all_that_can_be_done() {
    let mut cases = 0;
    for seed in 1..=300u64 {
        let mut random = DeterministicRandomNumbers::with_seed(seed);
        let mut three = three_racks();
        // Some more keyframes, in the racks, at frames they have none at (below the frame that the keyframes are made at).
        for rack in three.racks() {
            for _ in 0..random.next_below(3) {
                let at = random.next_below(90);
                if three.project.located_rack(rack).unwrap().rack.at(at).is_none() {
                    three.put_a_keyframe_in(rack, at);
                }
            }
        }
        let all: Vec<KeyframeId> = three
            .racks()
            .iter()
            .flat_map(|rack| {
                three
                    .project
                    .located_rack(*rack)
                    .unwrap()
                    .rack
                    .keyframes()
                    .iter()
                    .map(|keyframe| keyframe.id())
                    .collect::<Vec<_>>()
            })
            .collect();
        let moving: Vec<KeyframeId> = all.iter().copied().filter(|_| random.next_below(3) == 0).collect();
        if moving.is_empty() {
            continue;
        }
        cases += 1;
        let wanted = wanting(random.next_below(7) as i64 - 3, random.next_below(121) as i64 - 60);
        let racks = every_one_of(three.racks());

        let allowed = allowed_move(&three.project, &moving, &racks, wanted);

        let context = format!(
            "seed {seed}: {} moving, {wanted:?} wanted, {:?} allowed",
            moving.len(),
            allowed.by
        );
        assert!(allowed.by.racks_down.abs() <= wanted.racks_down.abs(), "{context}");
        assert!(allowed.by.racks_down * wanted.racks_down >= 0, "{context}");
        assert!(allowed.by.frames.abs() <= wanted.frames.abs(), "{context}");
        assert!(allowed.by.frames * wanted.frames >= 0, "{context}");
        assert_eq!(allowed.moves.len(), moving.len(), "{context}");
        assert_eq!(
            three.project.check_the_move_of_keyframes(&allowed.moves),
            Ok(()),
            "the moves can be made: {context}"
        );
        assert_eq!(
            allowed.moves,
            moves_by(&three.project, &moving, &racks, allowed.by),
            "the moves are those of moving by as much: {context}"
        );
        if allowed.by.frames != wanted.frames {
            let one_more = MoveBy {
                frames: allowed.by.frames + wanted.frames.signum(),
                ..allowed.by
            };
            let moves = moves_by(&three.project, &moving, &racks, one_more);
            assert!(
                moves.len() < moving.len() || three.project.check_the_move_of_keyframes(&moves).is_err(),
                "one frame more could have been done: {context}"
            );
        }
    }
    assert!(cases > 100, "only {cases} cases were tried");
}
