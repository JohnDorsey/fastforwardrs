// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The window of a plugin (`IPlugView`; piece 15.6 of `docs/engine-phase-15-vst3-design.md`): made by the plugin's controller, put in a window of the program that calls this (the
//! parent), given a frame to ask for another size with, told its size both ways, and watched for what a hand does in it.
//!
//! **The calls are those of a VST3 host, in the order the SDK's own host examples make them**: `createView("editor")`, `setFrame`, the scale, `attached`, `getSize`; and, when it ends,
//! `removed` and `setFrame(null)`. Everything is on the thread that made the [`Instance`] (the program's main thread, which has the message loop).
//!
//! A window **edits parameters by telling the host**, not by being told: [`Instance::view_events`] gives those edits (as plain values, like everything else here), the requests for
//! another size (which it also answers, by telling the window the size it will have), and "the state changed". It also hands the component the edits, so that the state saved
//! after them has them.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::ffi::c_void;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use thiserror::Error;
use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;
use vst3::{ComPtr, ComWrapper};

use crate::instance::Instance;
use crate::objects::{FrameObject, Step};

/// The most frames of the silent block that gives the component the edits that a window made.
const FLUSH_BLOCK_FRAMES: usize = 64;

/// Whether a plugin's window is told the scale of the screen. **Not on a Mac**: a window there is in points, and the system scales it; a plugin that is told the scale as well draws itself twice as big as the window.
const SCALE_IS_TOLD: bool = !cfg!(target_os = "macos");

/// How big a window is, and whether it can be resized (in pixels, as the plugin says).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewSize {
    pub width: u32,
    pub height: u32,
    pub can_resize: bool,
}

/// Why a window could not be put in a parent.
#[derive(Debug, Error)]
pub enum ViewError {
    #[error("the plugin has no window")]
    NoWindow,
    #[error("the plugin's window is open already")]
    AlreadyOpen,
    #[error("the window of the program has no handle of a kind that a plugin's window can be put in")]
    NoParentHandle,
    #[error("the plugin's window is not of the kind of window ({0}) that this system puts a window in")]
    KindNotSupported(&'static str),
    #[error("the plugin's window would not be put in its parent (it said {0:#x})")]
    WouldNotBeAttached(i32),
}

/// What a plugin's window did, or asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum ViewEvent {
    /// A hand took hold of a parameter.
    Begin { id: u32 },
    /// A hand moved it to this value (plain).
    Value { id: u32, plain: f64 },
    /// A hand let go of it.
    End { id: u32 },
    /// The window asks to be this big (in pixels). **It has been told it will be**; the window of the program it is in is to be made that big.
    ResizeRequested { width: u32, height: u32 },
    /// The plugin says that its state is not what was saved last.
    StateChanged,
    /// The plugin changed values by itself (a preset was chosen in its window): these are the ones that differ from what the host knew, as `(parameter, plain value)`.
    ValuesChanged(Vec<(u32, f64)>),
}

/// A window that is open: the plugin's view, the frame it was given, and the values the host last knew (plain), so that a change shows.
pub(crate) struct OpenView {
    view: ComPtr<IPlugView>,
    frame: ComWrapper<FrameObject>,
    known: HashMap<u32, f64>,
}

/// The kind of parent window that this system puts a plugin's window in, as VST3 names it, and as we do in words.
fn the_kind_of_parent() -> (FIDString, &'static str) {
    if cfg!(target_os = "windows") {
        (kPlatformTypeHWND, "HWND")
    } else if cfg!(target_os = "macos") {
        (kPlatformTypeNSView, "NSView")
    } else {
        (kPlatformTypeX11EmbedWindowID, "X11EmbedWindowID")
    }
}

