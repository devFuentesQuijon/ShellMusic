use std::time::Duration;

use shellmusic::config::Youtube;
use shellmusic::player::Player;
use shellmusic::player::mpv::MpvPlayer;

/// Verifica el cliente IPC de mpv con un archivo local (sin red):
/// loadfile, observe_property (time-pos/duration/volume) y set_property.
#[tokio::test]
#[ignore = "requiere mpv en PATH"]
async fn mpv_ipc_local_file_position_advances() {
    let fixture = std::env::temp_dir().join("shellmusic-test-tone.mp3");
    if !fixture.exists() {
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-f", "lavfi", "-i", "sine=frequency=440:duration=30",
                "-y",
            ])
            .arg(&fixture)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("ffmpeg debe estar disponible para generar el fixture");
        assert!(status.success());
    }
    let (player, _events) = MpvPlayer::spawn_with_args(70, &Youtube::default(), &["--ao=null"])
        .await
        .expect("mpv debe lanzarse y aceptar IPC");

    player
        .set_volume(55)
        .await
        .expect("set_property volume debe aceptarse");
    player
        .play(fixture.to_str().unwrap(), None)
        .await
        .expect("loadfile debe aceptarse");

    let mut last_position: Option<Duration> = None;
    let mut advanced = false;
    for _ in 0..15 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let status = player.status().await.expect("status debe responder");
        println!(
            "pos={:?} dur={:?} paused={} vol={}",
            status.position, status.duration, status.paused, status.volume
        );
        assert_eq!(status.volume, 55, "el volumen observado debe ser 55");
        if let (Some(prev), Some(pos)) = (last_position, status.position)
            && pos > prev
        {
            advanced = true;
            break;
        }
        last_position = status.position;
    }
    assert!(advanced, "time-pos nunca avanzó");

    player.toggle_pause().await.expect("cycle pause aceptado");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(player.status().await.unwrap().paused);

    player.stop().await.expect("stop aceptado");

    // Reanudación de sesión: play con start debe dejar la posición cerca del
    // punto indicado (seek absoluto tras el arranque).
    player
        .play(fixture.to_str().unwrap(), Some(Duration::from_secs(15)))
        .await
        .expect("play con start debe aceptarse");
    let mut resumed = false;
    for _ in 0..10 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let status = player.status().await.expect("status debe responder");
        println!("resume pos={:?}", status.position);
        if let Some(pos) = status.position
            && pos >= Duration::from_secs(14)
        {
            resumed = true;
            break;
        }
    }
    assert!(resumed, "la posición no reanudó cerca de los 15 s");
}
