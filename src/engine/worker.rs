// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The engine's thread: it makes the device, then takes what the window asks of it, a batch at a time, until it is told to stop.
//!
//! It is also **the watchdog**. A frame that the card does not finish in time (the wait for the card has a limit, `EngineSettings::frame_timeout`) and a device
//! that was lost make the thread throw the device away and make a new one, put the effects that were given to the engine as shaders on it, and say so
//! (`EngineEvent::DeviceReplaced`). What the thread knows about the frame that did not finish is which effects of strangers it had in it, and those are
//! switched off for as long as the engine runs. If a new device is needed again and again the engine stops instead (see `replacement`).

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use super::device::{DeviceError, DeviceTrouble, EngineDevice};
use super::frame_cache::FrameCache;
use super::frame_renderer::{FrameRenderer, RenderProblem};
use super::guard::GuardedShader;
use super::messages::{EngineEvent, EngineRequest, EngineSettings, WakeUp};
use super::replacement::{
    AfterAFailedFrame, ReplacementLimiter, what_to_do_after_a_failed_frame, why_the_device_was_lost,
};
use super::{FrameDescription, FrameStatistics, PictureData, PictureId};

/// What the thread does after a batch of requests.
enum Next {
    GoOn,
    /// The engine stops, for this reason, and says so.
    Stop(String),
    /// Nobody is listening any more: the thread ends without a word.
    Quit,
}

/// The body of the thread.
pub(super) fn run(
    settings: EngineSettings,
    requests: Receiver<EngineRequest>,
    events: Sender<EngineEvent>,
    wake_up: WakeUp,
) {
    let tell = |event: EngineEvent| {
        let sent = events.send(event).is_ok();
        wake_up();
        sent
    };
    let mut worker = match Worker::start(&settings, &tell) {
        Ok(worker) => worker,
        Err(error) => {
            tell(EngineEvent::Stopped {
                problem: format!("the picture engine could not start on {:?}: {error}", settings.adapter),
            });
            return;
        }
    };
    #[cfg(test)]
    if let Some(gate) = &settings.hold_the_first_batch_until {
        gate.wait_until_open();
    }

    loop {
        // Wait for something to be asked, then take everything that is waiting.
        let Ok(first) = requests.recv() else { return };
        let mut batch = vec![first];
        while let Ok(more) = requests.try_recv() {
            batch.push(more);
        }
        match worker.take(batch) {
            Next::GoOn => {}
            Next::Stop(problem) => {
                tell(EngineEvent::Stopped { problem });
                return;
            }
            Next::Quit => return,
        }
    }
}

/// Everything the thread keeps from one batch to the next.
struct Worker<'a> {
    settings: &'a EngineSettings,
    tell: &'a dyn Fn(EngineEvent) -> bool,
    renderer: FrameRenderer,
    cache: FrameCache,
    /// The effects that were given to the engine as shaders, in the order they were given, so that a new device can be given them again.
    given_effects: Vec<(String, Arc<GuardedShader>)>,
    /// The keys of the effects that are not run any more because a frame they were in did not finish.
    switched_off: Vec<String>,
    replacements: ReplacementLimiter,
}

impl<'a> Worker<'a> {
    fn start(settings: &'a EngineSettings, tell: &'a dyn Fn(EngineEvent) -> bool) -> Result<Self, DeviceError> {
        let device = EngineDevice::create(&settings.adapter)?;
        tell_the_watcher_of_the_tests(settings, &device);
        Ok(Self {
            settings,
            tell,
            renderer: FrameRenderer::new(device),
            cache: FrameCache::new(settings.most_cached_frames, settings.most_cached_frame_bytes),
            given_effects: Vec::new(),
            switched_off: Vec::new(),
            replacements: ReplacementLimiter::new(
                settings.replacements_that_stop_the_engine,
                settings.how_long_replacements_are_counted,
            ),
        })
    }

    /// Does what a batch of requests says, in order, and then makes the newest frame that was asked for in it.
    fn take(&mut self, batch: Vec<EngineRequest>) -> Next {
        if let Err(problem) = self.replace_the_device_if_it_was_lost() {
            return Next::Stop(problem);
        }
        let mut newest_render = None;
        let mut stop = false;
        for request in batch {
            let done = match request {
                EngineRequest::Upload { id, data } => self.take_a_picture(id, &data),
                EngineRequest::Forget(ids) => {
                    self.renderer.forget_pictures(ids);
                    Ok(())
                }
                EngineRequest::InstallEffect { key, shader } => self.take_an_effect(key, shader),
                EngineRequest::Render { serial, description } => {
                    newest_render = Some((serial, description));
                    Ok(())
                }
                EngineRequest::Stop => {
                    stop = true;
                    Ok(())
                }
            };
            if let Err(problem) = done {
                return Next::Stop(problem);
            }
        }
        if stop {
            return Next::Stop("the picture engine was told to stop".to_owned());
        }
        match newest_render {
            Some((serial, description)) => self.answer(serial, description),
            None => Next::GoOn,
        }
    }

