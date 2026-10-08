use crate::common::get_log_path;
use crate::config::get_default_config_path;
use crate::gui::app::{AppState, Page, SPACING};
use ellipse::Ellipse;

pub struct HelpPage {
    log_file_path: String,
    config_file_path: String,
}

impl Default for HelpPage {
    fn default() -> Self {
        Self {
            log_file_path: get_log_path().display().to_string(),
            config_file_path: get_default_config_path().display().to_string(),
        }
    }
}

impl Page for HelpPage {
    fn render(&mut self, ui: &mut egui::Ui, _context: &mut AppState) {
        ui.spacing_mut().item_spacing.y = SPACING;

        ui.hyperlink_to(
            " Home page",
            "https://github.com/Rhadamanthe0/sunshift-brightness",
        );

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.label("Software version: ");
            ui.hyperlink_to(
                env!("CARGO_PKG_VERSION"),
                "https://github.com/Rhadamanthe0/sunshift-brightness/releases",
            );
        });

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.label("Please report any bugs on the ");
            ui.hyperlink_to(
                "issues page",
                "https://github.com/Rhadamanthe0/sunshift-brightness/issues",
            );
        });

        let entries = vec![
            ("Log file", self.log_file_path.as_str()),
            ("Config file", self.config_file_path.as_str()),
        ];

        egui::Grid::new("file paths grid")
            .striped(false)
            .num_columns(3)
            .show(ui, |ui| {
                for (name, path) in entries {
                    ui.label(name);
                    ui.hyperlink_to(path.truncate_ellipse(45), path);
                    if ui.button("📋").on_hover_text("Copy path").clicked() {
                        ui.output_mut(|o| o.copied_text = path.to_string());
                    }
                    ui.end_row();
                }
            });
    }
}
