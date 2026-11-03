// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The objects that the host gives to a plugin, as VST3 wants them: a stream for its state, the host (its name, and the handler a controller tells its edits to), the parameter
//! changes that go into a block and come out of it, the list of events, the frame that a plugin's window is put in, and the messages (`IMessage` and `IAttributeList`) that a plugin makes with the host to talk to its other half.
//!
//! They are COM objects: a plugin holds a pointer to one and calls its methods, so every method is an `unsafe fn` of a trait the `vst3` crate makes, and the pointers it is given
//! (`*mut` to a buffer, to a number) are the plugin's to keep valid for the call. **The whole body of such a method is the boundary** (`unsafe_op_in_unsafe_fn` is allowed in
//! this module for that reason): each one only copies numbers between the pointers and the fields of the object.
//!
//! None of these objects is used by two threads at once: a copy of a plugin is used by one thread (the program that runs it), and so they keep their fields in `Cell`s.

#![allow(unsafe_code, unsafe_op_in_unsafe_fn)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_void};
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use vst3::Steinberg::Linux::IEventHandlerTrait;
use vst3::Steinberg::Linux::{
    FileDescriptor, IEventHandler, IRunLoop, IRunLoopTrait, ITimerHandler, ITimerHandlerTrait, TimerInterval,
};
use vst3::Steinberg::Vst::IAttributeList_::AttrID;
use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;
use vst3::{Class, ComPtr, ComRef, ComWrapper};

/// A stream of bytes in memory (`IBStream`), which a plugin reads its state from or writes it to.
pub(crate) struct MemoryStream {
    bytes: RefCell<Vec<u8>>,
    position: Cell<usize>,
}

impl MemoryStream {
    pub(crate) fn new(bytes: Vec<u8>) -> ComWrapper<Self> {
        ComWrapper::new(Self {
            bytes: RefCell::new(bytes),
            position: Cell::new(0),
        })
    }

    /// What the stream holds.
    pub(crate) fn bytes(&self) -> Vec<u8> {
        self.bytes.borrow().clone()
    }

    /// Back to the start, to be read again.
    pub(crate) fn rewind(&self) {
        self.position.set(0);
    }
}

impl Class for MemoryStream {
    type Interfaces = (IBStream,);
}

impl IBStreamTrait for MemoryStream {
    unsafe fn read(&self, buffer: *mut c_void, count: int32, read: *mut int32) -> tresult {
        let bytes = self.bytes.borrow();
        let position = self.position.get();
        let count = usize::try_from(count)
            .unwrap_or(0)
            .min(bytes.len().saturating_sub(position));
        std::ptr::copy_nonoverlapping(bytes.as_ptr().add(position), buffer.cast::<u8>(), count);
        self.position.set(position + count);
        if !read.is_null() {
            *read = count as int32;
        }
        kResultOk
    }

    unsafe fn write(&self, buffer: *mut c_void, count: int32, written: *mut int32) -> tresult {
        let mut bytes = self.bytes.borrow_mut();
        let position = self.position.get();
        let count = usize::try_from(count).unwrap_or(0);
        if bytes.len() < position + count {
            bytes.resize(position + count, 0);
        }
        std::ptr::copy_nonoverlapping(buffer.cast::<u8>(), bytes.as_mut_ptr().add(position), count);
        self.position.set(position + count);
        if !written.is_null() {
            *written = count as int32;
        }
        kResultOk
    }

    unsafe fn seek(&self, offset: int64, mode: int32, result: *mut int64) -> tresult {
        let length = self.bytes.borrow().len() as i64;
        // (0 from the start, 1 from here, 2 from the end.)
        let place = match mode {
            0 => offset,
            1 => self.position.get() as i64 + offset,
            _ => length + offset,
        };
        if place < 0 || place > length {
            return kResultFalse;
        }
        self.position.set(place as usize);
        if !result.is_null() {
            *result = place;
        }
        kResultOk
    }

