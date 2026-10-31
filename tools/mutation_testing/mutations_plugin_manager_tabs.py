# SPDX-FileCopyrightText: 2026 John Dorsey
# SPDX-License-Identifier: GPL-3.0-or-later

"""Planted mistakes for the plugin manager's *Bundles* tab as it was made on 2026-10-31: a tab for each kind of download with `(n/m)` in its title, the button above the list, one
size of text, and quieter text that can still be read (4.5 to 1 against the background).

    py tools/mutation_testing/mutate.py tools/mutation_testing/mutations_plugin_manager_tabs.py --workers 2
"""

import os

TESTS = os.environ.get("MUTATION_TESTS", "--lib -- plugin_manager sound_plugins")

TEXT = "src/application/plugin_manager_text.rs"
BUNDLES = "src/application/plugin_manager_bundles.rs"
WINDOW = "src/application/plugin_manager_window.rs"
FLOW = "src/application/fast_forward_application/sound_plugins_flow.rs"

THE_ROWS_AND_THE_BUTTON = """    show_the_download(ui, view, commands);
    ui.add_space(6.0);
    for row in view.rows {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} {}", row.name, row.version)).strong());
                let note = if !row.has_build {
                    NO_BUILD_FOR_THIS_COMPUTER_NOTE
                } else if row.is_fetched {
                    FETCHED_NOTE
                } else {
                    NOT_FETCHED_NOTE
                };
                ui.label(quieter(ui, note));
            });
            ui.label(format!("{} ({})", row.contents, row.licence));
            ui.label(quieter(ui, format!("By {}. Source: {}", row.authors, row.source)));
        });
    }
}
"""

THE_BUTTON_UNDER_THE_ROWS = """    for row in view.rows {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} {}", row.name, row.version)).strong());
                let note = if !row.has_build {
                    NO_BUILD_FOR_THIS_COMPUTER_NOTE
                } else if row.is_fetched {
                    FETCHED_NOTE
                } else {
                    NOT_FETCHED_NOTE
                };
                ui.label(quieter(ui, note));
            });
            ui.label(format!("{} ({})", row.contents, row.licence));
            ui.label(quieter(ui, format!("By {}. Source: {}", row.authors, row.source)));
        });
    }
    ui.add_space(6.0);
    show_the_download(ui, view, commands);
}
"""

MUTATIONS = [
    # --- the colour of the quieter text ---
    ("contrast: 3 to 1 is readable enough (the least of the quieter text)", TEXT,
     "pub const LEAST_CONTRAST: f32 = 4.5;", "pub const LEAST_CONTRAST: f32 = 3.0;", TESTS),
    ("contrast: the red of a colour weighs more than it does", TEXT, "0.2126 * linear(colour.r())", "0.2526 * linear(colour.r())", TESTS),
    ("quieter: the search keeps the mixes that are too quiet", TEXT,
     "if contrast(mixed(text, background, middle), background) >= LEAST_CONTRAST {",
     "if contrast(mixed(text, background, middle), background) <= LEAST_CONTRAST {", TESTS),
    ("quieter: one step of the search and not sixteen", TEXT, "for _ in 0..16 {", "for _ in 0..1 {", TESTS),
    ("quieter: a mix goes away from the background and not towards it", TEXT,
     "(f32::from(b) - f32::from(a)) * towards_to", "(f32::from(a) - f32::from(b)) * towards_to", TESTS),
    # (Left out, because it is an EQUIVALENT mistake: made from egui's weak colour, `quieter_text_colour(visuals.weak_text_color(), ..)` gives the same colour. In egui the weak
    # colour is the colour of the text at 60 % alpha, and the first thing that `quieter_text_colour` does is `to_opaque()`, which gives the colour of the text back. Shown by
    # printing what `quieter()` makes from each: #838383 both ways, on 2026-10-31.)
    # --- the counts and the titles of the tabs ---
    ("count: the sets that have no build here are counted as available", BUNDLES,
     "let available = self.rows.iter().filter(|row| row.has_build);", "let available = self.rows.iter();", TESTS),
    ("count: the sets that are not fetched are the ones that are counted as fetched", BUNDLES,
     "available.clone().filter(|row| row.is_fetched).count(),", "available.clone().filter(|row| !row.is_fetched).count(),", TESTS),
    ("title: the two numbers are the other way round", BUNDLES,
     'format!("{} ({fetched}/{available})", self.kind.heading())', 'format!("{} ({available}/{fetched})", self.kind.heading())', TESTS),
    # --- which tab is shown, and which is chosen ---
    ("tabs: a tab that is chosen and not there shows nothing (and not the first)", BUNDLES,
     "views.iter().find(|view| view.kind == selected).or(views.first())", "views.iter().find(|view| view.kind == selected)", TESTS),
    ("tabs: a kind that has nothing to fetch has a tab, and one that has has none", WINDOW,
     ".filter(|view| !view.rows.is_empty()),", ".filter(|view| view.rows.is_empty()),", TESTS),
    ("tabs: the tab that was chosen is not the one that is shown", WINDOW,
     "show_bundle_tabs(ui, &views, view.bundle_kind, commands);", "show_bundle_tabs(ui, &views, BundleKind::default(), commands);", TESTS),
    ("tabs: the command that chooses a tab is forgotten", FLOW,
     "PluginManagerCommand::SelectBundleKind(kind) => self.interface.plugin_manager_bundle_kind = kind,",
     "PluginManagerCommand::SelectBundleKind(_) => {}", TESTS),
    ("tabs: the tab that is shown first is the shaders", BUNDLES,
     "    #[default]\n    SoundPlugins,\n    /// Patches, wavetables and presets that plugins find beside themselves.\n    PluginData,\n    Shaders,\n}",
     "    SoundPlugins,\n    /// Patches, wavetables and presets that plugins find beside themselves.\n    PluginData,\n    #[default]\n    Shaders,\n}", TESTS),
    # --- one size of text, and text that can be read ---
    ("text: the line under a heading is weak (3 to 1) again", BUNDLES,
     "ui.label(quieter(ui, view.kind.intro()));", "ui.label(RichText::new(view.kind.intro()).weak());", TESTS),
    ("text: the line under a heading is small again", BUNDLES,
     "ui.label(quieter(ui, view.kind.intro()));", "ui.label(RichText::new(view.kind.intro()).small());", TESTS),
    ("text: the line of the authors and the source is small and weak again", BUNDLES,
     'ui.label(quieter(ui, format!("By {}. Source: {}", row.authors, row.source)));',
     'ui.label(RichText::new(format!("By {}. Source: {}", row.authors, row.source)).small().weak());', TESTS),
    # --- the button above the list ---
    ("layout: the button (or the question, or the progress) is under the list", BUNDLES, THE_ROWS_AND_THE_BUTTON, THE_BUTTON_UNDER_THE_ROWS, TESTS),
]
