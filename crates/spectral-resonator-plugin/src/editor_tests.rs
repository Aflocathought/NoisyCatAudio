//! Headless rendering uses the production egui UI and wgpu painter. It renders
//! to our own texture, without capturing or controlling any desktop window.
use super::*;
use egui_wgpu::{RenderState, RendererOptions, ScreenDescriptor, wgpu};
use nice_plug::{context::gui::GuiContextInner, params::internals::ParamPtr};

struct TestGui;
impl GuiContextInner for TestGui {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, _: ParamPtr) {}
    unsafe fn raw_set_parameter_normalized(&self, _: ParamPtr, _: f32) {}
    unsafe fn raw_end_set_parameter(&self, _: ParamPtr) {}
    fn get_state(&self) -> PluginState {
        unreachable!()
    }
    fn set_state(&self, _: PluginState) {
        unreachable!()
    }
    fn request_restart(&self) {
        unreachable!()
    }
}

struct GestureHost {
    params: Arc<SpectralResonatorParams>,
    events: std::sync::Mutex<Vec<char>>,
}
impl GuiContextInner for GestureHost {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, _: ParamPtr) {
        self.events.lock().unwrap().push('b');
    }
    unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, value: f32) {
        assert!(
            self.params
                .param_map()
                .iter()
                .any(|(_, ptr, _)| *ptr == param)
        );
        // SAFETY: The parameter pointer is validated above and owned by our
        // Arc for the entire synchronous simulated-host interaction.
        unsafe {
            param._internal_set_normalized_value(value);
        }
        self.events.lock().unwrap().push('s');
    }
    unsafe fn raw_end_set_parameter(&self, _: ParamPtr) {
        self.events.lock().unwrap().push('e');
    }
    fn get_state(&self) -> PluginState {
        unreachable!()
    }
    fn set_state(&self, _: PluginState) {
        unreachable!()
    }
    fn request_restart(&self) {
        self.events.lock().unwrap().push('r');
    }
}

#[test]
fn settings_restart_uses_committed_value_and_waits_for_host_activation() {
    let ctx = egui::Context::default();
    let params = Arc::new(SpectralResonatorParams::default());
    let host = Arc::new(GestureHost {
        params: params.clone(),
        events: Default::default(),
    });
    let gui = GuiContext::new(host.clone());
    let shared = SharedAnalysis::new();
    shared.fft.activate(crate::fft::FftSize::N4096, 48_000.0);
    let mut editor = ResonatorEditor::new(params.clone(), shared.clone());
    gui.param_setter()
        .set_parameter(&params.fft_size, crate::fft::FftSize::N3072);
    for _ in 0..3 {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            editor.settings(ui.ctx(), &gui)
        });
        output.textures_delta.clear();
    }
    assert_eq!(
        host.events
            .lock()
            .unwrap()
            .iter()
            .filter(|&&event| event == 'r')
            .count(),
        1
    );
    assert_eq!(shared.fft.samples(), 4096);
    shared.fft.activate(crate::fft::FftSize::N3072, 48_000.0);
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        editor.settings(ui.ctx(), &gui)
    });
    output.textures_delta.clear();
    assert_eq!(
        host.events
            .lock()
            .unwrap()
            .iter()
            .filter(|&&event| event == 'r')
            .count(),
        1
    );
    assert_eq!(params.fft_size.value(), crate::fft::FftSize::N3072);
}

