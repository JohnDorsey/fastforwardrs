// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Taking the plugins out of a release file of their authors: a `.zip` (opened here), a `.tar` (the system's `tar` gives a file on its output), or, for macOS, a **disk image that holds an
//! installer** (`.dmg` with a `.pkg`: the image is mounted with `hdiutil` and the installer is expanded with `pkgutil`, which are the programs of macOS that do that, and the plugin is
//! copied out of what the installer would install; nothing is installed). On macOS a CLAP plugin is a **bundle**, a folder, so a member of a `.zip` whose name ends with `/` is a folder and
//! is taken with everything in it, keeping which files can be run.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::media::background_command;

/// A release file that is open for its members to be taken out.
pub(super) enum Opened {
    Zip {
        name: String,
        zip: zip::ZipArchive<fs::File>,
    },
    Tar {
        name: String,
        path: PathBuf,
        /// A folder to unpack a folder of the archive in before it is put in place.
        work: PathBuf,
    },
    /// A disk image that is mounted, and its installer expanded: the members are paths in what the installer holds.
    Installer {
        name: String,
        expanded: PathBuf,
        /// Mounted for as long as this lives.
        _mount: Mount,
    },
}

/// A disk image that is mounted at a folder of ours, and is let go of when this is dropped.
pub(super) struct Mount {
    point: PathBuf,
}

impl Drop for Mount {
    fn drop(&mut self) {
        let _ = background_command(Path::new("hdiutil"))
            .args(["detach", "-force", "-quiet"])
            .arg(&self.point)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// Opens the release file at `archive`. `installer` is the name of the `.pkg` inside a disk image whose contents are wanted; `work` is a folder to mount and expand in.
pub(super) fn open(archive: &Path, installer: Option<&str>, work: &Path) -> Result<Opened, String> {
    let name = name_of(archive);
    if let Some(installer) = installer {
        return open_an_installer_of_a_disk_image(archive, installer, work, name);
    }
    if is_a_zip(archive) {
        let file = fs::File::open(archive).map_err(|error| format!("{name} could not be opened: {error}"))?;
        let zip = zip::ZipArchive::new(file).map_err(|error| format!("{name} is not a zip archive: {error}"))?;
        return Ok(Opened::Zip { name, zip });
    }
    Ok(Opened::Tar {
        name,
        path: archive.to_owned(),
        work: work.to_owned(),
    })
}

fn open_an_installer_of_a_disk_image(
    archive: &Path,
    installer: &str,
    work: &Path,
    name: String,
) -> Result<Opened, String> {
    let stem = name.replace(['/', '\\', ' '], "_");
    let point = work.join(format!("{stem}.mounted"));
    fs::create_dir_all(&point).map_err(|error| format!("{} could not be made: {error}", point.display()))?;
    let attached = background_command(Path::new("hdiutil"))
        .args(["attach", "-nobrowse", "-readonly", "-noverify", "-noautoopen", "-mountpoint"])
        .arg(&point)
        .arg(archive)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| {
            format!("`hdiutil` could not be run, and it opens the disk images of the builds for macOS (it is a program of macOS): {error}")
        })?;
    if !attached.status.success() {
        return Err(format!(
            "{name} could not be opened as a disk image: {}",
            String::from_utf8_lossy(&attached.stderr).trim()
        ));
    }
    let mount = Mount { point: point.clone() };
    let expanded = work.join(format!("{stem}.expanded"));
    let _ = fs::remove_dir_all(&expanded);
    let done = background_command(Path::new("pkgutil"))
        .arg("--expand-full")
        .arg(point.join(installer))
        .arg(&expanded)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| {
            format!("`pkgutil` could not be run, and it opens the installers of the builds for macOS: {error}")
        })?;
    if !done.status.success() {
        return Err(format!(
            "the installer {installer:?} in {name} could not be expanded: {}",
            String::from_utf8_lossy(&done.stderr).trim()
        ));
    }
    Ok(Opened::Installer {
        name,
        expanded,
        _mount: mount,
    })
}

impl Opened {
    /// Takes `member` out and writes it to `to`: a file, or (for a name that ends with `/` in a zip, and for a folder of an installer) a folder with everything in it.
    pub(super) fn take(&mut self, member: &str, to: &Path) -> Result<(), String> {
        match self {
            Self::Zip { name, zip } => take_from_a_zip(name, zip, member, to),
            Self::Tar { name, path, work } => take_from_a_tar(name, path, work, member, to),
            Self::Installer { name, expanded, .. } => {
                let from = expanded.join(member);
                if !from.exists() {
                    return Err(format!("the installer in {name} has no {member:?}"));
                }
                copy_a_tree(&from, to).map_err(|error| format!("{member:?} could not be copied: {error}"))
            }
        }
    }
}

fn take_from_a_zip(name: &str, zip: &mut zip::ZipArchive<fs::File>, member: &str, to: &Path) -> Result<(), String> {
    if let Some(folder) = member.strip_suffix('/') {
        return take_a_folder_from_a_zip(name, zip, folder, to);
    }
    let mut inside = zip
        .by_name(member)
        .map_err(|_| format!("{name} has no file called {member:?}"))?;
    let mut out = fs::File::create(to).map_err(|error| format!("{} could not be written: {error}", to.display()))?;
    io::copy(&mut inside, &mut out).map_err(|error| format!("{member:?} could not be taken out: {error}"))?;
    Ok(())
}

