// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The picture half of the filter graph: each section is black with its layers laid over it, the
//! one at the back first, and all of the sections are joined in order.
//!
//! A layer is a piece of a video or a still picture (the fast-forward symbol, a text object), and
//! it is somewhere on the picture: the box it fills, and how far that is turned about its middle.
//! A video is cut, and made to play at its speed, and only then scaled to its box (so that the
//! pictures that a sped-up clip drops are never scaled). A layer that is turned is made into a
//! picture with clear corners, the size of the smallest picture the turned box fits in, and that
//! is laid over with its middle where the box's middle is. A box can reach out past the edges of
//! the picture, which cuts off what is laid over it there.

use std::collections::BTreeMap;

use super::{
    PictureGraphSpecification, VIDEO_OUTPUT_LABEL, format_number, numbered_labels, renumber_pictures_filter, statement,
};
use crate::colour_effects::ColourMatrix;
use crate::project::{FrameRate, SpeedFactor};

const JOINED_VIDEO_LABEL: &str = "joined_video";

/// The video that is seen during a section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoToExport {
    /// Which of the graph's inputs the video is.
    pub source_input: usize,
    pub first_source_frame: u64,
    /// The frame just after the last source frame.
    pub end_source_frame: u64,
    pub speed_factor: SpeedFactor,
}

/// Where a layer goes on the finished picture, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaceToExport {
    /// The box the layer fills, before it is turned. It can reach out past the edges of the picture, so
    /// that its left and top are below zero when it starts before the picture does.
    pub left_in_pixels: i32,
    pub top_in_pixels: i32,
    pub width_in_pixels: u32,
    pub height_in_pixels: u32,
    /// How far the box is turned about its middle, clockwise.
    pub turn_in_degrees: f32,
}

impl PlaceToExport {
    fn is_turned(&self) -> bool {
        self.turn_in_degrees != 0.0
    }

    /// Whether all of the box (before it is turned) is on a picture `picture_width` by `picture_height`
    /// pixels: touching its edges is, reaching past them is not.
    fn is_inside_the_picture(&self, picture_width: u32, picture_height: u32) -> bool {
        self.left_in_pixels >= 0
            && self.top_in_pixels >= 0
            && i64::from(self.left_in_pixels) + i64::from(self.width_in_pixels) <= i64::from(picture_width)
            && i64::from(self.top_in_pixels) + i64::from(self.height_in_pixels) <= i64::from(picture_height)
    }
}

/// What a layer is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerContentToExport {
    /// A piece of a video, which is scaled to the box of the layer.
    Video(VideoToExport),
    /// A still picture: a picture that was made (the fast-forward symbol, text), which is the size
    /// of the box of the layer already, or a picture from a file, which is scaled to it.
    Still {
        /// Which of the graph's inputs the picture is.
        picture_input: usize,
        scaled_to_its_place: bool,
    },
}

/// A layer of the picture of a section.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerToExport {
    pub content: LayerContentToExport,
    pub place: PlaceToExport,
    /// How the colours of its picture are changed by its effects, once it is scaled to its place and before it is turned.
    pub colour: ColourMatrix,
}

/// A stretch of the finished picture.
#[derive(Clone, Debug, PartialEq)]
pub struct SectionToExport {
    pub frame_count: u64,
    /// What is seen over black, the layer at the back first. None at all is black.
    pub layers: Vec<LayerToExport>,
}

