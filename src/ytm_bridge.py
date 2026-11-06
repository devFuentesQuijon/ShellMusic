#!/usr/bin/env python3
"""Puente smusic <-> ytmusicapi (RF-68): un comando por invocacion, JSON de
una linea por salida (mismo sobre de respuesta que el socket de control:
{"ok": true, "data": ...} / {"ok": false, "error": ...}).

El host (Rust) aporta el path de los headers guardados con `smusic --ytm-auth`
y consume stdout. Acepta los headers en varios formatos de copia: JSON, bloque
«Nombre: valor» (vista cruda de DevTools), «Copiar como cURL» y «Copiar como
fetch»; con que incluyan la cookie de sesión basta, se completan el resto.
"""

import json
import re
import sys
import time
from hashlib import sha1
from http.cookies import SimpleCookie

ORIGIN = "https://music.youtube.com"
CHROME_UA = (
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36"
)
FIREFOX_UA = "Mozilla/5.0 (X11; Linux x86_64; rv:132.0) Gecko/20100101 Firefox/132.0"
BROWSERS = ["chrome", "chromium", "firefox", "edge", "brave", "opera", "vivaldi", "librewolf"]
FIREFOX_FAMILY = {"firefox", "librewolf"}


def fail(message):
    print(json.dumps({"ok": False, "error": str(message)}))
    sys.exit(0)  # el error viaja en el sobre; el exit != 0 lo reserva Rust para fallos de ejecucion


def ok(data):
    print(json.dumps({"ok": True, "data": data}))


def _header_get(headers, name):
    for key, value in headers.items():
        if key.lower() == name:
            return value
    return None


def sapisid_from_cookie(raw_cookie):
    """Extrae __Secure-3PAPISID de la cookie (mismo criterio que ytmusicapi)."""
    jar = SimpleCookie()
    try:
        jar.load(raw_cookie.replace('"', ""))
    except Exception:
        return None
    morsel = jar.get("__Secure-3PAPISID")
    return morsel.value if morsel else None


def sapisid_hash(sapisid, origin):
    """Authorization SAPISIDHASH de Google (ytmusicapi lo recalcula por
    petición desde la cookie y el origin; este valor solo fija el tipo de auth)."""
    ts = str(int(time.time()))
    digest = sha1(f"{ts} {sapisid} {origin}".encode("utf-8")).hexdigest()
    return f"SAPISIDHASH {ts}_{digest}"


def _parse_curl(text):
    """«Copiar como cURL»: argumentos -H 'Nombre: valor'."""
    headers = {}
    for _, pair in re.findall(r"-H\s+(['\"])(.*?)\1", text, flags=re.DOTALL):
        if ":" in pair:
            name, value = pair.split(":", 1)
            if name.strip():
                headers[name.strip()] = value.strip()
    return headers


def _fetch_headers_block(text):
    """Localiza el objeto `headers: {...}` de «Copiar como fetch» y devuelve
    su contenido entre llaves (escapando strings)."""
    match = re.search(r"[\"']?headers[\"']?\s*:\s*\{", text)
    if not match:
        return None
    start = match.end() - 1  # la llave de apertura
    depth, in_string, escaped = 0, False, False
    for pos in range(start, len(text)):
        char = text[pos]
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            continue
        if char == '"':
            in_string = True
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return text[start : pos + 1]
    return None


def _parse_fetch(text):
    """«Copiar como fetch»: pares "Nombre": "valor" del bloque headers."""
    block = _fetch_headers_block(text)
    if block is None:
        return {}
    headers = {}
    for raw_name, raw_value in re.findall(
        r'"((?:[^"\\]|\\.)*)"\s*:\s*"((?:[^"\\]|\\.)*)"', block
    ):
        try:
            name = json.loads(f'"{raw_name}"')
            value = json.loads(f'"{raw_value}"')
        except json.JSONDecodeError:
            continue
        if name:
            headers[name] = value
    return headers


def _parse_raw_lines(text):
    """Vista cruda de DevTools: lineas «Nombre: valor» (incluye
    pseudo-cabeceras HTTP/2 «:authority:», que se ignoran)."""
    headers = {}
    for line in text.splitlines():
        if ":" in line:
            name, value = line.split(":", 1)
            name = name.strip()
            if name:
                headers[name] = value.strip()
    return headers


def parse_headers_text(text):
    """Entiende los formatos de copia del navegador: JSON, cURL, fetch y
    bloque crudo. Devuelve {} si no se reconoce nada."""
    try:
        parsed = json.loads(text)
        if isinstance(parsed, dict):
            return dict(parsed)
    except json.JSONDecodeError:
        pass
    headers = _parse_curl(text)
    if headers:
        return headers
    headers = _parse_fetch(text)
    if headers:
        return headers
    return _parse_raw_lines(text)


