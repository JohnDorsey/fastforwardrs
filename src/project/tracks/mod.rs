// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tracks and the clips on them: the part of a project that says what is shown and heard, and
//! when.
//!
//! A project has several tracks, in front-to-back order. Each track is a row of clips, which are
//! pieces of videos, pieces of sound files, pictures and text objects, with a title, a volume, a speed and a
//! mute switch of its own. All of the arithmetic that says which clip is at which frame lives in
//! [`Track`]; everything here is plain data and logic, which can be tested quickly and thoroughly.

mod adjustment_clip;
mod clip;
mod crate_clip;
mod drawer;
mod gain;
mod ids;
mod image_clip;
mod keyframe_fitting;
mod media_clip;
mod picture_fit;
mod placement;
mod rack;
mod text_object;
mod track;
mod track_height;

pub use adjustment_clip::{AdjustmentClip, EmptyAdjustmentClip};
pub use clip::{Clip, ClipKind};
pub use crate_clip::{CrateClip, EmptyCrate};
pub use drawer::Drawer;
pub use gain::Gain;
pub use ids::{ClipId, KeyframeId, RackId, TrackId};
pub use image_clip::{EmptyImageClip, ImageClip};
pub use media_clip::{EmptyClip, MediaClip};
pub use picture_fit::{PictureFit, fit_inside_picture, fitted_placement};
pub use placement::{Angle, CanvasArea, Corner, PictureDimensions, PixelBox, Placement, PointOnThePicture};
pub use rack::{Rack, RackEditError, RackKeyframe};
pub use text_object::{EmptyTextClip, FontSize, TextClip, TextColour, TextObject};
pub use track::{
    ClipEditError, ClipPlacement, ClipToArrange, CutEdges, InsertError, RackCut, Room, SplitError, SplitOutcome, Track,
    TrackEntry,
};
pub use track_height::TrackHeight;
