// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Things kept by a key, up to so many bytes (and, if wanted, so many things), of which the one that was used least recently is let go first.
//!
//! It has no graphics card in it: the finished frames ([`super::FrameCache`]) and the pictures on the card (`picture_cache`) are both kept by it, so
//! that the rules of keeping are written and tested once.

use std::collections::HashMap;
use std::hash::Hash;

/// See the module documentation.
#[derive(Debug)]
pub struct ByteLru<Key: Eq + Hash + Copy, Value> {
    entries: HashMap<Key, Entry<Value>>,
    most_items: Option<usize>,
    most_bytes: usize,
    bytes: usize,
    use_counter: u64,
}

#[derive(Debug)]
struct Entry<Value> {
    value: Value,
    size_in_bytes: usize,
    last_used: u64,
}

/// What came of keeping a thing.
#[derive(Debug, PartialEq, Eq)]
pub enum Kept {
    /// It is kept.
    Yes,
    /// It is bigger than all that is wanted, so it is not kept (and what was kept under its key, if anything, is gone).
    TooBig,
}

impl<Key: Eq + Hash + Copy, Value> ByteLru<Key, Value> {
    pub fn new(most_items: Option<usize>, most_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            most_items,
            most_bytes,
            bytes: 0,
            use_counter: 0,
        }
    }

    /// The thing kept under `key`, which then counts as used just now.
    pub fn get(&mut self, key: &Key) -> Option<&Value> {
        self.use_counter += 1;
        let entry = self.entries.get_mut(key)?;
        entry.last_used = self.use_counter;
        Some(&entry.value)
    }

    /// Counts what is kept under `key` as used just now, if something is.
    pub fn touch(&mut self, key: &Key) {
        self.use_counter += 1;
        if let Some(entry) = self.entries.get_mut(key) {
            entry.last_used = self.use_counter;
        }
    }

    /// The thing kept under `key`, without counting it as used.
    pub fn peek(&self, key: &Key) -> Option<&Value> {
        self.entries.get(key).map(|entry| &entry.value)
    }

    pub fn contains(&self, key: &Key) -> bool {
        self.entries.contains_key(key)
    }

    /// Keeps `value`, which takes up `size_in_bytes`, under `key` (instead of what was kept under it), and lets go of the things used least recently
    /// while there are more than are wanted. The thing just kept is the one used most recently, so it is the last to go.
    pub fn insert(&mut self, key: Key, value: Value, size_in_bytes: usize) -> Kept {
        self.remove(&key);
        if size_in_bytes > self.most_bytes {
            return Kept::TooBig;
        }
        self.use_counter += 1;
        self.bytes += size_in_bytes;
        self.entries.insert(
            key,
            Entry {
                value,
                size_in_bytes,
                last_used: self.use_counter,
            },
        );
        while self.most_items.is_some_and(|most| self.entries.len() > most) || self.bytes > self.most_bytes {
            let Some((&least_recently_used, _)) = self.entries.iter().min_by_key(|(_, entry)| entry.last_used) else {
                break;
            };
            self.remove(&least_recently_used);
        }
        Kept::Yes
    }

    /// Lets go of what is kept under `key`, and gives it.
    pub fn remove(&mut self, key: &Key) -> Option<Value> {
        let entry = self.entries.remove(key)?;
        self.bytes -= entry.size_in_bytes;
        Some(entry.value)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many bytes the kept things take up.
    pub fn byte_count(&self) -> usize {
        self.bytes
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_kept_is_found_by_its_key_and_nothing_else_is() {
        let mut lru = ByteLru::new(None, 100);
        assert_eq!(lru.insert(1, "one", 10), Kept::Yes);
        assert_eq!(lru.get(&1), Some(&"one"));
        assert_eq!(lru.peek(&1), Some(&"one"));
        assert_eq!(lru.get(&2), None);
        assert!(lru.contains(&1));
        assert!(!lru.contains(&2));
        assert_eq!((lru.len(), lru.byte_count()), (1, 10));
    }

    #[test]
    fn a_thing_that_takes_up_exactly_the_bytes_that_are_wanted_is_kept_and_one_byte_more_is_too_big() {
        let mut lru = ByteLru::new(None, 100);
        lru.insert(1, "small", 40);
        assert_eq!(lru.insert(2, "as big as it can be", 100), Kept::Yes);
        assert!(lru.contains(&2));
        assert!(!lru.contains(&1), "and what was there is let go of to make room for it");
        assert_eq!((lru.len(), lru.byte_count()), (1, 100));
        assert_eq!(lru.insert(3, "one byte too big", 101), Kept::TooBig);
        assert!(!lru.contains(&3));
        assert!(lru.contains(&2), "and a thing that is too big lets go of nothing");
        assert_eq!((lru.len(), lru.byte_count()), (1, 100));
    }

    #[test]
    fn the_thing_used_least_recently_goes_first_when_there_are_too_many_things() {
        let mut lru = ByteLru::new(Some(2), 1000);
        lru.insert(1, "one", 1);
        lru.insert(2, "two", 1);
        lru.get(&1);
        lru.insert(3, "three", 1);
        assert!(!lru.contains(&2), "2 was used least recently");
        assert!(lru.contains(&1) && lru.contains(&3));
    }

    #[test]
    fn touching_counts_as_using_and_peeking_does_not() {
        let mut lru = ByteLru::new(Some(2), 1000);
        lru.insert(1, "one", 1);
        lru.insert(2, "two", 1);
        lru.peek(&1);
        lru.insert(3, "three", 1);
        assert!(!lru.contains(&1), "peeking at 1 did not save it");

        let mut lru = ByteLru::new(Some(2), 1000);
        lru.insert(1, "one", 1);
        lru.insert(2, "two", 1);
        lru.touch(&1);
        lru.insert(3, "three", 1);
        assert!(!lru.contains(&2), "touching 1 saved it, and 2 went instead");
        lru.touch(&99); // nothing is kept under it: no harm
    }

    #[test]
    fn things_go_while_there_are_too_many_bytes_and_one_big_thing_may_push_out_several() {
        let mut lru = ByteLru::new(None, 100);
        lru.insert(1, "a", 40);
        lru.insert(2, "b", 40);
        assert_eq!(lru.insert(3, "c", 80), Kept::Yes);
        assert!(!lru.contains(&1) && !lru.contains(&2) && lru.contains(&3));
        assert_eq!(lru.byte_count(), 80);
    }

    #[test]
    fn a_thing_bigger_than_all_that_is_wanted_is_not_kept_and_does_not_push_the_others_out() {
        let mut lru = ByteLru::new(None, 100);
        lru.insert(1, "a", 40);
        assert_eq!(lru.insert(2, "big", 400), Kept::TooBig);
        assert!(!lru.contains(&2));
        assert!(lru.contains(&1));
        assert_eq!(lru.byte_count(), 40);
    }

    #[test]
    fn keeping_another_thing_under_the_same_key_replaces_it_and_counts_its_bytes_once() {
        let mut lru = ByteLru::new(None, 1000);
        lru.insert(1, "old", 40);
        lru.insert(1, "new", 60);
        assert_eq!((lru.len(), lru.byte_count()), (1, 60));
        assert_eq!(lru.get(&1), Some(&"new"));
        // Replacing it with one that is too big loses the old one and keeps nothing.
        assert_eq!(lru.insert(1, "huge", 4000), Kept::TooBig);
        assert!(lru.is_empty());
        assert_eq!(lru.byte_count(), 0);
    }

    #[test]
    fn removing_gives_the_thing_back_and_its_bytes_are_not_counted_any_more() {
        let mut lru = ByteLru::new(None, 1000);
        lru.insert(1, "a", 40);
        lru.insert(2, "b", 60);
        assert_eq!(lru.remove(&1), Some("a"));
        assert_eq!(lru.remove(&1), None);
        assert_eq!(lru.byte_count(), 60);
        lru.clear();
        assert!(lru.is_empty());
        assert_eq!(lru.byte_count(), 0);
    }

    #[test]
    fn if_no_things_at_all_are_wanted_nothing_stays() {
        let mut lru = ByteLru::new(Some(0), 1000);
        // (Not even the one just kept stays.)
        assert_eq!(lru.insert(1, "a", 1), Kept::Yes);
        assert!(lru.is_empty());
    }
}