    /// Tells the window something. (The next thing that is told finds out that nobody is listening.)
    fn say(&self, event: EngineEvent) -> Next {
        if (self.tell)(event) { Next::GoOn } else { Next::Quit }
    }

    fn take_a_picture(&mut self, id: PictureId, data: &PictureData) -> Result<(), String> {
        // (A picture is always the same pixels under the same id, so this is not meant to happen. If it does, the frames that were made with the old picture are
        // not what would be made now, and none of them is kept.)
        let replaces_a_picture = self.renderer.picture_size(id).is_some();
        match self.attempt_and_again_on_a_new_device_if_it_was_lost(|renderer| renderer.upload_picture(id, data))? {
            Ok(()) if replaces_a_picture => self.cache.clear(),
            Ok(()) => {}
            Err(problem) => {
                (self.tell)(EngineEvent::PictureRejected {
                    id,
                    problem: problem.to_string(),
                });
            }
        }
        Ok(())
    }

    fn take_an_effect(&mut self, key: String, shader: Arc<GuardedShader>) -> Result<(), String> {
        if self.switched_off.contains(&key) {
            (self.tell)(EngineEvent::EffectRejected {
                key,
                problem: "the effect is switched off: a frame that it was in did not finish".to_owned(),
            });
            return Ok(());
        }
        match self
            .attempt_and_again_on_a_new_device_if_it_was_lost(|renderer| renderer.install_effect(&key, &shader))?
        {
            Ok(()) => {
                self.given_effects.retain(|(given, _)| *given != key);
                self.given_effects.push((key, shader));
                // (What a frame is made of now depends on a shader that frames that were made did not have.)
                self.cache.clear();
            }
            Err(problem) => {
                (self.tell)(EngineEvent::EffectRejected {
                    key,
                    problem: problem.to_string(),
                });
            }
        }
        Ok(())
    }

    /// Makes the frame `description` describes (or takes it from the cache), and says how it came out. A frame that did not finish makes a new device.
    fn answer(&mut self, serial: u64, description: FrameDescription) -> Next {
        let fingerprint = description.fingerprint();
        if let Some(frame) = self.cache.get(fingerprint) {
            return self.say(EngineEvent::Frame {
                serial,
                frame,
                statistics: FrameStatistics {
                    came_from_the_cache: true,
                    ..FrameStatistics::default()
                },
            });
        }
        match self.renderer.render(&description, self.settings.frame_timeout) {
            Ok((frame, statistics)) => {
                let frame = Arc::new(frame);
                self.cache.insert(fingerprint, Arc::clone(&frame));
                self.say(EngineEvent::Frame {
                    serial,
                    frame,
                    statistics,
                })
            }
            Err(RenderProblem::PicturesMissing(ids)) => self.say(EngineEvent::PicturesMissing { serial, ids }),
            Err(problem) => {
                // What the frame had in it that the engine did not write, written down before the renderer may be thrown away.
                let effects_of_strangers = self.renderer.shader_effects_in(&description);
                let device_lost = self.the_device_was_lost();
                let failed = EngineEvent::FrameFailed {
                    serial,
                    problem: problem.to_string(),
                };
                match what_to_do_after_a_failed_frame(&problem, device_lost, self.settings.frame_timeout) {
                    AfterAFailedFrame::GoOn => self.say(failed),
                    AfterAFailedFrame::ReplaceTheDevice { why } => {
                        if !(self.tell)(failed) {
                            return Next::Quit;
                        }
                        match self.replace_the_device(why, effects_of_strangers) {
                            Ok(()) => Next::GoOn,
                            Err(problem) => Next::Stop(problem),
                        }
                    }
                }
            }
        }
    }

    /// Runs `attempt`, and if it did not work and the device turns out to have been lost, replaces the device and runs it once more on the new one. (A device
    /// that is lost makes everything that is asked of it go wrong: the picture or the effect is not at fault, and is not to be said to be.)
    fn attempt_and_again_on_a_new_device_if_it_was_lost<T>(
        &mut self,
        mut attempt: impl FnMut(&mut FrameRenderer) -> Result<T, RenderProblem>,
    ) -> Result<Result<T, RenderProblem>, String> {
        let first = attempt(&mut self.renderer);
        if first.is_ok() {
            return Ok(first);
        }
        let Some(reason) = self.the_device_was_lost() else {
            return Ok(first);
        };
        self.replace_the_device(why_the_device_was_lost(&reason), Vec::new())?;
        Ok(attempt(&mut self.renderer))
    }

