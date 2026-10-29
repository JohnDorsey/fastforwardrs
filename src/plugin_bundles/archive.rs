// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Taking the plugins out of a release file of their authors: a `.zip` (opened here), a `.tar.gz` (opened here too: **the system's `tar` of Windows reads the names of a tar that was made on
//! Linux with the code page of the console, and turns `Café` into `Caf├⌐`**), another `.tar` (the system's `tar` gives a file on its output), or, for macOS, a **disk image that holds an
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
    if is_gzip(archive) {
        return take_from_a_gzipped_tar(name, archive, member, to);
    }
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

fn is_gzip(path: &Path) -> bool {
    let mut start = [0_u8; 2];
    fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut start))
        .is_ok_and(|()| start == [0x1f, 0x8b])
}

/// Reads one block of 512 bytes of a tar: `false` where the archive ends before it (no byte of it), an error where it ends inside it.
fn read_a_block(reader: &mut impl Read, block: &mut [u8; 512]) -> io::Result<bool> {
    let mut filled = 0;
    while filled < block.len() {
        let read = reader.read(&mut block[filled..])?;
        if read == 0 {
            return if filled == 0 {
                Ok(false)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the archive ends inside a block",
                ))
            };
        }
        filled += read;
    }
    Ok(true)
}

/// Throws away `count` bytes of what is read.
fn skip_bytes(reader: &mut impl Read, count: u64) -> io::Result<()> {
    let skipped = io::copy(&mut reader.take(count), &mut io::sink())?;
    if skipped == count {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the archive ends inside a file",
        ))
    }
}

/// The number in a field of a header of a tar: written as octal digits (with a nul or a space after them), or, for one that is too big for that, as a number of bytes (GNU tar sets the top bit).
fn number_in_a_tar_field(field: &[u8]) -> Option<u64> {
    if field.first().is_some_and(|first| first & 0x80 != 0) {
        return Some(field[1..].iter().fold(0_u64, |number, byte| {
            number.checked_shl(8).unwrap_or(0) | u64::from(*byte)
        }));
    }
    let digits: Vec<u8> = field
        .iter()
        .copied()
        .skip_while(|byte| *byte == b' ')
        .take_while(|byte| (b'0'..=b'7').contains(byte))
        .collect();
    if digits.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(std::str::from_utf8(&digits).ok()?, 8).ok()
}

/// The text of a field of a header (up to the first nul), as UTF-8: the names of a tar are bytes, and the ones that are written by GNU tar on Linux are UTF-8.
fn text_in_a_tar_field(field: &[u8]) -> String {
    let end = field.iter().position(|byte| *byte == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

/// The `path` of the records of a pax header (`<length> path=<name>\n`), if it says one.
fn path_in_a_pax_header(records: &[u8]) -> Option<String> {
    let mut rest = records;
    while !rest.is_empty() {
        let space = rest.iter().position(|byte| *byte == b' ')?;
        let length: usize = std::str::from_utf8(&rest[..space]).ok()?.parse().ok()?;
        let record = rest.get(space + 1..length.min(rest.len()))?;
        if let Some(name) = record.strip_prefix(b"path=") {
            return Some(String::from_utf8_lossy(name.strip_suffix(b"\n").unwrap_or(name)).into_owned());
        }
        rest = rest.get(length..)?;
    }
    None
}

/// Reads `size` bytes of the data of an entry (a name that is too long for its header, or a pax header), and the padding after them.
fn read_the_data_of_a_tar_entry(reader: &mut impl Read, size: u64) -> io::Result<Vec<u8>> {
    // (A name or a header of records is not big: a size that says it is is not a tar.)
    if size > 1 << 20 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "an entry that should be a name is too big",
        ));
    }
    let mut data = Vec::new();
    reader.take(size).read_to_end(&mut data)?;
    if data.len() as u64 != size {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the archive ends inside an entry",
        ));
    }
    skip_bytes(reader, (512 - size % 512) % 512)?;
    Ok(data)
}

