//! Tests of the racks of keyframes in the drawers: that they are there, where they are and what they look like, the diamonds
//! of the keyframes in them, selecting the keyframes, and the label that comes to the left of a rack.

use egui::{Pos2, Rect, Shape, vec2};
use egui_kittest::kittest::Queryable;

use super::test_window::{Window, project_of};
use crate::application::commands::Command;
use crate::application::selection::SelectionItem;
use crate::application::theme;
use crate::application::timeline_panel::{GripDotArrangement, keyframe_label};
use crate::project::{AnimationTarget, ClipId, KeyframeId, PropertyId, RackDestination, TimelineFrameIndex};

fn frame(number: u64) -> TimelineFrameIndex {
    TimelineFrameIndex::new(number)
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!((actual - expected).abs() < 1.0, "{what}: {actual} is not {expected}");
}

/// A window with a video of 60 frames (which is all in view, and is the first clip of the track), with nothing unsaved.
fn window_with_a_short_video() -> (Window, ClipId) {
    let mut window = Window::showing(project_of(60));
    window.application_mut().note_everything_is_saved();
    let clip = window.clip(0, 0);
    (window, clip)
}

/// Makes a keyframe of `property` of `target` at `at`, and lets the drawer that it opens finish opening.
fn make_a_keyframe(window: &mut Window, target: AnimationTarget, property: PropertyId, at: u64) {
    window.do_this(Command::AddKeyframe {
        target,
        property,
        frame: frame(at),
    });
    window.harness.run_steps(40);
}

/// The racks that were drawn in the last frame, as far as they can be seen, the top one first.
pub(super) fn racks_drawn(window: &Window) -> Vec<Rect> {
    let mut racks: Vec<Rect> = window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.fill == theme::RACK_BACKGROUND => Some(rectangle.rect),
            _ => None,
        })
        .collect();
    racks.sort_by(|first, second| first.top().total_cmp(&second.top()));
    racks
}

/// The keyframes that were drawn in the last frame (the middles of their diamonds, which are orange), left to right and
/// top to bottom.
pub(super) fn keyframes_drawn(window: &Window) -> Vec<Pos2> {
    let mut keyframes: Vec<Pos2> = window
        .shapes_drawn()
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Path(path) if path.fill == theme::KEYFRAME_BUTTON_ACTIVE => Some(middle_of(&path.points)),
            _ => None,
        })
        .collect();
    keyframes.sort_by(|first, second| first.y.total_cmp(&second.y).then(first.x.total_cmp(&second.x)));
    keyframes
}

fn middle_of(points: &[Pos2]) -> Pos2 {
    let sum = points.iter().fold(vec2(0.0, 0.0), |sum, point| sum + point.to_vec2());
    (sum / points.len() as f32).to_pos2()
}

/// How many keyframes have the outline of a selected thing round them.
fn selected_keyframes_drawn(window: &Window) -> usize {
    window
        .shapes_drawn()
        .into_iter()
        .filter(|shape| matches!(shape, Shape::Path(path) if path.fill == theme::SELECTION_OUTLINE))
        .count()
}

fn selected_keyframes(window: &Window) -> Vec<KeyframeId> {
    window
        .application()
        .selected_items()
        .into_iter()
        .filter_map(|item| match item {
            SelectionItem::Keyframe(keyframe) => Some(keyframe),
            _ => None,
        })
        .collect()
}

/// The text of the label that was drawn in the last frame, and where it is, if there is one.
fn label_drawn(window: &Window) -> Option<(String, Rect)> {
    let background = window.shapes_drawn().into_iter().find_map(|shape| match shape {
        Shape::Rect(rectangle) if rectangle.fill == theme::RACK_LABEL_BACKGROUND => Some(rectangle.rect),
        _ => None,
    })?;
    let text = window.shapes_drawn().into_iter().find_map(|shape| match shape {
        Shape::Text(text) if background.contains(text.pos) => Some(text.galley.text().to_owned()),
        _ => None,
    })?;
    Some((text, background))
}