/// What a window of the program is, as the id that a plugin's window is put in (an `HWND`, an X11 window id, an `NSView`), if it has one of a kind that VST3 knows.
pub(crate) fn handle_of_a_window<W: HasWindowHandle>(window: &W) -> Option<usize> {
    Some(match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => handle.hwnd.get().cast_unsigned(),
        RawWindowHandle::Xlib(handle) => usize::try_from(handle.window).ok()?,
        RawWindowHandle::Xcb(handle) => usize::try_from(handle.window.get()).ok()?,
        RawWindowHandle::AppKit(handle) => handle.ns_view.as_ptr().addr(),
        _ => return None,
    })
}

fn rect_of(width: u32, height: u32) -> ViewRect {
    ViewRect {
        left: 0,
        top: 0,
        right: i32::try_from(width).unwrap_or(i32::MAX),
        bottom: i32::try_from(height).unwrap_or(i32::MAX),
    }
}

fn size_of(rect: &ViewRect) -> Option<(u32, u32)> {
    let width = u32::try_from(rect.right.checked_sub(rect.left)?).ok()?;
    let height = u32::try_from(rect.bottom.checked_sub(rect.top)?).ok()?;
    Some((width, height))
}

impl Instance {
    /// A window of the plugin's controller, made and not put anywhere.
    fn make_a_view(&self) -> Option<ComPtr<IPlugView>> {
        let controller = self.controller.as_ref()?;
        // SAFETY: the controller is live; the name is text that ends in a zero and lives through the call.
        let raw = unsafe { controller.createView(c"editor".as_ptr()) };
        // SAFETY: a view that `createView` gave is ours to let go of (a null is no view).
        unsafe { ComPtr::from_raw(raw) }
    }

    /// Whether the plugin has a window of the kind that this system can put in a parent. (A window is made to find out, and let go of at once.)
    pub fn has_a_view(&self) -> bool {
        let (kind, _) = the_kind_of_parent();
        // SAFETY: the view is live; the kind is text that ends in a zero.
        self.make_a_view()
            .is_some_and(|view| unsafe { view.isPlatformTypeSupported(kind) } == kResultTrue)
    }

    /// Makes the plugin's window and puts it in `parent` (the window id of this system: an `HWND`, an X11 window id, an `NSView`), at the scale `scale` (1 is a pixel for each pixel). The
    /// window is told the scale and the frame first, as the SDK's hosts do.
    pub fn open_view(&mut self, parent: usize, scale: f64) -> Result<ViewSize, ViewError> {
        if self.view.is_some() {
            return Err(ViewError::AlreadyOpen);
        }
        let view = self.make_a_view().ok_or(ViewError::NoWindow)?;
        let (kind, kind_name) = the_kind_of_parent();
        // SAFETY: the view is live; the kind is text that ends in a zero.
        if unsafe { view.isPlatformTypeSupported(kind) } != kResultTrue {
            return Err(ViewError::KindNotSupported(kind_name));
        }
        let frame = FrameObject::new();
        let frame_pointer = frame
            .to_com_ptr::<IPlugFrame>()
            .ok_or(ViewError::WouldNotBeAttached(kInternalError))?;
        // SAFETY: the view is live; the frame is kept for as long as the view is (it is in `OpenView`, which lets go of the view first).
        unsafe { view.setFrame(frame_pointer.as_ptr()) };
        if SCALE_IS_TOLD && let Some(support) = view.cast::<IPlugViewContentScaleSupport>() {
            // SAFETY: the view is live; the call has a number only.
            unsafe { support.setContentScaleFactor(scale as f32) };
        }
        // (What the window does while it is being put in its parent counts: a plugin may edit, or say that its state changed, in `attached`. So the gestures are kept from before it.)
        self.host.record_the_gestures(true);
        let known: HashMap<u32, f64> = self.values_of_the_controller().into_iter().collect();
        // SAFETY: the view is live; the parent is a window id of this system (whoever calls this made it the program's own window), and the kind is text that ends in a zero.
        let attached = unsafe { view.attached(std::ptr::with_exposed_provenance_mut::<c_void>(parent), kind) };
        if attached != kResultOk {
            // SAFETY: the view is live; no frame is what it had before.
            unsafe { view.setFrame(std::ptr::null_mut()) };
            self.host.record_the_gestures(false);
            return Err(ViewError::WouldNotBeAttached(attached));
        }
        let mut rect = rect_of(0, 0);
        // SAFETY: the view is live; `rect` lives through the call, which fills it.
        let sized = unsafe { view.getSize(&mut rect) } == kResultOk;
        let (width, height) = sized.then(|| size_of(&rect)).flatten().unwrap_or((0, 0));
        // SAFETY: the view is live; the call has no arguments.
        let can_resize = unsafe { view.canResize() } == kResultTrue;
        self.view = Some(OpenView { view, frame, known });
        Ok(ViewSize {
            width,
            height,
            can_resize,
        })
    }

