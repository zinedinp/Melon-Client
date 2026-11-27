#!/usr/bin/env python3
"""Draws the program's icon: the cut face of the website's melon cube
(`.cube .f` in website/melon-cube.css), flat and head on. Writes
crates/melon-base/src/icon.png (the window's icon, embedded in the binary) and
packaging/melon*.png (the desktop entry's). Needs PIL.
Run from the repository root."""
from PIL import Image, ImageDraw

U = 280  # the face's size in the stylesheet
K = 4    # supersampling
S = U * K


def rgb(hexcode):
    return tuple((hexcode >> s) & 255 for s in (16, 8, 0))


img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)


def box(a, b, color):
    d.rectangle([a * K, a * K, b * K - 1, b * K - 1], fill=color)


box(0, U, rgb(0x0a4a1c))        # outline (the 4px border)
box(4, U - 4, rgb(0x27b358))    # rind
box(12, U - 12, rgb(0xe9ffd0))  # pale ring
a, b, bands = 24, U - 24, 48    # the flesh, light to deep red
top, bot = rgb(0xff7a90), rgb(0xff2a4a)
for i in range(bands):
    t = (i + 0.5) / bands
    color = tuple(round(top[k] + (bot[k] - top[k]) * t) for k in range(3))
    y0 = a + (b - a) * i / bands
    y1 = a + (b - a) * (i + 1) / bands
    d.rectangle([a * K, round(y0 * K), b * K - 1, round(y1 * K) - 1], fill=color)
for x, y in [(72, 80), (160, 72), (112, 136), (192, 152), (64, 184), (144, 200)]:
    d.rectangle([x * K, y * K, (x + 16) * K - 1, (y + 16) * K - 1], fill=rgb(0x111111))

icon = img.resize((256, 256), Image.LANCZOS)
icon.save("crates/melon-base/src/icon.png")
icon.save("packaging/melon.png")
for size in (48, 128):  # the sizes a taskbar or dock asks for
    icon.resize((size, size), Image.LANCZOS).save(f"packaging/melon-{size}.png")
print("crates/melon-base/src/icon.png, packaging/melon*.png")
