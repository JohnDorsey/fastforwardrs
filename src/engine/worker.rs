// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The engine's thread: it makes the device, then takes what the window asks of it, a batch at a time, until it is told to stop.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};

use super::FrameStatistics;
use super::device::{DeviceTrouble, EngineDevice};
use super::frame_cache::FrameCache;
use super::frame_renderer::FrameRenderer;
use super::messages::{EngineEvent, EngineRequest, EngineSettings, WakeUp};

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
    let device = match EngineDevice::create(&settings.adapter) {
        Ok(device) => device,
        Err(error) => {
            tell(EngineEvent::Stopped {
                problem: format!("the picture engine could not start on {:?}: {error}", settings.adapter),
            });
            return;
        }
    };
    let mut renderer = FrameRenderer::new(device);
    let mut cache = FrameCache::new(settings.most_cached_frames, settings.most_cached_frame_bytes);
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
        let mut newest_render = None;
        let mut stop = false;
        for request in batch {
            match request {
                EngineRequest::Render { serial, description } => newest_render = Some((serial, description)),
                EngineRequest::Stop => stop = true,
            }
        }

        for trouble in renderer.device().take_trouble() {
            if let DeviceTrouble::Lost { reason, message } = trouble {
                tell(EngineEvent::Stopped {
                    problem: format!("the graphics device was lost ({reason}: {message})"),
                });
                return;
            }
        }
        if stop {
            tell(EngineEvent::Stopped {
                problem: "the picture engine was told to stop".to_owned(),
            });
            return;
        }
        let Some((serial, description)) = newest_render else {
            continue;
        };

        let fingerprint = description.fingerprint();
        let event = if let Some(frame) = cache.get(fingerprint) {
            EngineEvent::Frame {
                serial,
                frame,
                statistics: FrameStatistics {
                    came_from_the_cache: true,
                    ..FrameStatistics::default()
                },
            }
        } else {
            match renderer.render(&description, settings.frame_timeout) {
                Ok((frame, statistics)) => {
                    let frame = Arc::new(frame);
                    cache.insert(fingerprint, Arc::clone(&frame));
                    EngineEvent::Frame {
                        serial,
                        frame,
                        statistics,
                    }
                }
                Err(problem) => EngineEvent::FrameFailed {
                    serial,
                    problem: problem.to_string(),
                },
            }
        };
        if !tell(event) {
            return;
        }
    }
}
