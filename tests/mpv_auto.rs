use std::sync::Arc;
use std::time::Duration;

use shellmusic::config::Youtube;
use shellmusic::player::{Player, PlayerEvent};
use shellmusic::player::mpv::MpvPlayer;

const URL_1: &str = "https://www.youtube.com/watch?v=LdpMpfp-J_I";
const URL_2: &str = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";

async fn wait_position_advances(player: &MpvPlayer) {
    let mut last: Option<Duration> = None;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let status = player.status().await.expect("status debe responder");
        println!("pos={:?} dur={:?}", status.position, status.duration);
        if let (Some(prev), Some(pos)) = (last, status.position)
            && pos > prev
        {
            return;
        }
        last = status.position;
    }
    panic!("time-pos nunca avanzó");
}

/// Modo auto con la config por defecto: el primer intento (sin cookies, con
/// player_client=android) funciona; la segunda pista usa el intento cacheado
/// sin reintentos.
#[tokio::test]
#[ignore = "requiere mpv, yt-dlp y acceso a red"]
async fn auto_mode_fast_path_and_session_cache() {
    let youtube = Youtube {
        cookies_from_browser: "auto".to_string(),
        ..Youtube::default()
    };
    let (player, mut events) = MpvPlayer::spawn_with_args(70, &youtube, &["--ao=null"])
        .await
        .expect("mpv debe lanzarse");

    player.play(URL_1, None).await.expect("el modo auto debe reproducir");
    let mut retried = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let PlayerEvent::Notice(notice) = event {
            println!("aviso play1: {}", notice.text);
            retried.push(notice.text);
        }
    }
    println!("reintentos en play1: {}", retried.len());
    wait_position_advances(&player).await;

    player
        .play(URL_2, None)
        .await
        .expect("la segunda pista debe reproducir con la caché");
    let mut second = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let PlayerEvent::Notice(notice) = event {
            println!("aviso play2: {}", notice.text);
            second.push(notice.text);
        }
    }
    assert!(
        !second.iter().any(|text| text.contains("reintentando")),
        "el segundo play debe usar el intento cacheado sin reintentos: {second:?}"
    );
    wait_position_advances(&player).await;
}

/// Modo auto sin asistencias (player_client/remote_components vacíos): el
/// intento sin cookies falla con error anti-bot y la cadena de reintentos se
/// dispara con los navegadores detectados (firefox primero).
#[tokio::test]
#[ignore = "requiere mpv, yt-dlp, navegador con sesión de YouTube y acceso a red"]
async fn auto_mode_retries_detected_browsers_on_block() {
    let youtube = Youtube {
        cookies_from_browser: "auto".to_string(),
        player_client: String::new(),
        remote_components: String::new(),
    };
    let (player, mut events) = MpvPlayer::spawn_with_args(70, &youtube, &["--ao=null"])
        .await
        .expect("mpv debe lanzarse");
    let player = Arc::new(player);

    let player_task = Arc::clone(&player);
    let mut play_handle = tokio::spawn(async move { player_task.play(URL_1, None).await });

    let mut saw_firefox_retry = false;
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        while let Ok(event) = events.try_recv() {
            if let PlayerEvent::Notice(notice) = event {
                println!("aviso: {}", notice.text);
                if notice.text.contains("reintentando con cookies de firefox") {
                    saw_firefox_retry = true;
                }
            }
        }
        if saw_firefox_retry {
            break;
        }
    }
    assert!(
        saw_firefox_retry,
        "la cadena de reintentos debe dispararse con firefox (detectado en el sistema)"
    );

    let outcome = tokio::time::timeout(Duration::from_secs(240), &mut play_handle).await;
    match outcome {
        Ok(Ok(Ok(()))) => {
            println!("un navegador con cookies resolvió el stream");
            wait_position_advances(&player).await;
        }
        Ok(Ok(Err(err))) => {
            println!("todos los intentos fallaron (esperado desde IPs muy limitadas): {err}");
            let message = err.to_string().to_lowercase();
            assert!(
                message.contains("youtube") || message.contains("espera"),
                "el error debe ser informativo: {err}"
            );
        }
        Ok(Err(join_err)) => panic!("la tarea de play falló: {join_err}"),
        Err(_) => println!("play no terminó en el margen del test; reintentos ya verificados"),
    }
}
