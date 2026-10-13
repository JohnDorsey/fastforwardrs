//! Writing a file so that a failure part-way (a full disk, say) can never leave an existing file
//! half-written. Project files, project archives and the settings file are all written this way.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Replaces the file at `destination` with `contents`. The new contents are written to a
/// temporary file next to the destination and then renamed over it.
pub(crate) fn write_file_atomically(destination: &Path, contents: &str) -> io::Result<()> {
    replace_file_atomically(destination, |file| file.write_all(contents.as_bytes()))
}

/// Replaces the file at `destination` with what `write` puts in the file it is given (which is a new file, empty to begin with).
/// That file is a temporary one next to the destination, which is flushed to the disk and then renamed over the destination: if
/// `write` fails, or anything else does, the destination is as it was and the temporary file is gone.
pub(crate) fn replace_file_atomically(
    destination: &Path,
    write: impl FnOnce(&mut fs::File) -> io::Result<()>,
) -> io::Result<()> {
    let temporary_path = temporary_path_beside(destination)?;
    let written = fs::File::create(&temporary_path).and_then(|mut file| {
        write(&mut file)?;
        file.sync_all()
    });
    let renamed = written.and_then(|()| fs::rename(&temporary_path, destination));
    if renamed.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    renamed
}

fn temporary_path_beside(destination: &Path) -> io::Result<PathBuf> {
    let mut temporary_name = destination
        .file_name()
        .ok_or_else(|| io::Error::other("the destination has no file name"))?
        .to_owned();
    temporary_name.push(".saving");
    Ok(destination.with_file_name(temporary_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names_in(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_new_file_is_written_and_no_temporary_file_is_left_behind() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("data.json");
        write_file_atomically(&path, "hello\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello\n");
        assert_eq!(names_in(folder.path()), vec!["data.json".to_owned()]);
    }

    #[test]
    fn an_existing_file_is_replaced_completely_even_by_shorter_contents() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("data.json");
        fs::write(&path, "something much longer than what replaces it ".repeat(100)).unwrap();
        write_file_atomically(&path, "short").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "short");
    }

    #[test]
    fn writing_into_a_folder_that_does_not_exist_fails_and_creates_nothing() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("no such folder").join("data.json");
        assert!(write_file_atomically(&path, "x").is_err());
        assert!(names_in(folder.path()).is_empty());
    }

    #[test]
    fn a_failed_replacement_leaves_the_old_file_untouched_and_no_temporary_file() {
        let folder = tempfile::tempdir().unwrap();
        // The destination is a folder, which a file cannot be renamed over.
        let path = folder.path().join("data.json");
        fs::create_dir(&path).unwrap();
        assert!(write_file_atomically(&path, "x").is_err());
        assert_eq!(names_in(folder.path()), vec!["data.json".to_owned()]);
        assert!(path.is_dir());
    }

    #[test]
    fn a_destination_with_no_file_name_is_refused() {
        assert!(write_file_atomically(Path::new("/"), "x").is_err());
    }

    #[test]
    fn a_file_that_is_written_in_pieces_is_replaced_whole() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("big.ffrs");
        fs::write(&path, "the old one").unwrap();

        replace_file_atomically(&path, |file| {
            for piece in ["one, ", "two, ", "three"] {
                file.write_all(piece.as_bytes())?;
            }
            Ok(())
        })
        .unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "one, two, three");
        assert_eq!(names_in(folder.path()), vec!["big.ffrs".to_owned()]);
    }

    #[test]
    fn a_write_that_fails_part_way_leaves_the_old_file_as_it_was_and_no_temporary_file() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("big.ffrs");
        fs::write(&path, "the old one").unwrap();

        let result = replace_file_atomically(&path, |file| {
            file.write_all(b"half of the new one")?;
            Err(io::Error::other("the disk is full"))
        });

        assert!(result.is_err_and(|error| error.to_string().contains("the disk is full")));
        assert_eq!(fs::read_to_string(&path).unwrap(), "the old one");
        assert_eq!(names_in(folder.path()), vec!["big.ffrs".to_owned()]);
    }

    #[test]
    fn a_write_that_fails_where_there_was_no_file_leaves_nothing() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("new.ffrs");

        let result = replace_file_atomically(&path, |_| Err(io::Error::other("no")));

        assert!(result.is_err());
        assert!(names_in(folder.path()).is_empty());
    }
}
