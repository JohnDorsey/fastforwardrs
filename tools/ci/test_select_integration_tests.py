# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""Checks of tools/ci/select_integration_tests.sh, which says which programs of integration tests (tests/*.rs) a part of the CI runs. The point of the checks is that the parts
are *every program between them and none twice* (a program of tests that fell out of every part would never be run, and nobody would see it), and that the script and ci.yml,
which both name the programs that have steps of their own, say the same. Run them with
    py -m unittest discover -s tools/ci
(from the folder of the project; they need `bash`)."""

import pathlib
import re
import shutil
import subprocess
import tempfile
import unittest

THE_FOLDER = pathlib.Path(__file__).resolve().parent
THE_PROJECT = THE_FOLDER.parent.parent
THE_SCRIPT = THE_FOLDER / "select_integration_tests.sh"
THE_WORKFLOW = THE_PROJECT / ".github" / "workflows" / "ci.yml"
THE_PARTS = ("all", "first", "second")


def the_bash():
    # (A computer without bash is a failure and not a skip: a test that is skipped passes.)
    found = shutil.which("bash")
    if found is None:
        raise AssertionError("these checks need bash, and there is none on the path")
    return found


def select(part, folder=None):
    """What the script did for a part: its exit status, what it wrote, what it said about a mistake."""
    command = [the_bash(), THE_SCRIPT.as_posix(), part] + ([pathlib.Path(folder).as_posix()] if folder is not None else [])
    process = subprocess.run(command, cwd=THE_PROJECT, capture_output=True, text=True, encoding="utf-8", timeout=60)
    return process.returncode, process.stdout, process.stderr


def programs_of(part, folder=None):
    status, written, said = select(part, folder)
    assert status == 0, f"the script failed for {part!r}: {said}"
    return re.findall(r"--test (\S+)", written)


def the_names_in_the_script(variable):
    text = THE_SCRIPT.read_text(encoding="utf-8")
    found = re.search(rf'^{variable}="([^"]*)"', text, re.MULTILINE)
    assert found, f"the script does not set {variable}"
    return found.group(1).split()


def a_folder_of_programs(*names):
    folder = tempfile.TemporaryDirectory()
    for name in names:
        (pathlib.Path(folder.name) / f"{name}.rs").write_text("", encoding="utf-8")
    return folder


class TheRealProgramsOfTests(unittest.TestCase):
    def test_the_first_and_the_second_part_are_every_program_between_them_and_none_twice(self):
        every = programs_of("all")
        first = programs_of("first")
        second = programs_of("second")
        self.assertEqual(sorted(first + second), sorted(every))
        self.assertEqual(set(first) & set(second), set())
        self.assertEqual(len(first) + len(second), len(every), "a program in both parts, or one twice in a part")
        self.assertTrue(first and second, "a part with nothing in it")

    def test_every_file_of_tests_is_run_by_the_parts_or_by_a_step_of_its_own(self):
        files = {path.stem for path in (THE_PROJECT / "tests").glob("*.rs")}
        separate = set(the_names_in_the_script("separate"))
        self.assertEqual(set(programs_of("all")) | separate, files)
        self.assertEqual(set(programs_of("all")) & separate, set(), "a program that has steps of its own is also in a part")

    def test_the_programs_that_have_steps_of_their_own_are_the_ones_that_ci_yml_runs_by_name(self):
        # (The two lists are written in two places, and a program that was run by neither would never be noticed.)
        text = THE_WORKFLOW.read_text(encoding="utf-8")
        run_by_name = set()
        for line in text.splitlines():
            if line.strip().startswith("run:") and "bash tools/ci/run_tests_and_report.sh" in line and "--test " in line:
                run_by_name |= set(re.findall(r"--test (\w+)", line))
        self.assertEqual(run_by_name, set(the_names_in_the_script("separate")))

    def test_the_programs_of_the_first_part_exist(self):
        files = {path.stem for path in (THE_PROJECT / "tests").glob("*.rs")}
        self.assertLessEqual(set(the_names_in_the_script("first")), files)
        self.assertEqual(set(the_names_in_the_script("first")), set(programs_of("first")))


class ThePartsAsTheWorkflowUsesThem(unittest.TestCase):
    def test_each_column_of_the_workflow_has_a_job_of_unit_tests_and_runs_all_of_the_integration_tests_in_one_job_or_both_halves_in_jobs_of_their_own(self):
        # A column (an operating system) is the job of the unit tests, and either one job that runs all of the integration tests or two that run a half each. Nothing
        # else, so that no part is left out when a job is edited. Each entry of the matrix is one line:
        #     - { name: "...", os: ..., part: unit|integration, integration_tests: all|first|second }
        text = THE_WORKFLOW.read_text(encoding="utf-8")
        entries = re.findall(r"^\s*- \{([^}]*)\}\s*$", text, re.MULTILINE)
        self.assertTrue(entries, "no entry of the matrix was found")
        by_os = {}
        for entry in entries:
            operating_system = re.search(r"\bos:\s*([\w.-]+)", entry).group(1)
            part = re.search(r"\bpart:\s*(\w+)", entry).group(1)
            tests = re.search(r"\bintegration_tests:\s*(\w+)", entry)
            by_os.setdefault(operating_system, []).append((part, tests.group(1) if tests else None))
            if tests:
                self.assertIn(tests.group(1), THE_PARTS, f"{operating_system} runs a part that the script does not know")
        for operating_system, jobs in by_os.items():
            self.assertIn(
                sorted(jobs, key=str),
                ([("integration", "all"), ("unit", None)], [("integration", "first"), ("integration", "second"), ("unit", None)]),
                f"{operating_system} runs {jobs}: not the unit tests and all of the integration tests, and not the unit tests and both halves of the integration tests",
            )
        self.assertEqual(sorted(by_os), ["macos-latest", "ubuntu-latest", "windows-latest"])


class WhatTheScriptDoesWithOtherFolders(unittest.TestCase):
    NAMED_BY_THE_SCRIPT = ("media_export", "media_export_tracks", "media_export_effects", "midi_in_the_sound")
    SEPARATE = ("application_playback", "application_sound", "media_ffmpeg_quirks", "sound_engine_real_time")

    def test_a_program_that_is_added_later_is_run_by_the_second_part_and_not_by_the_first(self):
        folder = a_folder_of_programs(*self.NAMED_BY_THE_SCRIPT, *self.SEPARATE, "a_program_nobody_listed")
        self.addCleanup(folder.cleanup)
        self.assertIn("a_program_nobody_listed", programs_of("second", folder.name))
        self.assertNotIn("a_program_nobody_listed", programs_of("first", folder.name))
        self.assertIn("a_program_nobody_listed", programs_of("all", folder.name))

    def test_a_name_that_the_script_lists_but_that_is_not_in_the_folder_is_an_error_and_not_a_smaller_part(self):
        folder = a_folder_of_programs(*self.SEPARATE, "media_export", "something_else")  # (media_export_tracks and the others are missing)
        self.addCleanup(folder.cleanup)
        status, written, said = select("first", folder.name)
        self.assertNotEqual(status, 0)
        self.assertEqual(written.strip(), "")
        self.assertIn("there is no", said)
        self.assertIn("media_export_tracks.rs", said)

    def test_a_part_that_the_script_does_not_know_is_an_error(self):
        status, written, said = select("third")
        self.assertNotEqual(status, 0)
        self.assertEqual(written.strip(), "")
        self.assertIn("usage", said)

    def test_the_programs_are_written_in_the_order_of_their_names(self):
        folder = a_folder_of_programs(*self.NAMED_BY_THE_SCRIPT, *self.SEPARATE, "zz_last", "aa_first")
        self.addCleanup(folder.cleanup)
        every = programs_of("all", folder.name)
        self.assertEqual(every, sorted(every))


if __name__ == "__main__":
    unittest.main()