    unsafe fn tell(&self, position: *mut int64) -> tresult {
        *position = self.position.get() as i64;
        kResultOk
    }
}

/// What the plugin has asked of the host through its handler since it was last looked at.
#[derive(Debug, Default)]
pub(crate) struct Asked {
    /// The edits it made (a parameter and its new value, normalised), in order.
    pub(crate) edits: Vec<(u32, f64)>,
    /// The flags of every `restartComponent`, put together.
    pub(crate) restart_flags: i32,
}

/// One step of what a hand does in a plugin's window, as the controller tells it: the three calls of `IComponentHandler`, in the order they came.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Step {
    Begin(u32),
    /// The parameter and its new value (normalised).
    Value(u32, f64),
    End(u32),
}

/// What the plugin's window did since it was last looked at, and which is kept only while a window is open (no one else reads it).
#[derive(Debug, Default)]
struct Gestures {
    steps: Vec<Step>,
    /// Whether the plugin said that its state is not what was last saved (`IComponentHandler2::setDirty`).
    dirty: bool,
}

/// The host (`IHostApplication`, the name it gives itself) and the handler of the controller (`IComponentHandler`, and `IComponentHandler2` for "the state changed"): one object, since
/// it is the same host.
pub(crate) struct HostObject {
    asked: RefCell<Asked>,
    gestures: RefCell<Gestures>,
    /// Whether the steps are kept: a window is open.
    recording: Cell<bool>,
}

impl HostObject {
    pub(crate) fn new() -> ComWrapper<Self> {
        ComWrapper::new(Self {
            asked: RefCell::new(Asked::default()),
            gestures: RefCell::new(Gestures::default()),
            recording: Cell::new(false),
        })
    }

    /// Keeps the steps of the window's gestures from now on (or stops, and forgets those that were kept).
    pub(crate) fn record_the_gestures(&self, on: bool) {
        self.recording.set(on);
        if !on {
            *self.gestures.borrow_mut() = Gestures::default();
        }
    }

    /// The steps that were kept since the last time, and whether the plugin said its state is dirty, and forgets both.
    pub(crate) fn take_the_gestures(&self) -> (Vec<Step>, bool) {
        let taken = std::mem::take(&mut *self.gestures.borrow_mut());
        (taken.steps, taken.dirty)
    }

    fn note_a_step(&self, step: Step) {
        if self.recording.get() {
            self.gestures.borrow_mut().steps.push(step);
        }
    }

    /// What was asked since the last time, and forgets it.
    pub(crate) fn take_what_was_asked(&self) -> Asked {
        std::mem::take(&mut *self.asked.borrow_mut())
    }
}

impl Class for HostObject {
    type Interfaces = (IHostApplication, IComponentHandler, IComponentHandler2);
}

/// The name the host gives itself.
const HOST_NAME: &str = "fastforwardrs";

impl IHostApplicationTrait for HostObject {
    unsafe fn getName(&self, name: *mut String128) -> tresult {
        let name = &mut *name;
        let mut units = HOST_NAME.encode_utf16();
        for place in name.iter_mut() {
            *place = units.next().unwrap_or(0);
        }
        if let Some(last) = name.last_mut() {
            *last = 0;
        }
        kResultOk
    }

    unsafe fn createInstance(&self, class: *mut TUID, interface: *mut TUID, object: *mut *mut c_void) -> tresult {
        // (A plugin asks for a message or an attribute list to talk to its other half with: **a plugin built with DPF sends every edit of its window like that**. Anything else is "not here".)
        if object.is_null() {
            return kInvalidArgument;
        }
        *object = std::ptr::null_mut();
        if class.is_null() || interface.is_null() {
            return kInvalidArgument;
        }
        let (class, interface) = (&*class, &*interface);
        let made: Option<*mut c_void> = if *class == IMessage_iid && *interface == IMessage_iid {
            HostMessage::new()
                .to_com_ptr::<IMessage>()
                .map(|message| message.into_raw().cast())
        } else if *class == IAttributeList_iid && *interface == IAttributeList_iid {
            HostAttributeList::new()
                .to_com_ptr::<IAttributeList>()
                .map(|list| list.into_raw().cast())
        } else {
            None
        };
        match made {
            Some(pointer) => {
                *object = pointer;
                kResultOk
            }
            None => kNotImplemented,
        }
    }
}

