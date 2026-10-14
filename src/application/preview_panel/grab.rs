//! What a press on the preview takes hold of.
//!
//! This is worked out from where the press was and what is on the preview, and from nothing else: not
//! from what the pointer was over a moment before, and not from the widgets of the frame before. (A click
//! that comes in the same frame as the move that brought the pointer there, as the tap of a finger
//! does, finds what is under it as well as one that comes when the pointer has rested.)

use egui::Pos2;

use super::PreviewContent;
use super::geometry::{PictureOnScreen, corner_to_turn_by, corner_whose_handle_is_at};
use super::speed_indicator_overlay::SymbolOnScreen;
use crate::project::{ClipId, Corner};

/// Something on the preview that the pointer can take hold of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Grab {
    /// A video, a picture or a text object: a click picks it, and dragging moves it.
    Layer(ClipId),
    /// The square handle on a corner of the selected layer: dragging it makes the layer bigger or smaller.
    Handle { layer: ClipId, corner: Corner },
    /// The place just outside a corner of the selected layer: dragging round from it turns the layer.
    PlaceToTurnFrom { layer: ClipId, corner: Corner },
    /// The fast-forward symbol: dragging it moves it.
    Symbol,
    /// The corner of the fast-forward symbol: dragging it makes the symbol bigger or smaller.
    SymbolCorner,
}

