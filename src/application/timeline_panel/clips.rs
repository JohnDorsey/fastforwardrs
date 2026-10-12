//! Drawing the clips of a track. Each clip has its own corners rounded a little, and is drawn
//! according to what it is made of:
//!
//! * a video clip is a row of pictures taken from the video, one for every second of the finished
//!   video, side by side with no gaps;
//! * a sound clip is the waveform of the sound, dark blue on pale violet;
//! * a text clip is what the text says, on a teal background;
//! * a picture clip is the picture, once, and the name of its file, on a violet background;
//! * a clip whose file cannot be found is a dull red, with the name of the file.
//!
//! The selected clip has a bright outline.

use egui::epaint::RectShape;
use egui::epaint::tessellator::Path;
use egui::{
    Align2, Color32, CornerRadius, FontId, Mesh, Painter, Rect, Shape, Stroke, StrokeKind, TextureId, Ui, pos2, remap,
    vec2,
};

use super::bevel::{self, bevel_lines};
use super::clip_shape::ClipShape;
use super::strip_view::StripView;
use super::{SourceLook, TimelineContent};
use crate::application::textures::Textures;
use crate::application::theme;
use crate::application::timeline_geometry::{TileLayout, WaveformColumn, tile_width_for};
use crate::media::ThumbnailLibrary;
use crate::project::{Clip, ClipPlacement, SourceMediaIdentifier, Track};

/// The part of a picture a tile shows: from the left edge to this fraction of the picture's width.
fn picture_part_up_to(fraction_of_width: f32) -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(fraction_of_width, 1.0))
}

/// How far a piece that is cut to the outline of its clip is made to overlap the pieces beside it, in
/// points, so that no seam shows between a piece with a soft edge along the curve of a corner and its
/// neighbour. (The neighbour is drawn after it, over the overlap.)
const OVERLAP_OF_NEIGHBOURING_PIECES: f32 = 1.0;

/// What a piece of a clip is filled with.
#[derive(Clone, Copy)]
enum PieceFill {
    Colour(Color32),
    /// A picture of which the piece shows `shown` (as fractions of its width and height, from the left
    /// and the top).
    Picture {
        texture: TextureId,
        shown: Rect,
    },
}

/// Draws a piece of a clip (one picture of a video, say) in `piece`, cut to the outline of the clip
/// (`shape`): what would be outside the curve of a corner is not drawn.
fn draw_piece_of_clip(painter: &Painter, shape: &ClipShape, piece: Rect, fill: PieceFill) {
    if shape.contains_all_of(piece) {
        let rectangle = match fill {
            PieceFill::Colour(colour) => RectShape::filled(piece, CornerRadius::ZERO, colour),
            PieceFill::Picture { texture, shown } => {
                RectShape::filled(piece, CornerRadius::ZERO, Color32::WHITE).with_texture(texture, shown)
            }
        };
        painter.add(rectangle);
        return;
    }
    let points = shape.part_of(piece.expand2(vec2(OVERLAP_OF_NEIGHBOURING_PIECES, 0.0)));
    if points.len() < 3 {
        return;
    }
    let feathering = 1.0 / painter.ctx().pixels_per_point();
    let mut path = Path::default();
    path.add_line_loop(&points);
    let mut mesh = Mesh::default();
    match fill {
        PieceFill::Colour(colour) => path.fill(feathering, colour, &mut mesh),
        PieceFill::Picture { texture, shown } => path.fill_with_uv(
            feathering,
            Color32::WHITE,
            texture,
            |position| {
                pos2(
                    remap(position.x, piece.x_range(), shown.x_range()),
                    remap(position.y, piece.y_range(), shown.y_range()),
                )
            },
            &mut mesh,
        ),
    }
    painter.add(Shape::mesh(mesh));
}

/// How loud a bar of the waveform is drawn: the quiet parts are made to stand out a little, so that
/// a soft recording still shows its shape. (Silence stays silent.)
fn displayed_height_of(loudness: f32) -> f32 {
    loudness.clamp(0.0, 1.0).powf(0.6)
}

/// How tall the bar of the waveform for `column` is drawn, in points, on a clip that is
/// `clip_height` tall: a bar is never less than a point tall unless the sound is silent.
fn waveform_bar_height(column: &WaveformColumn, clip_height: f32) -> f32 {
    if column.loudness <= 0.0 {
        return 0.0;
    }
    let most = clip_height - 2.0 * WAVEFORM_MARGIN;
    (displayed_height_of(column.loudness) * most).max(1.0)
}

