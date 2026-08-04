#!/usr/bin/env python3
"""Regenerate the autostand logo suite in `brand/logo/`.

The wordmark is emitted as <path> outlines, never as an SVG <text> element: a
<text> lockup reshapes itself on every machine that lacks Inter, so the logo
would not be a fixed asset. Outlines are extracted from the Inter variable font
with fontTools at wght=700 / opsz=14 — the same instance the UI renders, so the
lockup and the app's headings share a shape.

The mark is generated too, so the wordmark and the mark can never drift apart:
it is one closed outline built from the canonical lightning polygon rotated onto
a checkmark's long arm. Bolt = automation, check = completion, rounded tile =
the standup card.

Usage:
    python3 tests/make-wordmark.py                  # writes brand/logo/*.svg
    python3 tests/make-wordmark.py --font Inter.ttf # use a local Inter instead
    python3 tests/make-wordmark.py --out /tmp/logo  # write somewhere else

Requires: fontTools (>=4.40). Network access on first run only (the font is
cached in the system temp dir).
"""

from __future__ import annotations

import argparse
import math
import os
import tempfile
import urllib.request
from pathlib import Path

from fontTools.misc.transform import Transform
from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

# --------------------------------------------------------------------------- #
# Brand constants
# --------------------------------------------------------------------------- #

# Literal hex, not var(--brand-primary): CSS custom properties do not resolve in
# an SVG loaded through <img>, as a favicon or as an app icon, which is where
# these files are used. The mono variant is the themeable one.
BRAND_PRIMARY = "#2563eb"  # --brand-primary / --color-blue-600
SURFACE = "#ffffff"  # --bg-surface (light)
FOREGROUND = "#0f172a"  # --fg-base / --color-slate-900

WORD = "autostand"
WORDMARK_WEIGHT = 700  # docs/design-system/02-brand.md: Inter 700 for the wordmark
WORDMARK_OPSZ = 14  # Inter's default optical size — matches the static UI weights
TRACKING_EM = -0.012  # display-size text needs slightly tighter tracking than body

# Pinned to a commit, not to `main`: a font update upstream would silently
# reshape the wordmark on the next regeneration. Inter 4.001 (git-66647c0bb).
INTER_COMMIT = "e1d6480102fed30739fead0faee463101f892c8f"
INTER_URL = (
    f"https://raw.githubusercontent.com/google/fonts/{INTER_COMMIT}"
    "/ofl/inter/Inter%5Bopsz,wght%5D.ttf"
)

# --------------------------------------------------------------------------- #
# The mark
# --------------------------------------------------------------------------- #

# Canonical lightning polygon in its own frame: bottom tip at the origin, axis
# pointing straight up, y growing upward. Traversal is
# tip -> left flag -> left notch -> bottom tip -> right flag -> right notch.
BOLT = {
    "tip": (0.0, 20.1),
    "left_flag": (-8.76, 7.16),
    "left_notch": (0.20, 8.06),
    "bottom": (0.0, 0.0),
    "right_flag": (8.76, 12.94),
    "right_notch": (-0.20, 12.04),
}
BOLT_LEN = 20.1

# Tuned by rendering the mark at 16/20/24/32/128 px against light and dark.
MARK = {
    "bolt_len": 76.0,  # bolt axis length, before the fit-to-tile scale
    "bolt_width": 0.56,  # narrows the canonical bolt so it reads as a check arm
    "bolt_tilt": 34.0,  # degrees clockwise from vertical
    "arm_angle": 52.0,  # short arm, degrees above horizontal
    "arm_len": 40.0,
    "arm_width": 17.0,
    "fill": 0.61,  # glyph size as a fraction of the tile
    "corner": 0.047,  # fillet radius as a fraction of the tile
    "radius": 0.22,  # tile corner radius as a fraction of the tile
}

# The favicon trades fidelity for survival at 16px: a bigger glyph, a tighter
# tile radius (a 22% radius reads as a circle at 16px) and softer fillets.
FAVICON = dict(MARK, fill=0.68, corner=0.030, radius=0.19)


def _to_screen(point: tuple[float, float], sx: float, sy: float, tilt: float) -> tuple[float, float]:
    """Bolt frame (y up) -> SVG frame (y down), axis tilted `tilt`° clockwise."""
    x, y = point[0] * sx, point[1] * sy
    c, s = math.cos(math.radians(tilt)), math.sin(math.radians(tilt))
    return (x * c + y * s, x * s - y * c)


def _intersect(p1, p2, p3, p4) -> tuple[float, float]:
    """Where line p1p2 crosses line p3p4."""
    (x1, y1), (x2, y2), (x3, y3), (x4, y4) = p1, p2, p3, p4
    den = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4)
    a = x1 * y2 - y1 * x2
    b = x3 * y4 - y3 * x4
    return ((a * (x3 - x4) - (x1 - x2) * b) / den, (a * (y3 - y4) - (y1 - y2) * b) / den)


