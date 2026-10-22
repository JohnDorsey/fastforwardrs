// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The live preview: the finished picture for the current frame, which is every video and text
//! object that is seen, one over another, each where it is on the picture and turned as far as it
//! is turned, with the fast-forward symbol over the video that is in front when that is sped up.
//!
//! The symbol can be moved and resized right on the preview. It is the same one symbol for the
//! whole project, so it is set once and shows in the same place in every sped-up clip. Nothing is
//! drawn for this unless the pointer is over the symbol: then an outline and a corner to grab
//! appear.
//!
//! Videos, pictures and text objects can be moved by dragging them, and the selected one can be
//! resized by dragging any of its four corners and turned by dragging just outside a corner. What
//! the pointer can take hold of where is worked out in `grab`, what is done with it in `gestures`,
//! and what is drawn in `layers`.
//!
//! They can be put partly past the edge of the picture. What is past it is not seen (it is cut off,
//! as in the export), so only the selected one is taken hold of there; and the box of the selected one
//! (its outline, its corners, the places to turn it from) is drawn and taken hold of wherever it is over the preview panel,
//! outside the picture too, and nowhere else: it is not drawn over the other panels. The widgets that take the mouse are
//! made in `catchers`, and the names the parts of the preview are known by are in `names`.
//!
//! The picture can be zoomed out, to half the size it fits the panel at, and in, until its pixels can be seen, with the wheel
//! (see `zooming`), and moved about when it is bigger than the panel. Where it is drawn is worked out in `view`, from how it is
//! zoomed; the layers and what the mouse does to them are all in terms of where the picture is, so they are the same whatever
//! the zoom is. A button in the bottom right corner of the panel brings the picture back to where it fits, while it is not there.

mod catchers;
#[cfg(test)]
mod engine_parity_tests;
mod engine_preview;
mod engine_view;
#[cfg(test)]
mod engine_view_tests;
mod geometry;
mod gestures;
mod grab;
mod layers;
mod names;
mod speed_indicator_overlay;
#[cfg(test)]
mod test_content;
mod view;
#[cfg(test)]
mod zoomed_pictures_tests;
mod zooming;

#[cfg(test)]
pub use engine_preview::render_size_for;
pub use engine_preview::{EnginePreview, HowTheClockGoes};
pub use view::PreviewView;

// (The content of the preview is described by `frame_scene`, which the export uses too: what the preview shows and what the export shows are the same picture.)
pub use crate::frame_scene::{PreviewContent, PreviewLayer, PreviewLayerContent, VideoInThePicture};
// (Only the tests of the preview make a symbol by hand: the window and the export both get it from `frame_scene::symbol_of_a_section`.)
#[cfg(test)]
pub use crate::frame_scene::SymbolOnThePreview;
#[cfg(test)]
pub use zooming::FIT_BUTTON_LABEL;

// The names the layers and their handles are known by to screen readers, which the tests look them up by.
#[cfg(test)]
pub use names::{
    catcher_id, corner_handle_id, corner_handle_label, picture_label, text_object_label, turn_handle_label,
    turn_place_id, video_label,
};

use egui::{Rect, Ui, vec2};

use super::commands::Command;
use super::textures::Textures;
use crate::project::ClipId;
use crate::text_rendering::LazyTextRenderer;
use geometry::PictureOnScreen;
use speed_indicator_overlay::SymbolOnScreen;

impl PreviewLayer<'_> {
    /// Whether the layer takes whatever shape it is dragged to when it is resized, as a text object and a generator do (a video and a picture keep their own).
    fn takes_any_shape(&self) -> bool {
        matches!(
            self.content,
            PreviewLayerContent::Text(_) | PreviewLayerContent::Generator
        )
    }

    /// Whether the layer is a video that is all of the picture, so that there is nothing outside it.
    fn fills_the_picture(&self) -> bool {
        matches!(self.content, PreviewLayerContent::Video(_))
            && self.placement.rotation.is_zero()
            && self.placement.area == crate::project::CanvasArea::WHOLE_PICTURE
    }

    /// The name the layer is known by to screen readers (and the tests).
    fn label(&self) -> String {
        match self.content {
            PreviewLayerContent::Text(_) => names::text_object_label(self.clip),
            PreviewLayerContent::Video(_) => names::video_label(self.clip),
            PreviewLayerContent::Image { .. } => names::picture_label(self.clip),
            PreviewLayerContent::Generator => names::generator_label(self.clip),
        }
    }
}

impl<'a> PreviewContent<'a> {
    /// The layer of `clip`, if it is on the preview.
    fn layer(&self, clip: ClipId) -> Option<&PreviewLayer<'a>> {
        self.layers.iter().find(|layer| layer.clip == clip)
    }

    /// The selected layer, if there is one and it is on the preview.
    fn selected_layer(&self) -> Option<&PreviewLayer<'a>> {
        self.selected_clip.and_then(|clip| self.layer(clip))
    }

    /// Where the symbol is on the screen, if it is shown.
    fn symbol_on_screen(&self, picture: &PictureOnScreen) -> Option<SymbolOnScreen> {
        self.symbol
            .map(|symbol| SymbolOnScreen::of(self.indicator_layout, symbol.speed, picture.rectangle))
    }
}

/// Fills the rest of the window with the preview, with the picture zoomed and moved as `view` says (which the wheel and the mouse
/// change). The pictures of the layers are drawn by the window itself, one texture and one quad for each.
pub fn show(
    ui: &mut Ui,
    content: &PreviewContent<'_>,
    view: &mut PreviewView,
    textures: &mut Textures,
    text_renderer: &mut LazyTextRenderer,
    commands: &mut Vec<Command>,
) {
    show_how(
        ui,
        content,
        view,
        HowThePicturesAreMade::ByTheWindow {
            textures,
            text_renderer,
        },
        commands,
    );
}