/// How far the waveform keeps from the top and bottom of its clip, in points.
const WAVEFORM_MARGIN: f32 = 8.0;

/// One clip, as it is drawn: the view of the strip it is on, where it is on the screen, and its
/// outline, which every piece of it is cut to.
#[derive(Clone, Copy)]
struct ClipToDraw<'a> {
    view: &'a StripView,
    rectangles: ClipRectangles,
    shape: ClipShape,
}

/// Where on the screen a clip is, as far as it can be seen: its whole extent along the strip, and
/// the same cut to the strip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ClipRectangles {
    /// The whole clip, which may stretch far beyond the strip on either side.
    pub whole: Rect,
    /// The part of it that is in the strip.
    pub visible: Rect,
}

/// Where the clip of `placement` is on the screen, seen through `view` (the view of the strip of
/// the clip's track).
pub(super) fn rectangles_of(placement: &ClipPlacement<'_>, view: &StripView) -> ClipRectangles {
    let left = view.screen_x_of_frame(placement.first_timeline_frame.value());
    let right = view.screen_x_of_frame(placement.end_timeline_frame.value());
    let whole = Rect::from_min_max(
        pos2(left, view.rectangle.top()),
        pos2(right.max(left), view.rectangle.bottom()),
    );
    ClipRectangles {
        whole,
        visible: whole.intersect(view.rectangle),
    }
}

/// Fills the strip's background, which shows through between the clips, and draws the line along
/// its top and its bottom. The strip is a view onto a track that goes on beyond what is in the
/// window at either end, so it has square corners and no line down its sides: the only rounded
/// corners are those of the clips, at the places where they really begin and end.
pub(super) fn draw_background(painter: &Painter, strip_rectangle: Rect) {
    painter.rect_filled(strip_rectangle, 0.0, theme::TRACK_BACKGROUND);
    let border = Stroke::new(1.0, theme::TRACK_BORDER);
    for y in [strip_rectangle.top() + 0.5, strip_rectangle.bottom() - 0.5] {
        painter.hline(strip_rectangle.x_range(), y, border);
    }
}

/// Draws the clips of `track` that are in view of `view` (the view of its strip).
pub(super) fn draw_clips(
    ui: &Ui,
    painter: &Painter,
    track: &Track,
    content: &TimelineContent<'_>,
    view: &StripView,
    textures: &mut Textures,
) {
    // A picture is as wide as one of the video is on a row as tall as the strip, which is not
    // stretched nor cropped however tall or short the track is made.
    let row_height = view.rectangle.height();
    let tile_width_of = |source| {
        content.source_looks.get(&source).map_or(0.0, |look| {
            let (width, height) = look.picture_size_in_pixels;
            tile_width_for(row_height, width, height)
        })
    };
    let tiles = view
        .geometry
        .visible_tiles(track, tile_width_of, view.scroll_offset, view.width());
    for placement in track.placements() {
        let rectangles = rectangles_of(&placement, view);
        if rectangles.visible.width() <= 0.0 {
            continue;
        }
        let corner_radius = CornerRadius::same(theme::CLIP_CORNER_RADIUS);
        let clip = ClipToDraw {
            view,
            rectangles,
            shape: ClipShape::new(rectangles.whole, f32::from(theme::CLIP_CORNER_RADIUS)),
        };
        match placement.clip {
            Clip::Video(media) => {
                let look = content.source_looks.get(&media.source());
                match look {
                    Some(look) if look.is_available => match look.thumbnails {
                        Some(thumbnails) => draw_video_clip(
                            ui,
                            painter,
                            clip,
                            tiles.iter().filter(|tile| tile.clip == placement.id),
                            media.source(),
                            thumbnails,
                            textures,
                        ),
                        None => {
                            painter.rect_filled(rectangles.whole, corner_radius, theme::THUMBNAIL_PLACEHOLDER);
                        }
                    },
                    other => draw_missing_clip(painter, rectangles, corner_radius, other),
                }
            }
            Clip::Audio(media) => match content.source_looks.get(&media.source()) {
                Some(look) if look.is_available => {
                    painter.rect_filled(rectangles.whole, corner_radius, theme::SOUND_CLIP_BACKGROUND);
                    if let Some(waveform) = look.waveform {
                        let columns = view.geometry.waveform_columns(
                            &placement,
                            waveform,
                            view.scroll_offset,
                            view.width(),
                            theme::WAVEFORM_COLUMN_WIDTH,
                        );
                        draw_waveform(painter, clip, &columns);
                    }
                }
                other => draw_missing_clip(painter, rectangles, corner_radius, other),
            },
            Clip::Image(image) => match content.source_looks.get(&image.source()) {
                Some(look) if look.is_available => {
                    painter.rect_filled(rectangles.whole, corner_radius, theme::IMAGE_CLIP_BACKGROUND);
                    let picture_width =
                        draw_picture_of_a_picture_clip(ui, painter, clip, image.source(), look, textures);
                    draw_label_after(
                        painter,
                        rectangles,
                        picture_width,
                        &look.file_name,
                        theme::IMAGE_CLIP_TEXT,
                    );
                }
                other => draw_missing_clip(painter, rectangles, corner_radius, other),
            },
            Clip::Text(text) => {
                painter.rect_filled(rectangles.whole, corner_radius, theme::TEXT_CLIP_BACKGROUND);
                draw_label(painter, rectangles, &text.object.text, theme::TEXT_CLIP_TEXT);
            }
        }
        draw_bevel(painter, rectangles, view);
    }
}

