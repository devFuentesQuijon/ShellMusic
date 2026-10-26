#!/usr/bin/env python3
"""Genera el logo de ShellMusic (docs/logo.png + docs/logo-icon.png)."""

from PIL import Image, ImageDraw, ImageFont

SS = 4  # supersampling

# Paleta (estética Catppuccin del propio proyecto)
BG_TILE = (24, 24, 37, 255)        # #181825
BORDER = (49, 50, 68, 255)         # #313244
DOTS = [(243, 139, 168), (249, 226, 175), (166, 227, 161)]  # rosa, ámbar, verde
GRAD_START = (203, 166, 247)       # #cba6f7 malva
GRAD_MID = (137, 180, 250)         # #89b4fa azul
GRAD_END = (148, 226, 213)         # #94e2d5 turquesa
TEXT = (205, 214, 244)             # #cdd6f4 texto

FONT = "/usr/share/fonts/truetype/noto/NotoSans-ExtraBold.ttf"
WORDMARK = "ShellMusic"


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def gradient(w, h):
    """Degradado horizontal malva → azul → turquesa."""
    img = Image.new("RGB", (w, h))
    px = img.load()
    for x in range(w):
        t = x / max(w - 1, 1)
        if t < 0.5:
            c = lerp(GRAD_START, GRAD_MID, t * 2)
        else:
            c = lerp(GRAD_MID, GRAD_END, (t - 0.5) * 2)
        for y in range(h):
            px[x, y] = c
    return img


def rounded_bar(draw, x0, y0, x1, y1, r, fill):
    draw.rounded_rectangle([x0, y0, x1, y1], radius=r, fill=fill)


def draw_icon(size):
    """Tile terminal con puntos y ecualizador. Devuelve RGBA."""
    s = size * SS
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    m = int(s * 0.045)  # margen para la sombra sutil
    d.rounded_rectangle([m, m, s - m, s - m], radius=int(s * 0.22),
                        fill=BG_TILE, outline=BORDER, width=SS * 2)

    # Puntos de la barra de título
    dot_r = int(s * 0.030)
    dot_y = int(s * 0.135)
    x = int(s * 0.135)
    for c in DOTS:
        d.ellipse([x - dot_r, dot_y - dot_r, x + dot_r, dot_y + dot_r],
                  fill=c + (255,))
        x += int(dot_r * 2.9)

    # Ecualizador: barras redondeadas con degradado (por máscara)
    bars = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    bd = ImageDraw.Draw(bars)
    n = 5
    bw = int(s * 0.095)
    gap = int(s * 0.052)
    heights = [0.42, 0.72, 0.55, 0.82, 0.62]
    total = n * bw + (n - 1) * gap
    x0 = (s - total) // 2
    y_base = int(s * 0.78)
    top_min = int(s * 0.30)
    grad = gradient(s, s).convert("RGBA")
    for i, hf in enumerate(heights):
        h = int((y_base - top_min) * hf)
        bx0 = x0 + i * (bw + gap)
        rounded_bar(bd, bx0, y_base - h, bx0 + bw, y_base, bw // 2,
                    (255, 255, 255, 255))
    mask = bars.split()[3]
    img.paste(grad, (0, 0), mask)

    return img.resize((size, size), Image.LANCZOS)


def draw_logo():
    """Banner: icono + wordmark. Devuelve RGBA."""
    icon_size = 300
    pad = 48
    font_size = 168

    icon = draw_icon(icon_size)
    font = ImageFont.truetype(FONT, font_size * SS)

    # Medir wordmark
    probe = ImageDraw.Draw(Image.new("RGBA", (10, 10)))
    bbox = probe.textbbox((0, 0), WORDMARK, font=font)
    tw, th = bbox[2] - bbox[0], bbox[3] - bbox[1]

    W = (pad + icon_size + 40 + tw // SS + pad)
    H = pad * 2 + icon_size
    img = Image.new("RGBA", (W * SS, H * SS), (0, 0, 0, 0))

    icon_big = icon.resize((icon_size * SS, icon_size * SS), Image.LANCZOS)
    img.alpha_composite(icon_big, (pad * SS, pad * SS))

    # Wordmark con degradado a través de máscara
    tx = (pad + icon_size + 40) * SS
    ty = (H * SS - th) // 2 - bbox[1]
    mask = Image.new("L", (tw, th), 0)
    md = ImageDraw.Draw(mask)
    md.text((-bbox[0], -bbox[1]), WORDMARK, font=font, fill=255)
    grad = gradient(tw, th).convert("RGBA")
    img.paste(grad, (tx, ty), mask)

    return img.resize((W, H), Image.LANCZOS)


if __name__ == "__main__":
    logo = draw_logo()
    logo.save("docs/logo.png")
    print(f"docs/logo.png: {logo.size[0]}x{logo.size[1]}")
    icon = draw_icon(512)
    icon.save("docs/logo-icon.png")
    print(f"docs/logo-icon.png: 512x512")
