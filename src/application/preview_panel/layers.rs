// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The videos, pictures and text objects on the preview: drawn where they are in the finished picture,
//! each turned as far as it is turned, with the outline of the selected one and the handles at its
//! corners.
//!
//! What the mouse does to them is in [`super::gestures`]: the widgets that take the mouse (the "catchers", see
//! [`super::catchers`]) take every click and drag on the preview, and what each is about is worked out from
//! where it began. What is drawn here has no say in that, but for being what the pointer is over: the widgets
//! made here only give the layers their names for screen readers (and the tests), and cannot be clicked.
//! Everything is made in the order it is drawn in, the layer at the back first, so that what is in
//! front of the fast-forward symbol is drawn over it and what is behind it is drawn under it.
//!
//! The layers are cut off at the edge of the picture, as the export cuts them off. What shows where they are
//! and what can be taken hold of (their outlines, the handles, the ring to turn by) is not: it is drawn wherever
//! it is over the preview, outside the picture too (when the picture is zoomed out, or does not fill the panel), but it stops at
//! the edge of the preview panel and is never drawn over the other panels.

use egui::epaint::Vertex;
use egui::{
    Color32, Id, Mesh, Painter, Pos2, Rect, Sense, Shape, Stroke, TextureId, Ui, WidgetInfo, WidgetType, pos2, vec2,
};

use super::catchers::Catchers;
use super::geometry::{HANDLE_SIZE, PictureOnScreen};
use super::gestures;
use super::speed_indicator_overlay::{self, SymbolOnScreen};
use super::{PreviewContent, PreviewLayer, PreviewLayerContent, VideoInThePicture};
use crate::application::commands::Command;
use crate::application::textures::Textures;
use crate::application::theme;
use crate::media::ThumbnailKey;
use crate::project::Corner;
use crate::text_rendering::{LazyTextRenderer, TextAppearance};

/// Who draws the pictures of the layers and the symbol.
pub(super) enum Drawing<'a> {
    /// The window does, a texture and a quad for each layer.
    ByTheWindow {
        textures: &'a mut Textures,
        text_renderer: &'a mut LazyTextRenderer,
    },
    /// The picture engine has, and its picture of all of them together is on the screen already, under what is drawn here: what is drawn here is only what
    /// shows where the layers are, and what the mouse can take hold of.
    ByTheEngine,
}

/// Draws the layers (and the symbol over the front-most video) and adds a command for each thing
/// the user does to them. `panel_rectangle` is all of the room the preview has, of which the
/// picture is a part (all of the panel, or more, if the picture is zoomed in): the box of the selected layer, and the places
/// to turn it from, can be outside the picture, and are drawn and taken hold of as far as the panel goes.
pub(super) fn show(
    ui: &mut Ui,
    content: &PreviewContent<'_>,
    picture: PictureOnScreen,
    panel_rectangle: Rect,
    mut drawing: Drawing<'_>,
    commands: &mut Vec<Command>,
) {
    // What is drawn is cut off at the edge of the picture, as the export cuts it off (and at the edge of the panel, if the picture
    // reaches past it), but what is drawn to show where things are, and what can be taken hold of, is only cut off at the edge of
    // the panel: it is not drawn over the other panels.
    let painter = ui.painter().with_clip_rect(picture.rectangle);
    let painter_of_what_shows_where_things_are = ui.painter().with_clip_rect(panel_rectangle);
    let catchers = Catchers::register(
        ui,
        panel_rectangle,
        content.selected_layer().map(|layer| (layer, &picture)),
    );
    let pointing = gestures::handle(ui, &catchers, content, &picture, commands);

    for layer in &content.layers {
        if let Drawing::ByTheWindow {
            textures,
            text_renderer,
        } = &mut drawing
        {
            draw_layer(ui, &painter, layer, picture, textures, text_renderer);
        }
        name_the_layer(ui, layer, &picture);
        if pointing.outlines(content, layer) {
            painter_of_what_shows_where_things_are.add(Shape::closed_line(
                picture.outline_of(&layer.placement).to_vec(),
                Stroke::new(1.0, theme::INDICATOR_OUTLINE),
            ));
        }
        if let Some(symbol) = content.symbol
            && symbol.over == layer.clip
        {
            let on_screen = SymbolOnScreen::of(content.indicator_layout, symbol.speed, picture.rectangle);
            let textures_that_draw_the_symbol = match &mut drawing {
                Drawing::ByTheWindow { textures, .. } => Some(&mut **textures),
                Drawing::ByTheEngine => None,
            };
            speed_indicator_overlay::show(
                ui,
                &on_screen,
                symbol.speed,
                pointing.emphasises_the_symbol(),
                textures_that_draw_the_symbol,
            );
        }
    }

    // The selected layer has its outline and handles over everything else on the preview.
    if let Some(layer) = content.selected_layer() {
        show_the_box(&painter_of_what_shows_where_things_are, layer, picture);
    }
    pointing.show(ui, &painter_of_what_shows_where_things_are, content, &picture);
}

