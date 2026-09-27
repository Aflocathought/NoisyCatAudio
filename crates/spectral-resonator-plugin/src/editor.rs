//! Hosted egui/wgpu editor. Only newly analysed rows are uploaded to a GPU
//! texture; two UV rectangles scroll the circular history without copying it.
use crate::{
    analyzer::{self, Analyzer, BINS, HISTORY, SharedAnalysis},
    params::SpectralResonatorParams,
};
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use nice_plug::context::gui::GuiContext;
use nice_plug::prelude::*;
use nice_plug_egui::{
    EguiEditor, EguiEditorState, EguiNiceSettings, Frame, NiceEguiApp, RepaintNotifier,
    create_egui_editor, widgets::ParamSlider,
};
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

const BLUE: Color32 = Color32::from_rgb(53, 147, 255);
const INK: Color32 = Color32::from_rgb(8, 15, 26);
const WHITE: Color32 = Color32::from_rgb(232, 242, 255);

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
            .with_tile("Spectral Resonator")
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
    head: usize,
    rows: usize,
    freeze: bool,
    tab: usize,
    gestures: [bool; 2],
    #[cfg(test)]
    plot_rect: Rect,
}

impl ResonatorEditor {
    fn new(params: Arc<SpectralResonatorParams>, shared: Arc<SharedAnalysis>) -> Self {
        Self {
            params,
            shared,
            gui: None,
            analyzer: Analyzer::new(),
            texture: None,
            head: 0,
            rows: 0,
            freeze: false,
            tab: 0,
            gestures: [false; 2],
            #[cfg(test)]
            plot_rect: Rect::NOTHING,
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui, gui: &GuiContext) {
        egui::Frame::new()
            .fill(Color32::from_rgb(12, 20, 32))
            .inner_margin(14.0)
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                self.draw_contents(ui, gui);
            });
    }

    fn draw_contents(&mut self, ui: &mut egui::Ui, gui: &GuiContext) {
        ui.spacing_mut().item_spacing = Vec2::new(12.0, 8.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("SPECTRAL / RESONATOR")
                    .size(23.0)
                    .strong()
                    .color(WHITE),
            );
            ui.label(egui::RichText::new("0.9.1  /  STEREO").small().color(BLUE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.toggle_value(&mut self.freeze, "Freeze view");
                ui.colored_label(WHITE, "WET / POST UNISON");
                ui.colored_label(BLUE, "DRY INPUT");
            });
        });
        self.shared.enabled.store(!self.freeze, Ordering::Relaxed);
        ui.add_space(6.0);
        self.waterfall(ui, gui);
        ui.add_space(4.0);
        let setter = gui.param_setter();
        ui.columns(4, |columns| {
            crossover_control(
                &mut columns[0],
                &self.params.low_mid_hz,
                "LOW / MID",
                &setter,
                &mut self.gestures[0],
            );
            crossover_control(
                &mut columns[1],
                &self.params.mid_high_hz,
                "MID / HIGH",
                &setter,
                &mut self.gestures[1],
            );
            control(
                &mut columns[2],
                &self.params.mid_mix,
                "Middle dry / wet",
                &setter,
            );
            control(
                &mut columns[2],
                &self.params.m2_wet_level,
                "Wet level",
                &setter,
            );
            control(
                &mut columns[3],
                &self.params.output_gain,
                "Output gain",
                &setter,
            );
            choice(
                &mut columns[3],
                &self.params.output_mode,
                "Main output",
                &setter,
            );
        });
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
        egui::ScrollArea::vertical().id_salt("controls").max_height((ui.available_height() - 44.0).max(60.0)).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            match self.tab {
                0 => ui.columns(3, |c| {
                    choice(&mut c[0], &self.params.pitch_source, "Pitch source", &setter);
                    control(&mut c[0], &self.params.root_note, "Root note (MIDI number)", &setter);
                    control(&mut c[0], &self.params.harmonics, "Maximum partials", &setter);
                    choice(&mut c[1], &self.params.decay_mode, "Decay model", &setter);
                    control(&mut c[1], &self.params.decay_t60, "Decay / T60", &setter);
                    control(&mut c[1], &self.params.input_send_db, "Excitation", &setter);
                    control(&mut c[2], &self.params.lf_damp, "Low damping", &setter);
                    control(&mut c[2], &self.params.hf_damp, "High damping", &setter);
                    control(&mut c[2], &self.params.panic, "Mute wet / panic", &setter);
                }),
                1 => {
                    choice(ui, &self.params.decay_mode, "Choose Curve to apply these frequency / T60 points", &setter);
                    ui.columns(6, |cols| {
                        for (i, (col, point)) in cols.iter_mut().zip(&self.params.decay_points).enumerate() {
                            col.colored_label(BLUE, format!("POINT {:02}", i + 1));
                            control(col, &point.hz, "Frequency", &setter);
                            control(col, &point.seconds, "Decay seconds", &setter);
                        }
                    });
                },
                2 => ui.columns(3, |c| {
                    choice(&mut c[0], &self.params.mod_mode, "Motion", &setter);
                    control(&mut c[0], &self.params.mod_rate_hz, "Rate", &setter);
                    control(&mut c[0], &self.params.mod_amount, "Amount", &setter);
                    control(&mut c[1], &self.params.mod_pitch_semitones, "Pitch modulation", &setter);
                    control(&mut c[1], &self.params.grain_ms, "Grain decay", &setter);
                    control(&mut c[1], &self.params.voice_spread, "MIDI voice spread", &setter);
                    choice(&mut c[2], &self.params.unison_mode, "Unison algorithm", &setter);
                    control(&mut c[2], &self.params.unison_voices, "Unison voices", &setter);
                    control(&mut c[2], &self.params.unison_detune_cents, "Detune", &setter);
                }),
                _ => {
                    ui.label(egui::RichText::new("Send the resonator into your host's effects").strong());
                    ui.label("Wet only: route the main output through Bitwig FX. Low / high dry bands are excluded.");
                    ui.label("Stereo + Wet layout: the additional Wet / Post Unison output is available to a separate host chain.");
                    ui.label("For parallel recombination, set Main output to Dry contribution and add effects to the Wet output chain.");
                    ui.label("Mixed + Wet contains the wet sound twice if both chains are summed. Select Dry contribution first.");
                    ui.label("The white spectrum measures this plugin's wet output; effects added later in Bitwig are outside this display.");
                }
            }
        });
        ui.separator();
        ui.label(egui::RichText::new("Drag crossover lines • Shift + drag for fine control • Click a value to type • Double-click to reset").small().color(Color32::from_gray(150)));
    }

    fn waterfall(&mut self, ui: &mut egui::Ui, gui: &GuiContext) {
        if self.texture.is_none() {
            self.texture = Some(ui.ctx().load_texture(
                "spectral-history",
                egui::ColorImage::filled([BINS, HISTORY], INK),
                egui::TextureOptions::LINEAR,
            ));
        }
        let texture = self.texture.as_mut().unwrap();
        // Bound work even when the host bounces much faster than real time.
        for _ in 0..32 {
            let Some(packet) = self.shared.queue.pop() else {
                break;
            };
            if self.freeze {
                continue;
            }
            self.analyzer.ingest(packet, |spectrum, gap| {
                let pixels = (0..BINS)
                    .map(|i| {
                        if gap {
                            Color32::from_rgb(29, 36, 48)
                        } else {
                            spectral_color(spectrum[0][i], spectrum[1][i])
                        }
                    })
                    .collect();
                texture.set_partial(
                    [0, self.head],
                    egui::ColorImage::new([BINS, 1], pixels),
                    egui::TextureOptions::LINEAR,
                );
                self.head = (self.head + 1) % HISTORY;
                self.rows = (self.rows + 1).min(HISTORY);
            });
        }
        // Reserve the compact controls/footer at their normal logical size.
        // Every additional window pixel belongs to the spectrum, with no cap.
        let height = (ui.available_height() - 380.0).max(180.0);
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
        let painter = ui.painter_at(rect);
        #[cfg(test)]
        {
            self.plot_rect = rect;
        }
        painter.rect_filled(rect, 4.0, INK);
        let split = self.head as f32 / HISTORY as f32;
        let y = rect.top() + rect.height() * split;
        // Reverse vertical UVs: newest row is always at the top, old rows below.
        if self.head > 0 {
            painter.image(
                texture.id(),
                Rect::from_min_max(rect.min, Pos2::new(rect.right(), y)),
                Rect::from_min_max(Pos2::new(0.0, split), Pos2::new(1.0, 0.0)),
                Color32::WHITE,
            );
        }
        if self.head < HISTORY {
            painter.image(
                texture.id(),
                Rect::from_min_max(Pos2::new(rect.left(), y), rect.max),
                Rect::from_min_max(Pos2::new(0.0, 1.0), Pos2::new(1.0, split)),
                Color32::WHITE,
            );
        }
        for seconds in 1..=analyzer::HISTORY_SECONDS.floor() as usize {
            let y = rect.top() + rect.height() * seconds as f32 / analyzer::HISTORY_SECONDS;
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(1.0, Color32::from_white_alpha(14)),
            );
            painter.text(
                Pos2::new(rect.right() - 8.0, y - 3.0),
                egui::Align2::RIGHT_BOTTOM,
                format!("−{seconds}s"),
                egui::FontId::monospace(11.0),
                Color32::from_gray(150),
            );
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
            rect.left_top() + Vec2::new(10.0, 10.0),
            egui::Align2::LEFT_TOP,
            "NOW   /   -84 → 0 dBFS",
            egui::FontId::monospace(11.0),
            WHITE,
        );
        let setter = gui.param_setter();
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
                Stroke::new(1.5, BLUE),
            );
            let label = format!(
                "{}  {:.0} Hz",
                if i == 0 { "LOW / MID" } else { "MID / HIGH" },
                param.value()
            );
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(x - 56.0, rect.top() + 28.0),
                    Vec2::new(128.0, 25.0),
                ),
                4.0,
                Color32::from_rgb(20, 46, 77),
            );
            painter.text(
                Pos2::new(x + 8.0, rect.top() + 40.0),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::monospace(11.0),
                WHITE,
            );
        }
    }
}