    /// [`Self::open_view`] with a window of the program as the parent (what `winit` gives).
    pub fn open_view_in_a_window<W: HasWindowHandle>(&mut self, window: &W, scale: f64) -> Result<ViewSize, ViewError> {
        let parent = handle_of_a_window(window).ok_or(ViewError::NoParentHandle)?;
        self.open_view(parent, scale)
    }

    /// Takes the window out of its parent and lets go of it. (Nothing, if none is open.)
    pub fn close_view(&mut self) {
        if let Some(open) = self.view.take() {
            // SAFETY: the view is live; a window is taken out of its parent before its frame goes, as the SDK's hosts do.
            unsafe {
                open.view.removed();
                open.view.setFrame(std::ptr::null_mut());
            }
            self.host.record_the_gestures(false);
        }
    }

    /// How big the plugin's window says it is now, in pixels.
    pub fn view_size(&self) -> Option<(u32, u32)> {
        let open = self.view.as_ref()?;
        let mut rect = rect_of(0, 0);
        // SAFETY: the view is live; `rect` lives through the call, which fills it.
        (unsafe { open.view.getSize(&mut rect) } == kResultOk)
            .then(|| size_of(&rect))
            .flatten()
    }

    /// The size nearest to `width` by `height` that the window can be (a window that does not say is given the size it was asked for).
    pub fn adjust_view_size(&self, width: u32, height: u32) -> Option<(u32, u32)> {
        let open = self.view.as_ref()?;
        let mut rect = rect_of(width, height);
        // SAFETY: the view is live; `rect` lives through the call, which may change it.
        let said = unsafe { open.view.checkSizeConstraint(&mut rect) };
        Some(if said == kResultTrue {
            size_of(&rect).unwrap_or((width, height))
        } else {
            (width, height)
        })
    }

    /// Tells the window that it is `width` by `height` now. Whether it took it.
    pub fn set_view_size(&mut self, width: u32, height: u32) -> bool {
        let Some(open) = &self.view else {
            return false;
        };
        let mut rect = rect_of(width, height);
        // SAFETY: the view is live; `rect` lives through the call.
        let said = unsafe { open.view.onSize(&mut rect) };
        said == kResultOk
    }

    /// Tells the window that the screen's scale is this now.
    pub fn set_view_scale(&mut self, scale: f64) {
        let Some(open) = &self.view else {
            return;
        };
        if SCALE_IS_TOLD && let Some(support) = open.view.cast::<IPlugViewContentScaleSupport>() {
            // SAFETY: the view is live; the call has a number only.
            unsafe { support.setContentScaleFactor(scale as f32) };
        }
    }

    /// Gives the window a key that was pressed (a host forwards the keys that the plugin's window does not get itself). Whether the window took it.
    pub fn press_key(&mut self, key: char) -> bool {
        let Some(open) = &self.view else {
            return false;
        };
        let Ok(unit) = u16::try_from(u32::from(key)) else {
            return false;
        };
        // SAFETY: the view is live; the call has numbers only.
        let said = unsafe { open.view.onKeyDown(unit, 0, 0) };
        said == kResultTrue
    }

