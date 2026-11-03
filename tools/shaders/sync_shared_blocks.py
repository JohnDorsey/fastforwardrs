# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""Keeps the blocks of code that several of the program's own shaders share the same in all of them.

An ISF file is one file with no way to include another, so the code that many shaders need (the palettes, the noise) is **copied** into each of them. To keep a copy from
drifting, the one true text of each block is a file in `assets/shaders/shared/<name>.glsl` (two lines of SPDX, then the code), and a shader has its copy between two lines:

    // --- shared: <name> (begin: a copy of assets/shaders/shared/<name>.glsl, changed only by tools/shaders/sync_shared_blocks.py) ---
    ...
    // --- shared: <name> (end) ---

    py tools/shaders/sync_shared_blocks.py            puts the true text in every copy (and says which files it changed)
    py tools/shaders/sync_shared_blocks.py --check    changes nothing, and fails if a copy is not the true text (a test of the program, `shaders_of_the_program`, checks the same)
    py tools/shaders/sync_shared_blocks.py --add NAME FILE...   puts the block NAME into the files, after their header (before the first line of code), where they have no copy yet
"""

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SHADERS = ROOT / "assets" / "shaders"
SHARED = SHADERS / "shared"
BEGIN = "// --- shared: {name} (begin: a copy of assets/shaders/shared/{name}.glsl, changed only by tools/shaders/sync_shared_blocks.py) ---"
END = "// --- shared: {name} (end) ---"


def true_text_of(name: str) -> str:
    """The code of the block, without the two lines of SPDX and the blank line after them, and with one line end at the end."""
    text = (SHARED / f"{name}.glsl").read_text(encoding="utf-8").replace("\r\n", "\n")
    lines = text.split("\n")
    assert lines[0].startswith("// SPDX-FileCopyrightText") and lines[1].startswith("// SPDX-License-Identifier"), name
    return "\n".join(lines[2:]).strip("\n") + "\n"


def blocks_in(text: str):
    """(name, start of the begin line, end of the end line, code) of every block of `text`."""
    found = []
    at = 0
    while True:
        begin = text.find("// --- shared: ", at)
        if begin < 0:
            return found
        name = text[begin + len("// --- shared: "):].split(" ", 1)[0]
        begin_line_end = text.index("\n", begin)
        end_line = END.format(name=name)
        end = text.index(end_line, begin_line_end)
        found.append((name, begin, end + len(end_line), text[begin_line_end + 1:end]))
        at = end + len(end_line)


def synced(text: str) -> str:
    for name, begin, end, _code in reversed(blocks_in(text)):
        block = BEGIN.format(name=name) + "\n" + true_text_of(name) + END.format(name=name)
        text = text[:begin] + block + text[end:]
    return text


def main(arguments) -> int:
    files = sorted(SHADERS.glob("*/*.fs"))
    if arguments[:1] == ["--add"]:
        name = arguments[1]
        for file in arguments[2:]:
            path = Path(file)
            text = path.read_text(encoding="utf-8").replace("\r\n", "\n")
            if f"// --- shared: {name} " in text:
                continue
            # (After the header of the file: the first line after the closing of the JSON comment.)
            header_end = text.index("}*/") + len("}*/")
            line_end = text.index("\n", header_end)
            block = BEGIN.format(name=name) + "\n" + true_text_of(name) + END.format(name=name)
            text = text[:line_end + 1] + "\n" + block + "\n" + text[line_end + 1:]
            path.write_text(text, encoding="utf-8", newline="\n")
            print("added", name, "to", path.name)
        return 0
    check = arguments[:1] == ["--check"]
    wrong = []
    for path in files:
        text = path.read_bytes().decode("utf-8").replace("\r\n", "\n")
        new = synced(text)
        if new != text:
            wrong.append(path.name)
            if not check:
                path.write_text(new, encoding="utf-8", newline="\n")
    for name in wrong:
        print(("differs: " if check else "changed: ") + name)
    return 1 if (wrong and check) else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
