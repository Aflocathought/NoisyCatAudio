//! The graph queries the same decay functions as the engine. Mode switching
//! changes the visible controls only; both parameter sets remain host-owned.
use super::*;
use spectral_dsp::{MAX_DECAY_SECONDS, MIN_DECAY_SECONDS};

impl ResonatorEditor {
    pub(super) fn decay_page(&mut self, ui: &mut egui::Ui, gui: &GuiContext) {
        let params = self.params.clone();
        let setter = gui.param_setter();
        let curve_mode = params.decay_mode.value() == DecayMode::Curve;
        if !curve_mode && let Some(index) = self.curve_drag.take() {
            setter.end_set_parameter(&params.decay_points[index].hz);
            setter.end_set_parameter(&params.decay_points[index].seconds);
        }
        let width = ui.available_width();
        let gap = ui.spacing().item_spacing.x;
        let column_width = (width - 2.0 * gap) / 3.0;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(column_width * 2.0 + gap, 228.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.scope(|ui| {
                        ui.set_width(column_width);
                        choice(ui, &params.decay_mode, "Decay model", &setter);
                    });
                    self.decay_graph(ui, gui, curve_mode);
                },
            );
            ui.vertical(|ui| {
                ui.set_width(column_width);
                if curve_mode {
                    egui::ComboBox::from_id_salt("selected-decay-point")
                        .width(ui.available_width())
                        .selected_text(format!("Point {}", self.selected_point + 1))
                        .show_ui(ui, |ui| {
                            for index in 0..6 {
                                ui.selectable_value(
                                    &mut self.selected_point,
                                    index,
                                    format!("Point {}", index + 1),
                                );
                            }
                        });
                    let point = &params.decay_points[self.selected_point];
                    self.controls.knob(
                        ui,
                        &point.hz,
                        "Point frequency",
                        Units::plain("Hz"),
                        &setter,
                    );
                    self.controls.knob(
                        ui,
                        &point.seconds,
                        "Decay / T60",
                        Units::plain("s"),
                        &setter,
                    );
                    ui.small(
                        "Drag a point to set frequency and decay time. Click a point to select it.",
                    );
                } else {
                    self.controls.knob(
                        ui,
                        &params.decay_t60,
                        "Decay / T60",
                        Units::plain("s"),
                        &setter,
                    );
                    self.controls
                        .knob(ui, &params.lf_damp, "Low damping", Units::PERCENT, &setter);
                    self.controls.knob(
                        ui,
                        &params.hf_damp,
                        "High damping",
                        Units::PERCENT,
                        &setter,
                    );
                    ui.small(
                        "Damping follows partial order; each note has the same relative curve.",
                    );
                }
            });
        });
    }

    fn decay_graph(&mut self, ui: &mut egui::Ui, gui: &GuiContext, curve_mode: bool) {
        let (outer, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 180.0), Sense::hover());
        let rect = Rect::from_min_max(
            outer.min + Vec2::new(46.0, 10.0),
            outer.max - Vec2::new(12.0, 30.0),
        );
        #[cfg(test)]
        {
            self.curve_rect = rect;
        }
        let painter = ui.painter_at(outer);
        painter.rect_filled(outer, 4.0, INK);
        let seconds_y = |seconds: f32| {
            rect.bottom()
                - (seconds.clamp(MIN_DECAY_SECONDS, MAX_DECAY_SECONDS) / MIN_DECAY_SECONDS).ln()
                    / (MAX_DECAY_SECONDS / MIN_DECAY_SECONDS).ln()
                    * rect.height()
        };
        for seconds in [0.005, 0.05, 0.5, 2.0, 12.0] {
            let y = seconds_y(seconds);
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(1.0, Color32::from_white_alpha(20)),
            );
            painter.text(
                Pos2::new(rect.left() - 5.0, y),
                egui::Align2::RIGHT_CENTER,
                format!("{}s", significant(seconds as f64)),
                egui::FontId::monospace(10.0),
                Color32::from_gray(150),
            );
        }
        let ticks: &[f32] = if curve_mode {
            &[20.0, 100.0, 1000.0, 10000.0, 20000.0]
        } else {
            &[1.0, 4.0, 16.0, 64.0, 256.0, 512.0]
        };
        for &value in ticks {
            let fraction = if curve_mode {
                analyzer::fraction(value, 48000.0)
            } else {
                value.log2() / 9.0
            };
            let x = rect.left() + fraction * rect.width();
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0, Color32::from_white_alpha(16)),
            );
            let text = if curve_mode && value >= 1000.0 {
                format!("{}k", value / 1000.0)
            } else {
                format!("{value}")
            };
            painter.text(
                Pos2::new(x, rect.bottom() + 10.0),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::monospace(10.0),
                Color32::from_gray(150),
            );
        }
        painter.text(
            Pos2::new(rect.center().x, outer.bottom() - 4.0),
            egui::Align2::CENTER_BOTTOM,
            if curve_mode {
                "Frequency / Hz"
            } else {
                "Partial order / × base frequency"
            },
            egui::FontId::proportional(11.0),
            Color32::from_gray(155),
        );
        let params = self.params.clone();
        let curve = spectral_dsp::DecayCurve {
            points: std::array::from_fn(|i| spectral_dsp::DecayPoint {
                hz: params.decay_points[i].hz.value(),
                seconds: params.decay_points[i].seconds.value(),
            }),
        }
        .prepared();
        let points: Vec<_> = (0..=180)
            .map(|i| {
                let fraction = i as f32 / 180.0;
                let seconds = if curve_mode {
                    curve.seconds_at(analyzer::frequency(fraction, 48000.0))
                } else {
                    spectral_dsp::damping_seconds(
                        2.0_f32.powf(fraction * 9.0),
                        params.decay_t60.value(),
                        params.hf_damp.value(),
                        params.lf_damp.value(),
                    )
                };
                Pos2::new(rect.left() + fraction * rect.width(), seconds_y(seconds))
            })
            .collect();
        painter.add(egui::Shape::line(points, Stroke::new(2.0, BLUE)));
        if curve_mode {
            let setter = gui.param_setter();
            for (index, point) in params.decay_points.iter().enumerate() {
                let pos = Pos2::new(
                    rect.left() + analyzer::fraction(point.hz.value(), 48000.0) * rect.width(),
                    seconds_y(point.seconds.value()),
                );
                let response = ui.interact(
                    Rect::from_center_size(pos, Vec2::splat(18.0)),
                    ui.id().with(("decay-node", index)),
                    Sense::click_and_drag(),
                );
                if response.clicked() {
                    self.selected_point = index;
                }
                if response.drag_started() {
                    self.selected_point = index;
                    setter.begin_set_parameter(&point.hz);
                    setter.begin_set_parameter(&point.seconds);
                    self.curve_drag = Some(index);
                }
                if response.dragged() {
                    let (x, y) = if ui.input(|s| s.modifiers.shift) {
                        let delta = ui.input(|s| s.pointer.delta()) * 0.1;
                        (
                            analyzer::fraction(point.hz.unmodulated_plain_value(), 48000.0)
                                + delta.x / rect.width(),
                            (point.seconds.unmodulated_plain_value() / MIN_DECAY_SECONDS).ln()
                                / (MAX_DECAY_SECONDS / MIN_DECAY_SECONDS).ln()
                                - delta.y / rect.height(),
                        )
                    } else {
                        let pointer = response.interact_pointer_pos().unwrap_or(pos);
                        (
                            (pointer.x - rect.left()) / rect.width(),
                            (rect.bottom() - pointer.y) / rect.height(),
                        )
                    };
                    setter.set_parameter(&point.hz, analyzer::frequency(x, 48000.0));
                    setter.set_parameter(
                        &point.seconds,
                        MIN_DECAY_SECONDS
                            * (MAX_DECAY_SECONDS / MIN_DECAY_SECONDS).powf(y.clamp(0.0, 1.0)),
                    );
                }
                if response.drag_stopped() {
                    setter.end_set_parameter(&point.hz);
                    setter.end_set_parameter(&point.seconds);
                    self.curve_drag = None;
                }
                painter.circle_filled(
                    pos,
                    if index == self.selected_point {
                        5.0
                    } else {
                        4.0
                    },
                    if index == self.selected_point {
                        WHITE
                    } else {
                        BLUE
                    },
                );
                if response.hovered() || response.dragged() {
                    controls::tooltip(
                        &painter,
                        outer,
                        pos + Vec2::new(0.0, 12.0),
                        format!(
                            "{} Hz / {} s",
                            significant(point.hz.value() as f64),
                            significant(point.seconds.value() as f64)
                        ),
                    );
                }
            }
        }
    }
}
