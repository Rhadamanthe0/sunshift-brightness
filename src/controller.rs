use crate::apply::{apply_brightness, ApplyResults};
use crate::config::SsbConfig;
use human_repr::HumanDuration;
use std::mem::take;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{mpsc, Arc, RwLock};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
            Some(timeout) => {
                // Si l'expiry calculée est dans le passé (zone plate désynchronisée
                // par les décalages sunrise/sunset), `duration_since` échoue :
                // attendre 60 s plutôt que 0 évite un busy-loop à 100 % de CPU.
                let duration = timeout
                    .duration_since(SystemTime::now())
                    .unwrap_or_else(|_| Duration::from_secs(60))
                    .max(Duration::from_secs(1));
                log::info!(
                    "BrightnessController sleeping for {}s",
                    duration.human_duration()
                );
                receiver.recv_timeout(duration)
            }
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
