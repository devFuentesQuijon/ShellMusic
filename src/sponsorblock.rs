//! SponsorBlock (RF-67): salto automático de segmentos de los vídeos de
//! YouTube (sponsor, intro, outro…) usando la API pública de
//! <https://sponsor.ajay.app>. Opt-in (`[sponsorblock] enabled`): cada
//! reproducción envía el ID del vídeo a ese servidor, como documenta la
//! propia configuración.

use anyhow::{Context, Result};
use serde_json::Value;

const API: &str = "https://sponsor.ajay.app/api/skipSegments";

/// Tramo marcado por la comunidad: segundos de inicio/fin y categoría.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub category: String,
}

/// Pide los segmentos de un vídeo. `Not Found` = sin segmentos: lista vacía.
pub async fn fetch(
    client: &reqwest::Client,
    video_id: &str,
    categories: &[String],
) -> Result<Vec<Segment>> {
    if categories.is_empty() {
        return Ok(Vec::new());
    }
    let categories = serde_json::to_string(categories)?;
    let response = client
        .get(API)
        .query(&[("videoID", video_id), ("categories", &categories)])
        .send()
        .await
        .context("no se pudo contactar con SponsorBlock")?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    let text = response
        .error_for_status()
        .context("SponsorBlock devolvió un error")?
        .text()
        .await?;
    parse_segments(&text)
}

/// Parsea la respuesta de la API: array de arrays
/// `[start, end, category, uuid, …]` (los campos extra varían por versión).
pub fn parse_segments(text: &str) -> Result<Vec<Segment>> {
    let items: Value =
        serde_json::from_str(text).context("respuesta de SponsorBlock inválida")?;
    let items = items
        .as_array()
        .context("respuesta de SponsorBlock sin lista")?;
    let mut segments = Vec::new();
    for item in items {
        let Some(fields) = item.as_array() else {
            continue;
        };
        if let (Some(start), Some(end), Some(category)) = (
            fields.first().and_then(Value::as_f64),
            fields.get(1).and_then(Value::as_f64),
            fields.get(2).and_then(Value::as_str),
        ) {
            segments.push(Segment {
                start,
                end,
                category: category.to_string(),
            });
        }
    }
    Ok(segments)
}

/// Tramo que toca saltar en `position` (segundos): uno sin saltar aún que
/// contenga la posición y con al menos `MIN_SKIP` s restantes (un seek al
/// final del tramo es más molesto que dejarlo sonar).
pub fn skip_target(segments: &[Segment], position: f64, skipped: &std::collections::HashSet<usize>) -> Option<usize> {
    const MIN_SKIP: f64 = 1.5;
    segments
        .iter()
        .enumerate()
        .position(|(index, segment)| {
            !skipped.contains(&index)
                && position >= segment.start
                && position < segment.end - MIN_SKIP
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_segments_accepts_standard_response() {
        // Formato real: [inicio, fin, categoría, uuid, duración]
        let json = r#"[[5.2, 12.0, "sponsor", "uuid-1", 0], [100.0, 105.5, "intro", "uuid-2", 0]]"#;
        let segments = parse_segments(json).unwrap();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0], Segment {
            start: 5.2,
            end: 12.0,
            category: "sponsor".to_string(),
        });
        assert_eq!(segments[1].category, "intro");
    }

    #[test]
    fn parse_segments_rejects_garbage() {
        assert!(parse_segments("no json").is_err());
        assert!(parse_segments("{}").is_err());
        // Items incompletos se ignoran sin tumbar el resto.
        let segments = parse_segments(r#"[[1.0, 2.0, "sponsor"], [3.0]]"#).unwrap();
        assert_eq!(segments.len(), 1);
    }

    #[test]
    fn skip_target_only_picks_unskipped_with_room() {
        let segments = vec![
            Segment { start: 10.0, end: 20.0, category: "sponsor".into() },
            Segment { start: 30.0, end: 31.0, category: "intro".into() },
        ];
        let mut skipped = std::collections::HashSet::new();
        assert_eq!(skip_target(&segments, 5.0, &skipped), None);
        assert_eq!(skip_target(&segments, 10.0, &skipped), Some(0));
        assert_eq!(skip_target(&segments, 19.0, &skipped), None, "a <1,5 s del fin no merece seek");
        // El tramo de 1 s nunca se salta: no alcanza el mínimo.
        assert_eq!(skip_target(&segments, 30.0, &skipped), None);
        skipped.insert(0);
        assert_eq!(skip_target(&segments, 15.0, &skipped), None, "ya saltado");
    }
}
