#!/usr/bin/env python3
"""Verificación E2E del checklist MVP (SPEC.md §11) y de la fase 18 vía PTY.

Ejecuta la app real en una terminal emulada (pexpect + pyte), con datos
aislados en un directorio temporal, y comprueba: arranque < 1 s con la caja
de búsqueda con foco, búsqueda con resultados, reproducción < 5 s, controles
sin reiniciar, progreso real, errores en UI sin cerrar la app, URL pegada
resuelta directamente y copia al portapapeles.

Requiere `pexpect` y `pyte` (mismas dependencias que record_demo.py) y un
binario compilado. Reproduce audio real: úsalo en una sesión con altavoces.

    python3 scripts/e2e_check.py                  # suite completa
    python3 scripts/e2e_check.py --dumb-startup   # + arranque sin respuesta gráfica

El harness emula un terminal que responde la query de capacidades gráficas
de ratatui-image (kitty), igual que la terminal del usuario; sin respuesta,
la query consume su timeout completo y retrasa el primer frame. Los tests
que requieren red fallan si YouTube responde con bloqueo anti-bot: reintenta
más tarde o revisa docs/anti-bot.md.
"""

import argparse
import os
import re
import sys
import tempfile
import time
from pathlib import Path

import pexpect
import pyte

REPO_ROOT = Path(__file__).resolve().parent.parent

# Respuesta de terminal kitty ante la query de ratatui-image: kitty-OK +
# DA (sin sixel) + tamaño de celda + status report (terminador que espera
# el parser). Sin ella la query espera su timeout.
TERM_RESPONSE = (
    b"\x1b_Gi=31;OK\x1b\\"
    b"\x1b[?1;2c"
    b"\x1b[6;20;10t"
    b"\x1b[0n"
)

results = []


def check(name, ok, detail=""):
    results.append((name, ok, detail))
    print(f"  [{'OK' if ok else 'FALLA'}] {name} {detail}", flush=True)


def screen_text(screen):
    out = []
    for y in range(screen.lines):
        out.append(
            "".join(screen.buffer[y][x].data for x in range(screen.columns))
        )
    return "\n".join(out)


class Ui:
    """App smusic corriendo en un PTY, con terminal gráfico emulado."""

    def __init__(self, env, binary, answer=True, args=None):
        self.screen = pyte.Screen(240, 32)
        self.stream = pyte.ByteStream(self.screen)
        self.answer = answer
        self._answered = False
        self.log_path = os.path.join(env["XDG_DATA_HOME"], "..", "e2e.log")
        self.log = open(self.log_path, "wb")
        self.child = pexpect.spawn(
            binary,
            args or [],
            dimensions=(32, 240),
            env=env,
            encoding=None,
            timeout=30,
            cwd=str(REPO_ROOT),
        )
        self.child.logfile = self.log

    def pump(self, duration):
        end = time.time() + duration
        while time.time() < end:
            try:
                data = self.child.read_nonblocking(4096, timeout=0.1)
                if data:
                    self.stream.feed(data)
                    if self.answer and not self._answered and b"\x1b[5n" in data:
                        self._answered = True
                        self.child.write(TERM_RESPONSE)
            except pexpect.TIMEOUT:
                pass
            except pexpect.EOF:
                break

    def send(self, keys):
        self.child.send(keys)

    def wait_for(self, pattern, timeout):
        deadline = time.time() + timeout
        while time.time() < deadline:
            self.pump(0.15)
            if re.search(pattern, screen_text(self.screen), re.IGNORECASE):
                return True
        return False

    def text(self):
        return screen_text(self.screen)

    def close(self):
        try:
            self.child.close()
        except Exception:
            pass
        self.log.close()


