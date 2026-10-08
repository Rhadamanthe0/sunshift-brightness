//! Algorithm for computing the brightness percentage for a given time

use sunrise_sunset_calculator::SunriseSunsetResult;
use validator::Validate;

#[derive(Debug)]
pub struct BrightnessResult {
    /// Unix time that the brightness should be re-calculated
    pub expiry_time: Option<i64>,
    /// Brightness percentage for the current time
    pub brightness: u32,
}

fn sine_curve(
    time_now: i64,
    transition: u32,
    event_time: i64,
    decreasing: bool,
    low_brightness: u32,
    high_brightness: u32,
) -> BrightnessResult {
    // We need to transform the sine function
    // Scale the height to the difference between min and max brightness
    let y_multiplier = (high_brightness - low_brightness) as f64 / 2.0;
    // Shift upwards to the midpoint brightness
    let y_offset = y_multiplier + low_brightness as f64;
    // Scale half a cycle to be equal to the transition time
    let mut x_multiplier = std::f64::consts::PI / transition as f64;
    // Flip the curve to make it a decreasing function
    if decreasing {
        x_multiplier = -x_multiplier;
    }
    // Shift rightwards to centre on the sunrise/sunset event
    let x_offset = (time_now - event_time) as f64;
    // Compute brightness
    let brightness = (y_multiplier * (x_multiplier * x_offset).sin()) + y_offset;
    let brightness = brightness.round() as u32; // round to nearest integer brightness

    // Work out the expiry time; when the brightness will change to the next integer value
    // Rounding changes at half-integers, not at the next integer. At an
    // endpoint (including 0%) wait for the transition to end instead of
    // subtracting from zero or returning an expiry in the past.
    let threshold = if decreasing {
        brightness as f64 - 0.5
    } else {
        brightness as f64 + 0.5
    };
    let end = event_time + transition as i64 / 2;
    let expiry_time = if threshold < low_brightness as f64 || threshold > high_brightness as f64 {
        end
    } else {
        let asin = ((threshold - y_offset) / y_multiplier)
            .clamp(-1.0, 1.0)
            .asin();
        let crossing = asin / x_multiplier;
        // At a rising half-integer tie, round() already selects the new value.
        // A falling tie keeps the old value until the following second.
        let offset = if decreasing {
            crossing.floor() as i64 + 1
        } else {
            crossing.ceil() as i64
        };
        offset + event_time
    }
    .max(time_now + 1);
    BrightnessResult {
        expiry_time: Some(expiry_time),
        brightness,
    }
}

/// Plateau jour ou nuit : luminosité constante jusqu'à la prochaine bascule.
fn plateau(brightness: u32, expiry: i64) -> BrightnessResult {
    BrightnessResult {
        expiry_time: Some(expiry),
        brightness,
    }
}

pub fn calculate_brightness(
    brightness_day: u32,
    brightness_night: u32,
    transition_mins: u32,
    sunrise_offset_mins: i32,
    sunset_offset_mins: i32,
    sun: &SunriseSunsetResult,
    time_now: i64,
) -> BrightnessResult {
    // Special case where there is no difference in brightness
    if brightness_night == brightness_day {
        return BrightnessResult {
            expiry_time: None,
            brightness: brightness_day,
        };
    }

    let events = shifted_events(sun, sunrise_offset_mins, sunset_offset_mins);
    calculate_from_events(
        brightness_day,
        brightness_night,
        transition_mins,
        &events,
        time_now,
    )
}

#[derive(Clone, Copy, Debug)]
struct SolarEvent {
    time: i64,
    sunrise: bool,
}

fn shifted_events(
    sun: &SunriseSunsetResult,
    sunrise_offset: i32,
    sunset_offset: i32,
) -> Vec<SolarEvent> {
    let mut events = vec![
        SolarEvent {
            time: sun.rise - i64::from(sunrise_offset) * 60,
            sunrise: true,
        },
        SolarEvent {
            time: sun.set - i64::from(sunset_offset) * 60,
            sunrise: false,
        },
    ];
    events.sort_by_key(|event| (event.time, !event.sunrise));
    events
}

