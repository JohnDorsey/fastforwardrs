//! Where the files that are taken out of project archives are kept while the project is open.
//!
//! A file that is included in a `.ffrs` project file is in the archive as it is, but ffmpeg, which plays and reads every video and
//! sound the editor has, wants a file of its own to open. So when a project that has files included is loaded, each of them is taken
//! out of the archive and put in a folder of the computer's cache (once: a file that has been taken out before is there still, and is
//! used again), and from then on it is a file like any other that the project is made from.
//!
//! The place of a file is by what the archive says it is: its CRC-32 and its length, then the name it had (`<crc>-<length>/<name>`),
//! so that the same file in two archives is one file here, and two files of one name are two. A file of the right length at that place
//! is believed to be the one: it is not read again to be sure, which for a video of some gigabytes would take longer than taking it out.
//! Nothing in the cache is ever taken out of it by the program.

use std::fs;
use std::path::{Path, PathBuf};

use crate::project_file::archive::{
    ArchiveError, extract_entry, facts_of_entry, file_name_of_a_resource_entry, make_the_folder_for,
};

/// How long the name of a file is when it is kept here, in bytes: a name of a file that was cut down to the part that was cut
/// from the front (its ending, which tells what kind of file it is, stays), so that the whole path fits where paths are short.
const LONGEST_NAME_KEPT_IN_BYTES: usize = 100;

/// The folder the files taken out of archives are kept in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveCache {
    root: PathBuf,
}

impl ArchiveCache {
    /// A cache in the folder `root`, which is made when the first file is put in it.
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    /// The cache in the usual place: in the folder that the computer has for what a program can make again, if it has one.
    pub fn in_the_usual_place() -> Option<Self> {
        dirs::cache_dir().map(|folder| Self::at(folder.join("fastforwardrs").join("archive-resources")))
    }

    #[cfg(test)]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Takes the file of the entry `entry_name` of the archive at `archive` out of it, unless it was taken out before, and gives where
    /// it is: a file that can be opened like any other.
    pub fn take_out(&self, archive: &Path, entry_name: &str) -> Result<PathBuf, ArchiveError> {
        let file_name = file_name_of_a_resource_entry(entry_name).ok_or_else(|| ArchiveError::Unreadable {
            path: archive.to_owned(),
            reason: format!("\"{entry_name}\" is not the name of a file in a project file"),
        })?;
        let facts = facts_of_entry(archive, entry_name)?;
        let place = self
            .root
            .join(format!("{:08x}-{}", facts.crc32, facts.size))
            .join(name_to_keep(file_name));
        if fs::metadata(&place).is_ok_and(|found| found.is_file() && found.len() == facts.size) {
            return Ok(place);
        }
        make_the_folder_for(&place).map_err(|source| ArchiveError::CouldNotWrite {
            path: place.clone(),
            source,
        })?;
        extract_entry(archive, entry_name, &place)?;
        Ok(place)
    }
}