/// What there is to take hold of at `point` on the screen, if anything.
///
/// The handles and the places to turn the selected layer from are drawn over everything else on the preview, wherever
/// they are on it, so they come first. Then, from the front to the back, each layer is as far as it is on the
/// screen (turned as it is, and not as far as the rectangle round it), with the fast-forward symbol, which
/// is over the front-most video, between that video and the layers in front of it. What is past the edge
/// of the picture is not seen, so a layer is only taken hold of where it is over the picture, but for the
/// selected one, whose outline is drawn all the way round it, and is taken hold of inside that.
pub(super) fn grab_at(content: &PreviewContent<'_>, picture: &PictureOnScreen, point: Pos2) -> Option<Grab> {
    if let Some(selected) = content.selected_layer() {
        if let Some(corner) = corner_whose_handle_is_at(picture, &selected.placement, point) {
            return Some(Grab::Handle {
                layer: selected.clip,
                corner,
            });
        }
        if let Some(corner) = corner_to_turn_by(picture, &selected.placement, point) {
            return Some(Grab::PlaceToTurnFrom {
                layer: selected.clip,
                corner,
            });
        }
    }
    let is_over_the_picture = picture.rectangle.contains(point);
    for layer in content.layers.iter().rev() {
        if let Some(symbol) = content.symbol.filter(|symbol| symbol.over == layer.clip) {
            let on_screen = SymbolOnScreen::of(content.indicator_layout, symbol.speed, picture.rectangle);
            if on_screen.corner.contains(point) {
                return Some(Grab::SymbolCorner);
            }
            if on_screen.body.contains(point) {
                return Some(Grab::Symbol);
            }
        }
        let is_where_it_can_be_taken_hold_of = is_over_the_picture || content.selected_clip == Some(layer.clip);
        if is_where_it_can_be_taken_hold_of && picture.contains(&layer.placement, point) {
            return Some(Grab::Layer(layer.clip));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use egui::{pos2, vec2};

    use super::*;
    use crate::application::preview_panel::SymbolOnThePreview;
    use crate::application::preview_panel::test_content::{
        clip, content, in_the_middle, layer, picture, whole_picture,
    };
    use crate::project::{Angle, CanvasArea, Placement, SpeedFactor};

    #[test]
    fn what_is_in_front_is_taken_hold_of_before_what_is_behind_it() {
        let content = content(vec![layer(0, whole_picture()), layer(1, in_the_middle())], None);
        assert_eq!(
            grab_at(&content, &picture(), pos2(500.0, 275.0)),
            Some(Grab::Layer(clip(1)))
        );
        assert_eq!(
            grab_at(&content, &picture(), pos2(150.0, 100.0)),
            Some(Grab::Layer(clip(0))),
            "and beside it, what is behind"
        );
    }

    #[test]
    fn nothing_is_taken_hold_of_where_there_is_nothing() {
        let with_a_box = content(vec![layer(1, in_the_middle())], None);
        assert_eq!(grab_at(&with_a_box, &picture(), pos2(150.0, 100.0)), None);
        let empty = content(vec![], None);
        assert_eq!(grab_at(&empty, &picture(), pos2(500.0, 275.0)), None);
    }

    #[test]
    fn a_layer_that_is_turned_is_taken_hold_of_where_it_is_and_not_where_the_rectangle_round_it_is() {
        // 400 by 90 points turned an eighth of a turn: it only reaches 45 points either way across.
        let turned = Placement::new(CanvasArea::new(0.25, 0.5, 0.5, 0.2), Angle::from_degrees(45.0));
        let content = content(vec![layer(0, whole_picture()), layer(1, turned)], None);
        let picture = picture();
        let bounds = picture.bounding_rectangle_of(&turned);
        assert_eq!(
            grab_at(&content, &picture, bounds.left_top() + vec2(2.0, 2.0)),
            Some(Grab::Layer(clip(0))),
            "the corner of the rectangle round it is the video behind"
        );
        assert_eq!(
            grab_at(&content, &picture, picture.middle_of(&turned)),
            Some(Grab::Layer(clip(1)))
        );
    }

    #[test]
    fn the_handles_and_the_places_to_turn_from_of_the_selected_layer_are_over_everything_else() {
        // Layer 1 is selected, and layer 2, in front of it, covers its top left corner.
        let covering_the_corner = Placement::upright(CanvasArea::new(0.2, 0.2, 0.2, 0.2));
        let content = content(
            vec![
                layer(0, whole_picture()),
                layer(1, in_the_middle()),
                layer(2, covering_the_corner),
            ],
            Some(1),
        );
        let picture = picture();
        let corner = picture.corner_of(&in_the_middle(), Corner::TopLeft);
        assert_eq!(
            grab_at(&content, &picture, corner),
            Some(Grab::Handle {
                layer: clip(1),
                corner: Corner::TopLeft
            }),
            "the handle, though a layer in front is over the corner"
        );
        assert_eq!(
            grab_at(&content, &picture, corner + vec2(-14.0, -14.0)),
            Some(Grab::PlaceToTurnFrom {
                layer: clip(1),
                corner: Corner::TopLeft
            }),
            "and the place to turn from, just outside it"
        );
        assert_eq!(
            grab_at(&content, &picture, corner + vec2(30.0, 30.0)),
            Some(Grab::Layer(clip(2))),
            "but not inside, where what is in front is"
        );
    }

    /// A box 400 by 225 points that begins 80 points before the picture: from (20, 162.5) to (420, 387.5).
    fn sticking_out_at_the_left() -> Placement {
        Placement::upright(CanvasArea::new(-0.1, 0.25, 0.5, 0.5))
    }

    #[test]
    fn a_layer_that_is_not_selected_is_only_taken_hold_of_where_it_is_over_the_picture() {
        let content = content(vec![layer(1, sticking_out_at_the_left())], None);
        let picture = picture();
        let before_the_picture = pos2(60.0, 275.0);
        assert!(
            sticking_out_at_the_left().contains(picture.fractions_at(before_the_picture), picture.size_in_pixels),
            "the setup: the point is on the layer"
        );
        assert!(
            !picture.rectangle.contains(before_the_picture),
            "and not on the picture"
        );
        assert_eq!(grab_at(&content, &picture, before_the_picture), None);
        assert_eq!(
            grab_at(&content, &picture, pos2(200.0, 275.0)),
            Some(Grab::Layer(clip(1))),
            "but over the picture it is"
        );
    }

    #[test]
    fn the_selected_layer_is_taken_hold_of_wherever_it_is_even_where_it_is_not_over_the_picture() {
        let content = content(vec![layer(1, sticking_out_at_the_left())], Some(1));
        assert_eq!(
            grab_at(&content, &picture(), pos2(60.0, 275.0)),
            Some(Grab::Layer(clip(1)))
        );
    }

    #[test]
    fn where_the_picture_ends_what_is_taken_hold_of_is_the_selected_layer_and_not_the_others_that_reach_out_the_same_way()
     {
        // Layer 0 is selected and layer 1 is in front of it, and both begin before the picture.
        let content = content(
            vec![
                layer(0, sticking_out_at_the_left()),
                layer(1, sticking_out_at_the_left()),
            ],
            Some(0),
        );
        let picture = picture();
        assert_eq!(
            grab_at(&content, &picture, pos2(60.0, 275.0)),
            Some(Grab::Layer(clip(0))),
            "before the picture: only the selected layer is"
        );
        assert_eq!(
            grab_at(&content, &picture, pos2(200.0, 275.0)),
            Some(Grab::Layer(clip(1))),
            "over the picture: the one in front"
        );
    }

    #[test]
    fn the_handles_and_the_places_to_turn_from_of_the_selected_layer_are_taken_hold_of_far_from_the_picture() {
        let content = content(vec![layer(1, sticking_out_at_the_left())], Some(1));
        let picture = picture();
        let corner = picture.corner_of(&sticking_out_at_the_left(), Corner::TopLeft);
        assert!(
            !picture.rectangle.contains(corner),
            "the setup: the corner is before the picture"
        );
        assert_eq!(
            grab_at(&content, &picture, corner),
            Some(Grab::Handle {
                layer: clip(1),
                corner: Corner::TopLeft
            })
        );
        assert_eq!(
            grab_at(&content, &picture, corner + vec2(-20.0, -20.0)),
            Some(Grab::PlaceToTurnFrom {
                layer: clip(1),
                corner: Corner::TopLeft
            })
        );
    }

    #[test]
    fn a_layer_that_is_not_selected_has_no_handles_and_no_places_to_turn_from() {
        let content = content(vec![layer(0, whole_picture()), layer(1, in_the_middle())], None);
        let picture = picture();
        let corner = picture.corner_of(&in_the_middle(), Corner::TopLeft);
        assert_eq!(
            grab_at(&content, &picture, corner + vec2(-14.0, -14.0)),
            Some(Grab::Layer(clip(0)))
        );
        assert_eq!(grab_at(&content, &picture, corner), Some(Grab::Layer(clip(1))));
    }

    #[test]
    fn the_symbol_is_over_its_video_and_behind_the_layers_in_front_of_that_video() {
        let mut content = content(
            vec![
                layer(0, whole_picture()),
                layer(1, Placement::upright(CanvasArea::new(0.0, 0.0, 1.0, 0.5))),
            ],
            None,
        );
        content.symbol = Some(SymbolOnThePreview {
            over: clip(0),
            speed: SpeedFactor::new(4).unwrap(),
        });
        let picture = picture();
        let symbol = SymbolOnScreen::of(
            content.indicator_layout,
            SpeedFactor::new(4).unwrap(),
            picture.rectangle,
        );
        // The symbol is near the top left of the picture, where layer 1 (in front of the video) covers it.
        assert!(
            picture.rectangle.top() + 0.5 * picture.rectangle.height() > symbol.body.bottom(),
            "the setup: the symbol is in the half that layer 1 covers"
        );
        assert_eq!(
            grab_at(&content, &picture, symbol.body.center()),
            Some(Grab::Layer(clip(1))),
            "what is in front of the symbol has the pointer first"
        );

        // With nothing in front of it, the symbol is taken hold of before the video it is over.
        content.layers.pop();
        assert_eq!(grab_at(&content, &picture, symbol.body.center()), Some(Grab::Symbol));
        assert_eq!(
            grab_at(&content, &picture, symbol.corner.center()),
            Some(Grab::SymbolCorner),
            "and its corner is the corner"
        );
        assert_eq!(
            grab_at(&content, &picture, symbol.body.right_bottom() + vec2(30.0, 30.0)),
            Some(Grab::Layer(clip(0)))
        );
    }

    #[test]
    fn a_symbol_whose_video_is_not_there_is_not_there_either() {
        let mut content = content(vec![layer(1, whole_picture())], None);
        content.symbol = Some(SymbolOnThePreview {
            over: clip(0),
            speed: SpeedFactor::new(4).unwrap(),
        });
        let symbol = SymbolOnScreen::of(
            content.indicator_layout,
            SpeedFactor::new(4).unwrap(),
            picture().rectangle,
        );
        assert_eq!(
            grab_at(&content, &picture(), symbol.body.center()),
            Some(Grab::Layer(clip(1)))
        );
    }
}
