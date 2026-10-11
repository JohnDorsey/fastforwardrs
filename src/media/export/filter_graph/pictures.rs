//! The picture half of the filter graph: the video of each section, black where there is none, the
//! still pictures laid over them, and all of it joined in order.

use std::collections::BTreeMap;

use super::{PictureGraphSpecification, VIDEO_OUTPUT_LABEL, numbered_labels, renumber_pictures_filter, statement};
use crate::media::fit_inside_picture;
use crate::project::{FrameRate, SpeedFactor};

const JOINED_VIDEO_LABEL: &str = "joined_video";

/// A source file the graph reads pictures from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceToExport {
    /// How big the picture is as it is shown (after rotation and the shape of its pixels), for a
    /// video; `None` for a file that has no pictures. A video of another shape than the finished
    /// picture is fitted inside it.
    pub display_size_in_pixels: Option<(u32, u32)>,
}

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

/// A still picture laid over the picture of a section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayToExport {
    /// Which of the graph's inputs the picture is.
    pub picture_input: usize,
    pub left_in_pixels: u32,
    pub top_in_pixels: u32,
}

/// A stretch of the finished picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionToExport {
    pub frame_count: u64,
    /// The video that is seen, or `None` for black.
    pub video: Option<VideoToExport>,
    /// The still pictures over it, the one at the back first.
    pub overlays: Vec<OverlayToExport>,
}

pub(super) fn picture_statements(specification: &PictureGraphSpecification) -> Vec<String> {
    let frame_rate = specification.frame_rate;
    let width = specification.picture_width_in_pixels;
    let height = specification.picture_height_in_pixels;
    let sections = &specification.sections;
    let mut statements = Vec::new();

    // Each source video is made the size of the picture and the frame rate of the project once, and
    // then split into a copy for every section that uses it. Scale first and resample second:
    // resampling a variable frame rate video repeats pictures to fill the gaps, and repeating an
    // already scaled picture costs nothing.
    let mut sections_using_each_source: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (section_number, section) in sections.iter().enumerate() {
        if let Some(video) = section.video {
            sections_using_each_source
                .entry(video.source_input)
                .or_default()
                .push(section_number);
        }
    }
    for (source_input, section_numbers) in &sections_using_each_source {
        let display_size = specification
            .sources
            .get(*source_input)
            .and_then(|source| source.display_size_in_pixels);
        let copy_labels: Vec<String> = section_numbers
            .iter()
            .map(|section_number| format!("clip_source_video_{section_number}"))
            .collect();
        statements.push(statement(
            &[&format!("{source_input}:v:0")],
            &format!(
                "{},fps={frame_rate}:round=near,split={}",
                fit_filters(display_size, width, height),
                section_numbers.len()
            ),
            &copy_labels,
        ));
    }

    // A still picture can only be used once as it is, so each place it is laid over gets a copy.
    let mut copies_needed: BTreeMap<usize, usize> = BTreeMap::new();
    for overlay in sections.iter().flat_map(|section| &section.overlays) {
        *copies_needed.entry(overlay.picture_input).or_default() += 1;
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
        let base_label = if section.overlays.is_empty() {
            finished_label.clone()
        } else {
            format!("clip_picture_{section_number}")
        };
        match section.video {
            Some(video) => statements.push(statement(
                &[&format!("clip_source_video_{section_number}")],
                &cut_and_retime_filters(frame_rate, &video),
                &[&base_label],
            )),
            None => statements.push(statement(
                &[],
                &format!(
                    "color=c=black:s={width}x{height}:r={frame_rate},format=yuv420p,setsar=1,trim=end_frame={},{}",
                    section.frame_count,
                    renumber_pictures_filter(frame_rate)
                ),
                &[&base_label],
            )),
        }
        let mut current_label = base_label;
        for (overlay_number, overlay) in section.overlays.iter().enumerate() {
            let copy_label = unused_copies
                .get_mut(&overlay.picture_input)
                .and_then(Vec::pop)
                .expect("a copy was made for every place a still picture is laid over");
            let is_last = overlay_number + 1 == section.overlays.len();
            let next_label = if is_last {
                finished_label.clone()
            } else {
                format!("clip_picture_{section_number}_with_{}", overlay_number + 1)
            };
            statements.push(statement(
                &[&current_label, &copy_label],
                &format!(
                    "overlay=x={}:y={}:format=auto",
                    overlay.left_in_pixels, overlay.top_in_pixels
                ),
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

/// The filters that make a source video the size of the finished picture, with square pixels. A
/// video that is shown at the size of the picture, or at another size of the same shape, is simply
/// scaled to it. One of another shape is scaled to fit inside, and put in the middle of black.
fn fit_filters(display_size: Option<(u32, u32)>, picture_width: u32, picture_height: u32) -> String {
    let scaled_to_the_picture = format!("scale={picture_width}:{picture_height},setsar=1");
    let Some((display_width, display_height)) = display_size else {
        return scaled_to_the_picture;
    };
    let fit = fit_inside_picture(display_width, display_height, picture_width, picture_height);
    if fit.fills(picture_width, picture_height) {
        return scaled_to_the_picture;
    }
    format!(
        "scale={}:{},setsar=1,pad={picture_width}:{picture_height}:{}:{}",
        fit.width_in_pixels, fit.height_in_pixels, fit.left_in_pixels, fit.top_in_pixels
    )
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

#[cfg(test)]
mod tests;
