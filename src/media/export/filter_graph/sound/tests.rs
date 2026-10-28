// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn frame_rate_30() -> FrameRate {
    FrameRate::new(30, 1).unwrap()
}

fn speed(factor: u32) -> SpeedFactor {
    SpeedFactor::new(factor).unwrap()
}

/// The sound of source frames `first` up to `end` of the first input, at `factor`x, lasting as long as that does.
fn clip(first: u64, end: u64, factor: u32) -> SoundClipToExport {
    SoundClipToExport {
        source_input: 0,
        first_source_frame: first,
        end_source_frame: end,
        speed_factor: speed(factor),
        timeline_frame_count: (end - first).div_ceil(u64::from(factor)),
        loudness: LoudnessToExport::UNCHANGED,
        first_timeline_frame: 0,
        frames_of_the_clip_before: 0,
        effects: Vec::new(),
        sent_to: None,
        tail_routes: Vec::new(),
        music: None,
    }
}

fn clip_item(first: u64, end: u64, factor: u32) -> SoundItemToExport {
    SoundItemToExport::Clip(clip(first, end, factor))
}

fn silence(frame_count: u64) -> SoundItemToExport {
    SoundItemToExport::Silence { frame_count }
}

fn strip(items: Vec<SoundItemToExport>) -> SoundStripToExport {
    SoundStripToExport {
        items,
        buses: Vec::new(),
        loudness: Default::default(),
        muted: Vec::new(),
    }
}

fn specification(strips: Vec<SoundStripToExport>) -> SoundGraphSpecification {
    let total_frame_count = strips
        .iter()
        .map(SoundStripToExport::timeline_frame_count)
        .max()
        .unwrap_or(0);
    SoundGraphSpecification {
        frame_rate: frame_rate_30(),
        total_frame_count,
        strips,
        sped_up_clip_sound: SpedUpClipSound::MaintainPitch,
        lead_in_frames: 0,
        groups: Vec::new(),
        edge_fade_milliseconds: 10,
    }
}

const SAMPLE_FORMAT: &str = "aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo";

/// The statement that makes silence of `length` seconds for the `clip_number`th clip to be laid on.
fn silence_statement(clip_number: usize, length: &str) -> String {
    format!(
        "anullsrc=r=48000:cl=stereo,{SAMPLE_FORMAT},atrim=end={length},asetpts=PTS-STARTPTS[clip_silence_{clip_number}]"
    )
}

/// The statement that mixes the `clip_number`th clip, once it is cut, into its silence.
fn mixing_statement(clip_number: usize, item_label: &str) -> String {
    format!(
        "[clip_silence_{clip_number}][clip_cut_audio_{clip_number}]amix=inputs=2:duration=first:dropout_transition=0,volume=2,asetpts=PTS-STARTPTS[{item_label}]"
    )
}

#[test]
fn sound_is_cut_sped_up_and_laid_on_silence_of_the_exact_length_of_the_pictures() {
    let statements = sound_statements(&specification(vec![strip(vec![
        clip_item(0, 150, 1),
        clip_item(150, 330, 3),
    ])]));
    assert_eq!(
        statements,
        [
            format!("[0:a:0]{SAMPLE_FORMAT},asplit=2[clip_source_audio_0][clip_source_audio_1]"),
            // Normal speed: no atempo. 150 frames at 30 fps are exactly 5 s.
            "[clip_source_audio_0]atrim=start=0.000000000:end=5.000000000,asetpts=PTS-STARTPTS[clip_cut_audio_0]"
                .to_owned(),
            silence_statement(0, "5.000000000"),
            mixing_statement(0, "strip_0_item_0"),
            // 180 source frames at 3x are 60 frames = exactly 2 s.
            "[clip_source_audio_1]atrim=start=5.000000000:end=11.000000000,asetpts=PTS-STARTPTS,atempo=3[clip_cut_audio_1]"
                .to_owned(),
            silence_statement(1, "2.000000000"),
            mixing_statement(1, "strip_0_item_1"),
            "[strip_0_item_0][strip_0_item_1]concat=n=2:v=0:a=1[finished_audio]".to_owned(),
        ]
    );
}