def mark_outline(cfg: dict) -> list[tuple[float, float]]:
    """The bolt-check as ONE closed outline.

    The bolt's bottom tip doubles as the checkmark's vertex, so the two arms
    share a solid corner. Union-ing two overlapping shapes instead would leave a
    pinch at the joint that collapses at favicon sizes.
    """
    scale = cfg["bolt_len"] / BOLT_LEN
    sx, sy = scale * cfg["bolt_width"], scale
    pt = {k: _to_screen(v, sx, sy, cfg["bolt_tilt"]) for k, v in BOLT.items()}
    bottom = pt["bottom"]

    angle = math.radians(cfg["arm_angle"])
    towards_tip = (-math.cos(angle), -math.sin(angle))
    inward = (-towards_tip[1], towards_tip[0])  # perpendicular, points at the bolt
    width = cfg["arm_width"]

    tip_low = (
        bottom[0] + towards_tip[0] * cfg["arm_len"],
        bottom[1] + towards_tip[1] * cfg["arm_len"],
    )
    tip_high = (tip_low[0] + inward[0] * width, tip_low[1] + inward[1] * width)
    inner_edge = (
        (bottom[0] + inward[0] * width, bottom[1] + inward[1] * width),
        (tip_low[0] + inward[0] * width, tip_low[1] + inward[1] * width),
    )
    # The arm's inner edge stops where it meets the bolt's lower-left edge.
    joint = _intersect(*inner_edge, bottom, pt["left_notch"])

    return [
        tip_high,
        joint,
        pt["left_notch"],
        pt["left_flag"],
        pt["tip"],
        pt["right_notch"],
        pt["right_flag"],
        bottom,
        tip_low,
    ]


def fit(points, box: float, fraction: float):
    """Uniformly scale + centre `points` so their bbox fills `fraction` of `box`."""
    xs = [p[0] for p in points]
    ys = [p[1] for p in points]
    scale = (box * fraction) / max(max(xs) - min(xs), max(ys) - min(ys))
    cx, cy = (max(xs) + min(xs)) / 2, (max(ys) + min(ys)) / 2
    return [((x - cx) * scale + box / 2, (y - cy) * scale + box / 2) for x, y in points]


def rounded_polygon(points, radius: float, limit: float = 0.42) -> str:
    """Polygon with a quadratic fillet at every vertex.

    Sharp mitres shimmer when a 128-unit mark is rasterised at 16px; the fillets
    also echo the rounded tile. The radius is clamped per-vertex so short edges
    never overshoot into each other.
    """
    if radius <= 0:
        return "M" + " ".join(f"{x:.2f} {y:.2f}" for x, y in points) + "Z"

    corners = []
    n = len(points)
    for i in range(n):
        prev, here, nxt = points[(i - 1) % n], points[i], points[(i + 1) % n]
        dp = (prev[0] - here[0], prev[1] - here[1])
        dn = (nxt[0] - here[0], nxt[1] - here[1])
        lp, ln = math.hypot(*dp), math.hypot(*dn)
        t = min(radius, limit * lp, limit * ln)
        corners.append(
            (
                (here[0] + dp[0] / lp * t, here[1] + dp[1] / lp * t),
                here,
                (here[0] + dn[0] / ln * t, here[1] + dn[1] / ln * t),
            )
        )

    start = corners[0][0]
    d = f"M{start[0]:.2f} {start[1]:.2f}"
    for i, (_, here, after) in enumerate(corners):
        nxt = corners[(i + 1) % n][0]
        d += f" Q{here[0]:.2f} {here[1]:.2f} {after[0]:.2f} {after[1]:.2f}"
        d += f" L{nxt[0]:.2f} {nxt[1]:.2f}"
    return d + "Z"


def rounded_rect(x: float, y: float, w: float, h: float, r: float) -> str:
    return (
        f"M{x + r:.2f} {y:.2f} H{x + w - r:.2f} A{r:.2f} {r:.2f} 0 0 1 {x + w:.2f} {y + r:.2f} "
        f"V{y + h - r:.2f} A{r:.2f} {r:.2f} 0 0 1 {x + w - r:.2f} {y + h:.2f} "
        f"H{x + r:.2f} A{r:.2f} {r:.2f} 0 0 1 {x:.2f} {y + h - r:.2f} "
        f"V{y + r:.2f} A{r:.2f} {r:.2f} 0 0 1 {x + r:.2f} {y:.2f}Z"
    )


