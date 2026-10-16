#!/usr/bin/env python3
"""Record a ShellMusic demo session and produce docs/demo.gif."""

import os
import sys
import time
import shutil
import subprocess
import tempfile

import pexpect
import pyte
from PIL import Image, ImageDraw, ImageFont

# ── Config ──────────────────────────────────────────────────────────────
COLS = 110
ROWS = 32
FONT_PATH = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
FONT_SIZE = 16
FPS = 5
FRAME_DURATION_MS = 1000 // FPS
MAX_GIF_SIZE = 5 * 1024 * 1024  # 5 MB

# Dark theme matching the screenshot
COLORS = {
    "black":       (30, 30, 46),
    "red":         (243, 139, 168),
    "green":       (166, 227, 161),
    "yellow":      (249, 226, 175),
    "blue":        (137, 180, 250),
    "magenta":     (203, 166, 247),
    "cyan":        (137, 220, 235),
    "white":       (205, 214, 244),
    "default":     (205, 214, 244),
    "darkgray":    (108, 112, 134),
    "brightblack": (88, 91, 112),
}
BG_COLOR = (24, 24, 37)
BOLD_BRIGHTNESS = 1.25


def resolve_color(name, bold=False):
    if isinstance(name, int):
        name = _xterm256(name)
    rgb = COLORS.get(str(name).lower(), COLORS["default"])
    if bold:
        rgb = tuple(min(255, int(c * BOLD_BRIGHTNESS)) for c in rgb)
    return rgb


def _xterm256(n):
    """Convert xterm 256-color index to a color name approximation."""
    basic = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"]
    if n < 8:
        return basic[n]
    if n < 16:
        return basic[n - 8]
    return "white"


class TermRenderer:
    def __init__(self):
        self.font = ImageFont.truetype(FONT_PATH, FONT_SIZE)
        self.font_bold = ImageFont.truetype(
            FONT_PATH.replace(".ttf", "-Bold.ttf"), FONT_SIZE
        )
        bbox = self.font.getbbox("M")
        self.cw = bbox[2] - bbox[0] + 1
        self.ch = bbox[3] - bbox[1] + 4

    def render(self, screen):
        w = COLS * self.cw
        h = ROWS * self.ch
        img = Image.new("RGB", (w, h), BG_COLOR)
        draw = ImageDraw.Draw(img)

        for y in range(min(screen.lines, ROWS)):
            for x in range(min(screen.columns, COLS)):
                ch = screen.buffer[y][x]
                if not ch.data or ch.data == " ":
                    continue

                fg_name = str(ch.fg) if ch.fg != "default" else "default"
                bg_name = str(ch.bg) if ch.bg != "default" else None
                fg_rgb = resolve_color(fg_name, bold=ch.bold)

                if ch.reverse:
                    fg_rgb, bg_rgb_swap = resolve_color(bg_name) if bg_name else BG_COLOR, fg_rgb
                    bg_rgb = fg_rgb
                    fg_rgb = bg_rgb_swap
                elif bg_name:
                    bg_rgb = resolve_color(bg_name)
                else:
                    bg_rgb = None

                px = x * self.cw
                py = y * self.ch

                if bg_rgb and bg_rgb != BG_COLOR:
                    draw.rectangle([px, py, px + self.cw - 1, py + self.ch - 1], fill=bg_rgb)

                font = self.font_bold if ch.bold else self.font
                draw.text((px, py), ch.data, fill=fg_rgb, font=font)

        # Cursor
        cx, cy = screen.cursor.x, screen.cursor.y
        if cx < COLS and cy < ROWS:
            px, py = cx * self.cw, cy * self.ch
            ch = screen.buffer[cy][cx]
            cursor_char = ch.data if ch.data and ch.data != " " else " "
            draw.rectangle([px, py, px + self.cw - 1, py + self.ch - 1], fill=(180, 190, 254))
            draw.text((px, py), cursor_char, fill=BG_COLOR, font=self.font)

        return img


def capture_frame(stream, screen, renderer, frames):
    screen.dirty.clear()
    frames.append(renderer.render(screen))


def feed_and_capture(child, stream, screen, renderer, frames, wait=0.3):
    """Read available output and capture frames."""
    end = time.time() + wait
    while time.time() < end:
        try:
            data = child.read_nonblocking(size=4096, timeout=0.05)
            if data:
                stream.feed(data)
        except pexpect.TIMEOUT:
            pass
        except pexpect.EOF:
            break
    capture_frame(stream, screen, renderer, frames)


def send_keys(child, stream, screen, renderer, frames, keys, delay=0.15, settle=0.4):
    """Send keystrokes and capture frames."""
    child.send(keys)
    feed_and_capture(child, stream, screen, renderer, frames, settle)


def type_text(child, stream, screen, renderer, frames, text, char_delay=0.08):
    """Type text character by character."""
    for c in text:
        child.send(c)
        feed_and_capture(child, stream, screen, renderer, frames, char_delay)


def hold(stream, screen, renderer, frames, duration=1.0):
    """Capture repeated frames to simulate a pause."""
    n = max(1, int(duration * FPS))
    for _ in range(n):
        capture_frame(stream, screen, renderer, frames)


def record_demo():
    # Save existing config/data
    dirs = {
        "config": os.path.expanduser("~/.config/shellmusic"),
        "data": os.path.expanduser("~/.local/share/shellmusic"),
    }
    backup_dir = tempfile.mkdtemp(prefix="smusic_backup_")

    for tag, d in dirs.items():
        if os.path.exists(d):
            shutil.copytree(d, os.path.join(backup_dir, tag))

    try:
        # Clear data for fresh demo
        for d in dirs.values():
            if os.path.exists(d):
                shutil.rmtree(d)

        return _run_demo()
    finally:
        # Restore
        for tag, d in dirs.items():
            if os.path.exists(d):
                shutil.rmtree(d)
            bak = os.path.join(backup_dir, tag)
            if os.path.exists(bak):
                shutil.copytree(bak, d)
        shutil.rmtree(backup_dir, ignore_errors=True)