#[test]
fn the_exact_length_of_sound_follows_the_rounded_up_frame_count_not_the_raw_duration() {
    // 10 frames at 4x show 3 pictures (0, 4, 8) = 0.1 s, though 10/4 frames would be 0.0833 s.
    let statements = sound_statements(&specification(vec![strip(vec![clip_item(0, 10, 4)])]));
    assert!(
        statements.contains(&silence_statement(0, "0.100000000")),
        "{statements:?}"
    );
}

/// Regression test: a clip's sound used to be padded with `apad`, which gives sound without
/// timestamps when the clip is wholly past the end of its file's sound, so the sound after it came
/// out in the wrong place. Padding is now done by laying the clip on silence of its own.
#[test]
fn a_clips_sound_is_laid_on_silence_and_not_padded_because_padding_nothing_gives_sound_without_timestamps() {
    let graph = sound_statements(&specification(vec![strip(vec![
        clip_item(0, 150, 1),
        clip_item(150, 330, 3),
    ])]))
    .join(";");
    assert!(!graph.contains("apad"), "{graph}");
    assert_eq!(
        graph.matches("amix=inputs=2:duration=first").count(),
        2,
        "one for each clip: {graph}"
    );
}

#[test]
fn a_source_with_no_clips_is_not_read_at_all_and_a_graph_with_no_strips_is_empty() {
    assert!(sound_statements(&specification(vec![])).is_empty());
}

fn sound_chain_of_only_clip(clip: SoundClipToExport, sound: SpedUpClipSound) -> String {
    let mut spec = specification(vec![strip(vec![SoundItemToExport::Clip(clip)])]);
    spec.sped_up_clip_sound = sound;
    sound_statements(&spec)
        .into_iter()
        .find(|statement| statement.contains("atrim=start"))
        .unwrap()
}

// --- what the sound of a sped-up clip is like ---

#[test]
fn muted_sped_up_clips_get_silence_of_the_same_length_and_the_others_keep_their_sound() {
    let strip_of = || strip(vec![clip_item(0, 150, 1), clip_item(150, 330, 3)]);
    let unmuted = sound_statements(&specification(vec![strip_of()]));
    let mut muting = specification(vec![strip_of()]);
    muting.sped_up_clip_sound = SpedUpClipSound::Mute;
    let muted = sound_statements(&muting);

    assert_eq!(
        muted[..4],
        unmuted[..4],
        "the source and the normal speed clip are as they were"
    );
    assert_eq!(
        muted[4],
        "[clip_source_audio_1]atrim=start=5.000000000:end=11.000000000,asetpts=PTS-STARTPTS,volume=0[clip_cut_audio_1]",
        "the sped up clip is silent"
    );
    assert_eq!(
        muted[5..],
        unmuted[5..],
        "and as long as its pictures, and they are joined as before"
    );
}

#[test]
fn what_sped_up_sound_is_like_changes_nothing_when_no_clip_is_sped_up() {
    let strips = || vec![strip(vec![clip_item(0, 150, 1), clip_item(150, 300, 1)])];
    let reference = sound_statements(&specification(strips()));
    for sound in SpedUpClipSound::ALL {
        let mut changed = specification(strips());
        changed.sped_up_clip_sound = sound;
        assert_eq!(sound_statements(&changed), reference, "{sound:?}");
    }
}

fn treatment(clip: &SoundClipToExport, sound: SpedUpClipSound) -> ClipSoundTreatment {
    treatment_of(frame_rate_30(), clip, sound)
}

#[test]
fn a_clip_at_normal_speed_has_its_sound_just_cut_out_whatever_the_setting_for_sped_up_clips_is() {
    for sound in SpedUpClipSound::ALL {
        assert_eq!(
            treatment(&clip(0, 300, 1), sound),
            ClipSoundTreatment::None,
            "{sound:?}"
        );
        assert_eq!(ClipSoundTreatment::None.filters(), "");
    }
}

