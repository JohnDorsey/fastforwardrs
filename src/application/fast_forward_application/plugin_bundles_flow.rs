// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the *Bundles* tab of the window of sound plugins does: the question before a download, the download on a thread of its own with its progress and a way to stop it, and what
//! is done when it is over (the plugins are trusted as the program's own, and found by a scan). The download itself is `crate::plugin_bundles`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use super::FastForwardApplication;
use crate::application::sound_plugins_bundles::{BundleRow, BundleState, size_in_words};
use crate::plugin_bundles::{
    CANCELLED, Curl, Fetcher, Manifest, Outcome, Progress, bytes_to_download, fetch_plugins, fetched_plugins_folder,
    is_fetched, this_platform,
};

/// What a download of plugins is doing.
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

enum Message {
    Progress(Progress),
    Finished(Result<Outcome, String>),
}

/// The *Bundles* tab's state, and what the tests put in place of the network, the system and the folder.
#[derive(Default)]
pub(super) struct Bundles {
    pub(super) download: BundleDownload,
    /// What fetches (`None`: the system's `curl`, which refuses what is not https).
    pub(super) fetcher: Option<Arc<dyn Fetcher>>,
    /// Where the plugins are put (`None`: the folder of the programs' data of the person).
    pub(super) folder: Option<PathBuf>,
    /// The manifest (`None`: the one in the program).
    pub(super) manifest: Option<Manifest>,
    /// The system the builds are for (`None`: this one).
    pub(super) platform: Option<&'static str>,
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

    /// What the *Bundles* tab shows: each set, whether it is fetched, how big the download is, and where it comes from.
    pub(super) fn what_the_bundles_tab_shows(&self) -> (Vec<BundleRow>, bool, u64, String) {
        let manifest = self.manifest_of_the_bundles();
        let platform = self.platform_of_the_bundles();
        let folder = self.folder_of_the_bundles();
        let rows: Vec<BundleRow> = manifest
            .sets
            .iter()
            .map(|set| BundleRow {
                name: set.name.clone(),
                version: set.version.clone(),
                plugins: set.plugins.clone(),
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
        let mut hosts: Vec<String> = Vec::new();
        for set in &manifest.sets {
            let Some(build) = platform.and_then(|platform| set.platforms.get(platform)) else {
                continue;
            };
            for address in std::iter::once(&build.url).chain(set.licence_texts.iter().map(|text| &text.url)) {
                if let Some(host) = address.strip_prefix("https://").and_then(|rest| rest.split('/').next())
                    && !hosts.iter().any(|known| known == host)
                {
                    hosts.push(host.to_owned());
                }
            }
        }
        let has_builds = rows.iter().any(|row| row.has_build);
        (rows, has_builds, total, hosts.join(", "))
    }

    /// The button was pressed: the question is shown (nothing is downloaded yet).
    pub(super) fn ask_whether_to_download_the_bundles(&mut self) {
        let bundles = &mut self.environment.plugin_search.bundles;
        if matches!(bundles.download, BundleDownload::Idle) {
            bundles.download = BundleDownload::Asking;
        }
    }

    pub(super) fn do_not_download_the_bundles(&mut self) {
        let bundles = &mut self.environment.plugin_search.bundles;
        if matches!(bundles.download, BundleDownload::Asking) {
            bundles.download = BundleDownload::Idle;
        }
    }

    /// The question was answered yes: the plugins are fetched on a thread of their own. Only after the question: a download that nobody asked about is never begun.
    pub(super) fn download_the_bundles(&mut self) {
        if !matches!(self.environment.plugin_search.bundles.download, BundleDownload::Asking) {
            return;
        }
        let (Some(platform), Some(folder)) = (self.platform_of_the_bundles(), self.folder_of_the_bundles()) else {
            self.environment.plugin_search.bundles.download = BundleDownload::Failed(
                "there are no builds of the plugins for this computer, or no folder to keep them in".to_owned(),
            );
            return;
        };
        let manifest = self.manifest_of_the_bundles();
        let fetcher: Arc<dyn Fetcher> = self
            .environment
            .plugin_search
            .bundles
            .fetcher
            .clone()
            .unwrap_or_else(|| Arc::new(Curl::https_only()));
        let (sender, messages) = mpsc::channel();
        let sender = Arc::new(Mutex::new(sender));
        let cancel = Arc::new(AtomicBool::new(false));
        let wake_up = self.environment.wake_up.clone();
        let total = bytes_to_download(&manifest, platform);
        let (cancel_for_the_thread, sender_for_the_thread) = (Arc::clone(&cancel), Arc::clone(&sender));
        let started = thread::Builder::new()
            .name("fetching plugins".to_owned())
            .spawn(move || {
                let send = |message: Message| {
                    if let Ok(sender) = sender_for_the_thread.lock() {
                        let _ = sender.send(message);
                    }
                    wake_up();
                };
                let result = fetch_plugins(
                    &manifest,
                    platform,
                    &folder,
                    &*fetcher,
                    &mut |progress| send(Message::Progress(progress.clone())),
                    &|| cancel_for_the_thread.load(Ordering::SeqCst),
                );
                send(Message::Finished(result));
            });
        self.environment.plugin_search.bundles.download = match started {
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

    /// Whether a download is begun and not over.
    #[cfg(test)]
    pub(super) fn bundle_download_is_going_on(&self) -> bool {
        matches!(
            self.environment.plugin_search.bundles.download,
            BundleDownload::Running(_)
        )
    }

    /// The Cancel button of a download that is going on.
    pub(super) fn cancel_the_download_of_the_bundles(&mut self) {
        if let BundleDownload::Running(running) = &self.environment.plugin_search.bundles.download {
            running.cancel.store(true, Ordering::SeqCst);
        }
    }

    pub(super) fn dismiss_what_the_download_said(&mut self) {
        let bundles = &mut self.environment.plugin_search.bundles;
        if matches!(bundles.download, BundleDownload::Done(_) | BundleDownload::Failed(_)) {
            bundles.download = BundleDownload::Idle;
        }
    }

    /// Takes what the download has said since the last frame; when it is over, the plugins are found.
    pub(super) fn take_what_the_download_of_the_bundles_said(&mut self) {
        let mut finished = None;
        if let BundleDownload::Running(running) = &mut self.environment.plugin_search.bundles.download {
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
        self.environment.plugin_search.bundles.download = match result {
            Ok(outcome) => {
                if let Some(folder) = self.folder_of_the_bundles() {
                    self.trust_the_plugins_bundled_in(&folder);
                }
                self.start_scanning_plugins();
                BundleDownload::Done(format!(
                    "Fetched {} plugin{} ({}): {}. They are looked for now, and are in the menu of the + button once they are found.",
                    outcome.plugin_files.len(),
                    if outcome.plugin_files.len() == 1 { "" } else { "s" },
                    size_in_words(self.what_the_bundles_tab_shows().2),
                    outcome.sets.join(", "),
                ))
            }
            Err(error) if error == CANCELLED => BundleDownload::Idle,
            Err(error) => BundleDownload::Failed(error),
        };
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
