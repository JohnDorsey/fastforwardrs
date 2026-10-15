// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! The book that the pools of the engine are kept in: things that are not in use at the moment, by what kind of thing each is, which the next
//! user of that kind takes instead of making a new one. It is generic and knows nothing of the graphics card, so that its rules can be tested alone.

use std::collections::HashMap;
use std::hash::Hash;

/// Idle things by kind. The one that was given back most recently is taken first (it is the most likely to still be where the card keeps what it
/// is working on), and when there are too many, the one that has been idle the longest is let go first.
#[derive(Debug)]
pub struct Recycler<Kind: Eq + Hash + Copy, Item> {
    idle: HashMap<Kind, Vec<(u64, Item)>>,
    next_sequence_number: u64,
}

impl<Kind: Eq + Hash + Copy, Item> Default for Recycler<Kind, Item> {
    fn default() -> Self {
        Self {
            idle: HashMap::new(),
            next_sequence_number: 0,
        }
    }
}

impl<Kind: Eq + Hash + Copy, Item> Recycler<Kind, Item> {
    /// An idle one of this kind, if there is one, which is not idle any more.
    pub fn take(&mut self, kind: &Kind) -> Option<Item> {
        let items = self.idle.get_mut(kind)?;
        let (_, item) = items.pop()?;
        if items.is_empty() {
            self.idle.remove(kind);
        }
        Some(item)
    }

    /// Puts a thing of this kind that is not used any more where it can be taken from.
    pub fn give(&mut self, kind: Kind, item: Item) {
        let sequence_number = self.next_sequence_number;
        self.next_sequence_number += 1;
        self.idle.entry(kind).or_default().push((sequence_number, item));
    }

    /// How many idle things there are, of all kinds.
    pub fn idle_count(&self) -> usize {
        self.idle.values().map(Vec::len).sum()
    }

    /// How many idle things there are of this kind.
    #[cfg(test)]
    pub fn idle_count_of(&self, kind: &Kind) -> usize {
        self.idle.get(kind).map_or(0, Vec::len)
    }

    /// Lets go of idle things, the ones that have been idle the longest first, until what is left adds up to no more than `budget` (each
    /// of a kind counts as `size_of` that kind says). Returns how many were let go.
    pub fn trim(&mut self, budget: u64, size_of: impl Fn(&Kind) -> u64) -> usize {
        let mut total: u64 = self
            .idle
            .iter()
            .map(|(kind, items)| size_of(kind) * items.len() as u64)
            .sum();
        let mut let_go = 0;
        while total > budget {
            let oldest = self
                .idle
                .iter()
                .filter_map(|(kind, items)| Some((*kind, items.first()?.0)))
                .min_by_key(|(_, sequence_number)| *sequence_number);
            let Some((kind, _)) = oldest else { break };
            let items = self.idle.get_mut(&kind).expect("a kind that was just found is there");
            items.remove(0);
            if items.is_empty() {
                self.idle.remove(&kind);
            }
            total -= size_of(&kind);
            let_go += 1;
        }
        let_go
    }

    /// Lets go of everything that is idle.
    pub fn clear(&mut self) {
        self.idle.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_idle_gives_nothing() {
        let mut recycler: Recycler<u32, &str> = Recycler::default();
        assert_eq!(recycler.take(&1), None);
        assert_eq!(recycler.idle_count(), 0);
    }

    #[test]
    fn a_thing_given_back_is_taken_by_its_own_kind_only_and_only_once() {
        let mut recycler = Recycler::default();
        recycler.give(1, "one");
        recycler.give(2, "two");
        assert_eq!(recycler.take(&3), None);
        assert_eq!(recycler.take(&2), Some("two"));
        assert_eq!(recycler.take(&2), None);
        assert_eq!(recycler.take(&1), Some("one"));
        assert_eq!(recycler.idle_count(), 0);
    }

    #[test]
    fn the_thing_given_back_last_is_taken_first() {
        let mut recycler = Recycler::default();
        recycler.give(1, "first");
        recycler.give(1, "second");
        recycler.give(1, "third");
        assert_eq!(recycler.idle_count_of(&1), 3);
        assert_eq!(recycler.take(&1), Some("third"));
        assert_eq!(recycler.take(&1), Some("second"));
        assert_eq!(recycler.take(&1), Some("first"));
    }

    #[test]
    fn trimming_lets_go_of_what_has_been_idle_the_longest_first_whatever_its_kind() {
        let mut recycler = Recycler::default();
        recycler.give(1, "a");
        recycler.give(2, "b");
        recycler.give(1, "c");
        recycler.give(2, "d");
        // Every one counts as 10: room for two of the four is a budget of 20, and "a" and "b" go.
        let let_go = recycler.trim(20, |_| 10);
        assert_eq!(let_go, 2);
        assert_eq!(recycler.idle_count(), 2);
        assert_eq!(recycler.take(&1), Some("c"));
        assert_eq!(recycler.take(&2), Some("d"));
    }

    #[test]
    fn trimming_counts_each_kind_at_its_own_size() {
        let mut recycler: Recycler<u32, &str> = Recycler::default();
        recycler.give(1, "small");
        recycler.give(100, "big");
        recycler.give(1, "small again");
        // 1 + 100 + 1 = 102 is over 101: only the oldest goes, which is the small one.
        let let_go = recycler.trim(101, |kind| u64::from(*kind));
        assert_eq!(let_go, 1);
        assert_eq!(recycler.take(&100), Some("big"));
        assert_eq!(recycler.take(&1), Some("small again"));
    }

    #[test]
    fn trimming_within_the_budget_lets_go_of_nothing_and_trimming_to_nothing_lets_go_of_everything() {
        let mut recycler = Recycler::default();
        recycler.give(1, "a");
        recycler.give(1, "b");
        assert_eq!(recycler.trim(20, |_| 10), 0);
        assert_eq!(recycler.idle_count(), 2);
        assert_eq!(recycler.trim(0, |_| 10), 2);
        assert_eq!(recycler.idle_count(), 0);
        assert_eq!(recycler.trim(0, |_| 10), 0, "nothing left to let go of");
    }

    #[test]
    fn clearing_lets_go_of_everything() {
        let mut recycler = Recycler::default();
        recycler.give(1, "a");
        recycler.give(2, "b");
        recycler.clear();
        assert_eq!(recycler.idle_count(), 0);
        assert_eq!(recycler.take(&1), None);
    }
}
