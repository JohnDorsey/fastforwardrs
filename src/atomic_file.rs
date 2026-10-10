//! Writing a file so that a failure part-way (a full disk, say) can never leave an existing file
//! half-written. Project files and the settings file are both written this way.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Replaces the file at `destination` with `contents`. The new contents are written to a
/// temporary file next to the destination and then renamed over it.
pub(crate) fn write_file_atomically(destination: &Path, contents: &str) -> io::Result<()> {
    let temporary_path = temporary_path_beside(destination)?;
    fs::write(&temporary_path, contents)?;
    if let Err(error) = fs::rename(&temporary_path, destination) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    Ok(())
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
}