def bar_state(text):
    m = re.search(
        r"([▶⏸]) .*?(\d+):(\d\d)/(\d+):(\d\d)  cola (\d+)/(\d+)  vol:(\d+)",
        text,
    )
    if not m:
        return None
    return {
        "icon": m.group(1),
        "pos": int(m.group(2)) * 60 + int(m.group(3)),
        "dur": int(m.group(4)) * 60 + int(m.group(5)),
        "qi": int(m.group(6)),
        "qn": int(m.group(7)),
        "vol": int(m.group(8)),
    }


def make_env():
    tmp = tempfile.mkdtemp(prefix="smusic_e2e_")
    env = os.environ.copy()
    env["TERM"] = "xterm-256color"
    env["XDG_CONFIG_HOME"] = os.path.join(tmp, "config")
    env["XDG_DATA_HOME"] = os.path.join(tmp, "data")
    os.makedirs(env["XDG_CONFIG_HOME"], exist_ok=True)
    os.makedirs(env["XDG_DATA_HOME"], exist_ok=True)
    return env


def wait_bar(ui, timeout):
    deadline = time.time() + timeout
    while time.time() < deadline:
        ui.pump(0.3)
        state = bar_state(ui.text())
        if state:
            return state
    return None


def run_suite(binary):
    ui = Ui(make_env(), binary)

    # ── 1. Arranque < 1 s con la caja de búsqueda con foco ─────────────
    startups = []
    for attempt in range(3):
        t0 = time.time()
        found = ui.wait_for(r"Buscar en YouTube \(Enter: buscar", 10)
        startups.append(time.time() - t0)
        if attempt < 2:
            ui.send("q")
            ui.pump(0.8)
            ui.close()
            ui = Ui(make_env(), binary)
    startup = min(startups)
    check("arranque <1s + caja con foco", found and startup < 1.0,
          f"(mín {startup:.2f}s de {[f'{s:.2f}' for s in startups]})")

    # ── 2. Búsqueda "daft punk" ≥ 10 resultados ────────────────────────
    ui.send("daft punk")
    ui.pump(0.3)
    ui.send("\r")
    got = ui.wait_for(r"Resultados \(\d\d+\)", 30)
    ui.pump(1.0)
    text = ui.text()
    m = re.search(r"Resultados \((\d+)\)", text)
    n_res = int(m.group(1)) if m else 0
    n_dur = len([ln for ln in text.splitlines()
                 if re.search(r"\(\d+:\d\d\)", ln)])
    has_daft = "daft" in text.lower()
    check("búsqueda muestra resultados", got, f"({n_res} resultados)")
    check("≥10 resultados con título/artista/duración", n_res >= 10,
          f"({n_res}; duración visible en {n_dur} filas)")
    check("resultados pertenecen a la consulta", has_daft)

    # ── 3. Enter → reproducción < 5 s ───────────────────────────────────
    t0 = time.time()
    ui.send("\r")
    playing = ui.wait_for(r"▶ .*cola \d+/\d+  vol:\d+", 45)
    latency = time.time() - t0
    check("Enter inicia reproducción", playing, f"({latency:.1f}s)")
    check("latencia <5s", playing and latency < 5.0, f"({latency:.1f}s)")

    # ── 4. Barra de progreso real ───────────────────────────────────────
    s1 = wait_bar(ui, 15)
    time.sleep(4)
    ui.pump(0.5)
    s2 = bar_state(ui.text())
    if s1 and s2:
        delta = s2["pos"] - s1["pos"]
        check("progreso avanza ~tiempo real", 3 <= delta <= 6,
              f"(+{delta}s en 4s; {s1['pos']}→{s2['pos']} de {s2['dur']}s)")
        check("barra muestra posición y duración", s2["dur"] > 0,
              f"(dur {s2['dur']}s)")
    else:
        check("progreso avanza ~tiempo real", False, "barra no legible")
        check("barra muestra posición y duración", False)

    # ── 5. Controles sin reiniciar ──────────────────────────────────────
    ui.send(" ")
    check("espacio pausa", ui.wait_for(r"⏸ .*cola", 3))
    ui.send(" ")
    check("espacio reanuda", ui.wait_for(r"▶ .*cola", 3))

    # Encolar dos pistas más (down+a, down+a) y navegar con n/p
    ui.send("\x1b[B")
    ui.pump(0.3)
    ui.send("a")
    ui.pump(0.5)
    ui.send("\x1b[B")
    ui.pump(0.3)
    ui.send("a")
    ui.pump(0.8)
    before = wait_bar(ui, 10)
    ui.send("n")
    after_n = wait_bar(ui, 10)
    ui.send("p")
    after_p = wait_bar(ui, 10)
    ok_n = before and after_n and after_n["qi"] == before["qi"] + 1
    ok_p = after_p and before and after_p["qi"] == before["qi"]
    check("n avanza a la siguiente pista", ok_n,
          f"(cola {before['qi'] if before else '?'}→{after_n['qi'] if after_n else '?'})")
    check("p vuelve a la anterior", ok_p,
          f"(cola {after_n['qi'] if after_n else '?'}→{after_p['qi'] if after_p else '?'})")

    # Volumen + / -
    v0 = wait_bar(ui, 5)
    ui.send("+")
    v1 = wait_bar(ui, 5)
    ui.send("-")
    v2 = wait_bar(ui, 5)
    check("volumen sube con +",
          v0 and v1 and v1["vol"] > v0["vol"],
          f"({v0['vol'] if v0 else '?'}→{v1['vol'] if v1 else '?'})")
    check("volumen baja con -",
          v1 and v2 and v2["vol"] < v1["vol"],
          f"({v1['vol'] if v1 else '?'}→{v2['vol'] if v2 else '?'})")

    # Portapapeles (y): copia o falla con mensaje claro, sin panic
    ui.send("y")
    ui.pump(1.0)
    copied = re.search(r"copiado al portapapeles|portapapeles no disponible",
                       ui.text())
    check("y copia al portapapeles (o avisa si no hay)", bool(copied),
          f"({copied.group(0) if copied else 'sin mensaje'})")

    # Stop
    ui.send("s")
    check("s detiene la reproducción", ui.wait_for(r"Sin reproducción", 3))

    # ── 6. Búsqueda sin resultados ──────────────────────────────────────
    ui.send("/")
    ui.pump(0.3)
    ui.send("zzqxwv plg unlikelyquery 987654321")
    ui.pump(0.3)
    ui.send("\r")
    no_res = ui.wait_for(r"sin resultados", 30)
    check("búsqueda vacía muestra mensaje", no_res)
    check("la app sigue viva tras búsqueda vacía", "Buscar en" in ui.text())

    # ── 7. URL pegada en la búsqueda ────────────────────────────────────
    ui.send("/")
    ui.pump(0.3)
    ui.send("\x7f" * 45)  # limpiar la caja (solo hay Backspace)
    ui.pump(0.5)
    ui.send("https://www.youtube.com/watch?v=LdpMpfp-J_I")
    ui.pump(0.3)
    ui.send("\r")
    url_ok = ui.wait_for(r"Resultados \(1\)", 30)
    check("URL pegada se resuelve directamente", url_ok)

    # ── 7b. Bandcamp: ciclar con m, texto rechazado, URL de álbum ──────
    ui.send("\x1b")  # foco a resultados: 'm' cicla proveedor
    ui.pump(0.3)
    ui.send("m")     # YouTube → SoundCloud
    ui.pump(0.3)
    ui.send("m")     # SoundCloud → Bandcamp
    ui.pump(0.5)
    check("m cicla hasta Bandcamp", ui.wait_for(r"Buscar en Bandcamp", 3))

    ui.send("/")
    ui.pump(0.3)
    ui.send("\x7f" * 50)
    ui.pump(0.4)
    ui.send("daft punk")
    ui.pump(0.3)
    ui.send("\r")
    check(
        "texto en Bandcamp indica que se use URL",
        ui.wait_for(r"Bandcamp no admite búsqueda de texto", 15),
    )

    ui.send("/")
    ui.pump(0.3)
    ui.send("\x7f" * 20)
    ui.pump(0.4)
    ui.send("https://sufjanstevens.bandcamp.com/album/carrie-lowell")
    ui.pump(0.3)
    ui.send("\r")
    # "Resultados (0)" del intento anterior también casa \d+: esperar a que
    # la resolución del álbum devuelva varias pistas.
    n_album = 0
    deadline = time.time() + 45
    while time.time() < deadline:
        ui.pump(0.5)
        m = re.search(r"Resultados \((\d+)\)", ui.text())
        if m and int(m.group(1)) >= 2:
            n_album = int(m.group(1))
            break
    check("URL de álbum de Bandcamp se resuelve (varias pistas)",
          n_album >= 2, f"({n_album} pistas)")

    # ── 8. La app queda viva tras los errores anteriores ────────────────
    check("la app sigue viva tras errores de búsqueda", "Buscar en" in ui.text())

    # ── 8b. Fase 20: play-next, cola→playlist e historial ───────────────
    # Enter reproduce la pista seleccionada del álbum de Bandcamp: entra en
    # la cola (había 3 pistas de daft punk) en la posición 4.
    ui.send("\r")
    playing20 = ui.wait_for(r"▶ .*cola 4/4", 45)
    check("fase 20: reproducción del álbum", playing20)
    ui.send("\x1b[B")  # selección a la segunda pista del álbum
    ui.pump(0.3)
    ui.send("e")       # encolar a continuación
    ui.pump(0.8)
    check("e encola a continuación", "a continuación:" in ui.text())
    # La pista se inserta en current+1: el panel muestra «Cola (4/5)».
    check("pista insertada tras la actual", "Cola (4/5)" in ui.text())

    # w: guardar la cola como playlist (acepta el nombre sugerido).
    ui.send("\t")      # foco Cola
    ui.pump(0.3)
    ui.send("w")
    ui.pump(0.5)
    check("w pide nombre para la cola",
          "Guardar cola (5 pistas) como playlist" in ui.text())
    ui.send("\r")
    ui.pump(0.8)
    check("cola guardada como playlist", "cola guardada como playlist" in ui.text())
    ui.send("P")
    ui.pump(0.6)
    check("playlist nueva visible en el gestor",
          re.search(r"cola \d{4}-\d{2}-\d{2} \(5\)", ui.text()) is not None)
    ui.send("\x1b")    # cerrar gestor
    ui.pump(0.3)

    # t: el overlay de estadísticas cicla a la vista historial con fecha.
    ui.send("t")
    ui.pump(0.5)
    ui.send("\t")      # artistas
    ui.pump(0.3)
    ui.send("\t")      # historial
    ui.pump(0.5)
    hist = ui.text()
    check("vista historial de estadísticas",
          "Tus más escuchadas (historial)" in hist
          and re.search(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}", hist) is not None)
    ui.send("\x1b")
    ui.pump(0.3)

    # ── Salir ───────────────────────────────────────────────────────────
    ui.send("q")
    ui.pump(1.0)

    # ── 9. Audio: revisar el log de mpv ─────────────────────────────────
    logtxt = open(ui.log_path, "rb").read().decode("utf-8", "replace")
    audio_fail = "Audio device open failed" in logtxt or \
        "Failed to initialize audio" in logtxt
    check("mpv abrió el audio sin errores", not audio_fail)
    ui.close()


def run_dumb_startup(binary):
    """Arranque en terminal 'mudo' (sin respuesta a la query gráfica)."""
    ui = Ui(make_env(), binary, answer=False)
    t0 = time.time()
    found = ui.wait_for(r"Buscar en YouTube \(Enter: buscar", 15)
    elapsed = time.time() - t0
    ui.send("q")
    ui.pump(0.5)
    ui.close()
    check("terminal mudo arranca <2s", found and elapsed < 2.0,
          f"({elapsed:.2f}s)")


def run_cli_flags(binary):
    """Abreviaturas -t/--theme: arranca con un tema y rechaza uno inexistente."""
    ui = Ui(make_env(), binary, answer=False, args=["-t", "dracula"])
    t0 = time.time()
    found = ui.wait_for(r"Buscar en YouTube \(Enter: buscar", 15)
    elapsed = time.time() - t0
    check("-t <tema> arranca y pinta la UI", found and elapsed < 5.0,
          f"({elapsed:.2f}s)")
    ui.send("q")
    ui.pump(0.5)
    ui.close()

    # pty real (pexpect) para pasar el chequeo de TTY: el error llega a stderr
    # con la lista de temas y el proceso sale con código 2. close() cosecha el
    # proceso: sin él, exitstatus puede no estar relleno tras el EOF.
    child = pexpect.spawn(binary, ["-t", "temaimposiblexyz"],
                          env=make_env(), encoding="utf-8", timeout=15,
                          cwd=str(REPO_ROOT))
    child.expect(pexpect.EOF)
    out = child.before
    child.close()
    ok = ("Tema desconocido" in out and "dracula" in out
          and child.exitstatus == 2)
    check("-t <tema inexistente> falla con lista y exit 2", ok,
          f"(exit {child.exitstatus})")


def run_fase20_gapless(binary):
    """Gapless: arranque y ciclo de reproducción con [audio] gapless = true."""
    env = make_env()
    cfg_dir = os.path.join(env["XDG_CONFIG_HOME"], "shellmusic")
    os.makedirs(cfg_dir, exist_ok=True)
    # El config mínimo: la app crea el resto con valores por defecto.
    with open(os.path.join(cfg_dir, "config.toml"), "w") as f:
        f.write("[audio]\ngapless = true\n")

    ui = Ui(env, binary)
    found = ui.wait_for(r"Buscar en YouTube \(Enter: buscar", 15)
    check("gapless: arranque con gapless=true", found)
    ui.send("daft punk")
    ui.pump(0.3)
    ui.send("\r")
    got = ui.wait_for(r"Resultados \(\d\d+\)", 30)
    check("gapless: búsqueda funciona", got)
    if got:
        # Encolar dos y reproducir la primera: al arrancar, gapless apila la
        # segunda con loadfile append (ruta real del flag).
        ui.send("a")
        ui.pump(0.3)
        ui.send("\x1b[B")
        ui.pump(0.3)
        ui.send("a")
        ui.pump(0.5)
        ui.send("g")
        ui.pump(0.3)
        ui.send("\r")
        playing = ui.wait_for(r"▶ .*cola 1/2", 45)
        check("gapless: reproducción con siguiente apilada", playing)
        if playing:
            ui.pump(1.0)
            check("gapless: append sin error visible",
                  "no se pudo apilar" not in ui.text())
    ui.send("q")
    ui.pump(0.5)
    ui.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--binary", default=str(REPO_ROOT / "target/release/smusic"),
                        help="ruta del binario smusic (default: target/release/smusic)")
    parser.add_argument("--dumb-startup", action="store_true",
                        help="añade la medición de arranque sin respuesta gráfica")
    args = parser.parse_args()
    binary = str(Path(args.binary).resolve())
    if not Path(binary).exists():
        print(f"no existe el binario: {binary}", file=sys.stderr)
        print("compila antes: cargo build --release", file=sys.stderr)
        sys.exit(2)

    print(f"Binario: {binary}")
    run_suite(binary)
    run_cli_flags(binary)
    run_fase20_gapless(binary)
    if args.dumb_startup:
        run_dumb_startup(binary)

    print()
    failed = [r for r in results if not r[1]]
    print(f"TOTAL: {len(results) - len(failed)}/{len(results)} OK")
    if failed:
        for name, _, detail in failed:
            print(f"  FALLA: {name} {detail}")
        sys.exit(1)


if __name__ == "__main__":
    main()