/// Like [`show`], but the picture of all the layers together is made by the picture engine, and the window puts it on the screen as one picture under
/// everything else that it draws: the outlines, the handles, the places the mouse can take hold of.
pub fn show_drawn_by_the_engine(
    ui: &mut Ui,
    content: &PreviewContent<'_>,
    view: &mut PreviewView,
    engine: &mut EnginePreview,
    text_renderer: &mut LazyTextRenderer,
    clock: HowTheClockGoes,
    commands: &mut Vec<Command>,
) {
    show_how(
        ui,
        content,
        view,
        HowThePicturesAreMade::ByTheEngine {
            engine,
            text_renderer,
            clock,
        },
        commands,
    );
}

/// Who makes the picture of the preview.
enum HowThePicturesAreMade<'a> {
    ByTheWindow {
        textures: &'a mut Textures,
        text_renderer: &'a mut LazyTextRenderer,
    },
    ByTheEngine {
        engine: &'a mut EnginePreview,
        text_renderer: &'a mut LazyTextRenderer,
        clock: HowTheClockGoes,
    },
}

fn show_how(
    ui: &mut Ui,
    content: &PreviewContent<'_>,
    view: &mut PreviewView,
    how: HowThePicturesAreMade<'_>,
    commands: &mut Vec<Command>,
) {
    let panel_rectangle = ui.available_rect_before_wrap();

    let Some(picture_size) = content.picture_size else {
        return;
    };
    zooming::handle_the_wheel_and_the_mouse(ui, view, panel_rectangle, picture_size);
    let picture_rectangle = view.picture_rectangle(panel_rectangle, picture_size);
    // (What is drawn is cut off at the edge of the panel, as everything is.)
    ui.painter().rect_filled(picture_rectangle, 0.0, egui::Color32::BLACK);
    let pixels_per_point = ui.ctx().pixels_per_point();
    let filter = view.filter(panel_rectangle, picture_size, pixels_per_point);
    let drawing = match how {
        HowThePicturesAreMade::ByTheWindow {
            textures,
            text_renderer,
        } => layers::Drawing::ByTheWindow {
            textures,
            text_renderer,
        },
        HowThePicturesAreMade::ByTheEngine {
            engine,
            text_renderer,
            clock,
        } => {
            let render_size = engine_preview::render_size_for(picture_rectangle.size(), pixels_per_point, picture_size);
            engine.update(ui.ctx(), content, render_size, filter, text_renderer, clock);
            engine.paint(&ui.painter().with_clip_rect(picture_rectangle), picture_rectangle);
            layers::Drawing::ByTheEngine
        }
    };
    layers::show(
        ui,
        content,
        PictureOnScreen::new(picture_rectangle, picture_size.0, picture_size.1).shown_as(filter),
        panel_rectangle,
        drawing,
        commands,
    );
    zooming::show_the_button_that_makes_the_picture_fit(ui, view, panel_rectangle);
}

/// The largest rectangle of the shape `width` by `height` that fits inside `available`, centred.
fn fit_inside(available: Rect, width: u32, height: u32) -> Rect {
    let shape = width.max(1) as f32 / height.max(1) as f32;
    let (fitted_width, fitted_height) = if available.width() / available.height().max(1.0) > shape {
        (available.height() * shape, available.height())
    } else {
        (available.width(), available.width() / shape)
    };
    Rect::from_center_size(available.center(), vec2(fitted_width.max(1.0), fitted_height.max(1.0)))
}

#[cfg(test)]
mod tests {
    use egui::pos2;

    use super::*;

    fn rectangle(width: f32, height: f32) -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(width, height))
    }

    #[test]
    fn a_wide_video_in_a_tall_space_fills_the_width() {
        let fitted = fit_inside(rectangle(800.0, 800.0), 1600, 900);
        assert!((fitted.width() - 800.0).abs() < 1e-3);
        assert!((fitted.height() - 450.0).abs() < 1e-3);
        assert_eq!(fitted.center(), pos2(400.0, 400.0));
    }

    #[test]
    fn a_wide_video_in_a_wide_space_fills_the_height() {
        let fitted = fit_inside(rectangle(2000.0, 450.0), 1600, 900);
        assert!((fitted.height() - 450.0).abs() < 1e-3);
        assert!((fitted.width() - 800.0).abs() < 1e-3);
        assert_eq!(fitted.center(), pos2(1000.0, 225.0));
    }

    #[test]
    fn a_tall_video_is_fitted_by_its_height() {
        let fitted = fit_inside(rectangle(1000.0, 600.0), 1080, 1920);
        assert!((fitted.height() - 600.0).abs() < 1e-3);
        assert!((fitted.width() - 337.5).abs() < 1e-2);
    }

    #[test]
    fn the_fitted_rectangle_has_the_shape_of_the_video_and_stays_inside_the_space() {
        for (space_width, space_height) in [(800.0, 600.0), (300.0, 900.0), (1920.0, 200.0)] {
            for (width, height) in [(1920, 1080), (1080, 1920), (640, 640), (2560, 1080)] {
                let space = rectangle(space_width, space_height);
                let fitted = fit_inside(space, width, height);
                let video_shape = width as f32 / height as f32;
                assert!((fitted.width() / fitted.height() - video_shape).abs() < 0.01);
                assert!(fitted.width() <= space_width + 1e-3 && fitted.height() <= space_height + 1e-3);
            }
        }
    }

    #[test]
    fn a_space_with_no_room_still_gives_a_usable_rectangle() {
        let fitted = fit_inside(rectangle(0.0, 0.0), 1920, 1080);
        assert!(fitted.width() >= 1.0 && fitted.height() >= 1.0);
    }
}
