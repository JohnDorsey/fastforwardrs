//! The racks of keyframes in the drawers and the keyframe crates of the timeline: where they are on the screen, what they
//! look like (see [`super::rack_look`]) and what the mouse does to them.
//!
//! A rack is as long as the drawer it is in (or the crate), which goes along the timeline from where the clip it is of starts
//! to where it ends (for the drawer of a track, from the start of the timeline for ever), and racks are one under another in
//! the order they are in the drawer. Each keyframe in a rack is a diamond at the frame it is at.
//!
//! * Clicking a keyframe selects it (with Ctrl held it is added to what is selected, or let go of if it was).
//! * Dragging a keyframe moves it, and what is selected with it, along the timeline (see the `item_dragging` module).
//! * Dragging a rack takes it, with its keyframes, to another drawer or to an empty place on a track (see the
//!   `rack_dragging` module).
//! * With the pointer over a rack or a keyframe a label comes to the left of the rack, which says what it animates.

use egui::{
    Color32, Context, FontId, Id, LayerId, Order, PointerButton, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, WidgetInfo,
    WidgetType, pos2, vec2,
};

use super::grip_dots::{self, Arrangement};
use super::item_dragging::keyframes_dragged_on_their_own;
use super::rack_look::{self, KEYFRAME_HEIGHT};
use super::strip_view::StripView;
use super::view_state::{Grabbed, ItemDrag, RackDrag};
use super::{TimelineContent, TimelineViewState};
use crate::application::commands::Command;
use crate::application::naming::what_a_rack_is_called;
use crate::application::selection::{SelectionChange, SelectionItem};
use crate::application::textures::Textures;
use crate::application::theme;
use crate::project::{AnimationTarget, KeyframeId, Rack, RackDestination, RackId};

/// How far the left end of the grip dots of a rack is from the left end of the rack, in points.
const DOTS_MARGIN_AT_THE_LEFT: f32 = 4.0;

/// How much room a rack must have for its grip dots (which are not drawn if it has less), in points.
const LEAST_WIDTH_FOR_THE_DOTS: f32 = 28.0;

/// How far the label of a rack keeps from it, and from the edges of the window, in points.
const LABEL_GAP: f32 = 6.0;
const LABEL_MARGIN: f32 = 8.0;
const LABEL_PADDING: Pos2 = Pos2::new(7.0, 3.0);

/// How far below and to the right of the pointer the words that are said by it begin, in points.
const MESSAGE_OFFSET: egui::Vec2 = vec2(14.0, 18.0);

/// What a rack is known by to screen readers (and the tests).
pub fn rack_label(rack: RackId) -> String {
    format!("Rack {} of {}", rack.number(), rack.origin())
}

/// What a keyframe is known by to screen readers (and the tests).
pub fn keyframe_label(keyframe: KeyframeId) -> String {
    format!("Keyframe {} of {}", keyframe.number(), rack_label(keyframe.rack()))
}

/// A label that is drawn over everything at the left of a rack that the pointer is over, which says what the rack animates.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RackLabel {
    pub(super) text: String,
    /// Where the rack is, as far as it can be seen.
    pub(super) rack: Rect,
}

/// A drawer or a crate that a rack can be dragged into, and where it is on the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DropTarget {
    pub(super) rectangle: Rect,
    pub(super) destination: RackDestination,
}

/// Where the racks of one drawer (or crate) are on the screen.
#[derive(Clone, Copy, Debug)]
pub(super) struct DrawerPlace {
    /// Whose drawer it is: the clip or track that has it.
    pub(super) owner: AnimationTarget,
    /// Where the drawer begins along the strip, and where it ends (`f32::INFINITY` for the drawer of a track), on the screen.
    /// Either may be beyond the strip.
    pub(super) from_x: f32,
    pub(super) to_x: f32,
    /// The frame of the timeline that the drawer begins at, which is where the frames of its racks are counted from.
    pub(super) first_frame: u64,
    /// Where the first rack's top is, on the screen: the racks go down from there.
    pub(super) top: f32,
    /// The part of the screen the racks are seen in. (For a drawer that is sliding open, its racks are seen as far as it
    /// has opened.)
    pub(super) shown_in: Rect,
    /// Whether the drawer is open all the way, which is when its racks can be taken hold of.
    pub(super) is_open_all_the_way: bool,
}

