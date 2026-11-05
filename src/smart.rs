//! Playlists inteligentes (RF-65): listas virtuales derivadas de las
//! estadísticas locales y los favoritos. No se persisten: se regeneran a
//! partir de `stats.json` y `favorites.json` en cada uso.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::favorites::Favorites;
use crate::providers::{Track, track_key};
use crate::stats::Stats;

/// Días sin escuchar una pista para que aparezca en «Redescubrir».
const REDISCOVER_DAYS: u64 = 30;
/// Tope de pistas en las listas derivadas de estadísticas.
const MAX_TRACKS: usize = 100;

/// Playlist inteligente: variante de lista virtual de solo lectura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmartKind {
    /// Pistas más escuchadas (top por reproducciones).
    Top,
    /// Favoritos sin ninguna reproducción registrada.
    Unheard,
    /// Escuchadas por última vez hace más de `REDISCOVER_DAYS` días, las más
    /// olvidadas primero.
    Rediscover,
}

impl SmartKind {
    /// Variantes en el orden en que se muestran en el panel de playlists.
    pub const ALL: [Self; 3] = [Self::Top, Self::Unheard, Self::Rediscover];

    /// Nombre visible en la UI y resoluble por el socket de control.
    pub fn name(self) -> &'static str {
        match self {
            Self::Top => "Más escuchadas",
            Self::Unheard => "Sin estrenar",
            Self::Rediscover => "Redescubrir",
        }
    }

    /// Resuelve un nombre (sin distinguir mayúsculas) en una variante.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.name().eq_ignore_ascii_case(name))
    }

    /// Genera las pistas de la lista a partir de estadísticas y favoritos.
    pub fn tracks(self, stats: &Stats, favorites: &Favorites) -> Vec<Track> {
        match self {
            Self::Top => stats
                .top_tracks()
                .into_iter()
                .take(MAX_TRACKS)
                .map(|(_, track)| track)
                .collect(),
            Self::Unheard => favorites
                .tracks
                .iter()
                .filter(|track| {
                    let key = track_key(track);
                    !stats
                        .entries
                        .iter()
                        .any(|e| e.plays > 0 && (e.provider, e.id.as_str()) == (key.0, key.1.as_str()))
                })
                .cloned()
                .collect(),
            Self::Rediscover => {
                let cutoff = now_epoch().saturating_sub(REDISCOVER_DAYS * 86_400);
                let mut entries: Vec<_> = stats
                    .entries
                    .iter()
                    .filter(|e| e.last_played > 0 && e.last_played < cutoff)
                    .collect();
                // Las más olvidadas (menor `last_played`) primero.
                entries.sort_by_key(|e| e.last_played);
                entries
                    .into_iter()
                    .take(MAX_TRACKS)
                    .map(|e| e.to_track())
                    .collect()
            }
        }
    }
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ProviderKind;

    fn track(id: &str, artist: &str) -> Track {
        Track {
            id: id.to_string(),
            title: format!("título {id}"),
            artist: artist.to_string(),
            duration: None,
            source_url: format!("https://example.com/{id}"),
            provider: ProviderKind::Youtube,
        }
    }

    fn favorites_with(tracks: &[Track]) -> Favorites {
        let mut favorites = Favorites::default();
        favorites.tracks = tracks.to_vec();
        favorites
    }

    #[test]
    fn top_takes_most_played_capped() {
        let mut stats = Stats::default();
        for i in 0..(MAX_TRACKS as u64 + 10) {
            let t = track(&format!("t{i}"), "a");
            stats.record(&t);
            // Repeticiones decrecientes: t0 es la más escuchada.
            for _ in 0..(MAX_TRACKS as u64 + 10 - i) {
                stats.record(&t);
            }
        }
        let tracks = SmartKind::Top.tracks(&stats, &Favorites::default());
        assert_eq!(tracks.len(), MAX_TRACKS);
        assert_eq!(tracks[0].id, "t0");
    }

    #[test]
    fn unheard_keeps_only_favorites_without_plays() {
        let heard = track("oída", "a");
        let pending = track("pendiente", "b");
        let mut stats = Stats::default();
        stats.record(&heard);
        let favorites = favorites_with(&[heard.clone(), pending.clone()]);
        let tracks = SmartKind::Unheard.tracks(&stats, &favorites);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, "pendiente");
    }

    #[test]
    fn unheard_treats_zero_play_entries_as_unheard() {
        // Una entrada con plays = 0 (posible vía archivo manipulado) no
        // convierte el favorito en «oída».
        let fav = track("fav", "a");
        let mut stats = Stats::default();
        stats.record(&track("otra", "b"));
        stats.entries.push(crate::stats::StatsEntry {
            provider: ProviderKind::Youtube,
            id: "fav".to_string(),
            title: "fav".to_string(),
            artist: "a".to_string(),
            duration_secs: None,
            source_url: String::new(),
            plays: 0,
            last_played: 0,
        });
        let tracks = SmartKind::Unheard.tracks(&stats, &favorites_with(&[fav]));
        assert_eq!(tracks.len(), 1);
    }

    #[test]
    fn rediscover_picks_forgotten_tracks_oldest_first() {
        let mut stats = Stats::default();
        let recent = track("reciente", "a");
        let forgotten = track("olvidada", "b");
        let ancient = track("antigua", "c");
        stats.record(&recent);
        stats.record(&forgotten);
        stats.record(&ancient);
        let now = now_epoch();
        let cutoff = now - REDISCOVER_DAYS * 86_400;
        for (id, last) in [("reciente", now), ("olvidada", cutoff - 100), ("antigua", cutoff - 200_000)] {
            stats.entries.iter_mut().find(|e| e.id == id).unwrap().last_played = last;
        }
        let tracks = SmartKind::Rediscover.tracks(&stats, &Favorites::default());
        assert_eq!(tracks.len(), 2, "la reciente queda fuera");
        assert_eq!(tracks[0].id, "antigua");
        assert_eq!(tracks[1].id, "olvidada");
    }

    #[test]
    fn rediscover_never_played_entries_are_excluded() {
        // last_played = 0 (nunca oída, solo encolada sin sonar o entrada
        // manipulada) no es «redescubrimiento»: no se ha escuchado aún.
        let mut stats = Stats::default();
        stats.record(&track("x", "a"));
        stats.record(&track("y", "b"));
        stats.entries.iter_mut().find(|e| e.id == "x").unwrap().last_played = 1_000_000;
        stats.entries.iter_mut().find(|e| e.id == "y").unwrap().last_played = 0;
        stats.entries.iter_mut().find(|e| e.id == "y").unwrap().plays = 3;
        let tracks = SmartKind::Rediscover.tracks(&stats, &Favorites::default());
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, "x");
    }

    #[test]
    fn from_name_matches_case_insensitively() {
        assert_eq!(SmartKind::from_name("más escuchadas"), Some(SmartKind::Top));
        assert_eq!(SmartKind::from_name("SIN ESTRENAR"), Some(SmartKind::Unheard));
        assert_eq!(SmartKind::from_name("redescubrir"), Some(SmartKind::Rediscover));
        assert_eq!(SmartKind::from_name("otra"), None);
    }
}
