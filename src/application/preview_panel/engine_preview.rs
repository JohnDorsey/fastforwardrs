// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The picture engine as the preview uses it: the engine, what it has been sent, and the frame it made last, which is what the preview shows.
//!
//! Each time the window is drawn, [`EnginePreview::update`] takes what the engine has said since (a finished frame becomes the picture that is shown; a
//! picture that the engine does not have is sent again), describes the frame that the content of the preview makes ([`describe_frame`]), sends the pictures
//! that the engine has not been sent, and, if the frame is not the one that was asked for last, asks for it. The engine makes only the newest frame it is asked
//! for, so a window that changes faster than the engine makes frames shows the latest frame that is done, a moment behind. Nothing here waits for the engine.
//!
//! **If the engine stops** (it could not make a device, or lost it, or its thread ended) the preview says so ([`EnginePreview::why_it_stopped`]) and the
//! window draws the preview the old way from then on.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use egui::{Color32, ColorImage, Context, Painter, Rect, TextureHandle, pos2};

use super::PreviewContent;
use super::engine_view::{PictureSupply, PictureWanted, SentPictures, describe_frame};
use crate::application::textures::PictureFilter;
use crate::engine::{
    AdapterChoice, Engine, EngineEvent, EngineSettings, FrameDescription, LayerSource, PictureId, RenderedFrame, WakeUp,
};
use crate::text_rendering::LazyTextRenderer;

/// How many pictures are kept in the engine, at the most, before the ones that the last frames did not use are let go of. (A picture of a video is a new
/// one for every picture of the video that is shown, so without this they would pile up in the engine, and in the list of what it has been sent.)
const MOST_PICTURES_KEPT_WITHOUT_USE: usize = 48;

/// For how many of the last frames asked for a picture counts as used.
const FRAMES_A_PICTURE_STAYS_FOR: u64 = 4;

/// What size the engine is to make the frame at: the size the picture is shown at on the screen, in pixels of the screen, but no more than the picture of the
/// project is (a picture shown bigger than it is has its pixels made bigger by the window, which is how it is zoomed in on), and in the shape of that
/// picture. A picture that is shown smaller is made smaller (the engine has less to do for it, and the window has nothing to shrink).
pub fn render_size_for(
    shown_size_in_points: egui::Vec2,
    pixels_per_point: f32,
    picture_size: (u32, u32),
) -> (u32, u32) {
    let (width, height) = (picture_size.0.max(1) as f32, picture_size.1.max(1) as f32);
    let shown_width_in_pixels = (shown_size_in_points.x * pixels_per_point).max(1.0);
    let scale = (shown_width_in_pixels / width).min(1.0);
    (
        (width * scale).round().max(2.0) as u32,
        (height * scale).round().max(2.0) as u32,
    )
}

/// The frame that is on the preview.
struct ShownFrame {
    texture: TextureHandle,
    /// The frame it is a picture of, which is kept so that it can be put on the graphics card again when it is to be put on the screen another way.
    frame: Arc<RenderedFrame>,
    /// How it was put on the screen.
    filter: PictureFilter,
    /// The serial number of the request it came from.
    serial: u64,
}

/// See the module documentation.
pub struct EnginePreview {
    engine: Engine,
    /// The pictures the engine has been sent and has not been told to let go of.
    sent: HashSet<PictureId>,
    /// Pictures that the engine said it could not use, which are not sent again.
    rejected: HashSet<PictureId>,
    /// For each picture sent, the number of the last frame that was asked for that used it.
    last_used_in: HashMap<PictureId, u64>,
    /// A number the window changes when it lets go of what it knows of its files (see [`describe_frame`]).
    generation: u64,
    /// The fingerprint of the frame that was asked for last and the serial number of its request, so that the same frame is not asked for again.
    asked_for: Option<(u64, u64)>,
    /// How many frames were asked for.
    frames_asked_for: u64,
    /// The serial number of the last request.
    last_serial: u64,
    /// The first serial number whose frame may be shown: the frames of requests before it are of what the window has let go of.
    first_serial_that_counts: u64,
    shown: Option<ShownFrame>,
    /// Why the engine stopped, if it has.
    stopped: Option<String>,
    /// The last frame that the engine could not make, and why, or the last picture that it could not use.
    last_problem: Option<String>,
}

impl EnginePreview {
    /// Starts the engine on `adapter` (this does not wait for it), which calls `wake_up` whenever it has something to say.
    pub fn start(adapter: AdapterChoice, wake_up: WakeUp) -> Self {
        Self::with_engine(Engine::start(EngineSettings::new(adapter), wake_up))
    }

    /// The preview of an engine that is already started.
    pub fn with_engine(engine: Engine) -> Self {
        Self {
            engine,
            sent: HashSet::new(),
            rejected: HashSet::new(),
            last_used_in: HashMap::new(),
            generation: 0,
            asked_for: None,
            frames_asked_for: 0,
            last_serial: 0,
            first_serial_that_counts: 0,
            shown: None,
            stopped: None,
            last_problem: None,
        }
    }

