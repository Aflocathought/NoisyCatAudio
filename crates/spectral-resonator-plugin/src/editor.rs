//! Hosted egui/wgpu editor. Only newly analysed rows are uploaded to a GPU
//! texture; two UV rectangles scroll the circular history without copying it.
//! Visibility changes recolor the retained history once, including frozen rows.
use crate::{
    analyzer::{self, Analyzer, BINS, HISTORY, SharedAnalysis},
    params::{DecayMode, PitchSource, SpectralResonatorParams},
};
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use nice_plug::context::gui::GuiContext;
use nice_plug::prelude::*;
use nice_plug_egui::{
    EguiEditor, EguiEditorState, EguiNiceSettings, Frame, NiceEguiApp, RepaintNotifier,
    create_egui_editor,
};
use std::{
    sync::{Arc, atomic::Ordering},
    time::Instant,
};

#[path = "editor_controls.rs"]
mod controls;
#[path = "editor_curve.rs"]
mod curve;
#[path = "editor_diagnostics.rs"]
mod diagnostics;
use controls::{Controls, Units, significant};

const BLUE: Color32 = Color32::from_rgb(53, 147, 255);
const INK: Color32 = Color32::from_rgb(8, 15, 26);
const WHITE: Color32 = Color32::from_rgb(232, 242, 255);
const CROSSOVER_GUIDE_SPACE: f32 = 18.0;
// Two extra rows keep the moving viewport and its linear-filter footprint
// away from the newest row at the top, while retaining 128 visible time rows.
const TEXTURE_ROWS: usize = HISTORY + 2;

#[cfg(test)]
#[path = "editor_tests.rs"]
mod tests;

pub fn create(
    params: Arc<SpectralResonatorParams>,
    shared: Arc<SharedAnalysis>,
    state: Arc<EguiEditorState>,
) -> Option<EguiEditor<ResonatorEditor>> {
    create_egui_editor(
        state,
        RepaintNotifier::default(),
        EguiNiceSettings::new()
            .with_tile("Specatral Resonator")
            .with_graphics_config(
                nice_plug_egui::GraphicsConfig::default().with_unthrottled_presentation(),
            )
            .with_resize_hint(
                nice_plug::editor::ResizeHint::resizable()
                    .with_min_logical_size(nice_plug::editor::dpi::LogicalSize::new(980.0, 700.0)),
            ),
        ResonatorEditor::new(params, shared),
    )
}

pub struct ResonatorEditor {
    params: Arc<SpectralResonatorParams>,
    shared: Arc<SharedAnalysis>,
    gui: Option<GuiContext>,
    analyzer: Analyzer,
    texture: Option<egui::TextureHandle>,
    // Store each layer's mapped brightness on the UI thread. Toggling a layer
    // restores all history without rerunning FFTs or logarithms, even in Freeze.
    spectrum_history: Vec<[f32; 2]>,
    history_gaps: [bool; TEXTURE_ROWS],
    visible_layers: [bool; 2],
    head: usize,
    rows: usize,
    freeze: bool,
    tab: usize,
    gestures: [bool; 2],
    controls: Controls,
    preferences: crate::preferences::EditorPreferences,
    settings_open: bool,
    stats: diagnostics::FrameStats,
    last_frame: Option<u64>,
    analysis_ms: f64,
    last_audio_time: f64,
    scroll_cursor: f32,
    selected_point: usize,
    curve_drag: Option<usize>,
    #[cfg(test)]
    plot_rect: Rect,
    #[cfg(test)]
    curve_rect: Rect,
    #[cfg(test)]
    layer_rects: [Rect; 2],
}

impl ResonatorEditor {
    fn key_capture(&self) -> nice_plug_egui::KeyCapture {
        if self.controls.editing_text() {
            nice_plug_egui::KeyCapture::CaptureAll
        } else {
            nice_plug_egui::KeyCapture::IgnoreKeys(vec![nice_plug_egui::Key::Character(" ".into())])
        }
    }

