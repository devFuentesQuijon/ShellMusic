use shellmusic::providers::MusicProvider;
use shellmusic::providers::youtube::YoutubeProvider;

#[tokio::test]
#[ignore = "requiere yt-dlp en PATH y acceso a red"]
async fn search_daft_punk_returns_results() {
    let provider = YoutubeProvider::new();
    let tracks = provider
        .search("daft punk", 15)
        .await
        .expect("la búsqueda debe tener éxito");
    assert!(
        tracks.len() >= 10,
        "se esperaban ≥10 resultados, llegaron {}",
        tracks.len()
    );
    let first = &tracks[0];
    assert!(!first.title.is_empty());
    assert!(first.source_url.starts_with("http"));
    println!(
        "{} resultados; primero: «{}» — {} ({:?})",
        tracks.len(),
        first.title,
        first.artist,
        first.duration
    );
}
