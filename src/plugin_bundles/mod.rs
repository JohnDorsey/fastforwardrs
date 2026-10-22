// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugins that the program can fetch for a person who asks (the *Bundles* tab of the plugin manager; LG-02 of `docs/plugin-system-features.md`): the manifest kept in the
//! program, with the licence policy it is checked against, and the download of the release files of the plugins' own authors, checked by their checksums and unpacked beside a notice
//! of who wrote them and under which licence. See `docs/plugins-and-trust.md`.

mod archive;
mod download;
mod manifest;
mod shaders;

pub use download::{
    CANCELLED, Curl, Fetcher, Outcome, Progress, bytes_to_download, fetch_plugins, fetched_plugins_folder,
    fetched_shaders_folder, is_fetched, notice_of,
};
pub use manifest::{
    ALLOWED_LICENCES, Build, BundleSet, EMBEDDED_MANIFEST, LicenceText, Manifest, Member, Source, licence_is_allowed,
    this_platform,
};
pub use shaders::{
    EMBEDDED_SHADER_MANIFEST, ShaderEntry, ShaderManifest, ShaderOutcome, ShaderSet, fetch_shaders,
    folder_of_a_shader_set, notice_of_a_shader_set, shader_bytes_to_download, shaders_are_fetched,
};

#[cfg(test)]
mod macos_tests;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
