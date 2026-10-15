// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fast-forward symbol on the preview, which can be dragged to move it and resized by its corner.
//! (What dragging does is decided with everything else on the preview, see `gestures`; this is where the
//! symbol is on the screen and what it looks like.)

use egui::{Color32, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, WidgetType, pos2, vec2};

use crate::application::textures::Textures;
use crate::application::theme;
use crate::project::{SpeedFactor, SpeedIndicatorLayout};
use crate::speed_indicator::{SMALLEST_HEIGHT_IN_PIXELS, indicator_aspect_ratio, place_indicator};

/// The size of the corner that is grabbed to resize the symbol, in points.
const RESIZE_CORNER_SIZE: f32 = 14.0;

/// The name the symbol is known by to screen readers (and the tests).
const SYMBOL_LABEL: &str = "Fast-forward symbol";

/// The name the corner of the symbol that resizes it is known by.
const SYMBOL_CORNER_LABEL: &str = "Fast-forward symbol size handle";

/// Where the symbol is on the screen.
#[derive(Clone, Copy, Debug)]
pub(super) struct SymbolOnScreen {
    pub body: Rect,
    /// The corner of the body that is grabbed to resize it.
    pub corner: Rect,
}

impl SymbolOnScreen {
    /// Where the symbol for `speed_factor` is, with `layout`, on the picture that is at `picture_rectangle`.
    pub fn of(layout: SpeedIndicatorLayout, speed_factor: SpeedFactor, picture_rectangle: Rect) -> Self {
        let placement = place_indicator(
            layout,
            picture_rectangle.width(),
            picture_rectangle.height(),
            indicator_aspect_ratio(speed_factor),
        );
        let body = Rect::from_min_size(
            picture_rectangle.min + vec2(placement.left, placement.top),
            vec2(placement.width, placement.height),
        );
        let corner = Rect::from_min_size(
            body.right_bottom() - vec2(RESIZE_CORNER_SIZE, RESIZE_CORNER_SIZE),
            vec2(RESIZE_CORNER_SIZE, RESIZE_CORNER_SIZE),
        );
        Self { body, corner }
    }

    /// The layout that has the symbol `delta` further along, as far as the picture lets it go.
    pub fn layout_moved_by(
        &self,
        layout: SpeedIndicatorLayout,
        picture_rectangle: Rect,
        delta: Vec2,
    ) -> SpeedIndicatorLayout {
        let top_left = self.body.left_top() - picture_rectangle.min + delta;
        SpeedIndicatorLayout::new(
            top_left.x / picture_rectangle.width(),
            top_left.y / picture_rectangle.height(),
            layout.height_fraction_of_frame_height(),
        )
    }

    /// The layout that has the corner of the symbol that resizes it at `pointer`: the symbol is as
    /// tall as the way down to the pointer, and its top left corner stays where it is.
    pub fn layout_resized_to(&self, picture_rectangle: Rect, pointer: Pos2) -> SpeedIndicatorLayout {
        let new_height = (pointer.y - self.body.top()).max(1.0);
        let top_left = self.body.left_top() - picture_rectangle.min;
        SpeedIndicatorLayout::new(
            top_left.x / picture_rectangle.width(),
            top_left.y / picture_rectangle.height(),
            new_height / picture_rectangle.height(),
        )
    }
}

/// Draws the symbol for `speed_factor` on the picture, with an outline and its corner when
/// `emphasised`, which is while the pointer is over it or has hold of it.
pub(super) fn show(
    ui: &mut Ui,
    symbol: &SymbolOnScreen,
    speed_factor: SpeedFactor,
    emphasised: bool,
    textures: &mut Textures,
) {
    let height_in_pixels =
        ((symbol.body.height() * ui.ctx().pixels_per_point()).round() as u32).max(SMALLEST_HEIGHT_IN_PIXELS);
    let texture = textures.symbol(ui.ctx(), speed_factor, height_in_pixels);
    ui.painter().image(
        texture.id(),
        symbol.body,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );

    // Neither of these takes anything from the mouse (that is for the preview as a whole), and they
    // cannot take the keyboard focus either, so the keys keep their meanings.
    ui.interact(symbol.body, Id::new("speed indicator body"), Sense::hover())
        .widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, SYMBOL_LABEL));
    ui.interact(symbol.corner, Id::new("speed indicator corner"), Sense::hover())
        .widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, SYMBOL_CORNER_LABEL));

    if emphasised {
        ui.painter().rect_stroke(
            symbol.body,
            2.0,
            Stroke::new(1.5, theme::INDICATOR_OUTLINE),
            StrokeKind::Outside,
        );
        ui.painter()
            .rect_filled(symbol.corner.shrink(3.0), 1.0, theme::INDICATOR_OUTLINE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture of 800 by 450 points with its top left corner at (100, 50).
    fn picture() -> Rect {
        Rect::from_min_size(pos2(100.0, 50.0), vec2(800.0, 450.0))
    }

    fn layout() -> SpeedIndicatorLayout {
        SpeedIndicatorLayout::new(0.1, 0.2, 0.15)
    }

    fn symbol() -> SymbolOnScreen {
        SymbolOnScreen::of(layout(), SpeedFactor::new(4).unwrap(), picture())
    }

    #[test]
    fn the_symbol_is_where_its_layout_says_on_the_picture_and_its_corner_is_the_bottom_right_of_it() {
        let symbol = symbol();
        assert!((symbol.body.left() - (100.0 + 0.1 * 800.0)).abs() < 1.0, "{symbol:?}");
        assert!((symbol.body.top() - (50.0 + 0.2 * 450.0)).abs() < 1.0, "{symbol:?}");
        assert!((symbol.body.height() - 0.15 * 450.0).abs() < 1.0, "{symbol:?}");
        assert_eq!(symbol.corner.right_bottom(), symbol.body.right_bottom());
        assert_eq!(symbol.corner.size(), vec2(RESIZE_CORNER_SIZE, RESIZE_CORNER_SIZE));
    }

    #[test]
    fn moving_the_symbol_moves_it_by_that_much_and_does_not_resize_it() {
        let symbol = symbol();
        let moved = symbol.layout_moved_by(layout(), picture(), vec2(80.0, 45.0));
        assert!((moved.left_fraction_of_frame_width() - 0.2).abs() < 0.005, "{moved:?}");
        assert!((moved.top_fraction_of_frame_height() - 0.3).abs() < 0.005, "{moved:?}");
        assert_eq!(moved.height_fraction_of_frame_height(), 0.15);
    }

    #[test]
    fn resizing_the_symbol_keeps_its_top_left_corner_and_makes_it_as_tall_as_the_way_down_to_the_pointer() {
        let symbol = symbol();
        let pointer = pos2(symbol.body.right() + 30.0, symbol.body.top() + 90.0);
        let resized = symbol.layout_resized_to(picture(), pointer);
        assert!(
            (resized.left_fraction_of_frame_width() - 0.1).abs() < 0.005,
            "{resized:?}"
        );
        assert!(
            (resized.top_fraction_of_frame_height() - 0.2).abs() < 0.005,
            "{resized:?}"
        );
        assert!(
            (resized.height_fraction_of_frame_height() - 90.0 / 450.0).abs() < 0.005,
            "{resized:?}"
        );
    }

    #[test]
    fn a_pointer_above_the_top_of_the_symbol_gives_it_a_height_and_not_less_than_one_point() {
        let symbol = symbol();
        let resized = symbol.layout_resized_to(picture(), pos2(500.0, symbol.body.top() - 100.0));
        assert!(resized.height_fraction_of_frame_height() > 0.0);
    }
}