#[test]
fn crossover_drag_sends_a_balanced_host_gesture_and_layout_fits() {
    let ctx = egui::Context::default();
    setup_style(&ctx);
    let params = Arc::new(SpectralResonatorParams::default());
    let host = Arc::new(GestureHost {
        params: params.clone(),
        events: Default::default(),
    });
    let gui = GuiContext::new(host.clone());
    let mut editor = ResonatorEditor::new(params.clone(), SharedAnalysis::new());
    gui.param_setter()
        .set_parameter(&params.attack_mode, crate::params::AttackMode::Independent);
    host.events.lock().unwrap().clear();
    let mut default_plot_height = [0.0; 4];
    for (width, height) in [(1120.0, 780.0), (1120.0, 1100.0), (980.0, 700.0)] {
        let mut first_plot = None;
        for (tab, default_height) in default_plot_height.iter_mut().enumerate() {
            editor.tab = tab;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, height))),
                    ..Default::default()
                },
                |ui| {
                    editor.draw(ui, &gui);
                    assert!(
                        ui.min_rect().right() <= width + 1.0,
                        "tab {tab} grew past width"
                    );
                    assert!(
                        ui.min_rect().bottom() <= height + 1.0,
                        "tab {tab} grew past height {height}: {:?}",
                        ui.min_rect()
                    );
                },
            );
            output.textures_delta.clear();
            if let Some(first_plot) = first_plot {
                assert_eq!(
                    editor.plot_rect, first_plot,
                    "Switching tabs moved the spectrum"
                );
            } else {
                first_plot = Some(editor.plot_rect);
            }
            if height == 780.0 {
                *default_height = editor.plot_rect.height();
            } else if height == 1100.0 {
                assert!(
                    (editor.plot_rect.height() - *default_height - 320.0).abs() < 1.0,
                    "Resizing must grow the spectrum by the full added height"
                );
            }
        }
    }
    let start = Pos2::new(
        editor.plot_rect.left()
            + analyzer::fraction(params.low_mid_hz.value(), 48000.0) * editor.plot_rect.width(),
        editor.plot_rect.center().y,
    );
    let end = start + Vec2::new(60.0, 0.0);
    for events in [
        vec![
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
        vec![egui::Event::PointerMoved(end)],
        vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    ] {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(980.0, 700.0))),
                events,
                ..Default::default()
            },
            |ui| editor.draw(ui, &gui),
        );
        output.textures_delta.clear();
    }
    assert!(params.low_mid_hz.value() > 300.0);
    let events = host.events.lock().unwrap();
    assert_eq!(events.first(), Some(&'b'));
    assert_eq!(events.last(), Some(&'e'));
    assert_eq!(events.iter().filter(|&&e| e == 'b').count(), 1);
    assert_eq!(events.iter().filter(|&&e| e == 'e').count(), 1);
    drop(events);
    let cutoffs = (params.low_mid_hz.value(), params.mid_high_hz.value());
    for (label, param) in [
        ("Low / Mid", &params.mute_low),
        ("Mid / High", &params.mute_high),
    ] {
        let knob = editor
            .controls
            .values
            .iter()
            .find(|(name, ..)| name == label)
            .unwrap()
            .2;
        let pos = Pos2::new(knob.left() + 82.0, knob.bottom() + 15.0);
        host.events.lock().unwrap().clear();
        ui_frame(
            &ctx,
            &mut editor,
            &gui,
            vec![egui::Event::PointerMoved(pos), mouse(pos, true)],
        );
        ui_frame(&ctx, &mut editor, &gui, vec![mouse(pos, false)]);
        assert!(param.value());
        assert_eq!(*host.events.lock().unwrap(), ['b', 's', 'e']);
    }
    assert_eq!(
        cutoffs,
        (params.low_mid_hz.value(), params.mid_high_hz.value())
    );
}

fn ui_frame(
    ctx: &egui::Context,
    editor: &mut ResonatorEditor,
    gui: &GuiContext,
    events: Vec<egui::Event>,
) {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(980.0, 700.0))),
            events,
            ..Default::default()
        },
        |ui| editor.draw(ui, gui),
    );
    output.textures_delta.clear();
}
fn mouse(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}