    /// How many times the device has been replaced within the time that replacements are counted for.
    #[cfg(test)]
    fn replacements_made_so_far(&self) -> usize {
        self.replacements.remembered()
    }

    /// Whether the device is lost, and why: the library is asked what has happened to it (it says so when it is asked, and not before), and what the device has
    /// said about itself since it was last looked at is read.
    fn the_device_was_lost(&self) -> Option<String> {
        let _ = self.renderer.device().device.poll(wgpu::PollType::Poll);
        self.renderer
            .device()
            .take_trouble()
            .into_iter()
            .find_map(|trouble| match trouble {
                DeviceTrouble::Lost { reason, message } => Some(format!("{reason}: {message}")),
                DeviceTrouble::Error(_) => None,
            })
    }

    fn replace_the_device_if_it_was_lost(&mut self) -> Result<(), String> {
        match self.the_device_was_lost() {
            Some(reason) => self.replace_the_device(why_the_device_was_lost(&reason), Vec::new()),
            None => Ok(()),
        }
    }

    /// Throws the device away and makes a new one, gives it the effects that are still trusted, and tells the window. Fails (and the engine stops) if the device
    /// has been replaced too often or the new one cannot be made.
    fn replace_the_device(&mut self, why: String, effects_to_switch_off: Vec<String>) -> Result<(), String> {
        if !self.replacements.allow_another(Instant::now()) {
            return Err(format!(
                "the graphics device has had to be replaced {} times within {:?} and would have to be replaced again (because {why}), so the picture engine stops",
                self.settings.replacements_that_stop_the_engine.saturating_sub(1),
                self.settings.how_long_replacements_are_counted
            ));
        }
        let on_a_processor = self.renderer.device().adapter_information.device_type == wgpu::DeviceType::Cpu;
        // (On a card, what was running is stopped by this. On a processor it goes on until the program ends, as the findings of phase 0.5 say.)
        self.renderer.device().destroy();
        let new_device = EngineDevice::create(&self.settings.adapter)
            .map_err(|error| format!("the graphics device could not be replaced ({why}): {error}"))?;
        tell_the_watcher_of_the_tests(self.settings, &new_device);
        let old_renderer = std::mem::replace(&mut self.renderer, FrameRenderer::new(new_device));
        let_go_of_it_on_another_thread(old_renderer);
        // (Frames that were made are of the effects the engine trusted then, and of the effects it trusts now.)
        self.cache.clear();

        let newly_switched_off: Vec<String> = effects_to_switch_off
            .into_iter()
            .filter(|key| !self.switched_off.contains(key))
            .collect();
        self.switched_off.extend(newly_switched_off.iter().cloned());
        let given = std::mem::take(&mut self.given_effects);
        for (key, shader) in given {
            if self.switched_off.contains(&key) {
                continue;
            }
            match self.renderer.install_effect(&key, &shader) {
                Ok(()) => self.given_effects.push((key, shader)),
                Err(problem) => {
                    (self.tell)(EngineEvent::EffectRejected {
                        key,
                        problem: problem.to_string(),
                    });
                }
            }
        }

        (self.tell)(EngineEvent::DeviceReplaced {
            why: if on_a_processor {
                format!(
                    "{why} (this graphics adapter is a processor: what it was doing goes on using the processor until the program ends)"
                )
            } else {
                why
            },
            effects_switched_off: newly_switched_off,
        });
        Ok(())
    }
}

/// For the tests: says what device was made.
#[cfg(test)]
fn tell_the_watcher_of_the_tests(settings: &EngineSettings, device: &EngineDevice) {
    if let Some(watcher) = &settings.watch_the_devices {
        watcher.tell(device);
    }
}

#[cfg(not(test))]
fn tell_the_watcher_of_the_tests(_settings: &EngineSettings, _device: &EngineDevice) {}

/// Lets go of the old renderer on a thread of its own: **letting go of the last handle to a device whose card is still busy waits for the card** (found by
/// experiment on WARP, 2026-10-16: the engine's thread did not come back from it, and a card that is a processor cannot be told to stop what it is doing), and the
/// engine's own thread has a new device to serve. So the renderer is the last that has the old device, and nothing else on the engine's thread is let to keep a
/// handle to it.
fn let_go_of_it_on_another_thread<T: Send + 'static>(renderer: T) {
    let _ = std::thread::Builder::new()
        .name(THE_THREAD_THAT_LETS_GO_OF_OLD_DEVICES.to_owned())
        .spawn(move || drop(renderer));
}

/// The name of the thread that an old device is let go of on.
const THE_THREAD_THAT_LETS_GO_OF_OLD_DEVICES: &str = "old graphics device";

#[cfg(test)]
mod tests;
