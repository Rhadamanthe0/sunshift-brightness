use crate::apply::ApplyResults;
use crate::config::SsbConfig;
use crate::controller::Message;
use crate::gui::brightness_settings::BrightnessSettingsPage;
use crate::gui::help::HelpPage;
use crate::gui::location_settings::LocationSettingsPage;
use crate::gui::monitor_overrides::MonitorOverridePage;
use crate::gui::status::StatusPage;
use crate::gui::UserEvent;
use egui::{Align, Color32, Layout, ScrollArea};
use egui_winit::winit::event_loop::EventLoopProxy;
use enum_iterator::Sequence;
use std::collections::VecDeque;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, RwLock};

pub const SPACING: f32 = 10.0;

pub trait Page {
    fn render(&mut self, ui: &mut egui::Ui, app_state: &mut AppState);
}

pub struct SsbEguiApp {
    selected_page: PageId,
    brightness_settings_page: BrightnessSettingsPage,
    pub location_settings_page: LocationSettingsPage,
    help_page: HelpPage,
    monitor_override_page: MonitorOverridePage,
    context: AppState,
    pub modal: Option<Box<dyn Modal>>,
}

pub struct AppState {
    pub main_loop: EventLoopProxy<UserEvent>,
    pub config: Arc<RwLock<SsbConfig>>,
    pub controller: Sender<Message>,
    pub results: Arc<RwLock<Option<ApplyResults>>>,
    pub transitions: Transitions,
}

pub type TransitionFn = Box<dyn FnOnce(&mut SsbEguiApp) + Send>;

#[derive(Default, Clone)]
pub struct Transitions(Arc<Mutex<VecDeque<TransitionFn>>>);

impl Transitions {
    pub fn queue_state_transition<F: FnOnce(&mut SsbEguiApp) + Send + 'static>(
        &self,
        transition: F,
    ) {
        self.0.lock().unwrap().push_back(Box::new(transition));
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Sequence)]
enum PageId {
    Status,
    BrightnessSettings,
    LocationSettings,
    MonitorOverrides,
    Help,
}

impl PageId {
    fn title(self) -> &'static str {
        match self {
            PageId::Status => "Status",
            PageId::BrightnessSettings => "Brightness Settings",
            PageId::LocationSettings => "Location Settings",
            PageId::Help => "Help",
            PageId::MonitorOverrides => "Monitor Overrides",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            PageId::Status => "ℹ",
            PageId::BrightnessSettings => "🔅",
            PageId::LocationSettings => "🌐",
            PageId::Help => "❔",
            PageId::MonitorOverrides => "💻",
        }
    }
}

pub trait Modal {
    fn render(&self, ctx: &egui::Context, app_state: &mut AppState);
}

pub struct ExitModal;

impl Modal for ExitModal {
    fn render(&self, ctx: &egui::Context, app_state: &mut AppState) {
        egui::Window::new("Are you sure?")
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(
                    "Exiting the application will stop the brightness being automatically updated",
                );
                ui.with_layout(Layout::left_to_right(Align::LEFT), |ui| {
                    if ui.button("Exit").clicked() {
                        app_state
                            .main_loop
                            .send_event(UserEvent::Exit("Exit Modal"))
                            .unwrap();
                    }
                    if ui.button("Cancel").clicked() {
                        app_state.transitions.queue_state_transition(|app| {
                            app.modal = None;
                        });
                    };
                });
            });
    }
}

pub struct MessageModal {
    pub title: String,
    pub message: String,
}

impl Modal for MessageModal {
    fn render(&self, ctx: &egui::Context, app_state: &mut AppState) {
        egui::Window::new(&self.title)
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(&self.message);
                if ui.button("Ok").clicked() {
                    app_state.transitions.queue_state_transition(|app| {
                        app.modal = None;
                    });
                };
            });
    }
}

impl SsbEguiApp {
    pub fn new(
        main_loop: EventLoopProxy<UserEvent>,
        results: Arc<RwLock<Option<ApplyResults>>>,
        config: Arc<RwLock<SsbConfig>>,
        controller: Sender<Message>,
    ) -> Self {
        let config_read = config.read().unwrap();
        SsbEguiApp {
            selected_page: PageId::Status,
            brightness_settings_page: BrightnessSettingsPage::from_config(&config_read),
            location_settings_page: LocationSettingsPage::from_config(&config_read),
            monitor_override_page: MonitorOverridePage::from_config(&config_read),
            modal: None,
            context: AppState {
                main_loop,
                config: config.clone(),
                controller,
                results,
                transitions: Default::default(),
            },
            help_page: Default::default(),
        }
    }

