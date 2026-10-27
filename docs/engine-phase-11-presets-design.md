<!--
SPDX-FileCopyrightText: 2026 John Dorsey
SPDX-License-Identifier: GPL-3.0-or-later
-->

# Presets of LV2 plugins: design (piece 11.7, 2026-10-27)

The maintainer said (2026-10-27): "Then we can try those and you can work on presets." The plugins are in the *Bundles* tab now (`bundleable-lv2-plugins.md`); this is the design of the presets.
**[V]** = I looked at it or ran it; **[P]** = a plan, not built; **[B]** = belief.

## 1. What a preset is in LV2, and what the real plugins have [V]

LV2 presets (`http://lv2plug.in/ns/ext/presets#`) are plain Turtle. A preset is a node `a pset:Preset` with `lv2:appliesTo <plugin URI>` and an `rdfs:label`, **in the `manifest.ttl` of a bundle**,
with the values in a file the node names by `rdfs:seeAlso`. The values are `lv2:port [ lv2:symbol "gain" ; pset:value 0.5 ]` (a control port by its **symbol**, a number), and, for a plugin
with state, `state:state [ ... ]` (atoms, which this host cannot make). A preset may be in the bundle of its plugin or in a **bundle of its own that has no plugin** (that is how people install
presets: `~/.lv2/some-presets.lv2` or `%APPDATA%\LV2\...`).

What the bundled plugins have, read from the fetched files today:

* **DPF plugins** (MVerb, 3BandEQ, PingPongPan, CycleShifter, and Zam, which is not bundled): `manifest.ttl` has the nodes with the labels (MVerb: "Halves", "Dark", ...), `presets.ttl` has
  the values as `lv2:port` blank nodes with **integer literals** (`pset:value 50`, not `50.0`). Only control values. The values of a preset are of **every** control input of the plugin.
* **Dragonfly Reverb** has **no `presets.ttl`**: its rooms and halls are a list in its own window (DPF "programs" are not LV2 presets), so a person gets them by opening the window.
* **Wolf Shaper** has none either.

So with the current set a person gets presets for MVerb, 3BandEQ, PingPongPan and CycleShifter. That is little, and it is honest: presets are what plugin makers ship, and most of the
big ones ship them in their own window.

## 2. Decisions [P]

1. **Read presets from every scanned bundle and tie them to plugins by URI**, so that a bundle of presets that has no plugin works. (The scan of a bundle says its presets, each with its
   `appliesTo` URI; the list of effects is made from all the scans, and gives each plugin the presets that name its URI, in the order of the folders, then of the bundle, then of the label.)
2. **A preset is values.** `lv2:port` entries are converted to the values of the effect **in the way the effect's own controls are made** (a toggle is a flag; a choice is the option nearest to
   the number; a whole number is rounded; a range that is a fraction of the sample rate is multiplied by it; a value is clamped to the range the effect says). A symbol that the plugin has no
   port for, or a port that is not one of the effect's values (an output, or one the host decides, or past the 64 values an effect may have), is **left out and counted**.
3. **A preset with `state:state` and no ports** is not offered (it would need atoms); a preset with both is offered and its state is ignored (**said in the menu**: "values only").
4. **Applying a preset is one step of undo** that sets the values of the effect at once. A value that has **keyframes** is **left as it is** (the animation of a person is not overwritten by a
   click in a menu; the menu says how many values were kept). A value that has none gets the preset's number. A window of the plugin that is open is told the new values (it is already: the
   editor tells the window what the effect's values are).
5. **Where it is in the program:** the **menu of the box of the effect** (the one with *Open editor*, *Switch off*, *Duplicate*, *Delete*) gets a **Presets** submenu **only for an effect that has
   presets**: nothing is added that is always visible, and an effect without presets looks as it did. It shows the labels, the preset whose values are the effect's present values has a mark.
6. **Not in this piece:** saving a preset (a person's own), presets of CLAP (a different mechanism: the `preset-discovery` factory) and of LADSPA (none), `state:state`, `pset:bank`
   grouping (the labels are listed flat; a bank's label is put in front: `Bank: Label`), `pset:preview`.

## 3. Pieces [P]

* **11.7.1 host**: `lv2-host` reads `pset:Preset` nodes into `PresetDescription { uri, label, applies_to, values: Vec<(symbol, f64)>, has_state }` (`describe_bundle` and `scan_bundle` give
  them; `BundleScan.presets`, with a serde default so that old caches still read; the scan cache format goes up by one). Tests with the real `presets.ttl` of MVerb (a small copy as a
  fixture) and with hand-made Turtle: integer and float literals, a node with no label, no `appliesTo`, a symbol twice, a value that is not a number, `seeAlso` in another file, two
  bundles.
* **11.7.2 scan**: `InstalledPlugin.presets: Vec<EffectPreset { label, values: Vec<(value index, PropertyValue)>, left_out: usize, values_only: bool }>`, made in `plugins_found_in` from
  all the reports, with the conversion of decision 2 (the conversion is the same function that makes the descriptor, so a preset cannot disagree with the controls).
* **11.7.3 command**: `Command::ApplyEffectPreset { target, instance, preset }`; one step of undo; keyframed values are left; says how many.
* **11.7.4 panel**: the submenu in the box of the effect.
* **11.7.5 docs and a try with the real MVerb**: apply "Dark" and the sound changes as the preset says; the window of MVerb shows the new knobs.

## 4. What I do not know [B]

* Whether any other real bundle has presets in the shapes that I have not seen (a `pset:Preset` in the plugin's own Turtle, `state:state` only). The DPF ones are the only ones I have seen.
* Whether a person wants a keyframed value to be overwritten by a preset: the safe choice is made (it is kept), and the menu says so.
