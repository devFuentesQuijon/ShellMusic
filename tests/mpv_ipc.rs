use std::time::Duration;

use shellmusic::config::Youtube;
use shellmusic::player::Player;
use shellmusic::player::mpv::MpvPlayer;

/// Verificación end-to-end: mpv resuelve y reproduce una URL real de YouTube
/// con un navegador explícito (cookies_from_browser="firefox", sin modo auto).
#[tokio::test]
#[ignore = "requiere mpv, yt-dlp, Firefox con sesión de YouTube y acceso a red"]
async fn mpv_plays_youtube_track_and_position_advances() {
    let url = "https://www.youtube.com/watch?v=LdpMpfp-J_I";
    let youtube = Youtube {
        cookies_from_browser: "firefox".to_string(),
        ..Youtube::default()
    };
    let (player, mut events) = MpvPlayer::spawn_with_args(70, &youtube, &["--ao=null"])
        .await
        .expect("mpv debe lanzarse y aceptar IPC");
    player.play(url).await.expect("loadfile debe aceptarse");

    let mut last_position: Option<Duration> = None;
    let mut advanced = false;
    for _ in 0..45 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        while let Ok(event) = events.try_recv() {
            println!("evento mpv: {event:?}");
        }
        let status = player.status().await.expect("status debe responder");
        println!(
            "pos={:?} dur={:?} paused={} vol={}",
            status.position, status.duration, status.paused, status.volume
        );
        if let (Some(prev), Some(pos)) = (last_position, status.position)
            && pos > prev
        {
            advanced = true;
            break;
        }
        last_position = status.position;
    }
    assert!(advanced, "time-pos nunca avanzó");
}

/// Con una URL inválida, el error debe incluir el detalle real capturado
/// del stderr de mpv/yt-dlp, no el fallback "desconocido".
#[tokio::test]
#[ignore = "requiere mpv, yt-dlp y acceso a red"]
async fn mpv_reports_stderr_detail_on_failure() {
    let (player, _events) = MpvPlayer::spawn_with_args(70, &Youtube::default(), &["--ao=null"])
        .await
        .expect("mpv debe lanzarse");
    let err = player
        .play("https://www.youtube.com/watch?v=VIDEO_INEXISTENTE")
        .await
        .expect_err("un vídeo inexistente debe fallar");
    let message = err.to_string();
    println!("error reportado: {message}");
    assert!(
        !message.contains("desconocido"),
        "debe incluir el detalle real, no el fallback: {message}"
    );
    assert!(
        message.to_lowercase().contains("error") || message.to_lowercase().contains("youtube"),
        "el mensaje debe ser informativo: {message}"
    );
}