#[test]
fn a_muted_sped_up_clip_gets_silence_and_a_pitch_keeping_one_gets_atempo_and_a_pitch_raising_one_gets_asetrate() {
    let sped_up = clip(0, 300, 4);
    assert_eq!(treatment(&sped_up, SpedUpClipSound::Mute), ClipSoundTreatment::Silence);
    assert_eq!(
        treatment(&sped_up, SpedUpClipSound::MaintainPitch),
        ClipSoundTreatment::FasterAtTheSamePitch {
            tempos: vec!["4".to_owned()]
        }
    );
    assert_eq!(
        treatment(&sped_up, SpedUpClipSound::IncreasePitch),
        ClipSoundTreatment::FasterWithHigherPitch { speed_factor: 4 }
    );
}

#[test]
fn raising_the_pitch_says_the_sound_has_the_sample_rate_times_the_speed_and_converts_back_to_the_fixed_rate() {
    assert_eq!(
        ClipSoundTreatment::FasterWithHigherPitch { speed_factor: 4 }.filters(),
        ",asetrate=192000,aresample=48000"
    );
    // At the largest speed that raises the pitch, the rate still fits ffmpeg's whole number.
    let at_the_limit = ClipSoundTreatment::FasterWithHigherPitch {
        speed_factor: LARGEST_SPEED_THAT_RAISES_THE_PITCH,
    }
    .filters();
    assert_eq!(at_the_limit, ",asetrate=48000000,aresample=48000");
    assert!(48_000_u64 * u64::from(LARGEST_SPEED_THAT_RAISES_THE_PITCH) < u64::from(i32::MAX as u32));
}

#[test]
fn a_clip_too_fast_for_its_raised_pitch_to_be_heard_is_silent_instead_of_breaking_asetrate() {
    let at_the_limit = clip(0, 100_000, LARGEST_SPEED_THAT_RAISES_THE_PITCH);
    let past_the_limit = clip(0, 100_000, LARGEST_SPEED_THAT_RAISES_THE_PITCH + 1);
    assert!(matches!(
        treatment(&at_the_limit, SpedUpClipSound::IncreasePitch),
        ClipSoundTreatment::FasterWithHigherPitch { .. }
    ));
    assert_eq!(
        treatment(&past_the_limit, SpedUpClipSound::IncreasePitch),
        ClipSoundTreatment::Silence
    );
    // Keeping the pitch has no such limit: atempo can be chained.
    assert!(matches!(
        treatment(&past_the_limit, SpedUpClipSound::MaintainPitch),
        ClipSoundTreatment::FasterAtTheSamePitch { .. }
    ));
}

/// Regression test: `atempo` produces no sound for a very short clip at a high speed, and joining a
/// clip like that to the others made ffmpeg fail ("Invalid data found when processing input"). Such
/// clips now get silence of the right length instead, whichever way the clip is sped up.
#[test]
fn a_clip_whose_sped_up_sound_would_be_a_blip_gets_silence_instead_of_atempo() {
    // 2 source frames (66 ms) at 33x: 2 ms of sound.
    let chain = sound_chain_of_only_clip(clip(0, 2, 33), SpedUpClipSound::MaintainPitch);
    assert!(chain.ends_with(",volume=0[clip_cut_audio_0]"), "{chain}");
    assert!(!chain.contains("atempo"), "{chain}");
    let graph = sound_statements(&specification(vec![strip(vec![clip_item(0, 2, 33)])]));
    assert!(
        graph.contains(&silence_statement(0, "0.033333333")),
        "and it is as long as its pictures: {graph:?}"
    );
    for sound in [SpedUpClipSound::MaintainPitch, SpedUpClipSound::IncreasePitch] {
        assert_eq!(
            treatment(&clip(0, 2, 33), sound),
            ClipSoundTreatment::Silence,
            "{sound:?}"
        );
        assert_ne!(
            treatment(&clip(0, 30, 8), sound),
            ClipSoundTreatment::Silence,
            "{sound:?}"
        );
    }
}