impl IComponentHandlerTrait for HostObject {
    unsafe fn beginEdit(&self, id: ParamID) -> tresult {
        self.note_a_step(Step::Begin(id));
        kResultOk
    }

    unsafe fn performEdit(&self, id: ParamID, value: ParamValue) -> tresult {
        self.asked.borrow_mut().edits.push((id, value));
        self.note_a_step(Step::Value(id, value));
        kResultOk
    }

    unsafe fn endEdit(&self, id: ParamID) -> tresult {
        self.note_a_step(Step::End(id));
        kResultOk
    }

    unsafe fn restartComponent(&self, flags: int32) -> tresult {
        self.asked.borrow_mut().restart_flags |= flags;
        kResultOk
    }
}

impl IComponentHandler2Trait for HostObject {
    unsafe fn setDirty(&self, state: TBool) -> tresult {
        // (Only "it changed" is a change; the host decides itself when the state is saved, and so a plugin that says it is clean says nothing new.)
        if state != 0 && self.recording.get() {
            self.gestures.borrow_mut().dirty = true;
        }
        kResultOk
    }

    unsafe fn requestOpenEditor(&self, _name: FIDString) -> tresult {
        kNotImplemented
    }

    unsafe fn startGroupEdit(&self) -> tresult {
        kResultOk
    }

    unsafe fn finishGroupEdit(&self) -> tresult {
        kResultOk
    }
}

/// What an attribute of a message is.
#[derive(Clone, Debug, PartialEq)]
enum Attribute {
    Int(i64),
    Float(f64),
    /// Text in the units of VST3 (16 bits), **without** the zero that ends it.
    Text(Vec<u16>),
    Binary(Vec<u8>),
}

/// A list of attributes (`IAttributeList`): named numbers, floats, texts and bytes. A name that is given again is replaced, whatever it was.
pub(crate) struct HostAttributeList {
    attributes: RefCell<HashMap<String, Attribute>>,
}

impl HostAttributeList {
    pub(crate) fn new() -> ComWrapper<Self> {
        ComWrapper::new(Self {
            attributes: RefCell::new(HashMap::new()),
        })
    }

    /// The name that an attribute is given as, if it is one.
    unsafe fn name_of(id: AttrID) -> Option<String> {
        (!id.is_null()).then(|| CStr::from_ptr(id).to_string_lossy().into_owned())
    }

    fn set(&self, id: AttrID, value: Attribute) -> tresult {
        // SAFETY: the plugin gave a name that ends in a zero, as the interface says.
        match unsafe { Self::name_of(id) } {
            Some(name) => {
                self.attributes.borrow_mut().insert(name, value);
                kResultOk
            }
            None => kInvalidArgument,
        }
    }

    fn get<T>(&self, id: AttrID, take: impl FnOnce(&Attribute) -> Option<T>) -> Option<T> {
        // SAFETY: the plugin gave a name that ends in a zero, as the interface says.
        let name = unsafe { Self::name_of(id) }?;
        self.attributes.borrow().get(&name).and_then(take)
    }
}

impl Class for HostAttributeList {
    type Interfaces = (IAttributeList,);
}

impl IAttributeListTrait for HostAttributeList {
    unsafe fn setInt(&self, id: AttrID, value: int64) -> tresult {
        self.set(id, Attribute::Int(value))
    }

    unsafe fn getInt(&self, id: AttrID, value: *mut int64) -> tresult {
        match self.get(id, |attribute| match attribute {
            Attribute::Int(number) => Some(*number),
            _ => None,
        }) {
            Some(number) if !value.is_null() => {
                *value = number;
                kResultOk
            }
            _ => kResultFalse,
        }
    }

