// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The objects that the host gives to a plugin, as VST3 wants them: a stream for its state, the host (its name, and the handler a controller tells its edits to), the parameter
//! changes that go into a block and come out of it, and the list of events.
//!
//! They are COM objects: a plugin holds a pointer to one and calls its methods, so every method is an `unsafe fn` of a trait the `vst3` crate makes, and the pointers it is given
//! (`*mut` to a buffer, to a number) are the plugin's to keep valid for the call. **The whole body of such a method is the boundary** (`unsafe_op_in_unsafe_fn` is allowed in
//! this module for that reason): each one only copies numbers between the pointers and the fields of the object.
//!
//! None of these objects is used by two threads at once: a copy of a plugin is used by one thread (the program that runs it), and so they keep their fields in `Cell`s.

#![allow(unsafe_code, unsafe_op_in_unsafe_fn)]

use std::cell::{Cell, RefCell};
use std::ffi::c_void;

use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;
use vst3::{Class, ComWrapper};

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

/// The host (`IHostApplication`, the name it gives itself) and the handler of the controller (`IComponentHandler`): one object, since it is the same host.
pub(crate) struct HostObject {
    asked: RefCell<Asked>,
}

impl HostObject {
    pub(crate) fn new() -> ComWrapper<Self> {
        ComWrapper::new(Self {
            asked: RefCell::new(Asked::default()),
        })
    }

    /// What was asked since the last time, and forgets it.
    pub(crate) fn take_what_was_asked(&self) -> Asked {
        std::mem::take(&mut *self.asked.borrow_mut())
    }
}

impl Class for HostObject {
    type Interfaces = (IHostApplication, IComponentHandler);
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

    unsafe fn createInstance(&self, _class: *mut TUID, _interface: *mut TUID, object: *mut *mut c_void) -> tresult {
        // (A plugin may ask for a message or an attribute list to talk to its other half with. None of the plugins tried asked; the answer is "not here".)
        *object = std::ptr::null_mut();
        kNotImplemented
    }
}

impl IComponentHandlerTrait for HostObject {
    unsafe fn beginEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }

    unsafe fn performEdit(&self, id: ParamID, value: ParamValue) -> tresult {
        self.asked.borrow_mut().edits.push((id, value));
        kResultOk
    }

    unsafe fn endEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }

    unsafe fn restartComponent(&self, flags: int32) -> tresult {
        self.asked.borrow_mut().restart_flags |= flags;
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
