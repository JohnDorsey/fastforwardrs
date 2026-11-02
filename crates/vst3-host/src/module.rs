// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! A VST3 module that is loaded into the program. **Loading a library runs code that nobody here wrote**: whoever calls [`Module::open`] has to have been told by a person that this
//! module may be run (the editor's `plugin_trust` module is where that is decided), or be a child process that exists to find out whether it crashes (the scan).
//!
//! A module is a folder (`Name.vst3`, with the library in `Contents/<system>/`) or, for older plugins on Windows, the library itself with the name `Name.vst3`. The library gives
//! a **factory** (`GetPluginFactory`) that lists its classes and makes them. Each system has its own functions to call when the library is loaded and when it is let go; Windows'
//! are optional.
//!
//! Every `unsafe` block says why it is sound: **calling a function of the library** (sound if the library keeps the contract of the VST3 headers, the one thing that cannot be
//! checked, and why a library is only loaded after a person has said yes) and **calling a method of the factory** (sound while the library is loaded, which [`Loaded`] makes true
//! by letting go of the factory before it lets go of the library).

#![allow(unsafe_code)]

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use libloading::Library;
use thiserror::Error;
use vst3::Steinberg::*;
use vst3::{ComPtr, Interface};

use crate::classid::ClassId;
use crate::description::{ClassDescription, FactoryDescription};

/// The category of a class that is a plugin (the others are its controller, and classes for hosts of other times).
pub const AUDIO_MODULE_CLASS: &str = "Audio Module Class";

