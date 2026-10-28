# Engine phase 13: MIDI clips and synthesizer effects (design, draft 1, 2026-10-28)

*Draft written before any code. Labels: [V] = I looked at the code or ran it; [B] = I believe it from reading or from memory of a standard; **Q1..Q4** = questions only the maintainer can answer (asked in the chat), each with the answer I would choose. Nothing in this file is built.*

## 1. What the maintainer decided

* A **MIDI clip** feeds **synthesizer effects**.
* **No automatic pass-through:** a synthesizer is the **sole consumer** of the notes that reach it, unless the person says otherwise (by default it consumes them).

## 2. What exists today [V]

* **No MIDI anywhere in the program.** `ClipKind` has Video, Audio, Image, Text, Crate, Adjustment, Generator (`src/project/tracks/clip.rs`). The word "MIDI" appears only in the LV2 description reader (a port that says it supports `midi:MidiEvent` is read and then ignored) and in notes that say "not done".
* **The CLAP host sends parameter events and nothing else** (`crates/clap-host/src/instance.rs`: `input_events` gets only `ParamValueEvent`s; the scan reads audio ports and not note ports).
* **The LV2 host gives an atom port an empty `atom:Sequence`** (`crates/lv2-host/src/instance.rs`), so it can be filled with events later; the buffer layout code exists.
* **The scan refuses every plugin with no audio input** with "a generator or an instrument is not used yet" (`src/plugin_scan/effects.rs`, three places: LADSPA, CLAP, LV2). So no synthesizer is an effect today.
* **LADSPA has no notes at all** [B]: it stays effects-only for ever (DSSI was its instrument cousin; not planned).
* **Real synthesizers are already fetched by the Bundles tab** (per `docs/bundleable-clap-plugins.md`): Kars and Nekobi (DISTRHO), Surge XT (a CLAP in a zip that also has the effects one). `clap-test-plugins` has a test instrument (no audio input, a note port); I did not check that it makes sound from a note, and I expect it needs to be given that.
* **A text clip is "a clip whose content is the effect `builtin:text`"** (phase 9). A MIDI clip can be built the same way round, or as a clip with its own content; see 4.
* **The sound engine already has what a synthesizer needs around it**: one instance of a track's effects per run (the bus), tails (a synthesizer's release rings after the last note off, like a reverb), the lead-in (a seek begins up to 10 s early, unheard), the edge fade, windows. Notes have to travel the same paths as the sound does.

## 3. The model, in one paragraph

A MIDI clip has **notes** (and, later, controllers) and **makes no sound itself**. Its notes flow along the same road that sound flows along: **the clip's own effects, then its track's effects, then the effects of the clip groups it is in, then those of its track groups** (the order of `SoundClip::sent_to`). At each synthesizer on that road the notes are **consumed**: the synthesizer plays them and **the notes go no further**. A synthesizer **makes audio**, and that audio continues along the road as sound, through the effects after it. If no synthesizer is on the road, the notes are dropped and the clip is silent (and the clip is drawn so that this is visible: Q2).

## 4. Pieces (each is built, verified and committed before the next)

**13.0 Lock what is.** Tests that pin the scan's refusals, the sound plan's shape for a track with effects, and the file format, *before* anything changes (as for phases 9 and 12).

**13.1 Host: notes into plugins.**
* *CLAP*: read the note ports (`clap.note-ports`: how many, which dialects, which is preferred) in the scan, send `NOTE_ON`/`NOTE_OFF`/`NOTE_END` events with a sample offset inside the block, and (if the plugin takes the MIDI dialect) controllers and pitch bend as MIDI events. Choke and all-notes-off when a window ends. A note id for each note, so that releases are matched.
* *LV2*: fill the atom input port with MIDI events (`atom:Sequence`, time in frames, `midi:MidiEvent`); ports that are not MIDI stay as they were.
* The test instrument makes a tone from a note, so the host is tested without a real synthesizer; real ones (Kars, Nekobi, Surge XT) are tried after.

**13.2 The synthesizer as an effect.**
* The scan stops refusing a plugin with **no audio input and a note input**: it becomes an `EffectKind` of the sound domain with a flag *makes sound from notes*. (A plugin that has both an audio input and a note input, such as a vocoder, takes its audio from the road as an effect does and its notes as a synthesizer does: no special case.)
* It sits in a chain like any sound effect and has its values, keyframes, presets and window like any plugin (the work of phases 7, 10, 11 does not change).
* **Audio that reaches a synthesizer on the road passes by it** and the synthesizer's own audio is added to it (Q1).
* The sharing setting of the track (phase 12) decides how many instances there are: one for each clip, one for a run of clips, one for the whole track: so a synthesizer on a track has one set of voices for all its MIDI clips, and a synthesizer on a clip has its own.