pub(super) fn picture_statements(specification: &PictureGraphSpecification) -> Vec<String> {
    let frame_rate = specification.frame_rate;
    let width = specification.picture_width_in_pixels;
    let height = specification.picture_height_in_pixels;
    let sections = &specification.sections;
    let mut statements = Vec::new();

    // Each source video is resampled to the frame rate of the project once, and then split into a
    // copy for every layer that uses it. Resampling a variable frame rate video repeats pictures to
    // fill the gaps, and a picture that is repeated costs nothing until it is scaled, which is done
    // later, to only the pictures that are kept.
    let mut layers_using_each_source: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
    for (section_number, section) in sections.iter().enumerate() {
        for (layer_number, layer) in section.layers.iter().enumerate() {
            if let LayerContentToExport::Video(video) = layer.content {
                layers_using_each_source
                    .entry(video.source_input)
                    .or_default()
                    .push((section_number, layer_number));
            }
        }
    }
    for (source_input, layers) in &layers_using_each_source {
        let copy_labels: Vec<String> = layers
            .iter()
            .map(|(section_number, layer_number)| format!("clip_source_video_{section_number}_{layer_number}"))
            .collect();
        statements.push(statement(
            &[&format!("{source_input}:v:0")],
            &format!("fps={frame_rate}:round=near,split={}", layers.len()),
            &copy_labels,
        ));
    }

    // A still picture can only be used once as it is, so each place it is laid over gets a copy.
    let mut copies_needed: BTreeMap<usize, usize> = BTreeMap::new();
    for layer in sections.iter().flat_map(|section| &section.layers) {
        if let LayerContentToExport::Still { picture_input, .. } = layer.content {
            *copies_needed.entry(picture_input).or_default() += 1;
        }
    }
    let mut unused_copies: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (picture_input, count) in copies_needed {
        let mut labels = numbered_labels(&format!("still_picture_{picture_input}_copy"), count);
        statements.push(statement(
            &[&format!("{picture_input}:v")],
            &format!("split={count}"),
            &labels,
        ));
        labels.reverse();
        unused_copies.insert(picture_input, labels);
    }

    let mut finished_section_labels = Vec::with_capacity(sections.len());
    for (section_number, section) in sections.iter().enumerate() {
        let finished_label = format!("finished_clip_{section_number}");
        let layers = &section.layers;

        // The picture to lay the layers over: a video at the back that is not turned and is inside
        // the picture is that picture itself (put on black where it does not reach), and otherwise
        // it is black (a box that reaches out of the picture cannot be put on black by padding).
        let back_video = layers.first().and_then(|first| match first.content {
            LayerContentToExport::Video(video)
                if !first.place.is_turned() && first.place.is_inside_the_picture(width, height) =>
            {
                Some((video, first.place, first.colour))
            }
            _ => None,
        });
        let (mut current_label, first_layer_to_lay_over) = match back_video {
            Some((video, place, colour)) => {
                let label = if layers.len() == 1 {
                    finished_label.clone()
                } else {
                    format!("clip_picture_{section_number}")
                };
                statements.push(statement(
                    &[&format!("clip_source_video_{section_number}_0")],
                    &format!(
                        "{},{}",
                        cut_and_retime_filters(frame_rate, &video),
                        fit_to_place_filters(place, colour, width, height)
                    ),
                    &[&label],
                ));
                (label, 1)
            }
            None => {
                let label = if layers.is_empty() {
                    finished_label.clone()
                } else {
                    format!("clip_picture_{section_number}")
                };
                statements.push(statement(
                    &[],
                    &format!(
                        "color=c=black:s={width}x{height}:r={frame_rate},format=yuv420p,setsar=1,trim=end_frame={},{}",
                        section.frame_count,
                        renumber_pictures_filter(frame_rate)
                    ),
                    &[&label],
                ));
                (label, 0)
            }
        };

        for (layer_number, layer) in layers.iter().enumerate().skip(first_layer_to_lay_over) {
            let layer_label = match layer.content {
                LayerContentToExport::Video(video) => {
                    let label = format!("clip_layer_{section_number}_{layer_number}");
                    let mut filters = format!(
                        "{},scale={}:{},setsar=1",
                        cut_and_retime_filters(frame_rate, &video),
                        layer.place.width_in_pixels,
                        layer.place.height_in_pixels
                    );
                    if !layer.colour.changes_nothing() {
                        filters.push_str(&format!(",{}", layer.colour.ffmpeg_filter()));
                    }
                    if layer.place.is_turned() {
                        filters.push_str(&format!(",{}", turn_filters(layer.place)));
                    }
                    statements.push(statement(
                        &[&format!("clip_source_video_{section_number}_{layer_number}")],
                        &filters,
                        &[&label],
                    ));
                    label
                }
                LayerContentToExport::Still {
                    picture_input,
                    scaled_to_its_place,
                } => {
                    let copy_label = unused_copies
                        .get_mut(&picture_input)
                        .and_then(Vec::pop)
                        .expect("a copy was made for every place a still picture is laid over");
                    let mut filters = Vec::new();
                    if scaled_to_its_place {
                        filters.push(format!(
                            "scale={}:{},setsar=1",
                            layer.place.width_in_pixels, layer.place.height_in_pixels
                        ));
                    }
                    if !layer.colour.changes_nothing() {
                        filters.push(layer.colour.ffmpeg_filter());
                    }
                    if layer.place.is_turned() {
                        filters.push(turn_filters(layer.place));
                    }
                    if filters.is_empty() {
                        copy_label
                    } else {
                        let label = format!("clip_layer_{section_number}_{layer_number}");
                        statements.push(statement(&[&copy_label], &filters.join(","), &[&label]));
                        label
                    }
                }
            };
            let is_last = layer_number + 1 == layers.len();
            let next_label = if is_last {
                finished_label.clone()
            } else {
                format!("clip_picture_{section_number}_with_{layer_number}")
            };
            let (left, top) = left_and_top_when_turned(layer.place);
            statements.push(statement(
                &[&current_label, &layer_label],
                &format!("overlay=x={left}:y={top}:format=auto"),
                &[&next_label],
            ));
            current_label = next_label;
        }
        finished_section_labels.push(finished_label);
    }

    statements.push(statement(
        &finished_section_labels.iter().map(String::as_str).collect::<Vec<_>>(),
        &format!("concat=n={}:v=1:a=0", sections.len()),
        &[JOINED_VIDEO_LABEL],
    ));
    // Give every finished picture the timestamp its position calls for, exactly one frame after
    // the one before. Joining sections cannot be relied on to do this: it believes a section with
    // a single picture (a few frames sped up beyond their number) lasts no time at all, so the
    // section after it starts at the same instant, and the encoder then throws a picture away.
    statements.push(statement(
        &[JOINED_VIDEO_LABEL],
        &renumber_pictures_filter(frame_rate),
        &[VIDEO_OUTPUT_LABEL],
    ));
    statements
}

