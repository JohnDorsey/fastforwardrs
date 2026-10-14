"""Plants one deliberate mistake at a time and checks that the tests notice ("mutation testing" by hand-written mistakes).

    py tools/mutation_testing/mutate.py MUTATIONS_FILE [words that the names of the mistakes to run contain] [options]

MUTATIONS_FILE defines MUTATIONS = [(name, path_in_project, old_text, new_text), ...]; a fifth item may say what to give `cargo test`
instead of the library's whole suite ("--test application_journeys", "--lib -- a_test"): then what is run is all that is run.
README.md in this folder says what the runner does to be quick, and what it does so that quick does not mean wrong.

The mistakes are planted in work copies of the project (never in the project itself), each with a target folder of its own, so that
several can be tried at once and the project can go on being changed meanwhile."""

from __future__ import annotations

import argparse
import dataclasses
import json
import os
import pathlib
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
from collections.abc import Callable, Iterable, Sequence

import mutation_core as core
from mutation_core import Mutation, Outcome

PROJECT_ROOT = pathlib.Path(__file__).resolve().parents[2]

# Folders of the project that a work copy does not have: what is built, what is only history, what the screenshot tests draw, and
# these tools (the tests of the project do not look at them).
LEFT_OUT_OF_THE_COPIES = {"target", ".git", "screenshots", "tools"}

# A test program that was built without debugging information is built, and linked, faster (measured: 6.4 s instead of 8.1 s for the
# rebuild after one changed line) and its target folder is a quarter of the size. Nothing a test can see is different: the tests
# are still compiled with the checks of the debug profile (arithmetic overflow, debug assertions).
CARGO_SETTINGS = {"CARGO_TERM_COLOR": "never", "CARGO_PROFILE_DEV_DEBUG": "0"}

# ---- the controls: two mistakes that say whether the runner itself can be trusted ---------------------------------------------------

CONTROL_ANCHOR = "#[cfg(test)]\nmod test_support;\n"
CONTROLS = [
    (
        # Its name sorts before every other test, so that it is the first the program of the tests starts.
        Mutation(
            "control: a test that always fails (must be noticed)",
            "src/lib.rs",
            CONTROL_ANCHOR,
            CONTROL_ANCHOR
            + '\n#[cfg(test)]\nmod aaa_mutation_controls {\n    #[test]\n    fn fails_on_purpose() {\n        panic!("on purpose: this is the check of the runner");\n    }\n}\n',
        ),
        True,
    ),
    (
        Mutation(
            "control: a test that always passes (must not be noticed; this is also the check that the project's own tests pass)",
            "src/lib.rs",
            CONTROL_ANCHOR,
            CONTROL_ANCHOR
            + "\n#[cfg(test)]\nmod aaa_mutation_controls {\n    #[test]\n    fn passes() {\n        assert_eq!(1 + 1, 2);\n    }\n}\n",
        ),
        False,
    ),
]

# ---- processes ----------------------------------------------------------------------------------------------------------------------


def kill_the_tree_of(process: subprocess.Popen) -> None:
    """Ends a process and everything it started (cargo, the program of the tests, and what the tests ran), and nothing else: it never
    kills by the name of a program, because the user's own cargo may be running."""
    if os.name == "nt":
        subprocess.run(["taskkill", "/F", "/T", "/PID", str(process.pid)], capture_output=True)
    else:
        try:
            os.killpg(os.getpgid(process.pid), signal.SIGKILL)
        except ProcessLookupError:
            pass


@dataclasses.dataclass
class Run:
    """One run of `cargo test`."""

    lines: list[str]
    exit_code: int | None
    how_it_ended: str
    seconds_building: float
    seconds_testing: float


def environment_for_cargo() -> dict[str, str]:
    environment = {**os.environ, **CARGO_SETTINGS}
    if shutil.which("cargo", path=environment.get("PATH")) is None:
        cargo_folder = pathlib.Path.home() / ".cargo" / "bin"
        if cargo_folder.exists():
            environment["PATH"] = f"{cargo_folder}{os.pathsep}{environment.get('PATH', '')}"
    return environment


def wait_until_a_program_can_be_written_over(path: pathlib.Path, seconds: float = 20) -> bool:
    """Waits until the file `path` can be opened for writing, or is not there: a program that is running cannot be (it is in the way of the
    linker that makes the next one). Returns whether it can."""
    deadline = time.monotonic() + seconds
    while path.exists():
        try:
            with open(path, "r+b"):
                return True
        except OSError:
            if time.monotonic() > deadline:
                return False
            time.sleep(0.05)
    return True


