use nice_plug::plugin::ParamValue;
use nice_plug::prelude::*;

pub fn migrate(state: &mut PluginState) {
    state
        .fields
        .entry("ui_max_fps".into())
        .or_insert("60".into());
    state
        .fields
        .entry("ui_debug_fps".into())
        .or_insert("false".into());
    // Old presets contain only the six original controls. Explicit defaults
    // matter when loading into an instance that was previously in MIDI mode:
    // nice-plug leaves parameters absent from the serialized map untouched.
    for (id, value) in [
        ("fft_size", ParamValue::String("4096".into())),
        ("output_mode", ParamValue::String("mixed".into())),
        ("pitch_source", ParamValue::String("internal".into())),
        ("max_polyphony", ParamValue::I32(16)),
        // Pre-Harmonics presets used 32 partials. Preserve that legacy sound;
        // fresh instances use 256; counts above the new cap migrate below.
        ("harmonics", ParamValue::I32(32)),
        ("hf_damp", ParamValue::F32(0.0)),
        ("lf_damp", ParamValue::F32(0.0)),
        ("input_send_db", ParamValue::F32(0.0)),
        ("panic", ParamValue::Bool(false)),
        ("mute_low", ParamValue::Bool(false)),
        ("mute_high", ParamValue::Bool(false)),
        // Earlier versions always replaced the middle with wet audio. Missing
        // Mix must restore that behavior even in an already-loaded instance.
        ("mid_mix", ParamValue::F32(100.0)),
        ("decay_mode", ParamValue::String("damping".into())),
        ("attack_mode", ParamValue::String("natural".into())),
        ("attack_ms", ParamValue::F32(10.0)),
        ("attack_emphasis_db", ParamValue::F32(6.0)),
        ("align_wet", ParamValue::F32(0.5)),
        ("mod_mode", ParamValue::String("off".into())),
        ("mod_rate_hz", ParamValue::F32(0.5)),
        ("mod_amount", ParamValue::F32(50.0)),
        ("mod_pitch_semitones", ParamValue::F32(0.1)),
        ("grain_ms", ParamValue::F32(80.0)),
        ("unison_voices", ParamValue::I32(1)),
        ("unison_mode", ParamValue::String("spectral".into())),
        ("unison_detune_cents", ParamValue::F32(7.0)),
        ("voice_spread", ParamValue::F32(0.0)),
    ] {
        state.params.entry(id.into()).or_insert(value);
    }
    // 0.11.1 stored this ID as a switch. Preserve both explicit endpoints;
    // fresh/missing values use half a window, while new floats stay untouched.
    if let Some(value) = state.params.get_mut("align_wet")
        && let ParamValue::Bool(enabled) = value
    {
        *value = ParamValue::F32(if *enabled { 1.0 } else { 0.0 });
    }
    // The user explicitly lowered the capacity. Make old 513..1024 presets
    // deterministic instead of depending on host-specific range clamping.
    if let Some(ParamValue::I32(count)) = state.params.get_mut("harmonics") {
        *count = (*count).clamp(1, spectral_dsp::MAX_PARTIALS as i32);
    }
    for (index, hz) in spectral_dsp::DEFAULT_DECAY_HZ.into_iter().enumerate() {
        state
            .params
            .entry(format!("decay_point_hz_{}", index + 1))
            .or_insert(ParamValue::F32(hz));
        state
            .params
            .entry(format!("decay_point_seconds_{}", index + 1))
            .or_insert(ParamValue::F32(2.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn fft_defaults_to_legacy_window_and_preserves_each_saved_selection() {
        for value in [None, Some("1024"), Some("2048"), Some("3072"), Some("4096")] {
            let mut state = PluginState {
                version: "0.12.0".into(),
                params: value
                    .map(|v| BTreeMap::from([("fft_size".into(), ParamValue::String(v.into()))]))
                    .unwrap_or_default(),
                fields: BTreeMap::new(),
            };
            migrate(&mut state);
            migrate(&mut state);
            assert!(
                matches!(&state.params["fft_size"], ParamValue::String(v) if v == value.unwrap_or("4096"))
            );
        }
    }

    #[test]
    fn alignment_switch_states_migrate_and_float_values_survive_repeated_loads() {
        for (saved, expected) in [
            (ParamValue::Bool(false), 0.0),
            (ParamValue::Bool(true), 1.0),
            (ParamValue::F32(0.5), 0.5),
            (ParamValue::F32(0.37123), 0.37123),
        ] {
            let mut state = PluginState {
                version: "0.11.1".into(),
                params: BTreeMap::from([("align_wet".into(), saved)]),
                fields: BTreeMap::new(),
            };
            for _ in 0..2 {
                migrate(&mut state);
                assert!(matches!(state.params["align_wet"], ParamValue::F32(v) if v == expected));
            }
        }
        assert_eq!(
            crate::params::SpectralResonatorParams::default()
                .align_wet
                .value(),
            0.5
        );
    }

    #[test]
    fn old_presets_receive_defaults_without_overwriting_saved_values() {
        let mut state = PluginState {
            version: "0.4.1".into(),
            params: BTreeMap::new(),
            fields: BTreeMap::new(),
        };
        state
            .params
            .insert("m2_wet_level".into(), ParamValue::F32(1.0));
        state
            .params
            .insert("output_gain".into(), ParamValue::F32(0.4));
        migrate(&mut state);
        assert!(
            matches!(&state.params["pitch_source"], ParamValue::String(value) if value == "internal")
        );
        assert!(matches!(state.params["m2_wet_level"], ParamValue::F32(1.0)));
        assert!(matches!(state.params["output_gain"], ParamValue::F32(0.4)));
        assert!(matches!(state.params["mid_mix"], ParamValue::F32(100.0)));
        assert!(matches!(state.params["mute_low"], ParamValue::Bool(false)));
        assert!(matches!(state.params["mute_high"], ParamValue::Bool(false)));
        assert!(matches!(state.params["harmonics"], ParamValue::I32(32)));
        assert!(matches!(state.params["max_polyphony"], ParamValue::I32(16)));
        assert!(
            matches!(&state.params["attack_mode"], ParamValue::String(mode) if mode == "natural")
        );
        assert!(matches!(state.params["attack_ms"], ParamValue::F32(10.0)));
        assert!(matches!(
            state.params["attack_emphasis_db"],
            ParamValue::F32(6.0)
        ));
        assert!(matches!(state.params["align_wet"], ParamValue::F32(0.5)));
        assert_eq!(state.fields["ui_max_fps"], "60");
        assert_eq!(state.fields["ui_debug_fps"], "false");
        assert!(matches!(&state.params["mod_mode"], ParamValue::String(mode) if mode == "off"));
        assert!(
            matches!(&state.params["decay_mode"], ParamValue::String(mode) if mode == "damping")
        );
        assert!(matches!(state.params["unison_voices"], ParamValue::I32(1)));
        assert!(matches!(state.params["voice_spread"], ParamValue::F32(0.0)));
        assert!(matches!(
            state.params["decay_point_hz_3"],
            ParamValue::F32(1000.0)
        ));
        state
            .params
            .insert("mod_mode".into(), ParamValue::String("granular".into()));
        state
            .params
            .insert("decay_mode".into(), ParamValue::String("curve".into()));
        state
            .params
            .insert("decay_point_seconds_3".into(), ParamValue::F32(5.5));
        state
            .params
            .insert("pitch_source".into(), ParamValue::String("midi".into()));
        state.params.insert("hf_damp".into(), ParamValue::F32(0.75));
        state.params.insert("mid_mix".into(), ParamValue::F32(35.0));
        state
            .params
            .insert("mute_low".into(), ParamValue::Bool(true));
        state
            .params
            .insert("mute_high".into(), ParamValue::Bool(true));
        state
            .params
            .insert("align_wet".into(), ParamValue::Bool(false));
        state.params.insert(
            "attack_mode".into(),
            ParamValue::String("independent".into()),
        );
        state
            .params
            .insert("attack_ms".into(), ParamValue::F32(0.0));
        state
            .params
            .insert("max_polyphony".into(), ParamValue::I32(5));
        state.fields.insert("ui_max_fps".into(), "90".into());
        state.fields.insert("ui_debug_fps".into(), "true".into());
        state
            .params
            .insert("voice_spread".into(), ParamValue::F32(65.0));
        migrate(&mut state);
        assert!(
            matches!(&state.params["pitch_source"], ParamValue::String(value) if value == "midi")
        );
        assert!(matches!(state.params["hf_damp"], ParamValue::F32(0.75)));
        assert!(matches!(state.params["mid_mix"], ParamValue::F32(35.0)));
        assert!(matches!(state.params["mute_low"], ParamValue::Bool(true)));
        assert!(matches!(state.params["mute_high"], ParamValue::Bool(true)));
        assert!(matches!(state.params["align_wet"], ParamValue::F32(0.0)));
        assert!(
            matches!(&state.params["attack_mode"], ParamValue::String(mode) if mode == "independent")
        );
        assert!(matches!(state.params["attack_ms"], ParamValue::F32(0.0)));
        assert!(matches!(state.params["max_polyphony"], ParamValue::I32(5)));
        assert_eq!(state.fields["ui_max_fps"], "90");
        assert_eq!(state.fields["ui_debug_fps"], "true");
        assert!(matches!(
            state.params["voice_spread"],
            ParamValue::F32(65.0)
        ));
        assert!(
            matches!(&state.params["mod_mode"], ParamValue::String(mode) if mode == "granular")
        );
        assert!(matches!(&state.params["decay_mode"], ParamValue::String(mode) if mode == "curve"));
        assert!(matches!(
            state.params["decay_point_seconds_3"],
            ParamValue::F32(5.5)
        ));
        assert!(
            matches!(&state.params["unison_mode"], ParamValue::String(mode) if mode == "spectral")
        );
        state
            .params
            .insert("unison_mode".into(), ParamValue::String("post".into()));
        for count in [1, 32, 64, 256, 512, 1024] {
            state
                .params
                .insert("harmonics".into(), ParamValue::I32(count));
            migrate(&mut state);
            assert!(
                matches!(state.params["harmonics"], ParamValue::I32(saved) if saved == count.min(512))
            );
            assert!(
                matches!(&state.params["unison_mode"], ParamValue::String(mode) if mode == "post")
            );
        }
        for count in [1, 2, 4, 8] {
            state
                .params
                .insert("unison_voices".into(), ParamValue::I32(count));
            migrate(&mut state);
            assert!(
                matches!(state.params["unison_voices"], ParamValue::I32(saved) if saved == count)
            );
        }
        state
            .params
            .insert("attack_mode".into(), ParamValue::String("reshape".into()));
        state
            .params
            .insert("attack_emphasis_db".into(), ParamValue::F32(7.12345));
        migrate(&mut state);
        assert!(
            matches!(&state.params["attack_mode"], ParamValue::String(mode) if mode == "reshape")
        );
        assert!(matches!(state.params["attack_emphasis_db"], ParamValue::F32(v) if v == 7.12345));
    }
}
