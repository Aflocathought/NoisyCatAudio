use nice_plug::plugin::ParamValue;
use nice_plug::prelude::*;

pub fn migrate(state: &mut PluginState) {
    // Old presets contain only the six original controls. Explicit defaults
    // matter when loading into an instance that was previously in MIDI mode:
    // nice-plug leaves parameters absent from the serialized map untouched.
    for (id, value) in [
        ("output_mode", ParamValue::String("mixed".into())),
        ("pitch_source", ParamValue::String("internal".into())),
        // Pre-Harmonics presets used 32 partials. Preserve that legacy sound;
        // fresh instances use 256; counts above the new cap migrate below.
        ("harmonics", ParamValue::I32(32)),
        ("hf_damp", ParamValue::F32(0.0)),
        ("lf_damp", ParamValue::F32(0.0)),
        ("input_send_db", ParamValue::F32(0.0)),
        ("panic", ParamValue::Bool(false)),
        // Earlier versions always replaced the middle with wet audio. Missing
        // Mix must restore that behavior even in an already-loaded instance.
        ("mid_mix", ParamValue::F32(100.0)),
        ("decay_mode", ParamValue::String("damping".into())),
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
        assert!(matches!(state.params["harmonics"], ParamValue::I32(32)));
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
            .insert("voice_spread".into(), ParamValue::F32(65.0));
        migrate(&mut state);
        assert!(
            matches!(&state.params["pitch_source"], ParamValue::String(value) if value == "midi")
        );
        assert!(matches!(state.params["hf_damp"], ParamValue::F32(0.75)));
        assert!(matches!(state.params["mid_mix"], ParamValue::F32(35.0)));
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
    }
}