def mark_paths(box: float, cfg: dict) -> tuple[str, str]:
    """(tile path, glyph path) for a mark drawn in a `box`×`box` viewBox."""
    tile = rounded_rect(0, 0, box, box, box * cfg["radius"])
    glyph = rounded_polygon(fit(mark_outline(cfg), box, cfg["fill"]), box * cfg["corner"])
    return tile, glyph


# --------------------------------------------------------------------------- #
# The wordmark
# --------------------------------------------------------------------------- #


def load_inter(explicit: str | None) -> TTFont:
    """Inter, instantiated at the brand weight. Cached in the system temp dir."""
    if explicit:
        path = Path(explicit)
    else:
        path = Path(tempfile.gettempdir()) / "autostand-Inter-var.ttf"
        if not path.exists():
            print(f"downloading Inter -> {path}")
            urllib.request.urlretrieve(INTER_URL, path)  # noqa: S310 - pinned https URL
    font = TTFont(path)
    instancer.instantiateVariableFont(
        font, {"wght": WORDMARK_WEIGHT, "opsz": WORDMARK_OPSZ}, inplace=True
    )
    return font


def kerning(font: TTFont, wanted: set[tuple[str, str]]) -> dict[tuple[str, str], int]:
    """XAdvance for the given glyph pairs, read out of the GPOS `kern` feature.

    Applied by hand because there is no shaping engine here; without it "to" and
    "ta" in `autostand` sit a hair too loose.
    """
    found: dict[tuple[str, str], int] = {}
    if "GPOS" not in font:
        return found
    gpos = font["GPOS"].table
    indices: set[int] = set()
    for record in gpos.FeatureList.FeatureRecord:
        if record.FeatureTag == "kern":
            indices.update(record.Feature.LookupListIndex)

    for i in sorted(indices):
        lookup = gpos.LookupList.Lookup[i]
        subtables = list(lookup.SubTable)
        if lookup.LookupType == 9:  # Extension Positioning
            subtables = [st.ExtSubTable for st in subtables]
        for st in subtables:
            if not hasattr(st, "Coverage"):
                continue
            covered = set(st.Coverage.glyphs)
            if getattr(st, "Format", None) == 1 and hasattr(st, "PairSet"):
                for first, pair_set in zip(st.Coverage.glyphs, st.PairSet):
                    for record in pair_set.PairValueRecord:
                        pair = (first, record.SecondGlyph)
                        if pair in wanted:
                            found[pair] = found.get(pair, 0) + (
                                getattr(record.Value1, "XAdvance", 0) or 0
                            )
            elif getattr(st, "Format", None) == 2 and hasattr(st, "Class1Record"):
                classes1 = st.ClassDef1.classDefs if st.ClassDef1 else {}
                classes2 = st.ClassDef2.classDefs if st.ClassDef2 else {}
                for first, second in wanted:
                    if first not in covered:
                        continue
                    try:
                        record = st.Class1Record[classes1.get(first, 0)].Class2Record[
                            classes2.get(second, 0)
                        ]
                    except IndexError:
                        continue
                    value = getattr(record.Value1, "XAdvance", 0) or 0
                    if value:
                        found[(first, second)] = found.get((first, second), 0) + value
    return found


def _layout(font: TTFont, text: str) -> tuple[list[tuple[str, float]], tuple[float, float, float, float]]:
    """Glyph names with their pen origins, plus the ink bbox — all in font units."""
    names = [font.getBestCmap()[ord(ch)] for ch in text]
    widths = font["hmtx"]
    upem = font["head"].unitsPerEm
    pairs = {(names[i], names[i + 1]) for i in range(len(names) - 1)}
    kerns = kerning(font, pairs)
    tracking = TRACKING_EM * upem

    placed: list[tuple[str, float]] = []
    x = 0.0
    for i, name in enumerate(names):
        placed.append((name, x))
        x += widths[name][0]
        if i + 1 < len(names):
            x += kerns.get((name, names[i + 1]), 0) + tracking

    glyphs = font.getGlyphSet()
    box = [math.inf, math.inf, -math.inf, -math.inf]
    for name, offset in placed:
        pen = BoundsPen(glyphs)
        glyphs[name].draw(TransformPen(pen, Transform(1, 0, 0, 1, offset, 0)))
        if pen.bounds is None:  # a space, say — no ink
            continue
        box = [
            min(box[0], pen.bounds[0]),
            min(box[1], pen.bounds[1]),
            max(box[2], pen.bounds[2]),
            max(box[3], pen.bounds[3]),
        ]
    return placed, tuple(box)


