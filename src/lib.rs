pub mod app;
pub mod browsers;
pub mod config;
pub mod control;
pub mod discord;
pub mod favorites;
pub mod history;
pub mod lastfm;
pub mod lyrics;
pub mod player;
pub mod playlists;
pub mod portable;
pub mod providers;
pub mod queue;
pub mod random;
pub mod session;
pub mod state_file;
pub mod stats;
pub mod system;
pub mod theme;
pub mod tui;

/// Edad en días de una versión de yt-dlp (`YYYY.MM.DD`) respecto a hoy.
/// None si el formato no se reconoce (no se avisa de lo que no se entiende).
pub fn ytdlp_version_age_days(version: &str) -> Option<i64> {
    let mut parts = version.split('.');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let release = days_from_civil(year, month, day);
    let now = days_from_unix_epoch();
    Some(now - release)
}

/// Días desde 1970-01-01 para una fecha civil (algoritmo de Hinnant).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let mp = (month + 9).rem_euclid(12);
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn days_from_unix_epoch() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    (secs / 86_400) as i64
}

#[cfg(test)]
mod tests {
    #[test]
    fn ytdlp_version_age_parses_and_rejects() {
        // La edad exacta depende del reloj; lo testeable es el parseo.
        let age = super::ytdlp_version_age_days("2020.01.01").unwrap();
        assert!(age > 2000, "una versión de 2020 tiene miles de días: {age}");
        let today = super::ytdlp_version_age_days("2999.12.31").unwrap();
        assert!(today < 0, "fecha futura da edad negativa: {today}");
        assert_eq!(super::ytdlp_version_age_days("2026-08-19"), None);
        assert_eq!(super::ytdlp_version_age_days("2026.13.01"), None);
        assert_eq!(super::ytdlp_version_age_days("nope"), None);
        assert_eq!(super::ytdlp_version_age_days("2026.08"), None);
    }
}