class WorkCopy:
    """A copy of the project's files with a target folder of its own, in which mistakes are planted and `cargo test` is run."""

    def __init__(self, folder: pathlib.Path):
        self.folder = folder
        self.environment = environment_for_cargo()
        self._process: subprocess.Popen | None = None
        self.was_told_to_stop = False

    def synchronise_with(self, project: pathlib.Path) -> int:
        """Makes the files of the copy those of the project, and returns how many were written. A file is written only if what is in
        it differs, and then it has the time of now, so that cargo builds again what is built from it (a file that is put back with an
        old time would not be built again, and the program of the tests would still have the mistake that was in it)."""
        written = 0
        wanted: set[pathlib.Path] = set()
        for folder, subfolders, files in os.walk(project):
            relative_folder = pathlib.Path(folder).relative_to(project)
            if relative_folder == pathlib.Path("."):
                subfolders[:] = [name for name in subfolders if name not in LEFT_OUT_OF_THE_COPIES]
            subfolders[:] = [name for name in subfolders if name != "__pycache__"]
            (self.folder / relative_folder).mkdir(parents=True, exist_ok=True)
            for name in files:
                relative = relative_folder / name
                wanted.add(relative)
                content = (project / relative).read_bytes()
                copy = self.folder / relative
                if not copy.exists() or copy.read_bytes() != content:
                    copy.write_bytes(content)
                    written += 1
        for folder, subfolders, files in os.walk(self.folder):
            relative_folder = pathlib.Path(folder).relative_to(self.folder)
            if relative_folder == pathlib.Path("."):
                subfolders[:] = [name for name in subfolders if name not in LEFT_OUT_OF_THE_COPIES]
            for name in files:
                if relative_folder / name not in wanted:
                    (self.folder / relative_folder / name).unlink()
        return written

    def read(self, path: str) -> str:
        with open(self.folder / path, encoding="utf-8", newline="") as file:
            return file.read()

    def write(self, path: str, text: str) -> None:
        with open(self.folder / path, "w", encoding="utf-8", newline="") as file:
            file.write(text)

    def stop(self) -> None:
        self.was_told_to_stop = True
        if self._process is not None:
            kill_the_tree_of(self._process)

    def run_cargo_test(
        self,
        cargo_arguments: Sequence[str],
        *,
        stop_at_the_first_failure: bool,
        seconds_allowed_for_building: float,
        seconds_allowed_for_the_tests: float,
        low_priority: bool = False,
    ) -> Run:
        """Runs `cargo test <arguments>`. The time allowed for the tests counts from when their program starts. With
        stop_at_the_first_failure everything is killed as soon as a test is said to have failed: the answer is known then."""
        process = subprocess.Popen(
            ["cargo", "test", *cargo_arguments],
            cwd=self.folder,
            env=self.environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",  # (what a mistake that fails many tests prints can have characters that a code page does not)
            start_new_session=os.name != "nt",
            # (What cargo starts, the compiler and the tests, has the same priority.)
            creationflags=getattr(subprocess, "BELOW_NORMAL_PRIORITY_CLASS", 0) if low_priority else 0,
        )
        self._process = process
        started = time.monotonic()
        tests_started: list[float] = []  # the time the program of the tests was started, when it was
        program: list[str] = []  # (the program of the tests that cargo started: it is where cargo writes the next one)
        how_it_ended = core.FINISHED
        lines: list[str] = []

        def watch_the_time() -> None:
            nonlocal how_it_ended
            while process.poll() is None:
                now = time.monotonic()
                limit = tests_started[0] + seconds_allowed_for_the_tests if tests_started else started + seconds_allowed_for_building
                if now > limit:
                    how_it_ended = core.TOOK_TOO_LONG
                    kill_the_tree_of(process)
                    return
                time.sleep(0.25)

        watcher = threading.Thread(target=watch_the_time, daemon=True)
        watcher.start()
        assert process.stdout is not None
        for line in process.stdout:
            line = line.rstrip("\r\n")
            lines.append(line)
            if not tests_started and core.is_the_line_where_the_tests_start(line):
                tests_started.append(time.monotonic())
            if started_program := core.program_started_by(line):
                program[:] = [started_program]
            if stop_at_the_first_failure and core.failing_test_in(line):
                how_it_ended = core.STOPPED_AT_THE_FIRST_FAILURE
                kill_the_tree_of(process)
                break
        process.stdout.close()
        exit_code = process.wait()
        watcher.join()
        self._process = None
        if program:
            # A program that was killed is gone a moment later, not at once, and cargo cannot write the next one over it until it is.
            wait_until_a_program_can_be_written_over(self.folder / program[0])
        ended = time.monotonic()
        building = (tests_started[0] if tests_started else ended) - started
        return Run(lines, exit_code, how_it_ended, building, ended - tests_started[0] if tests_started else 0.0)