#[test]
fn hidden_controls_keep_values_curve_drag_balances_gestures_and_settings_persist() {
    let ctx = egui::Context::default();
    setup_style(&ctx);
    let params = Arc::new(SpectralResonatorParams::default());
    let host = Arc::new(GestureHost {
        params: params.clone(),
        events: Default::default(),
    });
    let gui = GuiContext::new(host.clone());
    let mut editor = ResonatorEditor::new(params.clone(), SharedAnalysis::new());
    gui.param_setter().set_parameter(&params.root_note, 65);
    gui.param_setter().set_parameter(&params.lf_damp, 0.32789);
    gui.param_setter()
        .set_parameter(&params.pitch_source, PitchSource::Midi);
    ui_frame(&ctx, &mut editor, &gui, vec![]);
    assert!(
        !editor
            .controls
            .values
            .iter()
            .any(|(name, ..)| name == "Root note")
    );
    assert!(
        editor
            .controls
            .values
            .iter()
            .any(|(name, ..)| name == "Maximum polyphony")
    );
    assert_eq!(params.root_note.value(), 65);
    gui.param_setter()
        .set_parameter(&params.attack_ms, 12.34567);
    assert!(
        !editor
            .controls
            .values
            .iter()
            .any(|(name, ..)| name == "Attack / 90%")
    );
    gui.param_setter()
        .set_parameter(&params.attack_mode, crate::params::AttackMode::Independent);
    ui_frame(&ctx, &mut editor, &gui, vec![]);
    assert!(
        editor
            .controls
            .values
            .iter()
            .any(|(name, ..)| name == "Attack / 90%")
    );
    gui.param_setter()
        .set_parameter(&params.attack_mode, crate::params::AttackMode::Natural);
    ui_frame(&ctx, &mut editor, &gui, vec![]);
    assert!((params.attack_ms.value() - 12.34567).abs() < 0.0001);
    gui.param_setter()
        .set_parameter(&params.attack_emphasis_db, 7.12345);
    gui.param_setter()
        .set_parameter(&params.attack_mode, crate::params::AttackMode::Reshape);
    ui_frame(&ctx, &mut editor, &gui, vec![]);
    assert!(
        editor
            .controls
            .values
            .iter()
            .any(|(name, ..)| name == "Transient emphasis")
    );
    gui.param_setter()
        .set_parameter(&params.attack_mode, crate::params::AttackMode::Natural);
    ui_frame(&ctx, &mut editor, &gui, vec![]);
    assert!(
        !editor
            .controls
            .values
            .iter()
            .any(|(name, ..)| name == "Transient emphasis")
    );
    assert!((params.attack_emphasis_db.value() - 7.12345).abs() < 0.0001);
    assert!((params.attack_ms.value() - 12.34567).abs() < 0.0001);
    editor.tab = 1;
    gui.param_setter()
        .set_parameter(&params.decay_mode, DecayMode::Curve);
    ui_frame(&ctx, &mut editor, &gui, vec![]);
    assert!(
        !editor
            .controls
            .values
            .iter()
            .any(|(name, ..)| name == "Low damping")
    );
    assert_eq!(params.lf_damp.value(), 0.32789);
    let rect = editor.curve_rect;
    let point = &params.decay_points[1];
    let start = Pos2::new(
        rect.left() + analyzer::fraction(point.hz.value(), 48000.0) * rect.width(),
        rect.bottom()
            - (point.seconds.value() / 0.05).ln() / (12.0_f32 / 0.05).ln() * rect.height(),
    );
    let end = start + Vec2::new(35.0, -20.0);
    host.events.lock().unwrap().clear();
    ui_frame(
        &ctx,
        &mut editor,
        &gui,
        vec![egui::Event::PointerMoved(start), mouse(start, true)],
    );
    ui_frame(
        &ctx,
        &mut editor,
        &gui,
        vec![egui::Event::PointerMoved(end)],
    );
    ui_frame(&ctx, &mut editor, &gui, vec![mouse(end, false)]);
    assert!(point.hz.value() > 250.0 && point.seconds.value() > 2.0);
    let saved_point = (point.hz.value(), point.seconds.value());
    let events = host.events.lock().unwrap().clone();
    assert_eq!(events.iter().filter(|&&e| e == 'b').count(), 2);
    assert_eq!(events.iter().filter(|&&e| e == 'e').count(), 2);
    gui.param_setter()
        .set_parameter(&params.decay_mode, DecayMode::Damping);
    ui_frame(&ctx, &mut editor, &gui, vec![]);
    assert_eq!((point.hz.value(), point.seconds.value()), saved_point);
    assert_eq!(params.lf_damp.value(), 0.32789);
    params.ui_max_fps.store(120, Ordering::Relaxed);
    params.ui_debug_fps.store(true, Ordering::Relaxed);
    let fields = params.serialize_fields();
    let loaded = SpectralResonatorParams::default();
    loaded.deserialize_fields(&fields);
    assert_eq!(loaded.ui_max_fps.load(Ordering::Relaxed), 120);
    assert!(loaded.ui_debug_fps.load(Ordering::Relaxed));
}