fn spectral_color(dry: f32, wet: f32) -> Color32 {
    fn light(power: f32) -> f32 {
        ((10.0 * power.max(1e-12).log10() + 84.0) / 84.0)
            .clamp(0.0, 1.0)
            .powf(1.5)
    }
    let d = light(dry);
    let w = light(wet);
    let base = [8.0 + 27.0 * d, 15.0 + 108.0 * d, 26.0 + 229.0 * d];
    Color32::from_rgb(
        (base[0] + (245.0 - base[0]) * w) as u8,
        (base[1] + (249.0 - base[1]) * w) as u8,
        (base[2] + (255.0 - base[2]) * w) as u8,
    )
}

fn control<P: Param>(ui: &mut egui::Ui, param: &P, label: &str, setter: &ParamSetter<'_>) {
    ui.push_id(param.as_ptr(), |ui| {
        ui.label(
            egui::RichText::new(label)
                .small()
                .color(Color32::from_gray(165)),
        );
        // ParamSlider's width excludes the editable value. Reserve its space
        // so columns cannot expand past the window's right edge.
        let value_width = ui
            .painter()
            .layout_no_wrap(param.to_string(), egui::FontId::proportional(14.0), WHITE)
            .size()
            .x
            + 28.0;
        ui.add(
            ParamSlider::for_param(param, setter)
                .with_width((ui.available_width() - value_width).clamp(16.0, 280.0)),
        );
    });
}