fn click_the_keyframe_at(window: &mut Window, rack_index: usize, frame_number: u64) {
    let position = keyframe_position(window, rack_index, frame_number);
    window.click(position);
}

fn keyframe_position(window: &Window, rack_index: usize, frame_number: u64) -> Pos2 {
    Pos2::new(
        window.x_of_frame_on_the_strip(0, frame_number),
        racks_drawn(window)[rack_index].center().y,
    )
}

// --- where the racks and the keyframes are ---

#[test]
fn a_keyframe_made_at_the_playhead_is_a_diamond_directly_under_it_in_a_rack_in_the_drawer_that_opens() {
    let (mut window, clip) = window_with_a_short_video();
    window.move_current_frame_to(20);
    assert!(racks_drawn(&window).is_empty(), "nothing in the drawer, which is shut");

    make_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 20);

    let racks = racks_drawn(&window);
    assert_eq!(racks.len(), 1, "a rack was made for it");
    let keyframes = keyframes_drawn(&window);
    assert_eq!(keyframes.len(), 1);
    let playhead = window.x_of_the_drawn_playhead().expect("the playhead is in view");
    assert_close(keyframes[0].x, playhead, "the keyframe is under the playhead");
    assert_close(
        keyframes[0].y,
        racks[0].center().y,
        "and in the middle of the rack up and down",
    );
}

#[test]
fn a_rack_goes_as_far_along_the_timeline_as_the_clip_whose_drawer_it_is_in() {
    let (mut window, clip) = window_with_a_short_video();
    make_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 20);

    let rack = racks_drawn(&window)[0];

    assert_close(
        rack.left(),
        window.x_of_frame_on_the_strip(0, 0),
        "from the start of the clip",
    );
    assert_close(rack.right(), window.x_of_frame_on_the_strip(0, 60), "to the end of it");
}

#[test]
fn a_rack_is_barely_taller_than_a_keyframe_with_corners_that_are_round_only_a_little() {
    let (mut window, clip) = window_with_a_short_video();
    make_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 20);

    let rack = window
        .shapes_drawn()
        .into_iter()
        .find_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.fill == theme::RACK_BACKGROUND => Some(rectangle.clone()),
            _ => None,
        })
        .unwrap();

    assert_close(rack.rect.height(), theme::RACK_HEIGHT, "the height of a rack");
    assert!(
        rack.rect.height() < 20.0 && rack.rect.height() > 12.0,
        "only a little taller than a keyframe"
    );
    assert!(
        rack.corner_radius.nw > 0 && rack.corner_radius.nw < 6,
        "{:?}",
        rack.corner_radius
    );
    assert_eq!(rack.corner_radius.nw, rack.corner_radius.se);
    assert!(
        rack.rect.height() - 2.0 * f32::from(rack.corner_radius.nw) > 4.0,
        "not a capsule: the ends have a straight edge"
    );
}

#[test]
fn racks_are_one_under_another_in_the_order_they_were_made_with_room_between_them() {
    let (mut window, clip) = window_with_a_short_video();
    let target = AnimationTarget::Clip(clip);
    make_a_keyframe(&mut window, target, PropertyId::Gain, 10);
    make_a_keyframe(&mut window, target, PropertyId::Position, 10);
    make_a_keyframe(&mut window, target, PropertyId::Rotation, 10);

    let racks = racks_drawn(&window);

    assert_eq!(racks.len(), 3);
    let strip = window.strip_of_track(0);
    assert_close(
        racks[0].top(),
        strip.bottom() + theme::SPACE_ABOVE_THE_RACKS,
        "the first is just under the clip with a little room above it",
    );
    for pair in racks.windows(2) {
        assert_close(
            pair[1].top() - pair[0].bottom(),
            theme::SPACE_BETWEEN_RACKS,
            "the room between two racks",
        );
        assert_close(pair[0].left(), pair[1].left(), "they are as long as each other");
        assert_close(pair[0].right(), pair[1].right(), "and as long as the drawer");
    }
}

