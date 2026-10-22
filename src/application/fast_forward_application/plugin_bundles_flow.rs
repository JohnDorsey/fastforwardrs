// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the *Bundles* tab of the plugin manager does: the question before a download, the download on a thread of its own with its progress and a way to stop it, and what
//! is done when it is over (the plugins are trusted as the program's own, and found by a scan; the shaders are found by a scan, and need no trust). The downloads themselves are
//! `crate::plugin_bundles`. **There is one for each kind, and they do not wait for each other**: a person who wants only the shaders is not made to fetch the plugins.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use super::FastForwardApplication;
use crate::application::plugin_manager_bundles::{BundleKind, BundleRow, BundleState, size_in_words};
use crate::plugin_bundles::{
    CANCELLED, Curl, Fetcher, Manifest, Outcome, Progress, ShaderManifest, ShaderOutcome, bytes_to_download,
    fetch_plugins, fetch_shaders, fetched_plugins_folder, fetched_shaders_folder, is_fetched, shader_bytes_to_download,
    shaders_are_fetched, this_platform,
};

/// What a download of plugins or of shaders is doing.
#[derive(Default)]
pub(super) enum BundleDownload {
    #[default]
    Idle,
    /// The question that comes before it is showing.
    Asking,
    Running(RunningDownload),
    /// What the last download said, until it is dismissed.
    Done(String),
    Failed(String),
}

pub(super) struct RunningDownload {
    progress: Progress,
    messages: Receiver<Message>,
    cancel: Arc<AtomicBool>,
}

/// What a download made.
enum Made {
    Plugins(Outcome),
    Shaders(ShaderOutcome),
}

enum Message {
    Progress(Progress),
    Finished(Result<Made, String>),
}

/// The *Bundles* tab's state, and what the tests put in place of the network, the system and the folders.
#[derive(Default)]
pub(super) struct Bundles {
    /// The download of the sound plugins.
    pub(super) download: BundleDownload,
    /// The download of the shaders.
    pub(super) shader_download: BundleDownload,
    /// What fetches (`None`: the system's `curl`, which refuses what is not https).
    pub(super) fetcher: Option<Arc<dyn Fetcher>>,
    /// Where the plugins are put (`None`: the folder of the programs' data of the person).
    pub(super) folder: Option<PathBuf>,
    /// Where the shaders are put (`None`: the folder of the programs' data of the person).
    pub(super) shader_folder: Option<PathBuf>,
    /// The manifest of plugins (`None`: the one in the program).
    pub(super) manifest: Option<Manifest>,
    /// The manifest of shaders (`None`: the one in the program).
    pub(super) shader_manifest: Option<ShaderManifest>,
    /// The system the builds of the plugins are for (`None`: this one).
    pub(super) platform: Option<&'static str>,
}

impl Bundles {
    fn download_of(&mut self, kind: BundleKind) -> &mut BundleDownload {
        match kind {
            BundleKind::SoundPlugins => &mut self.download,
            BundleKind::Shaders => &mut self.shader_download,
        }
    }

    fn the_download_of(&self, kind: BundleKind) -> &BundleDownload {
        match kind {
            BundleKind::SoundPlugins => &self.download,
            BundleKind::Shaders => &self.shader_download,
        }
    }
}

/// The sites that `addresses` are on, each once, in the order they come in.
fn hosts_of<'a>(addresses: impl Iterator<Item = &'a String>) -> String {
    let mut hosts: Vec<String> = Vec::new();
    for address in addresses {
        if let Some(host) = address.strip_prefix("https://").and_then(|rest| rest.split('/').next())
            && !hosts.iter().any(|known| known == host)
        {
            hosts.push(host.to_owned());
        }
    }
    hosts.join(", ")
}

impl FastForwardApplication {
    fn manifest_of_the_bundles(&self) -> Manifest {
        self.environment
            .plugin_search
            .bundles
            .manifest
            .clone()
            .unwrap_or_else(Manifest::embedded)
    }

    fn manifest_of_the_shader_bundles(&self) -> ShaderManifest {
        self.environment
            .plugin_search
            .bundles
            .shader_manifest
            .clone()
            .unwrap_or_else(ShaderManifest::embedded)
    }

