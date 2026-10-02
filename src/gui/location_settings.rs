use crate::config::{Location, SsbConfig};
use crate::controller::Message;
use crate::gui::app::{
    save_config, set_red_widget_border, AppState, MessageModal, Modal, Page, SPACING,
};
use crate::gui::UserEvent;
use egui::{Context, Widget};
use serde::Deserialize;
use validator::Validate;

pub struct LocationSettingsPage {
    latitude: String,
    longitude: String,
    search_query: String,
}

impl LocationSettingsPage {
    pub fn from_config(config: &SsbConfig) -> Self {
        let location = config.location.as_ref();
        Self {
            latitude: coord_to_string(location.map(|l| l.latitude).unwrap_or(0.0)),
            longitude: coord_to_string(location.map(|l| l.longitude).unwrap_or(0.0)),
            search_query: String::default(),
        }
    }

    fn copy_to_config(&self, config: &mut SsbConfig) {
        config.location = Some(Location {
            latitude: self.latitude.parse().unwrap(),
            longitude: self.longitude.parse().unwrap(),
        });
        assert!(config.validate().is_ok())
    }

    fn is_latitude_valid(&self) -> bool {
        self.latitude
            .parse::<f64>()
            .is_ok_and(|l| (-90.0..=90.0).contains(&l))
    }

    fn is_longitude_valid(&self) -> bool {
        self.longitude
            .parse::<f64>()
            .is_ok_and(|l| (-180.0..=180.0).contains(&l))
    }
}

impl Page for LocationSettingsPage {
    fn render(&mut self, ui: &mut egui::Ui, app_state: &mut AppState) {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::LEFT), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text("Enter a city or location name"),
            );
            if ui
                .add_enabled(!self.search_query.is_empty(), egui::Button::new("Search"))
                .on_hover_text("Search using OpenStreetMap")
                .clicked()
            {
                on_search(app_state, self.search_query.clone());
            }
        });

        ui.add_space(SPACING);

        let latitude_valid = self.is_latitude_valid();
        let longitude_valid = self.is_longitude_valid();

        egui::Grid::new("location_settings")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Latitude")
                    .on_hover_text("Latitude (N), between -90° to 90°");
                ui.vertical(|ui| {
                    if !latitude_valid {
                        set_red_widget_border(ui);
                    }
                    ui.text_edit_singleline(&mut self.latitude);
                });
                ui.end_row();

                ui.label("Longitude")
                    .on_hover_text("Longitude (E), between -180° to 180°");
                ui.vertical(|ui| {
                    if !longitude_valid {
                        set_red_widget_border(ui);
                    }
                    ui.text_edit_singleline(&mut self.longitude);
                });
                ui.end_row();
            });

        ui.add_space(SPACING);

        let save_enabled = latitude_valid && longitude_valid;
        if ui
            .add_enabled(save_enabled, egui::Button::new("Save"))
            .clicked()
        {
            let mut config = app_state.config.write().unwrap();
            self.copy_to_config(&mut config);
            app_state
                .controller
                .send(Message::Refresh("Location change"))
                .unwrap();
            save_config(&mut config, &app_state.transitions);
        }
    }
}

fn coord_to_string(coord: f64) -> String {
    format!("{:.5}", coord)
}

struct SpinnerModal;

impl Modal for SpinnerModal {
    fn render(&self, ctx: &Context, _: &mut AppState) {
        egui::Area::new("SpinnerModal")
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                egui::Spinner::new().size(30.0).ui(ui);
            });
    }
}

/// Point renvoyé par Nominatim (OpenStreetMap) pour une recherche directe.
/// `lat`/`lon` arrivent en chaînes dans le JSON (`format=jsonv2`).
#[derive(Debug, Deserialize)]
struct NominatimResult {
    lat: String,
    lon: String,
}

/// Extrait le premier résultat Nominatim (`lat`/`lon` en chaînes JSON).
fn parse_first_result(json: &str) -> anyhow::Result<Option<(f64, f64)>> {
    let results: Vec<NominatimResult> = serde_json::from_str(json)?;
    results
        .into_iter()
        .next()
        .map(|r| Ok::<_, anyhow::Error>((r.lat.parse::<f64>()?, r.lon.parse::<f64>()?)))
        .transpose()
}

/// Recherche Nominatim directe via reqwest 0.12 (remplace `geocoding 0.4`).
/// Politique d'usage Nominatim : User-Agent identifiant avec contact projet.
fn forward_search(query: &str) -> anyhow::Result<Option<(f64, f64)>> {
    let user_agent = format!(
        "{}/{} (https://github.com/jacob-pro/solar-screen-brightness)",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
    );
    let body = reqwest::blocking::Client::new()
        .get("https://nominatim.openstreetmap.org/search")
        .query(&[("q", query), ("format", "jsonv2"), ("limit", "1")])
        .header(reqwest::header::USER_AGENT, user_agent)
        .send()
        .map_err(anyhow::Error::from)?
        .error_for_status()
        .map_err(anyhow::Error::from)?
        .text()
        .map_err(anyhow::Error::from)?;
    parse_first_result(&body)
}

fn on_search(app_state: &mut AppState, search_string: String) {
    // Show the spinner modal
    app_state
        .transitions
        .queue_state_transition(|app| app.modal = Some(Box::new(SpinnerModal)));

    // Start a background thread
    let transitions = app_state.transitions.clone();
    let proxy = app_state.main_loop.clone();
    std::thread::spawn(move || {
        let result = forward_search(&search_string);
        match result {
            Ok(None) => transitions.queue_state_transition(move |app| {
                app.modal = Some(Box::new(MessageModal {
                    title: "No results".to_string(),
                    message: format!("No location could be found for '{}'", search_string),
                }))
            }),
            Ok(Some(p)) => {
                transitions.queue_state_transition(move |app| {
                    app.location_settings_page.latitude = coord_to_string(p.0);
                    app.location_settings_page.longitude = coord_to_string(p.1);
                    app.modal = None;
                });
            }
            Err(e) => {
                log::error!("Error searching for location: {:?}", e);
                transitions.queue_state_transition(move |app| {
                    app.modal = Some(Box::new(MessageModal {
                        title: "Error".to_string(),
                        message: format!("Error occurred searching for location: {}", e),
                    }))
                });
            }
        }

        // Refresh the UI
        proxy
            .send_event(UserEvent::RepaintNow("Location Search Completed"))
            .unwrap();
    });
}

#[cfg(test)]
mod tests {
    use super::parse_first_result;

    #[test]
    fn parses_first_nominatim_result() {
        let json = r#"[{"place_id":1,"lat":"48.8566","lon":"2.3522",
            "display_name":"Paris, France"}]"#;
        let (lat, lon) = parse_first_result(json).unwrap().unwrap();
        assert!((lat - 48.8566).abs() < 1e-9);
        assert!((lon - 2.3522).abs() < 1e-9);
    }

    #[test]
    fn empty_nominatim_response_is_no_result() {
        assert!(parse_first_result("[]").unwrap().is_none());
    }
}
