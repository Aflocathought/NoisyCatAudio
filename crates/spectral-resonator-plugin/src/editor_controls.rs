//! Compact host-bound controls. Display rounding never feeds back into the
//! parameter: text editing starts from the full value and commits separately.
use egui::{Color32, Rect, Sense, Stroke, Vec2};
use nice_plug::prelude::*;
use std::fmt::Display;

#[derive(Clone, Copy)]
pub struct Units {
    pub suffix: &'static str,
    factor: f64,
    gain: bool,
}
impl Units {
    pub const fn plain(suffix: &'static str) -> Self {
        Self {
            suffix,
            factor: 1.0,
            gain: false,
        }
    }
    pub const PERCENT: Self = Self {
        suffix: "%",
        factor: 100.0,
        gain: false,
    };
    pub const GAIN: Self = Self {
        suffix: "dB",
        factor: 1.0,
        gain: true,
    };
    fn display(self, value: f64) -> f64 {
        if self.gain {
            if value > 0.0 {
                20.0 * value.log10()
            } else {
                f64::NEG_INFINITY
            }
        } else {
            value * self.factor
        }
    }
    fn plain_value(self, text: &str) -> Option<f64> {
        let text = text.trim().replace('−', "-");
        let text = text.trim_end_matches(self.suffix).trim();
        if self.gain && matches!(text, "-inf" | "-∞") {
            return Some(0.0);
        }
        let (text, scale) = if self.suffix == "Hz" && text.ends_with('k') {
            (text.trim_end_matches('k').trim(), 1000.0)
        } else {
            (text, 1.0)
        };
        let value = text.parse::<f64>().ok()? * scale;
        if !value.is_finite() {
            return None;
        }
        let value = if self.gain {
            10.0_f64.powf(value / 20.0)
        } else {
            value / self.factor
        };
        value.is_finite().then_some(value)
    }
}

pub fn significant(value: f64) -> String {
    if value == f64::NEG_INFINITY {
        return "−∞".into();
    }
    if !value.is_finite() {
        return "—".into();
    }
    if value == 0.0 {
        return "0".into();
    }
    let scale = 10.0_f64.powi(4 - value.abs().log10().floor() as i32);
    let rounded = (value * scale).round() / scale;
    let exponent = rounded.abs().log10().floor() as i32;
    if !(-3..5).contains(&exponent) {
        let mantissa = format!("{:.4}", rounded / 10.0_f64.powi(exponent));
        format!(
            "{}e{exponent}",
            mantissa.trim_end_matches('0').trim_end_matches('.')
        )
    } else {
        let text = format!("{:.*}", (4 - exponent).max(0) as usize, rounded);
        if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.').into()
        } else {
            text
        }
    }
}

pub fn parse<P: Param>(param: &P, text: &str, units: Units) -> Option<f32> {
    let value = units.plain_value(text)?;
    param
        .string_to_normalized_value(&value.to_string())
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(0.0, 1.0))
}

struct Edit {
    id: egui::Id,
    text: String,
    changed: bool,
}
#[derive(Default)]
pub struct Controls {
    edit: Option<Edit>,
    // Store the pointer rather than a borrowed parameter so closing a window
    // can balance an interrupted host gesture without retaining a Ui borrow.
    active: Option<nice_plug::params::internals::ParamPtr>,
    drag_value: f32,
    active_seen: bool,
    #[cfg(test)]
    pub values: Vec<(String, Rect, Rect)>,
}
impl Controls {
    pub fn editing_text(&self) -> bool {
        self.edit.is_some()
    }
    pub fn begin_frame(&mut self) {
        self.active_seen = false;
        #[cfg(test)]
        {
            self.values.clear();
        }
    }
    pub fn finish_frame(&mut self, gui: &nice_plug::context::gui::GuiContext) {
        if self.active.is_some() && !self.active_seen {
            self.close(gui);
        }
    }
    pub fn close(&mut self, gui: &nice_plug::context::gui::GuiContext) {
        if let Some(ptr) = self.active.take() {
            // SAFETY: all pointers come from the Params Arc owned by the editor.
            unsafe {
                gui.raw_end_set_parameter(ptr);
            }
        }
        self.edit = None;
    }