/// Why a module could not be opened.
#[derive(Debug, Error)]
pub enum OpenError {
    /// There is no library where a module of this system keeps it.
    #[error("the module has no library for this system where it should be ({0})")]
    NoLibrary(String),
    /// The system's loader refused: it is not a library of this system, a library that it needs is not there, or its start-up code failed.
    #[error("the library could not be loaded: {0}")]
    CouldNotBeLoaded(String),
    /// It loaded, and it is not a VST3 module: it does not have `GetPluginFactory`.
    #[error("the library is not a VST3 module: it has no GetPluginFactory")]
    NotAModule,
    /// The module's start-up function said no.
    #[error("the module's start-up function ({0}) said it could not start")]
    DidNotStart(&'static str),
    /// `GetPluginFactory` gave nothing.
    #[error("the module gave no factory")]
    NoFactory,
}

/// The function that a module is told it is loaded with, and the one that tells it it is let go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ending {
    /// `ExitDll` (Windows), `ModuleExit` (Linux) or `bundleExit` (macOS), by name.
    Function(&'static str),
    /// A system this host knows no end function of.
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    Nothing,
}

/// A loaded library, its factory, and what to do when they are let go.
struct Loaded {
    path: PathBuf,
    /// Let go of first.
    factory: Option<ComPtr<IPluginFactory>>,
    /// Let go of last: the library stays loaded as long as anything made from it lives.
    library: Option<Library>,
    ending: Ending,
}

// SAFETY: a module is used by one thread at a time (the program that runs the plugins keeps each copy of a plugin, and the module they are made from, to one thread, which VST3
// asks for: it has no rule for calls from several threads at the factory). The COM pointers it holds have no thread of their own.
unsafe impl Send for Loaded {}
// SAFETY: see above; nothing is mutated through a shared reference except by the plugin's own code under that rule.
unsafe impl Sync for Loaded {}

impl Drop for Loaded {
    fn drop(&mut self) {
        // (The factory first, then the library's end function, then the library.)
        self.factory = None;
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        let Ending::Function(name) = self.ending else {
            self.library = None;
            return;
        };
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        let Ending::Function(name) = self.ending;
        if let Some(library) = &self.library {
            // SAFETY: the function is the module's end function, found by the name that its system gives it; it takes nothing and gives a bool. A module that does not have it
            // is not asked.
            if let Ok(end) = unsafe { library.get::<unsafe extern "system" fn() -> bool>(name.as_bytes()) } {
                // SAFETY: calling it is what the module is made for; every object made from it has been let go of, since each holds this `Loaded`.
                let _ = unsafe { end() };
            }
        }
        self.library = None;
    }
}

/// A module that is loaded. Cheap to copy: the copies share the library, which is let go of when the last one (and everything made from the module) is gone.
#[derive(Clone)]
pub struct Module {
    loaded: Arc<Loaded>,
}

/// Where the library of the module `module` is, on this system, as the VST3 documentation says: a folder has it in `Contents/<system>/`; a file is the library.
pub fn library_file_of(module: &Path) -> Result<PathBuf, OpenError> {
    if module.is_file() {
        return Ok(module.to_path_buf());
    }
    let name = module
        .file_stem()
        .ok_or_else(|| OpenError::NoLibrary(module.display().to_string()))?
        .to_string_lossy()
        .into_owned();
    let (folders, file) = if cfg!(target_os = "windows") {
        let folders: &[&str] = if cfg!(target_arch = "aarch64") {
            &["arm64-win", "arm64ec-win"]
        } else {
            &["x86_64-win"]
        };
        (folders, format!("{name}.vst3"))
    } else if cfg!(target_os = "macos") {
        (&["MacOS"][..], name)
    } else {
        let folders: &[&str] = if cfg!(target_arch = "aarch64") {
            &["aarch64-linux"]
        } else {
            &["x86_64-linux"]
        };
        (folders, format!("{name}.so"))
    };
    for folder in folders {
        let candidate = module.join("Contents").join(folder).join(&file);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(OpenError::NoLibrary(
        module
            .join("Contents")
            .join(folders[0])
            .join(&file)
            .display()
            .to_string(),
    ))
}

impl Module {
    /// Loads the module at `path` (a `.vst3` folder, or a library) and asks it for its factory.
    pub fn open(path: &Path) -> Result<Self, OpenError> {
        let file = library_file_of(path)?;
        let (library, handle) = open_library(&file)?;
        let ending = start(&library, handle)?;
        // SAFETY: `GetPluginFactory` is the function every module exports; it takes nothing and gives a factory with a reference counted for the caller (or nothing).
        let get_factory =
            unsafe { library.get::<unsafe extern "system" fn() -> *mut IPluginFactory>(b"GetPluginFactory\0") }
                .map_err(|_| OpenError::NotAModule)?;
        // SAFETY: calling it is what the module is made for.
        let factory = unsafe { get_factory() };
        // SAFETY: the pointer is a factory with one reference for us, or null (then `from_raw` gives nothing).
        let factory = unsafe { ComPtr::from_raw(factory) }.ok_or(OpenError::NoFactory)?;
        Ok(Self {
            loaded: Arc::new(Loaded {
                path: path.to_path_buf(),
                factory: Some(factory),
                library: Some(library),
                ending,
            }),
        })
    }

    /// Where the module was opened from.
    pub fn path(&self) -> &Path {
        &self.loaded.path
    }

    pub(crate) fn factory(&self) -> &ComPtr<IPluginFactory> {
        self.loaded
            .factory
            .as_ref()
            .expect("a module has its factory until it is dropped")
    }

    /// What the factory says of itself.
    pub fn factory_description(&self) -> FactoryDescription {
        let mut info = PFactoryInfo {
            vendor: [0; 64],
            url: [0; 256],
            email: [0; 128],
            flags: 0,
        };
        // SAFETY: `info` is a `PFactoryInfo` that lives through the call, which fills it.
        let result = unsafe { self.factory().getFactoryInfo(&mut info) };
        if result != vst3::Steinberg::kResultOk {
            return FactoryDescription::default();
        }
        FactoryDescription {
            vendor: crate::description::text_of(&info.vendor),
            url: crate::description::text_of(&info.url),
        }
    }

    /// Every class of the factory (the plugins and everything else in it), in the factory's order. The names are in Unicode where the factory can give them so.
    pub fn classes(&self) -> Vec<ClassDescription> {
        crate::description::classes_of(self.factory())
    }

    /// Makes an object of the class `class` with the interface `I`.
    pub(crate) fn create<I: Interface>(&self, class: ClassId) -> Result<ComPtr<I>, String> {
        let tuid = class.to_tuid();
        let mut object: *mut c_void = std::ptr::null_mut();
        // SAFETY: both ids live through the call, which writes a pointer to `object`; the factory is alive (it is held by `self`).
        let result = unsafe {
            self.factory()
                .createInstance(tuid.as_ptr(), I::IID.as_ptr().cast(), &mut object)
        };
        if result != vst3::Steinberg::kResultOk || object.is_null() {
            return Err(format!("the factory could not make the class (it said {result:#x})"));
        }
        // SAFETY: the factory gave a pointer to an object of the interface asked for, with one reference for us.
        unsafe { ComPtr::from_raw(object.cast::<I>()) }.ok_or_else(|| "the factory gave nothing".to_owned())
    }
}

/// Loads the library file, and gives it with the handle of the system's loader for it (which Linux modules are told in their start-up call; null where there is none). On
/// Windows the libraries it needs are looked for beside it first.
fn open_library(file: &Path) -> Result<(Library, *mut c_void), OpenError> {
    #[cfg(windows)]
    {
        // SAFETY: loading a library runs its start-up code; see the module's text for why that is allowed.
        let loaded = unsafe {
            libloading::os::windows::Library::load_with_flags(
                file,
                libloading::os::windows::LOAD_WITH_ALTERED_SEARCH_PATH,
            )
        };
        loaded
            .map(|library| (Library::from(library), std::ptr::null_mut()))
            .map_err(|error| OpenError::CouldNotBeLoaded(error.to_string()))
    }
    #[cfg(unix)]
    {
        // SAFETY: loading a library runs its start-up code; see the module's text for why that is allowed.
        let loaded = unsafe { libloading::os::unix::Library::new(file) }
            .map_err(|error| OpenError::CouldNotBeLoaded(error.to_string()))?;
        let handle = loaded.into_raw();
        // SAFETY: the handle was just made by `into_raw` of that library, and nothing else owns it.
        let library = unsafe { libloading::os::unix::Library::from_raw(handle) };
        Ok((Library::from(library), handle))
    }
}

/// Calls the function a module of this system is started with, if it has it, and says what to call when it is let go.
fn start(library: &Library, handle: *mut c_void) -> Result<Ending, OpenError> {
    #[cfg(windows)]
    {
        let _ = handle;
        // SAFETY: `InitDll` takes nothing and gives a bool; a module that has none is not asked.
        if let Ok(init) = unsafe { library.get::<unsafe extern "system" fn() -> bool>(b"InitDll\0") } {
            // SAFETY: calling it is what the module is made for.
            if !unsafe { init() } {
                return Err(OpenError::DidNotStart("InitDll"));
            }
        }
        Ok(Ending::Function("ExitDll\0"))
    }
    #[cfg(target_os = "linux")]
    {
        // SAFETY: `ModuleEntry` takes the handle of the library and gives a bool; a module that has none is not asked (VST3 says every Linux module has it).
        if let Ok(entry) = unsafe { library.get::<unsafe extern "C" fn(*mut c_void) -> bool>(b"ModuleEntry\0") } {
            // SAFETY: calling it is what the module is made for; the handle is the one `dlopen` gave for this library, which stays valid while the library is loaded.
            if !unsafe { entry(handle) } {
                return Err(OpenError::DidNotStart("ModuleEntry"));
            }
        }
        Ok(Ending::Function("ModuleExit\0"))
    }
    #[cfg(target_os = "macos")]
    {
        let _ = handle;
        // SAFETY: `bundleEntry` takes the bundle (a CFBundleRef, which this host does not make: it gives none) and gives a bool; a module that has none is not asked.
        if let Ok(entry) = unsafe { library.get::<unsafe extern "C" fn(*mut c_void) -> bool>(b"bundleEntry\0") } {
            // SAFETY: calling it is what the module is made for.
            if !unsafe { entry(std::ptr::null_mut()) } {
                return Err(OpenError::DidNotStart("bundleEntry"));
            }
        }
        Ok(Ending::Function("bundleExit\0"))
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = (library, handle);
        Ok(Ending::Nothing)
    }
}
