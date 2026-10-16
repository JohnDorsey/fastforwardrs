// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! How the colours of one video are read (phase 3, piece 3.7): from what its file says, what the user says of it, and whether the pictures are wanted in planes. **The
//! preview's supplier and the export both ask this one function**, so that they cannot differ about what the numbers of a video stand for, or about what ffmpeg is told when it
//! makes the colours itself.

use crate::colour_space::{ChromaSiting, VideoRange};
use crate::engine::PictureColour;
use crate::media::{HowToReadColours, NativeYuv420p, VideoStreamFormat};
use crate::project::InputColourOverride;

/// What is decided about reading the colours of one video.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoReading {
    /// What the planes stand for, if the pictures are to be delivered in planes (`None`: they are delivered as RGBA, made by ffmpeg).
    pub planes: Option<NativeYuv420p>,
    /// What the codes of the red, green and blue of the pictures stand for (the primaries and the curve), whichever way they are delivered.
    pub picture_colour: PictureColour,
    /// What ffmpeg is told about the matrix and the range when it makes the colours (`None` while the format of the video is not known, and for pictures in planes).
    pub ffmpeg_is_told: Option<HowToReadColours>,
    /// Where the colour differences of the pictures sit among the pixels, for pictures in planes (the usual place while the format is not known).
    pub chroma_siting: ChromaSiting,
}

/// How the colours of a video of `format` (if that is known), whose picture is `size` pixels, are read when the user says `said` of them. `planes_wanted` is whether the
/// pictures of a video that is stored in a way the engine can take directly (8 bits to a number, 4:2:0, a matrix that is known) are wanted in planes.
pub fn reading_of_a_video(
    format: Option<&VideoStreamFormat>,
    (width, height): (u32, u32),
    said: &InputColourOverride,
    planes_wanted: bool,
) -> VideoReading {
    let planes = format
        .and_then(|format| format.as_native_yuv420p_with(width, height, said))
        .filter(|_| planes_wanted);
    // (A video whose format is not known yet says nothing, which is sRGB; and what the user chose of the primaries and the curve is used all the same.)
    let colour = format
        .cloned()
        .unwrap_or_default()
        .colour_of_a_picture_of_size_with(width, height, said);
    let ffmpeg_is_told = (format.is_some() && planes.is_none()).then(|| HowToReadColours {
        matrix: colour.matrix,
        range: colour
            .full_range
            .map(|full| if full { VideoRange::Full } else { VideoRange::Limited }),
    });
    VideoReading {
        planes,
        picture_colour: PictureColour {
            primaries: colour.primaries,
            curve: colour.curve,
        },
        ffmpeg_is_told,
        chroma_siting: format.map_or_else(ChromaSiting::default, VideoStreamFormat::chroma_siting),
    }
}

#[cfg(test)]
mod tests;
