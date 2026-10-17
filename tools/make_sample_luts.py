# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""Makes the five sample look-up tables in assets/luts/ (phase 4, piece 4.6).

Each is a 33-point 3-D table made by a formula written here, and nothing else: no other person's table, grade or film stock was looked at, measured or copied. The tables
are dedicated to the public domain (CC0 1.0); the script that makes them is the program's (GPL-3.0-or-later). The numbers are for the codes of a screen (sRGB), which
is what the look-up table effect takes by default.

    python tools/make_sample_luts.py            writes the five files
    python tools/make_sample_luts.py --check    says whether the files that are there are the ones this script makes (for the tests and CI)
"""

import sys
from pathlib import Path

SIZE = 33
FOLDER = Path(__file__).resolve().parent.parent / "assets" / "luts"
HEADER = """# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: CC0-1.0
#
# {description}
# Made by a formula in tools/make_sample_luts.py; nothing of anyone else's. For the codes of a screen (sRGB), in and out.
TITLE "{title}"
LUT_3D_SIZE {size}
"""
LUMA = (0.2126, 0.7152, 0.0722)


def clamp(x):
    return min(1.0, max(0.0, x))


def luma(r, g, b):
    return LUMA[0] * r + LUMA[1] * g + LUMA[2] * b


def saturate(rgb, amount):
    y = luma(*rgb)
    return tuple(y + (c - y) * amount for c in rgb)


def s_curve(x, strength):
    """Moves the middle apart without moving black or white: strength 0 is the identity."""
    smooth = x * x * (3.0 - 2.0 * x)
    return x + (smooth - x) * strength


def warm(r, g, b):
    r, g, b = (s_curve(c, 0.25) for c in (r, g, b))
    return clamp(r * 1.04 + 0.01), clamp(g * 1.0), clamp(b * 0.92)


def cool(r, g, b):
    r, g, b = (s_curve(c, 0.2) for c in (r, g, b))
    return clamp(r * 0.93), clamp(g * 0.99 + 0.005), clamp(b * 1.05 + 0.01)


def faded(r, g, b):
    r, g, b = saturate((r, g, b), 0.7)
    return tuple(clamp(0.06 + c * 0.88) for c in (r, g, b))


def punchy(r, g, b):
    r, g, b = saturate((s_curve(r, 0.6), s_curve(g, 0.6), s_curve(b, 0.6)), 1.25)
    return clamp(r), clamp(g), clamp(b)


def monochrome(r, g, b):
    y = s_curve(luma(r, g, b), 0.3)
    return y, y, y


LOOKS = [
    ("warm", "Warm", "A gentle warm look: a little more red, a little less blue, and a soft curve.", warm),
    ("cool", "Cool", "A gentle cool look: a little less red, a little more blue, and a soft curve.", cool),
    ("faded", "Faded", "A faded look: lifted blacks, softened whites and less colour.", faded),
    ("punchy", "Punchy", "A punchy look: a strong curve and more colour.", punchy),
    ("monochrome", "Monochrome", "Black and white by the brightness of each colour, with a soft curve.", monochrome),
]


def text_of(title, description, make):
    lines = [HEADER.format(title=title, description=description, size=SIZE)]
    top = SIZE - 1
    for blue in range(SIZE):
        for green in range(SIZE):
            for red in range(SIZE):
                r, g, b = make(red / top, green / top, blue / top)
                lines.append(f"{r:.6f} {g:.6f} {b:.6f}\n")
    return "".join(lines)


def main():
    check = "--check" in sys.argv[1:]
    FOLDER.mkdir(parents=True, exist_ok=True)
    different = []
    for file_name, title, description, make in LOOKS:
        path = FOLDER / f"{file_name}.cube"
        text = text_of(title, description, make)
        if check:
            if not path.exists() or path.read_text(encoding="utf-8").replace("\r\n", "\n") != text:
                different.append(path.name)
        else:
            path.write_text(text, encoding="utf-8", newline="\n")
            print("wrote", path)
    if check and different:
        print("not what the script makes:", ", ".join(different))
        sys.exit(1)


main()