    fn new(params: Arc<SpectralResonatorParams>, shared: Arc<SharedAnalysis>) -> Self {
        Self {
            params,
            shared,
            gui: None,
            analyzer: Analyzer::new(),
            texture: None,
            spectrum_history: vec![[0.0; 2]; BINS * TEXTURE_ROWS],
            history_gaps: [false; TEXTURE_ROWS],
            visible_layers: [true; 2],
            head: 0,
            rows: 0,
            freeze: false,
            tab: 0,
            gestures: [false; 2],
            controls: Controls::default(),
            preferences: Default::default(),
            settings_open: false,
            stats: Default::default(),
            last_frame: None,
            analysis_ms: 0.0,
            last_audio_time: 0.0,
            scroll_cursor: 0.0,
            selected_point: 0,
            curve_drag: None,
            #[cfg(test)]
            plot_rect: Rect::NOTHING,
            #[cfg(test)]
            curve_rect: Rect::NOTHING,
            #[cfg(test)]
            layer_rects: [Rect::NOTHING; 2],
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui, gui: &GuiContext) {
        let start = Instant::now();
        self.preferences.poll();
        self.controls.begin_frame();
        let frame = ui.ctx().cumulative_frame_nr();
        let new_frame = self.last_frame != Some(frame);
        if new_frame {
            self.stats.begin(ui.input(|s| s.time));
            self.last_frame = Some(frame);
        }
        egui::Frame::new()
            .fill(Color32::from_rgb(12, 20, 32))
            .inner_margin(14.0)
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                self.draw_contents(ui, gui);
            });
        self.settings(ui.ctx(), gui);
        self.preferences.repository_confirmation(ui.ctx());
        self.controls.finish_frame(gui);
        if new_frame {
            self.stats
                .finish(start.elapsed().as_secs_f64() * 1000.0, self.analysis_ms);
        }
    }

    fn draw_contents(&mut self, ui: &mut egui::Ui, gui: &GuiContext) {
        ui.spacing_mut().item_spacing = Vec2::new(12.0, 8.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("SPECATRAL / RESONATOR")
                    .size(23.0)
                    .strong()
                    .color(WHITE),
            );
            ui.label(
                egui::RichText::new(concat!(env!("CARGO_PKG_VERSION"), " / STEREO"))
                    .small()
                    .color(BLUE),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Settings").clicked() {
                    self.settings_open = !self.settings_open;
                }
                ui.toggle_value(&mut self.freeze, "Freeze");
                for (index, label, color, visible) in [
                    (1, "WET", WHITE, &self.params.ui_show_wet),
                    (0, "DRY", BLUE, &self.params.ui_show_dry),
                ] {
                    let response = spectrum_layer_toggle(ui, label, color, visible);
                    #[cfg(test)]
                    {
                        self.layer_rects[index] = response.rect;
                    }
                    #[cfg(not(test))]
                    let _ = (index, response);
                }
            });
        });
        self.shared.enabled.store(!self.freeze, Ordering::Relaxed);
        ui.add_space(6.0);
        let (plot, highlighted) = self.waterfall(ui, gui);
        self.crossover_strip(ui, gui, plot, highlighted);
        let setter = gui.param_setter();
        let p = self.params.clone();
        ui.separator();
        ui.horizontal(|ui| {
            for (index, name) in ["RESONANCE", "DECAY CURVE", "MOTION / UNISON", "ROUTING"]
                .iter()
                .enumerate()
            {
                ui.selectable_value(&mut self.tab, index, *name);
            }
        });
        ui.add_space(4.0);
        egui::ScrollArea::vertical().id_salt("controls").auto_shrink([false, false]).max_height((ui.available_height() - 36.0).max(60.0)).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            match self.tab {
                0 => ui.columns(3, |c| {
                    choice(&mut c[0], &p.pitch_source, "Pitch source", &setter);
                    // Visibility changes do not write or reset any parameters.
                    if p.pitch_source.value() == PitchSource::Internal {
                        self.controls.knob(&mut c[0], &p.root_note, "Root note", Units::plain("MIDI"), &setter);
                    } else {
                        self.controls.knob(&mut c[0], &p.max_polyphony, "Maximum polyphony", Units::plain("voices"), &setter);
                    }
                    if p.attack_mode.value() == crate::params::AttackMode::Reshape {
                        self.controls.knob(&mut c[0], &p.attack_emphasis_db, "Transient emphasis", Units::plain("dB"), &setter);
                        c[0].small("Shapes the resonant wet sound only. No dry attack is added.");
                    }
                    self.controls.knob(&mut c[1], &p.harmonics, "Maximum partials", Units::plain("partials"), &setter);
                    self.controls.knob(&mut c[1], &p.input_send_db, "Excitation", Units::plain("dB"), &setter);
                    choice(&mut c[1], &p.attack_mode, "Attack response", &setter);
                    if p.attack_mode.value() != crate::params::AttackMode::Natural {
                        self.controls.knob(&mut c[1], &p.attack_ms, "Attack / 90%", Units::plain("ms"), &setter);
                    }
                    if p.attack_mode.value() == crate::params::AttackMode::Reshape {
                        c[1].small("Short Attack sharpens wet onset. Emphasis briefly boosts each detected strike; existing tails keep ringing.");
                    } else {
                        c[1].small("Natural follows decay. Independent shapes each rising partial; 0 ms is fastest. FFT still softens transients.");
                    }
                    self.controls.knob(&mut c[2], &p.voice_spread, "MIDI voice spread", Units::plain("%"), &setter);
                    let muted = p.panic.value();
                    if c[2].add(egui::Button::new(if muted { "WET MUTED / PANIC" } else { "Mute wet / panic" }).selected(muted)).clicked() {
                        setter.begin_set_parameter(&p.panic); setter.set_parameter(&p.panic, !muted); setter.end_set_parameter(&p.panic);
                    }
                    self.controls.knob(&mut c[2], &p.align_wet, "Wet alignment", Units::plain("windows"), &setter);
                    let active = self.shared.fft.samples();
                    let fft_samples = if active > 0 { active } else { p.fft_size.value().samples() as u32 };
                    let samples = (p.align_wet.value() * fft_samples as f32).round();
                    let rate = if active > 0 { self.shared.fft.rate() } else { self.analyzer.rate };
                    c[2].small(format!("{:.2} ms wet delay · 0: original · 0.5: half window · 1: full window", samples * 1000.0 / rate));
                    if p.attack_mode.value() == crate::params::AttackMode::Reshape {
                        c[2].small("For tight onsets, set Wet alignment to 0. Post Unison can add its own delay.");
                    }
                }),
                1 => self.decay_page(ui, gui),
                2 => ui.columns(3, |c| {
                    choice(&mut c[0], &p.mod_mode, "Motion", &setter);
                    self.controls.knob(&mut c[0], &p.mod_rate_hz, "Rate", Units::plain("Hz"), &setter);
                    self.controls.knob(&mut c[0], &p.mod_amount, "Amount", Units::plain("%"), &setter);
                    self.controls.knob(&mut c[1], &p.mod_pitch_semitones, "Pitch modulation", Units::plain("st"), &setter);
                    self.controls.knob(&mut c[1], &p.grain_ms, "Grain decay", Units::plain("ms"), &setter);
                    choice(&mut c[2], &p.unison_mode, "Unison algorithm", &setter);
                    self.controls.knob(&mut c[2], &p.unison_voices, "Unison voices", Units::plain("voices"), &setter);
                    self.controls.knob(&mut c[2], &p.unison_detune_cents, "Detune", Units::plain("ct"), &setter);
                }),
                _ => {
                    ui.columns(3, |c| {
                        choice(&mut c[0], &p.output_mode, "Main output", &setter);
                        self.controls.knob(&mut c[1], &p.output_gain, "Output gain", Units::GAIN, &setter);
                    });
                    ui.label("Wet only sends the post-Unison sound into your host's effects.");
                    ui.label("Stereo + Wet provides an additional wet output for a separate host chain.");
                    ui.label("For parallel processing, choose Dry contribution and process the Wet output separately.");
                    ui.label("The white spectrum shows this plugin's wet signal before any later host effects.");
                }
            }
        });
        ui.separator();
        ui.label(egui::RichText::new("Drag knobs or graph handles • Shift: fine adjustment • Click a value to type • Double-click a knob to reset").small().color(Color32::from_gray(150)));
    }

    fn crossover_strip(
        &mut self,
        ui: &mut egui::Ui,
        gui: &GuiContext,
        plot: Rect,
        highlighted: [bool; 2],
    ) {
        // Keep the connectors outside the spectrum so their curved portion
        // cannot be mistaken for a frequency boundary in the audio history.
        ui.add_space(CROSSOVER_GUIDE_SPACE);
        let (strip, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 70.0), Sense::hover());
        let row = Rect::from_min_size(strip.min, Vec2::new(strip.width(), 44.0));
        let side_width = 164.0;
        let middle_width = 172.0;
        let gap = 16.0;
        let bounds = [
            Rect::from_min_size(row.min, Vec2::new(side_width, row.height())),
            Rect::from_min_size(
                Pos2::new(row.right() - side_width, row.top()),
                Vec2::new(side_width, row.height()),
            ),
            Rect::from_min_size(
                Pos2::new(row.center().x - middle_width * 0.5, row.top()),
                Vec2::new(middle_width, row.height()),
            ),
            Rect::from_min_size(
                Pos2::new(row.center().x + middle_width * 0.5 + gap, row.top()),
                Vec2::new(middle_width, row.height()),
            ),
        ];
        let p = self.params.clone();
        let setter = gui.param_setter();
        for (i, (param, label, units)) in [
            (&p.low_mid_hz, "Low / Mid", Units::plain("Hz")),
            (&p.mid_high_hz, "Mid / High", Units::plain("Hz")),
            (&p.mid_mix, "Middle dry / wet", Units::plain("%")),
            (&p.m2_wet_level, "Wet level", Units::GAIN),
        ]
        .into_iter()
        .enumerate()
        {
            ui.scope_builder(
                egui::UiBuilder::new()
                    .id_salt(("crossover-strip", i))
                    .max_rect(bounds[i])
                    .layout(egui::Layout::top_down(egui::Align::Min)),
                |ui| self.controls.knob(ui, param, label, units, &setter),
            );
        }
        for (i, param) in [&p.mute_low, &p.mute_high].into_iter().enumerate() {
            let muted = param.value();
            let button = Rect::from_min_size(
                Pos2::new(bounds[i].left(), row.bottom() + 4.0),
                Vec2::new(side_width, 22.0),
            );
            if ui
                .put(
                    button,
                    egui::Button::new(if muted { "Muted" } else { "Mute" }).selected(muted),
                )
                .on_hover_text(if i == 0 {
                    "Mute the low-frequency dry band"
                } else {
                    "Mute the high-frequency dry band"
                })
                .clicked()
            {
                setter.begin_set_parameter(param);
                setter.set_parameter(param, !muted);
                setter.end_set_parameter(param);
            }
        }
        for (i, param) in [&p.low_mid_hz, &p.mid_high_hz].into_iter().enumerate() {
            let anchor_x = if i == 0 {
                bounds[i].right() + 9.0
            } else {
                bounds[i].left() - 9.0
            };
            let target_x = plot.left()
                + analyzer::fraction(param.value(), self.analyzer.rate).clamp(0.0, 1.0)
                    * plot.width();
            // Use the open space beside the knobs for the lower bend. A
            // shallow 14 px overlap stays clear of their labels/value rows.
            let elbow_y = row.top() + 14.0;
            // The visible knob is a radius-19 circle centred in the 44 px
            // row. Align the endpoint to that circle's bottom, not the text.
            let anchor = Pos2::new(anchor_x, row.center().y + 19.0);
            let mut points = Vec::with_capacity(50);
            points.push(anchor);
            // Quintic smoothstep joins the straight stems with zero slope
            // and curvature in x. The lower bend shares the knobs' height
            // instead of needing a separate tall strip above them.
            for step in 0..=48 {
                let t = step as f32 / 48.0;
                let sigmoid = t * t * t * (t * (6.0 * t - 15.0) + 10.0);
                points.push(Pos2::new(
                    anchor_x + (target_x - anchor_x) * sigmoid,
                    elbow_y + (plot.bottom() - elbow_y) * t,
                ));
            }
            let active = highlighted[i]
                || ui.input(|s| {
                    s.pointer
                        .hover_pos()
                        .is_some_and(|pos| bounds[i].contains(pos))
                });
            let color = if active {
                BLUE
            } else {
                Color32::from_rgba_unmultiplied(53, 147, 255, 90)
            };
            ui.painter()
                .add(egui::Shape::line(points, Stroke::new(1.5, color)));
            ui.painter().circle_filled(anchor, 2.0, color);
        }
    }

    fn fps_limit(&self) -> u32 {
        self.preferences.snapshot.maximum_ui_fps
    }

    fn settings(&mut self, ctx: &egui::Context, gui: &GuiContext) {
        let mut open = self.settings_open;
        egui::Window::new("Settings")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_width(430.0)
            .max_height(560.0)
            .vscroll(true)
            .show(ctx, |ui| {
                ui.label("FFT size");
                let mut selected = self.params.fft_size.value();
                let mut changed = false;
                let rate = self.shared.fft.rate();
                let rate = if rate > 0.0 { rate } else { 48_000.0 };
                egui::ComboBox::from_id_salt("fft-size")
                    .selected_text(format!("{} samples", selected.samples()))
                    .show_ui(ui, |ui| {
                        for size in crate::fft::FftSize::ALL {
                            changed |= ui
                                .selectable_value(
                                    &mut selected,
                                    size,
                                    format!(
                                        "{} samples — {:.2} ms",
                                        size.samples(),
                                        size.samples() as f32 * 1000.0 / rate
                                    ),
                                )
                                .changed();
                        }
                    });
                if changed {
                    let setter = gui.param_setter();
                    setter.begin_set_parameter(&self.params.fft_size);
                    setter.set_parameter(&self.params.fft_size, selected);
                    setter.end_set_parameter(&self.params.fft_size);
                }
                let active = self.shared.fft.samples();
                if active > 0 {
                    ui.small(format!(
                        "Active: {active} samples / {:.2} ms at {:.1} kHz",
                        active as f32 * 1000.0 / rate,
                        rate / 1000.0
                    ));
                    if active != selected.samples() as u32 {
                        ui.colored_label(
                            egui::Color32::LIGHT_YELLOW,
                            "Waiting for the host to restart audio...",
                        );
                    }
                } else {
                    ui.small("The selected FFT size applies when audio is activated.");
                }
                ui.small("Changing FFT size restarts the resonant tail. Saved with the project.");
                ui.small("Base latency shown; Wet Alignment and Post Unison may add delay.");
                ui.separator();
                let mut debug = self.params.ui_debug_fps.load(Ordering::Relaxed);
                ui.checkbox(&mut debug, "Show measured FPS on the spectrum");
                self.params.ui_debug_fps.store(debug, Ordering::Relaxed);
                ui.small("This overlay is saved with the project.");
                ui.separator();
                self.preferences.ui(ui);
            });
        self.settings_open = open;
        // A paused host can flush parameters without calling process(). Poll
        // the committed value here too, and share a deduplication flag with DSP.
        if self
            .shared
            .fft
            .request_restart(self.params.fft_size.value())
        {
            gui.request_restart();
        }
    }

    fn waterfall(&mut self, ui: &mut egui::Ui, gui: &GuiContext) -> (Rect, [bool; 2]) {
        let analysis_start = Instant::now();
        let now = ui.input(|s| s.time);
        if self.texture.is_none() {
            self.texture = Some(ui.ctx().load_texture(
                "spectral-history",
                egui::ColorImage::filled([BINS, TEXTURE_ROWS], INK),
                egui::TextureOptions::LINEAR,
            ));
        }
        let texture = self.texture.as_mut().unwrap();
        let visible = [
            self.params.ui_show_dry.load(Ordering::Relaxed),
            self.params.ui_show_wet.load(Ordering::Relaxed),
        ];
        if visible != self.visible_layers {
            // Normal frames still upload only new rows. A visibility change
            // recolors the single texture once, including its live bottom edge.
            let pixels = self
                .spectrum_history
                .iter()
                .enumerate()
                .map(|(i, &light)| spectral_color(light, self.history_gaps[i / BINS], visible))
                .collect();
            texture.set(
                egui::ColorImage::new([BINS, TEXTURE_ROWS], pixels),
                egui::TextureOptions::LINEAR,
            );
            self.visible_layers = visible;
        }
        // Bound work even when the host bounces much faster than real time.
        for _ in 0..32 {
            let Some(packet) = self.shared.queue.pop() else {
                break;
            };
            if self.freeze {
                continue;
            }
            self.last_audio_time = now;
            self.analyzer.ingest(packet, |spectrum, gap| {
                let row = &mut self.spectrum_history[self.head * BINS..(self.head + 1) * BINS];
                for (i, light) in row.iter_mut().enumerate() {
                    *light = [
                        spectral_light(spectrum[0][i]),
                        spectral_light(spectrum[1][i]),
                    ];
                }
                self.history_gaps[self.head] = gap;
                let pixels = row
                    .iter()
                    .map(|&light| spectral_color(light, gap, visible))
                    .collect();
                texture.set_partial(
                    [0, self.head],
                    egui::ColorImage::new([BINS, 1], pixels),
                    egui::TextureOptions::LINEAR,
                );
                self.head = (self.head + 1) % TEXTURE_ROWS;
                self.rows = (self.rows + 1).min(HISTORY);
            });
        }
        self.analysis_ms = analysis_start.elapsed().as_secs_f64() * 1000.0;
        // All pages share the same panel bounds, including the taller curve
        // page. Switching tabs never moves the spectrum or common controls;
        // additional window height still belongs entirely to the spectrum.
        // Reserve only the gap above the knobs. The rest of each curve uses
        // the control row itself, leaving more height for the spectrum.
        let height = (ui.available_height() - 380.0 - CROSSOVER_GUIDE_SPACE).max(180.0);
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
        let painter = ui.painter_at(rect);
        #[cfg(test)]
        {
            self.plot_rect = rect;
        }
        painter.rect_filled(rect, 4.0, INK);
        if !self.freeze && self.rows > 1 {
            // Interpolate the texture cursor between analysis rows. Capping
            // extrapolation at one row stops scrolling when audio stops.
            let phase = (self.analyzer.row_phase()
                + (now - self.last_audio_time).max(0.0) as f32 * analyzer::ROWS_PER_SECOND)
                .min(1.0);
            // Begin on old texel centers, not on the newest row's boundary.
            // The guard rows absorb one row of interpolation and its filter
            // footprint instead of letting fresh audio reappear at the top.
            self.scroll_cursor = (self.head as f32 + 0.5 + phase).rem_euclid(TEXTURE_ROWS as f32);
        }
        let before_wrap = (TEXTURE_ROWS as f32 - self.scroll_cursor).min(HISTORY as f32);
        let y = rect.top() + rect.height() * before_wrap / HISTORY as f32;
        // Read the circular history from oldest at the top to newest at the
        // bottom. Advancing the write cursor moves existing rows upward.
        if before_wrap > 0.0 {
            painter.image(
                texture.id(),
                Rect::from_min_max(rect.min, Pos2::new(rect.right(), y)),
                Rect::from_min_max(
                    Pos2::new(0.0, self.scroll_cursor / TEXTURE_ROWS as f32),
                    Pos2::new(
                        1.0,
                        (self.scroll_cursor + before_wrap) / TEXTURE_ROWS as f32,
                    ),
                ),
                Color32::WHITE,
            );
        }
        if before_wrap < HISTORY as f32 {
            painter.image(
                texture.id(),
                Rect::from_min_max(Pos2::new(rect.left(), y), rect.max),
                Rect::from_min_max(
                    Pos2::ZERO,
                    Pos2::new(1.0, (HISTORY as f32 - before_wrap) / TEXTURE_ROWS as f32),
                ),
                Color32::WHITE,
            );
        }
        if self.rows > 1 {
            // A two-pixel live edge uses the latest row's exact texel center.
            // It stays frequency-aligned at the bottom, without sampling an
            // adjacent old row or duplicating the brightness at the top.
            let latest = ((self.head + TEXTURE_ROWS - 1) % TEXTURE_ROWS) as f32;
            let v = (latest + 0.5) / TEXTURE_ROWS as f32;
            painter.image(
                texture.id(),
                Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - 2.0), rect.max),
                Rect::from_min_max(Pos2::new(0.0, v), Pos2::new(1.0, v)),
                Color32::WHITE,
            );
        }
        for seconds in 1..=analyzer::HISTORY_SECONDS.floor() as usize {
            let y = rect.bottom() - rect.height() * seconds as f32 / analyzer::HISTORY_SECONDS;
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(1.0, Color32::from_white_alpha(14)),
            );
            // The oldest label can reach the FPS overlay after flipping the
            // time axis. Keep its gridline, but avoid painting colliding text.
            if !(self.params.ui_debug_fps.load(Ordering::Relaxed) && y - rect.top() < 34.0) {
                painter.text(
                    Pos2::new(rect.right() - 8.0, y - 3.0),
                    egui::Align2::RIGHT_BOTTOM,
                    format!("−{seconds}s"),
                    egui::FontId::monospace(11.0),
                    Color32::from_gray(150),
                );
            }
        }
        for hz in [
            20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0,
        ] {
            let f = analyzer::fraction(hz, self.analyzer.rate);
            if !(0.0..=1.0).contains(&f) {
                continue;
            }
            let x = rect.left() + f * rect.width();
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0, Color32::from_white_alpha(22)),
            );
            painter.text(
                Pos2::new(x + 4.0, rect.bottom() - 14.0),
                egui::Align2::LEFT_CENTER,
                if hz >= 1000.0 {
                    format!("{}k", hz / 1000.0)
                } else {
                    format!("{hz}")
                },
                egui::FontId::monospace(11.0),
                Color32::from_gray(160),
            );
        }
        if self.rows < 2 {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "PLAY AUDIO TO EXPLORE THE SPECTRUM",
                egui::FontId::proportional(14.0),
                Color32::from_gray(150),
            );
        }
        painter.text(
            rect.left_bottom() + Vec2::new(10.0, -28.0),
            egui::Align2::LEFT_BOTTOM,
            "NOW   /   -84 → 0 dBFS",
            egui::FontId::monospace(11.0),
            WHITE,
        );
        let setter = gui.param_setter();
        let mut on_handle = false;
        let mut highlighted = [false; 2];
        for (i, param) in [&self.params.low_mid_hz, &self.params.mid_high_hz]
            .into_iter()
            .enumerate()
        {
            let x =
                rect.left() + analyzer::fraction(param.value(), self.analyzer.rate) * rect.width();
            let handle = Rect::from_min_max(
                Pos2::new(x - 9.0, rect.top()),
                Pos2::new(x + 9.0, rect.bottom()),
            );
            let response = ui
                .interact(handle, ui.id().with(("cross", i)), Sense::drag())
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            on_handle |= response.hovered() || response.dragged();
            highlighted[i] = response.hovered() || response.dragged();
            if response.drag_started() {
                setter.begin_set_parameter(param);
                self.gestures[i] = true;
            }
            if response.dragged() {
                let hz = if ui.input(|s| s.modifiers.shift) {
                    let fraction =
                        analyzer::fraction(param.unmodulated_plain_value(), self.analyzer.rate)
                            + ui.input(|s| s.pointer.delta().x) / rect.width() * 0.1;
                    analyzer::frequency(fraction, self.analyzer.rate)
                } else {
                    analyzer::frequency(
                        (response
                            .interact_pointer_pos()
                            .unwrap_or(Pos2::new(x, 0.0))
                            .x
                            - rect.left())
                            / rect.width(),
                        self.analyzer.rate,
                    )
                };
                setter.set_parameter(
                    param,
                    param.preview_plain(param.preview_normalized(hz).clamp(0.0, 1.0)),
                );
            }
            if response.drag_stopped() {
                setter.end_set_parameter(param);
                self.gestures[i] = false;
            }
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(
                    1.5,
                    if response.hovered() || response.dragged() {
                        BLUE
                    } else {
                        Color32::from_rgba_unmultiplied(53, 147, 255, 65)
                    },
                ),
            );
            if response.hovered() || response.dragged() {
                controls::tooltip(
                    &painter,
                    rect,
                    Pos2::new(x, rect.top() + 32.0),
                    format!(
                        "{}  {} Hz",
                        if i == 0 { "LOW / MID" } else { "MID / HIGH" },
                        significant(param.value() as f64)
                    ),
                );
            }
        }
        if !on_handle
            && !self.gestures.iter().any(|&active| active)
            && let Some(pos) = ui
                .input(|s| s.pointer.hover_pos())
                .filter(|pos| rect.contains(*pos))
        {
            let hz = analyzer::frequency((pos.x - rect.left()) / rect.width(), self.analyzer.rate);
            painter.line_segment(
                [
                    Pos2::new(pos.x, rect.top()),
                    Pos2::new(pos.x, rect.bottom()),
                ],
                Stroke::new(1.0, Color32::from_white_alpha(45)),
            );
            controls::tooltip(
                &painter,
                rect,
                pos + Vec2::new(0.0, 16.0),
                format!("{} Hz", significant(hz as f64)),
            );
        }
        if self.params.ui_debug_fps.load(Ordering::Relaxed) {
            painter.text(
                rect.right_top() + Vec2::new(-10.0, 10.0),
                egui::Align2::RIGHT_TOP,
                format!("{:.1} FPS / {}", self.stats.fps(), self.fps_limit()),
                egui::FontId::monospace(12.0),
                WHITE,
            );
        }
        (rect, highlighted)
    }
}