    /// Why the engine has stopped, if it has: the preview is then to be drawn the old way.
    pub fn why_it_stopped(&self) -> Option<&str> {
        self.stopped.as_deref()
    }

    /// The last problem the engine had that did not stop it: a frame it could not make, a picture it could not use.
    #[cfg(test)]
    pub fn last_problem(&self) -> Option<&str> {
        self.last_problem.as_deref()
    }

    /// How many frames have been asked for.
    #[cfg(test)]
    pub fn frames_asked_for(&self) -> u64 {
        self.frames_asked_for
    }

    /// The number that the window changes whenever it lets go of what it knows of its files.
    #[cfg(test)]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// How the frame that is shown is put on the screen.
    #[cfg(test)]
    pub fn filter_it_is_shown_with(&self) -> Option<PictureFilter> {
        self.shown.as_ref().map(|shown| shown.filter)
    }

    /// Whether the frame that is on the preview is the one that was asked for last (the engine has made it, and it has been taken).
    #[cfg(test)]
    pub fn has_shown_the_last_frame_asked_for(&self) -> bool {
        self.frames_asked_for > 0
            && self
                .shown
                .as_ref()
                .is_some_and(|shown| self.asked_for.is_some_and(|(_, serial)| shown.serial == serial))
    }

    /// Whether a frame of the engine is on the preview.
    #[cfg(test)]
    pub fn has_a_frame_to_show(&self) -> bool {
        self.shown.is_some()
    }

    /// The pixels and the size of the frame that is on the preview.
    #[cfg(test)]
    pub fn the_frame_shown(&self) -> Option<&RenderedFrame> {
        self.shown.as_ref().map(|shown| &*shown.frame)
    }

    /// How many pictures the engine has been sent and has not been told to let go of.
    #[cfg(test)]
    pub fn pictures_the_engine_has(&self) -> usize {
        self.sent.len()
    }

    /// How the window lets go of what it knows of its files (another project, or a file that was replaced): the pictures that come from files get other ids,
    /// the frame is asked for again, and the frames that are on their way, which are of the old ones, are not shown. (The frame that is shown stays until
    /// the new one comes, which is a moment: a picture that goes black in between would flash.)
    pub fn forget_what_is_known_of_the_files(&mut self) {
        self.generation += 1;
        self.asked_for = None;
        self.first_serial_that_counts = self.last_serial + 1;
    }

    /// Takes what the engine has said, describes `content` as a frame `render_size` big, sends what the engine does not have, and asks for the frame if it
    /// is not the one asked for last. `filter` says how the frame is put on the screen.
    pub fn update(
        &mut self,
        context: &Context,
        content: &PreviewContent<'_>,
        render_size: (u32, u32),
        filter: PictureFilter,
        text_renderer: &mut LazyTextRenderer,
    ) {
        self.take_what_the_engine_says(context, filter);
        if self.stopped.is_some() {
            return;
        }
        if !self.engine.is_running() {
            self.stopped = Some("the picture engine's thread ended without saying why".to_owned());
            return;
        }
        let description = {
            let Self {
                engine,
                sent,
                rejected,
                generation,
                ..
            } = self;
            let mut supply = SuppliesWhatIsNotRejected {
                inner: SentPictures {
                    sent,
                    text_renderer,
                    send: |id, data| engine.upload_picture(id, Arc::new(data)),
                },
                rejected,
            };
            describe_frame(content, render_size, *generation, &mut supply)
        };
        let fingerprint = description.fingerprint();
        if self.asked_for.map(|(asked, _)| asked) != Some(fingerprint) {
            self.note_the_pictures_used(&description);
            let serial = self.engine.request_frame(description);
            self.asked_for = Some((fingerprint, serial));
            self.last_serial = serial;
            self.frames_asked_for += 1;
            self.let_go_of_the_pictures_that_are_not_used_any_more();
        }
        self.put_the_frame_on_the_screen_the_way_it_is_asked_for(context, filter);
    }