#[test]
fn a_clip_whose_sped_up_sound_is_long_enough_keeps_its_sound() {
    // 30 source frames (1 s) at 8x: 125 ms of sound, over the 100 ms limit.
    let chain = sound_chain_of_only_clip(clip(0, 30, 8), SpedUpClipSound::MaintainPitch);
    assert!(chain.ends_with(",atempo=8[clip_cut_audio_0]"), "{chain}");
    assert!(!chain.contains("volume=0"), "{chain}");
}

#[test]
fn the_silence_rule_switches_exactly_at_a_tenth_of_a_second_of_sped_up_sound() {
    let chain = |first, end, factor| sound_chain_of_only_clip(clip(first, end, factor), SpedUpClipSound::MaintainPitch);
    // 3 frames at 30 fps = 0.1 s of source. At 1x that is not accelerated, so it is never silenced.
    assert!(!chain(0, 3, 1).contains("volume=0"));
    // 6 source frames (0.2 s) at 2x is exactly 0.1 s of sound: not under the limit.
    assert!(chain(0, 6, 2).contains("atempo=2"));
    // 5 source frames (0.1667 s) at 2x is 0.0833 s: under the limit.
    assert!(chain(0, 5, 2).contains("volume=0"));
}

#[test]
fn normal_speed_clips_are_never_silenced_however_short() {
    let chain = sound_chain_of_only_clip(clip(100, 101, 1), SpedUpClipSound::MaintainPitch);
    assert!(!chain.contains("volume=0") && !chain.contains("atempo"), "{chain}");
}

#[test]
fn a_pitch_raising_graph_has_asetrate_in_the_sped_up_clips_chain_and_no_atempo() {
    let mut spec = specification(vec![strip(vec![clip_item(0, 150, 1), clip_item(150, 330, 3)])]);
    spec.sped_up_clip_sound = SpedUpClipSound::IncreasePitch;
    let statements = sound_statements(&spec);
    assert_eq!(
        statements[4],
        "[clip_source_audio_1]atrim=start=5.000000000:end=11.000000000,asetpts=PTS-STARTPTS,asetrate=144000,aresample=48000[clip_cut_audio_1]"
    );
    assert!(!statements.join(";").contains("atempo"));
}

#[test]
fn audio_tempo_filters_multiply_up_to_the_speed_factor() {
    for factor in [
        1u32,
        2,
        3,
        7,
        50,
        99,
        100,
        101,
        150,
        250,
        999,
        1000,
        10_000,
        12_345,
        1_000_000,
        4_000_000_000,
    ] {
        let tempos = audio_tempo_filters(speed(factor));
        let product: f64 = tempos.iter().map(|tempo| tempo.parse::<f64>().unwrap()).product();
        let relative_error = (product - f64::from(factor)).abs() / f64::from(factor);
        assert!(
            relative_error < 1e-8,
            "factor {factor}: tempos {tempos:?} multiply to {product}"
        );
    }
}

#[test]
fn audio_tempo_filters_never_exceed_the_per_filter_limit_or_slow_sound_down() {
    for factor in [2u32, 100, 101, 250, 1000, 10_000, 1_000_000, 4_000_000_000] {
        for tempo in audio_tempo_filters(speed(factor)) {
            let tempo: f64 = tempo.parse().unwrap();
            assert!((1.0..=100.0).contains(&tempo), "factor {factor} gave tempo {tempo}");
        }
    }
}

#[test]
fn normal_speed_needs_no_tempo_filter_and_small_factors_need_exactly_one() {
    assert!(audio_tempo_filters(speed(1)).is_empty());
    assert_eq!(audio_tempo_filters(speed(8)), ["8"]);
    assert_eq!(audio_tempo_filters(speed(100)), ["100"]);
    assert_eq!(audio_tempo_filters(speed(250)), ["100", "2.5"]);
    assert_eq!(audio_tempo_filters(speed(1000)), ["100", "10"]);
    assert_eq!(audio_tempo_filters(speed(10_000)), ["100", "100"]);
}

