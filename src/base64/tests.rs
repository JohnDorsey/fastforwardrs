// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The examples of RFC 4648, section 10.
const EXAMPLES: [(&str, &str); 7] = [
    ("", ""),
    ("f", "Zg=="),
    ("fo", "Zm8="),
    ("foo", "Zm9v"),
    ("foob", "Zm9vYg=="),
    ("fooba", "Zm9vYmE="),
    ("foobar", "Zm9vYmFy"),
];

#[test]
fn the_examples_of_the_standard_are_what_it_says_both_ways() {
    for (plain, encoded) in EXAMPLES {
        assert_eq!(encode(plain.as_bytes()), encoded, "{plain:?}");
        assert_eq!(decode(encoded).unwrap(), plain.as_bytes(), "{encoded:?}");
    }
}

#[test]
fn every_byte_value_and_every_length_survive_the_trip() {
    let all: Vec<u8> = (0..=255).collect();
    assert_eq!(decode(&encode(&all)).unwrap(), all);
    for length in 0..40 {
        let bytes: Vec<u8> = (0..length).map(|number| (number * 37 + 11) as u8).collect();
        assert_eq!(decode(&encode(&bytes)).unwrap(), bytes, "{length} bytes");
    }
}

#[test]
fn the_last_two_characters_of_the_alphabet_are_plus_and_slash() {
    assert_eq!(encode(&[0xfb, 0xff, 0xbe]), "+/++");
    assert_eq!(decode("+/++").unwrap(), [0xfb, 0xff, 0xbe]);
    assert_eq!(encode(&[0xff, 0xff, 0xff]), "////");
    assert_eq!(decode("////").unwrap(), [0xff, 0xff, 0xff]);
}

#[test]
fn what_is_not_base_64_is_refused_and_says_why() {
    assert!(decode("Zm9").unwrap_err().contains("multiple of 4"));
    assert!(decode("Zm9vY").unwrap_err().contains("multiple of 4"));
    assert!(decode("Zm9-").unwrap_err().contains("is not a character"));
    assert!(decode("Zm 9").unwrap_err().contains("is not a character"));
    assert!(decode("Zm9\n").unwrap_err().contains("is not a character"));
    assert!(
        decode("Zg==Zg==").unwrap_err().contains("padding"),
        "padding in the middle"
    );
    assert!(decode("Z===").unwrap_err().contains("padding"), "three paddings");
    assert!(
        decode("=Zg=").unwrap_err().contains("is not a character"),
        "padding at the front"
    );
}

#[test]
fn the_bits_that_the_padding_leaves_over_are_not_looked_at() {
    // "Zh==" has a 1 where "Zg==" has a 0 in the bits that no byte takes; the standard says a decoder may refuse it, and this one reads the same byte.
    assert_eq!(decode("Zh==").unwrap(), b"f");
}