# ---- one mistake --------------------------------------------------------------------------------------------------------------------


# How many times a run is made when it fails because a file that has to be written or run is held by another program.
ATTEMPTS_WHEN_A_FILE_IS_IN_THE_WAY = 3


# A test that fails, and passes when it is run again by itself, did not notice the mistake: it fails when the computer is busy (a test of
# the speakers, say, or of the time), and a campaign makes the computer busy. Such tests are left out of the run, which is made again
# without them, as many times as it takes, up to this many.
MOST_TESTS_THAT_FAIL_ONCE_PER_STAGE = 8


@dataclasses.dataclass
class Options:
    in_stages: bool = True
    stop_at_the_first_failure: bool = True
    seconds_allowed_for_building: float = 900
    seconds_allowed_for_the_tests: float = 240
    low_priority: bool = False
    # Whether a test that fails has to fail again, run by itself, for the mistake to count as noticed.
    confirm_failures: bool = True


@dataclasses.dataclass
class Trial:
    """What came of trying one mistake."""

    mutation: Mutation
    outcome: Outcome | None  # None: the text to change was not in the file exactly once
    deciding_stage: str = ""
    seconds_building: float = 0.0
    seconds_testing: float = 0.0
    problem: str = ""
    # The tests that failed with the mistake and passed when they were run again by themselves: they were left out, and are not what noticed it.
    dismissed_failures: list[str] = dataclasses.field(default_factory=list)


def run_the_tests(
    copy: WorkCopy, arguments: Sequence[str], options: Options, trial: Trial, *, stop_at_the_first_failure: bool
) -> Outcome:
    """Runs `cargo test <arguments>` in the copy, again if a file that has to be written was held by another program, and counts the time."""
    for attempt in range(ATTEMPTS_WHEN_A_FILE_IS_IN_THE_WAY):
        run = copy.run_cargo_test(
            arguments,
            stop_at_the_first_failure=stop_at_the_first_failure,
            seconds_allowed_for_building=options.seconds_allowed_for_building,
            seconds_allowed_for_the_tests=options.seconds_allowed_for_the_tests,
            low_priority=options.low_priority,
        )
        trial.seconds_building += run.seconds_building
        trial.seconds_testing += run.seconds_testing
        outcome = core.classify(run.lines, run.exit_code, run.how_it_ended)
        if outcome.verdict != core.DID_NOT_COMPILE or not core.is_a_file_being_in_the_way(run.lines):
            break
        time.sleep(1 + attempt)  # (a file that another program holds for a moment is held no longer, then)
    return outcome


def tests_that_fail_again_by_themselves(
    copy: WorkCopy, mutation: Mutation, tests: Sequence[str], options: Options, trial: Trial
) -> list[str]:
    """Which of the tests that failed fail again when they are run alone (with the mistake still in the program, which is built already).
    Those that do not were failing for another reason than the mistake. (A run that says nothing, because it did not build, does not
    count as failing again: not being sure is not noticing.)"""
    outcome = run_the_tests(
        copy, core.arguments_to_run_exactly(mutation.cargo_arguments, tests), options, trial, stop_at_the_first_failure=False
    )
    if outcome.verdict == core.CAUGHT:
        return list(outcome.failing_tests) or list(tests)
    if outcome.verdict in (core.HUNG, core.CRASHED):
        return list(tests)
    return []


