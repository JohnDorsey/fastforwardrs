# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later
"""Makes the manifest of the shaders that the program can fetch (`bundled-plugins/shaders.json`), from a collection of ISF shaders in a repository at one commit.

What it does, in two phases, so that **what ships is only what the licence policy allows and what this build of the program runs**:

1. `--prepare`: downloads the commit's `.fs` files and its `LICENSE` (each file at that commit, by one run of curl, into the folder given by `--work`, **which must be empty or
   made by an earlier `--prepare`**), reads what each file says of its own licence and its credit, and **sorts them** (rules below). The files that may be bundled are copied,
   by their own names, into `<work>/candidates`, and `<work>/classified.json` says what was decided of every file, and why.
2. Run the corpus on that folder (the program reads, compiles, installs and draws every shader of it, and says what became of each):

       FASTFORWARDRS_ISF_CORPUS=<work>/candidates cargo test --lib isf_corpus -- --ignored --nocapture > <work>/corpus.txt

3. `--finish`: keeps of the candidates **only those that the corpus drew** (`ok` lines), and writes the manifest (`--out`) and the record of what was left out and why (`--report`,
   `docs/bundleable-isf-shaders.md`).

The rules (LG-02 of `docs/plugin-system-features.md`: the permissive licences, and GPL and LGPL that are compatible with GPLv3): a shader is bundled if

* its own text says an allowed licence (`License: MIT`, `The MIT License`, a BSD notice), **or**
* it is credited to the people of the repository (or to no one) or to an individual who gave it to the repository, names no other source of code and no licence that does not allow it
  (a gift to a repository that has an MIT `LICENSE` is under it: GitHub's terms say so), **and**
* it brings no pictures of its own (their licence is not said), and
* its text says nothing of Creative Commons, non-commercial terms, Shadertoy or Inigo Quilez, and does not come from the person named in `NOT_ALLOWED_AUTHORS`.

Everything it reads is **data from the network and is never run**: the files are read as bytes and text, the header as JSON. Run it with `py -I`.
"""

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
import urllib.parse
from pathlib import Path

# The people whose shaders are not given under a licence that allows this (their own text of it, in the ones that were read).
NOT_ALLOWED_AUTHORS = ("mojovideotech",)

# What a text can say of its licence: a line that matches the first pattern decides that the licence is not allowed; the others name the licence that the text says.
NOT_ALLOWED = re.compile(r"creative commons|non-?commercial|\bcc[- ]by", re.IGNORECASE)
LICENCES_SAID = (
    (re.compile(r"license\s*:?\s*bsd\s*3|bsd 3", re.IGNORECASE), "BSD-3-Clause"),
    (re.compile(r"license\s*:?\s*bsd\s*2|bsd 2", re.IGNORECASE), "BSD-2-Clause"),
    (re.compile(r"license\s*:?\s*mit|the mit license|mit license", re.IGNORECASE), "MIT"),
)

# What shows that the code is someone else's (the credit is looked at as well as the body).
OTHER_SOURCES = re.compile(
    r"shadertoy|http://|https://|adapted|ported|converted from|forked from|inspired|stackoverflow|inigo|quilez|original implementation|with help from",
    re.IGNORECASE,
)

# What the files that were looked at by eye and are let in anyway say, with the reason (the one place where a person's judgement is kept).
BY_HAND_IN = {
    "Simplex Noise.fs": "its text names the MIT licence of the noise it uses (Ashima Arts' webgl-noise), and the rest is of the repository",
    "Exposure Adjust.fs": "the only address in it is of the documentation of a filter of Apple's, not of code",
}

SHAPE_WORDS = {"Filter": "filter", "Generator": "generator", "Transition": "transition"}

# What comes between the reason that a shader is left out for (which is counted) and the line of the file that shows it (which is not).
REASON_AND_DETAIL = " -- "


def header_of(text):
    """The JSON of the header (the first block comment that begins with a brace), or None."""
    for found in re.finditer(r"/\*", text):
        rest = text[found.end():]
        if rest.lstrip().startswith("{"):
            end = rest.find("*/")
            if end < 0:
                return None
            try:
                return json.loads(rest[:end])
            except ValueError:
                return None
    return None


def shape_of(header):
    """`transition`, `filter` or `generator`, as the program tells them apart (by the image inputs)."""
    images = {item.get("NAME") for item in header.get("INPUTS", []) if item.get("TYPE") == "image"}
    if {"startImage", "endImage"} <= images:
        return "transition"
    return "filter" if "inputImage" in images else "generator"