    unsafe fn setFloat(&self, id: AttrID, value: f64) -> tresult {
        self.set(id, Attribute::Float(value))
    }

    unsafe fn getFloat(&self, id: AttrID, value: *mut f64) -> tresult {
        match self.get(id, |attribute| match attribute {
            Attribute::Float(number) => Some(*number),
            _ => None,
        }) {
            Some(number) if !value.is_null() => {
                *value = number;
                kResultOk
            }
            _ => kResultFalse,
        }
    }

    unsafe fn setString(&self, id: AttrID, string: *const TChar) -> tresult {
        if string.is_null() {
            return kInvalidArgument;
        }
        let mut text = Vec::new();
        let mut place = string;
        while *place != 0 {
            text.push(*place);
            place = place.add(1);
        }
        self.set(id, Attribute::Text(text))
    }

    unsafe fn getString(&self, id: AttrID, string: *mut TChar, size_in_bytes: uint32) -> tresult {
        let Some(text) = self.get(id, |attribute| match attribute {
            Attribute::Text(text) => Some(text.clone()),
            _ => None,
        }) else {
            return kResultFalse;
        };
        // (Room is counted in units of 16 bits, and the zero that ends the text has to fit in it.)
        let room = (size_in_bytes / 2) as usize;
        if string.is_null() || room == 0 {
            return kResultFalse;
        }
        let kept = text.len().min(room - 1);
        std::ptr::copy_nonoverlapping(text.as_ptr(), string, kept);
        *string.add(kept) = 0;
        kResultOk
    }

    unsafe fn setBinary(&self, id: AttrID, data: *const c_void, size_in_bytes: uint32) -> tresult {
        let bytes = if size_in_bytes == 0 || data.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(data.cast::<u8>(), size_in_bytes as usize).to_vec()
        };
        self.set(id, Attribute::Binary(bytes))
    }

    unsafe fn getBinary(&self, id: AttrID, data: *mut *const c_void, size_in_bytes: *mut uint32) -> tresult {
        // (The pointer is to the bytes the list keeps, which stay where they are until the same name is given again: the interface says so.)
        let Some((pointer, size)) = (|| {
            let name = Self::name_of(id)?;
            match self.attributes.borrow().get(&name)? {
                Attribute::Binary(bytes) => Some((bytes.as_ptr().cast::<c_void>(), u32::try_from(bytes.len()).ok()?)),
                _ => None,
            }
        })() else {
            return kResultFalse;
        };
        if data.is_null() || size_in_bytes.is_null() {
            return kInvalidArgument;
        }
        *data = pointer;
        *size_in_bytes = size;
        kResultOk
    }
}

/// A message (`IMessage`): an id and a list of attributes, which a plugin sends to its other half (`IConnectionPoint::notify`).
pub(crate) struct HostMessage {
    id: RefCell<CString>,
    attributes: ComWrapper<HostAttributeList>,
}

impl HostMessage {
    pub(crate) fn new() -> ComWrapper<Self> {
        ComWrapper::new(Self {
            id: RefCell::new(CString::default()),
            attributes: HostAttributeList::new(),
        })
    }
}

impl Class for HostMessage {
    type Interfaces = (IMessage,);
}

impl IMessageTrait for HostMessage {
    unsafe fn getMessageID(&self) -> FIDString {
        // (The text is good until the id is set again, as the interface says.)
        self.id.borrow().as_ptr()
    }

    unsafe fn setMessageID(&self, id: FIDString) {
        if !id.is_null() {
            *self.id.borrow_mut() = CStr::from_ptr(id).to_owned();
        }
    }

    unsafe fn getAttributes(&self) -> *mut IAttributeList {
        self.attributes
            .as_com_ref::<IAttributeList>()
            .map_or(std::ptr::null_mut(), |list| list.as_ptr())
    }
}