/// Where the top of the rack that is `index`th in a drawer whose first rack's top is at `first_top` is.
pub(super) fn top_of_the_rack(first_top: f32, index: usize) -> f32 {
    first_top + index as f32 * (theme::RACK_HEIGHT + theme::SPACE_BETWEEN_RACKS)
}

/// Draws the racks of the drawer at `place` (the racks that are in it, in `content`'s project), and adds a command for what is
/// done to them.
pub(super) fn show(
    ui: &Ui,
    place: DrawerPlace,
    content: &TimelineContent<'_>,
    view: &StripView,
    state: &mut TimelineViewState,
    textures: &mut Textures,
    commands: &mut Vec<Command>,
) {
    let Some(drawer) = content.project.drawer_of(place.owner) else {
        return;
    };
    if place.shown_in.width() <= 0.0 || place.shown_in.height() <= 0.0 {
        return;
    }
    // (A drawer with nothing in it is a place that a rack can be put in as much as one with racks in it.)
    if place.is_open_all_the_way {
        state.drop_targets.push(DropTarget {
            rectangle: place.shown_in,
            destination: RackDestination::Drawer(place.owner),
        });
    }
    if drawer.is_empty() {
        return;
    }
    let painter = ui.painter_at(place.shown_in);
    // (The keyframes at the ends of what can be seen are not cut in half by the edge of it: they go a little way into the
    // room beside the strip.)
    let keyframe_painter = ui
        .painter()
        .with_clip_rect(place.shown_in.expand2(vec2(KEYFRAME_HEIGHT / 2.0 + 1.0, 0.0)));
    let grain = textures.rack_grain(ui.ctx()).id();
    let can_touch = place.is_open_all_the_way && content.can_edit;
    for (index, rack) in drawer.racks().iter().enumerate() {
        let top = top_of_the_rack(place.top, index);
        let whole = Rect::from_min_max(pos2(place.from_x, top), pos2(place.to_x, top + theme::RACK_HEIGHT));
        let visible = whole.intersect(place.shown_in);
        if visible.width() <= 0.0 || visible.height() <= 0.0 {
            continue;
        }
        let corners = rack_look::corners(
            whole.left() < place.shown_in.left(),
            whole.right() > place.shown_in.right(),
        );
        rack_look::draw_rack(&painter, visible, corners, grain, pos2(whole.left(), top));
        let dots = Arrangement::FOR_A_RACK.size();
        if visible.width() >= LEAST_WIDTH_FOR_THE_DOTS {
            grip_dots::draw(
                &painter,
                Arrangement::FOR_A_RACK,
                pos2(
                    visible.left() + DOTS_MARGIN_AT_THE_LEFT + dots.x / 2.0,
                    visible.center().y,
                ),
            );
        }
        let body = ui.interact(
            visible,
            ui.id().with(("rack", rack.id())),
            if can_touch {
                Sense::CLICK | Sense::DRAG
            } else {
                Sense::hover()
            },
        );
        body.widget_info(|| WidgetInfo::labeled(WidgetType::Other, can_touch, rack_label(rack.id())));
        if can_touch
            && body.drag_started_by(PointerButton::Primary)
            && let Some(press) = ui.input(|input| input.pointer.press_origin())
            && let Some(first_frame) = rack.first_frame()
        {
            state.rack_drag = Some(RackDrag {
                rack: rack.id(),
                pointer_frame_at_the_start: view.frame_at(press.x).map_or(0, |frame| frame.value()),
                first_keyframe_frame_at_the_start: place.first_frame + first_frame,
            });
        }
        let mut is_hovered = body.hovered();
        is_hovered |= show_the_keyframes(
            ui,
            &keyframe_painter,
            (rack, &place, visible),
            (content, view),
            can_touch,
            (state, commands),
        );
        if is_hovered {
            state.rack_labels.push(RackLabel {
                text: what_a_rack_is_called(content.project, rack, place.owner, content.source_looks),
                rack: visible,
            });
        }
    }
}