/// Solar events covering a calculation interval, including neighbouring events.
/// Collect once for a preview, so each graph sample doesn't repeat solar searches.
pub struct SolarSchedule {
    events: Vec<SolarEvent>,
}

impl SolarSchedule {
    pub fn new(
        location: crate::config::Location,
        start: i64,
        end: i64,
        sunrise_offset: i32,
        sunset_offset: i32,
    ) -> anyhow::Result<Self> {
        use sunrise_sunset_calculator::SunriseSunsetParameters;
        location.validate()?;
        anyhow::ensure!(start <= end, "Solar schedule interval is reversed");
        anyhow::ensure!(
            (-180..=180).contains(&sunrise_offset) && (-180..=180).contains(&sunset_offset),
            "Solar offsets must be between -180 and 180 minutes"
        );
        // Validated settings allow at most 3h of offset and a 3h half-transition.
        let margin = 6 * 3600;
        let mut sun =
            SunriseSunsetParameters::new(start - margin, location.latitude, location.longitude)
                .calculate()?;
        let mut events = shifted_events(&sun, sunrise_offset, sunset_offset);
        // Offsets may swap the two events around a short day/night. Include
        // the predecessor too, so the initial plateau never depends on where
        // the preview or controller's search interval started.
        let previous_sun = SunriseSunsetParameters::new(
            sun.rise.min(sun.set) - 2,
            location.latitude,
            location.longitude,
        )
        .calculate()?;
        let previous_rise = previous_sun.rise < previous_sun.set;
        let previous_time = previous_sun.rise.min(previous_sun.set);
        let previous_offset = if previous_rise {
            sunrise_offset
        } else {
            sunset_offset
        };
        events.push(SolarEvent {
            time: previous_time - i64::from(previous_offset) * 60,
            sunrise: previous_rise,
        });
        let mut latest = sun.rise.max(sun.set);
        while latest <= end + margin {
            sun = SunriseSunsetParameters::new(latest + 2, location.latitude, location.longitude)
                .calculate()?;
            let next = sun.rise.max(sun.set);
            anyhow::ensure!(next > latest, "Solar event search did not progress");
            let sunrise = sun.rise > sun.set;
            let offset = if sunrise {
                sunrise_offset
            } else {
                sunset_offset
            };
            events.push(SolarEvent {
                time: next - i64::from(offset) * 60,
                sunrise,
            });
            latest = next;
        }
        events.sort_by_key(|event| (event.time, !event.sunrise));
        Ok(Self { events })
    }

    pub fn calculate(&self, day: u32, night: u32, transition: u32, now: i64) -> BrightnessResult {
        calculate_from_events(day, night, transition, &self.events, now)
    }
}

fn calculate_from_events(
    day: u32,
    night: u32,
    transition: u32,
    events: &[SolarEvent],
    now: i64,
) -> BrightnessResult {
    if day == night {
        return BrightnessResult {
            brightness: day,
            expiry_time: None,
        };
    }
    let mut brightness = if events[0].sunrise { night } else { day };
    for (index, event) in events.iter().enumerate() {
        let target = if event.sunrise { day } else { night };
        // Opposite offsets can reorder a very short day/night, placing two
        // rises or two sets next to each other. A repeated target is a plateau.
        if target == brightness {
            continue;
        }
        let mut duration = i64::from(transition) * 60;
        // Short days/nights and opposite offsets can make transition windows
        // overlap. Shorten them symmetrically to preserve continuity at each
        // event, instead of jumping from one incomplete curve to another.
        if let Some(previous) = index.checked_sub(1).map(|i| events[i]) {
            duration = duration.min(event.time - previous.time);
        }
        if let Some(next) = events.get(index + 1) {
            duration = duration.min(next.time - event.time);
        }
        let half = duration / 2;
        if now < event.time - half {
            return plateau(brightness, event.time - half);
        }
        if now < event.time + half {
            return sine_curve(
                now,
                duration as u32,
                event.time,
                if event.sunrise {
                    night > day
                } else {
                    day > night
                },
                day.min(night),
                day.max(night),
            );
        }
        brightness = target;
    }
    plateau(brightness, now + 60)
}

