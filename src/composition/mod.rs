// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Working out, from a project, what the finished video is: the picture at every frame, and the
//! sound.
//!
//! The live preview and the export both make their pictures and their sound from these plans. That
//! is what makes the preview show and play exactly what the export will contain: there is only
//! one place where the tracks are put together, and it is plain logic with no files or processes
//! involved, so it can be tested thoroughly against a simple frame-by-frame check.

mod pictures;
mod sound;

pub use pictures::{
    EffectClock, GroupSpan, Layer, LayerContent, LayerEffect, PicturePlan, PictureSection, SpanOwner, VisibleVideo,
    operations_of_effects, plan_pictures,
};
pub use sound::{
    DecibelsAt, Loudness, SOUND_SAMPLES_A_SECOND, SoundBus, SoundClip, SoundGroup, SoundItem, SoundPlan, SoundStrip,
    frames_that_reach, hash_the_sound_effects_into, plan_sound, tail_samples_of, tail_samples_of_a_chain,
};

#[cfg(test)]
pub(crate) mod test_projects;