#[test]
fn every_keyframe_of_a_rack_is_a_diamond_at_the_frame_it_is_at() {
    let (mut window, clip) = window_with_a_short_video();
    for at in [10, 30, 50] {
        make_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, at);
    }

    let keyframes = keyframes_drawn(&window);

    assert_eq!(racks_drawn(&window).len(), 1, "they are all in one rack");
    assert_eq!(keyframes.len(), 3);
    for (keyframe, at) in keyframes.iter().zip([10, 30, 50]) {
        assert_close(
            keyframe.x,
            window.x_of_frame_on_the_strip(0, at),
            &format!("the keyframe at {at}"),
        );
    }
}

#[test]
fn a_keyframe_is_about_as_tall_as_a_stopwatch_and_bright_orange_and_a_square_turned_over() {
    let (mut window, clip) = window_with_a_short_video();
    make_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 10);

    let diamond = window
        .shapes_drawn()
        .into_iter()
        .find_map(|shape| match shape {
            Shape::Path(path) if path.fill == theme::KEYFRAME_BUTTON_ACTIVE => Some(path.clone()),
            _ => None,
        })
        .expect("a keyframe is drawn");

    assert_eq!(diamond.points.len(), 4);
    let (top, bottom) = (diamond.points[0], diamond.points[2]);
    let (right, left) = (diamond.points[1], diamond.points[3]);
    assert_close(bottom.y - top.y, 12.0, "as tall as the stopwatch");
    assert_close(right.x - left.x, 12.0, "and as wide");
    assert_close(top.x, bottom.x, "the corners are over one another");
    assert_close(left.y, right.y, "and the others side by side");
    assert!(
        theme::KEYFRAME_BUTTON_ACTIVE.r() > 240
            && theme::KEYFRAME_BUTTON_ACTIVE.g() > 100
            && theme::KEYFRAME_BUTTON_ACTIVE.b() < 60,
        "bright orange"
    );
}

#[test]
fn a_rack_is_shaded_as_if_it_were_sunk_in_dark_along_its_top_edge_and_lit_along_its_bottom_edge() {
    let (mut window, clip) = window_with_a_short_video();
    make_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 30);
    let rack = racks_drawn(&window)[0];

    // The shade is a strip of four points along the top of the rack, black at the top of it and clear a few points down.
    let shade = window
        .shapes_drawn()
        .into_iter()
        .find_map(|shape| match shape {
            Shape::Mesh(mesh)
                if mesh.vertices.len() == 4
                    && mesh.texture_id == egui::TextureId::default()
                    && mesh.vertices.iter().all(|vertex| rack.expand(1.0).contains(vertex.pos)) =>
            {
                Some(mesh)
            }
            _ => None,
        })
        .expect("the shade of the top edge is drawn on the rack");
    let (upper, lower): (Vec<&egui::epaint::Vertex>, Vec<&egui::epaint::Vertex>) = shade
        .vertices
        .iter()
        .partition(|vertex| vertex.pos.y < rack.top() + 1.0);
    assert_eq!(upper.len(), 2, "two points at the top edge");
    assert!(
        upper
            .iter()
            .all(|vertex| vertex.color.r() == 0 && vertex.color.a() > 100),
        "dark there: {upper:?}"
    );
    assert!(lower.iter().all(|vertex| vertex.color.a() == 0), "and clear below it");
    assert!(
        lower[0].pos.y - rack.top() <= 8.0,
        "a shade along the edge, and not over the whole rack"
    );
    // The light on the bottom edge, which faces it.
    let light = window.shapes_drawn().into_iter().find_map(|shape| match shape {
        Shape::LineSegment { points, stroke }
            if stroke.color == egui::Color32::from_white_alpha(34)
                && (points[0].y - (rack.bottom() - 0.5)).abs() < 0.1 =>
        {
            Some(points)
        }
        _ => None,
    });
    assert!(light.is_some(), "a light line along the bottom edge");
}

