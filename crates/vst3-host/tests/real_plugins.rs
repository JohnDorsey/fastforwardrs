// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Real VST3 plugins, through this host: the one thing that shows that the host keeps the real ABI and not only what the test plugins do (a test plugin written with the same
//! bindings can agree with the host and both be wrong). **Ignored**: it needs a folder with real `.vst3` modules, which CI does not have (`docs/engine-phase-15-vst3-design.md`,
//! section 6). Point `FFRS_VST3_MODULES` at a folder; every `.vst3` folder below it is scanned, and each plugin is made and played:
//!
//! * an effect is given a sine and must give sound back that is made of numbers,
//! * an instrument is given a note and must make sound from it, and the same after a state that it saved itself is given back.
//!
//! `cargo test -p vst3-host --test real_plugins -- --ignored --nocapture`

use std::path::{Path, PathBuf};

use vst3_host::{ClassFacts, ClassId, Instance, NoteEvent, NoteEventKind, scan_module};

fn modules_below(folder: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    entries.sort();
    for path in entries {
        if path.extension().is_some_and(|extension| extension == "vst3") {
            found.push(path);
        } else if path.is_dir() {
            modules_below(&path, found);
        }
    }
}

fn is_an_instrument(facts: &ClassFacts) -> bool {
    facts.main_audio_input().is_none() && facts.takes_events()
}

fn peak(buffers: &[Vec<f32>]) -> f32 {
    buffers
        .iter()
        .flatten()
        .fold(0.0, |peak: f32, sample| peak.max(sample.abs()))
}

/// Plays `blocks` blocks of 512 frames; an instrument gets a note on at the start, an effect a sine. Gives the peak of the sound.
fn play(instance: &mut Instance, instrument: bool, blocks: usize) -> f32 {
    let (inputs, outputs) = instance.channels();
    let mut heard = 0.0_f32;
    for block in 0..blocks {
        let mut input: Vec<Vec<f32>> = (0..inputs)
            .map(|_| {
                (0..512)
                    .map(|place| (((block * 512 + place) as f32) * 0.05).sin() * 0.5)
                    .collect()
            })
            .collect();
        let mut output = vec![vec![0.0_f32; 512]; outputs];
        let mut input_slices: Vec<&mut [f32]> = input.iter_mut().map(Vec::as_mut_slice).collect();
        let mut output_slices: Vec<&mut [f32]> = output.iter_mut().map(Vec::as_mut_slice).collect();
        let notes = if instrument && block == 0 {
            vec![NoteEvent {
                time: 0,
                what: NoteEventKind::On {
                    channel: 0,
                    key: 60,
                    velocity: 0.8,
                    id: 1,
                },
            }]
        } else {
            Vec::new()
        };
        instance
            .run_with_notes(&mut input_slices, &mut output_slices, 512, &notes)
            .expect("a block is run");
        assert!(
            output.iter().flatten().all(|sample| sample.is_finite()),
            "the sound is made of numbers"
        );
        heard = heard.max(peak(&output));
    }
    heard
}

#[test]
#[ignore = "needs real VST3 modules (FFRS_VST3_MODULES)"]
fn real_plugins_are_found_made_and_heard_through_this_host() {
    let folder =
        PathBuf::from(std::env::var("FFRS_VST3_MODULES").expect("FFRS_VST3_MODULES names a folder with .vst3 modules"));
    let mut modules = Vec::new();
    modules_below(&folder, &mut modules);
    assert!(!modules.is_empty(), "no .vst3 module below {}", folder.display());
    let mut played = 0;
    for path in modules {
        let scan = scan_module(&path, &mut |step| println!("  {step}")).expect("the module is scanned");
        for scanned in scan.classes {
            println!(
                "{}: {:?}, problem {:?}",
                scanned.class.name, scanned.class.sub_categories, scanned.problem
            );
            let Some(facts) = scanned.facts else {
                continue;
            };
            if scanned.problem.is_some() {
                continue;
            }
            let module = vst3_host::Module::open(&path).expect("the module opens");
            let (mut instance, _) =
                Instance::new(&module, scanned.class.id, 48_000.0, 512, None).expect("the plugin is made");
            let instrument = is_an_instrument(&facts);
            let heard = play(&mut instance, instrument, 40);
            println!(
                "  {} parameters, {} in {} out, latency {}, tail {}; peak {heard:.3}",
                facts.parameters.len(),
                instance.channels().0,
                instance.channels().1,
                facts.latency,
                facts.tail
            );
            assert!(heard > 0.001, "{} gave no sound", scanned.class.name);
            // The state it gives is the state it takes: a new copy made from it sounds too.
            if scanned.has_state {
                let state = instance.save_state().expect("the state is saved");
                let (mut again, note) = Instance::new(&module, scanned.class.id, 48_000.0, 512, Some(&state))
                    .expect("the plugin is made with its state");
                assert_eq!(note, None, "the state was taken");
                assert!(
                    play(&mut again, instrument, 40) > 0.001,
                    "{} gave no sound after its state",
                    scanned.class.name
                );
            }
            // A value of a parameter that it has, given at a sample, does not hurt.
            if let Some(parameter) = facts
                .parameters
                .iter()
                .find(|parameter| parameter.can_automate && !parameter.is_read_only)
            {
                instance.set_value(parameter.id, parameter.plain_maximum, 100);
                play(&mut instance, instrument, 4);
            }
            played += 1;
            let _ = ClassId::from_text(&scanned.class.id.text()).expect("the id is written and read");
        }
    }
    assert!(played > 0, "no plugin was played");
    println!("played {played} plugins");
}
