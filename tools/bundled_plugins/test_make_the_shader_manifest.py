# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later
"""Tests of the tool that makes the manifest of shaders: what it decides of a shader's licence, what it takes from the corpus, and the manifest and the record it writes.

Run with: py -I -m unittest discover -s tools/bundled_plugins
"""

import argparse
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import make_the_shader_manifest as tool  # noqa: E402


def shader(header, body="void main() { gl_FragColor = vec4(1.0); }\n"):
    return "/*" + json.dumps(header) + "*/\n" + body


class HeaderAndShape(unittest.TestCase):
    def test_the_header_is_the_first_comment_that_begins_with_a_brace(self):
        text = "/* a licence */\n/*  {\"CREDIT\": \"x\"} */\nvoid main() {}"
        self.assertEqual(tool.header_of(text), {"CREDIT": "x"})

    def test_a_file_with_no_header_or_a_header_that_is_not_json_has_none(self):
        self.assertIsNone(tool.header_of("void main() {}"))
        self.assertIsNone(tool.header_of("/*{ not json }*/"))
        self.assertIsNone(tool.header_of("/*{ \"A\": 1 "))

    def test_the_shape_is_told_by_the_image_inputs(self):
        image = lambda name: {"NAME": name, "TYPE": "image"}
        self.assertEqual(tool.shape_of({"INPUTS": [image("startImage"), image("endImage")]}), "transition")
        self.assertEqual(tool.shape_of({"INPUTS": [image("inputImage")]}), "filter")
        self.assertEqual(tool.shape_of({"INPUTS": [{"NAME": "x", "TYPE": "float"}]}), "generator")
        self.assertEqual(tool.shape_of({}), "generator")
        self.assertEqual(tool.shape_of({"INPUTS": [image("startImage")]}), "generator", "one of the two is no transition")


