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

/// What a set holds: plugins (code, which the program runs), or data that plugins use (patches, wavetables: files, which nothing runs).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SetKind {
    #[default]
    Plugins,
    Data,
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
    /// Plugins, or data that plugins use (which has a section of its own in the tab: a person who wants the plugins is not made to fetch the data, which is bigger).
    #[serde(default)]
    pub kind: SetKind,
    /// For data: the name of the folder in the folder of fetched plugins that the set is put in, which is the name that the plugin looks for (Surge XT looks for `SurgeXTData` in the folders
    /// above its own file, so the set is put beside the folders of the plugins). Plugins are in a folder called after the id of the set.
    #[serde(default)]
    pub folder: Option<String>,
    /// The names of the plugins in it, for a person (for data: what is in it).
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
    /// For a disk image (`.dmg`, the builds for macOS): the name of the installer (`.pkg`) in it whose contents are taken; the members are then paths in what the installer holds. Nothing
    /// is installed.
    #[serde(default)]
    pub installer: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Member {
    /// Its name inside the archive.
    pub from: String,
    /// The name it is given in the set's folder: a plain name of a `.clap` file, or of an `.lv2` bundle (a folder). **For data it is `.`**: what is in the folder `from` is the set's folder.
    pub to: String,
    /// For data: paths inside the folder that are **not** kept (each a path with `/` between its parts, relative to the folder, and a folder ends with `/`), because they may not ship (what the
    /// licence policy does not allow, or whose licence is not known). One that is not in the archive is an error: the manifest would be out of date.
    #[serde(default)]
    pub leave_out: Vec<String>,
}

impl BundleSet {
    /// The name of the folder of this set in the folder of fetched plugins.
    pub fn folder_name(&self) -> &str {
        self.folder.as_deref().unwrap_or(&self.id)
    }

    /// Whether this is data that plugins use, and not plugins.
    pub fn is_data(&self) -> bool {
        self.kind == SetKind::Data
    }
}

impl Manifest {
    /// The same manifest with only the sets of one kind (the tab has a section for each).
    #[must_use]
    pub fn only(&self, kind: SetKind) -> Self {
        Self {
            format: self.format,
            sets: self.sets.iter().filter(|set| set.kind == kind).cloned().collect(),
        }
    }

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
        // (The folders that the sets are put in: two sets may not be put in one, and a folder of data may not be the folder of a set of plugins.)
        let mut folders: Vec<&str> = Vec::new();
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
            match (&set.folder, set.kind) {
                (None, _) => {}
                (Some(folder), SetKind::Data) if is_a_plain_file_name(folder) && !folder.starts_with('.') => {}
                (Some(_), SetKind::Data) => {
                    problems.push(format!("set {name}: the folder is not a plain name of a folder"));
                }
                (Some(_), SetKind::Plugins) => {
                    problems.push(format!(
                        "set {name}: only data has a folder of its own (plugins are in a folder called after the id)"
                    ));
                }
            }
            if folders.contains(&set.folder_name()) {
                problems.push(format!(
                    "set {name}: the folder {:?} is the folder of another set too",
                    set.folder_name()
                ));
            }
            folders.push(set.folder_name());
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
                if let Some(installer) = &build.installer
                    && !(is_a_plain_file_name(installer)
                        && installer.to_ascii_lowercase().ends_with(".pkg")
                        && build.url.to_ascii_lowercase().ends_with(".dmg"))
                {
                    problems.push(format!(
                        "{there}: an installer is a plain name of a .pkg file in a .dmg disk image"
                    ));
                }
                let mut kept: Vec<&str> = Vec::new();
                for member in &build.members {
                    let climbs = member.from.split(['/', '\\']).any(|part| part == "..");
                    if member.from.is_empty() || member.from.starts_with(['/', '\\']) || climbs {
                        problems.push(format!("{there}: {:?} is not a name inside the archive", member.from));
                    }
                    if set.is_data() {
                        problems.extend(problems_of_a_member_of_data(&there, member, build.members.len()));
                        continue;
                    }
                    if !member.leave_out.is_empty() {
                        problems.push(format!("{there}: only data leaves anything out"));
                    }
                    let lower = member.to.to_ascii_lowercase();
                    if !(is_a_plain_file_name(&member.to) && (lower.ends_with(".clap") || lower.ends_with(".lv2"))) {
                        problems.push(format!(
                            "{there}: {:?} is not a plain name of a .clap file or an .lv2 folder",
                            member.to
                        ));
                    }
                    // (An LV2 bundle is a folder: out of a zip or a tar it is taken by the name of the folder with a `/` at the end, and without it only one file would be taken.)
                    if lower.ends_with(".lv2") && build.installer.is_none() && !member.from.ends_with('/') {
                        problems.push(format!(
                            "{there}: {:?} is a folder (an .lv2 bundle) and its name inside the archive must end with /",
                            member.from
                        ));
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

/// What is wrong with the one member of a set of data: it is a folder of the archive (its name ends with `/`) whose contents are the folder of the set (`to` is `.`), and what it leaves out are
/// paths inside it that do not go out of it.
fn problems_of_a_member_of_data(there: &str, member: &Member, how_many_members: usize) -> Vec<String> {
    let mut problems = Vec::new();
    if how_many_members != 1 {
        problems.push(format!("{there}: a set of data takes one folder of the archive"));
    }
    if member.to != "." {
        problems.push(format!(
            "{there}: the member of a set of data is put as the folder of the set, and its name is \".\", not {:?}",
            member.to
        ));
    }
    if !member.from.ends_with('/') {
        problems.push(format!(
            "{there}: {:?} is the folder of a set of data, and its name inside the archive must end with /",
            member.from
        ));
    }
    for left_out in &member.leave_out {
        let climbs = left_out.split('/').any(|part| part == "..");
        if left_out.is_empty() || left_out.starts_with('/') || left_out.contains('\\') || climbs {
            problems.push(format!(
                "{there}: {left_out:?} is not a path inside the folder (parts with / between them, none going up)"
            ));
        }
    }
    problems
}

/// Whether an SPDX expression may ship: every part joined by AND must be allowed, and at least one part joined by OR. (Parentheses are not used, and a name with one in it is not an
/// allowed name.)
pub fn licence_is_allowed(expression: &str) -> bool {
    expression
        .split(" AND ")
        .all(|part| part.split(" OR ").any(|name| ALLOWED_LICENCES.contains(&name.trim())))
}

pub(super) fn is_https(address: &str) -> bool {
    address.starts_with("https://") && address.len() > "https://".len()
}

pub(super) fn is_a_sha256(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn is_a_plain_id(text: &str) -> bool {
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
    } else if cfg!(target_os = "macos") {
        // (The builds for macOS are universal: they hold the code for Intel and for Apple Silicon.)
        Some("macos-universal")
    } else {
        None
    }
}
