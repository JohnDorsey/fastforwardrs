# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""Checks of tools/bundled_plugins/fetch_bundled_plugins.py and of the manifest it reads. Nothing is downloaded: the archives are made here and found by `file:` addresses.
Run them with
    py -m unittest discover -s tools/bundled_plugins
(from the folder of the project)."""

import copy
import hashlib
import io
import json
import os
import pathlib
import sys
import tarfile
import tempfile
import unittest
import zipfile

import fetch_bundled_plugins as fetcher

THE_REAL_MANIFEST = json.loads(fetcher.THE_MANIFEST.read_text(encoding="utf8"))


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


class AFolderOfFixtures:
    """A zip and a tar.gz with fake plugins, a licence text and a source, and a manifest that names them."""

    def __init__(self, folder: pathlib.Path):
        self.folder = folder
        self.zip_path = folder / "plugins-win.zip"
        with zipfile.ZipFile(self.zip_path, "w") as archive:
            archive.writestr("pack-1.0/Reverb.clap", b"the reverb for windows")
            archive.writestr("pack-1.0/Other.clap", b"something else that is not taken")
        self.tar_path = folder / "plugins-linux.tar.gz"
        with tarfile.open(self.tar_path, "w:gz") as archive:
            for name, data in (("pack-1.0/Reverb.clap", b"the reverb for linux"), ("pack-1.0/Other.clap", b"not taken")):
                info = tarfile.TarInfo("./" + name)
                info.size = len(data)
                archive.addfile(info, io.BytesIO(data))
        self.licence_path = folder / "LICENSE"
        self.licence_path.write_bytes(b"GNU GENERAL PUBLIC LICENSE, a copy")
        self.source_path = folder / "pack-src.tar.xz"
        self.source_path.write_bytes(b"the source")

    def manifest(self) -> dict:
        def address(path: pathlib.Path) -> str:
            return path.as_uri()

        def archive(path: pathlib.Path) -> dict:
            return {
                "url": address(path),
                "sha256": sha256(path.read_bytes()),
                "members": [{"from": "pack-1.0/Reverb.clap", "to": "Reverb.clap"}],
            }

        return {
            "format": 1,
            "sets": [
                {
                    "id": "pack",
                    "name": "Pack",
                    "version": "1.0",
                    "authors": ["Someone"],
                    "licence": "GPL-3.0-or-later",
                    "homepage": "https://example.org/pack",
                    "plugins": ["Reverb"],
                    "licence_texts": [
                        {"name": "LICENSE.txt", "url": address(self.licence_path), "sha256": sha256(self.licence_path.read_bytes())}
                    ],
                    "source": {"url": address(self.source_path), "sha256": sha256(self.source_path.read_bytes())},
                    "platforms": {"windows-x86_64": archive(self.zip_path), "linux-x86_64": archive(self.tar_path)},
                }
            ],
        }


class TheLicencePolicy(unittest.TestCase):
    def test_gpl_and_lgpl_that_are_compatible_with_gplv3_and_the_permissive_licences_may_ship(self):
        for licence in (
            "GPL-3.0-or-later",
            "GPL-3.0-only",
            "GPL-2.0-or-later",
            "LGPL-3.0-only",
            "LGPL-3.0-or-later",
            "LGPL-2.1-or-later",
            "MIT",
            "ISC",
            "BSD-3-Clause",
            "CC0-1.0",
            "MIT AND GPL-3.0-or-later",
            "GPL-3.0-or-later OR MIT",
        ):
            self.assertTrue(fetcher.licence_is_allowed(licence), licence)

    def test_agpl_and_gpl_2_only_and_what_is_not_known_may_not(self):
        for licence in (
            "AGPL-3.0-only",
            "AGPL-3.0-or-later",
            "GPL-2.0-only",
            "GPL-2.0",
            "LGPL-2.1-only",
            "CC-BY-NC-4.0",
            "Proprietary",
            "",
            "MIT AND AGPL-3.0-only",
            "(MIT)",
            "GPL-3.0",
        ):
            self.assertFalse(fetcher.licence_is_allowed(licence), licence)

    def test_an_or_of_a_licence_that_may_not_and_one_that_may_is_fit_and_an_and_of_them_is_not(self):
        self.assertTrue(fetcher.licence_is_allowed("AGPL-3.0-only OR GPL-3.0-or-later"))
        self.assertFalse(fetcher.licence_is_allowed("AGPL-3.0-only AND GPL-3.0-or-later"))


class TheRealManifest(unittest.TestCase):
    def test_it_is_fit(self):
        self.assertEqual(fetcher.check_manifest(THE_REAL_MANIFEST), [])

    def test_it_has_what_the_maintainer_chose_and_not_what_the_maintainer_did_not(self):
        names = [plugin for plugin_set in THE_REAL_MANIFEST["sets"] for plugin in plugin_set["plugins"]]
        self.assertEqual(
            names,
            [
                "Dragonfly Hall Reverb",
                "Dragonfly Plate Reverb",
                "Dragonfly Room Reverb",
                "MVerb",
                "MaGigaverb",
                "Surge XT Effects",
            ],
        )
        # (What the sets are called, what they hold and how they are licensed: an address may name another plugin, as the text of the LGPL that MaGigaverb needs comes from the folder of 3BandEQ.)
        named = json.dumps([[s["id"], s["name"], s["plugins"], s["licence"]] for s in THE_REAL_MANIFEST["sets"]]).lower()
        for left_out in ("airwindows", "agpl", "nekobi", "zl equalizer", "3bandeq", "pingpongpan", "3bandsplitter"):
            self.assertNotIn(left_out, named)

    def test_every_set_has_a_build_for_windows_linux_and_macos_and_the_files_it_takes_are_the_plugins_it_names(self):
        for plugin_set in THE_REAL_MANIFEST["sets"]:
            self.assertEqual(set(plugin_set["platforms"]), {"windows-x86_64", "linux-x86_64", "macos-universal"}, plugin_set["id"])
            for build in plugin_set["platforms"].values():
                self.assertEqual(len(build["members"]), len(plugin_set["plugins"]), plugin_set["id"])

    def test_the_builds_for_macos_are_a_folder_of_a_zip_or_the_folders_of_an_installer_in_a_disk_image(self):
        for plugin_set in THE_REAL_MANIFEST["sets"]:
            build = plugin_set["platforms"]["macos-universal"]
            if build["url"].endswith(".dmg"):
                self.assertTrue(build["installer"].endswith(".pkg"), plugin_set["id"])
                for member in build["members"]:
                    self.assertTrue(member["from"].endswith(".clap") and "/Payload/" in member["from"], member["from"])
            else:
                self.assertTrue(build["url"].endswith(".zip"), plugin_set["id"])
                for member in build["members"]:
                    self.assertTrue(member["from"].endswith(".clap/"), member["from"])

    def test_the_source_of_every_set_is_named_with_a_checksum_or_the_reason_there_is_none(self):
        for plugin_set in THE_REAL_MANIFEST["sets"]:
            source = plugin_set["source"]
            self.assertTrue(source.get("sha256") or source.get("md5") or source.get("note"), plugin_set["id"])


class WhatIsWrongWithAManifest(unittest.TestCase):
    def setUp(self):
        self.manifest = copy.deepcopy(THE_REAL_MANIFEST)
        self.first = self.manifest["sets"][0]

    def problems(self) -> str:
        return "\n".join(fetcher.check_manifest(self.manifest))

    def test_a_licence_that_may_not_ship_is_said(self):
        self.first["licence"] = "AGPL-3.0-or-later"
        self.assertIn("is not one that may ship", self.problems())

    def test_an_address_that_is_not_https_is_said_for_every_kind_of_address(self):
        self.first["platforms"]["windows-x86_64"]["url"] = "http://example.org/a.zip"
        self.first["licence_texts"][0]["url"] = "ftp://example.org/LICENSE"
        self.first["source"]["url"] = "http://example.org/src.tar.gz"
        self.first["homepage"] = "http://example.org"
        problems = self.problems()
        for wanted in ("the address is not https", "a licence text needs", "the source has no https address", "the homepage is not an https address"):
            self.assertIn(wanted, problems)

    def test_a_missing_or_malformed_checksum_is_said(self):
        self.first["platforms"]["linux-x86_64"]["sha256"] = "abc"
        self.assertIn("no sha256", self.problems())

    def test_a_source_with_no_checksum_and_no_note_is_said(self):
        self.first["source"] = {"url": "https://example.org/src.tar.xz"}
        self.assertIn("the source has no checksum", self.problems())

    def test_a_file_that_would_go_outside_the_folder_is_said(self):
        build = self.first["platforms"]["windows-x86_64"]
        build["members"][0]["to"] = "../Escape.clap"
        build["members"][1:] = [{"from": "../../secret", "to": "Fine.clap"}, {"from": "/etc/passwd", "to": "Other.clap"}]
        problems = self.problems()
        self.assertIn("not a plain name of a .clap file", problems)
        self.assertIn("is not a name inside the archive", problems)

    def test_a_name_inside_the_archive_that_goes_up_is_said_on_its_own_and_so_is_one_that_begins_at_the_root(self):
        for name in ("../secret.clap", "pack/../../secret.clap", "pack\\..\\secret.clap", "/etc/passwd", "\\windows\\x.clap"):
            self.setUp()
            self.first["platforms"]["windows-x86_64"]["members"] = [{"from": name, "to": "Fine.clap"}]
            self.assertIn("is not a name inside the archive", self.problems(), name)

    def test_a_file_that_is_not_a_clap_library_or_is_taken_twice_is_said(self):
        build = self.first["platforms"]["windows-x86_64"]
        build["members"] = [{"from": "a/x.clap", "to": "x.clap"}, {"from": "b/x.clap", "to": "x.clap"}, {"from": "c/y.dll", "to": "y.dll"}]
        problems = self.problems()
        self.assertIn("'x.clap' is taken twice", problems)
        self.assertIn("'y.dll' is not a plain name of a .clap file", problems)

    def test_a_platform_that_is_not_known_and_a_set_with_no_plugins_and_an_id_used_twice_are_said(self):
        self.first["platforms"]["amiga"] = copy.deepcopy(self.first["platforms"]["linux-x86_64"])
        self.first["plugins"] = []
        self.manifest["sets"].append(copy.deepcopy(self.manifest["sets"][1]))
        self.manifest["sets"][-1]["id"] = self.manifest["sets"][1]["id"]
        problems = self.problems()
        for wanted in ("not a platform that is known", "no plugins named", "the id is used twice"):
            self.assertIn(wanted, problems)

    def test_an_installer_is_only_a_plain_pkg_name_in_a_dmg(self):
        build = self.first["platforms"]["windows-x86_64"]
        for installer, url in (
            ("Plugins.pkg", "https://example.org/a.zip"),
            ("../Plugins.pkg", "https://example.org/a.dmg"),
            ("sub/Plugins.pkg", "https://example.org/a.dmg"),
            ("Plugins.dmg", "https://example.org/a.dmg"),
            ("", "https://example.org/a.dmg"),
        ):
            build["installer"], build["url"] = installer, url
            self.assertIn("an installer is a plain name of a .pkg file in a .dmg", self.problems(), (installer, url))
        build["installer"], build["url"] = "Plugins.pkg", "https://example.org/a.dmg"
        self.assertNotIn("an installer", self.problems())

    def test_another_format_and_no_sets_are_refused_at_once(self):
        self.assertEqual(len(fetcher.check_manifest({"format": 2, "sets": []})), 1)
        self.assertEqual(fetcher.check_manifest({"format": 1, "sets": []}), ["the manifest has no sets"])


class AssemblingTheFolder(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.scratch.name)
        self.fixtures = AFolderOfFixtures(self.root)
        self.out = self.root / "plugins"
        self.cache = self.root / "cache"

    def tearDown(self):
        self.scratch.cleanup()

    def assemble(self, manifest=None, platform_name="windows-x86_64", **more):
        return fetcher.assemble(manifest or self.fixtures.manifest(), platform_name, self.out, self.cache, allow_local_addresses=True, **more)

    def test_only_the_files_the_manifest_names_are_taken_with_the_licence_and_a_notice(self):
        made = self.assemble()

        self.assertEqual(made, [self.out / "pack" / "Reverb.clap"])
        self.assertEqual(sorted(path.name for path in (self.out / "pack").iterdir()), ["LICENSE.txt", "NOTICE.txt", "Reverb.clap"])
        self.assertEqual((self.out / "pack" / "Reverb.clap").read_bytes(), b"the reverb for windows")
        self.assertEqual((self.out / "pack" / "LICENSE.txt").read_bytes(), b"GNU GENERAL PUBLIC LICENSE, a copy")
        notice = (self.out / "pack" / "NOTICE.txt").read_text(encoding="utf8")
        for wanted in ("Pack 1.0", "By Someone", "Licence: GPL-3.0-or-later", "https://example.org/pack", "corresponding source", "pack-src.tar.xz"):
            self.assertIn(wanted, notice)
        self.assertIn("Pack 1.0 (GPL-3.0-or-later): Reverb", (self.out / "README.txt").read_text(encoding="utf8"))

    def test_a_tar_archive_gives_its_files_by_the_names_inside_it(self):
        self.assemble(platform_name="linux-x86_64")
        self.assertEqual((self.out / "pack" / "Reverb.clap").read_bytes(), b"the reverb for linux")

    def a_zip_with_a_bundle_manifest(self, member_from: str) -> dict:
        path = self.root / "plugins-mac.zip"
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr("Reverb.clap/", b"")
            runnable = zipfile.ZipInfo("Reverb.clap/Contents/MacOS/Reverb")
            runnable.external_attr = (0o100755) << 16
            archive.writestr(runnable, b"the code")
            plain = zipfile.ZipInfo("Reverb.clap/Contents/Info.plist")
            plain.external_attr = (0o100644) << 16
            archive.writestr(plain, b"<plist/>")
            archive.writestr("Reverb.clapped/Contents/Other", b"a folder whose name only begins like it")
            archive.writestr("Other.clap/Contents/MacOS/Other", b"another plugin, not taken")
        manifest = self.fixtures.manifest()
        build = {"url": path.as_uri(), "sha256": sha256(path.read_bytes()), "members": [{"from": member_from, "to": "Reverb.clap"}]}
        manifest["sets"][0]["platforms"] = {"macos-universal": build}
        return manifest

    def test_a_folder_of_a_zip_is_taken_with_everything_in_it_and_nothing_outside_it_and_what_can_be_run_can_still_be(self):
        made = self.assemble(self.a_zip_with_a_bundle_manifest("Reverb.clap/"), platform_name="macos-universal")

        bundle = self.out / "pack" / "Reverb.clap"
        self.assertEqual(made, [bundle])
        found = sorted(str(path.relative_to(bundle).as_posix()) for path in bundle.rglob("*") if path.is_file())
        self.assertEqual(found, ["Contents/Info.plist", "Contents/MacOS/Reverb"], "not `Reverb.clapped`, not `Other.clap`")
        self.assertEqual((bundle / "Contents" / "MacOS" / "Reverb").read_bytes(), b"the code")
        if os.name == "posix":
            self.assertEqual((bundle / "Contents" / "MacOS" / "Reverb").stat().st_mode & 0o777, 0o755)
            self.assertEqual((bundle / "Contents" / "Info.plist").stat().st_mode & 0o777, 0o644)

    def test_a_folder_that_is_not_in_the_zip_is_said_and_nothing_is_left_half_made(self):
        with self.assertRaisesRegex(fetcher.FetchError, "no folder called 'Missing.clap'"):
            self.assemble(self.a_zip_with_a_bundle_manifest("Missing.clap/"), platform_name="macos-universal")
        self.assertFalse((self.out / "pack").exists() or (self.out / "pack.building").exists())

    @unittest.skipIf(sys.platform == "darwin", "macOS has hdiutil: this is about what is said where it has not")
    def test_a_disk_image_where_the_system_cannot_open_it_is_said_in_words_and_nothing_is_left(self):
        manifest = self.a_zip_with_a_bundle_manifest("Reverb.clap")
        build = manifest["sets"][0]["platforms"]["macos-universal"]
        image = self.root / "image.dmg"
        image.write_bytes(b"not really a disk image")
        build.update(url=image.as_uri(), sha256=sha256(image.read_bytes()), installer="Plugins.pkg")

        with self.assertRaisesRegex(fetcher.FetchError, "hdiutil"):
            self.assemble(manifest, platform_name="macos-universal")

        self.assertFalse((self.out / "pack").exists() or (self.out / "pack.building").exists())

    def test_with_source_the_source_archive_is_put_in_the_folder_and_the_notice_says_so(self):
        self.assemble(with_source=True)
        self.assertEqual((self.out / "pack" / "source" / "pack-src.tar.xz").read_bytes(), b"the source")
        self.assertIn("in the folder `source` here", (self.out / "pack" / "NOTICE.txt").read_text(encoding="utf8"))

    def test_without_source_there_is_none(self):
        self.assemble()
        self.assertFalse((self.out / "pack" / "source").exists())

    def test_an_archive_whose_checksum_is_not_the_one_said_is_refused_and_nothing_is_left_half_made(self):
        manifest = self.fixtures.manifest()
        manifest["sets"][0]["platforms"]["windows-x86_64"]["sha256"] = "0" * 64

        with self.assertRaisesRegex(fetcher.FetchError, "checksum is not the one the manifest says"):
            self.assemble(manifest)

        self.assertEqual([path.name for path in self.out.iterdir()], [], "no folder of the set, and no half made one")

    def test_a_licence_text_whose_checksum_is_wrong_is_refused_too(self):
        manifest = self.fixtures.manifest()
        manifest["sets"][0]["licence_texts"][0]["sha256"] = "1" * 64
        with self.assertRaisesRegex(fetcher.FetchError, "checksum"):
            self.assemble(manifest)
        self.assertFalse((self.out / "pack").exists())

    def test_a_file_that_is_not_in_the_archive_is_said_and_the_set_that_was_there_before_is_kept(self):
        self.assemble()
        manifest = self.fixtures.manifest()
        manifest["sets"][0]["platforms"]["windows-x86_64"]["members"][0]["from"] = "pack-1.0/Missing.clap"

        with self.assertRaisesRegex(fetcher.FetchError, "no file called 'pack-1.0/Missing.clap'"):
            self.assemble(manifest)

        self.assertEqual((self.out / "pack" / "Reverb.clap").read_bytes(), b"the reverb for windows")
        self.assertFalse((self.out / "pack.building").exists())

    def test_a_second_assembly_replaces_the_first_and_uses_the_cache_without_going_to_the_addresses_again(self):
        manifest = self.fixtures.manifest()
        self.assemble(manifest)
        (self.out / "pack" / "Stale.clap").write_bytes(b"left from before")
        self.fixtures.zip_path.unlink()

        self.assemble(manifest)

        self.assertFalse((self.out / "pack" / "Stale.clap").exists())
        self.assertTrue((self.out / "pack" / "Reverb.clap").exists())

    def test_a_platform_a_set_has_no_build_for_leaves_it_out_and_the_others_are_made(self):
        manifest = self.fixtures.manifest()
        second = copy.deepcopy(manifest["sets"][0])
        second["id"] = "other"
        del second["platforms"]["windows-x86_64"]
        manifest["sets"].append(second)

        made = self.assemble(manifest)

        self.assertEqual(made, [self.out / "pack" / "Reverb.clap"])
        self.assertFalse((self.out / "other").exists())

    def test_a_manifest_that_is_not_fit_is_refused_before_anything_is_fetched(self):
        manifest = self.fixtures.manifest()
        manifest["sets"][0]["licence"] = "AGPL-3.0-only"
        with self.assertRaisesRegex(fetcher.FetchError, "the manifest is not fit"):
            self.assemble(manifest)
        self.assertFalse(self.cache.exists())

    def test_the_command_line_checks_a_manifest_and_says_so(self):
        path = self.root / "manifest.json"
        path.write_text(json.dumps(THE_REAL_MANIFEST), encoding="utf8")
        self.assertEqual(fetcher.main(["--manifest", str(path), "--check"]), 0)
        THE_REAL_MANIFEST_BAD = copy.deepcopy(THE_REAL_MANIFEST)
        THE_REAL_MANIFEST_BAD["sets"][0]["licence"] = "AGPL-3.0-only"
        path.write_text(json.dumps(THE_REAL_MANIFEST_BAD), encoding="utf8")
        self.assertEqual(fetcher.main(["--manifest", str(path), "--check"]), 1)


if __name__ == "__main__":
    unittest.main()