// --- volume, silence between clips, several tracks ---

#[test]
fn a_clip_at_normal_loudness_has_no_volume_filter_and_one_that_is_not_has_exactly_its_factor() {
    let normal = sound_chain_of_only_clip(clip(0, 30, 1), SpedUpClipSound::MaintainPitch);
    assert!(!normal.contains("volume"), "{normal}");
    let louder = SoundClipToExport {
        loudness: LoudnessToExport::Constant(1.995_262_314_968_879_6),
        ..clip(0, 30, 1)
    };
    let chain = sound_chain_of_only_clip(louder, SpedUpClipSound::MaintainPitch);
    assert!(chain.ends_with(",volume=1.995262315[clip_cut_audio_0]"), "{chain}");
    let quieter = SoundClipToExport {
        loudness: LoudnessToExport::Constant(0.5),
        ..clip(0, 30, 1)
    };
    assert!(
        sound_chain_of_only_clip(quieter, SpedUpClipSound::MaintainPitch).ends_with(",volume=0.5[clip_cut_audio_0]"),
        "volume comes after the speed change, last of what is done to the clip"
    );
}

/// A clip of 90 frames (3 seconds at 30 frames a second) that is as loud as 0 decibels at its start, as -12 at 2 seconds, and
/// stays that.
fn clip_that_fades_out() -> SoundClipToExport {
    SoundClipToExport {
        loudness: LoudnessToExport::Changing(vec![(0, 0.0), (60, -12.0), (90, -12.0)]),
        ..clip(0, 90, 1)
    }
}

#[test]
fn a_clip_whose_loudness_changes_has_a_volume_that_is_worked_out_again_for_every_piece_of_its_sound() {
    let chain = sound_chain_of_only_clip(clip_that_fades_out(), SpedUpClipSound::MaintainPitch);
    assert!(
        chain.ends_with(
            ",volume='pow(10,(if(lt(t,2),0+(-6)*(t-0),if(lt(t,3),-12+(0)*(t-2),-12)))/20)':eval=frame[clip_cut_audio_0]"
        ),
        "{chain}"
    );
}

#[test]
fn the_decibels_are_a_chain_of_pieces_one_for_each_stretch_between_two_points_and_the_last_holds() {
    // 4 points: the pieces are from 0 to 1 s, from 1 to 1.5 s and from 1.5 to 2 s.
    let expression = decibels_expression(frame_rate_30(), &[(0, -3.0), (30, 3.0), (45, 3.0), (60, -9.0)]);
    assert_eq!(
        expression,
        "if(lt(t,1),-3+(6)*(t-0),if(lt(t,1.5),3+(0)*(t-1),if(lt(t,2),3+(-24)*(t-1.5),-9)))"
    );
}

#[test]
fn the_changing_volume_comes_after_the_speed_change_and_a_constant_one_has_no_expression() {
    let sped_up_and_fading = SoundClipToExport {
        loudness: LoudnessToExport::Changing(vec![(0, 0.0), (30, -6.0)]),
        ..clip(0, 300, 4)
    };
    let chain = sound_chain_of_only_clip(sped_up_and_fading, SpedUpClipSound::MaintainPitch);
    let atempo = chain.find("atempo=4").unwrap();
    let volume = chain.find("volume='").unwrap();
    assert!(atempo < volume, "{chain}");
    assert!(
        !chain.contains("eval=frame,"),
        "it is the last thing done to the clip: {chain}"
    );

    let constant = sound_chain_of_only_clip(
        SoundClipToExport {
            loudness: LoudnessToExport::Constant(0.5),
            ..clip(0, 30, 1)
        },
        SpedUpClipSound::MaintainPitch,
    );
    assert!(!constant.contains("eval=frame"), "{constant}");
}