/// Takes the folder `folder` of the zip, and everything in it, to `to`. A name that would leave the folder is refused; which files can be run is kept; a link is made as a link.
fn take_a_folder_from_a_zip(
    name: &str,
    zip: &mut zip::ZipArchive<fs::File>,
    folder: &str,
    to: &Path,
) -> Result<(), String> {
    let mut made_any = false;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| format!("{name}: {error}"))?;
        let Some(path) = entry.enclosed_name() else {
            return Err(format!(
                "{name} has an entry whose name goes out of the folder: {:?}",
                entry.name()
            ));
        };
        let Ok(inside) = path.strip_prefix(folder) else {
            continue;
        };
        let destination = if inside.as_os_str().is_empty() {
            to.to_owned()
        } else {
            to.join(inside)
        };
        made_any = true;
        if entry.is_dir() {
            fs::create_dir_all(&destination)
                .map_err(|error| format!("{} could not be made: {error}", destination.display()))?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("{} could not be made: {error}", parent.display()))?;
        }
        #[cfg(unix)]
        if entry.unix_mode().is_some_and(|mode| mode & 0o170_000 == 0o120_000) {
            let mut target = String::new();
            entry
                .read_to_string(&mut target)
                .map_err(|error| format!("a link of {name} could not be read: {error}"))?;
            std::os::unix::fs::symlink(&target, &destination)
                .map_err(|error| format!("the link {} could not be made: {error}", destination.display()))?;
            continue;
        }
        let mut out = fs::File::create(&destination)
            .map_err(|error| format!("{} could not be written: {error}", destination.display()))?;
        io::copy(&mut entry, &mut out)
            .map_err(|error| format!("{:?} could not be taken out: {error}", entry.name()))?;
        drop(out);
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&destination, fs::Permissions::from_mode(mode & 0o777));
        }
    }
    if made_any {
        Ok(())
    } else {
        Err(format!("{name} has no folder called {folder:?}"))
    }
}

fn take_from_a_tar(name: &str, archive: &Path, work: &Path, member: &str, to: &Path) -> Result<(), String> {
    if let Some(folder) = member.strip_suffix('/') {
        return take_a_folder_from_a_tar(name, archive, work, folder, to);
    }
    // (The system's tar gives the file on its output. Where the names were written with `./` in front, that is tried too.)
    for tried in [member.to_owned(), format!("./{member}")] {
        let out = fs::File::create(to).map_err(|error| format!("{} could not be written: {error}", to.display()))?;
        let status = background_command(Path::new("tar"))
            .arg("-xOf")
            .arg(archive)
            .arg(&tried)
            .stdout(out)
            .stderr(Stdio::null())
            .status()
            .map_err(|error| {
                format!("`tar` could not be run, and it opens the archives of the builds for Linux: {error}")
            })?;
        if status.success() && fs::metadata(to).is_ok_and(|metadata| metadata.len() > 0) {
            return Ok(());
        }
    }
    let _ = fs::remove_file(to);
    Err(format!("{name} has no file called {member:?}"))
}

/// Takes the folder `folder` of the tar (an LV2 bundle is one), and everything in it, to `to`: the system's `tar` unpacks it in a folder of `work` and it is copied from there, keeping which
/// files can be run (and links, where the system has them). Where the names were written with `./` in front, that is tried too.
fn take_a_folder_from_a_tar(name: &str, archive: &Path, work: &Path, folder: &str, to: &Path) -> Result<(), String> {
    let unpacked = work.join(format!(
        "{}.unpacked",
        to.file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    ));
    for tried in [folder.to_owned(), format!("./{folder}")] {
        let _ = fs::remove_dir_all(&unpacked);
        fs::create_dir_all(&unpacked).map_err(|error| format!("{} could not be made: {error}", unpacked.display()))?;
        let status = background_command(Path::new("tar"))
            .arg("-xf")
            .arg(archive)
            .arg("-C")
            .arg(&unpacked)
            .arg(&tried)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| {
                format!("`tar` could not be run, and it opens the archives of the builds for Linux: {error}")
            })?;
        let from = unpacked.join(folder);
        if status.success() && from.is_dir() {
            let copied = copy_a_tree(&from, to).map_err(|error| format!("{folder:?} could not be copied: {error}"));
            let _ = fs::remove_dir_all(&unpacked);
            return copied;
        }
    }
    let _ = fs::remove_dir_all(&unpacked);
    Err(format!("{name} has no folder called {folder:?}"))
}

fn is_a_zip(path: &Path) -> bool {
    let mut start = [0_u8; 2];
    fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut start))
        .is_ok_and(|()| &start == b"PK")
}

/// Copies a file or a folder and everything in it, keeping which files can be run (and links, where the system has them).
fn copy_a_tree(from: &Path, to: &Path) -> io::Result<()> {
    let kind = fs::symlink_metadata(from)?.file_type();
    if kind.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_a_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else if kind.is_symlink() {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(fs::read_link(from)?, to)
        }
        #[cfg(not(unix))]
        {
            fs::copy(from, to).map(|_| ())
        }
    } else {
        fs::copy(from, to).map(|_| ())
    }
}