#[test]
fn a_keyframe_that_the_pointer_is_over_is_brighter_than_the_others() {
    let (mut window, clip) = window_with_a_short_video();
    make_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 20);
    make_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 40);
    window.move_pointer_to(Pos2::new(5.0, 5.0));
    assert_eq!(keyframes_drawn(&window).len(), 2, "both are the orange of keyframes");

    let over = keyframe_position(&window, 0, 40);
    window.move_pointer_to(over);

    assert_eq!(keyframes_drawn(&window).len(), 1, "the other one is");
    let hovered_fill = window
        .shapes_drawn()
        .into_iter()
        .find_map(|shape| match shape {
            Shape::Path(path) if path.points.len() == 4 && middle_of(&path.points).distance(over) < 1.0 => {
                Some(path.fill)
            }
            _ => None,
        })
        .expect("the diamond is drawn");
    let orange = theme::KEYFRAME_BUTTON_ACTIVE;
    assert_ne!(hovered_fill, orange);
    assert!(
        hovered_fill.r() >= orange.r() && hovered_fill.g() > orange.g() && hovered_fill.b() > orange.b(),
        "lighter: {hovered_fill:?} against {orange:?}"
    );
}

#[test]
fn a_rack_has_six_grip_dots_in_three_rows_of_two_at_its_left_end() {
    let (mut window, clip) = window_with_a_short_video();
    make_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 30);
    let rack = racks_drawn(&window)[0];
    let size = GripDotArrangement::FOR_A_RACK.size();

    let dots: Vec<Rect> = window
        .grip_dots_drawn(GripDotArrangement::FOR_A_RACK)
        .into_iter()
        .filter(|dots| dots.width() < size.x + 3.0 && dots.height() < size.y + 3.0)
        .collect();

    assert_eq!(dots.len(), 1, "one set of them, on the rack");
    assert!(
        dots[0].left() - rack.left() < 12.0,
        "at its left end: {:?} on {rack:?}",
        dots[0]
    );
    assert!(
        dots[0].top() > rack.top() && dots[0].bottom() < rack.bottom(),
        "inside it up and down"
    );
    assert!(
        dots[0].height() > dots[0].width(),
        "three rows of two: taller than wide"
    );
}

/// The sets of grip dots of racks that were drawn in the last frame. (The boxes of the tracks have as many dots, in two rows of
/// three, which are wider than they are tall: those are not told from these by how many there are.)
fn grip_dots_of_racks_drawn(window: &Window) -> Vec<Rect> {
    window
        .grip_dots_drawn(GripDotArrangement::FOR_A_RACK)
        .into_iter()
        .filter(|dots| dots.height() > dots.width())
        .collect()
}

#[test]
fn a_rack_that_is_too_narrow_for_its_grip_dots_has_none() {
    let mut project = project_of(60);
    let in_front = project.add_track_in_front();
    // A text object three frames long, which is about 18 points wide: not room for the dots and the room round them.
    let (text, _) = project
        .insert_text(in_front, frame(10), crate::project::TextObject::default(), 3)
        .unwrap();
    let mut window = Window::showing(project);
    let video = window.clip(1, 0);
    make_a_keyframe(&mut window, AnimationTarget::Clip(text), PropertyId::Rotation, 11);
    let narrow = racks_drawn(&window)[0];
    assert!(narrow.width() < 28.0, "narrow: {narrow:?}");
    assert!(
        grip_dots_of_racks_drawn(&window).is_empty(),
        "no dots on a rack that has no room for them"
    );

    make_a_keyframe(&mut window, AnimationTarget::Clip(video), PropertyId::Gain, 30);
    assert_eq!(racks_drawn(&window).len(), 2);
    assert_eq!(grip_dots_of_racks_drawn(&window).len(), 1, "the wide rack has its dots");
}