def wordmark(font: TTFont, text: str, *, height: float = 0.0, width: float = 0.0, at=(0.0, 0.0)):
    """One <path> `d` for `text`, plus its rendered (width, height).

    Sized by ink bbox rather than by font metrics: side bearings and the
    ascender gap are dead space in a lockup.
    """
    placed, (x_min, y_min, x_max, y_max) = _layout(font, text)
    ink_w, ink_h = x_max - x_min, y_max - y_min
    scale = height / ink_h if height else width / ink_w

    glyphs = font.getGlyphSet()
    # y flips: font units grow upward, SVG user units grow downward.
    transform = Transform(scale, 0, 0, -scale, at[0] - x_min * scale, at[1] + y_max * scale)
    parts = []
    for name, offset in placed:
        pen = SVGPathPen(glyphs, ntos=lambda v: f"{v:.2f}")
        glyphs[name].draw(TransformPen(pen, transform.translate(offset, 0)))
        parts.append(pen.getCommands())
    return " ".join(p for p in parts if p), (ink_w * scale, ink_h * scale)


# --------------------------------------------------------------------------- #
# SVG assembly
# --------------------------------------------------------------------------- #

HEADER = "<!-- Generated by tests/make-wordmark.py — do not hand-edit. -->"


def svg(width: float, height: float, body: str, title: str, view: str | None = None) -> str:
    box = view or f"0 0 {width:g} {height:g}"
    return (
        f'{HEADER}\n<svg xmlns="http://www.w3.org/2000/svg" width="{width:g}" '
        f'height="{height:g}" viewBox="{box}" role="img" aria-label="{title}">\n'
        f"  <title>{title}</title>\n{body}\n</svg>\n"
    )


def colour_mark(box: float, cfg: dict, indent: str = "  ") -> str:
    tile, glyph = mark_paths(box, cfg)
    return (
        f'{indent}<path d="{tile}" fill="{BRAND_PRIMARY}"/>\n'
        f'{indent}<path d="{glyph}" fill="{SURFACE}"/>'
    )


def build_mark() -> str:
    return svg(128, 128, colour_mark(128, MARK), "autostand")


def build_favicon() -> str:
    return svg(32, 32, colour_mark(32, FAVICON), "autostand")


def build_horizontal(font: TTFont) -> str:
    size = 64.0
    gap = size * 0.30
    text_h = size * 0.50
    d, (w, _) = wordmark(font, WORD, height=text_h, at=(size + gap, (size - text_h) / 2))
    body = (
        f'  <g transform="scale({size / 128:.6f})">\n'
        f'{colour_mark(128, MARK, indent="    ")}\n  </g>\n'
        f'  <path d="{d}" fill="{FOREGROUND}"/>'
    )
    return svg(round(size + gap + w, 2), size, body, "autostand")


def build_vertical(font: TTFont) -> str:
    size = 96.0
    gap = size * 0.18
    d, (w, h) = wordmark(font, WORD, width=size * 1.38, at=(0, size + gap))
    body = (
        f'  <g transform="translate({(w - size) / 2:.2f} 0) scale({size / 128:.6f})">\n'
        f'{colour_mark(128, MARK, indent="    ")}\n  </g>\n'
        f'  <path d="{d}" fill="{FOREGROUND}"/>'
    )
    return svg(round(w, 2), round(size + gap + h, 2), body, "autostand")


def build_mono(font: TTFont) -> str:
    """Horizontal lockup in a single colour.

    The tile and the glyph are one evenodd path so the check knocks out of the
    card: two currentColor shapes stacked would render as a solid blob.
    """
    size = 64.0
    gap = size * 0.30
    text_h = size * 0.50
    d, (w, _) = wordmark(font, WORD, height=text_h, at=(size + gap, (size - text_h) / 2))
    tile, glyph = mark_paths(128, MARK)
    body = (
        f'  <g transform="scale({size / 128:.6f})">\n'
        f'    <path fill-rule="evenodd" d="{tile} {glyph}" fill="currentColor"/>\n'
        f"  </g>\n"
        f'  <path d="{d}" fill="currentColor"/>'
    )
    return svg(round(size + gap + w, 2), size, body, "autostand")


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", default=str(root / "brand" / "logo"))
    parser.add_argument("--font", default=os.environ.get("INTER_TTF"))
    args = parser.parse_args()

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    font = load_inter(args.font)

    files = {
        "logo-mark.svg": build_mark(),
        "logo-favicon.svg": build_favicon(),
        "logo-horizontal.svg": build_horizontal(font),
        "logo-vertical.svg": build_vertical(font),
        "logo-mono.svg": build_mono(font),
    }
    for name, content in files.items():
        (out / name).write_text(content, encoding="utf-8")
        print(f"wrote {out / name}  ({len(content.encode('utf-8'))} bytes)")


if __name__ == "__main__":
    main()