/// A timer that a plugin asked the host's run loop for: its handler is called every `every`.
struct Timer {
    handler: ComPtr<ITimerHandler>,
    every: Duration,
    due: Instant,
}

/// A file descriptor that a plugin asked the host's run loop to look at: its handler is called when there is something to read.
struct DescriptorWatch {
    handler: ComPtr<IEventHandler>,
    /// (Only looked at on Linux, where a plugin's window asks for it.)
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    descriptor: FileDescriptor,
}

/// What a plugin has registered with the run loop of the host (`IRunLoop`): **on Linux a plugin's window has no loop of its own and is given the host's**, to be told when its timers
/// are due and when its connection to the display has something to read. (On the other systems a plugin does not ask; what it registers is kept and not looked at.)
#[derive(Default)]
struct RunLoop {
    timers: Vec<Timer>,
    descriptors: Vec<DescriptorWatch>,
}

/// The frame that a plugin's window is put in (`IPlugFrame`, and `IRunLoop`, which a plugin asks the frame for): the window asks it for another size, and the host does it when it looks
/// (see `Instance::view_events`).
pub(crate) struct FrameObject {
    /// The sizes asked for, `(width, height)`, in order.
    requests: RefCell<Vec<(i32, i32)>>,
    run_loop: RefCell<RunLoop>,
}

impl FrameObject {
    pub(crate) fn new() -> ComWrapper<Self> {
        ComWrapper::new(Self {
            requests: RefCell::new(Vec::new()),
            run_loop: RefCell::new(RunLoop::default()),
        })
    }

    /// Calls the handlers of the timers that are due at `now`, and (on Linux) those of the descriptors that have something to read. **The lists are copied first**: a handler may
    /// register or unregister anything, itself included, in its call.
    pub(crate) fn run_the_loop_at(&self, now: Instant) {
        let due: Vec<ComPtr<ITimerHandler>> = self
            .run_loop
            .borrow_mut()
            .timers
            .iter_mut()
            .filter(|timer| now >= timer.due)
            .map(|timer| {
                timer.due = now + timer.every;
                timer.handler.clone()
            })
            .collect();
        for handler in due {
            // SAFETY: the handler is live (the frame holds a reference to it until it is unregistered); the call has no arguments.
            unsafe { handler.onTimer() };
        }
        #[cfg(target_os = "linux")]
        self.look_at_the_descriptors();
    }

    /// Calls the handler of each descriptor that has something to read.
    #[cfg(target_os = "linux")]
    fn look_at_the_descriptors(&self) {
        let watches: Vec<(ComPtr<IEventHandler>, FileDescriptor)> = self
            .run_loop
            .borrow()
            .descriptors
            .iter()
            .map(|watch| (watch.handler.clone(), watch.descriptor))
            .collect();
        if watches.is_empty() {
            return;
        }
        let mut polled: Vec<libc::pollfd> = watches
            .iter()
            .map(|(_, descriptor)| libc::pollfd {
                fd: *descriptor,
                events: libc::POLLIN,
                revents: 0,
            })
            .collect();
        // SAFETY: `polled` is a list of as many `pollfd` as the count says and lives through the call; the wait is none, so the call only looks.
        let ready = unsafe { libc::poll(polled.as_mut_ptr(), polled.len() as libc::nfds_t, 0) };
        if ready <= 0 {
            return;
        }
        for ((handler, descriptor), polled) in watches.iter().zip(&polled) {
            if polled.revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
                // SAFETY: the handler is live (see above); the call has a number only.
                unsafe { handler.onFDIsSet(*descriptor) };
            }
        }
    }

    /// The sizes that were asked for since the last time, and forgets them.
    pub(crate) fn take_the_requests(&self) -> Vec<(i32, i32)> {
        std::mem::take(&mut *self.requests.borrow_mut())
    }
}

impl Class for FrameObject {
    type Interfaces = (IPlugFrame, IRunLoop);
}