    /// Paints the frame that is shown over `rectangle` (all of the picture, wherever it is on the screen, which may reach past what `painter` is clipped to).
    pub fn paint(&self, painter: &Painter, rectangle: Rect) {
        if let Some(shown) = &self.shown {
            painter.image(
                shown.texture.id(),
                rectangle,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }

    /// Tells the engine's thread to stop (the preview is then to be drawn the old way).
    #[cfg(test)]
    pub fn stop(&self) {
        self.engine.stop();
    }

    fn take_what_the_engine_says(&mut self, context: &Context, filter: PictureFilter) {
        while let Some(event) = self.engine.try_next_event() {
            match event {
                EngineEvent::Frame { serial, frame, .. } => self.show(context, serial, frame, filter),
                EngineEvent::FrameFailed { problem, .. } => self.last_problem = Some(problem),
                EngineEvent::PicturesMissing { ids, .. } => {
                    // (They are sent again, and the frame is asked for again.)
                    for id in ids {
                        self.sent.remove(&id);
                        self.last_used_in.remove(&id);
                    }
                    self.asked_for = None;
                }
                EngineEvent::PictureRejected { id, problem } => {
                    self.sent.remove(&id);
                    self.last_used_in.remove(&id);
                    self.rejected.insert(id);
                    self.last_problem = Some(problem);
                    self.asked_for = None;
                }
                EngineEvent::EffectRejected { problem, .. } => self.last_problem = Some(problem),
                EngineEvent::DeviceReplaced { why, .. } => {
                    // (The new device has none of the pictures: they are all sent again, and the frame is asked for again, which the engine may now make
                    // without an effect that it does not trust any more.)
                    self.sent.clear();
                    self.last_used_in.clear();
                    self.asked_for = None;
                    self.last_problem = Some(why);
                }
                EngineEvent::Stopped { problem } => self.stopped = Some(problem),
            }
        }
    }

    /// Makes the frame the one that is shown, unless a later one already is or it is of what has been let go of.
    fn show(&mut self, context: &Context, serial: u64, frame: Arc<RenderedFrame>, filter: PictureFilter) {
        if serial < self.first_serial_that_counts || self.shown.as_ref().is_some_and(|shown| shown.serial > serial) {
            return;
        }
        let image = image_of(&frame);
        match &mut self.shown {
            Some(shown)
                if shown.frame.width_in_pixels == frame.width_in_pixels
                    && shown.frame.height_in_pixels == frame.height_in_pixels =>
            {
                shown.texture.set(image, filter.options());
                shown.frame = frame;
                shown.filter = filter;
                shown.serial = serial;
            }
            _ => {
                self.shown = Some(ShownFrame {
                    texture: context.load_texture("the preview, made by the picture engine", image, filter.options()),
                    frame,
                    filter,
                    serial,
                });
            }
        }
    }

    /// Puts the frame that is shown on the graphics card again if the way it is to be put on the screen is not the way it was.
    fn put_the_frame_on_the_screen_the_way_it_is_asked_for(&mut self, _context: &Context, filter: PictureFilter) {
        if let Some(shown) = &mut self.shown
            && shown.filter != filter
        {
            shown.texture.set(image_of(&shown.frame), filter.options());
            shown.filter = filter;
        }
    }

    fn note_the_pictures_used(&mut self, description: &FrameDescription) {
        let frame_number = self.frames_asked_for + 1;
        for layer in &description.layers {
            if let LayerSource::Picture(id) = layer.source {
                self.last_used_in.insert(id, frame_number);
            }
        }
    }

    /// Lets go of the pictures that none of the last frames asked for used, when there are more than a few: the pictures of a video are new ones all the time.
    fn let_go_of_the_pictures_that_are_not_used_any_more(&mut self) {
        if self.sent.len() <= MOST_PICTURES_KEPT_WITHOUT_USE {
            return;
        }
        let this_frame = self.frames_asked_for;
        let not_used: Vec<PictureId> = self
            .sent
            .iter()
            .copied()
            .filter(|id| {
                self.last_used_in
                    .get(id)
                    .is_none_or(|last| last + FRAMES_A_PICTURE_STAYS_FOR < this_frame)
            })
            .collect();
        if not_used.is_empty() {
            return;
        }
        for id in &not_used {
            self.sent.remove(id);
            self.last_used_in.remove(id);
        }
        self.engine.forget_pictures(not_used);
    }
}

/// The picture of a frame as the window's graphics card takes it: it is all opaque, so what is premultiplied and what is not is the same.
fn image_of(frame: &RenderedFrame) -> ColorImage {
    ColorImage::from_rgba_premultiplied(
        [frame.width_in_pixels as usize, frame.height_in_pixels as usize],
        &frame.rgba_pixels,
    )
}

/// The supply of [`SentPictures`], but for the pictures that the engine said it could not use, which it says it cannot make again.
struct SuppliesWhatIsNotRejected<'a, Send: FnMut(PictureId, crate::engine::PictureData)> {
    inner: SentPictures<'a, Send>,
    rejected: &'a HashSet<PictureId>,
}

impl<Send: FnMut(PictureId, crate::engine::PictureData)> PictureSupply for SuppliesWhatIsNotRejected<'_, Send> {
    fn make_sure_the_engine_has(&mut self, wanted: &PictureWanted<'_>) -> bool {
        !self.rejected.contains(&wanted.id) && self.inner.make_sure_the_engine_has(wanted)
    }
}

#[cfg(test)]
mod tests;
