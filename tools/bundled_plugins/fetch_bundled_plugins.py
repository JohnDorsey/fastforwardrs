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
import hashlib
import json
import pathlib
import platform
import re
import shutil
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
import zipfile

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
    for number, plugin_set in enumerate(sets):
        name = plugin_set.get("id", f"#{number}")
        where = f"set {name}"
        if not isinstance(plugin_set.get("id"), str) or not THE_ID.match(plugin_set["id"]):
            problems.append(f"{where}: the id is not lower case letters, digits and dashes")
        elif plugin_set["id"] in seen_ids:
            problems.append(f"{where}: the id is used twice")
        seen_ids.add(str(plugin_set.get("id")))
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
            targets: set[str] = set()
            for member in members:
                taken, kept = member.get("from", ""), member.get("to", "")
                if not taken or taken.startswith(("/", "\\")) or ".." in re.split(r"[\\/]", taken):
                    problems.append(f"{there}: the file {taken!r} is not a name inside the archive")
                if not (_is_a_plain_file_name(kept) and kept.lower().endswith(".clap")):
                    problems.append(f"{there}: {kept!r} is not a plain name of a .clap file")
                if kept in targets:
                    problems.append(f"{there}: {kept!r} is taken twice")
                targets.add(kept)
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


def assemble(
    manifest: dict,
    platform_name: str,
    out: pathlib.Path,
    cache: pathlib.Path,
    with_source: bool = False,
    allow_local_addresses: bool = False,
) -> list[pathlib.Path]:
    """Puts the plugins of the manifest for `platform_name` in `out`, one folder for each set, and gives the plugin files. A set whose files cannot all be had is not left half made."""
    problems = check_manifest(manifest, allow_local_addresses)
    if problems:
        raise FetchError("the manifest is not fit:\n  " + "\n  ".join(problems))
    made: list[pathlib.Path] = []
    out.mkdir(parents=True, exist_ok=True)
    for plugin_set in manifest["sets"]:
        archive = plugin_set["platforms"].get(platform_name)
        if archive is None:
            print(f"{plugin_set['id']}: no build for {platform_name}, left out", file=sys.stderr)
            continue
        folder = out / plugin_set["id"]
        building = out / (plugin_set["id"] + ".building")
        shutil.rmtree(building, ignore_errors=True)
        building.mkdir()
        try:
            archive_path = fetch(archive["url"], archive["sha256"], cache)
            for member in archive["members"]:
                (building / member["to"]).write_bytes(_member_bytes(archive_path, member["from"]))
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
        made.extend(folder / member["to"] for member in archive["members"])
    (out / "README.txt").write_text(readme_of(manifest, with_source), encoding="utf8")
    return made


def notice_of(plugin_set: dict, with_source: bool) -> str:
    source = plugin_set["source"]
    lines = [
        f"{plugin_set['name']} {plugin_set['version']}",
        f"By {', '.join(plugin_set['authors'])}.",
        f"Licence: {plugin_set['licence']}" + (f" ({plugin_set['licence_note']})" if plugin_set.get("licence_note") else ""),
        f"Home page: {plugin_set['homepage']}",
        "",
        "This is the plugin as its authors released it, unaltered, and it is bundled with fastforwardrs under its own licence (the text is in this folder).",
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
        made = assemble(manifest, given.platform, given.out, given.cache, given.with_source)
    except FetchError as error:
        print(error, file=sys.stderr)
        return 1
    print(f"{len(made)} plugin files in {given.out}:")
    for path in made:
        print("  ", path.relative_to(given.out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