def try_mutation(copy: WorkCopy, mutation: Mutation, options: Options, learned_tests: Sequence[str] = ()) -> Trial:
    """Plants the mistake in the copy, runs the tests that decide, and puts the file back as it was."""
    original = copy.read(mutation.path)
    planted = core.plant(original, mutation)
    if planted is None:
        count = core.how_many_times_the_text_to_change_is_in(original, mutation)
        return Trial(mutation, None, problem=f"the text to change is in {mutation.path} {count} times, not once")
    trial = Trial(mutation, None)
    try:
        copy.write(mutation.path, planted)
        details = []
        for stage in core.stages_for(mutation, learned_tests, in_stages=options.in_stages):
            if copy.was_told_to_stop:
                trial.problem = "stopped"
                return trial
            left_out: list[str] = []  # (the tests of this stage that failed once, and passed when they were run again by themselves)
            while True:
                outcome = run_the_tests(
                    copy,
                    core.arguments_leaving_out(stage.cargo_arguments, left_out),
                    options,
                    trial,
                    stop_at_the_first_failure=options.stop_at_the_first_failure,
                )
                trial.deciding_stage = stage.name
                if outcome.verdict != core.CAUGHT or not outcome.failing_tests or not options.confirm_failures:
                    break
                failing_again = tests_that_fail_again_by_themselves(copy, mutation, outcome.failing_tests, options, trial)
                if failing_again:
                    outcome = dataclasses.replace(outcome, failing_tests=tuple(failing_again))
                    break
                # They failed once, and no more: they say nothing about the mistake. The run is made again without them, for the rest of the tests.
                left_out += [name for name in outcome.failing_tests if name not in left_out]
                trial.dismissed_failures += [name for name in outcome.failing_tests if name not in trial.dismissed_failures]
                if len(left_out) > MOST_TESTS_THAT_FAIL_ONCE_PER_STAGE:
                    outcome = Outcome(core.NOT_CAUGHT, (), f"{len(left_out)} tests failed once and passed again: the computer is too busy to say")
                    break
            if outcome.verdict == core.NOT_CAUGHT:
                # The tests of this stage pass: the next stage has the rest of the tests, to find it.
                details.append(outcome.detail)
                trial.outcome = dataclasses.replace(outcome, detail="; ".join(details))
                continue
            trial.outcome = outcome
            break
    finally:
        copy.write(mutation.path, original)
    return trial


# ---- a campaign ---------------------------------------------------------------------------------------------------------------------

LEARNED_TESTS_KEPT_PER_FILE = 6