    pub fn knob<P>(
        &mut self,
        ui: &mut egui::Ui,
        param: &P,
        label: &str,
        units: Units,
        setter: &ParamSetter<'_>,
    ) where
        P: Param,
        P::Plain: Into<f64> + Display,
    {
        ui.push_id(param.as_ptr(), |ui| {
            if self.active == Some(param.as_ptr()) {
                self.active_seen = true;
            }
            let width = ui.available_width();
            ui.allocate_ui_with_layout(
                Vec2::new(width, 44.0),
                egui::Layout::left_to_right(egui::Align::Min),
                |ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::splat(44.0), Sense::click_and_drag());
                    let max_db = units.display(param.preview_plain(1.0).into());
                    let normalized = if units.gain {
                        ((units.display(param.unmodulated_plain_value().into()) + 60.0)
                            / (max_db + 60.0))
                            .clamp(0.0, 1.0) as f32
                    } else {
                        param.unmodulated_normalized_value()
                    };
                    if response.drag_started() {
                        setter.begin_set_parameter(param);
                        self.active = Some(param.as_ptr());
                        self.drag_value = normalized;
                        self.active_seen = true;
                    }
                    if response.dragged() {
                        let delta = ui.input(|s| {
                            -s.pointer.delta().y * if s.modifiers.shift { 0.0002 } else { 0.003 }
                        });
                        // Accumulate sub-step motion independently of the host's
                        // quantized integer value. Otherwise slow or Shift drags
                        // of polyphony/Unison knobs round back on every frame.
                        self.drag_value = (self.drag_value + delta).clamp(0.0, 1.0);
                        let next = self.drag_value;
                        let next = if units.gain {
                            let value = if next == 0.0 {
                                0.0
                            } else {
                                10.0_f64.powf((-60.0 + next as f64 * (max_db + 60.0)) / 20.0)
                            };
                            param
                                .string_to_normalized_value(&value.to_string())
                                .unwrap_or(normalized)
                        } else {
                            next
                        };
                        setter.set_parameter_normalized(param, next);
                    }
                    if response.drag_stopped() {
                        setter.end_set_parameter(param);
                        self.active = None;
                    }
                    if response.double_clicked() {
                        setter.begin_set_parameter(param);
                        setter.set_parameter(param, param.default_plain_value());
                        setter.end_set_parameter(param);
                    }
                    let center = rect.center();
                    ui.painter()
                        .circle_filled(center, 19.0, Color32::from_rgb(22, 36, 55));
                    ui.painter()
                        .circle_stroke(center, 19.0, Stroke::new(1.5, super::BLUE));
                    let direction = Vec2::angled((normalized * 270.0 + 135.0).to_radians());
                    ui.painter().line_segment(
                        [center + direction * 8.0, center + direction * 16.0],
                        Stroke::new(2.2, super::WHITE),
                    );
                    ui.vertical(|ui| {
                        ui.set_width((width - 56.0).max(30.0));
                        // Match the label to the visible circle's upper edge,
                        // keeping the name/value block inside the knob's height.
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.add_space(3.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(label)
                                    .size(12.0)
                                    .color(Color32::from_gray(168)),
                            )
                            .truncate(),
                        );
                        let id = ui.id().with("value");
                        if self.edit.as_ref().is_some_and(|e| e.id == id) {
                            let edit = self.edit.as_mut().unwrap();
                            let response = ui.add_sized(
                                [ui.available_width(), 22.0],
                                egui::TextEdit::singleline(&mut edit.text)
                                    .id(id)
                                    .desired_width(ui.available_width()),
                            );
                            edit.changed |= response.changed();
                            let cancel = ui.input(|s| s.key_pressed(egui::Key::Escape));
                            let enter = ui.input(|s| s.key_pressed(egui::Key::Enter));
                            if cancel || response.lost_focus() || enter {
                                if !cancel && edit.changed {
                                    if let Some(value) = parse(param, &edit.text, units) {
                                        setter.begin_set_parameter(param);
                                        setter.set_parameter_normalized(param, value);
                                        setter.end_set_parameter(param);
                                    } else if enter {
                                        response.request_focus();
                                        response
                                            .on_hover_text("Enter a number in the displayed unit");
                                        return;
                                    }
                                }
                                self.edit = None;
                            }
                        } else {
                            let value = units.display(param.unmodulated_plain_value().into());
                            // Wet Level / Output Gain use two decimal places
                            // for display only. Editing below still starts
                            // from the unrounded value, including silent -inf.
                            let number = if units.gain && value.is_finite() {
                                let displayed = if value.abs() < 0.005 { 0.0 } else { value };
                                format!("{displayed:.2}")
                            } else {
                                significant(value)
                            };
                            let text = format!("{number} {}", units.suffix);
                            let (value_rect, response) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 22.0),
                                Sense::click(),
                            );
                            let response = response.on_hover_cursor(egui::CursorIcon::Text);
                            if response.hovered() {
                                ui.painter().rect_filled(
                                    value_rect,
                                    3.0,
                                    Color32::from_white_alpha(12),
                                );
                            }
                            ui.painter().text(
                                value_rect.left_center(),
                                egui::Align2::LEFT_CENTER,
                                text,
                                egui::FontId::proportional(14.0),
                                super::WHITE,
                            );
                            #[cfg(test)]
                            {
                                self.values.push((label.to_owned(), response.rect, rect));
                            }
                            if response.clicked() {
                                // Use the underlying full-precision value on entry;
                                // merely opening/closing the field never rounds it.
                                let text = if !units.gain && units.factor == 1.0 {
                                    param.unmodulated_plain_value().to_string()
                                } else if value.is_finite() {
                                    value.to_string()
                                } else {
                                    "-inf".into()
                                };
                                self.edit = Some(Edit {
                                    id,
                                    text,
                                    changed: false,
                                });
                                ui.memory_mut(|m| m.request_focus(id));
                            }
                        }
                    });
                },
            );
        });
    }
}