def classify(file_name, text):
    """What is to be done with the shader `text` of the file `file_name`: ``(include, licence, reason)``. The licence is an SPDX name for one that is included."""
    header = header_of(text)
    if header is None:
        return False, None, "no header that can be read"
    credit = str(header.get("CREDIT", "")).strip()
    if header.get("IMPORTED"):
        return False, None, "brings pictures of its own, whose licence is not said"
    if any(author in credit.lower() for author in NOT_ALLOWED_AUTHORS):
        return False, None, "credited to a person whose shaders are Creative Commons non-commercial"
    lines = [line for line in text.splitlines() if "gl-transitions/gl-transitions/tree" not in line]
    said = []
    for line in lines:
        if NOT_ALLOWED.search(line):
            return False, None, "its own text says a licence that does not allow this (Creative Commons, non-commercial)"
        for pattern, name in LICENCES_SAID:
            if pattern.search(line) and name not in said:
                said.append(name)
    if len(said) > 1:
        return False, None, "its text names more than one licence (" + ", ".join(said) + "): a person must look at it"
    if said:
        return True, said[0], "its own text says " + said[0]
    if file_name in BY_HAND_IN:
        return True, "MIT", BY_HAND_IN[file_name]
    converted = "gl-transitions" in credit.lower() or "gl-transitions" in str(header.get("DESCRIPTION", "")).lower()
    if converted:
        return False, None, "converted from gl-transitions and says no licence"
    body = text[text.find("*/") + 2 :] if "*/" in text else text
    other = [line.strip() for line in body.splitlines() + [credit] if OTHER_SOURCES.search(line)]
    if other:
        return False, None, "names a source of code that says no licence" + REASON_AND_DETAIL + other[0][:100]
    # (Credited to the people of the repository, to no one, or to a person who gave the shader to it, and nothing that says it is someone else's.)
    return True, "MIT", "no licence of its own, no other source named: the licence of the repository"


def category_of(why):
    """What a reason is, for counting: without 'not bundled: ', without the line of the file that shows it, and (for what the corpus said) without its numbers and its names."""
    head = why.removeprefix("not bundled: ").split(REASON_AND_DETAIL)[0]
    head = head.removeprefix("may be bundled, and this build does not run it: ")
    head = re.sub(r"line \d+, column \d+: ", "", head)
    head = re.sub(r"called \w+", "called X", head)
    head = re.sub(r"\d+", "N", head)
    return head[:110]


