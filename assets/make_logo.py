"""Draws the 3 Bugs Studio logo: three bugs on the faces of an isometric CAD cube.

Writes logo.svg (the mark), logo-wordmark.svg (mark + name, for the README), icon-<size>.png
(icon-256.png is the app's window icon; the rest are for the Linux desktop entry), icon.ico (the
Windows exe and installer) and icon.icns (the macOS bundle) from the same geometry.
Needs Pillow: `uv run --no-project --with pillow python assets/make_logo.py`.
"""
import math
import pathlib

from PIL import Image, ImageDraw

OUT = pathlib.Path(__file__).parent
NAVY, INK = "#0f1b2d", "#0a1220"
FACES = {"top": "#6cc4ff", "left": "#2f86e0", "right": "#1b5aa8"}
GOLD, AMBER = "#ffb703", "#fb8500"

# Cube corners on a 256 x 256 canvas; each face is centre + u*U + v*V for u, v in [-1, 1].
FACE_FRAMES = {
    "top": ((128, 96), (31, 18), (-31, 18)),
    "left": ((97, 150), (31, 18), (0, 36)),
    "right": ((159, 150), (31, -18), (0, 36)),
}


def ellipse(cx, cy, rx, ry, n=40):
    return [(cx + rx * math.cos(2 * math.pi * i / n), cy + ry * math.sin(2 * math.pi * i / n)) for i in range(n)]


def bug():
    """A ladybird-like bug in face coordinates: (polygons, lines) as (points, colour[, width])."""
    polys = [
        (ellipse(0, 0.12, 0.36, 0.46), GOLD),
        (ellipse(0, -0.42, 0.19, 0.16), AMBER),
    ]
    for sx in (-1, 1):
        polys += [
            (ellipse(sx * 0.17, 0.02, 0.075, 0.075, 16), NAVY),
            (ellipse(sx * 0.16, 0.32, 0.06, 0.06, 16), NAVY),
        ]
    lines = [([(0, -0.3), (0, 0.58)], NAVY, 2.2)]
    for sx in (-1, 1):
        lines += [
            ([(sx * 0.3, -0.12), (sx * 0.62, -0.3)], GOLD, 3.0),
            ([(sx * 0.36, 0.12), (sx * 0.68, 0.14)], GOLD, 3.0),
            ([(sx * 0.3, 0.36), (sx * 0.6, 0.6)], GOLD, 3.0),
            ([(sx * 0.08, -0.55), (sx * 0.26, -0.82)], AMBER, 2.4),
        ]
    return polys, lines


def on_face(points, face, scale=0.9):
    (cx, cy), (ux, uy), (vx, vy) = FACE_FRAMES[face]
    return [(cx + scale * (x * ux + y * vx), cy + scale * (x * uy + y * vy)) for x, y in points]


def face_outline(face):
    return on_face([(-1, -1), (1, -1), (1, 1), (-1, 1)], face, 1.0)


def shapes():
    """Everything to draw, back to front: ("poly", points, fill) or ("line", points, colour, width)."""
    out = []
    for face, colour in FACES.items():
        out.append(("poly", face_outline(face), colour))
    for face in FACES:
        out.append(("line", face_outline(face) + face_outline(face)[:1], "#e8f4ff", 2.0))
        polys, lines = bug()
        out += [("poly", on_face(p, face), c) for p, c in polys]
        out += [("line", on_face(p, face), c, w) for p, c, w in lines]
    # A dimension line under the cube: it is CAD, after all.
    out.append(("line", [(70, 222), (186, 222)], GOLD, 2.0))
    out.append(("poly", [(70, 222), (79, 217), (79, 227)], GOLD))
    out.append(("poly", [(186, 222), (177, 217), (177, 227)], GOLD))
    out.append(("line", [(66, 172), (66, 228)], "#5b6b80", 1.4))
    out.append(("line", [(190, 172), (190, 228)], "#5b6b80", 1.4))
    return out


def svg_body():
    parts = ['<rect width="256" height="256" rx="52" fill="%s"/>' % NAVY]
    for s in shapes():
        pts = " ".join("%.2f,%.2f" % p for p in s[1])
        if s[0] == "poly":
            parts.append('<polygon points="%s" fill="%s"/>' % (pts, s[2]))
        else:
            parts.append(
                '<polyline points="%s" fill="none" stroke="%s" stroke-width="%s" '
                'stroke-linecap="round" stroke-linejoin="round"/>' % (pts, s[2], s[3])
            )
    return "\n  ".join(parts)


def write_svgs():
    mark = svg_body()
    (OUT / "logo.svg").write_text(
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">\n'
        "  <title>3 Bugs Studio</title>\n  %s\n</svg>\n" % mark,
        encoding="utf-8",
    )
    (OUT / "logo-wordmark.svg").write_text(
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 760 256" width="760" height="256">\n'
        "  <title>3BCAD by 3 Bugs Studio</title>\n  %s\n"
        '  <text x="292" y="132" font-family="Segoe UI, Helvetica, Arial, sans-serif" font-size="92" '
        'font-weight="700" fill="%s">3BCAD</text>\n'
        '  <text x="296" y="186" font-family="Segoe UI, Helvetica, Arial, sans-serif" font-size="34" '
        'fill="#5b6b80">by 3 Bugs Studio</text>\n</svg>\n' % (mark, "#1b5aa8"),
        encoding="utf-8",
    )


def render(px=1024):
    k = px / 256
    img = Image.new("RGBA", (px, px), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([0, 0, px - 1, px - 1], radius=52 * k, fill=NAVY)
    for s in shapes():
        pts = [(x * k, y * k) for x, y in s[1]]
        if s[0] == "poly":
            d.polygon(pts, fill=s[2])
        else:
            w = max(1, round(s[3] * k))
            d.line(pts, fill=s[2], width=w, joint="curve")
            for x, y in (pts[0], pts[-1]):
                d.ellipse([x - w / 2, y - w / 2, x + w / 2, y + w / 2], fill=s[2])
    return img


def write_icons():
    big = render()
    for size in (16, 32, 48, 64, 128, 256, 512):
        big.resize((size, size), Image.LANCZOS).save(OUT / ("icon-%d.png" % size))
    big.save(OUT / "icon.ico", sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)])
    big.save(OUT / "icon.icns")


if __name__ == "__main__":
    write_svgs()
    write_icons()
