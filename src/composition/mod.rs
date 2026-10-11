//! Working out, from a project, what the finished video is: the picture at every frame, and the
//! sound.
//!
//! The live preview and the export both make their pictures and their sound from these plans. That
//! is what makes the preview show and play exactly what the export will contain: there is only
//! one place where the tracks are put together, and it is plain logic with no files or processes
//! involved, so it can be tested thoroughly against a simple frame-by-frame check.

mod pictures;
mod sound;

pub use pictures::{Layer, LayerContent, PicturePlan, PictureSection, VisibleVideo, plan_pictures};
pub use sound::{SoundClip, SoundItem, SoundPlan, SoundStrip, plan_sound};

#[cfg(test)]
mod test_projects;