def _run_demo():
    screen = pyte.Screen(COLS, ROWS)
    stream = pyte.ByteStream(screen)
    renderer = TermRenderer()
    frames = []

    env = os.environ.copy()
    env["TERM"] = "xterm-256color"
    env["LINES"] = str(ROWS)
    env["COLUMNS"] = str(COLS)

    child = pexpect.spawn(
        "./target/release/smusic",
        dimensions=(ROWS, COLS),
        env=env,
        encoding=None,
        timeout=10,
    )

    print("Grabando demo...")

    # 1. Startup — wait for the app to render
    feed_and_capture(child, stream, screen, renderer, frames, 2.0)
    hold(stream, screen, renderer, frames, 1.0)

    # 2. Type search query
    type_text(child, stream, screen, renderer, frames, "daft punk", char_delay=0.15)
    hold(stream, screen, renderer, frames, 0.5)

    # 3. Press Enter to search
    send_keys(child, stream, screen, renderer, frames, "\r", settle=0.5)
    # Wait for results (network)
    feed_and_capture(child, stream, screen, renderer, frames, 5.0)
    hold(stream, screen, renderer, frames, 2.0)

    # 4. Navigate down through results
    for _ in range(2):
        send_keys(child, stream, screen, renderer, frames, "\x1b[B", settle=0.4)
    hold(stream, screen, renderer, frames, 0.5)

    # 5. Add to queue
    send_keys(child, stream, screen, renderer, frames, "a", settle=0.8)
    hold(stream, screen, renderer, frames, 0.5)

    # 6. Navigate down, add another, favorite it
    send_keys(child, stream, screen, renderer, frames, "\x1b[B", settle=0.4)
    send_keys(child, stream, screen, renderer, frames, "a", settle=0.8)
    hold(stream, screen, renderer, frames, 0.3)
    send_keys(child, stream, screen, renderer, frames, "f", settle=0.8)
    hold(stream, screen, renderer, frames, 0.5)

    # 7. Switch to queue panel (Tab) — show queue content
    send_keys(child, stream, screen, renderer, frames, "\t", settle=0.5)
    hold(stream, screen, renderer, frames, 1.5)

    # 8. Switch to favorites panel (Tab) — show favorites
    send_keys(child, stream, screen, renderer, frames, "\t", settle=0.5)
    hold(stream, screen, renderer, frames, 1.5)

    # 9. Switch back to results (Tab)
    send_keys(child, stream, screen, renderer, frames, "\t", settle=0.5)
    hold(stream, screen, renderer, frames, 0.5)

    # 10. Try to play (Enter) — shows player bar even if mpv fails
    send_keys(child, stream, screen, renderer, frames, "\r", settle=1.5)
    feed_and_capture(child, stream, screen, renderer, frames, 3.0)
    hold(stream, screen, renderer, frames, 2.0)

    # 11. Volume up a couple times
    send_keys(child, stream, screen, renderer, frames, "+", settle=0.3)
    send_keys(child, stream, screen, renderer, frames, "+", settle=0.3)
    send_keys(child, stream, screen, renderer, frames, "+", settle=0.3)
    hold(stream, screen, renderer, frames, 0.8)

    # 12. Open playlists overlay
    send_keys(child, stream, screen, renderer, frames, "P", settle=0.8)
    hold(stream, screen, renderer, frames, 1.5)

    # 13. Close overlay
    send_keys(child, stream, screen, renderer, frames, "\x1b", settle=0.5)
    hold(stream, screen, renderer, frames, 0.8)

    # 14. Quit
    send_keys(child, stream, screen, renderer, frames, "q", settle=0.5)
    feed_and_capture(child, stream, screen, renderer, frames, 1.0)

    try:
        child.close()
    except Exception:
        pass

    return frames


def save_gif(frames, output_path):
    """Save frames as optimized GIF, reducing size if needed."""
    # Deduplicate consecutive identical frames
    deduped = [frames[0]]
    durations = [FRAME_DURATION_MS]
    for f in frames[1:]:
        if f.tobytes() == deduped[-1].tobytes():
            durations[-1] += FRAME_DURATION_MS
        else:
            deduped.append(f)
            durations.append(FRAME_DURATION_MS)

    print(f"  {len(frames)} frames -> {len(deduped)} unique frames")

    # Try saving with different optimization levels
    for colors in [256, 128, 64]:
        deduped[0].save(
            output_path,
            save_all=True,
            append_images=deduped[1:],
            duration=durations,
            loop=0,
            optimize=True,
        )
        size = os.path.getsize(output_path)
        print(f"  GIF con {colors} colores: {size / 1024 / 1024:.1f} MB")
        if size <= MAX_GIF_SIZE:
            break

    return size


def main():
    output = "docs/demo.gif"
    frames = record_demo()
    if not frames:
        print("ERROR: no frames captured", file=sys.stderr)
        sys.exit(1)

    size = save_gif(frames, output)
    print(f"\nGIF guardado: {output} ({size / 1024 / 1024:.1f} MB)")

    if size > MAX_GIF_SIZE:
        print(f"ADVERTENCIA: el GIF supera {MAX_GIF_SIZE // 1024 // 1024} MB", file=sys.stderr)


if __name__ == "__main__":
    main()