/// Gives a clip its raised look: light along its top and left edges, shade along its bottom and
/// right edges (see [`bevel`]), so that its edges show whatever the colour of what it is made of.
fn draw_bevel(painter: &Painter, rectangles: ClipRectangles, view: &StripView) {
    let lines = bevel_lines(
        rectangles.whole,
        f32::from(theme::CLIP_CORNER_RADIUS),
        view.rectangle.left(),
        view.rectangle.right(),
    );
    for line in lines {
        for run in line.runs {
            painter.add(Shape::line(run, Stroke::new(bevel::LAYER_WIDTH, line.colour)));
        }
    }
}

/// Draws the picture of a picture clip once, at the left end of the clip (or of what can be seen of
/// it), as wide as the picture is on a row as tall as the strip (and no wider than the clip). Returns
/// how wide it is, which is nothing until the picture is decoded.
fn draw_picture_of_a_picture_clip(
    ui: &Ui,
    painter: &Painter,
    clip: ClipToDraw<'_>,
    source: SourceMediaIdentifier,
    look: &SourceLook<'_>,
    textures: &mut Textures,
) -> f32 {
    let ClipToDraw {
        view,
        rectangles,
        shape,
    } = clip;
    let Some(picture) = look.picture else {
        return 0.0;
    };
    let (width, height) = look.picture_size_in_pixels;
    let picture_width = tile_width_for(view.rectangle.height(), width, height) as f32;
    let tile = Rect::from_min_size(
        pos2(rectangles.visible.left(), view.rectangle.top()),
        vec2(picture_width, view.rectangle.height()),
    )
    .intersect(rectangles.visible);
    if tile.width() > 0.0 {
        let shown = if picture_width > 0.0 {
            tile.width() / picture_width
        } else {
            1.0
        };
        let texture = textures.image_picture(ui.ctx(), source, picture);
        draw_piece_of_clip(
            painter,
            &shape,
            tile,
            PieceFill::Picture {
                texture: texture.id(),
                shown: picture_part_up_to(shown),
            },
        );
    }
    tile.width()
}

/// Draws the pictures of one video clip, which are the `tiles` it has in view of `view`.
fn draw_video_clip<'a>(
    ui: &Ui,
    painter: &Painter,
    clip: ClipToDraw<'_>,
    tiles: impl Iterator<Item = &'a TileLayout>,
    source: SourceMediaIdentifier,
    thumbnails: &ThumbnailLibrary,
    textures: &mut Textures,
) {
    let ClipToDraw { view, shape, .. } = clip;
    for tile in tiles {
        let tile_rectangle = Rect::from_min_size(
            pos2(view.screen_x_of(tile.left), view.rectangle.top()),
            vec2(tile.width as f32, view.rectangle.height()),
        );
        let fill = match textures.thumbnail(ui.ctx(), source, thumbnails, tile.picture) {
            Some(texture) => PieceFill::Picture {
                texture: texture.id(),
                shown: picture_part_up_to(tile.fraction_of_picture_shown),
            },
            None => PieceFill::Colour(theme::THUMBNAIL_PLACEHOLDER),
        };
        draw_piece_of_clip(painter, &shape, tile_rectangle, fill);
    }
}