impl IRunLoopTrait for FrameObject {
    unsafe fn registerEventHandler(&self, handler: *mut IEventHandler, descriptor: FileDescriptor) -> tresult {
        let Some(handler) = ComRef::from_raw(handler).map(|handler| handler.to_com_ptr()) else {
            return kInvalidArgument;
        };
        self.run_loop
            .borrow_mut()
            .descriptors
            .push(DescriptorWatch { handler, descriptor });
        kResultOk
    }

    unsafe fn unregisterEventHandler(&self, handler: *mut IEventHandler) -> tresult {
        self.run_loop
            .borrow_mut()
            .descriptors
            .retain(|watch| watch.handler.as_ptr() != handler);
        kResultOk
    }

    unsafe fn registerTimer(&self, handler: *mut ITimerHandler, milliseconds: TimerInterval) -> tresult {
        let Some(handler) = ComRef::from_raw(handler).map(|handler| handler.to_com_ptr()) else {
            return kInvalidArgument;
        };
        if milliseconds == 0 {
            return kInvalidArgument;
        }
        let every = Duration::from_millis(milliseconds);
        self.run_loop.borrow_mut().timers.push(Timer {
            handler,
            every,
            due: Instant::now() + every,
        });
        kResultOk
    }

    unsafe fn unregisterTimer(&self, handler: *mut ITimerHandler) -> tresult {
        self.run_loop
            .borrow_mut()
            .timers
            .retain(|timer| timer.handler.as_ptr() != handler);
        kResultOk
    }
}

impl IPlugFrameTrait for FrameObject {
    unsafe fn resizeView(&self, _view: *mut IPlugView, new_size: *mut ViewRect) -> tresult {
        if new_size.is_null() {
            return kInvalidArgument;
        }
        // (Only kept: the window is made that size when the host looks, on the thread that has the program's window, and the plugin is told then.)
        let rect = &*new_size;
        self.requests.borrow_mut().push((
            rect.right.saturating_sub(rect.left),
            rect.bottom.saturating_sub(rect.top),
        ));
        kResultOk
    }
}

/// The points of one parameter in a block (`IParamValueQueue`): the sample and the normalised value of each.
pub(crate) struct ParameterQueue {
    id: Cell<u32>,
    points: RefCell<Vec<(i32, f64)>>,
}

impl Class for ParameterQueue {
    type Interfaces = (IParamValueQueue,);
}

impl IParamValueQueueTrait for ParameterQueue {
    unsafe fn getParameterId(&self) -> ParamID {
        self.id.get()
    }

    unsafe fn getPointCount(&self) -> int32 {
        self.points.borrow().len() as int32
    }

    unsafe fn getPoint(&self, index: int32, sample: *mut int32, value: *mut ParamValue) -> tresult {
        match self.points.borrow().get(usize::try_from(index).unwrap_or(usize::MAX)) {
            Some((at, point)) => {
                *sample = *at;
                *value = *point;
                kResultOk
            }
            None => kResultFalse,
        }
    }

    unsafe fn addPoint(&self, sample: int32, value: ParamValue, index: *mut int32) -> tresult {
        let mut points = self.points.borrow_mut();
        points.push((sample, value));
        if !index.is_null() {
            *index = points.len() as int32 - 1;
        }
        kResultOk
    }
}

/// The parameter changes of a block (`IParameterChanges`): the ones the host gives the plugin, or the ones it gives back. The queues are kept from block to block, so that a
/// block does not make objects.
pub(crate) struct ParameterChanges {
    /// The queues, and how many of them the block uses.
    queues: RefCell<Vec<ComWrapper<ParameterQueue>>>,
    used: Cell<usize>,
}

impl ParameterChanges {
    pub(crate) fn new() -> ComWrapper<Self> {
        ComWrapper::new(Self {
            queues: RefCell::new(Vec::new()),
            used: Cell::new(0),
        })
    }

    /// Empties it for the next block.
    pub(crate) fn clear(&self) {
        self.used.set(0);
    }