#[test]
fn the_volume_is_applied_after_the_speed_so_that_a_clip_that_is_both_sped_up_and_louder_has_both() {
    let both = SoundClipToExport {
        loudness: LoudnessToExport::Constant(2.0),
        ..clip(0, 300, 4)
    };
    let chain = sound_chain_of_only_clip(both, SpedUpClipSound::MaintainPitch);
    let atempo = chain.find("atempo=4").unwrap();
    let volume = chain.find("volume=2").unwrap();
    assert!(atempo < volume, "{chain}");
}

#[test]
fn silence_between_clips_is_made_from_nothing_in_the_same_kind_of_sound_and_the_exact_length() {
    let statements = sound_statements(&specification(vec![strip(vec![
        silence(15),
        clip_item(0, 30, 1),
        silence(45),
        clip_item(30, 60, 1),
    ])]));
    assert_eq!(
        statements[1],
        format!(
            "anullsrc=r=48000:cl=stereo,{SAMPLE_FORMAT},atrim=end=0.500000000,asetpts=PTS-STARTPTS[strip_0_item_0]"
        )
    );
    // (The first clip takes the three statements after it; the second silence is next.)
    assert!(statements[5].starts_with(&format!(
        "anullsrc=r=48000:cl=stereo,{SAMPLE_FORMAT},atrim=end=1.500000000"
    )));
    assert!(statements[5].ends_with("[strip_0_item_2]"), "{}", statements[5]);
    assert_eq!(
        statements.last().unwrap(),
        "[strip_0_item_0][strip_0_item_1][strip_0_item_2][strip_0_item_3]concat=n=4:v=0:a=1[finished_audio]"
    );
}

#[test]
fn a_track_that_is_shorter_than_the_sound_is_made_as_long_as_it_by_padding_with_silence() {
    // One strip of 1 s in a sound that is 2 s long (the other track is not here, but the length is).
    let mut spec = specification(vec![strip(vec![clip_item(0, 30, 1)])]);
    spec.total_frame_count = 60;
    let statements = sound_statements(&spec);
    assert_eq!(
        statements[statements.len() - 2..],
        [
            "[strip_0_item_0]concat=n=1:v=0:a=1[strip_0]",
            "[strip_0]apad=whole_dur=2.000000000,atrim=end=2.000000000,asetpts=PTS-STARTPTS[finished_audio]",
        ]
    );
}

#[test]
fn several_tracks_are_each_made_as_long_as_the_sound_and_mixed_with_the_volume_put_back() {
    let spec = specification(vec![
        strip(vec![clip_item(0, 30, 1)]),
        SoundStripToExport {
            items: vec![SoundItemToExport::Clip(SoundClipToExport {
                source_input: 1,
                ..clip(0, 60, 1)
            })],
            buses: Vec::new(),
            loudness: Default::default(),
            muted: Vec::new(),
        },
        strip(vec![silence(10), clip_item(30, 40, 1)]),
    ]);
    let statements = sound_statements(&spec);
    let text = statements.join(";");
    // Two files are read: input 0 for two clips (the first and the third strip's), input 1 for one.
    assert!(
        text.contains(&format!(
            "[0:a:0]{SAMPLE_FORMAT},asplit=2[clip_source_audio_0][clip_source_audio_2]"
        )),
        "{text}"
    );
    assert!(
        text.contains(&format!("[1:a:0]{SAMPLE_FORMAT},asplit=1[clip_source_audio_1]")),
        "{text}"
    );
    for strip_number in 0..3 {
        assert!(
            text.contains(&format!(
                "[strip_{strip_number}]apad=whole_dur=2.000000000,atrim=end=2.000000000,asetpts=PTS-STARTPTS[padded_strip_{strip_number}]"
            )),
            "{text}"
        );
    }
    assert_eq!(
        statements.last().unwrap(),
        "[padded_strip_0][padded_strip_1][padded_strip_2]amix=inputs=3:duration=longest:dropout_transition=0,volume=3[finished_audio]"
    );
}

