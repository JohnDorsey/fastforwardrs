// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The manifest of the plugins that the program can fetch for a person (`bundled-plugins/manifest.json`, kept inside the program): which plugins, from which release files of their
//! own authors, each with its checksum, under which licence, and where its source is. **It is checked against the licence policy** here as it is by
//! `tools/bundled_plugins/fetch_bundled_plugins.py` (LG-02 of `docs/plugin-system-features.md`): GPL and LGPL licences that are compatible with GPLv3 and the permissive ones may
//! be bundled; **AGPL and GPL-2.0-only may not**; every address is https; every file has a checksum.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The manifest as it was when the program was built.
pub const EMBEDDED_MANIFEST: &str = include_str!("../../bundled-plugins/manifest.json");

/// The licences that may ship, by their SPDX names (LG-02): the permissive ones, and GPL and LGPL that are compatible with GPLv3.
pub const ALLOWED_LICENCES: [&str; 14] = [
    "MIT",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Apache-2.0",
    "CC0-1.0",
    "Zlib",
    "CC-BY-4.0",
    "GPL-3.0-only",
    "GPL-3.0-or-later",
    "GPL-2.0-or-later",
    "LGPL-3.0-only",
    "LGPL-3.0-or-later",
    "LGPL-2.1-or-later",
];

/// The systems the manifest has builds for.
const KNOWN_PLATFORMS: [&str; 3] = ["windows-x86_64", "linux-x86_64", "macos-universal"];

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub sets: Vec<BundleSet>,
}

/// One set of plugins from one author: one folder when it is fetched.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct BundleSet {
    pub id: String,
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    /// An SPDX expression.
    pub licence: String,
    #[serde(default)]
    pub licence_note: Option<String>,
    pub homepage: String,
    /// The names of the plugins in it, for a person.
    pub plugins: Vec<String>,
    pub licence_texts: Vec<LicenceText>,
    pub source: Source,
    pub platforms: BTreeMap<String, Build>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct LicenceText {
    /// The name the file is given in the set's folder.
    pub name: String,
    pub url: String,
    pub sha256: String,
}

/// Where the corresponding source of the set's version is (it is not fetched: it is told).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Source {
    pub url: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub md5: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

/// The release file that has the plugins for one system.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Build {
    pub url: String,
    pub sha256: String,
    /// How many bytes it is.
    pub size: u64,
    /// The files taken out of it.
    pub members: Vec<Member>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Member {
    /// Its name inside the archive.
    pub from: String,
    /// The name it is given in the set's folder: a plain name of a `.clap` file.
    pub to: String,
}

impl Manifest {
    /// The manifest that is kept in the program.
    ///
    /// # Panics
    /// If it cannot be read (a test says that it can).
    pub fn embedded() -> Self {
        serde_json::from_str(EMBEDDED_MANIFEST).expect("the manifest in the program is read")
    }

    /// What is wrong with the manifest, one line each (nothing if it is fit).
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.format != 1 {
            return vec![format!(
                "the format of the manifest is {}, and this program reads 1",
                self.format
            )];
        }
        if self.sets.is_empty() {
            return vec!["the manifest has no sets".to_owned()];
        }
        let mut seen: Vec<&str> = Vec::new();
        for set in &self.sets {
            let name = &set.id;
            if !is_a_plain_id(&set.id) {
                problems.push(format!(
                    "set {name}: the id is not lower case letters, digits and dashes"
                ));
            }
            if seen.contains(&set.id.as_str()) {
                problems.push(format!("set {name}: the id is used twice"));
            }
            seen.push(&set.id);
            if set.name.is_empty() || set.version.is_empty() || set.authors.is_empty() || set.plugins.is_empty() {
                problems.push(format!("set {name}: a name, a version, authors and plugins are needed"));
            }
            if !is_https(&set.homepage) {
                problems.push(format!("set {name}: the homepage is not an https address"));
            }
            if !licence_is_allowed(&set.licence) {
                problems.push(format!(
                    "set {name}: the licence {:?} is not one that may ship",
                    set.licence
                ));
            }
            if set.licence_texts.is_empty() {
                problems.push(format!("set {name}: no licence text"));
            }
            for text in &set.licence_texts {
                if !(is_https(&text.url) && is_a_sha256(&text.sha256) && is_a_plain_file_name(&text.name)) {
                    problems.push(format!(
                        "set {name}: a licence text needs a plain file name, an https address and a sha256"
                    ));
                }
            }
            if !is_https(&set.source.url) {
                problems.push(format!("set {name}: the source has no https address"));
            }
            if set.platforms.is_empty() {
                problems.push(format!("set {name}: no platforms"));
            }
            for (platform, build) in &set.platforms {
                let there = format!("set {name}, {platform}");
                if !KNOWN_PLATFORMS.contains(&platform.as_str()) {
                    problems.push(format!("{there}: not a platform that is known"));
                }
                if !is_https(&build.url) || !is_a_sha256(&build.sha256) {
                    problems.push(format!("{there}: needs an https address and a sha256"));
                }
                if build.members.is_empty() {
                    problems.push(format!("{there}: no files taken"));
                }
                let mut kept: Vec<&str> = Vec::new();
                for member in &build.members {
                    let climbs = member.from.split(['/', '\\']).any(|part| part == "..");
                    if member.from.is_empty() || member.from.starts_with(['/', '\\']) || climbs {
                        problems.push(format!("{there}: {:?} is not a name inside the archive", member.from));
                    }
                    if !(is_a_plain_file_name(&member.to) && member.to.to_ascii_lowercase().ends_with(".clap")) {
                        problems.push(format!("{there}: {:?} is not a plain name of a .clap file", member.to));
                    }
                    if kept.contains(&member.to.as_str()) {
                        problems.push(format!("{there}: {:?} is taken twice", member.to));
                    }
                    kept.push(&member.to);
                }
            }
        }
        problems
    }
}

/// Whether an SPDX expression may ship: every part joined by AND must be allowed, and at least one part joined by OR. (Parentheses are not used, and a name with one in it is not an
/// allowed name.)
pub fn licence_is_allowed(expression: &str) -> bool {
    expression
        .split(" AND ")
        .all(|part| part.split(" OR ").any(|name| ALLOWED_LICENCES.contains(&name.trim())))
}

fn is_https(address: &str) -> bool {
    address.starts_with("https://") && address.len() > "https://".len()
}

fn is_a_sha256(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_a_plain_id(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A name that is a file and not a place: no folder separators, not `.` or `..`, not empty.
pub fn is_a_plain_file_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
}

/// The name of the build for the system this program runs on (`None` for a system with no builds).
pub fn this_platform() -> Option<&'static str> {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("windows-x86_64")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux-x86_64")
    } else {
        None
    }
}