#[test]
fn slow_integer_knob_drag_accumulates_fractional_steps() {
    let ctx = egui::Context::default();
    setup_style(&ctx);
    let params = Arc::new(SpectralResonatorParams::default());
    let host = Arc::new(GestureHost {
        params: params.clone(),
        events: Default::default(),
    });
    let gui = GuiContext::new(host.clone());
    gui.param_setter()
        .set_parameter(&params.pitch_source, PitchSource::Midi);
    let mut editor = ResonatorEditor::new(params.clone(), SharedAnalysis::new());
    ui_frame(&ctx, &mut editor, &gui, vec![]);
    let start = editor
        .controls
        .values
        .iter()
        .find(|(name, ..)| name == "Maximum polyphony")
        .unwrap()
        .2
        .center();
    host.events.lock().unwrap().clear();
    ui_frame(
        &ctx,
        &mut editor,
        &gui,
        vec![egui::Event::PointerMoved(start), mouse(start, true)],
    );
    // Each individual pixel is below half of one integer step. The complete
    // gesture must still change the value, without allocating new host gestures.
    for pixels in 1..=45 {
        ui_frame(
            &ctx,
            &mut editor,
            &gui,
            vec![egui::Event::PointerMoved(
                start + Vec2::new(0.0, pixels as f32),
            )],
        );
    }
    ui_frame(
        &ctx,
        &mut editor,
        &gui,
        vec![mouse(start + Vec2::new(0.0, 45.0), false)],
    );
    assert!(params.max_polyphony.value() < 16);
    let events = host.events.lock().unwrap();
    assert_eq!(events.iter().filter(|&&e| e == 'b').count(), 1);
    assert_eq!(events.iter().filter(|&&e| e == 'e').count(), 1);
}

#[test]
fn typing_more_than_five_digits_changes_the_full_parameter_value() {
    let ctx = egui::Context::default();
    setup_style(&ctx);
    let params = Arc::new(SpectralResonatorParams::default());
    let host = Arc::new(GestureHost {
        params: params.clone(),
        events: Default::default(),
    });
    let gui = GuiContext::new(host);
    let mut editor = ResonatorEditor::new(params.clone(), SharedAnalysis::new());
    assert_eq!(
        editor.key_capture(),
        nice_plug_egui::KeyCapture::IgnoreKeys(vec![nice_plug_egui::Key::Character(" ".into())])
    );
    ui_frame(&ctx, &mut editor, &gui, vec![]);
    let pos = editor
        .controls
        .values
        .iter()
        .find(|(name, ..)| name == "Low / Mid")
        .unwrap()
        .1
        .center();
    ui_frame(
        &ctx,
        &mut editor,
        &gui,
        vec![egui::Event::PointerMoved(pos), mouse(pos, true)],
    );
    ui_frame(&ctx, &mut editor, &gui, vec![mouse(pos, false)]);
    ui_frame(&ctx, &mut editor, &gui, vec![]);
    let ctrl = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    assert_eq!(editor.key_capture(), nice_plug_egui::KeyCapture::CaptureAll);
    ui_frame(
        &ctx,
        &mut editor,
        &gui,
        vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: ctrl,
        }],
    );
    ui_frame(
        &ctx,
        &mut editor,
        &gui,
        vec![egui::Event::Text("345.67891".into())],
    );
    ui_frame(
        &ctx,
        &mut editor,
        &gui,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
    );
    assert!(
        (params.low_mid_hz.value() - 345.6789).abs() < 0.0001,
        "typed value: {}",
        params.low_mid_hz.value()
    );
    assert_eq!(significant(params.low_mid_hz.value() as f64), "345.68");
    assert_eq!(
        editor.key_capture(),
        nice_plug_egui::KeyCapture::IgnoreKeys(vec![nice_plug_egui::Key::Character(" ".into())])
    );
}