/// Instant d'échantillonnage suivant, garantissant une progression **stricte**.
///
/// `calculate_brightness` peut renvoyer une `expiry_time` <= `current` dans les
/// zones plates que les décalages sunrise/sunset désynchronisent de
/// `sun.visible`. Sans garde, une boucle `current = expiry` n'avance plus et
/// tourne à l'infini (puis épuise la mémoire). On force alors un pas de 15 min.
pub fn next_sample_time(current: i64, expiry: Option<i64>, end: i64) -> i64 {
    let next = expiry.unwrap_or(end).min(end);
    if next > current {
        next
    } else {
        (current + 15 * 60).min(end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn zero_brightness_and_rounding_expiries_are_safe() {
        for decreasing in [false, true] {
            for (low, high) in [(0, 100), (0, 1), (35, 76), (40, 80)] {
                for time in -1800..1800 {
                    let result = sine_curve(time, 3600, 0, decreasing, low, high);
                    assert!((low..=high).contains(&result.brightness));
                    let expiry = result.expiry_time.unwrap();
                    assert!(expiry > time);
                    // No rounded brightness step should be skipped before expiry.
                    if expiry > time + 1 {
                        assert_eq!(
                            sine_curve(expiry - 1, 3600, 0, decreasing, low, high).brightness,
                            result.brightness
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn zero_transition_switches_at_shifted_events() {
        let sun = SunriseSunsetResult {
            rise: 10000,
            set: 50000,
            visible: true,
        };
        for (time, expected) in [(6399, 0), (6400, 100), (46399, 100), (46400, 0)] {
            let result = calculate_brightness(100, 0, 0, 60, 60, &sun, time);
            assert_eq!(result.brightness, expected);
            assert!(result.expiry_time.unwrap() > time);
        }
    }

    #[test]
    fn overlapping_transitions_do_not_jump() {
        let sun = SunriseSunsetResult {
            rise: 10000,
            set: 16000,
            visible: true,
        };
        let before = calculate_brightness(100, 0, 360, 0, 0, &sun, 20799);
        let after = calculate_brightness(100, 0, 360, 0, 0, &sun, 20800);
        assert!(before.brightness.abs_diff(after.brightness) <= 1);
    }

    #[test]
    fn short_day_schedule_is_continuous_across_real_solar_events() {
        let start = Utc
            .with_ymd_and_hms(2026, 12, 21, 0, 0, 0)
            .unwrap()
            .timestamp();
        let location = crate::config::Location {
            latitude: 60.0,
            longitude: 2.0,
        };
        for (sunrise_offset, sunset_offset) in [(-180, 180), (180, -180), (0, 0)] {
            let schedule = SolarSchedule::new(
                location,
                start,
                start + 86400,
                sunrise_offset,
                sunset_offset,
            )
            .unwrap();
            for (day, night) in [(100, 0), (0, 100)] {
                let mut previous = schedule.calculate(day, night, 360, start).brightness;
                // With crossed offsets the nearest events are only ~8 minutes
                // apart. Check continuity per second rather than impose a
                // five-percent/minute speed limit not present in the settings.
                for second in 1..=86400 {
                    let now = start + second;
                    let result = schedule.calculate(day, night, 360, now);
                    assert!(result.brightness <= 100);
                    assert!(
                        previous.abs_diff(result.brightness) <= 1,
                        "Abrupt change at {now} offsets={sunrise_offset}/{sunset_offset}: {previous} -> {}; events={:?}",
                        result.brightness, schedule.events
                    );
                    assert!(result.expiry_time.unwrap() > now);
                    previous = result.brightness;
                    if second % 3600 == 0 {
                        let fresh =
                            SolarSchedule::new(location, now, now, sunrise_offset, sunset_offset)
                                .unwrap();
                        assert!(
                            fresh
                                .calculate(day, night, 360, now)
                                .brightness
                                .abs_diff(result.brightness)
                                <= 1
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_sunset_sine_curve() {
        let low = 40;
        let high = 80;
        let t_secs = 60 * 60; //60 minutes
        let set = Utc
            .with_ymd_and_hms(2018, 12, 2, 16, 0, 0)
            .unwrap()
            .timestamp(); // Fictional
        let midpoint = (low as f64 + ((high - low) as f64 / 2.0)).round() as u32;

        // At start of the transition it should equal the day brightness
        let transition_start = Utc
            .with_ymd_and_hms(2018, 12, 2, 15, 30, 0)
            .unwrap()
            .timestamp();
        let r = sine_curve(transition_start, t_secs, set, true, low, high);
        assert_eq!(high, r.brightness); //80

        //Test part way between transition. It should be less than the daytime brightness. But greater than the midpoint because it is not yet sunset
        let before_sunset = Utc
            .with_ymd_and_hms(2018, 12, 2, 15, 45, 0)
            .unwrap()
            .timestamp();
        let r = sine_curve(before_sunset, t_secs, set, true, low, high);
        assert!(r.brightness < high && r.brightness > midpoint); //~74

        //At sunset it should be half way between the day and night brightness
        let r = sine_curve(set, t_secs, set, true, low, high);
        assert_eq!(midpoint, r.brightness); //60

        //At end of the transition it should equal the night brightness
        let transition_end = Utc
            .with_ymd_and_hms(2018, 12, 2, 16, 30, 0)
            .unwrap()
            .timestamp();
        let r = sine_curve(transition_end, t_secs, set, true, low, high);
        assert_eq!(r.brightness, low); // 40
    }

    #[test]
    fn test_sunrise_sine_curve() {
        let low = 35;
        let high = 76;
        let t_secs = 40 * 60; //40 minutes
        let rise = Utc
            .with_ymd_and_hms(2018, 12, 2, 8, 0, 0)
            .unwrap()
            .timestamp(); // Fictional
        let midpoint = (low as f64 + ((high - low) as f64 / 2.0)).round() as u32;

        //At start of the transition it should equal the night brightness
        let start_of_transition = Utc
            .with_ymd_and_hms(2018, 12, 2, 7, 40, 0)
            .unwrap()
            .timestamp();
        let r = sine_curve(start_of_transition, t_secs, rise, false, low, high);
        assert_eq!(low, r.brightness); // 35

        //Test part way between transition. It should be greater than night brighness. But less than the midpoint because it is not yet sunrise
        let before_sunrise = Utc
            .with_ymd_and_hms(2018, 12, 2, 7, 50, 0)
            .unwrap()
            .timestamp();
        let r = sine_curve(before_sunrise, t_secs, rise, false, low, high);
        assert!(r.brightness > low && r.brightness < midpoint); //~41

        //At sunrise it should be half way between the day and night brightness
        let r = sine_curve(rise, t_secs, rise, false, low, high);
        assert_eq!(midpoint, r.brightness); //55.5 is rounded to 56

        //At end of the transition it should equal the daytime brightness
        let end_of_transition = Utc
            .with_ymd_and_hms(2018, 12, 2, 8, 20, 0)
            .unwrap()
            .timestamp();
        let r = sine_curve(end_of_transition, t_secs, rise, false, low, high);
        assert_eq!(high, r.brightness); // 76
    }

    // Régression : avec des décalages sunrise/sunset, `calculate_brightness`
    // peut renvoyer une `expiry_time` <= `time_now` (zone plate désynchronisée
    // de `sun.visible`). La boucle d'échantillonnage du graphe doit malgré tout
    // toujours progresser, sinon elle tourne à l'infini (OOM).
    #[test]
    fn test_plot_loop_with_offsets_always_progresses() {
        use sunrise_sunset_calculator::SunriseSunsetParameters;
        let (day, night, transition, sr_off, ss_off) = (100u32, 20u32, 60u32, 60i32, 60i32);
        let (lat, lon) = (48.884_15, 2.236_89);
        // Balaie chaque "now" sur 48 h pour couvrir la fenêtre pathologique.
        for hour in 0..48i64 {
            let now = Utc
                .with_ymd_and_hms(2026, 6, 21, 0, 0, 0)
                .unwrap()
                .timestamp()
                + hour * 3600;
            let graph_start = now - 2 * 3600;
            let graph_end = now + 22 * 3600;
            let mut current = graph_start;
            let mut iters = 0u64;
            while current <= graph_end {
                iters += 1;
                assert!(
                    iters < 100_000,
                    "boucle non terminante : now={now}, bloque a current={current}"
                );
                let sun = SunriseSunsetParameters::new(current, lat, lon)
                    .calculate()
                    .unwrap();
                let b = calculate_brightness(day, night, transition, sr_off, ss_off, &sun, current);
                if current == graph_end {
                    break;
                }
                current = next_sample_time(current, b.expiry_time, graph_end);
            }
        }

        // Vérifications directes du garde de progression.
        assert_eq!(next_sample_time(1_000, Some(2_000), 10_000), 2_000); // futur -> expiry
        assert_eq!(next_sample_time(1_000, Some(500), 10_000), 1_900); // passé -> +15 min
        assert_eq!(next_sample_time(1_000, Some(1_000), 10_000), 1_900); // égal -> +15 min
        assert_eq!(next_sample_time(9_000, Some(500), 9_100), 9_100); // borné à end
    }

    // Régression : avec des décalages, la courbe doit rester un simple glissement
    // (plateau, transition, plateau) sans pic parasite. On échantillonne 24 h —
    // un coucher puis un lever — et on compte les inversions de sens : une courbe
    // propre en a exactement une (le creux de la nuit) ; la version buguée en
    // avait plusieurs (remontée à mi-hauteur après le coucher, et symétrique au lever).
    #[test]
    fn test_offsets_produce_clean_shifted_curve() {
        use sunrise_sunset_calculator::SunriseSunsetParameters;
        let (day, night, transition, sr_off, ss_off) = (100u32, 20u32, 60u32, 60i32, 60i32);
        let (lat, lon) = (48.884_15, 2.236_89);
        // Démarre à midi UTC (plein jour) pour englober un coucher puis un lever.
        let start = Utc
            .with_ymd_and_hms(2026, 6, 21, 12, 0, 0)
            .unwrap()
            .timestamp();

        let mut samples = Vec::new();
        for step in 0..=(24 * 12) {
            let t = start + step * 300; // toutes les 5 min sur 24 h
            let sun = SunriseSunsetParameters::new(t, lat, lon)
                .calculate()
                .unwrap();
            let b = calculate_brightness(day, night, transition, sr_off, ss_off, &sun, t);
            samples.push(b.brightness);
        }

        // La luminosité ne doit jamais sortir de [night, day].
        assert!(
            samples.iter().all(|&b| (night..=day).contains(&b)),
            "luminosité hors bornes : {samples:?}"
        );

        // Compte les changements de sens (les paliers sont ignorés).
        let mut dir = 0i32;
        let mut reversals = 0;
        for w in samples.windows(2) {
            let d = w[1] as i32 - w[0] as i32;
            if d == 0 {
                continue;
            }
            let nd = d.signum();
            if dir != 0 && nd != dir {
                reversals += 1;
            }
            dir = nd;
        }
        assert!(
            reversals <= 1,
            "courbe non propre : {reversals} inversions de sens (pic parasite ?) : {samples:?}"
        );
    }
}