#[test]
fn the_clips_of_all_the_tracks_are_numbered_in_order_so_that_each_uses_its_own_copy_of_its_source() {
    let spec = specification(vec![
        strip(vec![clip_item(0, 30, 1), clip_item(30, 60, 1)]),
        strip(vec![clip_item(0, 30, 1)]),
    ]);
    let text = sound_statements(&spec).join(";");
    for clip_number in 0..3 {
        let label = format!("[clip_source_audio_{clip_number}]");
        assert_eq!(
            text.matches(&label).count(),
            2,
            "{label}: made once and used once in {text}"
        );
    }
}

// --- sound from before a clip, for the plugins that remember ---

/// A clip with an effect of a plugin for each of `pre_rolls`, each asking for that many samples of sound from before the clip. **The caller holds the list of installed plugins** (`the_installed_plugins_to_myself`) for as long as it uses the clip.
fn clip_with_plugins_that_ask_for(first: u64, end: u64, factor: u32, pre_rolls: &[usize]) -> SoundClipToExport {
    use crate::project::{Animation, EffectInstanceId, set_installed_plugins};
    use crate::test_support::an_installed_sound_plugin;
    let plugins: Vec<_> = pre_rolls
        .iter()
        .enumerate()
        .map(|(number, pre_roll)| {
            let mut plugin = an_installed_sound_plugin(&format!("clap:test.plugin{number}"), "Maker", &[]);
            plugin.pre_roll_samples = *pre_roll;
            plugin
        })
        .collect();
    set_installed_plugins(plugins);
    let mut with_effects = clip(first, end, factor);
    // (The clip began at the start of its file, and has been heard for `first` frames when this stretch of it begins.)
    with_effects.frames_of_the_clip_before = first;
    with_effects.effects = crate::project::EffectKind::installed_plugin_kinds()
        .into_iter()
        .map(|kind| crate::composition::LayerEffect {
            state: None,
            clock: Default::default(),
            instance: EffectInstanceId::FIRST,
            kind,
            values: Vec::new(),
            animation: Animation::default(),
            tables: Vec::new(),
            tail_length: Default::default(),
            passes_notes_on: false,
            shared_by: None,
        })
        .collect();
    with_effects
}

#[test]
fn the_sound_from_before_a_clip_is_what_the_greatest_of_its_plugins_asks_for_in_whole_frames_and_no_more_than_the_file_has()
 {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    // At 30 frames a second a frame is 1600 samples.
    let frames = |first: u64, asks: &[usize]| {
        pre_roll_frames_of(
            frame_rate_30(),
            &clip_with_plugins_that_ask_for(first, first + 90, 1, asks),
        )
    };
    assert_eq!(
        frames(100, &[1]),
        1,
        "one sample is a frame, as a part of a frame cannot be cut"
    );
    assert_eq!(frames(100, &[1600]), 1);
    assert_eq!(frames(100, &[1601]), 2);
    assert_eq!(
        frames(100, &[1000, 4000, 2500]),
        3,
        "the greatest: 4000 samples are 2.5 frames"
    );
    assert_eq!(frames(100, &[96_000]), 60, "two seconds");
    assert_eq!(frames(100, &[0]), 0, "a plugin that remembers nothing");
    assert_eq!(frames(100, &[]), 0, "no plugin at all");
    assert_eq!(frames(2, &[96_000]), 2, "no more than the file has before the clip");
    assert_eq!(
        frames(0, &[96_000]),
        0,
        "a clip that begins at the start of its file has nothing before it"
    );
}