    pub fn update(&mut self, ctx: &egui::Context) {
        let queue = self.context.transitions.0.clone();
        while let Some(transition) = queue.lock().unwrap().pop_front() {
            (transition)(self);
        }

        if let Some(modal) = &self.modal {
            modal.render(ctx, &mut self.context);
        }

        egui::SidePanel::left("MenuPanel")
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_enabled(self.modal.is_none());
                self.render_menu_panel(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.set_enabled(self.modal.is_none());
            self.render_main(ui);
        });
    }

    fn render_menu_panel(&mut self, ui: &mut egui::Ui) {
        ScrollArea::vertical().show(ui, |ui| {
            ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                enum_iterator::all::<PageId>().for_each(|page| {
                    let text = format!("{} {}", page.icon(), page.title());
                    ui.selectable_value(&mut self.selected_page, page, text);
                });

                ui.separator();

                if ui
                    .add(egui::Button::new("❌ Close Window").fill(Color32::TRANSPARENT))
                    .clicked()
                {
                    self.context
                        .main_loop
                        .send_event(UserEvent::CloseWindow("Menu Button"))
                        .unwrap();
                }

                if ui
                    .add(egui::Button::new("⏹ Exit Application").fill(Color32::TRANSPARENT))
                    .clicked()
                {
                    self.modal = Some(Box::new(ExitModal));
                }
            });
        });
    }

    fn render_main(&mut self, ui: &mut egui::Ui) {
        ScrollArea::both().show(ui, |ui| {
            ui.heading(self.selected_page.title());
            ui.add_space(SPACING);
            match self.selected_page {
                PageId::Status => StatusPage.render(ui, &mut self.context),
                PageId::BrightnessSettings => {
                    self.brightness_settings_page.render(ui, &mut self.context)
                }
                PageId::LocationSettings => {
                    self.location_settings_page.render(ui, &mut self.context)
                }
                PageId::Help => self.help_page.render(ui, &mut self.context),
                PageId::MonitorOverrides => {
                    self.monitor_override_page.render(ui, &mut self.context)
                }
            }
        });
    }
}

pub fn set_red_widget_border(ui: &mut egui::Ui) {
    ui.style_mut().visuals.widgets.inactive.bg_stroke.color = egui::Color32::RED;
    ui.style_mut().visuals.widgets.inactive.bg_stroke.width = 1.0;
    ui.style_mut().visuals.widgets.hovered.bg_stroke.color = egui::Color32::RED;
}

pub fn save_config(config: &SsbConfig, transitions: &Transitions) -> bool {
    if let Err(e) = config.save() {
        log::error!("Unable to save config: {:#}", e);
        transitions.queue_state_transition(move |app| {
            app.modal = Some(Box::new(MessageModal {
                title: "Error".to_string(),
                message: format!("Unable to save config: {}", e),
            }));
        });
        return false;
    }
    true
}

#[cfg(all(test, windows))]
mod native_tests {
    use super::*;
    use crate::config::{Location, MonitorOverride, MonitorProperty};
    use crate::controller::BrightnessController;
    use crate::gui::WgpuWinitApp;
    use egui_winit::winit::event::Event;
    use egui_winit::winit::event_loop::EventLoopBuilder;
    use egui_winit::winit::platform::run_return::EventLoopExtRunReturn;
    use egui_winit::winit::platform::windows::EventLoopBuilderExtWindows;
    use std::time::{Duration, Instant};

    fn assert_rendered_frame(frame: Option<egui::ColorImage>, size: [usize; 2]) {
        let frame = frame.expect("Native GUI frame was not rendered and captured");
        assert!(size[0] > 0 && size[1] > 0, "Native GUI frame is empty");
        assert_eq!(frame.size, size, "Capture does not match the native window");
        assert_eq!(frame.pixels.len(), size[0] * size[1]);
        assert!(
            frame.pixels.iter().any(|pixel| *pixel != frame.pixels[0]),
            "Native GUI frame contains only a clear color, without UI"
        );
        eprintln!(
            "Captured native GUI frame {}x{} with UI pixels",
            size[0], size[1]
        );
    }