fn draw_missing_clip(
    painter: &Painter,
    rectangles: ClipRectangles,
    corner_radius: CornerRadius,
    look: Option<&SourceLook<'_>>,
) {
    painter.rect_filled(rectangles.whole, corner_radius, theme::MISSING_CLIP_BACKGROUND);
    let name = look.map_or("a file", |look| look.file_name.as_str());
    draw_label(
        painter,
        rectangles,
        &format!("{name} (not found)"),
        theme::MISSING_CLIP_TEXT,
    );
}

/// Draws the bars of a waveform, each as tall as the sound is loud, centred on the middle of the
/// clip, side by side so that together they are one solid shape. (They are one mesh of rectangles,
/// which is quick to draw however many there are, and has no faint seams between them.)
fn draw_waveform(painter: &Painter, clip: ClipToDraw<'_>, columns: &[WaveformColumn]) {
    let ClipToDraw {
        view,
        rectangles,
        shape,
    } = clip;
    let middle = rectangles.whole.center().y;
    let clip_height = rectangles.whole.height();
    let mut mesh = Mesh::default();
    for column in columns {
        let height = waveform_bar_height(column, clip_height);
        if height <= 0.0 {
            continue;
        }
        let left = view.screen_x_of(column.left);
        let bar = Rect::from_min_max(
            pos2(left, middle - height / 2.0),
            pos2(left + column.width as f32, middle + height / 2.0),
        );
        if shape.contains_all_of(bar) {
            mesh.add_colored_rect(bar, theme::SOUND_CLIP_WAVEFORM);
        } else {
            // A bar near the end of the clip is cut by the curve of the corners. (Its edges are not
            // softened: the bars are narrow and side by side, and the softened edges would show as seams.)
            add_polygon(&mut mesh, &shape.part_of(bar), theme::SOUND_CLIP_WAVEFORM);
        }
    }
    painter.with_clip_rect(rectangles.visible).add(Shape::mesh(mesh));
}

/// Adds the convex polygon with these `points` (in order) to `mesh`, all of one colour.
fn add_polygon(mesh: &mut Mesh, points: &[egui::Pos2], colour: Color32) {
    if points.len() < 3 {
        return;
    }
    let first = mesh.vertices.len() as u32;
    for point in points {
        mesh.colored_vertex(*point, colour);
    }
    for index in 1..points.len() as u32 - 1 {
        mesh.add_triangle(first, first + index, first + index + 1);
    }
}

/// Writes `text` (its first line) inside the clip, at its left edge, or at the left edge of the
/// strip if the clip's start is scrolled out of view.
fn draw_label(painter: &Painter, rectangles: ClipRectangles, text: &str, colour: Color32) {
    let first_line = text.lines().next().unwrap_or("");
    let painter = painter.with_clip_rect(rectangles.visible);
    painter.text(
        pos2(rectangles.visible.left() + 12.0, rectangles.visible.center().y),
        Align2::LEFT_CENTER,
        first_line,
        FontId::proportional(14.0),
        colour,
    );
}

/// Like [`draw_label`], but starting `space_taken` points in from the left edge of what can be seen
/// of the clip: after what is already drawn there.
fn draw_label_after(painter: &Painter, rectangles: ClipRectangles, space_taken: f32, text: &str, colour: Color32) {
    let painter = painter.with_clip_rect(rectangles.visible);
    painter.text(
        pos2(
            rectangles.visible.left() + space_taken + 12.0,
            rectangles.visible.center().y,
        ),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(14.0),
        colour,
    );
}

/// Draws the outline of the selected clip.
pub(super) fn draw_selection_outline(painter: &Painter, rectangles: ClipRectangles) {
    painter.with_clip_rect(rectangles.visible.expand(2.0)).rect_stroke(
        rectangles.whole,
        CornerRadius::same(theme::CLIP_CORNER_RADIUS),
        Stroke::new(3.0, theme::SELECTION_OUTLINE),
        StrokeKind::Inside,
    );
}