class Campaign:
    """Tries mistakes, several at a time in work copies, and says what came of each as it is known."""

    def __init__(
        self,
        project: pathlib.Path,
        work_folder: pathlib.Path,
        workers: int,
        options: Options,
        say: Callable[[str], None] = print,
    ):
        self.project = project
        self.work_folder = work_folder
        self.options = options
        self.say = say
        self.copies = [WorkCopy(work_folder / f"worker{number}") for number in range(workers)]
        self._lock = threading.Lock()
        self._learned_file = work_folder / "learned_tests.json"
        # The tests that noticed mistakes in each file: they are run first for the next mistakes in that file.
        self.learned_tests: dict[str, list[str]] = {}
        if self._learned_file.exists():
            try:
                self.learned_tests = json.loads(self._learned_file.read_text(encoding="utf-8"))
            except (OSError, ValueError):
                self.learned_tests = {}
        self.stop_asked = threading.Event()

    def prepare_the_copies(self) -> None:
        for copy in self.copies:
            written = copy.synchronise_with(self.project)
            self.say(f"{copy.folder.name}: {written} file(s) written to make it the project")

    def stop(self) -> None:
        self.stop_asked.set()
        for copy in self.copies:
            copy.stop()

    def _learn_from(self, trial: Trial) -> None:
        outcome = trial.outcome
        aimed = trial.mutation.cargo_arguments is not None
        if outcome is None or outcome.verdict != core.CAUGHT or not outcome.failing_tests or aimed:
            return
        if core.module_path_of(trial.mutation.path) is None:
            return  # (the controls, for one, are in the root of the library)
        with self._lock:
            known = self.learned_tests.setdefault(trial.mutation.path, [])
            for name in reversed(outcome.failing_tests[:2]):
                if name in known:
                    known.remove(name)
                known.insert(0, name)
            del known[LEARNED_TESTS_KEPT_PER_FILE:]

    def _learned_for(self, path: str) -> list[str]:
        with self._lock:
            return list(self.learned_tests.get(path, []))

    def run(self, mutations: Sequence[Mutation], report: Callable[[int, Trial], None]) -> list[Trial | None]:
        """Tries every mutation (with as many in step as there are work copies) and calls `report(index, trial)` for each as soon as
        it is known. Returns the trials in the order of the mutations (None for one that was not tried because of a stop)."""
        trials: list[Trial | None] = [None] * len(mutations)
        queue = list(enumerate(mutations))
        queue_lock = threading.Lock()

        def work(copy: WorkCopy) -> None:
            while not self.stop_asked.is_set():
                with queue_lock:
                    if not queue:
                        return
                    index, mutation = queue.pop(0)
                trial = try_mutation(copy, mutation, self.options, self._learned_for(mutation.path))
                if trial.problem == "stopped":
                    return
                trials[index] = trial
                self._learn_from(trial)
                with self._lock:
                    report(index, trial)

        threads = [threading.Thread(target=work, args=(copy,), daemon=True) for copy in self.copies]
        for thread in threads:
            thread.start()
        try:
            while any(thread.is_alive() for thread in threads):
                for thread in threads:
                    thread.join(timeout=0.5)
        except KeyboardInterrupt:
            self.say("interrupted: stopping what is running")
            self.stop()
            for thread in threads:
                thread.join(timeout=30)
        self._save_what_was_learned()
        return trials

    def _save_what_was_learned(self) -> None:
        try:
            self.work_folder.mkdir(parents=True, exist_ok=True)
            self._learned_file.write_text(json.dumps(self.learned_tests, indent=1), encoding="utf-8")
        except OSError:
            pass

    def check_the_controls(self) -> bool:
        """Plants the two mistakes that say whether the runner works: one that must be noticed and one that must not. Returns whether
        the runner did what it should (and says so if not). The second is run on all the tests, so it is also the check that they
        pass when nothing is wrong, which is what a verdict of "noticed" rests on."""
        reports: list[str] = []
        good = True

        def report(index: int, trial: Trial) -> None:
            nonlocal good
            must_be_noticed = CONTROLS[index][1]
            if trial.outcome is None:
                reports.append(f"    the control could not be planted: {trial.problem} (update CONTROL_ANCHOR in mutate.py)")
                good = False
            elif trial.outcome.is_noticed != must_be_noticed:
                reports.append(
                    f"    {trial.mutation.name}: the verdict was {trial.outcome.verdict}: {trial.outcome.detail[:300]}"
                )
                good = False
            elif must_be_noticed and "aaa_mutation_controls::fails_on_purpose" not in trial.outcome.failing_tests:
                reports.append(f"    {trial.mutation.name}: noticed, but not by the test that was made to fail: {trial.outcome}")
                good = False

        trials = self.run([control for control, _ in CONTROLS], report)
        if any(trial is None for trial in trials):
            reports.append("    a control was not run")
            good = False
        if not good:
            self.say("THE CONTROLS FAILED, so the runner cannot be trusted:")
            for text in reports:
                self.say(text)
            return False
        fails, passes = (trial.outcome for trial in trials if trial and trial.outcome)
        self.say(
            f"the controls behave: a test that always fails was noticed (by {fails.failing_tests[0]}), and one that always passes was "
            f"not, with all the tests run ({passes.detail.split(';')[0].removeprefix('test result: ')})"
        )
        return True

    def check_that_what_noticed_passes_without_the_mistakes(self, trials: Iterable[Trial | None]) -> bool:
        """A mistake is "noticed" when a test fails with it. That says nothing if the test fails without it too (a test that is
        broken, or too dependent on the time, or on the load of the computer). So the tests that were seen to fail are run on the
        project as it is, and must pass."""
        wanted: dict[tuple[str, ...], set[str]] = {}
        for trial in trials:
            if trial and trial.outcome and trial.outcome.verdict == core.CAUGHT:
                for name in trial.outcome.failing_tests:
                    wanted.setdefault(core.arguments_to_run_exactly(trial.mutation.cargo_arguments, []), set()).add(name)
        if not wanted:
            return True
        copy = self.copies[0]
        good = True
        for prefix, names in wanted.items():
            ordered = sorted(names)
            run = copy.run_cargo_test(
                [*prefix, *ordered],
                stop_at_the_first_failure=False,
                seconds_allowed_for_building=self.options.seconds_allowed_for_building,
                seconds_allowed_for_the_tests=self.options.seconds_allowed_for_the_tests,
                low_priority=self.options.low_priority,
            )
            outcome = core.classify(run.lines, run.exit_code, run.how_it_ended)
            if outcome.verdict != core.NOT_CAUGHT:
                good = False
                self.say(
                    f"UNRELIABLE: these tests failed without any mistake planted, so what they caught was not caught by them "
                    f"({' '.join(prefix)}): {', '.join(outcome.failing_tests) or outcome.detail}"
                )
            elif f" {len(ordered)} passed;" not in outcome.detail:
                good = False
                self.say(f"UNRELIABLE: {len(ordered)} tests were asked for but the result says: {outcome.detail}")
            else:
                self.say(f"the {len(ordered)} test(s) that noticed mistakes pass without them ({' '.join(prefix)}): {outcome.detail}")
        return good


