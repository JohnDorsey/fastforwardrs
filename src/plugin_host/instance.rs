// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a plugin of either format is to the effect that runs it (C14 of `docs/engine-phase-7-clap-design.md`): a small trait that the LADSPA host and the CLAP host each fill, so
//! that the layouts, the 32-sample looks at the keyframes, the range, the silence for sound that is not fit to be heard, the patience for a slow plugin and the notes are written
//! once, in [`super::PluginEffect`].

use std::sync::{Arc, Mutex};

use super::effect::SAMPLES_BETWEEN_LOOKS_WHEN_ANIMATED;
use crate::sound_engine::SAMPLE_RATE;

/// How many frames a CLAP plugin is activated for at most, and so how many it is given in one call (a block of the sound engine is no more than this; a longer stretch is
/// given in pieces).
pub const MOST_FRAMES_OF_A_CLAP_PLUGIN: usize = crate::sound_engine::BLOCK_SAMPLES;

/// What the effect does with one copy of a plugin.
pub trait PluginInstance: Send {
    /// Makes the value that the plugin calls `handle` (a port number, a parameter id) `value`, from sample `at_sample` of the next [`Self::run`] (or at once, for a format that
    /// cannot say when: see [`Self::takes_values_at_samples`]).
    fn set_value(&mut self, handle: u32, value: f32, at_sample: usize);

    /// Whether [`Self::set_value`] with a sample is applied at that sample, in the middle of a run (CLAP). Where it is not (LADSPA), the effect runs the plugin in pieces and
    /// sets the values between them.
    fn takes_values_at_samples(&self) -> bool;

    /// The most samples to give in one run, if there is a limit.
    fn most_samples_at_once(&self) -> Option<usize>;

    /// Runs the plugin on `frames` samples: the channels of its main input and of its main output, each as long as `frames`.
    fn run(&mut self, inputs: &mut [&mut [f32]], outputs: &mut [&mut [f32]], frames: usize) -> Result<(), String>;

    /// Makes the plugin forget what it has heard (the values it was given are kept).
    fn reset(&mut self);

    /// How many samples later its sound comes out than the sound that goes in.
    fn latency(&self) -> usize;

    /// What a plugin has asked of the host that is done between blocks, and not in the middle of one (CLAP: a call on its main thread, a restart). Says what happened.
    fn between_blocks(&mut self) -> BetweenBlocks {
        BetweenBlocks::default()
    }
}

/// What was done between two blocks for a plugin.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct BetweenBlocks {
    /// The plugin was made again (it asked to be): it has forgotten the values it was given, and what it had heard.
    pub was_restarted: bool,
    /// Something for the person, which the effect says once.
    pub note: Option<String>,
}

/// A copy of a LADSPA plugin.
pub struct LadspaCopy(pub ladspa_host::Instance);

impl PluginInstance for LadspaCopy {
    fn set_value(&mut self, handle: u32, value: f32, _at_sample: usize) {
        // (A value that is not a number is refused by the host, and the plugin keeps the one it had.)
        let _ = self.0.set_control(handle as usize, value);
    }

    fn takes_values_at_samples(&self) -> bool {
        false
    }

    fn most_samples_at_once(&self) -> Option<usize> {
        None
    }

    fn run(&mut self, inputs: &mut [&mut [f32]], outputs: &mut [&mut [f32]], frames: usize) -> Result<(), String> {
        self.0.run(inputs, outputs, frames).map_err(|error| error.to_string())
    }

    fn reset(&mut self) {
        self.0.reset();
    }

    fn latency(&self) -> usize {
        0
    }
}

/// What makes a CLAP plugin again, when it asks to be made again.
pub struct ClapMaker {
    pub worker: Arc<clap_host::MainThread>,
    pub library: clap_host::Library,
    pub id: String,
}

impl ClapMaker {
    /// A copy of the plugin, activated at the rate of the sound engine.
    pub fn make(&self) -> Result<clap_host::Instance, clap_host::InstanceError> {
        clap_host::Instance::new(
            &self.worker,
            &self.library,
            &self.id,
            f64::from(SAMPLE_RATE),
            MOST_FRAMES_OF_A_CLAP_PLUGIN as u32,
        )
    }
}

/// How many times a copy of a CLAP plugin is made again because it asks to be: a plugin that asks every time it is made would be made again at every block for ever, and the
/// sound would be a gap after a gap.
pub const MOST_RESTARTS_OF_A_COPY: usize = 5;

/// Which plugins are alive, by library and by the stream whose effects they are, and which of those streams are being stopped: what a plugin that cannot have two of itself alive at
/// once needs to be waited for (see [`PluginHost`](super::PluginHost)).
#[derive(Default)]
pub struct Lives {
    /// For each plugin that is alive: its number, the library it is of, and the stream it is for.
    alive: Vec<(u64, std::path::PathBuf, usize)>,
    /// The streams that are being stopped, by what names them, with something that says whether they are still there: their plugins are about to be let go of. (A stream
    /// that is being stopped may not have made its plugins yet: it will, in the block it is in.)
    stopping: Vec<(usize, std::sync::Weak<Mutex<Vec<String>>>)>,
    next: u64,
}

impl Lives {
    /// Says that a plugin of `library` is alive for `stream`. Dropping what is given says that it is not.
    pub fn is_alive(lives: &Arc<Mutex<Self>>, library: &std::path::Path, stream: usize) -> LiveGuard {
        let mut inside = lives.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let number = inside.next;
        inside.next += 1;
        inside.alive.push((number, library.to_owned(), stream));
        LiveGuard {
            lives: Arc::clone(lives),
            number,
        }
    }