class Classification(unittest.TestCase):
    def test_a_shader_whose_text_says_the_mit_licence_is_in_with_that_licence(self):
        include, licence, why = tool.classify("a.fs", shader({"CREDIT": "gre"}, "// License: MIT\nvoid main() {}\n"))
        self.assertEqual((include, licence), (True, "MIT"))
        self.assertIn("MIT", why)

    def test_the_licences_that_a_text_says_are_named_by_their_spdx_names(self):
        self.assertEqual(tool.classify("a.fs", shader({}, "// license: BSD 3 Clause\n"))[:2], (True, "BSD-3-Clause"))
        self.assertEqual(tool.classify("a.fs", shader({}, "// license: BSD 2 Clause\n"))[:2], (True, "BSD-2-Clause"))
        self.assertEqual(tool.classify("a.fs", shader({}, "// The MIT License\n"))[:2], (True, "MIT"))

    def test_a_text_that_says_creative_commons_or_non_commercial_is_out_whatever_else_it_says(self):
        for line in ("// License Creative Commons Attribution-NonCommercial-ShareAlike 3.0", "// NonCommercial use only", "// cc by 4.0"):
            include, licence, why = tool.classify("a.fs", shader({"CREDIT": "VIDVOX"}, line + "\n// License: MIT\n"))
            self.assertEqual((include, licence), (False, None), line)
            self.assertIn("does not allow", why)

    def test_a_text_that_names_two_licences_is_left_for_a_person_to_look_at(self):
        include, _, why = tool.classify("a.fs", shader({}, "// License: MIT\n// license: BSD 3 Clause\n"))
        self.assertFalse(include)
        self.assertIn("more than one licence", why)

    def test_a_shader_with_no_licence_of_its_own_by_the_people_of_the_repository_has_the_licence_of_the_repository(self):
        for credit in ("by VIDVOX", "VIDVOX", "by zoidberg", "", "icalvin102 (calvin@icalvin.de)"):
            include, licence, _ = tool.classify("a.fs", shader({"CREDIT": credit}))
            self.assertEqual((include, licence), (True, "MIT"), repr(credit))

    def test_a_shader_that_names_a_source_of_code_and_says_no_licence_is_out(self):
        for line in (
            "// ported from https://www.shadertoy.com/view/abc",
            "// adapted from a blog",
            "// Value Noise code by Inigo Quilez",
            "// via http://stackoverflow.com/questions/1",
            "// original implementation as a plugin",
        ):
            include, _, why = tool.classify("a.fs", shader({"CREDIT": "VIDVOX"}, line + "\n"))
            self.assertFalse(include, line)
            self.assertIn("names a source of code", why)
            self.assertIn(line.strip("/ ").split()[0], why.split(tool.REASON_AND_DETAIL, 1)[1], "the line that shows it is said after the reason")
        include, _, _ = tool.classify("a.fs", shader({"CREDIT": "by VIDVOX (Ported from https://www.shadertoy.com/view/x)"}))
        self.assertFalse(include, "the credit is looked at too")

    def test_a_source_that_is_named_does_not_matter_where_the_text_says_a_licence_that_is_allowed(self):
        include, licence, _ = tool.classify("a.fs", shader({"CREDIT": "gre"}, "// ported by gre from https://gist.github.com/x\n// License: MIT\n"))
        self.assertEqual((include, licence), (True, "MIT"))

    def test_a_shader_that_brings_pictures_is_out_and_so_is_one_by_a_person_whose_shaders_are_non_commercial(self):
        include, _, why = tool.classify("a.fs", shader({"CREDIT": "VIDVOX", "IMPORTED": {"a": {"PATH": "a.png"}}}))
        self.assertFalse(include)
        self.assertIn("pictures", why)
        include, _, why = tool.classify("a.fs", shader({"CREDIT": "by mojovideotech"}))
        self.assertFalse(include)
        self.assertIn("non-commercial", why)

    def test_a_shader_converted_from_gl_transitions_needs_the_licence_line_that_it_kept(self):
        header = {"CREDIT": "Automatically converted from https://www.github.com/gl-transitions/gl-transitions/tree/master/x.glsl"}
        self.assertFalse(tool.classify("a.fs", shader(header))[0])
        self.assertEqual(tool.classify("a.fs", shader(header, "// License: MIT\n"))[:2], (True, "MIT"))

    def test_the_two_that_a_person_looked_at_and_let_in_are_in_with_their_reason(self):
        for name in tool.BY_HAND_IN:
            include, licence, why = tool.classify(name, shader({"CREDIT": "VIDVOX"}, "// https://example.com/documentation\n"))
            self.assertEqual((include, licence, why), (True, "MIT", tool.BY_HAND_IN[name]), name)

    def test_a_file_with_no_header_that_can_be_read_is_out(self):
        self.assertEqual(tool.classify("a.fs", "void main() {}")[:2], (False, None))


class Counting(unittest.TestCase):
    def test_a_reason_is_counted_without_the_line_that_shows_it_and_a_message_of_the_corpus_without_its_numbers_and_names(self):
        self.assertEqual(
            tool.category_of("not bundled: names a source of code that says no licence" + tool.REASON_AND_DETAIL + "// adapted from https://x"),
            "names a source of code that says no licence",
        )
        self.assertEqual(
            tool.category_of("may be bundled, and this build does not run it: structure: this shader reads a picture called lookupImage, which this version cannot give it yet"),
            "structure: this shader reads a picture called X, which this version cannot give it yet",
        )
        self.assertEqual(
            tool.category_of("may be bundled, and this build does not run it: compile: line 12, column 3: Unknown variable: audio"),
            "compile: Unknown variable: audio",
        )
        self.assertEqual(tool.category_of("may be bundled, and this build does not run it: structure: the shader reads 9 pictures, and 7 are the most"), "structure: the shader reads N pictures, and N are the most")