/// Draws the keyframes of `rack`, which is in the drawer at `place` and is seen in `visible`, and adds a command for what is
/// done to them. Whether the pointer is over one of them.
fn show_the_keyframes(
    ui: &Ui,
    painter: &egui::Painter,
    (rack, place, visible): (&Rack, &DrawerPlace, Rect),
    (content, view): (&TimelineContent<'_>, &StripView),
    can_touch: bool,
    (state, commands): (&mut TimelineViewState, &mut Vec<Command>),
) -> bool {
    let adding_to_the_selection = ui.input(|input| input.modifiers.command);
    let mut is_hovered = false;
    for keyframe in rack.keyframes() {
        let x = view.screen_x_of_frame(place.first_frame + keyframe.frame);
        if x < visible.left() - KEYFRAME_HEIGHT || x > visible.right() + KEYFRAME_HEIGHT {
            continue;
        }
        let centre = pos2(x, visible.center().y);
        let id = rack.id_of(keyframe);
        let response = ui.interact(
            rack_look::keyframe_grip(centre),
            ui.id().with(("keyframe", id)),
            if can_touch {
                Sense::CLICK | Sense::DRAG
            } else {
                Sense::hover()
            },
        );
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, can_touch, keyframe_label(id)));
        is_hovered |= response.hovered();
        if can_touch {
            state.keyframe_grips.push((id, rack_look::keyframe_grip(centre)));
        }
        rack_look::draw_keyframe(
            painter,
            centre,
            content.selection.contains_keyframe(id),
            response.hovered(),
        );
        if response.clicked() {
            commands.push(Command::SelectItems {
                items: vec![SelectionItem::Keyframe(id)],
                change: if adding_to_the_selection {
                    SelectionChange::Toggle
                } else {
                    SelectionChange::Replace
                },
            });
        }
        if can_touch
            && response.drag_started_by(PointerButton::Primary)
            && let Some(press) = ui.input(|input| input.pointer.press_origin())
        {
            take_hold_of_the_keyframe(
                (id, place.first_frame + keyframe.frame, press),
                (content, view),
                (state, commands),
                adding_to_the_selection,
            );
        }
    }
    is_hovered
}

