//! Exercise the real plugin callback with a minimal in-process host. Only the
//! host adapter needs unsafe buffer setup; production DSP remains unsafe-free.
use super::*;
use nice_plug::{
    context::process::SendEventError,
    midi::{Channel, Key, VoiceID},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    collections::VecDeque,
};

thread_local! {
    static TRACK_ALLOCATIONS: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct TrackingAllocator;

fn track() {
    let _ = TRACK_ALLOCATIONS.try_with(|enabled| {
        if enabled.get() {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        }
    });
}

// SAFETY: Every allocation operation is forwarded unchanged to the standard
// system allocator. Per-thread counters contain no heap storage or locks.
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        track();
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        track();
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        track();
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

#[test]
fn visualization_producer_is_allocation_free_even_when_saturated() {
    let mut capture = analyzer::AudioCapture::new(analyzer::SharedAnalysis::new());
    ALLOCATIONS.set(0);
    TRACK_ALLOCATIONS.set(true);
    for _ in 0..20_000 {
        capture.push([0.3, -0.3], [0.2, -0.2], true);
    }
    capture.reset(96_000.0);
    TRACK_ALLOCATIONS.set(false);
    assert_eq!(ALLOCATIONS.get(), 0);
}

#[test]
fn output_routes_recombine_and_aux_is_post_unison_wet() {
    let mut mixed = plugin();
    let mut dry = plugin();
    let mut wet = plugin();
    for (plugin, mode) in [
        (&mut mixed, OutputMode::Mixed),
        (&mut dry, OutputMode::Dry),
        (&mut wet, OutputMode::Wet),
    ] {
        let p = Arc::get_mut(&mut plugin.params).unwrap();
        p.output_mode = EnumParam::new("Main Output", mode);
        p.pitch_source = EnumParam::new("Pitch Source", PitchSource::Internal);
        p.unison_mode = EnumParam::new("Unison Mode", params::UnisonMode::Post);
        p.unison_voices = IntParam::new("Unison", 4, IntRange::Linear { min: 1, max: 8 });
        plugin.reset();
    }
    let mut mixed_l = vec![0.0; 16384];
    for (i, x) in mixed_l.iter_mut().enumerate() {
        *x = 0.1 * (std::f32::consts::TAU * 440.0 * i as f32 / 48000.0).sin();
    }
    let mut dry_l = mixed_l.clone();
    let mut wet_l = mixed_l.clone();
    let mut mixed_r = vec![0.0; mixed_l.len()];
    let mut dry_r = mixed_r.clone();
    let mut wet_r = mixed_r.clone();
    callback(&mut mixed, &mut mixed_l, &mut mixed_r, vec![]);
    callback(&mut dry, &mut dry_l, &mut dry_r, vec![]);
    // Exercise the actual auxiliary-buffer path, not just the DSP tap.
    let mut aux_l = vec![f32::NAN; wet_l.len()];
    let mut aux_r = aux_l.clone();
    let mut buffer = Buffer::default();
    let mut aux_buffer = Buffer::default();
    // SAFETY: All channel slices are disjoint, equally sized, and remain live
    // until both temporary buffers have been dropped.
    unsafe {
        buffer.set_slices(wet_l.len(), |s| {
            s.push(&mut wet_l);
            s.push(&mut wet_r);
        });
        aux_buffer.set_slices(aux_l.len(), |s| {
            s.push(&mut aux_l);
            s.push(&mut aux_r);
        });
    }
    let mut outputs = [aux_buffer];
    let mut aux = AuxiliaryBuffers {
        inputs: &mut [],
        outputs: &mut outputs,
    };
    let mut host = Host {
        transport: Transport::new(48000.0),
        events: VecDeque::new(),
    };
    ALLOCATIONS.set(0);
    TRACK_ALLOCATIONS.set(true);
    wet.process(&mut buffer, &mut aux, &mut host);
    TRACK_ALLOCATIONS.set(false);
    assert_eq!(ALLOCATIONS.get(), 0);
    for i in 0..wet_l.len() {
        assert!((mixed_l[i] - dry_l[i] - wet_l[i]).abs() < 1e-6);
        assert_eq!(aux_l[i], wet_l[i]);
        assert_eq!(aux_r[i], 0.0);
        assert_eq!(wet_r[i], 0.0);
    }
    assert!(wet_l.iter().any(|v| v.abs() > 1e-4));
}

struct Host {
    transport: Transport,
    events: VecDeque<NoteEvent<()>>,
}

impl ProcessContext<SpectralResonator> for Host {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    fn execute_background(&self, _: ()) {}
    fn execute_gui(&self, _: ()) {}
    fn transport(&self) -> &Transport {
        &self.transport
    }
    fn next_event(&mut self) -> Option<NoteEvent<()>> {
        self.events.pop_front()
    }
    fn try_send_event(
        &mut self,
        event: NoteEvent<()>,
    ) -> Result<(), (NoteEvent<()>, SendEventError)> {
        Err((event, SendEventError::NoOutputBuffer))
    }
    fn set_latency_samples(&self, _: u32) {}
    fn request_restart(&self) {}
    fn set_current_voice_capacity(&self, _: u32) {}
}

fn plugin() -> SpectralResonator {
    plugin_with_mix(100.0)
}

fn plugin_with_mix(mid_mix: f32) -> SpectralResonator {
    let params = SpectralResonatorParams {
        pitch_source: EnumParam::new("Pitch Source", PitchSource::Midi),
        mid_mix: FloatParam::new(
            "Mid Mix",
            mid_mix,
            FloatRange::Linear {
                min: 0.0,
                max: 100.0,
            },
        ),
        ..Default::default()
    };
    for param in [
        &params.output_gain,
        &params.decay_t60,
        &params.m2_wet_level,
        &params.mid_mix,
        &params.low_mid_hz,
        &params.mid_high_hz,
        &params.hf_damp,
        &params.lf_damp,
        &params.input_send_db,
        &params.mod_rate_hz,
        &params.mod_amount,
        &params.mod_pitch_semitones,
        &params.grain_ms,
        &params.unison_detune_cents,
        &params.voice_spread,
    ] {
        param.smoothed.reset(param.value());
    }
    for point in &params.decay_points {
        point.hz.smoothed.reset(point.hz.value());
        point.seconds.smoothed.reset(point.seconds.value());
    }
    SpectralResonator {
        params: Arc::new(params),
        stft: Some(ResonatorEngine::new(2, FFT_SIZE, HOP_SIZE, 48_000.0).unwrap()),
        sample_rate: 48_000.0,
        pitch_source: PitchSource::Midi,
        ..Default::default()
    }
}

fn callback(
    plugin: &mut SpectralResonator,
    left: &mut [f32],
    right: &mut [f32],
    events: Vec<NoteEvent<()>>,
) {
    let mut host = Host {
        transport: Transport::new(plugin.sample_rate),
        events: events.into(),
    };
    let mut buffer = Buffer::default();
    assert_eq!(left.len(), right.len());
    // SAFETY: Both slices are live, disjoint and equal-length. The temporary
    // Buffer is dropped before either borrowed slice can be used again.
    unsafe {
        buffer.set_slices(left.len(), |slices| {
            slices.push(left);
            slices.push(right);
        });
    }
    let mut aux = AuxiliaryBuffers {
        inputs: &mut [],
        outputs: &mut [],
    };
    ALLOCATIONS.set(0);
    TRACK_ALLOCATIONS.set(true);
    let status = plugin.process(&mut buffer, &mut aux, &mut host);
    TRACK_ALLOCATIONS.set(false);
    assert_eq!(
        ALLOCATIONS.get(),
        0,
        "audio callback allocated or freed heap memory"
    );
    assert!(matches!(status, ProcessStatus::Tail(_)));
    assert!(host.events.is_empty());
}

fn on(timing: u32, key: u8, id: i32) -> NoteEvent<()> {
    NoteEvent::NoteOn {
        timing,
        voice_id: VoiceID::ID(id),
        channel: Channel::Number(0),
        key: Key::Number(key),
        velocity: 1.0,
    }
}

#[test]
fn callback_events_are_block_size_invariant_and_allocation_free() {
    let mut schedule = vec![
        (2300, true, 69, 1),
        (6500, true, 72, 2),
        (7200, false, 72, 2),
        (9000, false, 69, 1),
        (12001, true, 57, 3),
        (12065, false, 57, 3),
    ];
    // Exercise the full voice pool, pending replacements and note-offs while
    // stealing under the same allocation and block-size assertions.
    for id in 10..50 {
        let time = 13_000 + (id as usize - 10) * 31;
        schedule.push((time, true, 33 + (id % 12) as u8, id));
        schedule.push((time + 83, false, 33 + (id % 12) as u8, id));
    }
    schedule.sort_by_key(|event| event.0);
    let render = |blocks: &[usize], with_notes: bool| {
        let mut plugin = plugin();
        let mut left: Vec<_> = (0..24_000)
            .map(|i| {
                if i < 15_000 {
                    ((i as f32 * 0.0576).sin() + (i as f32 * 0.072).sin()) * 0.2
                } else {
                    0.0
                }
            })
            .collect();
        let mut right = vec![0.0; left.len()];
        let mut position = 0;
        let mut block = 0;
        while position < left.len() {
            let end = (position + blocks[block % blocks.len()]).min(left.len());
            let events = schedule
                .iter()
                .filter(|(t, ..)| with_notes && (position..end).contains(t))
                .map(|&(t, pressed, key, id)| {
                    if pressed {
                        on((t - position) as u32, key, id)
                    } else {
                        NoteEvent::NoteOff {
                            timing: (t - position) as u32,
                            voice_id: VoiceID::ID(id),
                            channel: Channel::Number(0),
                            key: Key::Number(key),
                            velocity: 0.0,
                        }
                    }
                })
                .collect();
            callback(
                &mut plugin,
                &mut left[position..end],
                &mut right[position..end],
                events,
            );
            position = end;
            block += 1;
        }
        assert!(right.iter().all(|&sample| sample == 0.0));
        assert!(plugin.notes.current().is_none());
        left
    };
    let fixed = render(&[128], true);
    let varied = render(&[1, 7, 64, 127, 257, 512, 1024], true);
    assert_eq!(fixed, varied);
    let dry = render(&[128], false);
    assert_eq!(
        &fixed[..2300],
        &dry[..2300],
        "event affected audio before its input timestamp"
    );
    let energy: f64 = fixed
        .iter()
        .zip(&dry)
        .map(|(&a, &b)| f64::from(a - b).powi(2))
        .sum();
    assert!(energy > 0.01, "MIDI never excited the resonator");
}

#[test]
fn boundary_and_empty_buffer_events_are_not_lost() {
    let mut plugin = plugin();
    // Simulate a source change whose first host callback contains no audio.
    plugin.pitch_source = PitchSource::Internal;
    callback(&mut plugin, &mut [], &mut [], vec![on(0, 69, 1)]);
    assert_eq!(plugin.notes.current().unwrap().key, 69);
    callback(
        &mut plugin,
        &mut [0.0; 7],
        &mut [0.0; 7],
        vec![on(7, 72, 2)],
    );
    assert_eq!(plugin.notes.current().unwrap().key, 72);
    plugin.reset();
    assert!(plugin.notes.current().is_none());
}

#[test]
fn zero_mix_callback_restores_delayed_dry_with_active_midi() {
    let mut plugin = plugin_with_mix(0.0);
    let input: Vec<_> = (0..12_000)
        .map(|i| (i as f32 * 0.037).sin() * 0.4)
        .collect();
    let mut left = input.clone();
    left.resize(input.len() + FFT_SIZE, 0.0);
    let mut right = vec![0.0; left.len()];
    callback(&mut plugin, &mut left, &mut right, vec![on(0, 69, 1)]);
    for (i, &sample) in left.iter().enumerate() {
        let expected = i
            .checked_sub(FFT_SIZE)
            .and_then(|i| input.get(i))
            .copied()
            .unwrap_or(0.0);
        assert!(
            (sample - expected).abs() < 3e-6,
            "sample {i}: {sample} != {expected}"
        );
    }
    assert!(right.iter().all(|&sample| sample == 0.0));
}

fn off(timing: u32, key: u8, id: i32) -> NoteEvent<()> {
    NoteEvent::NoteOff {
        timing,
        voice_id: VoiceID::ID(id),
        channel: Channel::Number(0),
        key: Key::Number(key),
        velocity: 0.0,
    }
}

fn render_events(events: Vec<NoteEvent<()>>) -> [Vec<f32>; 2] {
    let mut plugin = plugin();
    let mut left: Vec<f32> = (0..24_000)
        .map(|i| ((i as f32 * 0.0576).sin() + (i as f32 * 0.0685).sin()) * 0.2)
        .collect();
    let mut right: Vec<f32> = (0..24_000)
        .map(|i| (i as f32 * 0.109).sin() * 0.15)
        .collect();
    callback(&mut plugin, &mut left, &mut right, events);
    [left, right]
}

#[test]
fn plugin_chord_and_released_tails_equal_independent_note_outputs() {
    let a = render_events(vec![on(1000, 69, 1), off(6500, 69, 1)]);
    let b = render_events(vec![on(5000, 72, 2), off(12000, 72, 2)]);
    let chord = render_events(vec![
        on(1000, 69, 1),
        on(5000, 72, 2),
        off(6500, 69, 1),
        off(12000, 72, 2),
    ]);
    let dry = render_events(vec![]);
    for channel in 0..2 {
        for i in 0..chord[channel].len() {
            let expected = a[channel][i] + b[channel][i] - dry[channel][i];
            assert!(
                (chord[channel][i] - expected).abs() < 3e-6,
                "channel={channel} sample={i}"
            );
        }
    }
    for voice in [a, b] {
        let energy: f64 = voice[0][18_000..]
            .iter()
            .zip(&dry[0][18_000..])
            .map(|(a, b)| f64::from(a - b).powi(2))
            .sum();
        assert!(energy > 0.01, "released voice tail disappeared");
    }
}

#[test]
fn sustain_release_matches_delayed_note_off_in_polyphonic_callback() {
    let sustained = render_events(vec![
        on(1000, 69, 1),
        on(2000, 72, 2),
        NoteEvent::MidiCC {
            timing: 3000,
            channel: 0,
            cc: 64,
            value: 1.0,
        },
        off(4000, 69, 1),
        NoteEvent::MidiCC {
            timing: 8000,
            channel: 0,
            cc: 64,
            value: 0.0,
        },
        off(10000, 72, 2),
    ]);
    let held = render_events(vec![
        on(1000, 69, 1),
        on(2000, 72, 2),
        off(8000, 69, 1),
        off(10000, 72, 2),
    ]);
    assert_eq!(sustained, held);
}

#[test]
fn maximum_harmonics_handles_sixteen_voices_eight_unison_and_reset_without_allocations() {
    for algorithm in [
        crate::params::UnisonMode::Spectral,
        crate::params::UnisonMode::Post,
    ] {
        let mut plugin = plugin();
        Arc::get_mut(&mut plugin.params).unwrap().unison_mode =
            EnumParam::new("Unison Mode", algorithm);
        assert_eq!(plugin.params.harmonics.value(), 256);
        assert_eq!(plugin.params.harmonics.preview_plain(1.0), 512);
        assert_eq!(plugin.params.unison_voices.value(), 1);
        assert_eq!(plugin.params.unison_voices.preview_plain(1.0), 8);
        // Configure maximum partials before processing, with the same parameter
        // range exposed to the host. 192 kHz permits all 512 for these low notes.
        Arc::get_mut(&mut plugin.params).unwrap().harmonics =
            IntParam::new("Harmonics", 512, IntRange::Linear { min: 1, max: 512 });
        Arc::get_mut(&mut plugin.params).unwrap().unison_voices =
            IntParam::new("Unison", 8, IntRange::Linear { min: 1, max: 8 });
        plugin.params.voice_spread.smoothed.reset(100.0);
        plugin.sample_rate = 192_000.0;
        plugin.stft =
            Some(ResonatorEngine::new(2, FFT_SIZE, HOP_SIZE, plugin.sample_rate).unwrap());
        let mut left: Vec<f32> = (0..16_384)
            .map(|i| (i as f32 * 0.113).sin() * 0.3)
            .collect();
        let mut right = vec![0.0; left.len()];
        // Repeated low pitches have distinct identities and permit every partial
        // at this rate. First fill all 16 slots, then force pending replacements.
        let capacity = spectral_dsp::MAX_VOICES as i32;
        let mut events: Vec<_> = (0..capacity).map(|id| on(0, 33, id)).collect();
        callback(&mut plugin, &mut left[..4096], &mut right[..4096], events);
        assert_eq!(plugin.stft.as_ref().unwrap().active_voice_count(), 16);
        events = Vec::new();
        for id in capacity..capacity * 3 {
            events.push(on((id - capacity) as u32 * 31, 33, id));
        }
        for id in 0..capacity * 3 {
            events.push(off(4096, 33, id));
        }
        callback(&mut plugin, &mut left[4096..], &mut right[4096..], events);
        assert!(left.iter().all(|value| value.is_finite()));
        assert!(right.iter().all(|&value| value == 0.0));
        ALLOCATIONS.set(0);
        TRACK_ALLOCATIONS.set(true);
        plugin.reset();
        TRACK_ALLOCATIONS.set(false);
        assert_eq!(
            ALLOCATIONS.get(),
            0,
            "reset reallocated the enlarged voice pool"
        );
        callback(&mut plugin, &mut [0.0; 512], &mut [0.0; 512], vec![]);
        assert_eq!(plugin.stft.as_ref().unwrap().active_voice_count(), 0);
    }
}

#[test]
fn modulation_curve_automation_is_block_invariant_stereo_isolated_and_allocation_free() {
    use crate::params::ModulationMode;
    let render = |blocks: &[usize], mix: f32| {
        let mut plugin = plugin_with_mix(mix);
        let mut left: Vec<f32> = (0..40_960)
            .map(|i| {
                if i < 24_576 {
                    (i as f32 * 0.0576).sin() * 0.3
                } else {
                    0.0
                }
            })
            .collect();
        let mut right = vec![0.0; left.len()];
        let mut position = 0;
        let mut block = 0;
        while position < left.len() {
            // Deliver identical parameter automation at absolute positions,
            // independently of the host's choice of audio block sizes.
            if position % 8192 == 0 {
                let stage = position / 8192;
                let params = Arc::get_mut(&mut plugin.params).unwrap();
                params.mod_mode = EnumParam::new(
                    "Modulation Mode",
                    [
                        ModulationMode::Chorus,
                        ModulationMode::Wander,
                        ModulationMode::Granular,
                        ModulationMode::Off,
                        ModulationMode::Granular,
                    ][stage],
                );
                params.unison_voices = IntParam::new(
                    "Unison Voices",
                    [8, 3, 8, 1, 2][stage],
                    IntRange::Linear {
                        min: 1,
                        max: spectral_dsp::MAX_UNISON as i32,
                    },
                );
                params.unison_mode = EnumParam::new(
                    "Unison Mode",
                    [
                        crate::params::UnisonMode::Spectral,
                        crate::params::UnisonMode::Post,
                        crate::params::UnisonMode::Post,
                        crate::params::UnisonMode::Spectral,
                        crate::params::UnisonMode::Post,
                    ][stage],
                );
                params
                    .voice_spread
                    .smoothed
                    .reset([0.0, 35.0, 100.0, 60.0, 0.0][stage]);
                params.decay_mode = EnumParam::new(
                    "Decay Mode",
                    if stage == 3 {
                        DecayMode::Damping
                    } else {
                        DecayMode::Curve
                    },
                );
                params.decay_points[2]
                    .seconds
                    .smoothed
                    .reset(0.4 + stage as f32 * 0.3);
                params.decay_points[2]
                    .hz
                    .smoothed
                    .reset(600.0 + stage as f32 * 100.0);
            }
            let end = (position + blocks[block % blocks.len()])
                .min((position / 8192 + 1) * 8192)
                .min(left.len());
            let events = match position {
                0 => vec![on(0, 69, 1), on(0, 57, 2)],
                16_384 => vec![off(0, 69, 1), off(0, 57, 2)],
                _ => vec![],
            };
            callback(
                &mut plugin,
                &mut left[position..end],
                &mut right[position..end],
                events,
            );
            position = end;
            block += 1;
        }
        assert!(left.iter().all(|sample| sample.is_finite()));
        assert!(right.iter().all(|&sample| sample == 0.0));
        Arc::get_mut(&mut plugin.params).unwrap().panic = BoolParam::new("Panic", true);
        callback(&mut plugin, &mut [0.0; 8192], &mut [0.0; 8192], vec![]);
        assert_eq!(plugin.stft.as_ref().unwrap().active_voice_count(), 0);
        ALLOCATIONS.set(0);
        TRACK_ALLOCATIONS.set(true);
        plugin.reset();
        TRACK_ALLOCATIONS.set(false);
        assert_eq!(
            ALLOCATIONS.get(),
            0,
            "modulation reset allocated or freed memory"
        );
        left
    };
    let fixed = render(&[128], 100.0);
    assert_eq!(fixed, render(&[1, 7, 64, 127, 257, 512, 1024], 100.0));
    let dry = render(&[128], 0.0);
    for (i, sample) in dry.iter().enumerate() {
        let expected = i
            .checked_sub(FFT_SIZE)
            .filter(|&i| i < 24_576)
            .map_or(0.0, |i| (i as f32 * 0.0576).sin() * 0.3);
        assert!((sample - expected).abs() < 3e-6);
    }
    let tail_energy: f64 = fixed[32_768..].iter().map(|&x| f64::from(x).powi(2)).sum();
    assert!(
        tail_energy > 1e-5,
        "mode changes cleared the released resonance"
    );
    assert!(fixed.iter().zip(dry).any(|(a, b)| (a - b).abs() > 0.01));
}

#[test]
fn curve_host_parameter_ids_are_unique_and_match_state_migration() {
    let params = SpectralResonatorParams::default();
    assert_eq!(params.voice_spread.value(), 0.0);
    assert_eq!(
        params.unison_mode.value(),
        crate::params::UnisonMode::Spectral
    );
    let map = params.param_map();
    let ids: std::collections::BTreeSet<_> = map.iter().map(|(id, ..)| id.as_str()).collect();
    assert_eq!(ids.len(), map.len());
    let mut state = PluginState {
        version: "0.6.1".into(),
        params: Default::default(),
        fields: Default::default(),
    };
    state::migrate(&mut state);
    for id in state.params.keys() {
        assert!(
            ids.contains(id.as_str()),
            "migration refers to unknown parameter {id}"
        );
    }
    for index in 1..=6 {
        for prefix in ["decay_point_hz", "decay_point_seconds"] {
            assert!(ids.contains(format!("{prefix}_{index}").as_str()));
        }
    }
    // Regress the CLAP validator failure at normalized 1/11: an unrestricted
    // float formatter exposed last-bit changes after logarithmic conversion.
    for point in &params.decay_points {
        for param in [&point.hz, &point.seconds] {
            for step in 0..=1100 {
                let value = step as f32 / 1100.0;
                let display = param.normalized_value_to_string(value, true);
                let parsed = param.string_to_normalized_value(&display).unwrap();
                assert_eq!(param.normalized_value_to_string(parsed, true), display);
            }
        }
    }
}

#[test]
fn voice_spread_reaches_callback_wet_output_and_preserves_dry_and_left_channel() {
    let render = |spread: f32, wet: f32| {
        let mut plugin = plugin();
        plugin.params.voice_spread.smoothed.reset(spread);
        plugin.params.m2_wet_level.smoothed.reset(wet);
        let mut left: Vec<_> = (0..24_000)
            .map(|i| {
                if i < 12_000 {
                    (i as f32 * 0.0576).sin() * 0.3
                } else {
                    0.0
                }
            })
            .collect();
        let mut right = left.clone();
        callback(
            &mut plugin,
            &mut left,
            &mut right,
            vec![on(0, 69, 1), off(12_000, 69, 1)],
        );
        [left, right]
    };
    let centered = render(0.0, 4.0);
    let left_pan = render(100.0, 4.0);
    let dry = render(0.0, 0.0);
    assert_eq!(
        left_pan[0], centered[0],
        "favored left wet channel should retain unity gain"
    );
    let mut tail_energy = 0.0;
    for i in 8192..24_000 {
        assert!(
            (left_pan[1][i] - dry[1][i]).abs() < 3e-6,
            "right wet output not panned away at {i}"
        );
        if i >= 18_000 {
            tail_energy += f64::from(centered[1][i] - dry[1][i]).powi(2);
        }
    }
    assert!(
        tail_energy > 1e-4,
        "reference tail was silent, Pan test would be inconclusive"
    );
}
