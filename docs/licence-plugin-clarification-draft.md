# Plugin clarification: a draft for the copyright holder to decide

Status: **variant A ADOPTED 2026-10-15** by the copyright holder, John Dorsey; the text is in `LICENSE-PLUGIN-CLARIFICATION`. Written 2026-10-15 for phase 0.2 of `plugin-system-plan-of-action.md` (features list, LG-04, Q-34). **This is a technical
draft, not legal advice.** Only the copyright holder can adopt it, and it can only be made for work that the copyright holder owns (every contribution by someone else needs that
person's agreement). It is easy while the program has one author and hard later.

## What the question is

fastforwardrs is `GPL-3.0-or-later`. It will load things that other people wrote, under other licences, at run time: sound plugins in LADSPA and CLAP format (binary
libraries, some GPL, many not), ISF shaders and `.cube` tables (data files, which are not programs in the usual sense). Whether a plugin that is loaded into a GPL program is a "work
based on" it is a debated case. Ardour, the best-known GPL program in this position, settled it for its users with a written **"Plugin Clarification"** that says the GPL does not reach
plugins that talk to it only through a published plugin interface. The same sentence protects the authors of plugins from a doubt that would stop them from offering their work.

## The draft (variant A: a clarification, as Ardour did)

> **Plugin clarification.** fastforwardrs is licensed under the GNU General Public License, version 3 or (at your option) any later version. The copyright holder(s)
> of fastforwardrs state that a program or file (a "plugin") which fastforwardrs loads at run time, and which works with fastforwardrs only through an interface that is
> published for plugins (at the time of writing: LADSPA, CLAP, ISF and `.cube`), is **not**, merely because fastforwardrs loads it and calls it, a work based on fastforwardrs, and
> need not be licensed under the GPL: its authors may license it as they choose, free or not. This clarification does not apply to a plugin that contains code copied from
> fastforwardrs, that is linked into fastforwardrs's own executable, or that uses fastforwardrs's internal (unpublished) interfaces; and it gives no permission to
> copy, modify or distribute fastforwardrs itself on any terms other than those of the GPL. A project file that contains a plugin is a collection of separate works, each under its own licence.

## Variant B: a formal additional permission

GPLv3 section 7 lets the copyright holder add **additional permissions** to the licence itself, which is firmer than a statement of how the holder reads the GPL. It would be a
file next to `LICENSE` (for example `LICENSE-ADDITIONAL-PERMISSION`) with the same meaning in the licence's own words ("As a special exception ... you may convey a combination of
fastforwardrs with a plugin ... without applying the GPL to the plugin ..."). Its weakness is that it has to be worded to the standard of a licence, and a further permission can
be removed by anyone who conveys the program (section 7), so a lawyer should read it. **My recommendation: variant A now** (it is what Ardour does, it is one paragraph, and it
asks nothing of anyone), and a lawyer's look before the first release that other people use.

## Things the draft deliberately does and does not say

* It names the formats by example ("at the time of writing"), so a later format (a node or a script) is not excluded or silently included: the phrase that decides is "a
  published interface for plugins".
* It protects plugins that **fastforwardrs loads**. It says nothing about fastforwardrs loading a *GPL-incompatible* plugin as a combined work *distributed by us* (we do not ship any
  such plugin: features list, LG-02).
* It says nothing about ffmpeg, which runs as a separate program (`ffmpeg` and `ffprobe` as child processes) and is under its own licence (the GPL build we use is GPLv3 or later; the
  corresponding source is offered: LG-03).
* It is separate from the question of **who the copyright holder is**: the files should say (phase 0.2, `SPDX-FileCopyrightText`).

## What happens next

1. The maintainer chooses the variant and edits the words (or says "adopt A as written").
2. I put the adopted text in `LICENSE-PLUGIN-CLARIFICATION` (or in the README and `LICENSE` header, as the maintainer prefers), refer to it from `Cargo.toml` (`license-file` is not
   used; the SPDX expression stays `GPL-3.0-or-later`), the README, and the about text.
3. It is dated in the decision log of `plugin-system-features.md`.
