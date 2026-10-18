// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The decisions: what has to be asked of the person, what the answers do, and **the one way a library may be loaded** ([`load_library`]), which refuses unless the answers are
//! yes.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::approvals::ApprovalStore;
use super::extraction::{ExtractionError, MOST_BYTES_OF_A_PLUGIN, PluginCache};
use crate::digest::Sha256Digest;
use crate::project::{LinkedLibrary, PluginLibraries, PluginLibrary, PluginLibrarySource};

/// What loads a library of native code. (Phase 7.4 makes the one that does; the tests have a fake.) **It is only ever given a file that [`load_library`] has let through.**
pub trait LibraryLoader {
    /// What a loaded library is.
    type Loaded;

    fn load(&mut self, file: &Path) -> Result<Self::Loaded, String>;
}

/// What was approved or declined: a linked file as it is (its path and what is in it), an embedded one by its name and what is in it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Subject {
    Linked { path: PathBuf, digest: Sha256Digest },
    Embedded { library: String, digest: Sha256Digest },
}

/// What the person is asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Question {
    /// A plugin that is a file somewhere is about to be run for the first time (the warning).
    Linked {
        library: String,
        path: PathBuf,
        digest: Sha256Digest,
    },
    /// A plugin that is inside the project file is about to be run (the severe one, asked every time the project is opened). `platform` is the build that would be loaded.
    Embedded {
        library: String,
        platform: String,
        size: usize,
        digest: Sha256Digest,
    },
}

impl Question {
    fn subject(&self) -> Subject {
        match self {
            Self::Linked { path, digest, .. } => Subject::Linked {
                path: path.clone(),
                digest: *digest,
            },
            Self::Embedded { library, digest, .. } => Subject::Embedded {
                library: library.clone(),
                digest: *digest,
            },
        }
    }

    /// The name of the library that is asked about.
    pub fn library(&self) -> &str {
        match self {
            Self::Linked { library, .. } | Self::Embedded { library, .. } => library,
        }
    }
}

/// What the person said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Load,
    DontLoad,
}

/// What has been answered while the project is open. **It lasts as long as the project does** and no longer: an embedded plugin is asked about again every time the project is
/// opened (and after a project is recovered from an autosave, which is opening it).
#[derive(Debug, Default)]
pub struct TrustSession {
    allowed: HashSet<Subject>,
    declined: HashSet<Subject>,
}

impl TrustSession {
    /// Records `answer` to `question`. A *yes* to a linked plugin is also written to `approvals`, which remembers it by the path and the hash of the file; a *yes* to an
    /// embedded plugin is **only** for this session.
    pub fn answer(&mut self, question: &Question, answer: Answer, approvals: &mut ApprovalStore) {
        let subject = question.subject();
        match answer {
            Answer::Load => {
                if let Question::Linked { path, digest, .. } = question {
                    approvals.approve(path, *digest);
                }
                self.declined.remove(&subject);
                self.allowed.insert(subject);
            }
            Answer::DontLoad => {
                self.allowed.remove(&subject);
                self.declined.insert(subject);
            }
        }
    }

    /// Forgets every answer (another project was opened).
    pub fn forget_everything(&mut self) {
        self.allowed.clear();
        self.declined.clear();
    }

    fn is_allowed(&self, subject: &Subject) -> bool {
        self.allowed.contains(subject)
    }

    fn is_declined(&self, subject: &Subject) -> bool {
        self.declined.contains(subject)
    }
}

/// Why a library cannot be used at all, whatever the person says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unavailable {
    /// An embedded library has no build for this kind of computer: these are the kinds it has.
    NoBuildForThisPlatform {
        platforms: Vec<String>,
    },
    /// A linked library is not at any of the places it is said to be at.
    FileNotFound {
        tried: Vec<PathBuf>,
    },
    FileUnreadable {
        path: PathBuf,
        reason: String,
    },
    FileTooBig {
        path: PathBuf,
        bytes: u64,
    },
}

/// What the questions are, and what cannot be used.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WhatIsNeeded {
    /// To be asked, one after another, in the order of the libraries.
    pub questions: Vec<Question>,
    /// Libraries that cannot be used, by name.
    pub unavailable: Vec<(String, Unavailable)>,
}

/// Where a linked library is now and what is in it. The places it is looked for are the one it was at, and the one that is the same from the folder of the project file.
fn find_the_linked_file(
    link: &LinkedLibrary,
    project_directory: Option<&Path>,
) -> Result<(PathBuf, Sha256Digest), Unavailable> {
    let mut tried = vec![link.absolute_path.clone()];
    if let (Some(relative), Some(directory)) = (&link.relative_path, project_directory) {
        let there = directory.join(relative);
        if there != link.absolute_path {
            tried.push(there);
        }
    }
    let Some(path) = tried.iter().find(|candidate| candidate.is_file()).cloned() else {
        return Err(Unavailable::FileNotFound { tried });
    };
    let bytes = fs::metadata(&path).map_or(0, |metadata| metadata.len());
    if bytes > MOST_BYTES_OF_A_PLUGIN as u64 {
        return Err(Unavailable::FileTooBig { path, bytes });
    }
    match Sha256Digest::of_file(&path) {
        Ok(digest) => Ok((path, digest)),
        Err(error) => Err(Unavailable::FileUnreadable {
            path,
            reason: error.to_string(),
        }),
    }
}

