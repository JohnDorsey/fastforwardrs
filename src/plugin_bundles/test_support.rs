// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the tests of fetching plugins share (here and in the window of sound plugins): a fetcher that serves archives made on the spot, a way to make a zip, and a manifest of a few
//! sets that is true to them.

use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Write};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{Build, BundleSet, CANCELLED, Fetcher, LicenceText, Manifest, Member, Source};
use crate::digest::Sha256Digest;

pub(crate) fn sha(bytes: &[u8]) -> String {
    Sha256Digest::of_bytes(bytes).to_hex()
}

/// Serves what it is given (and says what it was asked for), and can say that the download is cancelled after some fetches.
pub(crate) struct FakeFetcher {
    pub(crate) files: HashMap<String, Vec<u8>>,
    pub(crate) asked: Mutex<Vec<String>>,
    pub(crate) cancel_after: Option<usize>,
    pub(crate) cancelled: AtomicBool,
}

impl FakeFetcher {
    pub(crate) fn new(files: HashMap<String, Vec<u8>>) -> Self {
        Self {
            files,
            asked: Mutex::new(Vec::new()),
            cancel_after: None,
            cancelled: AtomicBool::new(false),
        }
    }
    pub(crate) fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

impl Fetcher for FakeFetcher {
    fn fetch(
        &self,
        url: &str,
        to: &Path,
        on_bytes: &mut dyn FnMut(u64),
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        let count = {
            let mut asked = self.asked.lock().unwrap();
            asked.push(url.to_owned());
            asked.len()
        };
        if self.cancel_after.is_some_and(|after| count > after) {
            self.cancelled.store(true, Ordering::SeqCst);
            return Err(CANCELLED.to_owned());
        }
        let bytes = self
            .files
            .get(url)
            .ok_or_else(|| format!("{url} could not be downloaded: 404"))?;
        std::fs::write(to, bytes).map_err(|error| error.to_string())?;
        on_bytes(bytes.len() as u64);
        Ok(())
    }
}

pub(crate) fn a_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in files {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(content).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

pub(crate) struct Fixture {
    pub(crate) manifest: Manifest,
    pub(crate) files: HashMap<String, Vec<u8>>,
}

/// Two sets in two zips for `windows-x86_64` (the second set has two plugins), a third set in the zip of the second (a file that two sets are in), and a set that has no build for this system.
pub(crate) fn a_fixture() -> Fixture {
    let zip_one = a_zip(&[
        ("one-1.0/Reverb.clap", b"the reverb"),
        ("one-1.0/Other.clap", b"not taken"),
    ]);
    let zip_two = a_zip(&[
        ("pack/First.clap", b"first"),
        ("pack/Second.clap", b"second"),
        ("pack/Third.clap", b"third"),
    ]);
    let licence = b"GNU GENERAL PUBLIC LICENSE, a copy".to_vec();
    let mut files = HashMap::new();
    files.insert("https://example.org/one.zip".to_owned(), zip_one.clone());
    files.insert("https://example.org/two.zip".to_owned(), zip_two.clone());
    files.insert("https://example.org/LICENSE".to_owned(), licence.clone());
    let set = |id: &str, plugins: &[&str], archive: &[u8], url: &str, members: &[(&str, &str)]| BundleSet {
        id: id.to_owned(),
        name: id.to_uppercase(),
        version: "1.0".to_owned(),
        authors: vec!["Someone".to_owned()],
        licence: "GPL-3.0-or-later".to_owned(),
        licence_note: None,
        homepage: "https://example.org/".to_owned(),
        plugins: plugins.iter().map(|name| (*name).to_owned()).collect(),
        licence_texts: vec![LicenceText {
            name: "LICENSE.txt".to_owned(),
            url: "https://example.org/LICENSE".to_owned(),
            sha256: sha(&licence),
        }],
        source: Source {
            url: format!("https://example.org/{id}-source.tar.gz"),
            sha256: None,
            md5: None,
            note: Some("a note about the source".to_owned()),
        },
        platforms: BTreeMap::from([(
            "windows-x86_64".to_owned(),
            Build {
                url: url.to_owned(),
                sha256: sha(archive),
                size: archive.len() as u64,
                members: members
                    .iter()
                    .map(|(from, to)| Member {
                        from: (*from).to_owned(),
                        to: (*to).to_owned(),
                    })
                    .collect(),
            },
        )]),
    };
    let mut sets = vec![
        set(
            "one",
            &["Reverb"],
            &zip_one,
            "https://example.org/one.zip",
            &[("one-1.0/Reverb.clap", "Reverb.clap")],
        ),
        set(
            "two",
            &["First", "Second"],
            &zip_two,
            "https://example.org/two.zip",
            &[("pack/First.clap", "First.clap"), ("pack/Second.clap", "Second.clap")],
        ),
        set(
            "three",
            &["Third"],
            &zip_two,
            "https://example.org/two.zip",
            &[("pack/Third.clap", "Third.clap")],
        ),
    ];
    let mut elsewhere = set(
        "elsewhere",
        &["Nothing here"],
        &zip_one,
        "https://example.org/one.zip",
        &[("one-1.0/Reverb.clap", "Reverb.clap")],
    );
    elsewhere.platforms = BTreeMap::from([("linux-x86_64".to_owned(), elsewhere.platforms["windows-x86_64"].clone())]);
    sets.push(elsewhere);
    Fixture {
        manifest: Manifest { format: 1, sets },
        files,
    }
}