#[test]
#[ignore = "requires a GPU; writes diagnostic UI images under target/ui-preview"]
fn render_gpu_preview() {
    let ctx = egui::Context::default();
    setup_style(&ctx);
    // These are independent still-frame fixtures. A closing Settings window
    // must not fade over the next fixture's waterfall pixel assertions.
    ctx.style_mut_of(egui::Theme::Dark, |style| style.animation_time = 0.0);
    let gui = GuiContext::new(Arc::new(TestGui));
    let shared = SharedAnalysis::new();
    shared.fft.activate(crate::fft::FftSize::N4096, 48_000.0);
    let mut capture = crate::analyzer::AudioCapture::new(shared.clone());
    let preview_params = SpectralResonatorParams {
        attack_mode: EnumParam::new("Attack Response", crate::params::AttackMode::Independent),
        unison_mode: EnumParam::new("Unison Mode", crate::params::UnisonMode::Post),
        unison_voices: IntParam::new("Unison", 4, IntRange::Linear { min: 1, max: 8 }),
        ..Default::default()
    };
    let mut editor = ResonatorEditor::new(Arc::new(preview_params), shared);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let render = pollster::block_on(RenderState::create(
        &Default::default(),
        &instance,
        None,
        RendererOptions::default(),
    ))
    .unwrap();
    eprintln!("Preview GPU: {:?}", render.adapter.get_info());
    let device = &render.device;
    let queue = &render.queue;
    let mut renderer = render.renderer.write();
    let mut engine = spectral_dsp::SpectralResonator::new(2, 4096, 512, 48000.0).unwrap();
    let controls = spectral_dsp::ResonatorControls {
        note: Some(57),
        wet_level: 4.0,
        align_wet: 0.5,
        unison_mode: spectral_dsp::UnisonMode::Post,
        modulation: spectral_dsp::ModulationControls {
            unison_voices: 4,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut seed = 7u32;
    // Eight seconds of a sweep plus stereo noise, processed by the actual DSP.
    // Upload each frame's deltas; no invented waterfall pixels are used.
    for block in 0..240 {
        for i in 0..1600 {
            let t = (block * 1600 + i) as f32 / 48000.0;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = (seed as f32 / u32::MAX as f32 - 0.5) * 0.06;
            let input = [
                noise + 0.15 * (std::f32::consts::TAU * (100.0 * t + 60.0 * t * t)).sin(),
                noise,
            ];
            let mut audio = input;
            engine.process_frame(&mut audio, controls);
            capture.push(input, engine.output_parts().1, true);
        }
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1120.0, 780.0))),
                time: Some(block as f64 / 30.0),
                ..Default::default()
            },
            |ui| editor.draw(ui, &gui),
        );
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                renderer.update_texture(device, queue, *id, delta);
            }
        }
        for id in &output.textures_delta.free {
            renderer.free_texture(id);
        }
        output.textures_delta.clear();
    }
    assert_eq!(editor.rows, HISTORY);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-preview");
    std::fs::create_dir_all(&root).unwrap();
    for (name, tab, width, height) in [
        ("tab-0", 0, 1120u32, 780u32),
        ("tab-1", 1, 980, 700),
        ("tab-2", 2, 1120, 780),
        ("tab-3", 3, 1120, 780),
        ("tall", 0, 1120, 1100),
        ("curve", 1, 980, 700),
        ("reshape", 0, 980, 700),
        ("settings", 0, 1120, 780),
        ("guide-min", 0, 980, 700),
        ("guide-max", 3, 980, 700),
        ("edge-start", 0, 980, 700),
        ("edge-half", 0, 980, 700),
        ("edge-wrap", 0, 980, 700),
        ("edge-middle", 0, 980, 700),
    ] {
        editor.tab = tab;
        // This offline preview host does not process parameter output events.
        // SAFETY: the renderer exclusively owns these parameters here.
        unsafe {
            // Exercise the guide routing at both parameter limits as well
            // as the normal layout, using the production controls/painter.
            for (param, default) in [
                (&editor.params.low_mid_hz, 250.0),
                (&editor.params.mid_high_hz, 4000.0),
            ] {
                param.as_ptr()._internal_set_normalized_value(match name {
                    "guide-min" => 0.0,
                    "guide-max" => 1.0,
                    _ => param.preview_normalized(default),
                });
            }
            editor
                .params
                .mute_low
                .as_ptr()
                ._internal_set_normalized_value(if name == "guide-min" { 1.0 } else { 0.0 });
            editor
                .params
                .mute_high
                .as_ptr()
                ._internal_set_normalized_value(if name == "guide-max" { 1.0 } else { 0.0 });
            editor
                .params
                .attack_mode
                .as_ptr()
                ._internal_set_normalized_value(if name == "reshape" { 1.0 } else { 0.5 });
            editor
                .params
                .align_wet
                .as_ptr()
                ._internal_set_normalized_value(if name == "reshape" { 0.0 } else { 0.5 });
        }
        editor.settings_open = name == "settings";
        editor
            .params
            .ui_debug_fps
            .store(name == "settings", Ordering::Relaxed);
        let edge_fixture = match name {
            "edge-start" => Some((0, 0.0)),
            "edge-half" => Some((0, 0.5)),
            "edge-wrap" => Some((1, 1.0)),
            "edge-middle" => Some((65, 0.5)),
            _ => None,
        };
        if let Some((head, phase)) = edge_fixture {
            // Regression fixture: silence in every old row, a loud current
            // row only. Inspect GPU pixels so a wrap/filter seam cannot pass
            // just because CPU coordinates look plausible.
            editor.freeze = true;
            editor.head = head;
            editor.scroll_cursor = (head as f32 + 0.5 + phase) % TEXTURE_ROWS as f32;
            let mut pixels = vec![INK; BINS * TEXTURE_ROWS];
            let latest = (head + TEXTURE_ROWS - 1) % TEXTURE_ROWS;
            pixels[latest * BINS..(latest + 1) * BINS].fill(Color32::WHITE);
            editor.texture.as_mut().unwrap().set(
                egui::ColorImage::new([BINS, TEXTURE_ROWS], pixels),
                egui::TextureOptions::LINEAR,
            );
        }
        if name == "curve" {
            // SAFETY: the preview owns this parameter throughout rendering.
            unsafe {
                editor
                    .params
                    .decay_mode
                    .as_ptr()
                    ._internal_set_normalized_value(1.0);
            }
        }
        if name == "settings" {
            // Floating windows first measure themselves and fade in. Render a
            // warmup frame instead of capturing that intentionally hidden pass.
            let mut warmup = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        Pos2::ZERO,
                        Vec2::new(width as f32, height as f32),
                    )),
                    time: Some(9.0),
                    ..Default::default()
                },
                |ui| editor.draw(ui, &gui),
            );
            for (id, deltas) in &warmup.textures_delta.set {
                for delta in deltas {
                    renderer.update_texture(device, queue, *id, delta);
                }
            }
            warmup.textures_delta.clear();
        }
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(width as f32, height as f32),
                )),
                time: (name == "settings").then_some(9.3),
                ..Default::default()
            },
            |ui| editor.draw(ui, &gui),
        );
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                renderer.update_texture(device, queue, *id, delta);
            }
        }
        output.textures_delta.clear();
        let shapes = ctx.tessellate(output.shapes, output.pixels_per_point);
        let screen = ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: output.pixels_per_point,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("UI test target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: render.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        let commands = renderer.update_buffers(device, queue, &mut encoder, &shapes, &screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            renderer.render(&mut pass.forget_lifetime(), &shapes, &screen);
        }
        let stride = (width * 4).div_ceil(256) * 256;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (stride * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(height),
                },
            },
            texture.size(),
        );
        queue.submit(commands.into_iter().chain([encoder.finish()]));
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            tx.send(r).unwrap();
        });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let mapped = readback.slice(..).get_mapped_range().unwrap();
        let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
        let pixels_start = ppm.len();
        for row in mapped.chunks(stride as usize) {
            for rgba in row[..width as usize * 4].chunks(4) {
                if matches!(
                    render.target_format,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
                ) {
                    ppm.extend_from_slice(&[rgba[2], rgba[1], rgba[0]]);
                } else {
                    ppm.extend_from_slice(&rgba[..3]);
                }
            }
        }
        if edge_fixture.is_some() {
            let pixel = |x: usize, y: usize| {
                let index = pixels_start + (y * width as usize + x) * 3;
                &ppm[index..index + 3]
            };
            for fraction in [0.3, 0.5, 0.7] {
                let x = (editor.plot_rect.left() + fraction * editor.plot_rect.width()) as usize;
                for offset in [0, 1] {
                    let top = pixel(x, editor.plot_rect.top() as usize + offset);
                    assert!(
                        top.iter().all(|&v| v < 140),
                        "{name}: current row leaked to top: {top:?}"
                    );
                }
                let bottom = pixel(x, editor.plot_rect.bottom() as usize - 1);
                assert!(
                    bottom.iter().all(|&v| v > 200),
                    "{name}: live bottom edge missing: {bottom:?}"
                );
            }
        }
        std::fs::write(root.join(format!("{name}.ppm")), ppm).unwrap();
        drop(mapped);
        readback.unmap();
    }
    editor.editor_closed();
    assert!(!editor.shared.enabled.load(Ordering::Relaxed));
    assert!(editor.texture.is_none());
}