fn choice<P: Param>(ui: &mut egui::Ui, param: &P, label: &str, setter: &ParamSetter<'_>) {
    ui.label(
        egui::RichText::new(label)
            .small()
            .color(Color32::from_gray(165)),
    );
    egui::ComboBox::from_id_salt(param.as_ptr())
        .selected_text(param.to_string())
        .width(ui.available_width().min(320.0))
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

fn crossover_control(
    ui: &mut egui::Ui,
    param: &FloatParam,
    label: &str,
    setter: &ParamSetter<'_>,
    gesture: &mut bool,
) {
    ui.horizontal(|ui| {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(54.0), Sense::click_and_drag());
        if response.drag_started() {
            setter.begin_set_parameter(param);
            *gesture = true;
        }
        if response.dragged() {
            let step =
                ui.input(|s| -s.pointer.delta().y * if s.modifiers.shift { 0.0002 } else { 0.002 });
            setter.set_parameter_normalized(
                param,
                (param.unmodulated_normalized_value() + step).clamp(0.0, 1.0),
            );
        }
        if response.drag_stopped() {
            setter.end_set_parameter(param);
            *gesture = false;
        }
        if response.double_clicked() {
            setter.begin_set_parameter(param);
            setter.set_parameter(param, param.default_plain_value());
            setter.end_set_parameter(param);
        }
        let center = rect.center();
        ui.painter()
            .circle_filled(center, 22.0, Color32::from_rgb(22, 36, 55));
        ui.painter()
            .circle_stroke(center, 22.0, Stroke::new(1.5, BLUE));
        let angle = (param.modulated_normalized_value() * 270.0 + 135.0).to_radians();
        ui.painter().line_segment(
            [
                center + Vec2::angled(angle) * 9.0,
                center + Vec2::angled(angle) * 18.0,
            ],
            Stroke::new(2.5, WHITE),
        );
        ui.vertical(|ui| {
            control(ui, param, label, setter);
        });
    });
}

impl NiceEguiApp for ResonatorEditor {
    fn build(
        &mut self,
        ctx: egui::Context,
        gui: GuiContext,
        _: &mut Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        setup_style(&ctx);
        self.gui = Some(gui);
        self.analyzer.clear();
        self.texture = None;
        self.head = 0;
        self.rows = 0;
        while self.shared.queue.pop().is_some() {}
        self.shared.enabled.store(!self.freeze, Ordering::Relaxed);
        Ok(())
    }
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut Frame) {
        if let Some(gui) = self.gui.clone() {
            self.draw(ui, &gui);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(33));
    }
    fn editor_closed(&mut self) {
        self.shared.enabled.store(false, Ordering::Relaxed);
        if let Some(gui) = &self.gui {
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
