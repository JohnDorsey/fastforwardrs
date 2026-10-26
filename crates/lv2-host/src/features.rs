// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the host gives a plugin when it is made: the map of URIs to numbers (`urid:map`, `urid:unmap`), the options (the sample rate and the lengths of the blocks), the
//! worker's `schedule` and the features that are only a yes. A plugin that requires a feature that is not here is not made.
//!
//! The data that the features point to is **kept in this struct, on the heap, and not moved** for as long as it is alive: the plugin keeps the pointers it was given (it may
//! call `map` for as long as it lives), and so a [`FeatureSet`] is kept for as long as the instance it was made for.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::{Arc, Mutex, PoisonError};

use crate::abi::{self, RawFeature, RawOption, RawUridMap, RawUridUnmap, RawWorkerSchedule};
use crate::worker::WorkerShared;

/// The numbers that URIs are given, for the plugins of one host. (Number 0 is "no URI": the first is 1.) A URI that is given a number keeps it for as long as the table lives, and
/// the pointer that `unmap` gives for it stays good as long.
#[derive(Default)]
pub(crate) struct UridTable {
    inside: Mutex<Inside>,
}

#[derive(Default)]
struct Inside {
    by_uri: HashMap<String, u32>,
    /// The URI of number `n` is `uris[n - 1]`. (A `CString` keeps its characters on the heap: they do not move when this list grows.)
    uris: Vec<CString>,
}

impl UridTable {
    /// The number of `uri`, which it is given if it has none.
    pub(crate) fn map(&self, uri: &str) -> u32 {
        let mut inside = self.inside.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(urid) = inside.by_uri.get(uri) {
            return *urid;
        }
        // (A URI with a zero byte in it is not one that a plugin can have written as a C string.)
        let Ok(text) = CString::new(uri) else {
            return 0;
        };
        inside.uris.push(text);
        let urid = u32::try_from(inside.uris.len()).unwrap_or(u32::MAX);
        inside.by_uri.insert(uri.to_owned(), urid);
        urid
    }

    /// The URI that has the number `urid`, as the pointer that `unmap` gives, if there is one.
    pub(crate) fn unmap(&self, urid: u32) -> Option<*const c_char> {
        let inside = self.inside.lock().unwrap_or_else(PoisonError::into_inner);
        let place = usize::try_from(urid).ok()?.checked_sub(1)?;
        inside.uris.get(place).map(|uri| uri.as_ptr())
    }

    /// The URI that has the number `urid`, as text.
    pub(crate) fn uri_of(&self, urid: u32) -> Option<String> {
        let inside = self.inside.lock().unwrap_or_else(PoisonError::into_inner);
        let place = usize::try_from(urid).ok()?.checked_sub(1)?;
        inside.uris.get(place).map(|uri| uri.to_string_lossy().into_owned())
    }
}

/// `urid:map`'s `map`.
unsafe extern "C" fn map_callback(handle: *mut c_void, uri: *const c_char) -> u32 {
    if handle.is_null() || uri.is_null() {
        return 0;
    }
    // SAFETY: `handle` is the pointer to the `UridTable` that this feature was made with, which the `FeatureSet` that holds the feature keeps alive (it holds the `Arc`), and the
    // plugin only has it for as long as that. `uri` is a C string that the plugin is required to give (a pointer that is not null, to bytes that end in a zero).
    let (table, uri) = unsafe { (&*(handle as *const UridTable), CStr::from_ptr(uri)) };
    table.map(&uri.to_string_lossy())
}

/// `urid:unmap`'s `unmap`.
unsafe extern "C" fn unmap_callback(handle: *mut c_void, urid: u32) -> *const c_char {
    if handle.is_null() {
        return std::ptr::null();
    }
    // SAFETY: as for `map_callback`: the table is alive for as long as the plugin has the feature.
    let table = unsafe { &*(handle as *const UridTable) };
    table.unmap(urid).unwrap_or(std::ptr::null())
}

/// `worker:schedule`'s `schedule_work`: the plugin gives the host something to do on another thread.
unsafe extern "C" fn schedule_work_callback(handle: *mut c_void, size: u32, data: *const c_void) -> i32 {
    if handle.is_null() {
        return 1;
    }
    let bytes: Vec<u8> = if size == 0 || data.is_null() {
        Vec::new()
    } else {
        // SAFETY: the plugin gives a pointer to `size` bytes that it keeps valid for the length of this call (the interface says the host copies them).
        unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size as usize) }.to_vec()
    };
    // SAFETY: `handle` is the pointer to the `WorkerShared` that the schedule feature was made with, which the worker, and the feature set that holds the feature, both keep
    // alive (each has an `Arc` of it).
    let shared = unsafe { &*(handle as *const WorkerShared) };
    i32::from(!shared.schedule(bytes))
}

/// What the options say, and where: they are pointed to by the options feature.
struct OptionValues {
    sample_rate: f32,
    min_block_length: i32,
    max_block_length: i32,
    nominal_block_length: i32,
    sequence_size: i32,
}

/// The features that were given to one plugin, and what they point to.
pub(crate) struct FeatureSet {
    /// (Held so that the table that the features point to lives as long as they do.)
    _table: Arc<UridTable>,
    // (The boxes: what the features point to. Never moved or changed after they are made.)
    _map: Box<RawUridMap>,
    _unmap: Box<RawUridUnmap>,
    _option_values: Box<OptionValues>,
    _options: Box<[RawOption]>,
    _schedule: Option<Box<RawWorkerSchedule>>,
    _uris: Vec<CString>,
    _features: Vec<RawFeature>,
    /// The null-terminated list that `instantiate` takes.
    pointers: Vec<*const RawFeature>,
}