/// Draws a thin line where each clip that touches the one before it begins, light beside dark so
/// that it shows up against any picture. The lines stop short of the top and bottom by the radius
/// of the corners, so that they do not cover the notches where two clips' rounded corners meet.
pub(super) fn draw_clip_boundaries(painter: &Painter, view: &StripView, track: &Track) {
    let strip_rectangle = view.rectangle;
    for boundary in view.geometry.clip_boundaries(track) {
        let x = view.screen_x_of(boundary);
        if x < strip_rectangle.left() - 2.0 || x > strip_rectangle.right() + 2.0 {
            continue;
        }
        let top = strip_rectangle.top() + f32::from(theme::CLIP_CORNER_RADIUS);
        let bottom = strip_rectangle.bottom() - f32::from(theme::CLIP_CORNER_RADIUS);
        painter.line_segment(
            [pos2(x - 1.0, top), pos2(x - 1.0, bottom)],
            Stroke::new(1.0, theme::CLIP_BOUNDARY_LIGHT),
        );
        painter.line_segment(
            [pos2(x + 0.5, top), pos2(x + 0.5, bottom)],
            Stroke::new(2.0, theme::CLIP_BOUNDARY_DARK),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::timeline_geometry::TimelineGeometry;
    use crate::project::SourceFrameIndex;

    #[test]
    fn the_corners_of_clips_are_rounded_half_as_much_again_as_in_the_last_round() {
        // Round 2 had a radius of 6 points, round 3 three times that, and round 4 asked for about 50% more.
        assert_eq!(theme::CLIP_CORNER_RADIUS, 27);
    }

    fn column(loudness: f32) -> WaveformColumn {
        WaveformColumn {
            left: 0.0,
            width: 2.0,
            loudness,
        }
    }

    #[test]
    fn silence_has_no_bar_and_any_sound_has_one_at_least_a_point_tall() {
        assert_eq!(waveform_bar_height(&column(0.0), 104.0), 0.0);
        assert_eq!(waveform_bar_height(&column(0.0001), 104.0), 1.0);
    }

    #[test]
    fn the_loudest_sound_fills_the_clip_except_for_the_margins() {
        assert_eq!(waveform_bar_height(&column(1.0), 104.0), 104.0 - 2.0 * WAVEFORM_MARGIN);
    }

    #[test]
    fn louder_sound_is_never_drawn_shorter_and_soft_sound_is_made_to_show() {
        let heights: Vec<f32> = (0..=20)
            .map(|step| waveform_bar_height(&column(step as f32 / 20.0), 104.0))
            .collect();
        assert!(heights.windows(2).all(|pair| pair[0] <= pair[1]), "{heights:?}");
        assert!(
            heights[10] > 0.5 * heights[20],
            "half as loud shows as more than half as tall"
        );
    }

    #[test]
    fn a_clip_is_where_its_frames_are_along_the_strip_and_only_the_part_in_view_is_visible() {
        use crate::project::{MediaClip, SpeedFactor, TrackId};
        let geometry = TimelineGeometry::for_picture(crate::project::FrameRate::new(30, 1).unwrap(), 1920, 1080, 90.0);
        let media = MediaClip::new(
            SourceMediaIdentifier::new(1),
            SourceFrameIndex::new(0),
            SourceFrameIndex::new(300),
            SpeedFactor::NORMAL,
        )
        .unwrap();
        let track = Track::new(TrackId::new(0), "track 0").with_clips([(30, Clip::Video(media))]);
        let placement = track.placement_at_index(0).unwrap();
        let strip = Rect::from_min_size(pos2(100.0, 50.0), vec2(500.0, 104.0));

        let rectangles = rectangles_of(&placement, &StripView::new(geometry, 0.0, strip));
        // 160 points a second: the clip starts one second in and lasts ten.
        assert_eq!(rectangles.whole.left(), 100.0 + 160.0);
        assert_eq!(rectangles.whole.width(), 1600.0);
        assert_eq!(rectangles.visible.right(), strip.right());

        let scrolled = rectangles_of(&placement, &StripView::new(geometry, 400.0, strip));
        assert_eq!(scrolled.whole.left(), 100.0 + 160.0 - 400.0);
        assert_eq!(
            scrolled.visible.left(),
            strip.left(),
            "cut off at the left edge of the strip"
        );
    }
}
