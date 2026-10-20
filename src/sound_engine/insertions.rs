// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The places in the graph where effects are put (pieces 7.5 and 7.6 fill them; the graph and its tests are made for them now, so that where they go is settled before
//! there is any plugin to put in them).

use super::feed::ClipRequest;

/// Something that changes sound a block at a time, in place: an effect.
pub trait BlockProcessor: Send {
    /// Changes `left` and `right` (as long as each other), which are the next samples after the ones it was given before.
    fn process(&mut self, left: &mut [f32], right: &mut [f32]);

    /// Forgets what it has heard (a seek).
    fn reset(&mut self);

    /// How many samples later than the sound that goes in the sound that comes out is, in samples at the rate of the sound engine (a limiter that looks ahead). **The graph
    /// makes up for it for the effects of a clip**: it gives the effects of a clip that many samples more than the clip has before the first sample is heard, and the sound that
    /// comes out of them is where the picture is. 0 for an effect that does not delay the sound.
    fn latency(&self) -> usize {
        0
    }
}

/// What gives the graph the effects of a clip and of a track. **A clip's are applied to its sound before its volume**, the way an insert comes before the fader of a
/// track; **a track's are applied to the sound of the track after its clips are put together** and before it is added to the others.
pub trait Insertions {
    fn for_clip(&mut self, request: &ClipRequest<'_>) -> Vec<Box<dyn BlockProcessor>>;
    fn for_strip(&mut self, strip: usize) -> Vec<Box<dyn BlockProcessor>>;
}

/// No effects at all.
pub struct NoInsertions;

impl Insertions for NoInsertions {
    fn for_clip(&mut self, _request: &ClipRequest<'_>) -> Vec<Box<dyn BlockProcessor>> {
        Vec::new()
    }

    fn for_strip(&mut self, _strip: usize) -> Vec<Box<dyn BlockProcessor>> {
        Vec::new()
    }
}
