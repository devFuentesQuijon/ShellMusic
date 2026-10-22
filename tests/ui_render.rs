use ratatui::Terminal;
use ratatui::backend::TestBackend;

use shellmusic::app::{App, Focus};
use shellmusic::config::Config;
use shellmusic::providers::{ProviderKind, Track};
use shellmusic::tui;

fn track(id: &str, title: &str) -> Track {
    Track {
        id: id.to_string(),
        title: title.to_string(),
        artist: "artista".to_string(),
        duration: Some(std::time::Duration::from_secs(200)),
        source_url: format!("https://example.com/{id}"),
        provider: ProviderKind::Youtube,
    }
}


fn test_app() -> App {
    App::with_data_dir(
        &Config::default(),
        &std::env::temp_dir().join("shellmusic-ui-test-datos"),
    )
    .unwrap()
}

fn render(app: &mut App) -> String {
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| tui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

#[test]
fn queue_panel_lists_tracks_with_current_marker() {
    let mut app = test_app();
    app.queue.add(track("a", "Primera"));
    app.queue.add(track("b", "Segunda"));
    app.queue.add(track("c", "Tercera"));
    app.queue.current = Some(1);
    app.queue_select(1);
    app.focus = Focus::Queue;

    let screen = render(&mut app);
    assert!(screen.contains("Cola (2/3)"), "título con posición:\n{screen}");
    assert!(screen.contains("1. Primera"), "pista 1:\n{screen}");
    assert!(screen.contains("♪ 2. Segunda"), "marcador actual:\n{screen}");
    assert!(screen.contains("▶ ♪ 2. Segunda"), "selección sobre actual:\n{screen}");
    assert!(screen.contains("3. Tercera"), "pista 3:\n{screen}");
}

#[test]
fn player_bar_shows_queue_position() {
    let mut app = test_app();
    app.queue.add(track("a", "Primera"));
    app.queue.add(track("b", "Segunda"));
    app.queue.current = Some(0);

    let screen = render(&mut app);
    assert!(screen.contains("cola 1/2"), "posición en cola:\n{screen}");
    assert!(screen.contains("▶ Primera"), "pista actual:\n{screen}");
}

#[test]
fn empty_queue_shows_hint() {
    let mut app = test_app();
    let screen = render(&mut app);
    assert!(screen.contains("Cola (0)"), "título vacío:\n{screen}");
    assert!(
        screen.contains("Vacía — añade con 'a'"),
        "hint de cola vacía:\n{screen}"
    );
}

#[test]
fn favorites_panel_lists_tracks_with_star() {
    let mut app = test_app();
    app.favorites.add(track("a", "Favorita Uno"));
    app.favorites.add(track("b", "Favorita Dos"));
    app.favorites_select(0);
    app.focus = Focus::Favorites;

    let screen = render(&mut app);
    assert!(screen.contains("Favoritos (2)"), "título:\n{screen}");
    assert!(screen.contains("★ 1. Favorita Uno"), "pista 1:\n{screen}");
    assert!(screen.contains("★ 2. Favorita Dos"), "pista 2:\n{screen}");
}

#[test]
fn results_mark_favorites_with_star() {
    let mut app = test_app();
    app.results = vec![track("a", "Normal"), track("b", "Favorita")];
    app.favorites.add(track("b", "Favorita"));

    let screen = render(&mut app);
    assert!(screen.contains("★ Favorita"), "estrella en resultados:\n{screen}");
    assert!(!screen.contains("★ Normal"), "sin estrella en no favorita:\n{screen}");
}

#[test]
fn playlist_overlay_list_and_content() {
    use shellmusic::app::Overlay;
    let mut app = test_app();
    let pl = app.playlists.create("prueba").unwrap();
    app.playlists.add_track(pl, track("a", "Pista A"));
    app.playlists.add_track(pl, track("b", "Pista B"));
    app.overlay_list_select(0);

    app.overlay = Overlay::List;
    let screen = render(&mut app);
    assert!(screen.contains("Playlists (P)"), "título overlay:\n{screen}");
    assert!(screen.contains("prueba (2)"), "entrada con contador:\n{screen}");
    assert!(screen.contains("n nueva"), "hints:\n{screen}");

    app.overlay = Overlay::Content(0);
    app.overlay_content_select(0, 0);
    let screen = render(&mut app);
    assert!(screen.contains("Playlist: prueba (2)"), "título contenido:\n{screen}");
    assert!(screen.contains("1. Pista A"), "pista 1:\n{screen}");
    assert!(screen.contains("2. Pista B"), "pista 2:\n{screen}");
}

#[test]
fn pick_overlay_offers_new_playlist_entry() {
    use shellmusic::app::Overlay;
    let mut app = test_app();
    app.overlay = Overlay::Pick(track("x", "Canción X"));
    app.overlay_list_state.select(Some(0));
    let screen = render(&mut app);
    assert!(screen.contains("Añadir a playlist: Canción X"), "título pick:\n{screen}");
    assert!(screen.contains("+ nueva playlist…"), "entrada nueva:\n{screen}");
}

#[test]
fn help_overlay_lists_keybindings() {
    use shellmusic::app::Overlay;
    let mut app = test_app();
    app.overlay = Overlay::Help;
    let screen = render(&mut app);
    assert!(screen.contains("Ayuda"), "título de ayuda:\n{screen}");
    assert!(screen.contains("seek"), "seek documentado:\n{screen}");
    assert!(screen.contains("repetición"), "repetición documentada:\n{screen}");
    assert!(screen.contains("barajar"), "shuffle documentado:\n{screen}");
}

#[test]
fn player_bar_shows_repeat_mode() {
    use shellmusic::app::RepeatMode;
    let mut app = test_app();
    app.queue.add(track("a", "Primera"));
    app.queue.current = Some(0);
    app.repeat = RepeatMode::All;

    let screen = render(&mut app);
    assert!(screen.contains("rep:all"), "indicador de repetición:\n{screen}");
}

#[test]
fn player_bar_truncates_long_titles() {
    let mut app = test_app();
    app.queue.add(track("a", &"Título extremadamente largo ".repeat(10)));
    app.queue.current = Some(0);
    app.radio = true;

    let screen = render(&mut app);
    // Los metadatos del final de la barra no se pierden con títulos largos.
    assert!(screen.contains("radio:ON"), "indicadores visibles:\n{screen}");
    assert!(screen.contains('…'), "título truncado:\n{screen}");
}