/// Takes a folder (a `member` that ends with `/`) or a file of a **gzipped tar** to `to`, reading it here: the names come out as they were written, which is as UTF-8 (the system's `tar`
/// of Windows turns them into what the code page of the console says). The GNU format and the pax format are read: the names of more than 100 bytes, which GNU tar gives in an entry of
/// their own, and the ones of a pax header. A leading `./` of the names is no part of them; **an entry whose name goes out of the folder is refused**; which files can be run is kept
/// (where the system has that), and a link is made as a link (where the system has them).
fn take_from_a_gzipped_tar(name: &str, archive: &Path, member: &str, to: &Path) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|error| format!("{name} could not be opened: {error}"))?;
    let mut reader = flate2::read::GzDecoder::new(io::BufReader::new(file));
    let read_error = |error: io::Error| format!("{name} could not be read: {error}");
    let as_a_folder = member.ends_with('/');
    let wanted = member.trim_start_matches("./").trim_end_matches('/');
    let mut found = false;
    let (mut long_name, mut pax_name): (Option<String>, Option<String>) = (None, None);
    let mut header = [0_u8; 512];
    while read_a_block(&mut reader, &mut header).map_err(read_error)? {
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        if &header[257..262] != b"ustar" {
            return Err(format!("{name} is not a tar archive"));
        }
        let size = number_in_a_tar_field(&header[124..136]).ok_or_else(|| format!("{name} is not a tar archive"))?;
        let kind = header[156];
        match kind {
            b'L' => {
                long_name = Some(text_in_a_tar_field(
                    &read_the_data_of_a_tar_entry(&mut reader, size).map_err(read_error)?,
                ));
                continue;
            }
            b'x' => {
                pax_name = path_in_a_pax_header(&read_the_data_of_a_tar_entry(&mut reader, size).map_err(read_error)?);
                continue;
            }
            b'g' | b'K' => {
                read_the_data_of_a_tar_entry(&mut reader, size).map_err(read_error)?;
                continue;
            }
            _ => {}
        }
        let padding = (512 - size % 512) % 512;
        // (The name: one of its own before the entry, or a pax header, or the header's. A header of the POSIX kind has a prefix of the name; the GNU kind uses those bytes for other things.)
        let own_name = if &header[257..263] == b"ustar\0" && header[345] != 0 {
            format!(
                "{}/{}",
                text_in_a_tar_field(&header[345..500]),
                text_in_a_tar_field(&header[0..100])
            )
        } else {
            text_in_a_tar_field(&header[0..100])
        };
        let full_name = long_name.take().or_else(|| pax_name.take()).unwrap_or(own_name);
        let clean = full_name.trim_start_matches("./").trim_end_matches('/');
        let is_a_file = kind == b'0' || kind == 0;
        let relative = if as_a_folder {
            if clean == wanted {
                Some("")
            } else {
                clean.strip_prefix(wanted).and_then(|rest| rest.strip_prefix('/'))
            }
        } else if is_a_file && clean == wanted {
            Some("")
        } else {
            None
        };
        let Some(relative) = relative else {
            skip_bytes(&mut reader, size + padding).map_err(read_error)?;
            continue;
        };
        let destination = if as_a_folder && !relative.is_empty() {
            if relative
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
                || relative.contains('\\')
            {
                return Err(format!(
                    "{name} has an entry whose name goes out of the folder: {full_name:?}"
                ));
            }
            to.join(relative)
        } else {
            to.to_owned()
        };
        found = true;
        match kind {
            b'5' => {
                fs::create_dir_all(&destination)
                    .map_err(|error| format!("{} could not be made: {error}", destination.display()))?;
                skip_bytes(&mut reader, size + padding).map_err(read_error)?;
            }
            b'0' | 0 => {
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|error| format!("{} could not be made: {error}", parent.display()))?;
                }
                let mut out = fs::File::create(&destination)
                    .map_err(|error| format!("{} could not be written: {error}", destination.display()))?;
                let copied = io::copy(&mut (&mut reader).take(size), &mut out)
                    .map_err(|error| format!("{full_name:?} could not be taken out: {error}"))?;
                if copied != size {
                    return Err(format!(
                        "{name} could not be read: the archive ends inside {full_name:?}"
                    ));
                }
                drop(out);
                skip_bytes(&mut reader, padding).map_err(read_error)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mode = number_in_a_tar_field(&header[100..108]).unwrap_or(0o644) as u32;
                    let _ = fs::set_permissions(&destination, fs::Permissions::from_mode(mode & 0o777));
                }
            }
            b'2' => {
                #[cfg(unix)]
                {
                    let target = text_in_a_tar_field(&header[157..257]);
                    if let Some(parent) = destination.parent() {
                        fs::create_dir_all(parent)
                            .map_err(|error| format!("{} could not be made: {error}", parent.display()))?;
                    }
                    std::os::unix::fs::symlink(&target, &destination)
                        .map_err(|error| format!("the link {} could not be made: {error}", destination.display()))?;
                }
                skip_bytes(&mut reader, size + padding).map_err(read_error)?;
            }
            // (A hard link, a device, a pipe: nothing of these is wanted.)
            _ => skip_bytes(&mut reader, size + padding).map_err(read_error)?,
        }
        if !as_a_folder {
            return Ok(());
        }
    }
    if found {
        Ok(())
    } else if as_a_folder {
        Err(format!("{name} has no folder called {wanted:?}"))
    } else {
        Err(format!("{name} has no file called {member:?}"))
    }
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