def sha256_of(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def read_corpus(text):
    """What the corpus run said: ``{file name: shape word}`` for each shader that ``ok`` (read, compiled, installed and drawn), and ``{file name: why}`` for the others."""
    ok, failed = {}, {}
    for line in text.splitlines():
        if line.startswith("ok "):
            name, _, rest = line[3:].strip().partition(" | ")
            shape = rest.split(",")[0].strip()
            ok[name.strip()] = SHAPE_WORDS.get(shape, shape.lower())
        elif line.startswith("FAIL "):
            name, _, why = line[5:].strip().partition(" | ")
            failed[name.strip()] = why.strip()
    return ok, failed


def build_manifest(repository, commit, folder_in_repository, licence_text, files):
    """The manifest. `files` are the shaders that stay, each ``{"file", "name", "sha256", "size", "licence", "shape"}``, in the order of their names."""
    base = f"https://raw.githubusercontent.com/{repository}/{commit}/"
    licences = []
    for entry in files:
        if entry["licence"] not in licences:
            licences.append(entry["licence"])
    return {
        "format": 1,
        "sets": [
            {
                "id": repository.replace("/", "-").lower().replace("_", "-"),
                "name": "Vidvox ISF files" if repository == "Vidvox/ISF-Files" else repository,
                "version": commit[:7],
                "authors": ["Vidvox", "and the people credited in the header of each shader"],
                "licence": " AND ".join(licences),
                "licence_note": "the licence of the repository, and of each file the one its own text says",
                "homepage": f"https://github.com/{repository}",
                "licence_texts": [
                    {
                        "name": "LICENSE-" + repository.split("/")[-1] + ".txt",
                        "url": base + "LICENSE",
                        "sha256": licence_text["sha256"],
                    }
                ],
                "source": {
                    "url": f"https://github.com/{repository}/tree/{commit}",
                    "note": "The shaders are the files of that version of the repository, unaltered.",
                },
                "shaders": [
                    {
                        "name": entry["name"],
                        "file": entry["file"],
                        "url": base + urllib.parse.quote(folder_in_repository + "/" + entry["file"]),
                        "sha256": entry["sha256"],
                        "size": entry["size"],
                        "licence": entry["licence"],
                        "shape": entry["shape"],
                    }
                    for entry in files
                ],
            }
        ],
    }


def run_curl(config_text, work):
    config = work / "curl.configuration"
    config.write_text(config_text, encoding="utf8")
    command = ["curl", "--fail-early", "--fail", "--location", "--silent", "--show-error", "--proto", "=https", "--proto-redir", "=https"]
    command += ["--retry", "5", "--retry-delay", "2", "--retry-all-errors", "--connect-timeout", "20", "--config", str(config)]
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode != 0 and "revocation" in result.stderr.lower():
        result = subprocess.run(command + ["--ssl-revoke-best-effort"], capture_output=True, text=True)
    config.unlink(missing_ok=True)
    if result.returncode != 0:
        sys.exit("curl failed: " + result.stderr.strip())


def quoted(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def prepare(arguments):
    work = Path(arguments.work)
    work.mkdir(parents=True, exist_ok=True)
    if any(work.iterdir()) and not (work / "mapping.json").exists():
        sys.exit(f"{work} is not empty and was not made by --prepare: use a folder of its own")
    mapping_file = work / "mapping.json"
    if not mapping_file.exists():
        tree_url = f"https://api.github.com/repos/{arguments.repository}/git/trees/{arguments.commit}?recursive=1"
        listing = work / "tree.json"
        run_curl("url = " + quoted(tree_url) + "\noutput = " + quoted(str(listing)) + "\n", work)
        tree = json.loads(listing.read_text(encoding="utf8"))
        if tree.get("truncated"):
            sys.exit("the list of files of the repository is cut short: it is too big for this tool")
        wanted = [
            entry["path"]
            for entry in tree["tree"]
            if entry["type"] == "blob"
            and (entry["path"] == "LICENSE" or (entry["path"].startswith(arguments.folder + "/") and entry["path"].lower().endswith(".fs")))
        ]
        files_folder = work / "files"
        files_folder.mkdir(exist_ok=True)
        base = f"https://raw.githubusercontent.com/{arguments.repository}/{arguments.commit}/"
        config = ""
        mapping = []
        for number, path in enumerate(wanted):
            name = f"{number:04d}"
            config += "url = " + quoted(base + urllib.parse.quote(path)) + "\noutput = " + quoted(str(files_folder / name)) + "\n"
            mapping.append({"n": name, "path": path})
        run_curl(config, work)
        mapping_file.write_text(json.dumps({"commit": arguments.commit, "files": mapping}, indent=1), encoding="utf8")
    mapping = json.loads(mapping_file.read_text(encoding="utf8"))
    candidates = work / "candidates"
    if candidates.exists():
        shutil.rmtree(candidates)
    candidates.mkdir()
    decided = []
    licence_text = None
    for item in mapping["files"]:
        data = (work / "files" / item["n"]).read_bytes()
        if item["path"] == "LICENSE":
            licence_text = {"sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}
            continue
        file_name = item["path"].rsplit("/", 1)[-1]
        text = data.decode("utf8", errors="replace")
        include, licence, reason = classify(file_name, text)
        header = header_of(text) or {}
        decided.append(
            {
                "file": file_name,
                "include": include,
                "licence": licence,
                "reason": reason,
                "credit": str(header.get("CREDIT", "")).strip(),
                "sha256": hashlib.sha256(data).hexdigest(),
                "size": len(data),
                "shape": shape_of(header) if header else None,
            }
        )
        if include:
            (candidates / file_name).write_bytes(data)
    (work / "classified.json").write_text(
        json.dumps({"commit": mapping["commit"], "licence_text": licence_text, "files": decided}, indent=1), encoding="utf8"
    )
    kept = sum(1 for entry in decided if entry["include"])
    print(f"{len(decided)} shaders read: {kept} may be bundled (in {candidates}), {len(decided) - kept} may not (see classified.json)")


def finish(arguments):
    work = Path(arguments.work)
    classified = json.loads((work / "classified.json").read_text(encoding="utf8"))
    ok, failed = read_corpus(Path(arguments.corpus).read_text(encoding="utf8", errors="replace"))
    files = []
    left_out = []
    for entry in sorted(classified["files"], key=lambda entry: entry["file"]):
        if not entry["include"]:
            left_out.append((entry["file"], "not bundled: " + entry["reason"]))
        elif entry["file"] not in ok:
            left_out.append((entry["file"], "may be bundled, and this build does not run it: " + failed.get(entry["file"], "the corpus said nothing of it")))
        else:
            name = entry["file"].rsplit(".", 1)[0]
            files.append(
                {
                    "file": entry["file"],
                    "name": name,
                    "sha256": entry["sha256"],
                    "size": entry["size"],
                    "licence": entry["licence"],
                    "shape": ok[entry["file"]],
                }
            )
    if not classified["licence_text"]:
        sys.exit("the repository has no LICENSE file: nothing may be bundled from it")
    manifest = build_manifest(arguments.repository, classified["commit"], arguments.folder, classified["licence_text"], files)
    Path(arguments.out).write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf8")
    shapes = {}
    for entry in files:
        shapes[entry["shape"]] = shapes.get(entry["shape"], 0) + 1
    in_words = ", ".join(f"{count} {shape}{'' if count == 1 else 's'}" for shape, count in sorted(shapes.items()))
    not_allowed = [(name, why) for name, why in left_out if why.startswith("not bundled: ")]
    not_run = [(name, why) for name, why in left_out if not why.startswith("not bundled: ")]

    def counted(reasons):
        """The reasons, each with how many shaders have it: the reason itself, without the line of the file that shows it, and a message of the corpus without its numbers and names."""
        counts = {}
        for _, why in reasons:
            key = category_of(why)
            counts[key] = counts.get(key, 0) + 1
        return sorted(counts.items(), key=lambda item: (-item[1], item[0]))

    lines = [
        "# Shaders that can be bundled (ISF)",
        "",
        f"The record of `bundled-plugins/shaders.json`, made by `tools/bundled_plugins/make_the_shader_manifest.py` from **{arguments.repository} at commit `{classified['commit']}`**. The decisions are made",
        "by the rules in that file (LG-02 of `plugin-system-features.md`: the permissive licences, and GPL and LGPL that are compatible with GPLv3), from **what each file says of itself**, and then by running this build of the program on the",
        "files that may be bundled (`engine::isf_corpus_tests`: read, compile, install and draw each), so that **a shader in the manifest is one that runs**. Nothing that was read is ever run by the tool.",
        "",
        "## What came of it",
        "",
        f"* **In the manifest: {len(files)} shader{'' if len(files) == 1 else 's'}** ({in_words}).",
        f"* Left out: **{len(left_out)}** of the {len(classified['files'])} shaders of the repository: {len(not_allowed)} that may not be bundled, and {len(not_run)} that may be and that this build does not run yet.",
        "",
        "### Why the ones that may not be bundled are not",
        "",
    ]
    for reason, count in counted(not_allowed):
        lines.append(f"* {count}: {reason}")
    lines += ["", "### What stops this build from running the others", ""]
    for reason, count in counted(not_run):
        lines.append(f"* {count}: {reason}")
    lines += ["", "## Every shader that was left out", "", "| file | why |", "|---|---|"]
    for file_name, why in sorted(left_out, key=lambda item: (item[1], item[0])):
        lines.append(f"| {file_name} | {why.replace('|', '/')} |")
    Path(arguments.report).write_text("\n".join(lines) + "\n", encoding="utf8")
    print(f"{len(files)} shaders in {arguments.out}; {len(left_out)} left out, in {arguments.report}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--repository", default="Vidvox/ISF-Files")
    parser.add_argument("--commit", help="the commit (a full hash) to make the manifest of (--prepare)")
    parser.add_argument("--folder", default="ISF", help="the folder of the repository that the shaders are in")
    parser.add_argument("--work", required=True, help="an empty folder, or one made by an earlier --prepare")
    parser.add_argument("--prepare", action="store_true")
    parser.add_argument("--finish", action="store_true")
    parser.add_argument("--corpus", help="the output of the corpus run on <work>/candidates (--finish)")
    parser.add_argument("--out", default="bundled-plugins/shaders.json")
    parser.add_argument("--report", default="docs/bundleable-isf-shaders.md")
    arguments = parser.parse_args()
    if arguments.prepare == arguments.finish:
        sys.exit("say --prepare or --finish")
    if arguments.prepare:
        if not arguments.commit:
            sys.exit("--prepare needs --commit")
        prepare(arguments)
    else:
        if not arguments.corpus:
            sys.exit("--finish needs --corpus")
        finish(arguments)


if __name__ == "__main__":
    main()