fn spectrum_layer_toggle(
    ui: &mut egui::Ui,
    label: &str,
    color: Color32,
    visible: &std::sync::atomic::AtomicBool,
) -> egui::Response {
    let mut shown = visible.load(Ordering::Relaxed);
    let font = egui::TextStyle::Body.resolve(ui.style());
    let text = ui.painter().layout_no_wrap(label.into(), font, color);
    let (rect, mut response) = ui.allocate_exact_size(
        Vec2::new(20.0 + text.size().x, text.size().y.max(24.0)),
        Sense::click(),
    );
    if response.clicked() {
        shown = !shown;
        visible.store(shown, Ordering::Relaxed);
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), shown, label)
    });
    let square = Rect::from_center_size(
        Pos2::new(rect.left() + 6.0, rect.center().y),
        Vec2::splat(12.0),
    );
    let ink = if shown {
        color
    } else {
        color.gamma_multiply(0.45)
    };
    ui.painter()
        .rect_filled(square, 1.0, if shown { color } else { INK });
    ui.painter().rect_stroke(
        square,
        1.0,
        Stroke::new(if response.hovered() { 1.5 } else { 1.0 }, ink),
        egui::StrokeKind::Inside,
    );
    // Explicit coordinates keep the square to the LEFT in the header's RTL row.
    ui.painter().galley_with_override_text_color(
        Pos2::new(rect.left() + 20.0, rect.center().y - text.size().y * 0.5),
        text,
        ink,
    );
    response.on_hover_text(format!(
        "{} {label} spectrum (display only)",
        if shown { "Hide" } else { "Show" }
    ))
}

