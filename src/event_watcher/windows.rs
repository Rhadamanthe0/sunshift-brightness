use crate::controller::{BrightnessController, Message};
use crate::gui::UserEvent;
use egui_winit::winit::event_loop::{EventLoop, EventLoopProxy};
use std::sync::mpsc;
use std::sync::mpsc::sync_channel;
use std::thread::JoinHandle;
use win32_utils::error::{check_error, CheckError};
use win32_utils::window::WindowDataExtension;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, PostMessageW,
    PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SetWindowLongPtrW, UnregisterClassW,
    CW_USEDEFAULT, GWLP_USERDATA, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_DISPLAYCHANGE,
    WM_WTSSESSION_CHANGE, WNDCLASSW, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
};

const EXIT_LOOP: u32 = WM_APP + 999;

pub struct EventWatcher {
    thread: Option<JoinHandle<()>>,
    hwnd: HWND,
}

impl EventWatcher {
    pub fn start(
        controller: &BrightnessController,
        main_loop: Option<&EventLoop<UserEvent>>,
    ) -> anyhow::Result<Self> {
        let brightness_sender = controller.sender.clone();
        let proxy = main_loop.map(|m| m.create_proxy());
        let (tx, rx) = sync_channel(0);

        let thread = std::thread::spawn(move || {
            let mut window_data = Box::new(WindowData {
                sender: brightness_sender,
                open_window_msg_code: register_open_window_message(),
                main_loop: proxy,
            });

            let setup = (|| -> anyhow::Result<WatcherWindow> {
                unsafe {
                    let instance = GetModuleHandleW(None)?.into();
                    let class = WNDCLASSW {
                        hInstance: instance,
                        lpszClassName: w!("ssb_event_watcher"),
                        lpfnWndProc: Some(wndproc),
                        ..Default::default()
                    };
                    let atom = check_error(|| RegisterClassW(&class))?;
                    let mut window = WatcherWindow {
                        hwnd: HWND::default(),
                        instance,
                    };
                    window.hwnd = CreateWindowExW(
                        WINDOW_EX_STYLE::default(),
                        PCWSTR(atom as *const u16),
                        None,
                        WINDOW_STYLE::default(),
                        CW_USEDEFAULT,
                        CW_USEDEFAULT,
                        CW_USEDEFAULT,
                        CW_USEDEFAULT,
                        None,
                        None,
                        instance,
                        None,
                    )
                    .check_error()?;
                    check_error(|| {
                        SetWindowLongPtrW(
                            window.hwnd,
                            GWLP_USERDATA,
                            window_data.as_mut() as *mut _ as isize,
                        )
                    })?;
                    WTSRegisterSessionNotification(window.hwnd, 0)?;
                    Ok(window)
                }
            })();
            match setup {
                Ok(window) => {
                    if tx.send(Ok(window.hwnd)).is_err() {
                        return;
                    }
                    unsafe {
                        let mut message = MSG::default();
                        loop {
                            let status = GetMessageW(&mut message, None, 0, 0).0;
                            if status == -1 {
                                log::error!(
                                    "Event watcher GetMessageW failed: {}",
                                    windows::core::Error::from_win32()
                                );
                                break;
                            }
                            if status == 0 {
                                break;
                            }
                            DispatchMessageW(&message);
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                }
            }
            log::debug!("EventWatcher thread exiting");
        });

        let hwnd = match rx.recv()? {
            Ok(hwnd) => hwnd,
            Err(error) => {
                let _ = thread.join();
                anyhow::bail!("Unable to start event watcher: {error}");
            }
        };
        Ok(EventWatcher {
            thread: Some(thread),
            hwnd,
        })
    }
}

impl Drop for EventWatcher {
    fn drop(&mut self) {
        log::info!("Stopping EventWatcher");
        if let Err(e) = unsafe { PostMessageW(self.hwnd, EXIT_LOOP, None, None) } {
            log::error!("Unable to stop event watcher: {e}");
        }
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                log::error!("Event watcher thread panicked");
            }
        }
    }
}

// The hidden window must die before its user-data Box is released.
struct WatcherWindow {
    hwnd: HWND,
    instance: HINSTANCE,
}

impl Drop for WatcherWindow {
    fn drop(&mut self) {
        unsafe {
            if self.hwnd != HWND::default() {
                let _ = WTSUnRegisterSessionNotification(self.hwnd);
                SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
                let _ = DestroyWindow(self.hwnd);
            }
            let _ = UnregisterClassW(w!("ssb_event_watcher"), self.instance);
        }
    }
}

struct WindowData {
    sender: mpsc::Sender<Message>,
    open_window_msg_code: u32,
    main_loop: Option<EventLoopProxy<UserEvent>>,
}

unsafe extern "system" fn wndproc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if let Some(window_data) = window.get_user_data::<WindowData>() {
        match message {
            WM_DISPLAYCHANGE => {
                log::info!("Detected possible display change (WM_DISPLAYCHANGE)");
                let _ = window_data
                    .sender
                    .send(Message::Refresh("WM_DISPLAYCHANGE"));
            }
            EXIT_LOOP => {
                log::debug!("Received EXIT_LOOP message");
                PostQuitMessage(0);
            }
            WM_WTSSESSION_CHANGE => match wparam.0 as u32 {
                WTS_SESSION_LOCK => {
                    log::info!("Detected WTS_SESSION_LOCK");
                    let _ = window_data
                        .sender
                        .send(Message::Disable("WTS_SESSION_LOCK"));
                }
                WTS_SESSION_UNLOCK => {
                    log::info!("Detected WTS_SESSION_UNLOCK");
                    let _ = window_data
                        .sender
                        .send(Message::Enable("WTS_SESSION_UNLOCK"));
                }
                _ => {}
            },
            msg if msg == window_data.open_window_msg_code => {
                if let Some(event_loop) = &window_data.main_loop {
                    log::info!("Opening window due to external message");
                    let _ = event_loop.send_event(UserEvent::OpenWindow("Broadcast Message"));
                }
            }
            _ => {}
        }
    }
    DefWindowProcW(window, message, wparam, lparam)
}

pub fn register_open_window_message() -> u32 {
    unsafe {
        check_error(|| RegisterWindowMessageW(w!("solar-screen-brightness.open_window"))).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SsbConfig;
    use std::sync::{Arc, RwLock};
    use std::time::Duration;
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, SMTO_ABORTIFHUNG};

    #[test]
    fn watcher_handles_session_messages_and_releases_its_window_class() {
        // Starting twice in one process catches leaked windows/class registrations.
        for _ in 0..2 {
            let (tx, rx) = mpsc::channel();
            let controller = BrightnessController::start(
                Arc::new(RwLock::new(SsbConfig::default())),
                move || {
                    tx.send(()).unwrap();
                },
            );
            rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let watcher = EventWatcher::start(&controller, None).unwrap();
            let send = |message, parameter| unsafe {
                assert_ne!(
                    SendMessageTimeoutW(
                        watcher.hwnd,
                        message,
                        WPARAM(parameter),
                        None,
                        SMTO_ABORTIFHUNG,
                        1000,
                        None,
                    )
                    .0,
                    0
                );
            };
            send(WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK as usize);
            send(WM_DISPLAYCHANGE, 0);
            send(WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK as usize);
            // The display refresh while locked must not trigger an update.
            rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(rx.try_recv().is_err());
            send(WM_DISPLAYCHANGE, 0);
            rx.recv_timeout(Duration::from_secs(5)).unwrap();
            drop(watcher);
            drop(controller);
        }
    }
}