/// Takes hold of the keyframe `keyframe`, which is at frame `frame` of the timeline, when it begins to be dragged, the pointer
/// having been pressed at `press`: what is dragged is what is selected if it is one of what is selected, and otherwise it
/// alone (which is selected, with Ctrl added to what was selected).
fn take_hold_of_the_keyframe(
    (keyframe, frame, press): (KeyframeId, u64, Pos2),
    (content, view): (&TimelineContent<'_>, &StripView),
    (state, commands): (&mut TimelineViewState, &mut Vec<Command>),
    adding_to_the_selection: bool,
) {
    let (clips, keyframes) = if content.selection.contains_keyframe(keyframe) {
        (content.selection.to_vec(), content.selection.keyframes_to_vec())
    } else {
        commands.push(Command::SelectItems {
            items: vec![SelectionItem::Keyframe(keyframe)],
            change: if adding_to_the_selection {
                SelectionChange::Toggle
            } else {
                SelectionChange::Replace
            },
        });
        let (clips, mut keyframes) = if adding_to_the_selection {
            (content.selection.to_vec(), content.selection.keyframes_to_vec())
        } else {
            (Vec::new(), Vec::new())
        };
        keyframes.push(keyframe);
        (clips, keyframes)
    };
    state.item_drag = Some(ItemDrag {
        keyframes: keyframes_dragged_on_their_own(content.project, &clips, &keyframes),
        clips,
        grabbed: Grabbed::Keyframe(keyframe),
        pointer_frame_at_the_start: view.frame_at(press.x).map_or(0, |frame| frame.value()),
        grabbed_frame_at_the_start: frame,
        start_time: None,
    });
}

/// How big the label for `text` is, and where it goes: at the left of `rack` (which is as far as it can be seen), the middle
/// of it in line with the middle of the rack, and kept inside `inside`.
pub(super) fn label_rectangle(text_size: egui::Vec2, rack: Rect, inside: Rect) -> Rect {
    let size = text_size + LABEL_PADDING.to_vec2() * 2.0;
    let top_left = pos2(rack.left() - LABEL_GAP - size.x, rack.center().y - size.y / 2.0);
    let rectangle = Rect::from_min_size(top_left, size);
    let keep_inside = inside.shrink(LABEL_MARGIN);
    let shift = vec2(
        (keep_inside.left() - rectangle.left()).max(0.0) - (rectangle.right() - keep_inside.right()).max(0.0),
        (keep_inside.top() - rectangle.top()).max(0.0) - (rectangle.bottom() - keep_inside.bottom()).max(0.0),
    );
    rectangle.translate(shift)
}

/// How big the words said by the pointer are, and where they go: a little below and to the right of `pointer`, brought back
/// inside `screen` when they would go past the right edge or the bottom edge of it.
pub(super) fn message_rectangle(text_size: egui::Vec2, pointer: Pos2, screen: Rect) -> Rect {
    let rectangle = Rect::from_min_size(pointer + MESSAGE_OFFSET, text_size + LABEL_PADDING.to_vec2() * 2.0);
    let keep_inside = screen.shrink(LABEL_MARGIN);
    let shift = vec2(
        -(rectangle.right() - keep_inside.right()).max(0.0),
        -(rectangle.bottom() - keep_inside.bottom()).max(0.0),
    );
    rectangle.translate(shift)
}

/// Draws what is said by the pointer (`message`, if there is something to say), over everything that is on the screen: the
/// words "not enough space here" when a rack that is dragged cannot go where it is over.
pub(super) fn draw_the_message_by_the_pointer(context: &Context, message: Option<&str>) {
    let (Some(message), Some(pointer)) = (message, context.input(|input| input.pointer.hover_pos())) else {
        return;
    };
    let painter = context.layer_painter(LayerId::new(Order::Tooltip, Id::new("what is said by the pointer")));
    let galley = painter.layout_no_wrap(message.to_owned(), FontId::proportional(13.0), theme::RACK_LABEL_TEXT);
    let rectangle = message_rectangle(galley.size(), pointer, context.viewport_rect());
    painter.rect_filled(rectangle, 4.0, theme::RACK_LABEL_BACKGROUND);
    painter.rect_stroke(
        rectangle,
        4.0,
        Stroke::new(1.0, Color32::from_white_alpha(40)),
        StrokeKind::Inside,
    );
    painter.galley(rectangle.min + LABEL_PADDING.to_vec2(), galley, theme::RACK_LABEL_TEXT);
}

/// Draws the labels of the racks that the pointer was over, over everything that is on the screen.
pub(super) fn draw_the_labels(context: &Context, labels: &[RackLabel]) {
    if labels.is_empty() {
        return;
    }
    let painter = context.layer_painter(LayerId::new(
        Order::Tooltip,
        Id::new("labels of the racks of keyframes"),
    ));
    let screen = context.viewport_rect();
    for label in labels {
        let galley = painter.layout_no_wrap(label.text.clone(), FontId::proportional(13.0), theme::RACK_LABEL_TEXT);
        let rectangle = label_rectangle(galley.size(), label.rack, screen);
        painter.rect_filled(rectangle, 4.0, theme::RACK_LABEL_BACKGROUND);
        painter.rect_stroke(
            rectangle,
            4.0,
            Stroke::new(1.0, Color32::from_white_alpha(40)),
            StrokeKind::Inside,
        );
        painter.galley(rectangle.min + LABEL_PADDING.to_vec2(), galley, theme::RACK_LABEL_TEXT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_racks_of_a_drawer_are_one_under_another_with_the_room_between_them_that_the_theme_says() {
        let tops: Vec<f32> = (0..4).map(|index| top_of_the_rack(100.0, index)).collect();
        assert_eq!(tops[0], 100.0);
        for pair in tops.windows(2) {
            assert_eq!(pair[1] - pair[0], theme::RACK_HEIGHT + theme::SPACE_BETWEEN_RACKS);
        }
    }

    #[test]
    fn the_label_is_at_the_left_of_the_rack_in_line_with_its_middle() {
        let rack = Rect::from_min_size(pos2(400.0, 300.0), vec2(500.0, 16.0));
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0));

        let label = label_rectangle(vec2(60.0, 14.0), rack, screen);

        assert_eq!(label.center().y, rack.center().y);
        assert_eq!(label.right(), rack.left() - LABEL_GAP, "just before the rack");
        assert!(
            label.width() > 60.0 && label.height() > 14.0,
            "with room round the words"
        );
    }

    #[test]
    fn a_label_that_would_be_off_the_screen_is_brought_back_on_it() {
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0));
        let at_the_left_edge = Rect::from_min_size(pos2(10.0, 300.0), vec2(500.0, 16.0));
        let label = label_rectangle(vec2(60.0, 14.0), at_the_left_edge, screen);
        assert!(label.left() >= LABEL_MARGIN - 1e-4, "{label:?}");
        let at_the_top = Rect::from_min_size(pos2(400.0, 2.0), vec2(500.0, 16.0));
        assert!(label_rectangle(vec2(60.0, 14.0), at_the_top, screen).top() >= LABEL_MARGIN - 1e-4);
        let at_the_bottom = Rect::from_min_size(pos2(400.0, 798.0), vec2(500.0, 16.0));
        assert!(label_rectangle(vec2(60.0, 14.0), at_the_bottom, screen).bottom() <= 800.0 - LABEL_MARGIN + 1e-4);
    }

    #[test]
    fn the_words_by_the_pointer_are_a_little_below_and_to_the_right_of_it_with_room_round_them() {
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0));
        let pointer = pos2(300.0, 200.0);

        let rectangle = message_rectangle(vec2(120.0, 14.0), pointer, screen);

        assert_eq!(rectangle.left_top(), pointer + MESSAGE_OFFSET);
        assert_eq!(rectangle.size(), vec2(120.0, 14.0) + LABEL_PADDING.to_vec2() * 2.0);
    }

    #[test]
    fn the_words_by_the_pointer_are_brought_back_when_they_would_go_past_the_right_or_the_bottom_of_the_screen() {
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0));
        let words = vec2(120.0, 14.0);

        let at_the_right = message_rectangle(words, pos2(1270.0, 200.0), screen);
        assert!(
            (at_the_right.right() - (1280.0 - LABEL_MARGIN)).abs() < 1e-4,
            "{at_the_right:?}"
        );
        assert_eq!(at_the_right.top(), 200.0 + MESSAGE_OFFSET.y, "only sideways");
        assert_eq!(
            at_the_right.size(),
            words + LABEL_PADDING.to_vec2() * 2.0,
            "as big as ever"
        );

        let at_the_bottom = message_rectangle(words, pos2(300.0, 795.0), screen);
        assert!(
            (at_the_bottom.bottom() - (800.0 - LABEL_MARGIN)).abs() < 1e-4,
            "{at_the_bottom:?}"
        );
        assert_eq!(at_the_bottom.left(), 300.0 + MESSAGE_OFFSET.x, "only up");

        let in_the_corner = message_rectangle(words, pos2(1279.0, 799.0), screen);
        assert!(in_the_corner.right() <= 1280.0 - LABEL_MARGIN + 1e-4);
        assert!(in_the_corner.bottom() <= 800.0 - LABEL_MARGIN + 1e-4);
    }

    #[test]
    fn a_keyframe_and_a_rack_are_known_by_names_that_say_which() {
        let rack = RackId::new(crate::project::TrackId::new(2), 5);
        assert_eq!(rack_label(rack), "Rack 5 of track 2");
        assert_eq!(
            keyframe_label(KeyframeId::new(rack, 3)),
            "Keyframe 3 of Rack 5 of track 2"
        );
    }
}