# ---- the command line ---------------------------------------------------------------------------------------------------------------


def describe(index: int, count: int, trial: Trial) -> str:
    width = len(str(count))
    head = f"[{index + 1:{width}}/{count}] "
    mutation, outcome = trial.mutation, trial.outcome
    if outcome is None:
        return f"{head}SKIPPED ({trial.problem}): {mutation.name}"
    timing = f"(build {trial.seconds_building:.1f} s, tests {trial.seconds_testing:.1f} s; decided by: {trial.deciding_stage})"
    if outcome.verdict == core.DID_NOT_COMPILE:
        return f"{head}DID NOT COMPILE (so it says nothing): {mutation.name}\n    {outcome.detail[-500:]}"
    left_out = (
        f"    (left out, as they failed once and passed when they were run by themselves: {', '.join(trial.dismissed_failures)})"
        if trial.dismissed_failures
        else ""
    )
    if outcome.verdict == core.NOT_CAUGHT:
        return f"{head}NOT CAUGHT: {mutation.name}\n    {outcome.detail} {timing}" + (f"\n{left_out}" if left_out else "")
    lines = [f"{head}caught: {mutation.name}"]
    if outcome.verdict == core.HUNG:
        lines.append(f"    HUNG: {outcome.detail} {timing}")
    elif outcome.verdict == core.CRASHED:
        lines.append(f"    CRASHED: {outcome.detail} {timing}")
    else:
        lines.append(f"    {outcome.detail} {timing}")
    for name in outcome.failing_tests[:3]:
        lines.append(f"      test {name} ... FAILED")
    if len(outcome.failing_tests) > 3:
        lines.append(f"      ... and {len(outcome.failing_tests) - 3} more")
    if left_out:
        lines.append(left_out)
    return "\n".join(lines)


def results_as_json(index: int, trial: Trial) -> dict:
    outcome = trial.outcome
    return {
        "index": index,
        "name": trial.mutation.name,
        "path": trial.mutation.path,
        "verdict": outcome.verdict if outcome else "SKIPPED",
        "failing_tests": list(outcome.failing_tests) if outcome else [],
        "detail": outcome.detail if outcome else trial.problem,
        "stage": trial.deciding_stage,
        "seconds_building": round(trial.seconds_building, 2),
        "seconds_testing": round(trial.seconds_testing, 2),
        "dismissed_failures": list(trial.dismissed_failures),
    }