#[test]
fn the_sound_from_before_a_clip_is_counted_in_the_frames_of_the_frame_rate_and_in_samples_by_the_arithmetic_of_the_graph()
 {
    let sixty = FrameRate::new(60, 1).unwrap();
    let ntsc = FrameRate::new(30_000, 1001).unwrap();
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let clip_at = |asks: usize| clip_with_plugins_that_ask_for(1000, 1090, 1, &[asks]);
    // At 60 frames a second a frame is 800 samples.
    assert_eq!(pre_roll_frames_of(sixty, &clip_at(1600)), 2);
    assert_eq!(pre_roll_frames_of(sixty, &clip_at(1601)), 3);
    // At 29.97 a second a frame is 1601.6 samples: 3200 samples are 2 frames, and 3204 are 3.
    assert_eq!(pre_roll_frames_of(ntsc, &clip_at(3200)), 2);
    assert_eq!(pre_roll_frames_of(ntsc, &clip_at(3204)), 3);
    // The samples are those between the starts of the frames, by the rounding that places everything else.
    let samples = crate::sound_engine::pre_roll_samples_of(ntsc, &clip_at(3200));
    assert_eq!(
        samples as u64,
        crate::sound_engine::sample_at_frame(ntsc, 1000) - crate::sound_engine::sample_at_frame(ntsc, 998)
    );
    assert!((3203..=3204).contains(&samples), "{samples}");
    assert_eq!(
        crate::sound_engine::pre_roll_samples_of(sixty, &clip_at(1601)),
        2400,
        "three frames of 800 samples"
    );
}

#[test]
fn a_clip_that_is_sped_up_has_no_sound_from_before_it() {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    assert_eq!(
        pre_roll_frames_of(frame_rate_30(), &clip_with_plugins_that_ask_for(100, 190, 2, &[96_000])),
        0
    );
    assert_eq!(
        pre_roll_frames_of(frame_rate_30(), &clip_with_plugins_that_ask_for(100, 190, 1, &[96_000])),
        60
    );
}

#[test]
fn the_feed_of_a_clip_with_sound_from_before_it_is_cut_out_of_the_file_that_many_frames_earlier_and_a_feed_with_none_is_as_it_was()
 {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    let with = clip_with_plugins_that_ask_for(100, 190, 1, &[3200]);
    let plain = clip(100, 190, 1);

    let filters = clip_feed_filters(frame_rate_30(), &with, SpedUpClipSound::MaintainPitch);
    let as_it_was = clip_feed_filters(frame_rate_30(), &plain, SpedUpClipSound::MaintainPitch);

    // Frame 98 is 3.266666667 seconds into the file; frame 100 is 3.333333333, and the end does not move.
    assert!(filters.contains("atrim=start=3.266666667:end=6.333333333"), "{filters}");
    assert!(
        as_it_was.contains("atrim=start=3.333333333:end=6.333333333"),
        "{as_it_was}"
    );
    // The filter graph of ffmpeg that makes the sound with no plugins in it does not have the sound from before.
    assert!(
        super::super::build_audio_graph(&specification(vec![strip(vec![SoundItemToExport::Clip(with)])]))
            .text
            .contains("atrim=start=3.333333333:end=6.333333333")
    );
}

#[test]
fn the_sound_of_the_file_from_before_the_first_frame_of_a_clip_is_not_given_to_a_plugin_and_that_of_the_clip_heard_before_the_stretch_is()
 {
    let _alone = crate::test_support::the_installed_plugins_to_myself();
    // A clip cut to begin 100 frames into its file (the file has something before it that was cut off), seen from a stretch that begins 10 frames into the clip.
    let mut clip = clip_with_plugins_that_ask_for(110, 200, 1, &[96_000]);

    clip.frames_of_the_clip_before = 10;
    assert_eq!(
        pre_roll_frames_of(frame_rate_30(), &clip),
        10,
        "the 10 frames of the clip that were heard before: and not the 60 that the plugin asks for, nor the 110 that the file has"
    );
    assert!(
        clip_feed_filters(frame_rate_30(), &clip, SpedUpClipSound::MaintainPitch).contains("atrim=start=3.333333333:"),
        "the feed begins at the frame 100 of the file, where the clip begins"
    );

    clip.frames_of_the_clip_before = 0;
    assert_eq!(
        pre_roll_frames_of(frame_rate_30(), &clip),
        0,
        "a clip played from its beginning has no sound of its own from before, whatever the file has before it"
    );
    assert!(
        clip_feed_filters(frame_rate_30(), &clip, SpedUpClipSound::MaintainPitch).contains("atrim=start=3.666666667:"),
        "the feed begins at the frame 110 of the file, as it did before sound from before a clip was a thing"
    );
}
