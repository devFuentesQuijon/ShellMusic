#!/usr/bin/env python3
"""Puente smusic <-> ytmusicapi (RF-68): un comando por invocacion, JSON de
una linea por salida (mismo sobre de respuesta que el socket de control:
{"ok": true, "data": ...} / {"ok": false, "error": ...}).

El host (Rust) aporta el path de los headers guardados con `smusic --ytm-auth`
y consume stdout. Acepta los headers como JSON (lo que copia el navegador) o
como bloque "Nombre: valor" (copia cruda de herramientas de desarrollo).
"""

import json
import sys


def fail(message):
    print(json.dumps({"ok": False, "error": str(message)}))
    sys.exit(0)  # el error viaja en el sobre; el exit != 0 lo reserva Rust para fallos de ejecucion


def ok(data):
    print(json.dumps({"ok": True, "data": data}))


def load_headers(path):
    with open(path, encoding="utf-8") as handle:
        text = handle.read().strip()
    try:
        parsed = json.loads(text)
        if isinstance(parsed, dict):
            return parsed
    except json.JSONDecodeError:
        pass
    # Formato crudo de "copiar cabeceras": lineas "Nombre: valor".
    headers = {}
    for line in text.splitlines():
        if ":" in line:
            name, value = line.split(":", 1)
            headers[name.strip()] = value.strip()
    if not headers:
        raise ValueError("los headers no se entienden (ni JSON ni bloque «Nombre: valor»)")
    return headers


def check_headers(headers):
    """ytmusicapi solo trata los headers como auth de navegador si incluyen la
    cabecera Authorization con SAPISIDHASH (viene en la copia de la petición
    /browse); sin ella asume OAuth y el error que da confunde."""
    authorization = next(
        (str(v) for k, v in headers.items() if k.lower() == "authorization"), ""
    )
    if "SAPISIDHASH" not in authorization:
        raise ValueError(
            "los headers no incluyen la cabecera Authorization (SAPISIDHASH); "
            "copia las cabeceras completas de la petición «browse»"
        )


def norm(song):
    """Dict de ytmusicapi -> pista del protocolo de ShellMusic."""
    video_id = song.get("videoId") or ""
    if not video_id:
        return None
    artists = [a["name"] for a in (song.get("artists") or []) if a.get("name")]
    artist = ", ".join(artists) or song.get("artist") or "Desconocido"
    duration = song.get("duration_seconds")
    return {
        "id": video_id,
        "title": song.get("title") or "",
        "artist": artist,
        "duration_secs": duration if isinstance(duration, int) else None,
        "url": "https://music.youtube.com/watch?v=" + video_id,
        "provider": "youtube",
    }


def norm_tracks(songs):
    return [track for track in (norm(s) for s in (songs or [])) if track]


def main():
    if len(sys.argv) < 3:
        fail("uso: ytm_bridge.py <headers> <comando> [args]")
        return
    headers_path, command = sys.argv[1], sys.argv[2]
    args = sys.argv[3:]

    try:
        from ytmusicapi import YTMusic
    except ImportError:
        fail("falta ytmusicapi: instálalo con «pip install ytmusicapi»")
        return

    try:
        headers = load_headers(headers_path)
        check_headers(headers)
        ytm = YTMusic(auth=headers)
    except Exception as err:  # cookies caducas, formato raro...
        fail(f"no se pudo autenticar con los headers guardados ({err}); rehaz: smusic --ytm-auth")
        return

    try:
        if command == "likes":
            data = ytm.get_liked_songs(limit=10_000)
            ok({"tracks": norm_tracks(data.get("tracks"))})
        elif command == "playlists":
            playlists = ytm.get_library_playlists(limit=None) or []
            ok({"playlists": [{"id": p.get("playlistId"), "name": p.get("title")} for p in playlists]})
        elif command == "import":
            if not args:
                fail("falta el nombre de la playlist a importar")
                return
            wanted = args[0].lower()
            playlists = ytm.get_library_playlists(limit=None) or []
            match = next((p for p in playlists if wanted in (p.get("title") or "").lower()), None)
            if match is None:
                fail(f"playlist no encontrada en tu biblioteca de YouTube Music: «{args[0]}»")
                return
            data = ytm.get_playlist(playlistId=match["playlistId"], limit=None)
            ok({"name": match.get("title") or args[0], "tracks": norm_tracks(data.get("tracks"))})
        elif command == "home":
            shelves = ytm.get_home(limit=3) or []
            songs = [item for shelf in shelves for item in (shelf.get("contents") or [])]
            ok({"tracks": norm_tracks(songs)})
        elif command == "radio":
            if not args:
                fail("falta el videoId de la pista semilla")
                return
            data = ytm.get_watch_playlist(videoId=args[0], radio=True, limit=50)
            tracks = [t for t in norm_tracks(data.get("tracks")) if t["id"] != args[0]]
            ok({"tracks": tracks})
        elif command in ("like", "unlike"):
            if not args:
                fail("falta el videoId de la pista")
                return
            rating = "LIKE" if command == "like" else "INDIFFERENT"
            try:
                from ytmusicapi.models.content.enums import LikeStatus
                rating = LikeStatus(rating)
            except Exception:
                pass  # versiones que aceptan el literal directamente
            ytm.rate_song(args[0], rating)
            ok({})
        else:
            fail(f"comando desconocido: {command}")
    except Exception as err:
        fail(err)


if __name__ == "__main__":
    main()
