# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""Checks of tools/ci/run_tests_and_report.sh, which turns what a test command did into annotations of the GitHub job (the annotations are how the results of the CI
are read without signing in to GitHub). The commands it runs are made up here and print what cargo's test programs print. Run them with
    py -m unittest discover -s tools/ci
(from the folder of the project; they need `bash`)."""

import pathlib
import shutil
import subprocess
import unittest

THE_FOLDER = pathlib.Path(__file__).resolve().parent
THE_SCRIPT = "run_tests_and_report.sh"
THE_TITLE = "Tests that failed"
THE_TITLE_OF_WHAT_RAN = THE_TITLE + " (what ran)"

A_RESULT_LINE_OF_A_PASSING_PROGRAM = "test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.1s"
A_RESULT_LINE_OF_A_FAILING_PROGRAM = "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.1s"


class WhatTheHelperDid:
    """Its exit status, everything it wrote, and the annotations among that, each as (kind, title, message with its escapes undone)."""

    def __init__(self, status, written):
        self.status = status
        self.written = written
        self.annotations = []
        for line in written.splitlines():
            if line.startswith("::") and "::" in line[2:]:
                head, message = line[2:].split("::", 1)
                kind, _, title = head.partition(" title=")
                self.annotations.append((kind, title, message.replace("%0A", "\n").replace("%25", "%")))

    def kinds(self):
        return [kind for (kind, _, _) in self.annotations]

    def message_of(self, kind, title):
        found = [message for (a_kind, a_title, message) in self.annotations if (a_kind, a_title) == (kind, title)]
        assert len(found) == 1, f"expected one {kind} titled {title!r} among {self.annotations}"
        return found[0]

    def lines_of_kind(self, kind):
        """The lines as they were written, which is how GitHub reads them: an annotation is one line."""
        return [line for line in self.written.splitlines() if line.startswith(f"::{kind} ")]


def the_bash():
    # (A computer without bash is a failure and not a skip: a test that is skipped passes.)
    found = shutil.which("bash")
    if found is None:
        raise AssertionError("these checks need bash, and there is none on the path")
    return found


def run_the_helper(level, *the_command):
    process = subprocess.run(
        [the_bash(), "./" + THE_SCRIPT, level, THE_TITLE, *the_command],
        cwd=THE_FOLDER,
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=120,
    )
    return WhatTheHelperDid(process.returncode, process.stdout)


def a_command_that_prints(*lines, exit_status=0, with_arguments=()):
    """The command for the helper to run: it prints the lines (none of them has an apostrophe), and ends with the exit status. The arguments after the script are
    what `cargo test` would be given."""
    script = "; ".join(f"echo '{line}'" for line in lines) + f"; exit {exit_status}"
    return ["bash", "-c", script, "the_command", *with_arguments]


class WhatRanTests(unittest.TestCase):
    def test_a_passing_command_succeeds_and_what_ran_is_written_as_a_notice_with_the_result_line_shortened(self):
        done = run_the_helper("error", *a_command_that_prints(A_RESULT_LINE_OF_A_PASSING_PROGRAM))
        self.assertEqual(done.status, 0)
        notice = done.message_of("notice", THE_TITLE_OF_WHAT_RAN)
        self.assertIn("test result: ok. 2 passed; 0 failed; 0 ignored; finished in 0.1s", notice)
        self.assertNotIn("0 measured", notice)
        self.assertEqual(done.kinds(), ["notice"])

    def test_the_result_line_of_each_program_of_tests_is_written(self):
        done = run_the_helper(
            "error",
            *a_command_that_prints(
                A_RESULT_LINE_OF_A_PASSING_PROGRAM,
                "test result: ok. 15 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 9.7s",
            ),
        )
        self.assertEqual(done.status, 0)
        notice = done.message_of("notice", THE_TITLE_OF_WHAT_RAN)
        self.assertIn("2 passed", notice)
        self.assertIn("15 passed; 0 failed; 1 ignored", notice)


class SkippedLinesTests(unittest.TestCase):
    """A test that skips itself passes, and the only way to tell it from one that ran is the line that says SKIPPED."""

    def test_the_lines_that_say_skipped_are_counted_when_the_run_keeps_the_output_of_the_tests_that_pass(self):
        done = run_the_helper(
            "warning",
            *a_command_that_prints(
                "SKIPPED: there is no software graphics adapter on this computer",
                "SKIPPED: no sound output to try",
                A_RESULT_LINE_OF_A_PASSING_PROGRAM,
                with_arguments=("--", "engine::", "--nocapture"),
            ),
        )
        self.assertEqual(done.status, 0)
        self.assertIn("lines that say SKIPPED: 2", done.message_of("notice", THE_TITLE_OF_WHAT_RAN))

    def test_no_line_that_says_skipped_is_a_count_of_nought_when_the_run_keeps_the_output_of_the_tests_that_pass(self):
        done = run_the_helper("error", *a_command_that_prints(A_RESULT_LINE_OF_A_PASSING_PROGRAM, with_arguments=("--show-output",)))
        self.assertIn("lines that say SKIPPED: 0", done.message_of("notice", THE_TITLE_OF_WHAT_RAN))

    def test_the_lines_that_say_skipped_are_not_counted_when_the_output_of_the_tests_that_pass_is_not_kept(self):
        # Regression test: the notice said "lines that say SKIPPED: 0" for runs without `--nocapture`, where cargo does not show what a passing test printed, so that a
        # Mac that skipped every test of the picture engine looked like one that skipped nothing. (The command here prints no such line, as cargo would not show it.)
        done = run_the_helper("error", *a_command_that_prints(A_RESULT_LINE_OF_A_PASSING_PROGRAM, with_arguments=("--lib", "--no-fail-fast")))
        notice = done.message_of("notice", THE_TITLE_OF_WHAT_RAN)
        self.assertNotIn("SKIPPED: 0", notice)
        self.assertIn("lines that say SKIPPED: not counted", notice)
        self.assertIn("--nocapture", notice)  # (it says how to have them counted)

    def test_an_argument_that_only_contains_the_word_does_not_make_the_output_count_as_kept(self):
        done = run_the_helper("error", *a_command_that_prints(A_RESULT_LINE_OF_A_PASSING_PROGRAM, with_arguments=("a_filter_with_--nocapture_in_it",)))
        self.assertIn("not counted", done.message_of("notice", THE_TITLE_OF_WHAT_RAN))


class FailureTests(unittest.TestCase):
    A_FAILING_RUN = (
        "test the_one ... FAILED",
        "",
        "---- the_one stdout ----",
        "thread panicked: 100% wrong",
        "second line of what it said",
        A_RESULT_LINE_OF_A_FAILING_PROGRAM,
    )

    def test_a_failing_command_at_the_level_error_fails_and_writes_the_failed_tests_and_what_they_said_as_an_error(self):
        done = run_the_helper("error", *a_command_that_prints(*self.A_FAILING_RUN, exit_status=101))
        self.assertEqual(done.status, 1)
        error = done.message_of("error", THE_TITLE)
        self.assertIn("test the_one ... FAILED", error)
        self.assertIn("thread panicked: 100% wrong", error)
        self.assertIn("second line of what it said", error)

    def test_a_percent_sign_and_the_new_lines_are_escaped_so_that_the_annotation_is_one_line_that_github_reads_whole(self):
        done = run_the_helper("error", *a_command_that_prints(*self.A_FAILING_RUN, exit_status=101))
        [the_line] = done.lines_of_kind("error")
        self.assertIn("thread panicked: 100%25 wrong", the_line)
        self.assertNotIn("100% wrong", the_line)
        self.assertIn("%0A", the_line)
        self.assertIn("second line of what it said", the_line)  # (on the same line as the failed test)

    def test_the_same_failure_at_the_level_warning_is_a_warning_and_the_helper_succeeds(self):
        done = run_the_helper("warning", *a_command_that_prints(*self.A_FAILING_RUN, exit_status=101))
        self.assertEqual(done.status, 0)
        self.assertIn("test the_one ... FAILED", done.message_of("warning", THE_TITLE))
        self.assertNotIn("error", done.kinds())

    def test_what_ran_is_written_also_when_the_command_failed(self):
        done = run_the_helper("error", *a_command_that_prints(*self.A_FAILING_RUN, exit_status=101))
        self.assertIn("test result: FAILED. 0 passed; 1 failed", done.message_of("notice", THE_TITLE_OF_WHAT_RAN))

    def test_a_command_that_fails_without_naming_a_failed_test_still_fails_the_helper(self):
        # (A build that does not compile prints no `test ... FAILED` line.)
        done = run_the_helper("error", *a_command_that_prints("error[E0432]: unresolved import", exit_status=101))
        self.assertEqual(done.status, 1)
        self.assertIn("error", done.kinds())

    def test_the_exit_status_of_the_command_decides_even_when_its_output_goes_through_tee(self):
        # (`set -o pipefail`: without it the status of `tee` would be the one that counts, and a failing command would pass.)
        done = run_the_helper("error", *a_command_that_prints(A_RESULT_LINE_OF_A_PASSING_PROGRAM, exit_status=1))
        self.assertEqual(done.status, 1)


if __name__ == "__main__":
    unittest.main()