    /// What the window did since the last time, in order: the steps of a hand's gestures, the requests for another size (**answered here**: the window is told the size it will have,
    /// after the size has been brought inside what it allows), and whether its state changed or its values did. The edits are given to the component as well, so that the state saved
    /// after them has them. Nothing if no window is open.
    pub fn view_events(&mut self) -> Vec<ViewEvent> {
        if self.view.is_none() {
            return Vec::new();
        }
        // (The run loop of the host is looked at first: a plugin's window does what it is called for in it, and what it did is in what follows.)
        if let Some(open) = &self.view {
            open.frame.run_the_loop_at(std::time::Instant::now());
        }
        let mut events = Vec::new();
        let between = self.between_blocks();
        let (steps, dirty) = self.host.take_the_gestures();
        for step in steps {
            events.push(match step {
                Step::Begin(id) => ViewEvent::Begin { id },
                Step::Value(id, normalized) => {
                    let plain = self.plain_of(id, normalized);
                    if let Some(open) = &mut self.view {
                        open.known.insert(id, plain);
                    }
                    ViewEvent::Value { id, plain }
                }
                Step::End(id) => ViewEvent::End { id },
            });
        }
        let requests = self
            .view
            .as_ref()
            .map(|open| open.frame.take_the_requests())
            .unwrap_or_default();
        for (width, height) in requests {
            let (width, height) = (
                u32::try_from(width).unwrap_or(0).max(1),
                u32::try_from(height).unwrap_or(0).max(1),
            );
            let (width, height) = self.adjust_view_size(width, height).unwrap_or((width, height));
            self.set_view_size(width, height);
            events.push(ViewEvent::ResizeRequested { width, height });
        }
        let mut changed = dirty;
        if between.values_changed {
            changed = true;
            let differing = self.values_that_differ_from_the_known();
            if !differing.is_empty() {
                events.push(ViewEvent::ValuesChanged(differing));
            }
        }
        if changed {
            events.push(ViewEvent::StateChanged);
        }
        self.apply_waiting_changes();
        events
    }

    /// Runs one short silent block if changes of values wait for one (a component hears a parameter change only in a block), so that what the plugin saves next has them.
    pub fn apply_waiting_changes(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let frames = self.most_frames.min(FLUSH_BLOCK_FRAMES);
        // (A block that fails is the plugin's to answer for; the edit is not lost to the project, which has it from the event.)
        let _ = self.run_with_notes(&mut [], &mut [], frames, &[]);
    }

    /// Notes that the host gave the parameter `id` the plain value `plain`, so that the plugin saying it changed it to that is not a change.
    pub(crate) fn note_that_a_value_was_given(&mut self, id: u32, plain: f64) {
        if let Some(open) = &mut self.view {
            open.known.insert(id, plain);
        }
    }

    /// The plain values of the parameters the controller has now, that a hand could have moved.
    fn values_of_the_controller(&self) -> Vec<(u32, f64)> {
        let Some(controller) = &self.controller else {
            return Vec::new();
        };
        self.facts
            .parameters
            .iter()
            .filter(|parameter| !parameter.is_read_only)
            .map(|parameter| {
                // SAFETY: the controller is live; the call has numbers only.
                let normalized = unsafe { controller.getParamNormalized(parameter.id) };
                (parameter.id, self.plain_of(parameter.id, normalized))
            })
            .filter(|(_, plain)| plain.is_finite())
            .collect()
    }

    /// The values the controller has that are not what the host knew, which the host now knows.
    fn values_that_differ_from_the_known(&mut self) -> Vec<(u32, f64)> {
        let now = self.values_of_the_controller();
        let Some(open) = &mut self.view else {
            return Vec::new();
        };
        // (In the order of the plugin's list of parameters: the same every time.)
        let differing: Vec<(u32, f64)> = now
            .into_iter()
            .filter(|(id, plain)| open.known.get(id).is_none_or(|known| (known - plain).abs() > 1e-9))
            .collect();
        open.known.extend(differing.iter().copied());
        differing
    }
}
