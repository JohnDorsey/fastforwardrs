# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""What the mutation runner decides without starting a program: which mistakes to plant, how to plant one in a text, which tests to
run first, and what a run of `cargo test` printed means. It is all plain functions on text, so that `test_mutation_core.py` can check
it without cargo (and `test_mutate_end_to_end.py` checks it together with the real cargo)."""

from __future__ import annotations

import dataclasses
import importlib.util
import itertools
import pathlib
import re
from collections.abc import Iterable, Sequence

# ---- the mistakes -------------------------------------------------------------------------------------------------------------------


@dataclasses.dataclass(frozen=True)
class Mutation:
    """One deliberate mistake: `old` is changed to `new` in the file `path` of the project (with forward slashes)."""

    name: str
    path: str
    # The text to change. It must be in the file exactly once, so that it is clear which place is meant.
    old: str
    new: str
    # What to give `cargo test` instead of the library's whole suite, for example "--test application_journeys" or "--lib -- a_test".
    # A mistake that has this is *aimed*: it counts as noticed only if what is run fails, and what is run is all that is run.
    cargo_arguments: str | None = None


def load_mutations(file: pathlib.Path) -> list[Mutation]:
    """The mistakes of a Python file that defines MUTATIONS = [(name, path, old, new), ...], each with an optional fifth item
    (`Mutation.cargo_arguments`). Files may import each other (to run a part of another file again)."""
    specification = importlib.util.spec_from_file_location("mutations_file", file)
    assert specification is not None and specification.loader is not None, file
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    mutations = []
    for number, entry in enumerate(module.MUTATIONS):
        if len(entry) not in (4, 5) or not all(isinstance(part, str) for part in entry):
            raise ValueError(f"{file}: mutation number {number} is not (name, path, old, new) with an optional fifth text: {entry!r}")
        mutations.append(Mutation(*entry))
    return mutations


def plant(text: str, mutation: Mutation) -> str | None:
    """The text with the mistake in it, or None when the text to change is not in it exactly once."""
    old, new = mutation.old, mutation.new
    if "\r\n" in text:
        # The files of a working tree can have Windows line endings: so must the text to find.
        old, new = old.replace("\n", "\r\n"), new.replace("\n", "\r\n")
    if text.count(old) != 1:
        return None
    return text.replace(old, new, 1)


def how_many_times_the_text_to_change_is_in(text: str, mutation: Mutation) -> int:
    old = mutation.old.replace("\n", "\r\n") if "\r\n" in text else mutation.old
    return text.count(old)


# ---- which tests first --------------------------------------------------------------------------------------------------------------


def module_path_of(source_file: str) -> str | None:
    """The path of the module that a file of the library is, as it begins the names of the tests in it: src/a/b/c.rs and
    src/a/b/c/mod.rs are both "a::b::c". None for the files that are not a module of the library (lib.rs, main.rs, src/bin)."""
    parts = pathlib.PurePosixPath(source_file).parts
    if len(parts) < 2 or parts[0] != "src" or not parts[-1].endswith(".rs") or parts[1] == "bin":
        return None
    modules = [*parts[1:-1], parts[-1].removesuffix(".rs")]
    if modules[-1] == "mod":
        modules.pop()
    if modules in (["lib"], ["main"]):
        return None
    return "::".join(modules) or None


@dataclasses.dataclass(frozen=True)
class Stage:
    """One run of `cargo test` for a mistake: `name` says why it is run, and `cargo_arguments` is what follows `cargo test`."""

    name: str
    cargo_arguments: tuple[str, ...]


def stages_for(mutation: Mutation, learned_tests: Sequence[str] = (), *, in_stages: bool = True) -> list[Stage]:
    """The runs of `cargo test` that decide whether a mistake is noticed, in order; the next is only needed if the one before found
    nothing wrong.

    A mistake that is *aimed* (it has cargo_arguments) is run as asked, once. Of the others the whole suite of the library has to be
    run, and that is done in two parts: first the tests that are most likely to notice it (the ones of the module of the file that
    has the mistake, and `learned_tests`, the ones that noticed other mistakes in the same file), then all the rest. Together they
    are exactly the whole suite, so the verdict is the one the whole suite would give; the first part only makes it quick to say
    "noticed". With in_stages=False the suite is run as one."""
    if mutation.cargo_arguments is not None:
        return [Stage("aimed", tuple(mutation.cargo_arguments.split()))]
    first = [*([module_path_of(mutation.path)] if module_path_of(mutation.path) else []), *learned_tests]
    first = list(dict.fromkeys(first))  # without repeats, in order
    if not in_stages or not first:
        return [Stage("all", ("--lib",))]
    skipping = itertools.chain.from_iterable(("--skip", filter_text) for filter_text in first)
    return [
        Stage("the tests of its module", ("--lib", "--", *first)),
        Stage("all the other tests", ("--lib", "--", *skipping)),
    ]


_CARGO_FLAGS_WITH_A_VALUE = {"--test", "--bin", "--example", "--bench", "-p", "--package", "--features", "-F", "--target", "--profile"}


def arguments_to_run_exactly(cargo_arguments: str | None, test_names: Iterable[str]) -> tuple[str, ...]:
    """The arguments of `cargo test` that run exactly the tests named, in the same program (and with the same flags, such as
    --ignored) that the mistake with these cargo_arguments was run in. Used to check that the tests that noticed a mistake pass
    without it."""
    tokens = cargo_arguments.split() if cargo_arguments is not None else ["--lib"]
    for_cargo: list[str] = []
    for_the_tests: list[str] = []
    after_the_dashes = False
    previous_takes_a_value = False
    for token in tokens:
        if token == "--":
            after_the_dashes = True
        elif after_the_dashes:
            if token.startswith("-"):
                for_the_tests.append(token)
        elif token.startswith("-") or previous_takes_a_value:
            for_cargo.append(token)
            previous_takes_a_value = token in _CARGO_FLAGS_WITH_A_VALUE
            continue
        # (A word that is not a flag, and not the value of one, is a name to filter by: the names are put in its place.)
        previous_takes_a_value = False
    return (*for_cargo, "--", *for_the_tests, "--exact", *test_names)


def arguments_leaving_out(cargo_arguments: Sequence[str], test_names: Iterable[str]) -> tuple[str, ...]:
    """The arguments of `cargo test` that run what `cargo_arguments` run, except the tests named (each is skipped by its whole name). Used
    to go on with a run that was stopped by a test that failed once and passed when it was run again."""
    names = list(dict.fromkeys(test_names))
    if not names:
        return tuple(cargo_arguments)
    arguments = list(cargo_arguments)
    if "--" not in arguments:
        arguments.append("--")
    for name in names:
        arguments += ["--skip", name]
    return tuple(arguments)


# ---- what a run printed -------------------------------------------------------------------------------------------------------------

CAUGHT = "caught"
NOT_CAUGHT = "NOT CAUGHT"
DID_NOT_COMPILE = "DID NOT COMPILE"
HUNG = "hung"
CRASHED = "crashed"

FINISHED = "finished"
STOPPED_AT_THE_FIRST_FAILURE = "stopped at the first failure"
TOOK_TOO_LONG = "took too long"

_FAILED = re.compile(r"test (\S+) \.\.\. FAILED\s*$")
_RUNNING = re.compile(r"\s*Running ")
_PROGRAM_STARTED = re.compile(r"\s*Running .*\((?P<program>[^()]+)\)\s*$")
# What is printed when a file that has to be written, or a program that has to be started, is held by another program for a moment (the
# program of the tests that was killed a moment ago and has not quite gone, or a scan of a program that was just made): nothing to do with the
# mistake, and gone after a little time.
_A_FILE_IS_IN_THE_WAY = re.compile(
    r"LNK1104|LNK1168|exit code: 1104|exit code: 1168|Access is denied|os error 5\b|os error 32\b|being used by another process|Text file busy|ETXTBSY"
)


def is_the_line_where_the_tests_start(line: str) -> bool:
    """`cargo test` prints "     Running unittests src\\lib.rs (target\\debug\\deps\\...exe)" when it has built a program and starts it."""
    return _RUNNING.match(line) is not None


def program_started_by(line: str) -> str | None:
    """The program that the line "     Running unittests src\\lib.rs (target\\debug\\deps\\name-hash.exe)" says is started, if it says so."""
    found = _PROGRAM_STARTED.match(line)
    return found.group("program") if found else None


def is_a_file_being_in_the_way(lines: Iterable[str]) -> bool:
    """Whether the output says that a file was held by another program, which is for a moment: trying again is what to do."""
    return any(_A_FILE_IS_IN_THE_WAY.search(line) for line in lines)


def failing_test_in(line: str) -> str | None:
    """The name of the test that a line of the output says has failed, if it says that."""
    found = _FAILED.search(line)
    return found.group(1) if found else None


def failing_tests_in(lines: Iterable[str]) -> list[str]:
    names = (failing_test_in(line) for line in lines)
    return [name for name in dict.fromkeys(name for name in names if name)]


@dataclasses.dataclass(frozen=True)
class Outcome:
    verdict: str
    failing_tests: tuple[str, ...] = ()
    # The line "test result: ...", or what went wrong.
    detail: str = ""

    @property
    def is_noticed(self) -> bool:
        """Whether the tests noticed the mistake. A program that hangs or is killed by its crash did not pass its tests; a program that
        does not build says nothing about them."""
        return self.verdict in (CAUGHT, HUNG, CRASHED)


def what_went_wrong_in(lines: Sequence[str], most_lines: int = 12) -> list[str]:
    """The lines of the output of a build that did not work, from the first error on (the end of the output is the summary of the errors,
    which does not say what they were)."""
    first_error = next((number for number, line in enumerate(lines) if line.startswith("error")), max(0, len(lines) - most_lines))
    return list(lines[first_error : first_error + most_lines])


def classify(lines: Sequence[str], exit_code: int | None, how_it_ended: str) -> Outcome:
    """What a run of `cargo test` means for the mistake that was in the program: `lines` is everything it printed (what it said about
    building too), `exit_code` is what it returned, `how_it_ended` is FINISHED, STOPPED_AT_THE_FIRST_FAILURE or TOOK_TOO_LONG."""
    failing = tuple(failing_tests_in(lines))
    results = [line for line in lines if line.startswith("test result:")]
    if failing:
        # A test that fails is a mistake that is noticed, however the run ended after that.
        return Outcome(CAUGHT, failing, results[0] if results else "(stopped at the first test that failed)")
    if how_it_ended == TOOK_TOO_LONG:
        return Outcome(HUNG, (), "the run took too long and was killed")
    if results:
        if results[0].startswith("test result: ok"):
            return Outcome(NOT_CAUGHT, (), results[0])
        return Outcome(CAUGHT, (), results[0])
    if any(is_the_line_where_the_tests_start(line) for line in lines) and exit_code != 0:
        # (A program that was started and did not finish its tests: a crash, if cargo says so. If it says that the program could not be started
        # at all, that is something else, which says nothing about the tests.)
        why = [line.strip() for line in lines if "didn't exit successfully" in line]
        if why:
            return Outcome(CRASHED, (), why[0])
    return Outcome(DID_NOT_COMPILE, (), "\n".join(what_went_wrong_in(lines)))
