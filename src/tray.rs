use crate::common::APP_NAME;
use crate::gui::UserEvent;
use egui_winit::winit::event_loop::{EventLoop, EventLoopProxy};
use std::sync::{Arc, Mutex};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItemBuilder};
use tray_icon::{Icon, MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};

const MENU_ID_OPEN: &str = "OPEN";
const MENU_ID_EXIT: &str = "EXIT";

pub fn read_icon() -> (Vec<u8>, png::OutputInfo) {
    let mut decoder = png::Decoder::new(include_bytes!("../assets/icon-256.png").as_slice());
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    (buf, info)
}

// Backend ksni (StatusNotifier D-Bus) sur Linux : pas de boucle gtk à
// initialiser, le comportement est identique sur tous les OS.
pub fn create(event_loop: &EventLoop<UserEvent>) -> TrayIcon {
    create_internal(event_loop.create_proxy())
}

fn create_internal(event_loop: EventLoopProxy<UserEvent>) -> TrayIcon {
    let (buf, info) = read_icon();
    let icon = Icon::from_rgba(buf, info.width, info.height).unwrap();

    let menu = Menu::with_items(&[
        &MenuItemBuilder::new()
            .text("Open")
            .id(MenuId::new(MENU_ID_OPEN))
            .enabled(true)
            .build(),
        &MenuItemBuilder::new()
            .text("Exit")
            .id(MenuId::new(MENU_ID_EXIT))
            .enabled(true)
            .build(),
    ])
    .unwrap();

    let tray_icon = TrayIconBuilder::new()
        .with_tooltip(APP_NAME)
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .build()
        .unwrap();

    let tray_loop = Arc::new(Mutex::new(event_loop.clone()));
    let menu_loop = Arc::new(Mutex::new(event_loop));

    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if matches!(
            event,
            TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            }
        ) {
            let _ = tray_loop
                .lock()
                .unwrap()
                .send_event(UserEvent::OpenWindow("Tray Button"));
        }
    }));

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let action = match event.id.0.as_str() {
            MENU_ID_OPEN => UserEvent::OpenWindow("Tray Button"),
            MENU_ID_EXIT => UserEvent::Exit("Tray Button"),
            _ => return,
        };
        let _ = menu_loop.lock().unwrap().send_event(action);
    }));

    tray_icon
}