pub fn tooltip(painter: &egui::Painter, bounds: Rect, anchor: egui::Pos2, text: String) {
    let galley = painter.layout_no_wrap(text, egui::FontId::monospace(12.0), super::WHITE);
    let size = galley.size() + Vec2::new(20.0, 12.0);
    let pos = egui::pos2(
        (anchor.x - size.x * 0.5).clamp(
            bounds.left() + 2.0,
            (bounds.right() - size.x - 2.0).max(bounds.left() + 2.0),
        ),
        anchor.y.clamp(
            bounds.top() + 2.0,
            (bounds.bottom() - size.y - 2.0).max(bounds.top() + 2.0),
        ),
    );
    painter.rect_filled(
        Rect::from_min_size(pos, size),
        4.0,
        Color32::from_rgb(20, 46, 77),
    );
    painter.galley(pos + Vec2::new(10.0, 6.0), galley, super::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn five_significant_digits_do_not_limit_input_precision() {
        assert_eq!(significant(123.456789), "123.46");
        assert_eq!(significant(12345.6789), "12346");
        assert_eq!(significant(0.123456789), "0.12346");
        assert_eq!(significant(99999.9), "1e5");
        let param = FloatParam::new(
            "test",
            250.0,
            FloatRange::Linear {
                min: 20.0,
                max: 20000.0,
            },
        );
        let value =
            param.preview_plain(parse(&param, "1234.56789 Hz", Units::plain("Hz")).unwrap());
        assert!((value - 1234.5679).abs() < 0.001);
        assert_eq!(significant(value as f64), "1234.6");
        assert!(parse(&param, "NaN", Units::plain("Hz")).is_none());
        assert!(
            (param.preview_plain(parse(&param, "1.23456789 kHz", Units::plain("Hz")).unwrap())
                - value)
                .abs()
                < 0.001
        );
    }
    #[test]
    fn db_mapping_preserves_linear_gain_and_silence() {
        let param = FloatParam::new(
            "gain",
            1.0,
            FloatRange::Linear {
                min: 0.0,
                max: 16.0,
            },
        );
        assert!(
            (param.preview_plain(parse(&param, "12.041199826559248 dB", Units::GAIN).unwrap())
                - 4.0)
                .abs()
                < 1e-5
        );
        assert_eq!(
            param.preview_plain(parse(&param, "-inf dB", Units::GAIN).unwrap()),
            0.0
        );
    }
}
