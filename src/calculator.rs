//! Algorithm for computing the brightness percentage for a given time

use sunrise_sunset_calculator::SunriseSunsetResult;

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
    let mut next_update_brightness = if decreasing {
        brightness - 1
    } else {
        brightness + 1
    };
    if next_update_brightness > high_brightness {
        next_update_brightness = high_brightness;
    } else if next_update_brightness < low_brightness {
        next_update_brightness = low_brightness;
    }
    let expiry_time = if time_now == event_time {
        time_now + 1 // Don't get stuck into an infinite loop when exactly on the boundary
    } else {
        // Inverse of the sine function at next_update_brightness
        let asin = ((next_update_brightness as f64 - y_offset) / y_multiplier).asin();
        let expiry_offset = (asin / x_multiplier).round();
        expiry_offset as i64 + event_time
    };
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

    // Apply the configured offsets. A positive offset advances the transition,
    // i.e. shifts the solar event earlier; a negative offset delays it.
    let rise = sun.rise - (sunrise_offset_mins as i64) * 60;
    let set = sun.set - (sunset_offset_mins as i64) * 60;

    let low = brightness_day.min(brightness_night);
    let high = brightness_day.max(brightness_night);
    let transition_secs = transition_mins * 60; // time for transition from low to high
    let half = (transition_secs / 2) as i64;
    // If nighttime brightness is greater than day (weird!) the sine slopes flip:
    // the morning event then dims, and the evening event brightens.
    let backwards = brightness_night > brightness_day;
    let morning_decreasing = backwards;
    let evening_decreasing = !backwards;

    // Le programme de luminosité est une courbe affine par morceaux construite
    // **uniquement** à partir des événements décalés (`rise`/`set`), sans dépendre
    // de `sun.visible`. Choisir le régime sur `sun.visible` évaluerait la sinusoïde
    // hors de sa fenêtre lorsqu'un décalage éloigne une transition de l'événement
    // solaire réel, créant un pic parasite entre la transition décalée et cet
    // événement. Ici, chaque transition est bornée à [event - half, event + half]
    // et les plateaux la prolongent : le décalage devient un simple glissement.
    if sun.visible {
        // Journée : la transition du matin est derrière nous, celle du soir devant.
        //   nuit | rise-half [montée] rise+half | jour | set-half [descente] set+half | nuit
        if time_now < rise - half {
            plateau(brightness_night, rise - half)
        } else if time_now < rise + half {
            sine_curve(
                time_now,
                transition_secs,
                rise,
                morning_decreasing,
                low,
                high,
            )
        } else if time_now < set - half {
            plateau(brightness_day, set - half)
        } else if time_now < set + half {
            sine_curve(
                time_now,
                transition_secs,
                set,
                evening_decreasing,
                low,
                high,
            )
        } else {
            // Transition du soir terminée mais le soleil réel est encore levé :
            // on tient le plateau nuit et on recalcule au coucher réel (événements frais).
            plateau(brightness_night, sun.set)
        }
    } else {
        // Nuit : la transition du soir est derrière nous, celle du matin devant.
        //   jour | set-half [descente] set+half | nuit | rise-half [montée] rise+half | jour
        if time_now < set - half {
            plateau(brightness_day, set - half)
        } else if time_now < set + half {
            sine_curve(
                time_now,
                transition_secs,
                set,
                evening_decreasing,
                low,
                high,
            )
        } else if time_now < rise - half {
            plateau(brightness_night, rise - half)
        } else if time_now < rise + half {
            sine_curve(
                time_now,
                transition_secs,
                rise,
                morning_decreasing,
                low,
                high,
            )
        } else {
            // Transition du matin terminée mais le soleil réel n'est pas encore levé :
            // on tient le plateau jour et on recalcule au lever réel (événements frais).
            plateau(brightness_day, sun.rise)
        }
    }
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
    fn test_sunset_sine_curve() {
        let low = 40;
        let high = 80;
        let t_secs = 60 * 60; //60 minutes
        let set = Utc.ymd(2018, 12, 2).and_hms(16, 0, 0).timestamp(); // Fictional
        let midpoint = (low as f64 + ((high - low) as f64 / 2.0)).round() as u32;

        // At start of the transition it should equal the day brightness
        let transition_start = Utc.ymd(2018, 12, 2).and_hms(15, 30, 0).timestamp();
        let r = sine_curve(transition_start, t_secs, set, true, low, high);
        assert_eq!(high, r.brightness); //80

        //Test part way between transition. It should be less than the daytime brightness. But greater than the midpoint because it is not yet sunset
        let before_sunset = Utc.ymd(2018, 12, 2).and_hms(15, 45, 0).timestamp();
        let r = sine_curve(before_sunset, t_secs, set, true, low, high);
        assert!(r.brightness < high && r.brightness > midpoint); //~74

        //At sunset it should be half way between the day and night brightness
        let r = sine_curve(set, t_secs, set, true, low, high);
        assert_eq!(midpoint, r.brightness); //60

        //At end of the transition it should equal the night brightness
        let transition_end = Utc.ymd(2018, 12, 2).and_hms(16, 30, 0).timestamp();
        let r = sine_curve(transition_end, t_secs, set, true, low, high);
        assert_eq!(r.brightness, low); // 40
    }

    #[test]
    fn test_sunrise_sine_curve() {
        let low = 35;
        let high = 76;
        let t_secs = 40 * 60; //40 minutes
        let rise = Utc.ymd(2018, 12, 2).and_hms(8, 0, 0).timestamp(); // Fictional
        let midpoint = (low as f64 + ((high - low) as f64 / 2.0)).round() as u32;

        //At start of the transition it should equal the night brightness
        let start_of_transition = Utc.ymd(2018, 12, 2).and_hms(7, 40, 0).timestamp();
        let r = sine_curve(start_of_transition, t_secs, rise, false, low, high);
        assert_eq!(low, r.brightness); // 35

        //Test part way between transition. It should be greater than night brighness. But less than the midpoint because it is not yet sunrise
        let before_sunrise = Utc.ymd(2018, 12, 2).and_hms(7, 50, 0).timestamp();
        let r = sine_curve(before_sunrise, t_secs, rise, false, low, high);
        assert!(r.brightness > low && r.brightness < midpoint); //~41

        //At sunrise it should be half way between the day and night brightness
        let r = sine_curve(rise, t_secs, rise, false, low, high);
        assert_eq!(midpoint, r.brightness); //55.5 is rounded to 56

        //At end of the transition it should equal the daytime brightness
        let end_of_transition = Utc.ymd(2018, 12, 2).and_hms(8, 20, 0).timestamp();
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