/// The filters that scale a picture to the box of `place` with square pixels, change its colours as `colour` says (if it changes
/// any), and, if the box is not all of the picture, put it in its place on black. (The box must be inside the picture.)
fn fit_to_place_filters(place: PlaceToExport, colour: ColourMatrix, picture_width: u32, picture_height: u32) -> String {
    debug_assert!(
        place.is_inside_the_picture(picture_width, picture_height),
        "{place:?} cannot be padded to {picture_width}x{picture_height}"
    );
    let mut scaled = format!("scale={}:{},setsar=1", place.width_in_pixels, place.height_in_pixels);
    if !colour.changes_nothing() {
        scaled.push_str(&format!(",{}", colour.ffmpeg_filter()));
    }
    let fills_the_picture = place.left_in_pixels == 0
        && place.top_in_pixels == 0
        && place.width_in_pixels == picture_width
        && place.height_in_pixels == picture_height;
    if fills_the_picture {
        scaled
    } else {
        format!(
            "{scaled},pad={picture_width}:{picture_height}:{}:{}",
            place.left_in_pixels, place.top_in_pixels
        )
    }
}

/// The filters that cut `video` out of the resampled source and make it play at its speed: take
/// its range of pictures, keep every `speed_factor`th one, and number them from 0 at the
/// project's frame rate.
fn cut_and_retime_filters(frame_rate: FrameRate, video: &VideoToExport) -> String {
    let mut filters = format!(
        "trim=start_frame={}:end_frame={}",
        video.first_source_frame, video.end_source_frame
    );
    if video.speed_factor.is_accelerated() {
        // `n` counts the pictures that reach `select`, from 0: the section's own first picture.
        filters.push_str(&format!(",select='not(mod(n,{}))'", video.speed_factor.value()));
    }
    filters.push_str(&format!(",{}", renumber_pictures_filter(frame_rate)));
    filters
}

/// The filters that turn a picture the size of `place`'s box about its middle, by the angle of
/// `place`, into a picture just big enough to hold it, with what is not the picture clear.
fn turn_filters(place: PlaceToExport) -> String {
    let (width, height) = size_when_turned(place.width_in_pixels, place.height_in_pixels, place.turn_in_degrees);
    let radians = f64::from(place.turn_in_degrees).to_radians();
    format!(
        "format=rgba,rotate=a={}:ow={width}:oh={height}:c=black@0",
        format_number(radians)
    )
}

/// How big the picture is that a picture `width` by `height` pixels, turned by `degrees`, fits in:
/// in whole pixels, and even ones so that the middle stays where it was.
fn size_when_turned(width: u32, height: u32, degrees: f32) -> (u32, u32) {
    let (sine, cosine) = f64::from(degrees).to_radians().sin_cos();
    let (width, height) = (f64::from(width), f64::from(height));
    // (A turn that is a whole quarter must not come out a pixel too big because of a rounding error.)
    let whole_even_pixels = |size: f64| {
        let whole = (size - 1e-6).ceil().max(1.0) as u32;
        whole + whole % 2
    };
    (
        whole_even_pixels(width * cosine.abs() + height * sine.abs()),
        whole_even_pixels(width * sine.abs() + height * cosine.abs()),
    )
}

/// Where the top left corner of the picture that [`turn_filters`] makes goes, so that its middle
/// is where the middle of the box of `place` is. (The box itself, if it is not turned.) It can be
/// outside the picture, when what is turned reaches past the edge.
fn left_and_top_when_turned(place: PlaceToExport) -> (i32, i32) {
    let left = i64::from(place.left_in_pixels);
    let top = i64::from(place.top_in_pixels);
    if !place.is_turned() {
        return (left as i32, top as i32);
    }
    let (width, height) = size_when_turned(place.width_in_pixels, place.height_in_pixels, place.turn_in_degrees);
    let middle_x = left as f64 + f64::from(place.width_in_pixels) / 2.0;
    let middle_y = top as f64 + f64::from(place.height_in_pixels) / 2.0;
    (
        (middle_x - f64::from(width) / 2.0).round() as i32,
        (middle_y - f64::from(height) / 2.0).round() as i32,
    )
}

#[cfg(test)]
mod tests;