fn spectral_light(power: f32) -> f32 {
    ((10.0 * power.max(1e-12).log10() + 84.0) / 84.0)
        .clamp(0.0, 1.0)
        .powf(1.5)
}

fn spectral_color(light: [f32; 2], gap: bool, visible: [bool; 2]) -> Color32 {
    if !visible[0] && !visible[1] {
        return INK;
    }
    if gap {
        return Color32::from_rgb(29, 36, 48);
    }
    let d = if visible[0] { light[0] } else { 0.0 };
    let w = if visible[1] { light[1] } else { 0.0 };
    let base = [8.0 + 27.0 * d, 15.0 + 108.0 * d, 26.0 + 229.0 * d];
    Color32::from_rgb(
        (base[0] + (245.0 - base[0]) * w) as u8,
        (base[1] + (249.0 - base[1]) * w) as u8,
        (base[2] + (255.0 - base[2]) * w) as u8,
    )
}

fn choice<P: Param>(ui: &mut egui::Ui, param: &P, label: &str, setter: &ParamSetter<'_>) {
    ui.label(
        egui::RichText::new(label)
            .small()
            .color(Color32::from_gray(165)),
    );
    egui::ComboBox::from_id_salt(param.as_ptr())
        .selected_text(param.to_string())
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            let steps = param.step_count().unwrap_or(1).max(1);
            for i in 0..=steps {
                let normalized = i as f32 / steps as f32;
                let name = param.normalized_value_to_string(normalized, true);
                if ui
                    .selectable_label(
                        param.unmodulated_plain_value() == param.preview_plain(normalized),
                        name,
                    )
                    .clicked()
                {
                    setter.begin_set_parameter(param);
                    setter.set_parameter_normalized(param, normalized);
                    setter.end_set_parameter(param);
                }
            }
        });
}