    fn platform_of_the_bundles(&self) -> Option<&'static str> {
        self.environment.plugin_search.bundles.platform.or_else(this_platform)
    }

    /// Where the plugins that the tab fetches are put.
    pub(super) fn folder_of_the_bundles(&self) -> Option<PathBuf> {
        self.environment
            .plugin_search
            .bundles
            .folder
            .clone()
            .or_else(fetched_plugins_folder)
    }

    /// Where the shaders that the tab fetches are put.
    pub(super) fn folder_of_the_shader_bundles(&self) -> Option<PathBuf> {
        self.environment
            .plugin_search
            .bundles
            .shader_folder
            .clone()
            .or_else(fetched_shaders_folder)
    }

    /// Where what is fetched of this kind is put.
    pub(super) fn folder_of_the_bundles_of(&self, kind: BundleKind) -> Option<PathBuf> {
        match kind {
            BundleKind::SoundPlugins => self.folder_of_the_bundles(),
            BundleKind::Shaders => self.folder_of_the_shader_bundles(),
        }
    }

    /// What the *Bundles* tab shows of one kind: each set, whether it is fetched, how big the download is, and where it comes from.
    pub(super) fn what_the_bundles_tab_shows(&self, kind: BundleKind) -> (Vec<BundleRow>, bool, u64, String) {
        match kind {
            BundleKind::SoundPlugins => self.what_the_bundles_tab_shows_of_plugins(),
            BundleKind::Shaders => self.what_the_bundles_tab_shows_of_shaders(),
        }
    }

    fn what_the_bundles_tab_shows_of_plugins(&self) -> (Vec<BundleRow>, bool, u64, String) {
        let manifest = self.manifest_of_the_bundles();
        let platform = self.platform_of_the_bundles();
        let folder = self.folder_of_the_bundles();
        let rows: Vec<BundleRow> = manifest
            .sets
            .iter()
            .map(|set| BundleRow {
                name: set.name.clone(),
                version: set.version.clone(),
                contents: set.plugins.join(", "),
                count: set.plugins.len(),
                authors: set.authors.join(", "),
                licence: set.licence.clone(),
                source: set.source.url.clone(),
                has_build: platform.is_some_and(|platform| set.platforms.contains_key(platform)),
                is_fetched: platform
                    .zip(folder.as_deref())
                    .is_some_and(|(platform, folder)| is_fetched(set, platform, folder)),
            })
            .collect();
        let total = platform.map_or(0, |platform| bytes_to_download(&manifest, platform));
        let addresses: Vec<String> = manifest
            .sets
            .iter()
            .filter_map(|set| {
                platform
                    .and_then(|platform| set.platforms.get(platform))
                    .map(|build| (set, build))
            })
            .flat_map(|(set, build)| {
                std::iter::once(build.url.clone()).chain(set.licence_texts.iter().map(|text| text.url.clone()))
            })
            .collect();
        let has_builds = rows.iter().any(|row| row.has_build);
        (rows, has_builds, total, hosts_of(addresses.iter()))
    }

    fn what_the_bundles_tab_shows_of_shaders(&self) -> (Vec<BundleRow>, bool, u64, String) {
        let manifest = self.manifest_of_the_shader_bundles();
        let folder = self.folder_of_the_shader_bundles();
        let rows: Vec<BundleRow> = manifest
            .sets
            .iter()
            .map(|set| {
                let of_each_shape = ["filter", "generator", "transition"]
                    .iter()
                    .filter_map(|shape| {
                        let count = set.shaders.iter().filter(|shader| shader.shape == *shape).count();
                        (count > 0).then(|| format!("{count} {shape}{}", if count == 1 { "" } else { "s" }))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                BundleRow {
                    name: set.name.clone(),
                    version: set.version.clone(),
                    contents: format!("{} shaders: {of_each_shape}", set.shaders.len()),
                    count: set.shaders.len(),
                    authors: set.authors.join(", "),
                    licence: set.licence.clone(),
                    source: set.source.url.clone(),
                    has_build: true,
                    is_fetched: folder.as_deref().is_some_and(|folder| shaders_are_fetched(set, folder)),
                }
            })
            .collect();
        let addresses: Vec<String> = manifest
            .sets
            .iter()
            .flat_map(|set| {
                set.shaders
                    .iter()
                    .map(|shader| shader.url.clone())
                    .chain(set.licence_texts.iter().map(|text| text.url.clone()))
            })
            .collect();
        let has_builds = !rows.is_empty();
        (
            rows,
            has_builds,
            shader_bytes_to_download(&manifest),
            hosts_of(addresses.iter()),
        )
    }

    /// The button was pressed: the question is shown (nothing is downloaded yet).
    pub(super) fn ask_whether_to_download_the_bundles(&mut self, kind: BundleKind) {
        let download = self.environment.plugin_search.bundles.download_of(kind);
        if matches!(download, BundleDownload::Idle) {
            *download = BundleDownload::Asking;
        }
    }

    pub(super) fn do_not_download_the_bundles(&mut self, kind: BundleKind) {
        let download = self.environment.plugin_search.bundles.download_of(kind);
        if matches!(download, BundleDownload::Asking) {
            *download = BundleDownload::Idle;
        }
    }

    /// The question was answered yes: what is of this kind is fetched on a thread of its own. Only after the question: a download that nobody asked about is never begun.
    pub(super) fn download_the_bundles(&mut self, kind: BundleKind) {
        if !matches!(
            self.environment.plugin_search.bundles.the_download_of(kind),
            BundleDownload::Asking
        ) {
            return;
        }
        let Some(folder) = self.folder_of_the_bundles_of(kind) else {
            *self.environment.plugin_search.bundles.download_of(kind) =
                BundleDownload::Failed("there is no folder to keep what is fetched in".to_owned());
            return;
        };
        let fetcher: Arc<dyn Fetcher> = self
            .environment
            .plugin_search
            .bundles
            .fetcher
            .clone()
            .unwrap_or_else(|| Arc::new(Curl::https_only()));
        // What is to be done on the thread, and how many bytes it is.
        enum Job {
            Plugins(Manifest, &'static str),
            Shaders(ShaderManifest),
        }
        let (job, total) = match kind {
            BundleKind::SoundPlugins => {
                let Some(platform) = self.platform_of_the_bundles() else {
                    self.environment.plugin_search.bundles.download =
                        BundleDownload::Failed("there are no builds of the plugins for this computer".to_owned());
                    return;
                };
                let manifest = self.manifest_of_the_bundles();
                let total = bytes_to_download(&manifest, platform);
                (Job::Plugins(manifest, platform), total)
            }
            BundleKind::Shaders => {
                let manifest = self.manifest_of_the_shader_bundles();
                let total = shader_bytes_to_download(&manifest);
                (Job::Shaders(manifest), total)
            }
        };
        let (sender, messages) = mpsc::channel();
        let sender = Arc::new(Mutex::new(sender));
        let cancel = Arc::new(AtomicBool::new(false));
        let wake_up = self.environment.wake_up.clone();
        let (cancel_for_the_thread, sender_for_the_thread) = (Arc::clone(&cancel), Arc::clone(&sender));
        let started = thread::Builder::new()
            .name(
                match kind {
                    BundleKind::SoundPlugins => "fetching plugins",
                    BundleKind::Shaders => "fetching shaders",
                }
                .to_owned(),
            )
            .spawn(move || {
                let send = |message: Message| {
                    if let Ok(sender) = sender_for_the_thread.lock() {
                        let _ = sender.send(message);
                    }
                    wake_up();
                };
                let cancelled = || cancel_for_the_thread.load(Ordering::SeqCst);
                let result = match job {
                    Job::Plugins(manifest, platform) => fetch_plugins(
                        &manifest,
                        platform,
                        &folder,
                        &*fetcher,
                        &mut |progress| send(Message::Progress(progress.clone())),
                        &cancelled,
                    )
                    .map(Made::Plugins),
                    Job::Shaders(manifest) => fetch_shaders(
                        &manifest,
                        &folder,
                        &*fetcher,
                        &mut |progress| send(Message::Progress(progress.clone())),
                        &cancelled,
                    )
                    .map(Made::Shaders),
                };
                send(Message::Finished(result));
            });
        *self.environment.plugin_search.bundles.download_of(kind) = match started {
            Ok(_) => BundleDownload::Running(RunningDownload {
                progress: Progress {
                    step: "Starting".to_owned(),
                    bytes_done: 0,
                    bytes_total: total,
                },
                messages,
                cancel,
            }),
            Err(error) => BundleDownload::Failed(format!("the download could not be started: {error}")),
        };
    }

    /// Whether a download of plugins is begun and not over.
    #[cfg(test)]
    pub(super) fn bundle_download_is_going_on(&self) -> bool {
        matches!(
            self.environment.plugin_search.bundles.download,
            BundleDownload::Running(_)
        )
    }

    /// Whether a download of shaders is begun and not over.
    #[cfg(test)]
    pub(super) fn shader_download_is_going_on(&self) -> bool {
        matches!(
            self.environment.plugin_search.bundles.shader_download,
            BundleDownload::Running(_)
        )
    }

    /// The Cancel button of a download that is going on.
    pub(super) fn cancel_the_download_of_the_bundles(&mut self, kind: BundleKind) {
        if let BundleDownload::Running(running) = self.environment.plugin_search.bundles.the_download_of(kind) {
            running.cancel.store(true, Ordering::SeqCst);
        }
    }

    pub(super) fn dismiss_what_the_download_said(&mut self, kind: BundleKind) {
        let download = self.environment.plugin_search.bundles.download_of(kind);
        if matches!(download, BundleDownload::Done(_) | BundleDownload::Failed(_)) {
            *download = BundleDownload::Idle;
        }
    }

    /// Takes what the downloads have said since the last frame; when one is over, what it fetched is found.
    pub(super) fn take_what_the_download_of_the_bundles_said(&mut self) {
        for kind in [BundleKind::SoundPlugins, BundleKind::Shaders] {
            self.take_what_the_download_of_a_kind_said(kind);
        }
    }

    fn take_what_the_download_of_a_kind_said(&mut self, kind: BundleKind) {
        let mut finished = None;
        if let BundleDownload::Running(running) = self.environment.plugin_search.bundles.download_of(kind) {
            loop {
                match running.messages.try_recv() {
                    Ok(Message::Progress(progress)) => running.progress = progress,
                    Ok(Message::Finished(result)) => {
                        finished = Some(result);
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        finished = Some(Err("the download ended without an answer".to_owned()));
                        break;
                    }
                }
            }
        }
        let Some(result) = finished else {
            return;
        };
        let after = match result {
            Ok(made) => {
                // (A plugin is code, and what the program fetched itself is trusted as its own; a shader is data, and nothing is trusted.)
                if matches!(made, Made::Plugins(_))
                    && let Some(folder) = self.folder_of_the_bundles_of(kind)
                {
                    self.trust_the_plugins_bundled_in(&folder);
                }
                self.start_scanning_plugins();
                let size = size_in_words(self.what_the_bundles_tab_shows(kind).2);
                BundleDownload::Done(match made {
                    Made::Plugins(outcome) => format!(
                        "Fetched {} plugin{} ({size}): {}. They are looked for now, and are in the menu of the + button once they are found.",
                        outcome.plugin_files.len(),
                        if outcome.plugin_files.len() == 1 { "" } else { "s" },
                        outcome.sets.join(", "),
                    ),
                    Made::Shaders(outcome) => format!(
                        "Fetched {} shader{} ({size}): {}. They are looked for now, and are in the menu of the + button (the filters) and in New, then Generator, on a track (the generators) once they are found.",
                        outcome.files,
                        if outcome.files == 1 { "" } else { "s" },
                        outcome.sets.join(", "),
                    ),
                })
            }
            Err(error) if error == CANCELLED => BundleDownload::Idle,
            Err(error) => BundleDownload::Failed(error),
        };
        *self.environment.plugin_search.bundles.download_of(kind) = after;
    }
}

/// What the tab says of the download, for the window.
pub(super) fn state_of(download: &BundleDownload) -> BundleState<'_> {
    match download {
        BundleDownload::Idle => BundleState::Idle,
        BundleDownload::Asking => BundleState::Asking,
        BundleDownload::Running(running) => BundleState::Running(&running.progress),
        BundleDownload::Done(message) => BundleState::Done(message),
        BundleDownload::Failed(message) => BundleState::Failed(message),
    }
}