// SAFETY: a `FeatureSet` is a bundle of heap data and raw pointers into that same data (and to the table, which is `Send` and `Sync`); nothing in it is tied to a thread.
unsafe impl Send for FeatureSet {}

impl FeatureSet {
    /// The features for a plugin that lists `wanted` (what it requires and what it only likes), made at `sample_rate` for blocks of at most `max_block` frames. `worker` is the
    /// state of the worker if the plugin has one to give work to.
    pub(crate) fn new(
        table: &Arc<UridTable>,
        wanted: &[String],
        sample_rate: f64,
        max_block: u32,
        worker: Option<&Arc<WorkerShared>>,
    ) -> Self {
        let table_pointer = Arc::as_ptr(table) as *mut c_void;
        let map = Box::new(RawUridMap {
            handle: table_pointer,
            map: Some(map_callback),
        });
        let unmap = Box::new(RawUridUnmap {
            handle: table_pointer,
            unmap: Some(unmap_callback),
        });
        let max_block_length = i32::try_from(max_block).unwrap_or(i32::MAX);
        let option_values = Box::new(OptionValues {
            sample_rate: sample_rate as f32,
            min_block_length: 1,
            max_block_length,
            nominal_block_length: max_block_length,
            sequence_size: i32::try_from(crate::instance::ATOM_BUFFER_BYTES).unwrap_or(i32::MAX),
        });
        let float = table.map(abi::ATOM_FLOAT);
        let int = table.map(abi::ATOM_INT);
        let option = |key: &str, type_: u32, value: *const c_void| RawOption {
            context: 0,
            subject: 0,
            key: table.map(key),
            size: 4,
            type_,
            value,
        };
        let options: Box<[RawOption]> = vec![
            option(
                abi::PARAM_SAMPLE_RATE,
                float,
                (&raw const option_values.sample_rate).cast(),
            ),
            option(
                abi::BUFSZ_MIN_BLOCK_LENGTH,
                int,
                (&raw const option_values.min_block_length).cast(),
            ),
            option(
                abi::BUFSZ_MAX_BLOCK_LENGTH,
                int,
                (&raw const option_values.max_block_length).cast(),
            ),
            option(
                abi::BUFSZ_NOMINAL_BLOCK_LENGTH,
                int,
                (&raw const option_values.nominal_block_length).cast(),
            ),
            option(
                abi::BUFSZ_SEQUENCE_SIZE,
                int,
                (&raw const option_values.sequence_size).cast(),
            ),
            // (The list ends with an option of all zeros.)
            RawOption {
                context: 0,
                subject: 0,
                key: 0,
                size: 0,
                type_: 0,
                value: std::ptr::null(),
            },
        ]
        .into_boxed_slice();
        let schedule = worker.map(|shared| {
            Box::new(RawWorkerSchedule {
                handle: Arc::as_ptr(shared) as *mut c_void,
                schedule_work: Some(schedule_work_callback),
            })
        });

        // The features that a plugin is given: the three that every host gives, the ones with no data that the plugin lists and this host supports, and the worker if the plugin
        // lists it. (A feature that is listed and not supported is not given: if it is required the plugin is not made at all, before this.)
        let mut names: Vec<&str> = vec![abi::URID_MAP, abi::URID_UNMAP, abi::OPTIONS, abi::BOUNDED_BLOCK_LENGTH];
        for feature in wanted {
            let feature = feature.as_str();
            if abi::SUPPORTED_FEATURES.contains(&feature)
                && !names.contains(&feature)
                && (feature != abi::WORKER_SCHEDULE || schedule.is_some())
            {
                names.push(feature);
            }
        }
        let uris: Vec<CString> = names.iter().filter_map(|name| CString::new(*name).ok()).collect();
        let features: Vec<RawFeature> = uris
            .iter()
            .map(|uri| {
                let data: *mut c_void = match uri.to_str().unwrap_or("") {
                    abi::URID_MAP => (&raw const *map).cast_mut().cast(),
                    abi::URID_UNMAP => (&raw const *unmap).cast_mut().cast(),
                    abi::OPTIONS => options.as_ptr().cast_mut().cast(),
                    abi::WORKER_SCHEDULE => schedule.as_ref().map_or(std::ptr::null_mut(), |schedule| {
                        (&raw const **schedule).cast_mut().cast()
                    }),
                    _ => std::ptr::null_mut(),
                };
                RawFeature {
                    uri: uri.as_ptr(),
                    data,
                }
            })
            .collect();
        let mut pointers: Vec<*const RawFeature> = features.iter().map(std::ptr::from_ref).collect();
        pointers.push(std::ptr::null());
        Self {
            _table: Arc::clone(table),
            _map: map,
            _unmap: unmap,
            _option_values: option_values,
            _options: options,
            _schedule: schedule,
            _uris: uris,
            _features: features,
            pointers,
        }
    }

    /// The null-terminated list of features that `instantiate`, `save` and `restore` take. Good for as long as this set is alive.
    pub(crate) fn as_pointer(&self) -> *const *const RawFeature {
        self.pointers.as_ptr()
    }
}