#[test]
fn the_drawer_of_a_track_has_racks_that_go_from_the_start_of_the_timeline_across_the_whole_strip() {
    let (mut window, _) = window_with_a_short_video();
    let track = window.application().project().tracks()[0].id();
    make_a_keyframe(&mut window, AnimationTarget::Track(track), PropertyId::Gain, 30);

    let rack = racks_drawn(&window)[0];

    let strip = window.strip_of_track(0);
    assert_close(
        rack.left(),
        window.x_of_frame_on_the_strip(0, 0),
        "from the start of the timeline",
    );
    assert_close(rack.right(), strip.right(), "as far as can be seen");
    assert!(rack.top() >= strip.bottom() + theme::SPACE_ABOVE_THE_RACKS - 0.6);
}

// --- opening ---

#[test]
fn the_racks_of_a_drawer_that_is_shut_are_not_seen_and_those_of_a_drawer_that_slides_are_seen_as_far_as_it_has_opened()
{
    let (mut window, clip) = window_with_a_short_video();
    // (Not through `do_this`, which lets the window settle, and by then the drawer has opened.)
    window.application_mut().execute_all(
        vec![Command::AddKeyframe {
            target: AnimationTarget::Clip(clip),
            property: PropertyId::Gain,
            frame: frame(20),
        }],
        std::time::Instant::now(),
        &egui::Context::default(),
    );

    let mut how_much_of_the_rack_is_seen = Vec::new();
    for _ in 0..30 {
        window.harness.step();
        let drawer_bottom = window.shapes_drawn().into_iter().find_map(|shape| match shape {
            Shape::Rect(rectangle) if rectangle.fill == theme::DRAWER_BACKGROUND => Some(rectangle.rect.bottom()),
            _ => None,
        });
        for rack in racks_drawn(&window) {
            assert!(
                drawer_bottom.is_some_and(|bottom| rack.bottom() <= bottom + 0.6),
                "a rack is not seen below the drawer: {rack:?}, drawer to {drawer_bottom:?}"
            );
            how_much_of_the_rack_is_seen.push(rack.height());
        }
    }

    assert!(
        how_much_of_the_rack_is_seen
            .first()
            .is_none_or(|first| *first < theme::RACK_HEIGHT),
        "it is revealed as the drawer opens, and not all at once: {how_much_of_the_rack_is_seen:?}"
    );
    assert_close(
        *how_much_of_the_rack_is_seen.last().unwrap(),
        theme::RACK_HEIGHT,
        "all of it at last",
    );
}

// --- selecting ---

#[test]
fn clicking_a_keyframe_selects_it_and_only_it_and_it_has_the_outline_of_what_is_selected() {
    let (mut window, clip) = window_with_a_short_video();
    for at in [10, 30] {
        make_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, at);
    }
    assert_eq!(selected_keyframes_drawn(&window), 0);

    click_the_keyframe_at(&mut window, 0, 10);

    assert_eq!(selected_keyframes(&window).len(), 1);
    assert_eq!(selected_keyframes_drawn(&window), 1);
    assert!(window.application().selected_clips().is_empty(), "not the clip");

    click_the_keyframe_at(&mut window, 0, 30);
    assert_eq!(selected_keyframes(&window).len(), 1, "the first was let go of");
    assert_eq!(selected_keyframes_drawn(&window), 1);
}

#[test]
fn with_control_held_a_click_adds_a_keyframe_to_what_is_selected_and_lets_go_of_it_when_it_is_selected_already() {
    let (mut window, clip) = window_with_a_short_video();
    for at in [10, 30] {
        make_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, at);
    }
    click_the_keyframe_at(&mut window, 0, 10);

    let second = keyframe_position(&window, 0, 30);
    window.click_with_control(second);
    assert_eq!(selected_keyframes(&window).len(), 2);
    assert_eq!(selected_keyframes_drawn(&window), 2);

    window.click_with_control(second);
    assert_eq!(selected_keyframes(&window).len(), 1);
}

