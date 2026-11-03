// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Measuring the tails of the sound effects of a project (docs/engine-phase-15-vst3-design.md, section 16): which effects want one is `composition::the_tails_to_measure_in`, how one is
//! measured is [`PluginHost::measure_the_ring_out_of`](crate::sound_engine::PluginRunner::measure_the_ring_out_of), and **this decides when, and on which thread**.
//!
//! * **In the editor, in the background**: [`PluginHost::measure_the_tails_in_the_background`] starts one thread for the effects that want one and are not being measured, which measures
//!   them one after another and keeps each as it is done (`composition::note_the_measured_tail`); the preview makes its sound again when what is measured has changed. The window
//!   does not wait.
//! * **Before an export, and in the program that exports without a window**: [`PluginHost::measure_the_tails_now`] measures what is wanted and waits for what the thread is
//!   measuring, so that **the plan of an export is made with every tail known**: an export is the same sound whether or not the window had the time.
//! * **A plugin that could not be measured is tried again only when what may be run has changed** (a folder that was approved), and not at every frame: the guess of the scan is
//!   used for it, as it was.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant};

use super::PluginHost;
use crate::composition::{TailToMeasure, is_measured, note_the_measured_tail, the_tails_to_measure_in};
use crate::project::Project;
use crate::sound_engine::PluginRunner as _;

/// How long [`PluginHost::measure_the_tails_now`] waits, at the most, for what the thread of the background is measuring.
const LONGEST_WAIT_FOR_THE_BACKGROUND: Duration = Duration::from_secs(120);

/// What is being measured and what could not be.
#[derive(Default)]
pub(super) struct RingOuts {
    /// The keys of the effects that a thread is measuring now.
    in_flight: HashSet<u64>,
    /// The keys of the effects that could not be measured, with the generation of the host (what may be run) when they could not: they are not tried again until it is different.
    could_not_be: HashMap<u64, u64>,
}

/// The keys of some effects, claimed for measuring: they are let go of (and counted as tried, if no measurement was kept for them) when this is dropped, **whatever happens to the thread**.
struct Claimed<'host> {
    host: &'host PluginHost,
    keys: Vec<u64>,
}

impl Drop for Claimed<'_> {
    fn drop(&mut self) {
        let generation = self.host.generation();
        let mut ring_outs = self.host.ring_outs.lock().unwrap_or_else(PoisonError::into_inner);
        for key in &self.keys {
            ring_outs.in_flight.remove(key);
            // (A key whose measurement was not kept is one that could not be measured: counted so, so that it is not tried again at the next frame.)
            if !is_measured(*key) {
                ring_outs.could_not_be.insert(*key, generation);
            }
        }
    }
}

impl PluginHost {
    /// The effects of `project` whose tail is wanted, that no thread is measuring and that have not just failed to be: **claimed** for the caller (the keys are in `in_flight` until the
    /// caller makes a [`Claimed`] of them and drops it), and how many more are wanted that a thread is measuring.
    fn claim_the_tails_to_measure(&self, project: &Project) -> (Vec<TailToMeasure>, usize) {
        let wanted = the_tails_to_measure_in(project);
        let generation = self.generation();
        let mut ring_outs = self.ring_outs.lock().unwrap_or_else(PoisonError::into_inner);
        let mut mine = Vec::new();
        let mut being_measured_by_others = 0;
        for want in wanted {
            if ring_outs.could_not_be.get(&want.key) == Some(&generation) {
                continue;
            }
            if ring_outs.in_flight.insert(want.key) {
                mine.push(want);
            } else {
                being_measured_by_others += 1;
            }
        }
        (mine, being_measured_by_others)
    }

    /// Measures `wanted` one after another, keeping each measurement as it is done.
    fn measure_what_was_claimed(&self, wanted: &[TailToMeasure]) {
        for want in wanted {
            if let Some(ring) = self.measure_the_ring_out_of(&want.effect) {
                note_the_measured_tail(want.key, ring.samples);
            }
        }
    }

    /// Starts a thread that measures the tails of the sound effects of `project` that are wanted, if there are any that nobody is measuring. The window does not wait: what is
    /// measured changes the sound, and the preview makes it again (see `composition::how_many_tails_have_been_measured`).
    pub fn measure_the_tails_in_the_background(self: &Arc<Self>, project: &Project) {
        // (One thread at a time: a plugin that is run in a program of its own for a moment is not run for ten values of a hand's movement at once. Things are looked at again when it ends.)
        if self.how_many_tails_are_being_measured() > 0 {
            return;
        }
        let (mine, _) = self.claim_the_tails_to_measure(project);
        if mine.is_empty() {
            return;
        }
        let keys: Vec<u64> = mine.iter().map(|want| want.key).collect();
        let host = Arc::clone(self);
        let thread_keys = keys.clone();
        let started = std::thread::Builder::new()
            .name("tail meter".to_owned())
            .spawn(move || {
                // (Dropped when the thread ends, however it ends: the keys are let go of.)
                let _claimed = Claimed {
                    host: &host,
                    keys: thread_keys,
                };
                host.measure_what_was_claimed(&mine);
            });
        if started.is_err() {
            // (No thread: nothing is measured, and what was claimed is let go of and counted as tried. The scan's guess is used.)
            drop(Claimed { host: self, keys });
        }
    }

    /// Measures the tails of the sound effects of `project` that are wanted, and waits for those that a thread is measuring: when it returns, every tail that can be known is.
    /// **Waits at most two minutes** for the others.
    pub fn measure_the_tails_now(&self, project: &Project) {
        let deadline = Instant::now() + LONGEST_WAIT_FOR_THE_BACKGROUND;
        loop {
            let (mine, being_measured_by_others) = self.claim_the_tails_to_measure(project);
            {
                let _claimed = Claimed {
                    host: self,
                    keys: mine.iter().map(|want| want.key).collect(),
                };
                self.measure_what_was_claimed(&mine);
            }
            if being_measured_by_others == 0 || Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// How many tails a thread is measuring now.
    pub fn how_many_tails_are_being_measured(&self) -> usize {
        self.ring_outs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .in_flight
            .len()
    }
}