/// What has to be asked of the person before the libraries of a project may be used, and which cannot be used. **Each library that is not answered for already** is asked about
/// once: a linked one that was approved (by this path and this file) is not, nor one that was answered in this session.
pub fn what_is_needed(
    libraries: &PluginLibraries,
    project_directory: Option<&Path>,
    platform: &str,
    approvals: &ApprovalStore,
    session: &TrustSession,
) -> WhatIsNeeded {
    let mut needed = WhatIsNeeded::default();
    for library in libraries {
        match library.source() {
            PluginLibrarySource::Linked(link) => match find_the_linked_file(link, project_directory) {
                Ok((path, digest)) => {
                    let subject = Subject::Linked {
                        path: path.clone(),
                        digest,
                    };
                    if !approvals.is_approved(&path, digest)
                        && !session.is_allowed(&subject)
                        && !session.is_declined(&subject)
                    {
                        needed.questions.push(Question::Linked {
                            library: library.name().to_owned(),
                            path,
                            digest,
                        });
                    }
                }
                Err(why) => needed.unavailable.push((library.name().to_owned(), why)),
            },
            PluginLibrarySource::Embedded(_) => match library.build_for(platform) {
                None => needed.unavailable.push((
                    library.name().to_owned(),
                    Unavailable::NoBuildForThisPlatform {
                        platforms: library.platforms().iter().map(|name| (*name).to_owned()).collect(),
                    },
                )),
                Some(build) => {
                    let subject = Subject::Embedded {
                        library: library.name().to_owned(),
                        digest: build.digest(),
                    };
                    if !session.is_allowed(&subject) && !session.is_declined(&subject) {
                        needed.questions.push(Question::Embedded {
                            library: library.name().to_owned(),
                            platform: platform.to_owned(),
                            size: build.bytes().len(),
                            digest: build.digest(),
                        });
                    }
                }
            },
        }
    }
    needed
}

/// The libraries that the command line has to **refuse** to run a project with, unless it is told that plugins are allowed: it cannot ask. These are the embedded ones that have
/// a build for this computer, and the linked ones that are not approved (or cannot be found: they cannot be approved). Each is a line that says which and why, for the message.
pub fn what_the_command_line_refuses(
    libraries: &PluginLibraries,
    project_directory: Option<&Path>,
    platform: &str,
    approvals: &ApprovalStore,
) -> Vec<String> {
    let mut lines = Vec::new();
    for library in libraries {
        match library.source() {
            PluginLibrarySource::Embedded(_) => {
                if library.build_for(platform).is_some() {
                    lines.push(format!(
                        "{} (inside the project file: a program that nobody has looked at)",
                        library.name()
                    ));
                }
            }
            PluginLibrarySource::Linked(link) => match find_the_linked_file(link, project_directory) {
                Ok((path, digest)) => {
                    if !approvals.is_approved(&path, digest) {
                        lines.push(format!(
                            "{} (the file {}, which has not been approved)",
                            library.name(),
                            path.display()
                        ));
                    }
                }
                Err(_) => lines.push(format!(
                    "{} (the file {}, which cannot be read, so it cannot have been approved)",
                    library.name(),
                    link.absolute_path.display()
                )),
            },
        }
    }
    lines
}

/// Why a library was not loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WhyNot {
    /// The person said no. What uses the library passes the sound through.
    Declined,
    /// Nobody has been asked yet.
    NotAsked,
    Unavailable(Unavailable),
    /// The plugin is not the one the project says it is, or it is too big, or it could not be written out.
    CouldNotBeWrittenOut(String),
    /// The loader said no.
    LoaderFailed(String),
}

/// Loads `library` with `loader` **if the person has said yes, and not otherwise.** A linked library is loaded where it is, if its path and what is in it have been approved; an
/// embedded one is written out by `cache` (checked against its recorded digest) and loaded from there, if this session has said yes to it.
pub fn load_library<Loader: LibraryLoader>(
    library: &PluginLibrary,
    project_directory: Option<&Path>,
    platform: &str,
    approvals: &ApprovalStore,
    session: &TrustSession,
    cache: &PluginCache,
    loader: &mut Loader,
) -> Result<Loader::Loaded, WhyNot> {
    let path = match library.source() {
        PluginLibrarySource::Linked(link) => {
            let (path, digest) = find_the_linked_file(link, project_directory).map_err(WhyNot::Unavailable)?;
            let subject = Subject::Linked {
                path: path.clone(),
                digest,
            };
            if session.is_declined(&subject) {
                return Err(WhyNot::Declined);
            }
            if !approvals.is_approved(&path, digest) && !session.is_allowed(&subject) {
                return Err(WhyNot::NotAsked);
            }
            path
        }
        PluginLibrarySource::Embedded(_) => {
            let Some(build) = library.build_for(platform) else {
                return Err(WhyNot::Unavailable(Unavailable::NoBuildForThisPlatform {
                    platforms: library.platforms().iter().map(|name| (*name).to_owned()).collect(),
                }));
            };
            let subject = Subject::Embedded {
                library: library.name().to_owned(),
                digest: build.digest(),
            };
            if session.is_declined(&subject) {
                return Err(WhyNot::Declined);
            }
            if !session.is_allowed(&subject) {
                return Err(WhyNot::NotAsked);
            }
            cache
                .extract(library.name(), build)
                .map_err(|error: ExtractionError| WhyNot::CouldNotBeWrittenOut(error.to_string()))?
        }
    };
    loader.load(&path).map_err(WhyNot::LoaderFailed)
}