def arguments_of_the_command_line(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("mutations_file", type=pathlib.Path)
    parser.add_argument("words", nargs="*", help="only the mistakes whose names contain one of these")
    parser.add_argument("--workers", type=int, default=3, help="how many mistakes to try at once, each in a work copy (default 3)")
    parser.add_argument(
        "--thorough",
        action="store_true",
        help="run all the tests for every mistake, however soon one fails (slow: how it was done before), and say how many failed",
    )
    parser.add_argument("--project", type=pathlib.Path, default=PROJECT_ROOT, help="the project to take the files from")
    parser.add_argument("--work-folder", type=pathlib.Path, help="where the work copies are (default: in the temporary files)")
    parser.add_argument("--results", type=pathlib.Path, help="write one line of JSON for each mistake tried to this file")
    parser.add_argument("--only-not-noticed-in", type=pathlib.Path, metavar="RESULTS", help="only the mistakes that RESULTS says were not noticed")
    parser.add_argument("--check", action="store_true", help="only check that each text to change is in the project exactly once")
    parser.add_argument("--list", action="store_true", help="only list the mistakes that would be tried")
    parser.add_argument("--no-controls", action="store_true", help="do not first plant the two mistakes that check the runner itself")
    parser.add_argument("--no-final-check", action="store_true", help="do not check at the end that the tests that noticed pass without the mistakes")
    parser.add_argument("--seconds-for-the-tests", type=float, default=240, help="a run of the tests that takes longer than this is killed and counts as noticed")
    parser.add_argument(
        "--low-priority", action="store_true", help="run cargo and the tests at a lower priority, so that the computer can be used meanwhile"
    )
    parser.add_argument(
        "--trust-every-failure",
        action="store_true",
        help="count a test that fails as noticing the mistake at once, without running it again by itself (a little faster; a test that fails "
        "because the computer is busy would count)",
    )
    parser.add_argument("--clean", action="store_true", help="delete the work copies (they are big) and stop")
    return parser.parse_args(argv)


def default_work_folder() -> pathlib.Path:
    return pathlib.Path(os.environ.get("MUTATION_WORK_FOLDER", pathlib.Path(tempfile.gettempdir()) / "fastforwardrs-mutation-workers"))


def main(argv: Sequence[str] | None = None) -> int:
    arguments = arguments_of_the_command_line(argv)
    work_folder = arguments.work_folder or default_work_folder()
    if arguments.clean:
        shutil.rmtree(work_folder, ignore_errors=True)
        print(f"deleted {work_folder}")
        return 0

    mutations = core.load_mutations(arguments.mutations_file.resolve())
    if arguments.words:
        mutations = [m for m in mutations if any(word in m.name for word in arguments.words)]
    if arguments.only_not_noticed_in:
        noticed = set()
        for line in arguments.only_not_noticed_in.read_text(encoding="utf-8").splitlines():
            if line.strip():
                entry = json.loads(line)
                if entry["verdict"] in (core.CAUGHT, core.HUNG, core.CRASHED):
                    noticed.add(entry["name"])
        mutations = [m for m in mutations if m.name not in noticed]
    if arguments.list:
        for mutation in mutations:
            print(f"{mutation.name}  [{mutation.path}]")
        return 0

    anchors_wrong = 0
    for mutation in mutations:
        with open(arguments.project / mutation.path, encoding="utf-8", newline="") as file:
            count = core.how_many_times_the_text_to_change_is_in(file.read(), mutation)
        if count != 1:
            anchors_wrong += 1
            print(f"{count} times in {mutation.path}, not once: {mutation.name}\n    {mutation.old[:100]!r}")
    print(f"{len(mutations)} mistake(s); {anchors_wrong} with a text to change that is not in the project exactly once")
    if arguments.check or not mutations:
        return 1 if anchors_wrong else 0

    options = Options(
        in_stages=not arguments.thorough,
        stop_at_the_first_failure=not arguments.thorough,
        seconds_allowed_for_the_tests=arguments.seconds_for_the_tests,
        low_priority=arguments.low_priority,
        confirm_failures=not arguments.trust_every_failure,
    )
    campaign = Campaign(arguments.project, work_folder, max(1, arguments.workers), options)
    started = time.monotonic()
    campaign.prepare_the_copies()
    if not arguments.no_controls and not campaign.check_the_controls():
        return 2

    results_file = arguments.results.open("w", encoding="utf-8") if arguments.results else None

    def report(index: int, trial: Trial) -> None:
        print(describe(index, len(mutations), trial), flush=True)
        if results_file:
            results_file.write(json.dumps(results_as_json(index, trial)) + "\n")
            results_file.flush()

    trials = campaign.run(mutations, report)
    if results_file:
        results_file.close()

    good = True
    if not arguments.no_final_check and not arguments.thorough and not campaign.stop_asked.is_set():
        good = campaign.check_that_what_noticed_passes_without_the_mistakes(trials)

    tried = [trial for trial in trials if trial is not None]
    noticed = [t for t in tried if t.outcome and t.outcome.is_noticed]
    not_noticed = [t for t in tried if t.outcome and t.outcome.verdict == core.NOT_CAUGHT]
    other_problems = [t for t in tried if not t.outcome or t.outcome.verdict == core.DID_NOT_COMPILE]
    print(
        f"\n{len(noticed)} noticed, {len(not_noticed)} NOT CAUGHT, {len(other_problems)} that did not compile or could not be planted, "
        f"{len(mutations) - len(tried)} not tried; {time.monotonic() - started:.0f} s in all"
    )
    for trial in not_noticed + other_problems:
        print(f"  {trial.outcome.verdict if trial.outcome else 'SKIPPED'}: {trial.mutation.name}")
    return 0 if good and not not_noticed and not other_problems and len(tried) == len(mutations) else 1


if __name__ == "__main__":
    sys.exit(main())
