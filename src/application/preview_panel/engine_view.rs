// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the picture engine is asked to draw for the preview. The work is done by `frame_scene`, which the export uses too; this is the window's way into it, with the
//! colour that the window draws a layer without a picture in ([`theme::TRACK_BACKGROUND`]).

use super::PreviewContent;
use crate::application::theme;
use crate::engine::FrameDescription;
#[cfg(test)]
pub use crate::frame_scene::HowToMakeIt;
pub use crate::frame_scene::{PictureSupply, PictureWanted, SentPictures};

/// What the engine is to draw for `content` on a frame `render_size` big, the layers at the back first (see `frame_scene::describe_frame`). `generation` is a number that
/// the window changes whenever it lets go of what it knows of the files it shows.
pub fn describe_frame(
    content: &PreviewContent<'_>,
    render_size: (u32, u32),
    generation: u64,
    supply: &mut impl PictureSupply,
) -> FrameDescription {
    crate::frame_scene::describe_frame(
        content,
        render_size,
        generation,
        theme::TRACK_BACKGROUND.to_srgba_unmultiplied(),
        supply,
    )
}
