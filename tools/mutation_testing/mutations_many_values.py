# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""Planted mistakes for the removal of the limit of 255 values of an effect (2026-11-03): the bound of a descriptor, the check for two values with one id, the slot of a value as a
property (it is a `u16`; a mistake that keeps only a byte of it), and the driven values (their places are `u16` too).

    py tools/mutation_testing/mutate.py tools/mutation_testing/mutations_many_values.py --no-controls --results first.jsonl

Mistakes that cannot be seen by the tests, said plainly (left out of the list, not forgotten): a place above 255 for a value that is read from a mask in the project file and in the picture
of an effect (only the numbers of a shader can be read from a mask, and no shader has so many).
"""

import os

TESTS = os.environ.get("MUTATION_TESTS", "--lib -- project::effects")

DESCRIPTOR = "src/project/effects/descriptor.rs"
PROPERTY = "src/project/animation/property.rs"
DRIVEN = "src/project/effects/driven.rs"

MUTATIONS = [
    # --- the bound and the check of a descriptor ---
    ("bound: the most values is still 255", DESCRIPTOR, "pub const MOST_VALUES_OF_AN_EFFECT: usize = u16::MAX as usize;", "pub const MOST_VALUES_OF_AN_EFFECT: usize = 255;", TESTS),
    ("bound: the most values is one too few", DESCRIPTOR, "if self.parameters.len() > MOST_VALUES_OF_AN_EFFECT {", "if self.parameters.len() >= MOST_VALUES_OF_AN_EFFECT {", TESTS),
    ("bound: the most values is one too many", DESCRIPTOR, "if self.parameters.len() > MOST_VALUES_OF_AN_EFFECT {", "if self.parameters.len() > MOST_VALUES_OF_AN_EFFECT + 1 {", TESTS),
    ("ids: two values with one id are never found", DESCRIPTOR, "if !ids_met.insert(&*parameter.id) {", "if !ids_met.insert(&*parameter.id) && false {", TESTS),
    ("ids: only the first value is kept in mind", DESCRIPTOR, "if !ids_met.insert(&*parameter.id) {", "if place > 0 && !ids_met.insert(&*parameter.id) {", TESTS),
    # --- the slot of a value ---
    ("slot: the kind of a value is that of the place in a byte", PROPERTY, "effect.value_kind(usize::from(slot)).unwrap_or(ValueKind::Number)", "effect.value_kind(usize::from(slot as u8)).unwrap_or(ValueKind::Number)", TESTS),
    ("slot: the name of a value is that of the place in a byte", PROPERTY,
     ".get(usize::from(slot))\n                .map_or(\"value of an effect\", |parameter| &parameter.name)", ".get(usize::from(slot as u8))\n                .map_or(\"value of an effect\", |parameter| &parameter.name)", TESTS),
    ("slot: the name of a value in a file is that of the place in a byte", PROPERTY,
     ".get(usize::from(slot))\n                .map_or(\"value of an effect\", |parameter| &parameter.id)", ".get(usize::from(slot as u8))\n                .map_or(\"value of an effect\", |parameter| &parameter.id)", TESTS),
    # --- the driven values ---
    ("driven: the places are put in order by a byte of them", DRIVEN, "values.sort_by_key(|(place, _)| *place);", "values.sort_by_key(|(place, _)| *place as u8);", TESTS),
    ("driven: a place is told by a byte of it", DRIVEN, ".find(|(found, _)| usize::from(*found) == place)", ".find(|(found, _)| usize::from(*found as u8) == place % 256)", TESTS),
    ("driven: letting go of a place lets go of the one 256 before it as well", DRIVEN, ".filter(|(found, _)| *found != place)", ".filter(|(found, _)| (*found as u8) != (place as u8))", TESTS),
]
