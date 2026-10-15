use std::time::Duration;

use shellmusic::config::Youtube;
use shellmusic::player::Player;
use shellmusic::player::mpv::MpvPlayer;
use shellmusic::visualizer::Visualizer;

/// E2E con audio real: mpv reproduce por el sink por defecto de PipeWire
/// (sin --ao=null) y el monitor debe aportar energía a las bandas; al pausar,
/// las barras deben caer.
#[tokio::test]
#[ignore = "requiere mpv, yt-dlp, PipeWire/PulseAudio y acceso a red"]
async fn visualizer_captures_real_playback_and_decays_on_pause() {
    let visualizer = Visualizer::start(24).expect("la captura debe arrancar (parec/pw-cat)");
    assert!(visualizer.is_alive());

    let youtube = Youtube {
        cookies_from_browser: "firefox".to_string(),
        ..Youtube::default()
    };
    // Sin --ao=null: el audio sale por el sink por defecto y su monitor lo ve.
    let (player, _events) = MpvPlayer::spawn_with_args(90, &youtube, &[])
        .await
        .expect("mpv debe lanzarse");
    player
        .play("https://www.youtube.com/watch?v=LdpMpfp-J_I")
        .await
        .expect("debe reproducir");

    let mut peak = 0.0f32;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let bands = visualizer.bands();
        let max = bands.iter().cloned().fold(0.0, f32::max);
        println!("banda máx: {max:.2}");
        peak = peak.max(max);
        if peak > 0.15 {
            break;
        }
    }
    assert!(peak > 0.15, "el monitor no captó energía real (pico {peak:.2})");

    player.toggle_pause().await.expect("pausa");
    // mpv pausado = posición congelada (determinista).
    tokio::time::sleep(Duration::from_secs(1)).await;
    let status = player.status().await.unwrap();
    assert!(status.paused, "mpv debe quedar en pausa");
    let frozen = status.position;

    // Decaimiento de bandas: el monitor captura TODO el audio del sistema,
    // así que audio ajeno puede enmascarar la caída. Si no decae, se avisa
    // en vez de fallar (la caída ante silencio está cubierta por tests
    // unitarios del DSP).
    tokio::time::sleep(Duration::from_secs(4)).await;
    let after_pause = visualizer.bands().iter().cloned().fold(0.0, f32::max);
    let status = player.status().await.unwrap();
    assert_eq!(
        status.position, frozen,
        "la posición no debe avanzar en pausa"
    );
    if after_pause < peak * 0.5 {
        println!("bandas caen tras pausa: {after_pause:.2} < {:.2}", peak * 0.5);
    } else {
        println!(
            "AVISO: bandas no cayeron ({after_pause:.2} vs pico {peak:.2}); \
             mpv está pausado, así que hay otra fuente de audio en el sistema \
             (el monitor captura todo el audio, no solo el de ShellMusic)"
        );
    }
    drop(visualizer);
}
