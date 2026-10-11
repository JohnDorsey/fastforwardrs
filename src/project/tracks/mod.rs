//! Tracks and the clips on them: the part of a project that says what is shown and heard, and
//! when.
//!
//! A project has several tracks, in front-to-back order. Each track is a row of clips, which are
//! pieces of videos, pieces of sound files and text objects, with a title, a volume, a speed and a
//! mute switch of its own. All of the arithmetic that says which clip is at which frame lives in
//! [`Track`]; everything here is plain data and logic, which can be tested quickly and thoroughly.

mod clip;
mod gain;
mod ids;
mod media_clip;
mod text_object;
mod track;

pub use clip::{Clip, ClipKind};
pub use gain::Gain;
pub use ids::{ClipId, ClipLocation, TrackId};
pub use media_clip::{EmptyClip, MediaClip};
pub use text_object::{CanvasArea, Corner, EmptyTextClip, FontSize, TextClip, TextColour, TextObject};
pub use track::{ClipEditError, ClipPlacement, InsertError, Room, SplitError, SplitOutcome, Track, TrackEntry};