/// Gives the layer its name for screen readers (and the tests), over where it is. It senses nothing.
fn name_the_layer(ui: &Ui, layer: &PreviewLayer<'_>, picture: &PictureOnScreen) {
    ui.interact(
        picture.bounding_rectangle_of(&layer.placement),
        Id::new(("layer on the preview", layer.clip)),
        Sense::hover(),
    )
    .widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, layer.label()));
}

fn draw_layer(
    ui: &Ui,
    painter: &Painter,
    layer: &PreviewLayer<'_>,
    picture: PictureOnScreen,
    textures: &mut Textures,
    text_renderer: &mut LazyTextRenderer,
) {
    let corners = picture.outline_of(&layer.placement);
    let context = ui.ctx();
    let texture: Option<TextureId> = match &layer.content {
        PreviewLayerContent::Video(VideoInThePicture::Picture(supplied)) => Some(
            textures
                .preview_picture(context, layer.track, supplied, picture.filter, layer.colour())
                .id(),
        ),
        PreviewLayerContent::Video(VideoInThePicture::Thumbnail {
            source,
            second,
            library,
        }) => {
            let key = ThumbnailKey {
                level: 0,
                index: *second,
            };
            textures
                .thumbnail_with_colours_changed(context, *source, library, key, layer.colour())
                .map(|texture| texture.id())
        }
        PreviewLayerContent::Video(VideoInThePicture::Waiting) => {
            painter.add(Shape::convex_polygon(
                corners.to_vec(),
                theme::TRACK_BACKGROUND,
                Stroke::NONE,
            ));
            None
        }
        PreviewLayerContent::Image {
            source,
            picture: Some(decoded),
            ..
        } => Some(
            textures
                .image_picture(context, *source, decoded, picture.filter, layer.colour())
                .id(),
        ),
        PreviewLayerContent::Image { picture: None, .. } => {
            painter.add(Shape::convex_polygon(
                corners.to_vec(),
                theme::TRACK_BACKGROUND,
                Stroke::NONE,
            ));
            None
        }
        PreviewLayerContent::Solid(colour) => {
            // (The old drawing mixes the colour as it mixes the colours of a picture, and leaves the other effects out.)
            let mut pixel = [colour.red, colour.green, colour.blue, colour.alpha];
            layer.colour().apply_to_rgba_pixels(&mut pixel);
            painter.add(Shape::convex_polygon(
                corners.to_vec(),
                Color32::from_rgba_unmultiplied(pixel[0], pixel[1], pixel[2], pixel[3]),
                Stroke::NONE,
            ));
            None
        }
        PreviewLayerContent::Text(object) => {
            let (picture_width, picture_height) = picture.size_in_whole_pixels();
            let key = TextAppearance::of(object, picture_width, picture_height);
            textures
                .text_picture(context, key, picture.filter, layer.colour(), || {
                    text_renderer
                        .get_if_ready()
                        .map(|renderer| renderer.render(object, picture_width, picture_height))
                })
                .map(|texture| texture.id())
        }
    };
    if let Some(texture) = texture {
        draw_turned_picture(painter, texture, corners);
    }
}

/// Draws the whole of a picture on the quadrilateral with these corners, in the order top left, top
/// right, bottom right, bottom left.
fn draw_turned_picture(painter: &Painter, texture: TextureId, corners: [Pos2; 4]) {
    let corners_of_the_picture = [pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0)];
    let mut mesh = Mesh::with_texture(texture);
    for (position, uv) in corners.into_iter().zip(corners_of_the_picture) {
        mesh.vertices.push(Vertex {
            pos: position,
            uv,
            color: Color32::WHITE,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(Shape::mesh(mesh));
}

/// The outline of the selected layer and the square handles on its corners that resize it. (The places just
/// outside the corners that turn it are not drawn: the ring that shows one is drawn while the pointer is in it.)
fn show_the_box(painter: &Painter, layer: &PreviewLayer<'_>, picture: PictureOnScreen) {
    painter.add(Shape::closed_line(
        picture.outline_of(&layer.placement).to_vec(),
        Stroke::new(2.0, theme::SELECTION_OUTLINE),
    ));
    for corner in Corner::ALL {
        let handle = Rect::from_center_size(
            picture.corner_of(&layer.placement, corner),
            vec2(HANDLE_SIZE, HANDLE_SIZE),
        );
        painter.rect_filled(handle, 2.0, theme::SELECTION_OUTLINE);
        painter.rect_stroke(handle, 2.0, Stroke::new(1.0, Color32::BLACK), egui::StrokeKind::Inside);
    }
}