    /// Says that the plugins of the stream that `notes` are of, which `stream` names, are about to be let go of (or will be, as soon as it has made them: it may not have).
    pub fn stream_is_stopping(&mut self, stream: usize, notes: &Arc<Mutex<Vec<String>>>) {
        self.stopping.retain(|(_, still_there)| still_there.strong_count() > 0);
        self.stopping.push((stream, Arc::downgrade(notes)));
    }

    /// Whether the stream that `stream` names is being stopped.
    pub fn is_stopping(&self, stream: usize) -> bool {
        self.stopping
            .iter()
            .any(|(of, still_there)| *of == stream && still_there.strong_count() > 0)
    }

    /// Whether a plugin of `library` is alive for a stream that is being stopped, **and is not `except`** (the streams are told apart by what the host is given with them).
    pub fn is_something_of_a_stopping_stream_alive(&self, library: &std::path::Path, except: usize) -> bool {
        self.alive
            .iter()
            .any(|(_, of_library, stream)| of_library == library && *stream != except && self.is_stopping(*stream))
    }
}

/// What says, for as long as it is kept, that a plugin is alive (see [`Lives`]).
pub struct LiveGuard {
    lives: Arc<Mutex<Lives>>,
    number: u64,
}

impl Drop for LiveGuard {
    fn drop(&mut self) {
        let mut inside = self.lives.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(place) = inside.alive.iter().position(|(number, _, _)| *number == self.number) else {
            return;
        };
        inside.alive.remove(place);
    }
}

/// A copy of a CLAP plugin.
pub struct ClapCopy {
    instance: clap_host::Instance,
    /// (After the instance, which is dropped first: the plugin is gone when it is said to be.)
    _alive: LiveGuard,
    maker: ClapMaker,
    /// The values to be applied in the next run, in order of time.
    pending: Vec<clap_host::ParameterChange>,
    restarts: usize,
}

impl ClapCopy {
    pub fn new(instance: clap_host::Instance, maker: ClapMaker, alive: LiveGuard) -> Self {
        Self {
            instance,
            _alive: alive,
            maker,
            pending: Vec::new(),
            restarts: 0,
        }
    }
}

impl PluginInstance for ClapCopy {
    fn set_value(&mut self, handle: u32, value: f32, at_sample: usize) {
        self.pending.push(clap_host::ParameterChange {
            time: u32::try_from(at_sample).unwrap_or(u32::MAX),
            id: handle,
            value: f64::from(value),
        });
    }

    fn takes_values_at_samples(&self) -> bool {
        true
    }

    fn most_samples_at_once(&self) -> Option<usize> {
        Some(MOST_FRAMES_OF_A_CLAP_PLUGIN)
    }

    fn run(&mut self, inputs: &mut [&mut [f32]], outputs: &mut [&mut [f32]], _frames: usize) -> Result<(), String> {
        let result = self.instance.run(inputs, outputs, &self.pending);
        self.pending.clear();
        result.map(|_| ()).map_err(|error| error.to_string())
    }

    fn reset(&mut self) {
        self.instance.reset();
    }

    fn latency(&self) -> usize {
        self.instance.latency() as usize
    }

    fn between_blocks(&mut self) -> BetweenBlocks {
        let mut done = BetweenBlocks::default();
        self.instance.do_the_main_thread_callback_if_asked();
        if self.instance.restart_was_requested() && self.restarts >= MOST_RESTARTS_OF_A_COPY {
            if self.restarts == MOST_RESTARTS_OF_A_COPY {
                self.restarts += 1;
                done.note = Some(format!(
                    "it asked to be made again more than {MOST_RESTARTS_OF_A_COPY} times, and is not made again any more (it goes on as it is)"
                ));
            }
        } else if self.instance.restart_was_requested() {
            self.restarts += 1;
            match self.maker.make() {
                Ok(again) => {
                    let latency_changed = again.latency() != self.instance.latency();
                    self.instance = again;
                    done.was_restarted = true;
                    if latency_changed {
                        done.note = Some(
                            "its latency changed when it was made again, and the sound is not made up for the new latency until the next play".to_owned(),
                        );
                    }
                }
                Err(error) => {
                    done.note = Some(format!("it asked to be made again and could not be: {error}"));
                }
            }
        }
        done
    }
}

/// Where in a block the keyframes are looked at, for a plugin that is given values at samples: at the start of each piece, and at every [`SAMPLES_BETWEEN_LOOKS_WHEN_ANIMATED`]th
/// sample of the clip (counted from its first, so the same places whatever the blocks are) in between, if the values are animated.
pub fn places_to_look_in(first_sample_of_the_piece: u64, length: usize, animated: bool) -> Vec<usize> {
    let mut places = vec![0];
    if animated {
        let step = SAMPLES_BETWEEN_LOOKS_WHEN_ANIMATED as u64;
        let first_grid_sample = first_sample_of_the_piece.div_ceil(step) * step;
        let mut place = (first_grid_sample - first_sample_of_the_piece) as usize;
        while place < length {
            if place > 0 {
                places.push(place);
            }
            place += SAMPLES_BETWEEN_LOOKS_WHEN_ANIMATED;
        }
    }
    places
}