def complete_headers(headers):
    """Rellena lo imprescindible que las copias no traen: origin y
    content-type (la vista fetch los omite por ser cabeceras prohibidas) y,
    si falta, el SAPISIDHASH calculado desde la cookie."""
    if _header_get(headers, "cookie") is not None and (
        _header_get(headers, "origin") is None and _header_get(headers, "x-origin") is None
    ):
        headers["origin"] = ORIGIN
    if _header_get(headers, "content-type") is None:
        headers["content-type"] = "application/json"
    if _header_get(headers, "user-agent") is None:
        headers["user-agent"] = CHROME_UA
    if _header_get(headers, "authorization") is None:
        sapisid = sapisid_from_cookie(str(_header_get(headers, "cookie") or ""))
        if sapisid:
            headers["authorization"] = sapisid_hash(sapisid, ORIGIN)
    return headers


def load_headers(path):
    with open(path, encoding="utf-8") as handle:
        text = handle.read().strip()
    headers = parse_headers_text(text)
    if not headers:
        raise ValueError(
            "los headers no se entienden (JSON, cURL, fetch o bloque «Nombre: valor»)"
        )
    return complete_headers(headers)


def check_headers(headers):
    """ytmusicapi exige la cookie de sesión con __Secure-3PAPISID para tratar
    los headers como auth de navegador (sin ella asume OAuth y el error
    confunde); el origin y el SAPISIDHASH ya los garantiza complete_headers."""
    cookie = _header_get(headers, "cookie")
    if not cookie:
        raise ValueError(
            "los headers no incluyen la cookie de la sesión; copia las cabeceras "
            "completas de la petición «browse» o usa: smusic --ytm-auth --from-browser"
        )
    if not sapisid_from_cookie(str(cookie)):
        raise ValueError(
            "la cookie no incluye __Secure-3PAPISID; vuelve a copiar las cabeceras "
            "de la petición «browse» estando logueado en music.youtube.com"
        )


def _jar_to_cookie_string(jar):
    return "; ".join(f"{c.name}={c.value}" for c in jar if c.value)


def auth_from_browser(browser):
    """Extrae los headers de la sesión de YouTube desde las cookies del
    navegador (browser_cookie3); «auto» prueba los soportados en orden."""
    try:
        import browser_cookie3
    except ImportError:
        raise ValueError(
            "falta browser_cookie3: instálalo con «pip install browser-cookie3» "
            "o pega los headers manualmente (smusic --ytm-auth)"
        )
    names = BROWSERS if browser == "auto" else [browser]
    tried = []
    for name in names:
        loader = getattr(browser_cookie3, name, None)
        if loader is None:
            if browser == "auto":
                continue
            valid = ", ".join(["auto"] + BROWSERS)
            raise ValueError(f"navegador no soportado: «{browser}» (válidos: {valid})")
        try:
            jar = loader(domain_name="youtube.com")
        except Exception as err:
            tried.append(f"{name}: {err}")
            continue
        cookie = _jar_to_cookie_string(jar)
        if not sapisid_from_cookie(cookie):
            tried.append(f"{name}: sin sesión de YouTube")
            continue
        return {
            "accept": "*/*",
            "authorization": sapisid_hash(sapisid_from_cookie(cookie), ORIGIN),
            "content-type": "application/json",
            "cookie": cookie,
            "origin": ORIGIN,
            "user-agent": FIREFOX_UA if name in FIREFOX_FAMILY else CHROME_UA,
        }
    detail = "; ".join(tried) or "sin navegadores que probar"
    if browser == "auto":
        raise ValueError(
            "no se encontró una sesión de YouTube Music en los navegadores "
            f"({detail}); abre music.youtube.com, loguéate y reintenta, "
            "o pega los headers manualmente (smusic --ytm-auth)"
        )
    raise ValueError(
        f"no se pudo extraer la sesión de {browser} ({detail}); prueba con "
        "otro navegador o pega los headers manualmente (smusic --ytm-auth)"
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

    # Ops que no necesitan ytmusicapi ni los headers guardados.
    if command == "auth_browser":
        try:
            headers = auth_from_browser(args[0] if args else "auto")
        except ValueError as err:
            fail(err)
            return
        ok(headers)
        return
    if command == "auth_check":
        try:
            check_headers(load_headers(headers_path))
        except Exception as err:
            fail(f"headers no válidos: {err}")
            return
        ok({})
        return

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
