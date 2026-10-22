// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Base 64 (RFC 4648, the alphabet with `+` and `/`, padded with `=`), for the images that a shader brings with it, which a project file keeps as text (phase 6.4.4). It is here and
//! not a crate because it is forty lines and nothing else in the program needs more than this.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `bytes` as base 64, with the padding.
pub fn encode(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let value = (u32::from(group[0]) << 16)
            | (u32::from(*group.get(1).unwrap_or(&0)) << 8)
            | u32::from(*group.get(2).unwrap_or(&0));
        for place in 0..4 {
            // (A group of one byte has two characters and two paddings; of two bytes, three and one.)
            if place > group.len() {
                text.push('=');
            } else {
                text.push(char::from(ALPHABET[((value >> (18 - 6 * place)) & 63) as usize]));
            }
        }
    }
    text
}

fn value_of(character: u8) -> Option<u32> {
    Some(match character {
        b'A'..=b'Z' => u32::from(character - b'A'),
        b'a'..=b'z' => u32::from(character - b'a') + 26,
        b'0'..=b'9' => u32::from(character - b'0') + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    })
}

/// The bytes that `text` says, or what is wrong with it. The padding is wanted (the length is a multiple of 4) and nothing but the padding may come after it; spaces and line ends
/// are not allowed (a project file does not break the lines of it).
pub fn decode(text: &str) -> Result<Vec<u8>, String> {
    let characters = text.as_bytes();
    if !characters.len().is_multiple_of(4) {
        return Err("its length is not a multiple of 4".to_owned());
    }
    let mut bytes = Vec::with_capacity(characters.len() / 4 * 3);
    let groups = characters.len() / 4;
    for (number, group) in characters.chunks(4).enumerate() {
        let padding = group.iter().rev().take_while(|character| **character == b'=').count();
        if padding > 2 || (padding > 0 && number + 1 != groups) {
            return Err("the padding is where it cannot be".to_owned());
        }
        let mut value = 0_u32;
        for (place, character) in group.iter().enumerate() {
            let sextet = if place >= 4 - padding {
                0
            } else {
                value_of(*character)
                    .ok_or_else(|| format!("{:?} is not a character of base 64", char::from(*character)))?
            };
            value = (value << 6) | sextet;
        }
        let [_, first, second, third] = value.to_be_bytes();
        bytes.extend_from_slice(&[first, second, third][..3 - padding]);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests;