/// `file_name`, cut from the front if it is longer than the names that are kept are allowed to be.
fn name_to_keep(file_name: &str) -> &str {
    let mut start = file_name.len().saturating_sub(LONGEST_NAME_KEPT_IN_BYTES);
    while !file_name.is_char_boundary(start) {
        start += 1;
    }
    &file_name[start..]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::SourceMediaIdentifier;
    use crate::project_file::archive::{FileToInclude, resource_entry_name, write_archive};

    /// An archive in `folder` that has a file called `name` (as the source file `number`) with `contents`; gives the archive
    /// and the name of the entry.
    fn an_archive_with(
        folder: &Path,
        archive_name: &str,
        number: u32,
        name: &str,
        contents: &[u8],
    ) -> (PathBuf, String) {
        let original = folder.join(format!("original of {archive_name}"));
        fs::write(&original, contents).unwrap();
        let entry_name = resource_entry_name(SourceMediaIdentifier::new(number), name);
        let archive = folder.join(archive_name);
        write_archive(
            &archive,
            "project",
            &[FileToInclude {
                entry_name: entry_name.clone(),
                source: original.into(),
            }],
        )
        .unwrap();
        (archive, entry_name)
    }

    #[test]
    fn a_file_is_taken_out_into_a_folder_named_for_what_it_is_and_has_the_name_it_had() {
        let folder = tempfile::tempdir().unwrap();
        let cache = ArchiveCache::at(folder.path().join("cache"));
        let (archive, entry) = an_archive_with(folder.path(), "a.ffrs", 3, "holiday.mp4", b"the video");

        let taken_out = cache.take_out(&archive, &entry).unwrap();

        assert_eq!(fs::read(&taken_out).unwrap(), b"the video");
        assert!(taken_out.starts_with(folder.path().join("cache")));
        assert_eq!(taken_out.file_name().unwrap(), "holiday.mp4");
        let facts = facts_of_entry(&archive, &entry).unwrap();
        assert_eq!(
            taken_out.parent().unwrap().file_name().unwrap().to_string_lossy(),
            format!("{:08x}-{}", facts.crc32, facts.size)
        );
    }

    #[test]
    fn a_file_that_was_taken_out_before_is_used_again_and_not_taken_out_again() {
        let folder = tempfile::tempdir().unwrap();
        let cache = ArchiveCache::at(folder.path().join("cache"));
        let (archive, entry) = an_archive_with(folder.path(), "a.ffrs", 1, "v.mp4", b"0123456789");
        let first = cache.take_out(&archive, &entry).unwrap();
        // (Something of the same length is put in its place: if it were taken out again it would be what the archive has.)
        fs::write(&first, b"ABCDEFGHIJ").unwrap();

        let second = cache.take_out(&archive, &entry).unwrap();

        assert_eq!(second, first);
        assert_eq!(
            fs::read(&second).unwrap(),
            b"ABCDEFGHIJ",
            "a file of the right length is believed"
        );
    }

    #[test]
    fn a_file_of_another_length_at_the_place_is_taken_to_be_something_else_and_is_replaced() {
        let folder = tempfile::tempdir().unwrap();
        let cache = ArchiveCache::at(folder.path().join("cache"));
        let (archive, entry) = an_archive_with(folder.path(), "a.ffrs", 1, "v.mp4", b"0123456789");
        let place = cache.take_out(&archive, &entry).unwrap();
        fs::write(&place, b"cut short").unwrap();

        let again = cache.take_out(&archive, &entry).unwrap();

        assert_eq!(again, place);
        assert_eq!(fs::read(&again).unwrap(), b"0123456789");
    }

    #[test]
    fn the_same_file_in_two_archives_is_one_file_here_and_different_files_are_different_ones() {
        let folder = tempfile::tempdir().unwrap();
        let cache = ArchiveCache::at(folder.path().join("cache"));
        let (first, first_entry) = an_archive_with(folder.path(), "first.ffrs", 1, "clip.mp4", b"same bytes");
        let (second, second_entry) = an_archive_with(folder.path(), "second.ffrs", 9, "clip.mp4", b"same bytes");
        let (third, third_entry) = an_archive_with(folder.path(), "third.ffrs", 1, "clip.mp4", b"other bytes");

        let from_the_first = cache.take_out(&first, &first_entry).unwrap();
        let from_the_second = cache.take_out(&second, &second_entry).unwrap();
        let from_the_third = cache.take_out(&third, &third_entry).unwrap();

        assert_eq!(from_the_first, from_the_second);
        assert_ne!(from_the_first, from_the_third);
        assert_eq!(fs::read(&from_the_third).unwrap(), b"other bytes");
    }

    #[test]
    fn files_that_are_as_long_as_one_another_and_are_not_the_same_are_two_files_in_the_cache() {
        let folder = tempfile::tempdir().unwrap();
        let cache = ArchiveCache::at(folder.path().join("cache"));
        let (first, first_entry) = an_archive_with(folder.path(), "first.ffrs", 1, "clip.mp4", b"aaaaaaaaaa");
        let (second, second_entry) = an_archive_with(folder.path(), "second.ffrs", 1, "clip.mp4", b"bbbbbbbbbb");

        let from_the_first = cache.take_out(&first, &first_entry).unwrap();
        let from_the_second = cache.take_out(&second, &second_entry).unwrap();

        assert_ne!(
            from_the_first, from_the_second,
            "what the archive says of the files tells them apart"
        );
        assert_eq!(fs::read(&from_the_first).unwrap(), b"aaaaaaaaaa");
        assert_eq!(fs::read(&from_the_second).unwrap(), b"bbbbbbbbbb");
    }

    #[test]
    fn two_files_of_one_name_in_an_archive_are_two_files_in_the_cache() {
        let folder = tempfile::tempdir().unwrap();
        let cache = ArchiveCache::at(folder.path().join("cache"));
        let (archive, entry) = an_archive_with(folder.path(), "a.ffrs", 1, "clip.mp4", b"first one");
        let (other_archive, other_entry) = an_archive_with(folder.path(), "b.ffrs", 2, "clip.mp4", b"the second");

        let first = cache.take_out(&archive, &entry).unwrap();
        let second = cache.take_out(&other_archive, &other_entry).unwrap();

        assert_ne!(first, second);
        assert_eq!(fs::read(&first).unwrap(), b"first one");
        assert_eq!(fs::read(&second).unwrap(), b"the second");
    }

    #[test]
    fn an_entry_that_is_not_in_the_archive_is_not_taken_out_and_makes_nothing() {
        let folder = tempfile::tempdir().unwrap();
        let cache = ArchiveCache::at(folder.path().join("cache"));
        let (archive, _) = an_archive_with(folder.path(), "a.ffrs", 1, "v.mp4", b"x");

        let error = cache.take_out(&archive, "resources/2-other.mp4").unwrap_err();

        assert!(matches!(error, ArchiveError::NoSuchEntry { .. }), "{error:?}");
        assert!(!folder.path().join("cache").exists());
    }

    #[test]
    fn a_name_that_is_not_one_of_a_file_in_a_project_file_is_refused_before_the_archive_is_looked_at() {
        let folder = tempfile::tempdir().unwrap();
        let cache = ArchiveCache::at(folder.path().join("cache"));
        let (archive, _) = an_archive_with(folder.path(), "a.ffrs", 1, "v.mp4", b"x");

        for bad in [
            "../../outside.mp4",
            "project.fastforwardrs.json",
            "resources/1-a/b.mp4",
            "",
        ] {
            let error = cache.take_out(&archive, bad).unwrap_err();
            assert!(matches!(error, ArchiveError::Unreadable { .. }), "{bad:?}: {error:?}");
        }
        assert!(!folder.path().join("cache").exists());
    }

    #[test]
    fn an_archive_that_cannot_be_read_is_said_so() {
        let folder = tempfile::tempdir().unwrap();
        let cache = ArchiveCache::at(folder.path().join("cache"));

        let error = cache
            .take_out(&folder.path().join("missing.ffrs"), "resources/1-a.mp4")
            .unwrap_err();

        assert!(matches!(error, ArchiveError::CouldNotRead { .. }), "{error:?}");
    }

    #[test]
    fn a_long_name_is_cut_from_the_front_so_that_the_path_stays_short_and_the_ending_stays() {
        let folder = tempfile::tempdir().unwrap();
        let cache = ArchiveCache::at(folder.path().join("cache"));
        let long_name = format!("{}.mp4", "q".repeat(200));
        let (archive, entry) = an_archive_with(folder.path(), "a.ffrs", 1, &long_name, b"x");

        let place = cache.take_out(&archive, &entry).unwrap();

        let name = place.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.len() <= 100, "{} bytes", name.len());
        assert!(name.ends_with("q.mp4"), "{name}");
        assert_eq!(fs::read(&place).unwrap(), b"x");
    }

    #[test]
    fn a_name_is_never_cut_in_the_middle_of_a_character() {
        let name = format!("{}.mp4", "ワ".repeat(60));

        let kept = name_to_keep(&name);

        assert!(kept.len() <= LONGEST_NAME_KEPT_IN_BYTES);
        assert!(kept.ends_with("ワ.mp4"), "{kept}");
        assert_eq!(name_to_keep("short.mp4"), "short.mp4");
    }

    #[test]
    fn the_usual_place_is_one_the_program_owns_inside_the_computers_cache() {
        if let Some(cache) = ArchiveCache::in_the_usual_place() {
            assert!(
                cache
                    .root()
                    .ends_with(Path::new("fastforwardrs").join("archive-resources"))
            );
        }
    }
}
