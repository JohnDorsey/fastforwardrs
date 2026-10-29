# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""Builds the folder of plugins that goes beside the program (what "bundled" means: LG-02 of docs/plugin-system-features.md, amended 2026-10-20).

    py tools/bundled_plugins/fetch_bundled_plugins.py --check
    py tools/bundled_plugins/fetch_bundled_plugins.py --platform windows-x86_64 --out target/release/plugins [--with-source]

`bundled-plugins/manifest.json` says which plugins, from which release files (each with its checksum), under which licence, and where their source is. This script

1. **checks the manifest against the licence policy** (`--check` does only this, and needs no network): GPL and LGPL licences that are compatible with GPLv3, and the permissive ones
   of LG-02, are allowed; **AGPL and GPL-2.0-only are not**; every address is https; every file has a checksum;
2. downloads each release file, **checks its checksum before anything is taken out of it** (and tries again if the network drops), and keeps it in a cache folder so that it is not
   downloaded again;
3. takes out of it **only the files the manifest names**, into `<out>/<id>/`, with **the licence text(s)** and a `NOTICE.txt` (name, version, authors, licence, homepage, where the
   source is), and with `--with-source` the source archive too (the corresponding source that a release of GPL and LGPL software owes: `docs/distribution-notices.md`).

The editor looks for a folder called `plugins` beside the program and trusts it without a question (the files of a release are as trusted as the program). Only the standard
library is used."""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import os
import pathlib
import platform
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
import zipfile
from collections.abc import Iterator

THE_PROJECT = pathlib.Path(__file__).resolve().parents[2]
THE_MANIFEST = THE_PROJECT / "bundled-plugins" / "manifest.json"
THE_PLATFORMS = ("windows-x86_64", "linux-x86_64", "macos-universal")

# The licences that may ship, by their SPDX names. LG-02: the permissive ones; and, since 2026-10-20, GPL and LGPL that are compatible with GPLv3.
PERMISSIVE = {"MIT", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Apache-2.0", "CC0-1.0", "Zlib", "CC-BY-4.0"}
COPYLEFT = {
    "GPL-3.0-only",
    "GPL-3.0-or-later",
    "GPL-2.0-or-later",
    "LGPL-3.0-only",
    "LGPL-3.0-or-later",
    "LGPL-2.1-or-later",
}
ALLOWED = PERMISSIVE | COPYLEFT

THE_SHA256 = re.compile(r"^[0-9a-f]{64}$")
THE_ID = re.compile(r"^[a-z0-9][a-z0-9-]*$")
# What a set holds: plugins (code), or data that plugins find beside themselves (Surge XT's patches and wavetables: files).
THE_KINDS = ("plugins", "data")


def licence_is_allowed(expression: str) -> bool:
    """Whether an SPDX expression may ship: every part joined by AND must be allowed, and at least one part joined by OR. (No parentheses are used in the manifest, and a name with
    one in it is not an allowed name.)"""
    for part in expression.split(" AND "):
        alternatives = [name.strip() for name in part.split(" OR ")]
        if not any(name in ALLOWED for name in alternatives):
            return False
    return True


def check_manifest(manifest: dict, allow_local_addresses: bool = False) -> list[str]:
    """What is wrong with the manifest, one line each (nothing if it is fit)."""
    problems: list[str] = []

    def address_is_fit(address) -> bool:
        if not isinstance(address, str):
            return False
        return address.startswith("https://") or (allow_local_addresses and address.startswith("file:"))

    if manifest.get("format") != 1:
        return [f"the format of the manifest is {manifest.get('format')!r}, and this script reads 1"]
    sets = manifest.get("sets")
    if not isinstance(sets, list) or not sets:
        return ["the manifest has no sets"]
    seen_ids: set[str] = set()
    # (The folders that the sets are put in: two sets may not be put in one, and a folder of data may not be the folder of a set of plugins.)
    seen_folders: set[str] = set()
    for number, plugin_set in enumerate(sets):
        name = plugin_set.get("id", f"#{number}")
        where = f"set {name}"
        if not isinstance(plugin_set.get("id"), str) or not THE_ID.match(plugin_set["id"]):
            problems.append(f"{where}: the id is not lower case letters, digits and dashes")
        elif plugin_set["id"] in seen_ids:
            problems.append(f"{where}: the id is used twice")
        seen_ids.add(str(plugin_set.get("id")))
        kind = plugin_set.get("kind", "plugins")
        if kind not in THE_KINDS:
            problems.append(f"{where}: {kind!r} is not a kind of set ({', '.join(THE_KINDS)})")
        is_data = kind == "data"
        folder_name = set_folder_name(plugin_set)
        if "folder" in plugin_set:
            if not is_data:
                problems.append(f"{where}: only data has a folder of its own (plugins are in a folder called after the id)")
            elif not _is_a_plain_file_name(plugin_set["folder"]) or plugin_set["folder"].startswith("."):
                problems.append(f"{where}: the folder is not a plain name of a folder")
        if folder_name in seen_folders:
            problems.append(f"{where}: the folder {folder_name!r} is the folder of another set too")
        seen_folders.add(str(folder_name))
        for field in ("name", "version", "homepage"):
            if not plugin_set.get(field):
                problems.append(f"{where}: no {field}")
        if not address_is_fit(plugin_set.get("homepage")):
            problems.append(f"{where}: the homepage is not an https address")
        if not plugin_set.get("authors"):
            problems.append(f"{where}: no authors")
        if not plugin_set.get("plugins"):
            problems.append(f"{where}: no plugins named")
        licence = plugin_set.get("licence", "")
        if not licence_is_allowed(licence):
            problems.append(f"{where}: the licence {licence!r} is not one that may ship (AGPL and GPL-2.0-only are not, and an unknown one is not)")
        texts = plugin_set.get("licence_texts") or []
        if not texts:
            problems.append(f"{where}: no licence text")
        for text in texts:
            if not (address_is_fit(text.get("url")) and THE_SHA256.match(str(text.get("sha256"))) and _is_a_plain_file_name(text.get("name"))):
                problems.append(f"{where}: a licence text needs a plain file name, an https address and a sha256")
        source = plugin_set.get("source") or {}
        if not address_is_fit(source.get("url")):
            problems.append(f"{where}: the source has no https address (the corresponding source is owed)")
        if not (THE_SHA256.match(str(source.get("sha256"))) or source.get("md5") or source.get("note")):
            problems.append(f"{where}: the source has no checksum, and no note that says why")
        platforms = plugin_set.get("platforms") or {}
        if not platforms:
            problems.append(f"{where}: no platforms")
        for platform_name, archive in platforms.items():
            there = f"{where}, {platform_name}"
            if platform_name not in THE_PLATFORMS:
                problems.append(f"{there}: not a platform that is known ({', '.join(THE_PLATFORMS)})")
            if not address_is_fit(archive.get("url")):
                problems.append(f"{there}: the address is not https")
            if not THE_SHA256.match(str(archive.get("sha256"))):
                problems.append(f"{there}: no sha256")
            members = archive.get("members") or []
            if not members:
                problems.append(f"{there}: no files taken")
            installer = archive.get("installer")
            if installer is not None and not (
                _is_a_plain_file_name(installer) and installer.lower().endswith(".pkg") and str(archive.get("url")).lower().endswith(".dmg")
            ):
                problems.append(f"{there}: an installer is a plain name of a .pkg file in a .dmg disk image")
            targets: set[str] = set()
            for member in members:
                taken, kept = member.get("from", ""), member.get("to", "")
                if not taken or taken.startswith(("/", "\\")) or ".." in re.split(r"[\\/]", taken):
                    problems.append(f"{there}: the file {taken!r} is not a name inside the archive")
                if is_data:
                    problems.extend(_problems_of_a_member_of_data(there, member, len(members)))
                    continue
                if member.get("leave_out"):
                    problems.append(f"{there}: only data leaves anything out")
                if not (_is_a_plain_file_name(kept) and kept.lower().endswith((".clap", ".lv2"))):
                    problems.append(f"{there}: {kept!r} is not a plain name of a .clap file or an .lv2 folder")
                # (An LV2 bundle is a folder: out of a zip or a tar it is taken by the name of the folder with a `/` at the end, and without it only one file would be taken.)
                if kept.lower().endswith(".lv2") and not installer and not taken.endswith("/"):
                    problems.append(f"{there}: {taken!r} is a folder (an .lv2 bundle) and its name inside the archive must end with /")
                if kept in targets:
                    problems.append(f"{there}: {kept!r} is taken twice")
                targets.add(kept)
    return problems


def set_folder_name(plugin_set: dict) -> str:
    """The name of the folder of a set in the folder of the plugins: the id, unless a set of data says another (the name that a plugin looks for)."""
    return plugin_set.get("folder") if isinstance(plugin_set.get("folder"), str) else str(plugin_set.get("id"))


def _problems_of_a_member_of_data(there: str, member: dict, how_many: int) -> list[str]:
    """What is wrong with the one member of a set of data: a folder of the archive (its name ends with `/`) whose contents are the folder of the set (`to` is `.`), and what it leaves out are paths inside it."""
    problems: list[str] = []
    if how_many != 1:
        problems.append(f"{there}: a set of data takes one folder of the archive")
    if member.get("to") != ".":
        problems.append(f"{there}: the member of a set of data is put as the folder of the set, and its name is \".\", not {member.get('to')!r}")
    if not str(member.get("from", "")).endswith("/"):
        problems.append(f"{there}: {member.get('from')!r} is the folder of a set of data, and its name inside the archive must end with /")
    for left_out in member.get("leave_out") or []:
        if not isinstance(left_out, str) or not left_out or left_out.startswith("/") or "\\" in left_out or ".." in left_out.split("/"):
            problems.append(f"{there}: {left_out!r} is not a path inside the folder (parts with / between them, none going up)")
    return problems


def _is_a_plain_file_name(name) -> bool:
    return isinstance(name, str) and bool(name) and name not in (".", "..") and not re.search(r"[\\/\x00]", name)


class FetchError(Exception):
    """Something that is not fit to go on with: a checksum that is wrong, a file that is not in the archive."""


def fetch(address: str, sha256: str | None, cache: pathlib.Path, md5: str | None = None) -> pathlib.Path:
    """The file at `address`, in the cache, whose checksum is as it is said to be (it is not downloaded again if the cache has it; one with the wrong checksum is thrown away)."""
    cache.mkdir(parents=True, exist_ok=True)
    key = sha256 or md5 or hashlib.sha256(address.encode()).hexdigest()
    target = cache / (key + "-" + address.rsplit("/", 1)[-1])
    if target.exists() and _checksum_is_right(target, sha256, md5):
        return target
    target.unlink(missing_ok=True)
    last_error: Exception | None = None
    for attempt in range(6):
        partial = target.with_suffix(target.suffix + ".part")
        try:
            with urllib.request.urlopen(address, timeout=60) as response, open(partial, "wb") as out:
                shutil.copyfileobj(response, out, 1 << 20)
        except (OSError, urllib.error.URLError) as error:
            last_error = error
            partial.unlink(missing_ok=True)
            time.sleep(0 if address.startswith("file:") else 2 * (attempt + 1))
            continue
        if not _checksum_is_right(partial, sha256, md5):
            partial.unlink(missing_ok=True)
            raise FetchError(f"{address}: its checksum is not the one the manifest says (the file at that address changed, or the manifest is wrong)")
        partial.replace(target)
        return target
    raise FetchError(f"{address}: could not be downloaded ({last_error})")


def _checksum_is_right(path: pathlib.Path, sha256: str | None, md5: str | None) -> bool:
    if sha256 is None and md5 is None:
        return True
    data = path.read_bytes()
    if sha256 is not None:
        return hashlib.sha256(data).hexdigest() == sha256
    return hashlib.md5(data).hexdigest() == md5  # noqa: S324 (the checksum a project publishes; not used to keep anyone out)


def _member_bytes(archive_path: pathlib.Path, wanted: str) -> bytes:
    """The content of the file `wanted` inside the zip or tar archive (exactly that name, a leading `./` of a tar being no part of it)."""
    if zipfile.is_zipfile(archive_path):
        with zipfile.ZipFile(archive_path) as archive:
            try:
                return archive.read(wanted)
            except KeyError:
                raise FetchError(f"{archive_path.name}: no file called {wanted!r} inside") from None
    with tarfile.open(archive_path, "r:*") as archive:
        for member in archive:
            if member.isfile() and member.name.removeprefix("./") == wanted:
                extracted = archive.extractfile(member)
                if extracted is not None:
                    return extracted.read()
    raise FetchError(f"{archive_path.name}: no file called {wanted!r} inside")


def _take_a_folder_from_a_zip(archive_path: pathlib.Path, wanted: str, to: pathlib.Path) -> None:
    """The folder `wanted` of the zip (a plugin of macOS is a bundle, a folder) with everything in it, at `to`: which files can be run is kept, and a link is made as a link."""
    prefix = wanted.rstrip("/") + "/"
    found = False
    with zipfile.ZipFile(archive_path) as archive:
        for info in archive.infolist():
            if not info.filename.startswith(prefix):
                continue
            relative = info.filename[len(prefix) :]
            if relative.startswith(("/", "\\")) or ".." in re.split(r"[\\/]", relative):
                raise FetchError(f"{archive_path.name}: an entry whose name goes out of the folder: {info.filename!r}")
            found = True
            target = to / relative if relative else to
            if info.is_dir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            mode = info.external_attr >> 16
            if stat.S_ISLNK(mode):
                os.symlink(archive.read(info).decode("utf8"), target)
                continue
            target.write_bytes(archive.read(info))
            if mode:
                target.chmod(mode & 0o777)
    if not found:
        raise FetchError(f"{archive_path.name}: no folder called {wanted.rstrip('/')!r} inside")


def _take_a_folder_from_a_tar(archive_path: pathlib.Path, wanted: str, to: pathlib.Path) -> None:
    """The folder `wanted` of the tar (an LV2 bundle is one) with everything in it, at `to`: which files can be run is kept, and a link is made as a link. A leading `./` of the names is no part of them."""
    prefix = wanted.rstrip("/")
    found = False
    with tarfile.open(archive_path, "r:*") as archive:
        for member in archive:
            name = member.name.removeprefix("./")
            if name != prefix and not name.startswith(prefix + "/"):
                continue
            relative = name[len(prefix) :].lstrip("/")
            if ".." in re.split(r"[\\/]", relative):
                raise FetchError(f"{archive_path.name}: an entry whose name goes out of the folder: {member.name!r}")
            found = True
            target = to / relative if relative else to
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            elif member.issym():
                target.parent.mkdir(parents=True, exist_ok=True)
                os.symlink(member.linkname, target)
            elif member.isfile():
                target.parent.mkdir(parents=True, exist_ok=True)
                extracted = archive.extractfile(member)
                if extracted is not None:
                    target.write_bytes(extracted.read())
                    target.chmod(member.mode & 0o777)
    if not found:
        raise FetchError(f"{archive_path.name}: no folder called {prefix!r} inside")


def _run_a_program_of_macos(command: list[str], what: str) -> None:
    try:
        done = subprocess.run(command, capture_output=True, text=True, check=False)
    except OSError as error:
        raise FetchError(f"`{command[0]}` could not be run, and it opens {what} of the builds for macOS (it is a program of macOS): {error}") from None
    if done.returncode != 0:
        raise FetchError(f"{what} could not be opened (`{' '.join(command[:2])}`): {done.stderr.strip()}")


@contextlib.contextmanager
def _expanded_installer(archive_path: pathlib.Path, installer: str, work: pathlib.Path) -> Iterator[pathlib.Path]:
    """What the installer `installer` (a .pkg) of the disk image would install, expanded in `work` (nothing is installed); the image is let go of afterwards."""
    mount = work / "mounted"
    mount.mkdir()
    _run_a_program_of_macos(["hdiutil", "attach", "-nobrowse", "-readonly", "-noverify", "-noautoopen", "-mountpoint", str(mount), str(archive_path)], "the disk images")
    try:
        expanded = work / "expanded"
        _run_a_program_of_macos(["pkgutil", "--expand-full", str(mount / installer), str(expanded)], "the installers")
        yield expanded
    finally:
        subprocess.run(["hdiutil", "detach", "-force", "-quiet", str(mount)], capture_output=True, check=False)


def _take_from_an_expanded_installer(expanded: pathlib.Path, wanted: str, to: pathlib.Path) -> None:
    source = expanded / wanted
    if not source.exists():
        raise FetchError(f"the installer has no {wanted!r}")
    if source.is_dir():
        shutil.copytree(source, to, symlinks=True)
    else:
        shutil.copy2(source, to)


def _leave_out(taken: pathlib.Path, paths: list[str], from_name: str) -> None:
    """Takes away from what was taken the paths that the manifest says are not kept (the parts of a set of data that may not ship). One that is not there is an error: the release file is not the one the manifest was made for."""
    for path in paths:
        wanted = taken / path.rstrip("/")
        if wanted.is_dir():
            shutil.rmtree(wanted)
        elif wanted.is_file() and not path.endswith("/"):
            wanted.unlink()
        else:
            raise FetchError(f"{path!r} is not in {from_name!r}, and the manifest says that it is left out: the release file is not the one that the manifest was made for, so nothing of it was used")


def _take_the_members(archive_path: pathlib.Path, archive: dict, building: pathlib.Path) -> None:
    """Puts the files the manifest names for this build in `building`: from a zip or a tar (a name that ends with `/` is a folder), or from the installer in a disk image."""
    members = archive["members"]
    if archive.get("installer"):
        with tempfile.TemporaryDirectory() as work, _expanded_installer(archive_path, archive["installer"], pathlib.Path(work)) as expanded:
            for member in members:
                _take_from_an_expanded_installer(expanded, member["from"], building / member["to"])
                _leave_out(building / member["to"], member.get("leave_out") or [], member["from"])
        return
    for member in members:
        if member["from"].endswith("/") and zipfile.is_zipfile(archive_path):
            _take_a_folder_from_a_zip(archive_path, member["from"], building / member["to"])
        elif member["from"].endswith("/"):
            _take_a_folder_from_a_tar(archive_path, member["from"], building / member["to"])
        else:
            (building / member["to"]).write_bytes(_member_bytes(archive_path, member["from"]))
        _leave_out(building / member["to"], member.get("leave_out") or [], member["from"])


def assemble(
    manifest: dict,
    platform_name: str,
    out: pathlib.Path,
    cache: pathlib.Path,
    with_source: bool = False,
    allow_local_addresses: bool = False,
    with_data: bool = True,
) -> list[pathlib.Path]:
    """Puts the plugins of the manifest for `platform_name` in `out`, one folder for each set, and gives the plugin files (the sets of data, which are put in too unless `with_data` is false, are not plugin
    files). A set whose files cannot all be had is not left half made."""
    problems = check_manifest(manifest, allow_local_addresses)
    if problems:
        raise FetchError("the manifest is not fit:\n  " + "\n  ".join(problems))
    made: list[pathlib.Path] = []
    out.mkdir(parents=True, exist_ok=True)
    kept_sets = []
    for plugin_set in manifest["sets"]:
        is_data = plugin_set.get("kind", "plugins") == "data"
        if is_data and not with_data:
            continue
        kept_sets.append(plugin_set)
        archive = plugin_set["platforms"].get(platform_name)
        if archive is None:
            print(f"{plugin_set['id']}: no build for {platform_name}, left out", file=sys.stderr)
            continue
        folder = out / set_folder_name(plugin_set)
        building = out / (plugin_set["id"] + ".building")
        shutil.rmtree(building, ignore_errors=True)
        building.mkdir()
        try:
            archive_path = fetch(archive["url"], archive["sha256"], cache)
            _take_the_members(archive_path, archive, building)
            for text in plugin_set["licence_texts"]:
                (building / text["name"]).write_bytes(fetch(text["url"], text["sha256"], cache).read_bytes())
            if with_source:
                source = plugin_set["source"]
                source_path = fetch(source["url"], source.get("sha256"), cache, source.get("md5"))
                (building / "source").mkdir()
                shutil.copyfile(source_path, building / "source" / source["url"].rsplit("/", 1)[-1])
            (building / "NOTICE.txt").write_text(notice_of(plugin_set, with_source), encoding="utf8")
        except BaseException:
            shutil.rmtree(building, ignore_errors=True)
            raise
        shutil.rmtree(folder, ignore_errors=True)
        building.rename(folder)
        if not is_data:
            made.extend(folder / member["to"] for member in archive["members"])
    (out / "README.txt").write_text(readme_of({**manifest, "sets": kept_sets}, with_source), encoding="utf8")
    return made


def notice_of(plugin_set: dict, with_source: bool) -> str:
    source = plugin_set["source"]
    lines = [
        f"{plugin_set['name']} {plugin_set['version']}",
        f"By {', '.join(plugin_set['authors'])}.",
        f"Licence: {plugin_set['licence']}" + (f" ({plugin_set['licence_note']})" if plugin_set.get("licence_note") else ""),
        f"Home page: {plugin_set['homepage']}",
        "",
        (
            "This is data as its authors released it (what the manifest says is not kept was left out), and it is bundled with fastforwardrs under its own licence (the text is in this folder)."
            if plugin_set.get("kind", "plugins") == "data"
            else "This is the plugin as its authors released it, unaltered, and it is bundled with fastforwardrs under its own licence (the text is in this folder)."
        ),
        "",
        f"The corresponding source of this version is {'in the folder `source` here and also ' if with_source else ''}at: {source['url']}",
    ]
    if source.get("note"):
        lines.append(source["note"])
    return "\n".join(lines) + "\n"


def readme_of(manifest: dict, with_source: bool) -> str:
    lines = [
        "Plugins bundled with fastforwardrs",
        "",
        "Each folder is one set of plugins that someone else wrote, with the licence text and a NOTICE.txt that says who wrote it, under which licence, and where its source is.",
        "fastforwardrs runs them as separate works (LICENSE-PLUGIN-CLARIFICATION). The sources" + (" are in the folders called `source`." if with_source else " are at the addresses in the NOTICE.txt files (a release also keeps its own copy)."),
        "",
    ]
    for plugin_set in manifest["sets"]:
        lines.append(f"- {plugin_set['name']} {plugin_set['version']} ({plugin_set['licence']}): {', '.join(plugin_set['plugins'])}")
    return "\n".join(lines) + "\n"


def this_platform() -> str:
    machine = platform.machine().lower()
    if sys.platform.startswith("win") and machine in ("amd64", "x86_64"):
        return "windows-x86_64"
    if sys.platform.startswith("linux") and machine in ("x86_64", "amd64"):
        return "linux-x86_64"
    if sys.platform == "darwin":
        return "macos-universal"
    return f"{sys.platform}-{machine}"


def main(arguments: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--manifest", type=pathlib.Path, default=THE_MANIFEST)
    parser.add_argument("--platform", default=this_platform(), help="one of " + ", ".join(THE_PLATFORMS))
    parser.add_argument("--out", type=pathlib.Path, help="the folder to make (it is called `plugins` and put beside the program)")
    parser.add_argument("--cache", type=pathlib.Path, default=pathlib.Path(tempfile.gettempdir()) / "fastforwardrs-bundled-plugin-cache")
    parser.add_argument("--with-source", action="store_true", help="also put the source archives in the folders")
    parser.add_argument("--without-data", action="store_true", help="leave out the sets of data (patches and wavetables that plugins use), which are the biggest download")
    parser.add_argument("--check", action="store_true", help="only check the manifest against the policy (no network)")
    given = parser.parse_args(arguments)
    manifest = json.loads(given.manifest.read_text(encoding="utf8"))
    problems = check_manifest(manifest)
    if problems:
        print("The manifest is not fit:\n  " + "\n  ".join(problems), file=sys.stderr)
        return 1
    if given.check:
        print(f"The manifest is fit: {len(manifest['sets'])} sets.")
        return 0
    if given.out is None:
        parser.error("--out is needed (or --check)")
    try:
        made = assemble(manifest, given.platform, given.out, given.cache, given.with_source, with_data=not given.without_data)
    except FetchError as error:
        print(error, file=sys.stderr)
        return 1
    print(f"{len(made)} plugin files in {given.out}:")
    for path in made:
        print("  ", path.relative_to(given.out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
