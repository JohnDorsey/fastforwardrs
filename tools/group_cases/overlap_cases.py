# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Usage: python -I tools/group_cases/overlap_cases.py TRACKS FRAMES MAX_GROUPS TAIL   (the design docs/engine-phase-12-design.md, section 4, used 2 5 2 2 and 3 4 3 2)
"""Throwaway: enumerate every small timeline layout the group rules allow, and classify every place where a
signal with memory (a tail, or the state of a shared track effect) can pass from one group context into another.

Model (from src/project/groups/rules.rs, read 2026-10-27):
  * a clip is one cell (track, frame); clips never overlap
  * a clip group = a set of clips (and clip groups inside it) whose TIGHT bounding rectangle holds no clip that is not in it
  * two clip groups are nested or share no clip (a clip is directly in at most one shape); their rectangles may overlap in empty space
  * a tail of length L cells follows each clip on its own track
"""
import itertools
import sys
from collections import Counter, defaultdict

TRACKS, FRAMES, MAX_GROUPS, TAIL = (int(a) for a in sys.argv[1:5])
CELLS = [(t, x) for t in range(TRACKS) for x in range(FRAMES)]


def bbox(cells):
    ts = [c[0] for c in cells]
    xs = [c[1] for c in cells]
    return (min(ts), max(ts), min(xs), max(xs))


def inside(box, cell):
    return box[0] <= cell[0] <= box[1] and box[2] <= cell[1] <= box[3]


def convex_sets(clips):
    out = []
    for n in range(2, len(clips) + 1):
        for subset in itertools.combinations(clips, n):
            box = bbox(subset)
            if all(c in subset or not inside(box, c) for c in clips):
                out.append(frozenset(subset))
    return out


def laminar_families(sets, k):
    fams = [()]
    for n in range(1, k + 1):
        for fam in itertools.combinations(sets, n):
            ok = True
            for a, b in itertools.combinations(fam, 2):
                if (a & b) and not (a <= b or b <= a):
                    ok = False
                    break
                if a == b:
                    ok = False
                    break
            if ok:
                fams.append(fam)
    return fams


def classify(clips, fam, a, point):
    """A signal made by clip `a` is present at cell `point` (the same track, later). Say what it passes through."""
    boxes = {g: bbox(g) for g in fam}
    anc = [g for g in fam if a in g]
    here = [g for g in fam if inside(boxes[g], point)]
    exited = [g for g in anc if g not in here]
    entered = [g for g in here if g not in anc]
    # entered groups that are not nested in one another = a place that two rectangles claim
    incomparable = any(not (g <= h or h <= g) for g, h in itertools.combinations(entered, 2))
    on_clip = point in clips
    e = "enters" + str(len(entered)) + ("*" if incomparable else "")
    x = "exits" + str(len(exited))
    # picture: the tail is a layer at the z of its track; it interleaves a group's run if its track is in the group's tracks
    interleaves = any(boxes[g][0] <= a[0] <= boxes[g][1] for g in entered)
    return (x, e, "on a clip" if on_clip else "in a gap", "interleaves" if interleaves else "clear of z-run")


def main():
    table = Counter()
    example = {}
    layouts = 0
    for mask in range(1, 1 << len(CELLS)):
        clips = [CELLS[i] for i in range(len(CELLS)) if mask >> i & 1]
        cs = frozenset(clips)
        fams = laminar_families(convex_sets(clips), MAX_GROUPS)
        for fam in fams:
            layouts += 1
            for a in clips:
                for d in range(1, TAIL + 1):
                    point = (a[0], a[1] + d)
                    if point[1] >= FRAMES:
                        continue
                    key = classify(cs, fam, a, point)
                    table[key] += 1
                    example.setdefault(key, (sorted(clips), [sorted(g) for g in fam], a, point))
    print("layouts enumerated:", layouts)
    print("situations classified:", sum(table.values()), "into", len(table), "classes")
    for key, n in sorted(table.items()):
        print(f"{n:>9}  {key}")
    print()
    for key in sorted(example):
        print(key, "e.g.", example[key])


main()
