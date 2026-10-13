//! Where the bytes of a media file are, which is what the ffmpeg tools are told to read.
//!
//! A media file is usually a file of its own. But a file that is included in a project file (see
//! [`crate::project_file::archive`]) is stored in it as it is, one stretch of the archive from where its first byte is to where its
//! last is, and the tools can read that stretch where it is, as if it were a file, with their `subfile` protocol: no copy of it is
//! made, and nothing has to be taken out first.

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

/// Which bytes of a file a media file is, when it is not the whole of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Stretch {
    /// How far into the file the first byte is.
    pub start: u64,
    /// How many bytes there are.
    pub length: u64,
}

/// Where the bytes of a media file are: a file of its own, or a stretch of another file.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MediaLocation {
    file: PathBuf,
    stretch: Option<Stretch>,
}

impl MediaLocation {
    /// A media file that is a file of its own.
    pub fn of_a_file(path: impl Into<PathBuf>) -> Self {
        Self {
            file: path.into(),
            stretch: None,
        }
    }

    /// A media file that is the `length` bytes of `file` from the byte `start` on.
    pub fn of_a_stretch_of(file: impl Into<PathBuf>, start: u64, length: u64) -> Self {
        Self {
            file: file.into(),
            stretch: Some(Stretch { start, length }),
        }
    }

    /// The file that has the bytes: the media file itself, or the file that it is a stretch of.
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// Which bytes of [`Self::file`] they are, if not all of it.
    pub fn stretch(&self) -> Option<Stretch> {
        self.stretch
    }

    /// The media file as a file of its own, if it is one.
    pub fn as_a_file_of_its_own(&self) -> Option<&Path> {
        self.stretch.is_none().then_some(self.file.as_path())
    }

    /// Whether the file that has the bytes is there. (Not whether the stretch is: that is for reading it to find out.)
    pub fn file_exists(&self) -> bool {
        self.file.exists()
    }

    /// What to give an ffmpeg tool as its input (the text after `-i`): the path of a file of its own, and for a stretch the form
    /// of the `subfile` protocol, which reads that stretch of the file and nothing else.
    pub fn ffmpeg_input(&self) -> OsString {
        match self.stretch {
            None => self.file.clone().into_os_string(),
            Some(Stretch { start, length }) => {
                // (The first comma after `subfile` says which character separates the options, and two of them in a row end the
                // options: what comes after the colon is the path, whatever characters it has.)
                let mut input = OsString::from(format!("subfile,,start,{start},end,{},,:", start + length));
                input.push(&self.file);
                input
            }
        }
    }
}

impl From<PathBuf> for MediaLocation {
    fn from(path: PathBuf) -> Self {
        Self::of_a_file(path)
    }
}

impl From<&Path> for MediaLocation {
    fn from(path: &Path) -> Self {
        Self::of_a_file(path)
    }
}

impl From<&PathBuf> for MediaLocation {
    fn from(path: &PathBuf) -> Self {
        Self::of_a_file(path.as_path())
    }
}

impl From<&str> for MediaLocation {
    fn from(path: &str) -> Self {
        Self::of_a_file(path)
    }
}

impl From<&MediaLocation> for MediaLocation {
    fn from(location: &MediaLocation) -> Self {
        location.clone()
    }
}

/// How the place is told to the user: the path of a file, and for a stretch the file that it is in.
impl fmt::Display for MediaLocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.stretch {
            None => write!(formatter, "{}", self.file.display()),
            Some(_) => write!(formatter, "{} (a file in it)", self.file.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_of_its_own_is_given_to_ffmpeg_as_its_path() {
        let location = MediaLocation::of_a_file("videos/holiday.mp4");

        assert_eq!(location.ffmpeg_input(), OsString::from("videos/holiday.mp4"));
        assert_eq!(location.file(), Path::new("videos/holiday.mp4"));
        assert_eq!(location.stretch(), None);
        assert_eq!(location.as_a_file_of_its_own(), Some(Path::new("videos/holiday.mp4")));
    }

    #[test]
    fn a_stretch_of_a_file_is_given_to_ffmpeg_as_the_subfile_protocol_with_where_it_begins_and_where_it_ends() {
        let location = MediaLocation::of_a_stretch_of("C:/projects/trip.ffrs", 107, 337_235);

        assert_eq!(
            location.ffmpeg_input(),
            OsString::from("subfile,,start,107,end,337342,,:C:/projects/trip.ffrs")
        );
        assert_eq!(location.file(), Path::new("C:/projects/trip.ffrs"));
        assert_eq!(
            location.stretch(),
            Some(Stretch {
                start: 107,
                length: 337_235
            })
        );
        assert_eq!(location.as_a_file_of_its_own(), None);
    }

    #[test]
    fn a_path_with_commas_colons_spaces_and_other_letters_is_given_as_it_is_after_the_options() {
        let location = MediaLocation::of_a_stretch_of("D:/Ferien, Österreich: (1) 'a'/trip,v2.ffrs", 5, 10);

        assert_eq!(
            location.ffmpeg_input(),
            OsString::from("subfile,,start,5,end,15,,:D:/Ferien, Österreich: (1) 'a'/trip,v2.ffrs")
        );
    }

    #[test]
    fn a_stretch_of_a_file_longer_than_four_gigabytes_has_its_ends_in_full() {
        let location = MediaLocation::of_a_stretch_of("big.ffrs", 4_294_967_400, 5_000_000_000);

        assert_eq!(
            location.ffmpeg_input(),
            OsString::from("subfile,,start,4294967400,end,9294967400,,:big.ffrs")
        );
    }

    #[test]
    fn a_place_is_told_to_the_user_as_the_path_and_for_a_stretch_the_file_it_is_in() {
        assert_eq!(MediaLocation::of_a_file("a/b.mp4").to_string(), "a/b.mp4");
        assert_eq!(
            MediaLocation::of_a_stretch_of("a/trip.ffrs", 1, 2).to_string(),
            "a/trip.ffrs (a file in it)"
        );
    }

    #[test]
    fn two_places_are_the_same_only_if_the_file_and_the_stretch_are() {
        let a = MediaLocation::of_a_stretch_of("x.ffrs", 10, 20);
        assert_eq!(a, MediaLocation::of_a_stretch_of("x.ffrs", 10, 20));
        assert_ne!(a, MediaLocation::of_a_stretch_of("x.ffrs", 10, 21));
        assert_ne!(a, MediaLocation::of_a_stretch_of("x.ffrs", 11, 20));
        assert_ne!(a, MediaLocation::of_a_stretch_of("y.ffrs", 10, 20));
        assert_ne!(a, MediaLocation::of_a_file("x.ffrs"));
    }

    #[test]
    fn a_path_converts_into_a_file_of_its_own() {
        let from_a_buffer: MediaLocation = PathBuf::from("a.mp4").into();
        let from_a_path: MediaLocation = Path::new("a.mp4").into();
        let from_text: MediaLocation = "a.mp4".into();

        assert_eq!(from_a_buffer, MediaLocation::of_a_file("a.mp4"));
        assert_eq!(from_a_path, from_a_buffer);
        assert_eq!(from_text, from_a_buffer);
    }
}