#[test]
fn selecting_a_keyframe_is_not_an_edit_and_the_keyframe_is_where_it_was() {
    let (mut window, clip) = window_with_a_short_video();
    make_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 10);
    window.application_mut().note_everything_is_saved();
    let before = window.application().project().clone();

    click_the_keyframe_at(&mut window, 0, 10);

    assert_eq!(window.application().project(), &before);
    assert!(!window.application().has_unsaved_changes());
    let id = selected_keyframes(&window)[0];
    assert_eq!(
        window
            .application()
            .project()
            .locate_keyframe(id)
            .unwrap()
            .keyframe
            .frame,
        10,
        "the keyframe is where it was"
    );
}

#[test]
fn a_keyframe_can_be_found_by_screen_readers_by_a_name_that_says_which_it_is() {
    let (mut window, clip) = window_with_a_short_video();
    make_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 10);
    let rack = window
        .application()
        .project()
        .drawer_of(AnimationTarget::Clip(clip))
        .unwrap()
        .racks()[0]
        .clone();
    let id = rack.keyframes()[0].id();

    let found = window.harness.get_by_label(&keyframe_label(id)).rect();

    assert_close(
        found.center().x,
        window.x_of_frame_on_the_strip(0, 10),
        "where it is drawn",
    );
}

// --- the label ---

#[test]
fn the_pointer_over_a_rack_makes_a_label_at_its_left_that_says_the_property_and_it_goes_with_the_pointer() {
    let (mut window, clip) = window_with_a_short_video();
    make_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 40);
    assert_eq!(
        label_drawn(&window),
        None,
        "no label while the pointer is not over a rack"
    );
    let rack = racks_drawn(&window)[0];

    window.move_pointer_to(Pos2::new(rack.left() + 40.0, rack.center().y));

    let (text, label) = label_drawn(&window).expect("the pointer is over the rack");
    assert_eq!(
        text, "Gain",
        "the rack is in the drawer of the clip it animates, so only the property is said"
    );
    assert!(
        label.right() <= rack.left() + 0.6,
        "to the left of the rack: {label:?} and {rack:?}"
    );
    assert_close(label.center().y, rack.center().y, "in line with its middle");

    window.move_pointer_to(Pos2::new(5.0, 5.0));
    assert_eq!(label_drawn(&window), None, "gone with the pointer");
}

#[test]
fn the_pointer_over_a_keyframe_makes_the_label_of_its_rack_too() {
    let (mut window, clip) = window_with_a_short_video();
    make_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Position, 40);

    let on_the_keyframe = keyframe_position(&window, 0, 40);
    window.move_pointer_to(on_the_keyframe);

    assert_eq!(label_drawn(&window).map(|(text, _)| text), Some("Position".to_owned()));
}

#[test]
fn a_rack_that_is_in_a_drawer_of_something_else_has_a_label_that_says_what_it_animates_too() {
    let (mut window, clip) = window_with_a_short_video();
    make_a_keyframe(&mut window, AnimationTarget::Clip(clip), PropertyId::Gain, 40);
    let rack = window
        .application()
        .project()
        .drawer_of(AnimationTarget::Clip(clip))
        .unwrap()
        .racks()[0]
        .id();
    let track = window.application().project().tracks()[0].id();
    window.do_this(Command::MoveRacks {
        racks: vec![rack],
        to: RackDestination::Drawer(AnimationTarget::Track(track)),
        shift_in_frames: 0,
        slot: None,
    });
    window.application_mut().note_everything_is_saved();
    let button = window
        .harness
        .get_by_label(&crate::application::timeline_panel::track_drawer_button_label(
            &window.title_of_track(0),
        ))
        .rect()
        .center();
    window.click(button);
    window.harness.run_steps(40);
    let in_the_track_drawer = racks_drawn(&window)[0];

    window.move_pointer_to(Pos2::new(
        in_the_track_drawer.left() + 60.0,
        in_the_track_drawer.center().y,
    ));

    assert_eq!(
        label_drawn(&window).map(|(text, _)| text),
        Some("Gain of holiday.mp4".to_owned()),
        "the file the clip is made from is what it is called"
    );
}
