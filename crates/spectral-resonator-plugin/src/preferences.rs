//! Per-user UI settings. No preferences client is constructed by the audio
//! engine: it is attached only while an editor is open and joined on close.
use audio_plugin_settings::{Edit, Preferences, Snapshot, json, settings_path};
use std::{path::PathBuf, sync::Arc};

pub const NAMESPACE: &str = "com.aflocat.audio";

pub struct EditorPreferences {
    client: Option<Arc<Preferences>>,
    pub snapshot: Snapshot,
    path: Option<PathBuf>,
    connection_error: Option<String>,
    confirm_repository: bool,
    #[cfg(test)]
    open_button: egui::Rect,
}
impl Default for EditorPreferences {
    fn default() -> Self {
        Self {
            client: None,
            snapshot: Snapshot {
                maximum_ui_fps: 60,
                language: "system".into(),
                loaded: false,
                pending: false,
                error: None,
            },
            path: None,
            connection_error: None,
            confirm_repository: false,
            #[cfg(test)]
            open_button: egui::Rect::NOTHING,
        }
    }
}
impl EditorPreferences {
    pub fn connect(&mut self) {
        let path = match settings_path(NAMESPACE) {
            Ok(path) => path,
            Err(error) => {
                self.connection_error = Some(error.to_string());
                return;
            }
        };
        self.path = Some(path.clone());
        match Preferences::open(path) {
            Ok(client) => {
                self.client = Some(client);
                self.connection_error = None;
                self.poll();
            }
            Err(error) => self.connection_error = Some(error.to_string()),
        }
    }
    pub fn poll(&mut self) {
        if let Some(client) = &self.client {
            self.snapshot = client.snapshot();
        }
    }
    pub fn disconnect(&mut self) {
        self.client = None;
        self.confirm_repository = false;
    }
    pub fn set_fps(&mut self, value: u32) {
        if !matches!(value, 30 | 60 | 90 | 120) {
            return;
        }
        self.snapshot.maximum_ui_fps = value;
        if let Some(client) = &self.client {
            client.set(Edit::global("maximum_ui_fps", json!(value)));
        }
    }
    fn set_language(&mut self, value: String) {
        self.snapshot.language = value.clone();
        if let Some(client) = &self.client {
            client.set(Edit::global("language", json!(value)));
        }
    }
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.strong("Noisy Cat Audio / Global preferences");
        ui.label("Maximum UI frame rate");
        let mut fps = self.snapshot.maximum_ui_fps;
        ui.horizontal(|ui| {
            for cap in [30, 60, 90, 120] {
                ui.selectable_value(&mut fps, cap, format!("{cap} FPS"));
            }
        });
        if fps != self.snapshot.maximum_ui_fps {
            self.set_fps(fps);
        }
        ui.small("Shared by all Noisy Cat Audio plugins on this user account.");
        ui.small("Projects do not overwrite this preference. Actual FPS depends on the host, GPU and display.");
        ui.add_space(6.0);
        ui.label("Preferred language");
        let label = |code: &str| match code {
            "system" => "System default",
            "en" => "English",
            "zh-CN" => "Chinese (Simplified)",
            _ => "Custom language",
        };
        let mut language = self.snapshot.language.clone();
        egui::ComboBox::from_id_salt("global-language")
            .selected_text(label(&language))
            .show_ui(ui, |ui| {
                for code in ["system", "en", "zh-CN"] {
                    ui.selectable_value(&mut language, code.into(), label(code));
                }
            });
        if language != self.snapshot.language {
            self.set_language(language);
        }
        ui.small(
            "Preference saved for future translations. This version's interface remains English.",
        );
        if let Some(path) = &self.path {
            ui.add_space(6.0);
            ui.small("Shared settings file (created on first change):");
            ui.add(
                egui::Label::new(egui::RichText::new(path.display().to_string()).small()).wrap(),
            );
        }
        if self.snapshot.pending {
            ui.small("Saving shared preferences...");
        }
        if let Some(error) = self
            .connection_error
            .as_ref()
            .or(self.snapshot.error.as_ref())
        {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
            if self.client.is_some() {
                ui.small("Preferences are not saved yet. The existing file is left intact; retrying while open.");
            } else {
                ui.small("Preferences are not saved. Close and reopen the editor to retry.");
            }
        }
        ui.add_space(6.0);
        let repository = repository_url();
        if ui
            .add_enabled(
                repository.is_some(),
                egui::Button::new("Open repository..."),
            )
            .on_disabled_hover_text("Repository URL has not been configured yet.")
            .clicked()
        {
            self.confirm_repository = true;
        }
    }
    pub fn repository_confirmation(&mut self, ctx: &egui::Context) {
        if let Some(url) = repository_url() {
            self.confirm_url(ctx, url);
        }
    }
    fn confirm_url(&mut self, ctx: &egui::Context, url: &str) {
        if !self.confirm_repository {
            return;
        }
        let response =
            egui::Modal::new(egui::Id::new("open-repository-confirmation")).show(ctx, |ui| {
                ui.strong("Open repository in your browser?");
                ui.label(url);
                ui.horizontal(|ui| {
                    let open = ui.button("Open browser");
                    #[cfg(test)]
                    {
                        self.open_button = open.rect;
                    }
                    // The backend receives an OpenUrl command only after this
                    // explicit confirmation. Merely displaying the link does nothing.
                    if open.clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(url));
                        self.confirm_repository = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_repository = false;
                    }
                });
            });
        if response.should_close() {
            self.confirm_repository = false;
        }
    }
}

fn repository_url() -> Option<&'static str> {
    let url = env!("CARGO_PKG_REPOSITORY");
    // Set the package's repository metadata when the real repository exists.
    // Never invent a destination or open a file/custom protocol from this UI.
    url.starts_with("https://").then_some(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repository_opens_only_after_confirming_the_displayed_destination() {
        let ctx = egui::Context::default();
        ctx.style_mut_of(egui::Theme::Dark, |style| style.animation_time = 0.0);
        let mut prefs = EditorPreferences::default();
        let url = "https://example.invalid/test-only";
        let render = |prefs: &mut EditorPreferences, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| prefs.confirm_url(ui.ctx(), url),
            );
            output.textures_delta.clear();
            output
        };
        assert!(
            render(&mut prefs, vec![])
                .platform_output
                .commands
                .is_empty()
        );
        prefs.confirm_repository = true;
        // Let the modal finish measuring before aiming a pointer at its button.
        assert!(
            render(&mut prefs, vec![])
                .platform_output
                .commands
                .is_empty()
        );
        assert!(
            render(&mut prefs, vec![])
                .platform_output
                .commands
                .is_empty()
        );
        let pos = prefs.open_button.center();
        let click = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        assert!(
            render(
                &mut prefs,
                vec![egui::Event::PointerMoved(pos), click(true)]
            )
            .platform_output
            .commands
            .is_empty()
        );
        let result = render(&mut prefs, vec![click(false)]);
        assert!(
            result
                .platform_output
                .commands
                .iter()
                .any(|cmd| matches!(cmd, egui::OutputCommand::OpenUrl(open) if open.url == url))
        );
        assert!(!prefs.confirm_repository);
        prefs.confirm_repository = true;
        let escaped = render(
            &mut prefs,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert!(escaped.platform_output.commands.is_empty());
        assert!(!prefs.confirm_repository);
    }
}