    /// Puts the changes `(parameter, sample, normalised value)` in, one queue for each parameter, the points of each in order of the sample (those of one sample in the order given).
    pub(crate) fn fill(&self, changes: &[(u32, i32, f64)]) {
        self.clear();
        let mut order: Vec<usize> = (0..changes.len()).collect();
        order.sort_by_key(|place| (changes[*place].0, changes[*place].1));
        for place in order {
            let (id, sample, value) = changes[place];
            let queue = self.queue_for(id);
            queue.points.borrow_mut().push((sample, value));
        }
    }

    fn queue_for(&self, id: u32) -> ComWrapper<ParameterQueue> {
        let used = self.used.get();
        let mut queues = self.queues.borrow_mut();
        if used > 0 && queues[used - 1].id.get() == id {
            return queues[used - 1].clone();
        }
        if queues.len() == used {
            queues.push(ComWrapper::new(ParameterQueue {
                id: Cell::new(id),
                points: RefCell::new(Vec::new()),
            }));
        }
        let queue = queues[used].clone();
        queue.id.set(id);
        queue.points.borrow_mut().clear();
        self.used.set(used + 1);
        queue
    }

    /// What the plugin gave: for each parameter, its last point's value (normalised).
    pub(crate) fn last_values(&self) -> Vec<(u32, f64)> {
        self.queues.borrow()[..self.used.get()]
            .iter()
            .filter_map(|queue| queue.points.borrow().last().map(|(_, value)| (queue.id.get(), *value)))
            .collect()
    }
}

impl Class for ParameterChanges {
    type Interfaces = (IParameterChanges,);
}

impl IParameterChangesTrait for ParameterChanges {
    unsafe fn getParameterCount(&self) -> int32 {
        self.used.get() as int32
    }

    unsafe fn getParameterData(&self, index: int32) -> *mut IParamValueQueue {
        match usize::try_from(index).ok().filter(|place| *place < self.used.get()) {
            Some(place) => self.queues.borrow()[place]
                .as_com_ref::<IParamValueQueue>()
                .map_or(std::ptr::null_mut(), |queue| queue.as_ptr()),
            None => std::ptr::null_mut(),
        }
    }

    unsafe fn addParameterData(&self, id: *const ParamID, index: *mut int32) -> *mut IParamValueQueue {
        // (A parameter that is added twice is given the queue it has.)
        let wanted = *id;
        let existing = self.queues.borrow()[..self.used.get()]
            .iter()
            .position(|queue| queue.id.get() == wanted);
        let place = match existing {
            Some(place) => place,
            None => {
                self.queue_for(wanted);
                self.used.get() - 1
            }
        };
        if !index.is_null() {
            *index = place as int32;
        }
        self.queues.borrow()[place]
            .as_com_ref::<IParamValueQueue>()
            .map_or(std::ptr::null_mut(), |queue| queue.as_ptr())
    }
}

/// The events of a block (`IEventList`).
pub(crate) struct EventList {
    events: RefCell<Vec<Event>>,
}

impl EventList {
    pub(crate) fn new() -> ComWrapper<Self> {
        ComWrapper::new(Self {
            events: RefCell::new(Vec::new()),
        })
    }

    pub(crate) fn clear(&self) {
        self.events.borrow_mut().clear();
    }

    pub(crate) fn push(&self, event: Event) {
        self.events.borrow_mut().push(event);
    }
}

impl Class for EventList {
    type Interfaces = (IEventList,);
}

impl IEventListTrait for EventList {
    unsafe fn getEventCount(&self) -> int32 {
        self.events.borrow().len() as int32
    }

    unsafe fn getEvent(&self, index: int32, event: *mut Event) -> tresult {
        match self.events.borrow().get(usize::try_from(index).unwrap_or(usize::MAX)) {
            Some(found) => {
                *event = *found;
                kResultOk
            }
            None => kResultFalse,
        }
    }

    unsafe fn addEvent(&self, event: *mut Event) -> tresult {
        self.events.borrow_mut().push(*event);
        kResultOk
    }
}