impl NiceEguiApp for ResonatorEditor {
    fn build(
        &mut self,
        ctx: egui::Context,
        gui: GuiContext,
        frame: &mut Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        setup_style(&ctx);
        self.preferences.connect();
        frame.set_key_capture(self.key_capture());
        frame.set_max_fps(self.fps_limit());
        self.stats = Default::default();
        self.last_frame = None;
        self.gui = Some(gui);
        self.analyzer.clear();
        self.texture = None;
        self.spectrum_history.fill([0.0; 2]);
        self.history_gaps.fill(false);
        self.head = 0;
        self.rows = 0;
        while self.shared.queue.pop().is_some() {}
        self.shared.enabled.store(!self.freeze, Ordering::Relaxed);
        Ok(())
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame) {
        if let Some(gui) = self.gui.clone() {
            self.draw(ui, &gui);
        }
        frame.set_max_fps(self.fps_limit());
        ui.ctx().request_repaint();
        frame.set_key_capture(self.key_capture());
    }
    fn editor_closed(&mut self) {
        self.stats.export(self.fps_limit());
        self.shared.enabled.store(false, Ordering::Relaxed);
        if let Some(gui) = &self.gui {
            self.controls.close(gui);
            if let Some(index) = self.curve_drag.take() {
                let point = &self.params.decay_points[index];
                gui.param_setter().end_set_parameter(&point.hz);
                gui.param_setter().end_set_parameter(&point.seconds);
            }
            let setter = gui.param_setter();
            for (i, param) in [&self.params.low_mid_hz, &self.params.mid_high_hz]
                .into_iter()
                .enumerate()
            {
                if self.gestures[i] {
                    setter.end_set_parameter(param);
                }
            }
        }
        self.gestures = [false; 2];
        self.gui = None;
        self.texture = None;
        self.preferences.disconnect();
    }
}

fn setup_style(ctx: &egui::Context) {
    let mut style = egui::Style {
        visuals: egui::Visuals::dark(),
        ..Default::default()
    };
    style.visuals.panel_fill = Color32::from_rgb(12, 20, 32);
    style.visuals.selection.bg_fill = Color32::from_rgb(31, 75, 122);
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_style_of(egui::Theme::Dark, style);
}
