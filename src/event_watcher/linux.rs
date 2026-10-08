use crate::common::local_data_directory;
use crate::controller::{BrightnessController, Message};
use crate::gui::UserEvent;
use egui_winit::winit::event_loop::EventLoop;
use nix::errno::Errno;
use nix::poll::{poll, PollFd, PollFlags};
use nix::unistd::pipe2;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::JoinHandle;
use udev::{EventType, MonitorBuilder};

pub const MSG_OPEN_WINDOW: u8 = 1;

pub fn get_ipc_path() -> PathBuf {
    local_data_directory().join("ipc")
}

pub struct EventWatcher {
    stop_handles: Vec<File>,
    threads: Vec<JoinHandle<()>>,
}

fn stop_pipe() -> anyhow::Result<(File, File)> {
    let (read, write) = pipe2(nix::fcntl::OFlag::O_CLOEXEC)?;
    // Each newly created descriptor is owned by exactly one File.
    Ok(unsafe { (File::from_raw_fd(read), File::from_raw_fd(write)) })
}

// Closing the write end wakes poll with POLLHUP, even if initialization failed.
// Shutdown never opens a FIFO or writes to a reader that may have exited.
fn wait_for_event(fd: RawFd, stop: RawFd) -> anyhow::Result<bool> {
    loop {
        let mut pfds = [
            PollFd::new(fd, PollFlags::POLLIN),
            PollFd::new(stop, PollFlags::POLLIN),
        ];
        match poll(&mut pfds, -1) {
            Err(Errno::EINTR) => continue,
            other => {
                other?;
            }
        }
        if !pfds[1]
            .revents()
            .unwrap_or_else(PollFlags::empty)
            .is_empty()
        {
            return Ok(false);
        }
        let events = pfds[0].revents().unwrap_or_else(PollFlags::empty);
        if events.intersects(PollFlags::POLLERR | PollFlags::POLLNVAL | PollFlags::POLLHUP) {
            anyhow::bail!("Watched descriptor disconnected");
        }
        if events.contains(PollFlags::POLLIN) {
            return Ok(true);
        }
    }
}

fn watch_ddcci(
    stop: File,
    controller: mpsc::Sender<Message>,
    ready: mpsc::Sender<Result<(), String>>,
) -> anyhow::Result<()> {
    let setup = MonitorBuilder::new()
        .and_then(|builder| builder.match_subsystem("ddcci"))
        .and_then(|builder| builder.match_subsystem("backlight"))
        .and_then(MonitorBuilder::listen);
    let _ = ready.send(setup.as_ref().map(|_| ()).map_err(|e| e.to_string()));
    let socket = setup?;
    log::info!("Monitoring backlight and DDC/CI connections");
    while wait_for_event(socket.as_raw_fd(), stop.as_raw_fd())? {
        // The safe iterator owns/unrefs the device and borrows its action string.
        for event in socket.iter() {
            if matches!(event.event_type(), EventType::Add | EventType::Remove) {
                controller.send(Message::Refresh("udev device change"))?;
            }
        }
    }
    Ok(())
}

fn watch_ipc_pipe(
    ipc_path: &Path,
    stop: File,
    mut on_open: impl FnMut(),
    ready: mpsc::Sender<Result<(), String>>,
) -> anyhow::Result<()> {
    // A read/write open keeps the FIFO connected without waiting for a writer.
    let setup = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(nix::libc::O_NONBLOCK | nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
        .open(ipc_path);
    let _ = ready.send(setup.as_ref().map(|_| ()).map_err(|e| e.to_string()));
    let mut fifo = setup?;
    while wait_for_event(fifo.as_raw_fd(), stop.as_raw_fd())? {
        let mut buffer = [0; 64];
        match fifo.read(&mut buffer) {
            Ok(len) => {
                for byte in &buffer[..len] {
                    if *byte == MSG_OPEN_WINDOW {
                        on_open();
                    }
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

impl EventWatcher {
    pub fn start(
        controller: &BrightnessController,
        event_loop: Option<&EventLoop<UserEvent>>,
    ) -> anyhow::Result<Self> {
        let (ddcci_stop, ddcci_write) = stop_pipe()?;
        let (ipc_stop, ipc_write) = stop_pipe()?;
        let (ddcci_ready, ddcci_rx) = mpsc::channel();
        let (ipc_ready, ipc_rx) = mpsc::channel();
        let sender = controller.sender.clone();
        let ddcci = std::thread::spawn(move || {
            if let Err(e) = watch_ddcci(ddcci_stop, sender, ddcci_ready) {
                log::error!("Error monitoring DDC/CI connections: {e:#}");
            }
        });
        let path = get_ipc_path();
        let proxy = event_loop.map(|e| e.create_proxy());
        let ipc = std::thread::spawn(move || {
            if let Err(e) = watch_ipc_pipe(
                &path,
                ipc_stop,
                || {
                    if let Some(proxy) = &proxy {
                        let _ = proxy.send_event(UserEvent::OpenWindow("IPC watcher"));
                    }
                },
                ipc_ready,
            ) {
                log::error!("Error monitoring IPC: {e:#}");
            }
        });
        let watcher = Self {
            stop_handles: vec![ddcci_write, ipc_write],
            threads: vec![ddcci, ipc],
        };
        // Keep the cleanup guard alive while waiting: a failed initialization
        // must stop and join the other thread too.
        for (name, receiver) in [("device events", ddcci_rx), ("IPC", ipc_rx)] {
            if let Err(error) = receiver.recv()? {
                anyhow::bail!("Unable to initialize {name} watcher: {error}");
            }
        }
        Ok(watcher)
    }
}

impl Drop for EventWatcher {
    fn drop(&mut self) {
        self.stop_handles.clear();
        for thread in self.threads.drain(..) {
            if thread.join().is_err() {
                log::error!("Event watcher thread panicked");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::Duration;

    #[test]
    fn ipc_handles_messages_and_stops_without_an_external_writer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ipc");
        nix::unistd::mkfifo(
            &path,
            nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
        )
        .unwrap();
        let (stop, write) = stop_pipe().unwrap();
        let (tx, rx) = mpsc::channel();
        let (ready, ready_rx) = mpsc::channel();
        let mut fifo = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let thread = std::thread::spawn(move || {
            watch_ipc_pipe(
                &path,
                stop,
                || {
                    tx.send(()).unwrap();
                },
                ready,
            )
        });
        ready_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        fifo.write_all(&[MSG_OPEN_WINDOW, 99, MSG_OPEN_WINDOW])
            .unwrap();
        for _ in 0..2 {
            rx.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        drop(fifo);
        drop(write);
        assert!(thread.join().unwrap().is_ok());
    }

    #[test]
    fn ipc_initialization_failure_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let (stop, _write) = stop_pipe().unwrap();
        let (ready, rx) = mpsc::channel();
        assert!(watch_ipc_pipe(&dir.path().join("missing"), stop, || {}, ready).is_err());
        assert!(rx.recv_timeout(Duration::from_secs(2)).unwrap().is_err());
    }
}