**13.3 The MIDI clip in the project.**
* New `ClipKind::Midi`; file format 21; undo and redo like other clips; trim, split, move, duplicate, speed, mute work as for a sound clip.
* Content: the notes, in **seconds of the clip's own time** (the tempo map of the file is applied at import; the project has no tempo or bars, Q4), with pitch, velocity, channel; controllers and pitch bend in a second list.
* **Speed changes the timing and not the pitch** (the program's own idea: shrink when sped up).
* **Edges:** a note that began before the clip's first frame is not played; a note that is still held at the clip's last frame gets its note off there (the synthesizer's release rings on as a tail, under the tail policies of phase 12).
* **Mute** of a MIDI clip stops its notes (note offs for what was held). Volume of the clip is *not* velocity: it is the volume of what the clip's own effects make, which is nothing if the synthesizer is on the track (stated in the menu text).

**13.4 The plan and the engine.**
* `composition::sound`: a MIDI clip makes a **`NoteClip`** item with the notes and the same `sent_to` and tail routes as a `SoundClip`; `SoundPlan` finds, for each synthesizer, the notes that reach it (the walk of section 3), and the plan is hashed so that moving a note re-makes the sound.
* `sound_engine`: each instance of a synthesizer gets, in each block, the notes of its clips with sample offsets (sample-exact, not frame-exact); notes are consumed there (not sent on) unless the setting of 13.5 says otherwise. **A seek** runs the lead-in as for sound, and a note held longer than the lead-in is *chased*: it is started at the seek (Q3 asks nothing, I decided this; say so if wrong).
* The ffmpeg oracle of the export tests cannot play a synthesizer: the export is compared with the preview, and a null test uses the test instrument, whose tone is exact arithmetic.

**13.5 The one thing that is not the default: letting notes pass.**
* How the person turns it on is **Q3**. Whatever it is, it is stored with the effect (a saved project keeps it), is undoable, and is shown on the effect's box.

**13.6 What the person sees and does.**
* A MIDI clip on the timeline shows a small **piano roll** of its notes (read-only), a title, and a marker when no synthesizer is on its road.
* The **Plugin manager** lists synthesizers as sound effects, in a group of their own.
* **Where the notes come from is Q4**: importing `.mid` files (small; `midly` is `Unlicense`, which `deny.toml` allows [V by `cargo info`], or a reader of our own: the format is simple) is in; a **note editor** is a large piece on its own.

**13.7 Real use.** Kars, Nekobi and Surge XT played from a `.mid` file, listened to, a window opened, presets; the report `docs/engine-phase-13-report.md`.

## 5. Risks, in the order I would worry

1. **Sample-exact events with the engine's blocks and latency compensation** [B]: the engine delays members to line up effects with latency; notes have to be shifted the same way, or a synthesizer is early.
2. **Tails and chokes at the edges of clips, groups and windows**: a note that is held while a group's window closes. The phase 12 machinery covers the audio; the notes need their own note offs.
3. **A plugin with a window that plays by itself** (an arpeggiator in a synthesizer) is fine; one that *sends* notes out is not supported (output events are ignored, as now).
4. **Disk and CI**: the real synthesizers are large sets; the tests use the test instrument, the real ones go in the integration tests that are `#[ignore]`d without the bundled plugins, as for the effects.

## 6. Left out (named so that it is a choice and not an accident)

MIDI **effects** (arpeggiators, chord makers, transposers that take notes and give notes: the road above has room for them, as a note-in note-out kind of effect that does *not* consume) · MPE and per-note expression · MIDI 2.0 · recording or live playing from a keyboard · a project tempo, a bar grid, quantise · LADSPA/DSSI instruments · a synthesizer that sends notes out · sidechain notes · sampler instruments that keep files in their state (refused already for LV2).

## 7. The maintainer's answers (2026-10-28)

* **Q1** Audio that reaches a synthesizer **passes by it, and the synthesizer's sound is added**.
* **Q2** Notes find a synthesizer **by position, the first one on the road wins**; a MIDI clip with none on its road is silent and carries a marker.
* **Q3** A synthesizer lets notes pass on by **a switch on the synthesizer** (off by default; saved, undoable, shown on its box). No separate "pass notes" effect.
* **Q4** Notes come from **`.mid` import only** in this phase; each MIDI track of a file becomes a clip on its own timeline track, drawn as a read-only piano roll. The note editor is a later phase of its own.

Decided by me and not asked (say so if one is wrong): the notes are stored in seconds with the file's tempo map applied (no project tempo); speed changes timing and not pitch; a note begun before a clip's first frame is not played and one held at its end gets a note off there; a held note longer than the 10 s lead-in is chased at a seek; MIDI effects (note in, note out) are left out, and the road has room for them.