    #[test]
    #[ignore = "Requires an interactive Windows desktop; run explicitly for each Vulkan/OpenGL backend"]
    fn native_windows_gui_lifecycle() {
        // Le test doit exposer les échecs d'initialisation du pilote CI.
        let _ = simplelog::TermLogger::init(
            log::LevelFilter::Debug,
            simplelog::Config::default(),
            simplelog::TerminalMode::Stderr,
            simplelog::ColorChoice::Never,
        );
        // Variable lue seulement par ce test ignoré ; aucun repli vers l'autre
        // API ne doit donner une fausse preuve de rendu (migration MINIPC).
        let requested = std::env::var("SSB_TEST_GUI_BACKEND").ok();
        let selected = requested.as_deref().map(crate::gui::ci_test_backend);
        let config = Arc::new(RwLock::new(SsbConfig {
            location: Some(Location {
                latitude: 48.8566,
                longitude: 2.3522,
            }),
            sunrise_offset_mins: 180,
            sunset_offset_mins: -180,
            overrides: vec![MonitorOverride {
                key: MonitorProperty::DeviceName,
                pattern: "*".into(),
                brightness: None,
            }],
            ..Default::default()
        }));
        // No config/log files are loaded or saved. The wildcard disables every
        // monitor, so the real device enumeration never writes brightness.
        let (ready, initialized) = std::sync::mpsc::channel();
        let controller = BrightnessController::start(config.clone(), move || {
            let _ = ready.send(());
        });
        initialized.recv_timeout(Duration::from_secs(30)).unwrap();
        let results = controller.last_result.read().unwrap();
        let result = results.as_ref().unwrap();
        assert!(result.monitors.iter().all(|m| m.brightness.is_none()));
        eprintln!(
            "Native enumeration: {} disabled monitors, {} device errors",
            result.monitors.len(),
            result.unknown_devices.len()
        );
        for error in &result.unknown_devices {
            eprintln!("Native enumeration error: {error}");
        }
        drop(results);

        eprintln!("Creating native event loop");
        let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event()
            .with_any_thread(true)
            .build();
        let _event_watcher =
            crate::event_watcher::EventWatcher::start(&controller, Some(&event_loop)).unwrap();
        eprintln!("Creating native tray");
        let _tray = crate::tray::create(&event_loop);
        eprintln!("Native tray created; starting window lifecycle");
        let proxy = event_loop.create_proxy();
        let mut framework = WgpuWinitApp::new(proxy.clone(), true, || {
            SsbEguiApp::new(
                proxy.clone(),
                controller.last_result.clone(),
                config.clone(),
                controller.sender.clone(),
            )
        });
        framework.test_backend = selected.map(|(supported, _)| supported);
        let pages = enum_iterator::all::<PageId>().collect::<Vec<_>>();
        let mut rendered = 0;
        let mut closed = false;
        let mut reopened = false;
        let deadline = Instant::now() + Duration::from_secs(60);
        event_loop.run_return(|event, target, flow| {
            assert!(Instant::now() < deadline, "Native GUI test timed out");
            framework.on_event(target, &event);
            if matches!(event, Event::Resumed) {
                assert!(
                    framework.window().is_none(),
                    "Minimised startup opened a window"
                );
                eprintln!("Opening window from minimised startup");
                framework.on_event(
                    target,
                    &Event::UserEvent(UserEvent::OpenWindow("Native GUI test")),
                );
            }
            if matches!(event, Event::RedrawEventsCleared) {
                if rendered < pages.len() {
                    let running = framework.running.as_mut().unwrap();
                    let adapter = running.painter.render_state().unwrap().adapter.get_info();
                    let backend = adapter.backend;
                    eprintln!(
                        "Requested {}; rendering {backend:?}; adapter: {}; device: {:?}",
                        requested.as_deref().unwrap_or("Vulkan or OpenGL"),
                        adapter.name,
                        adapter.device_type
                    );
                    if let Some((_, expected)) = selected {
                        assert_eq!(
                            backend, expected,
                            "Requested rendering API was not obtained"
                        );
                        // wgpu-hal 0.16 infère le type GL depuis le renderer.
                        // Le nom SwiftShader approuvé prouve le pilote logiciel,
                        // sans supposer un DeviceType universel à travers les API.
                        assert!(
                            adapter.name.to_ascii_lowercase().contains("swiftshader"),
                            "Requested SwiftShader software adapter was not obtained"
                        );
                    } else {
                        assert!(matches!(
                            backend,
                            egui_wgpu::wgpu::Backend::Vulkan | egui_wgpu::wgpu::Backend::Gl
                        ));
                    }
                    running.app.selected_page = pages[rendered];
                    // Ignore desktop input: a user click must never activate
                    // Save against the real configuration during this test.
                    let _ = running.egui_winit.take_egui_input(&running.window);
                    framework.paint();
                    let running = framework.running.as_mut().unwrap();
                    let size = running.window.inner_size();
                    assert_rendered_frame(
                        running.test_frame.take(),
                        [size.width as usize, size.height as usize],
                    );
                    eprintln!("Rendered {} with {backend:?}", pages[rendered].title());
                    rendered += 1;
                } else if !closed {
                    framework.close_window();
                    closed = true;
                } else if !reopened {
                    framework.on_event(
                        target,
                        &Event::UserEvent(UserEvent::OpenWindow("Native GUI test")),
                    );
                    assert!(framework.running.as_ref().unwrap().test_frame.is_none());
                    framework.paint();
                    let running = framework.running.as_mut().unwrap();
                    let size = running.window.inner_size();
                    assert_rendered_frame(
                        running.test_frame.take(),
                        [size.width as usize, size.height as usize],
                    );
                    reopened = true;
                } else {
                    framework.close_window();
                    flow.set_exit();
                    return;
                }
            }
            flow.set_poll();
        });
        assert_eq!(rendered, pages.len());
        assert!(closed && reopened);
    }
}
