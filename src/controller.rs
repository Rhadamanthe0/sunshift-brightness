use crate::apply::{apply_brightness, ApplyResults};
use crate::config::SsbConfig;
use human_repr::HumanDuration;
use std::mem::take;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{mpsc, Arc, RwLock};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub enum Message {
    Shutdown,
    Refresh(&'static str),
    Disable(&'static str),
    Enable(&'static str),
}

pub struct BrightnessController {
    pub sender: mpsc::Sender<Message>,
    pub last_result: Arc<RwLock<Option<ApplyResults>>>,
    join_handle: Option<JoinHandle<()>>,
}

impl BrightnessController {
    pub fn start<F: Fn() + Send + 'static>(
        config: Arc<RwLock<SsbConfig>>,
        on_update: F,
    ) -> BrightnessController {
        let (sender, receiver) = mpsc::channel();
        let last_result = Arc::new(RwLock::new(None));
        let cloned = last_result.clone();
        let join_handle = thread::spawn(move || {
            run(config, receiver, cloned, on_update);
        });
        BrightnessController {
            sender,
            last_result,
            join_handle: Some(join_handle),
        }
    }
}

impl Drop for BrightnessController {
    fn drop(&mut self) {
        let _ = self.sender.send(Message::Shutdown);
        if let Some(handle) = take(&mut self.join_handle) {
            if handle.join().is_err() {
                log::error!("Brightness controller thread panicked");
            }
        }
        log::debug!("Stopped BrightnessController");
    }
}

fn run<F: Fn()>(
    config: Arc<RwLock<SsbConfig>>,
    receiver: mpsc::Receiver<Message>,
    last_result: Arc<RwLock<Option<ApplyResults>>>,
    on_update: F,
) {
    log::info!("Starting BrightnessController");
    let mut enabled = true;

    loop {
        let timeout = if enabled {
            // Apply brightness using latest config
            let config = config.read().unwrap().clone();
            let result = apply(config);
            let timeout = calculate_timeout(&result);

            // Update last result
            *last_result.write().unwrap() = result;
            on_update();
            timeout
        } else {
            log::info!("BrightnessController is disabled, skipping update");
            None
        };

        // Sleep until receiving message or timeout
        let rx_result = match timeout {
            None => {
                log::info!("Brightness Worker sleeping indefinitely");
                receiver.recv().map_err(|e| e.into())
            }
            Some(timeout) => receive_until(
                timeout,
                |duration| receiver.recv_timeout(duration),
                || (SystemTime::now(), Instant::now()),
            ),
        };

        match rx_result {
            Ok(Message::Shutdown) => {
                log::info!("Stopping BrightnessController");
                break;
            }
            Ok(Message::Refresh(src)) => {
                log::info!("Refreshing due to '{src}'");
            }
            Ok(Message::Disable(src)) => {
                log::info!("Disabling BrightnessController due to '{src}'");
                enabled = false;
            }
            Ok(Message::Enable(src)) => {
                log::info!("Enabling BrightnessController due to '{src}'");
                enabled = true;
            }
            Err(RecvTimeoutError::Timeout) => {
                log::debug!("Refreshing due to timeout")
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

// recv_timeout uses a monotonic clock. Recheck civil time during long waits so
// a clock correction does not leave brightness on the old solar schedule.
fn receive_until(
    timeout: SystemTime,
    mut receive: impl FnMut(Duration) -> Result<Message, RecvTimeoutError>,
    mut now: impl FnMut() -> (SystemTime, Instant),
) -> Result<Message, RecvTimeoutError> {
    let (wall_start, monotonic_start) = now();
    // Preserve the guards against past expiries and subsecond busy loops.
    let duration = timeout
        .duration_since(wall_start)
        .unwrap_or_else(|_| Duration::from_secs(60))
        .max(Duration::from_secs(1));
    log::info!(
        "BrightnessController sleeping for {}s",
        duration.human_duration()
    );
    let mut elapsed = Duration::ZERO;
    loop {
        match receive((duration - elapsed).min(Duration::from_secs(60))) {
            Err(RecvTimeoutError::Timeout) => {}
            result => return result,
        }
        let (wall_now, monotonic_now) = now();
        elapsed = monotonic_now.duration_since(monotonic_start);
        if elapsed >= duration {
            return Err(RecvTimeoutError::Timeout);
        }
        let Some(expected_wall) = wall_start.checked_add(elapsed) else {
            return Err(RecvTimeoutError::Timeout);
        };
        let clock_shift = wall_now
            .duration_since(expected_wall)
            .unwrap_or_else(|error| error.duration());
        // Brightness is calculated at whole-second precision. Intermediate
        // checks with an unchanged clock must not reapply brightness or DDC/CI.
        if clock_shift >= Duration::from_secs(1) {
            log::debug!("Refreshing after a system clock change");
            return Err(RecvTimeoutError::Timeout);
        }
    }
}

// The time at which the brightness should be re-applied
fn calculate_timeout(results: &Option<ApplyResults>) -> Option<SystemTime> {
    if let Some(results) = results {
        let next_change = results
            .monitors
            .iter()
            .flat_map(|m| m.brightness.as_ref().map(|b| b.expiry_time))
            .flatten()
            .min()
            .and_then(|e| u64::try_from(e).ok())
            .and_then(|e| UNIX_EPOCH.checked_add(Duration::from_secs(e)));
        if results.monitors.is_empty()
            || !results.unknown_devices.is_empty()
            || results.monitors.iter().any(|m| m.error.is_some())
        {
            let retry = SystemTime::now() + Duration::from_secs(60);
            Some(next_change.map_or(retry, |next| next.min(retry)))
        } else {
            next_change
        }
    } else {
        None
    }
}

// Calculate and apply the brightness
fn apply(config: SsbConfig) -> Option<ApplyResults> {
    if let Some(location) = config.location {
        Some(apply_brightness(
            config.brightness_day,
            config.brightness_night,
            config.transition_mins,
            config.sunrise_offset_mins,
            config.sunset_offset_mins,
            location,
            config.overrides,
        ))
    } else {
        log::warn!("Skipping apply because no location is configured");
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::{BrightnessDetails, MonitorProperties, MonitorResult, SunriseSunsetResult};
    use std::cell::Cell;

    fn elapsed_until_timeout(
        start: SystemTime,
        deadline: SystemTime,
        wall_time_after_wait: impl Fn(Duration) -> SystemTime,
    ) -> Duration {
        let monotonic_start = Instant::now();
        let elapsed = Cell::new(Duration::ZERO);
        let result = receive_until(
            deadline,
            |duration| {
                elapsed.set(elapsed.get() + duration);
                Err(RecvTimeoutError::Timeout)
            },
            || {
                let elapsed = elapsed.get();
                let wall_time = if elapsed.is_zero() {
                    start
                } else {
                    wall_time_after_wait(elapsed)
                };
                (wall_time, monotonic_start + elapsed)
            },
        );
        assert!(matches!(result, Err(RecvTimeoutError::Timeout)));
        elapsed.get()
    }

    #[test]
    fn clock_jumps_refresh_before_the_old_solar_deadline() {
        let start = UNIX_EPOCH + Duration::from_secs(24 * 3600);
        let deadline = start + Duration::from_secs(2 * 3600);
        let adjustment = Duration::from_secs(6 * 3600);
        for corrected_time in [start + adjustment, start - adjustment] {
            let elapsed =
                elapsed_until_timeout(start, deadline, |elapsed| corrected_time + elapsed);
            assert!(
                elapsed <= Duration::from_secs(60),
                "A civil clock jump left the old brightness in place for {elapsed:?}"
            );
        }
    }

    #[test]
    fn steady_clock_does_not_refresh_before_the_solar_deadline() {
        let start = UNIX_EPOCH + Duration::from_secs(24 * 3600);
        let duration = Duration::from_secs(2 * 3600 + 17);
        assert_eq!(
            elapsed_until_timeout(start, start + duration, |elapsed| start + elapsed),
            duration
        );
        // Subsecond sampling differences must not cause extra hardware writes.
        for wall_start in [
            start + Duration::from_millis(500),
            start - Duration::from_millis(500),
        ] {
            assert_eq!(
                elapsed_until_timeout(start, start + duration, |elapsed| wall_start + elapsed),
                duration
            );
        }
    }

    #[test]
    fn short_and_past_deadlines_preserve_existing_delay_guards() {
        let start = UNIX_EPOCH + Duration::from_secs(24 * 3600);
        for (deadline, expected) in [
            (start + Duration::from_secs(2), Duration::from_secs(2)),
            (start + Duration::from_millis(250), Duration::from_secs(1)),
            (start, Duration::from_secs(1)),
            (start - Duration::from_secs(1), Duration::from_secs(60)),
        ] {
            assert_eq!(
                elapsed_until_timeout(start, deadline, |elapsed| start + elapsed),
                expected
            );
        }
    }

    #[test]
    fn all_messages_and_disconnection_interrupt_the_wait() {
        let (sender, receiver) = mpsc::channel();
        let messages = [
            Message::Shutdown,
            Message::Refresh("test"),
            Message::Disable("test"),
            Message::Enable("test"),
        ];
        for message in messages {
            sender.send(message).unwrap();
        }
        drop(sender);
        for expected in ["shutdown", "refresh", "disable", "enable", "disconnected"] {
            let clock_reads = Cell::new(0);
            let result = receive_until(
                SystemTime::now() + Duration::from_secs(2 * 3600),
                |duration| receiver.recv_timeout(duration),
                || {
                    clock_reads.set(clock_reads.get() + 1);
                    (SystemTime::now(), Instant::now())
                },
            );
            let received = match result {
                Ok(Message::Shutdown) => "shutdown",
                Ok(Message::Refresh("test")) => "refresh",
                Ok(Message::Disable("test")) => "disable",
                Ok(Message::Enable("test")) => "enable",
                Err(RecvTimeoutError::Disconnected) => "disconnected",
                _ => panic!("The pending message or disconnection was not forwarded"),
            };
            assert_eq!(received, expected);
            assert_eq!(
                clock_reads.get(),
                1,
                "A received message must return immediately"
            );
        }
    }

    #[test]
    fn missing_or_failed_devices_retry_even_with_constant_brightness() {
        let mut results = ApplyResults {
            unknown_devices: vec![],
            monitors: vec![],
            sun: SunriseSunsetResult {
                rise: 0,
                set: 1,
                visible: true,
            },
        };
        let mut missing_devices = Some(results);
        assert!(calculate_timeout(&missing_devices).is_some());
        missing_devices
            .as_mut()
            .unwrap()
            .unknown_devices
            .push("Enumeration failed".into());
        assert!(calculate_timeout(&missing_devices).is_some());
        results = ApplyResults {
            unknown_devices: vec![],
            monitors: vec![MonitorResult {
                properties: MonitorProperties {
                    device_name: "Test".into(),
                    #[cfg(windows)]
                    device_description: "Test".into(),
                    #[cfg(windows)]
                    device_key: "Test".into(),
                    #[cfg(windows)]
                    device_path: "Test".into(),
                },
                brightness: Some(BrightnessDetails {
                    expiry_time: None,
                    brightness: 50,
                    brightness_day: 50,
                    brightness_night: 50,
                }),
                error: Some("Temporarily unavailable".into()),
            }],
            sun: SunriseSunsetResult {
                rise: 0,
                set: 1,
                visible: true,
            },
        };
        assert!(calculate_timeout(&Some(results)).is_some());
        assert!(calculate_timeout(&None).is_none());
    }
}
