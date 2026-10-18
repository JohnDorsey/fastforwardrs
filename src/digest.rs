// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! SHA-256, for telling one file from another where being wrong would matter: whether a plugin that a person approved is the file that is about to be loaded, and
//! whether the plugin that a project carries is the one it says it carries. (The CRC-32 that names the entries of resources is for telling versions apart and trusts the
//! archive to check the bytes; this is not that.)

use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

/// The SHA-256 of some bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    /// The digest of the contents of the file at `path`, read in pieces (so a big file is not held in memory).
    pub fn of_file(path: &Path) -> io::Result<Self> {
        let mut file = fs::File::open(path)?;
        let mut hasher = Sha256::new();
        let mut piece = vec![0_u8; 1 << 16];
        loop {
            let count = file.read(&mut piece)?;
            if count == 0 {
                return Ok(Self(hasher.finalize().into()));
            }
            hasher.update(&piece[..count]);
        }
    }

    /// 64 characters, lower case.
    pub fn to_hex(self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// The digest that `text` (64 hexadecimal digits, in either case) is. `None` if it is not one.
    pub fn from_hex(text: &str) -> Option<Self> {
        if text.len() != 64 || !text.is_ascii() {
            return None;
        }
        let mut bytes = [0_u8; 32];
        for (place, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[place * 2..place * 2 + 2], 16).ok()?;
        }
        Some(Self(bytes))
    }

    /// The first 16 characters of [`Self::to_hex`], which is enough to tell the files of one project apart and is what the names of entries are made of.
    pub fn short_hex(self) -> String {
        self.to_hex()[..16].to_owned()
    }
}

impl fmt::Debug for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "sha256:{}", self.short_hex())
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digests_of_the_published_test_vectors() {
        // FIPS 180-4 / NIST examples.
        assert_eq!(
            Sha256Digest::of_bytes(b"").to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            Sha256Digest::of_bytes(b"abc").to_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            Sha256Digest::of_bytes(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq").to_hex(),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn a_file_has_the_digest_its_bytes_have_however_long_it_is() {
        let folder = tempfile::tempdir().unwrap();
        let bytes: Vec<u8> = (0..300_000_u32).map(|number| (number % 251) as u8).collect();
        let path = folder.path().join("long.bin");
        fs::write(&path, &bytes).unwrap();
        assert_eq!(Sha256Digest::of_file(&path).unwrap(), Sha256Digest::of_bytes(&bytes));
        assert!(Sha256Digest::of_file(&folder.path().join("not there")).is_err());
    }

    #[test]
    fn a_digest_is_written_as_hex_and_read_back_and_what_is_not_one_is_refused() {
        let digest = Sha256Digest::of_bytes(b"abc");
        assert_eq!(Sha256Digest::from_hex(&digest.to_hex()), Some(digest));
        assert_eq!(Sha256Digest::from_hex(&digest.to_hex().to_uppercase()), Some(digest));
        assert_eq!(digest.short_hex(), "ba7816bf8f01cfea");
        assert_eq!(format!("{digest}"), digest.to_hex());
        assert_eq!(format!("{digest:?}"), "sha256:ba7816bf8f01cfea");
        for bad in [
            "",
            "ba78",
            &"g".repeat(64),
            &format!("{}0", digest.to_hex()),
            "é".repeat(32).as_str(),
        ] {
            assert_eq!(Sha256Digest::from_hex(bad), None, "{bad:?}");
        }
    }
}