class CorpusAndManifest(unittest.TestCase):
    def test_the_lines_of_the_corpus_that_say_ok_are_kept_with_their_shape_and_the_others_with_their_reason(self):
        ok, failed = tool.read_corpus(
            "running 1 test\n"
            "ok   Checkerboard.fs | Generator, 1 passes, mean rgba (3, 3, 3, 255)\n"
            "ok   Blur Two.fs | Filter, 2 passes, mean rgba (1, 2, 3, 255)\n"
            "ok   Wipe.fs | Transition, 1 passes, installed (not drawn: a transition)\n"
            "FAIL Needs Audio.fs | compile: Unknown variable: audio\n"
            "280 shaders, 1 failed\n"
        )
        self.assertEqual(ok, {"Checkerboard.fs": "generator", "Blur Two.fs": "filter", "Wipe.fs": "transition"})
        self.assertEqual(failed, {"Needs Audio.fs": "compile: Unknown variable: audio"})

    def test_the_manifest_names_each_file_at_the_commit_with_its_checksum_and_the_licences_of_the_set_are_those_of_its_files(self):
        files = [
            {"file": "A Shader.fs", "name": "A Shader", "sha256": "a" * 64, "size": 10, "licence": "MIT", "shape": "filter"},
            {"file": "Curl.fs", "name": "Curl", "sha256": "b" * 64, "size": 20, "licence": "BSD-3-Clause", "shape": "transition"},
            {"file": "Other.fs", "name": "Other", "sha256": "c" * 64, "size": 30, "licence": "MIT", "shape": "generator"},
        ]
        manifest = tool.build_manifest("Vidvox/ISF-Files", "0123456789abcdef", "ISF", {"sha256": "d" * 64}, files)
        self.assertEqual(manifest["format"], 1)
        (only,) = manifest["sets"]
        self.assertEqual(only["id"], "vidvox-isf-files")
        self.assertEqual(only["version"], "0123456")
        self.assertEqual(only["licence"], "MIT AND BSD-3-Clause")
        self.assertEqual(
            only["shaders"][0]["url"],
            "https://raw.githubusercontent.com/Vidvox/ISF-Files/0123456789abcdef/ISF/A%20Shader.fs",
        )
        self.assertEqual([entry["file"] for entry in only["shaders"]], ["A Shader.fs", "Curl.fs", "Other.fs"])
        self.assertEqual(only["licence_texts"][0]["url"], "https://raw.githubusercontent.com/Vidvox/ISF-Files/0123456789abcdef/LICENSE")
        self.assertEqual(only["licence_texts"][0]["sha256"], "d" * 64)
        self.assertEqual(only["source"]["url"], "https://github.com/Vidvox/ISF-Files/tree/0123456789abcdef")

    def test_finishing_keeps_what_the_corpus_drew_and_says_why_the_rest_was_left_out(self):
        with tempfile.TemporaryDirectory() as folder:
            work = Path(folder)
            entry = lambda name, include, licence, shape: {
                "file": name,
                "include": include,
                "licence": licence,
                "reason": "its own text says MIT" if include else "its own text says Creative Commons",
                "credit": "",
                "sha256": "e" * 64,
                "size": 100,
                "shape": shape,
            }
            (work / "classified.json").write_text(
                json.dumps(
                    {
                        "commit": "f" * 40,
                        "licence_text": {"sha256": "d" * 64, "size": 5},
                        "files": [
                            entry("Runs.fs", True, "MIT", "filter"),
                            entry("Does Not Run.fs", True, "MIT", "filter"),
                            entry("Not Allowed.fs", False, None, "filter"),
                        ],
                    }
                )
            )
            (work / "corpus.txt").write_text("ok   Runs.fs | Filter, 1 passes, mean rgba (1, 1, 1, 255)\nFAIL Does Not Run.fs | compile: a mistake\n")
            out, report = work / "shaders.json", work / "report.md"
            tool.finish(
                argparse.Namespace(
                    work=str(work), corpus=str(work / "corpus.txt"), repository="Vidvox/ISF-Files", folder="ISF", out=str(out), report=str(report)
                )
            )
            manifest = json.loads(out.read_text(encoding="utf8"))
            self.assertEqual([shader["file"] for shader in manifest["sets"][0]["shaders"]], ["Runs.fs"])
            words = report.read_text(encoding="utf8")
            self.assertIn("Not Allowed.fs | not bundled: its own text says Creative Commons", words)
            self.assertIn("Does Not Run.fs | may be bundled, and this build does not run it: compile: a mistake", words)
            self.assertIn("**In the manifest: 1 shader** (1 filter)", words)


if __name__ == "__main__":
    unittest.main()
