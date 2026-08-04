#!/usr/bin/env python3
"""Generate the autostand raster brand assets from vector geometry.

Draws the brand mark described in `docs/design-system/02-brand.md` — a rounded
"standup card" with a lightning bolt forming a checkmark — and emits:

  apps/autostand-app/src-tauri/icons/32x32.png
  apps/autostand-app/src-tauri/icons/128x128.png
  apps/autostand-app/src-tauri/icons/128x128@2x.png   (256px)
  apps/autostand-app/src-tauri/icons/512x512.png
  apps/autostand-app/src-tauri/icons/icon.png         (512px)
  apps/autostand-app/src-tauri/icons/icon.icns        (macOS, needs `iconutil`)
  brand/logo/logo-og.png                              (1200x630 Open Graph card)

The Windows `icon.ico` is written by `make-ico.py`, which imports `render_mark`
from this file so every asset is provably the same artwork.

Why Pillow and not an SVG rasteriser: this machine has no cairosvg / rsvg /
inkscape / magick, so the geometry is drawn directly. It is rendered at
SUPERSAMPLE x and downsampled with LANCZOS, which is what keeps the diagonals
clean at 32px.

Usage:
    python3 tests/make-icons.py [--out DIR] [--og PATH] [--skip-og] [--preview DIR]
"""

from __future__ import annotations

import argparse
import math
import os
import shutil
import subprocess
import sys
import tempfile
import urllib.request
from dataclasses import dataclass
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_ICON_DIR = REPO_ROOT / "apps" / "autostand-app" / "src-tauri" / "icons"
DEFAULT_OG_PATH = REPO_ROOT / "brand" / "logo" / "logo-og.png"

# Palette — docs/design-system/01-tokens.md. Keep these in sync with tokens.css.
BLUE_600 = (0x25, 0x63, 0xEB)  # --brand-primary
BLUE_500 = (0x3B, 0x82, 0xF6)  # --color-blue-500
SLATE_950 = (0x02, 0x06, 0x17)  # --bg-base (dark)
SLATE_900 = (0x0F, 0x17, 0x2A)  # --bg-surface (dark)
SLATE_800 = (0x1E, 0x29, 0x3B)  # --bg-elevated (dark)
SLATE_400 = (0x94, 0xA3, 0xB8)  # --fg-muted (dark)
SLATE_300 = (0xCB, 0xD5, 0xE1)  # --color-slate-300
SLATE_50 = (0xF8, 0xFA, 0xFC)  # --fg-base (dark)
WHITE = (0xFF, 0xFF, 0xFF)

SUPERSAMPLE = 4  # render at 4x, downsample with LANCZOS

# --- mark geometry -----------------------------------------------------------
# All coordinates live in a 128-unit design space (same viewBox as
# brand/logo/logo-mark.svg) and are scaled by the requested size. The shape is
# auto-fitted into the tile afterwards, so tweaking a number here changes the
# drawing, never the composition.

MARK_SHORT_TIP = (30.0, 60.0)  # tip of the check's short arm
MARK_VERTEX = (57.0, 87.0)  # where the two arms meet
MARK_LONG_TIP = (95.0, 30.0)  # tip of the long arm, before the bolt offset
MARK_HALF_WIDTH = 10.0  # half the stroke thickness
MARK_BOLT_AT = 0.50  # where the bolt kink sits along the long arm (0..1)
MARK_BOLT_OFFSET = 13.0  # lateral jump of the kink; > 2 * half width would split the shape
MARK_BOLT_SKEW = 0.10  # stagger between the two crossbars, so they interlock like a bolt
MARK_TIP_POINT = 11.0  # how far the long arm tapers past its edges


@dataclass(frozen=True)
class _Vec:
    x: float
    y: float

    def __add__(self, other: "_Vec") -> "_Vec":
        return _Vec(self.x + other.x, self.y + other.y)

    def __sub__(self, other: "_Vec") -> "_Vec":
        return _Vec(self.x - other.x, self.y - other.y)

    def __mul__(self, k: float) -> "_Vec":
        return _Vec(self.x * k, self.y * k)

    def unit(self) -> "_Vec":
        length = math.hypot(self.x, self.y)
        return _Vec(self.x / length, self.y / length)

    def inner_normal(self) -> "_Vec":
        """Unit normal pointing at the concave side of the check."""
        u = self.unit()
        return _Vec(u.y, -u.x)

    def as_tuple(self) -> tuple[float, float]:
        return (self.x, self.y)


def _intersect(p1: _Vec, d1: _Vec, p2: _Vec, d2: _Vec) -> _Vec:
    """Intersection of two lines given as point + direction (miter join)."""
    det = d1.x * -d2.y - -d2.x * d1.y
    if abs(det) < 1e-9:
        raise ValueError("parallel edges cannot be mitred")
    rhs = p2 - p1
    t = (rhs.x * -d2.y - -d2.x * rhs.y) / det
    return p1 + d1 * t


def bolt_check_outline() -> list[tuple[float, float]]:
    """The mark as a single closed polygon in 128-unit design space.

    A checkmark whose long arm carries the interlocking lateral jump of a
    lightning bolt: below the kink the band sits on the centreline, above it the
    band is shifted toward the concave side. The two halves still overlap
    (offset < 2 * half width), so the silhouette stays one connected shape and
    still reads as a check at 16px.
    """
    short_tip = _Vec(*MARK_SHORT_TIP)
    vertex = _Vec(*MARK_VERTEX)
    long_tip = _Vec(*MARK_LONG_TIP)

    short_dir = vertex - short_tip
    long_dir = long_tip - vertex
    n_short = short_dir.inner_normal()
    n_long = long_dir.inner_normal()
    h = MARK_HALF_WIDTH
    jump = n_long * MARK_BOLT_OFFSET
    # The two crossbars are staggered along the arm, which is what makes the two
    # halves interlock instead of reading as a single folded ribbon.
    kink_outer = vertex + long_dir * (MARK_BOLT_AT + MARK_BOLT_SKEW)
    kink_inner = vertex + long_dir * (MARK_BOLT_AT - MARK_BOLT_SKEW)

    # Outer (convex) side of both arms, then the tip, then back along the inner side.
    short_outer = short_tip - n_short * h
    short_inner = short_tip + n_short * h
    corner_outer = _intersect(short_outer, short_dir, vertex - n_long * h, long_dir)
    corner_inner = _intersect(short_inner, short_dir, vertex + n_long * h, long_dir)

    kink_outer_low = kink_outer - n_long * h
    kink_inner_low = kink_inner + n_long * h
    apex = long_tip + jump + long_dir.unit() * MARK_TIP_POINT

    points = [
        short_outer,
        corner_outer,
        kink_outer_low,
        kink_outer_low + jump,  # the bolt's crossbar, outer side
        long_tip - n_long * h + jump,
        apex,
        long_tip + n_long * h + jump,
        kink_inner_low + jump,
        kink_inner_low,  # the bolt's crossbar, inner side
        corner_inner,
        short_inner,
    ]
    return [p.as_tuple() for p in points]


def render_mark(
    size: int,
    *,
    tile_color: tuple[int, int, int] | None = BLUE_600,
    mark_color: tuple[int, int, int] = WHITE,
    radius_ratio: float = 0.22,
    mark_ratio: float = 0.60,
    supersample: int = SUPERSAMPLE,
) -> Image.Image:
    """Render the brand mark at `size` x `size` as RGBA.

    This is the single source of every raster asset. `tile_color=None` drops the
    rounded card and returns just the bolt-check (used for monochrome contexts).
    Pixels outside the rounded corners stay fully transparent.
    """
    if size < 1:
        raise ValueError("size must be positive")
    big = size * supersample
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    if tile_color is not None:
        draw.rounded_rectangle(
            [(0, 0), (big - 1, big - 1)], radius=radius_ratio * big, fill=(*tile_color, 255)
        )

    outline = bolt_check_outline()
    xs = [p[0] for p in outline]
    ys = [p[1] for p in outline]
    span = max(max(xs) - min(xs), max(ys) - min(ys))
    scale = (mark_ratio * big) / span
    # Centre the mark's bounding box on the tile so composition survives any
    # tweak to the geometry constants above.
    off_x = (big - (max(xs) - min(xs)) * scale) / 2 - min(xs) * scale
    off_y = (big - (max(ys) - min(ys)) * scale) / 2 - min(ys) * scale
    placed = [(x * scale + off_x, y * scale + off_y) for x, y in outline]
    draw.polygon(placed, fill=(*mark_color, 255))

    return img.resize((size, size), Image.LANCZOS)


# --- fonts -------------------------------------------------------------------

_GOOGLE_CSS = "https://fonts.googleapis.com/css2?family=Inter:wght@{weight}"
_LEGACY_UA = "Mozilla/4.0"  # old UA => Google Fonts serves TTF instead of woff2
_FONT_CACHE = Path(tempfile.gettempdir()) / "autostand-brand-fonts"
# Documented fallbacks, in order, if Inter cannot be fetched. Pillow cannot read
# the woff2 files the app self-hosts, so these are the only local options.
_FALLBACK_FONTS = (
    "/System/Library/Fonts/SFNS.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
)


def _inter_ttf(weight: int) -> Path | None:
    """Path to an Inter TTF for `weight`, downloading (and caching) if needed."""
    override = os.environ.get("AUTOSTAND_INTER_DIR")
    if override:
        for name in (f"Inter-{weight}.ttf", f"Inter_{weight}.ttf", "Inter.ttf"):
            candidate = Path(override) / name
            if candidate.is_file():
                return candidate

    cached = _FONT_CACHE / f"Inter-{weight}.ttf"
    if cached.is_file():
        return cached

    try:
        request = urllib.request.Request(
            _GOOGLE_CSS.format(weight=weight), headers={"User-Agent": _LEGACY_UA}
        )
        with urllib.request.urlopen(request, timeout=20) as response:
            css = response.read().decode("utf-8")
        start = css.index("url(") + 4
        url = css[start : css.index(")", start)]
        with urllib.request.urlopen(url, timeout=30) as response:
            data = response.read()
        _FONT_CACHE.mkdir(parents=True, exist_ok=True)
        cached.write_bytes(data)
        return cached
    except Exception as exc:  # offline or Google changed the payload
        print(f"  ! could not fetch Inter {weight}: {exc}", file=sys.stderr)
        return None


def load_font(weight: int, size: int) -> ImageFont.FreeTypeFont:
    """Inter at `weight`, or the first documented fallback that loads."""
    path = _inter_ttf(weight)
    if path is not None:
        return ImageFont.truetype(str(path), size)

    for fallback in _FALLBACK_FONTS:
        if Path(fallback).is_file():
            print(f"  ! Inter unavailable, falling back to {fallback}", file=sys.stderr)
            font = ImageFont.truetype(fallback, size)
            try:  # SFNS is variable; ask for the closest named instance
                font.set_variation_by_name("Bold" if weight >= 600 else "Regular")
            except Exception:
                pass
            return font
    raise RuntimeError("no usable TTF found; set AUTOSTAND_INTER_DIR to a folder with Inter TTFs")


# --- assets ------------------------------------------------------------------

PNG_SIZES = {
    "32x32.png": 32,
    "128x128.png": 128,
    "128x128@2x.png": 256,
    "512x512.png": 512,
    "icon.png": 512,
}
ICNS_SIZES = (  # (filename, pixel size) per Apple's iconset layout
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
)


def write_pngs(out_dir: Path) -> list[Path]:
    out_dir.mkdir(parents=True, exist_ok=True)
    written = []
    for name, size in PNG_SIZES.items():
        path = out_dir / name
        render_mark(size).save(path, "PNG")
        written.append(path)
    return written


def write_icns(out_dir: Path) -> Path | None:
    """Build a real .icns via macOS `iconutil`. Returns None if unavailable."""
    iconutil = shutil.which("iconutil")
    if iconutil is None:
        print(
            "  ! iconutil not found — icon.icns NOT regenerated. A valid icns cannot be "
            "produced here without it; run this script on macOS.",
            file=sys.stderr,
        )
        return None

    out_path = out_dir / "icon.icns"
    with tempfile.TemporaryDirectory() as tmp:
        iconset = Path(tmp) / "autostand.iconset"
        iconset.mkdir()
        for name, size in ICNS_SIZES:
            render_mark(size).save(iconset / name, "PNG")
        result = subprocess.run(
            [iconutil, "-c", "icns", str(iconset), "-o", str(out_path)],
            capture_output=True,
            text=True,
            check=False,
        )
    if result.returncode != 0:
        print(f"  ! iconutil failed: {result.stderr.strip()}", file=sys.stderr)
        return None
    return out_path


def render_og_card(width: int = 1200, height: int = 630) -> Image.Image:
    """The Open Graph card: mark + wordmark + tagline on the dark brand surface."""
    # Diagonal wash from --bg-base to --bg-surface, echoing .hero-gradient.
    # Computed on a tiny grid and scaled up: smooth, and Pillow-only.
    grid = 64
    ramp = Image.new("RGB", (grid, grid))
    ramp_px = ramp.load()
    for gy in range(grid):
        for gx in range(grid):
            t = (gx / (grid - 1) + gy / (grid - 1)) / 2
            ramp_px[gx, gy] = tuple(round(a + (b - a) * t) for a, b in zip(SLATE_950, SLATE_900))
    card = ramp.resize((width, height), Image.BICUBIC)
    draw = ImageDraw.Draw(card)

    # Soft brand glow behind the lockup (gradients are allowed on backgrounds,
    # never inside the mark itself — docs/design-system/02-brand.md).
    glow = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    glow_draw = ImageDraw.Draw(glow)
    cx, cy, r0 = 250, 250, 460
    for step in range(48, 0, -1):
        r = r0 * step / 48
        alpha = round(34 * (1 - step / 48) ** 2)
        glow_draw.ellipse([(cx - r, cy - r), (cx + r, cy + r)], fill=(*BLUE_500, alpha))
    card = Image.alpha_composite(card.convert("RGBA"), glow).convert("RGB")
    draw = ImageDraw.Draw(card)

    margin = 96
    mark_size = 152
    mark_top = 152
    mark = render_mark(mark_size)
    card.paste(mark, (margin, mark_top), mark)

    wordmark_font = load_font(700, 104)
    tagline_font = load_font(400, 46)
    repo_font = load_font(500, 26)

    # Optically centre the wordmark on the mark using its ink box, not its line box.
    word_box = draw.textbbox((0, 0), "autostand", font=wordmark_font)
    word_y = mark_top + (mark_size - (word_box[3] - word_box[1])) / 2 - word_box[1]
    draw.text((margin + mark_size + 40, word_y), "autostand", font=wordmark_font, fill=SLATE_50)

    draw.text(
        (margin, 366),
        "Automate your standup. Know what you did.",
        font=tagline_font,
        fill=SLATE_300,
    )

    rule_y = 478
    draw.rectangle([(margin, rule_y), (margin + 200, rule_y + 2)], fill=SLATE_800)
    draw.text((margin, rule_y + 34), "github.com/MAECLY/autostand", font=repo_font, fill=SLATE_400)

    return card


# --- verification ------------------------------------------------------------


def verify_icon_png(path: Path, expected: int) -> str:
    """Reopen a written icon and prove it is the real artwork, not a flat fill."""
    with Image.open(path) as img:
        img.load()
        assert img.size == (expected, expected), f"{path.name}: {img.size} != {(expected, expected)}"
        assert img.mode == "RGBA", f"{path.name}: mode {img.mode} != RGBA"
        rgba = img.convert("RGBA")
        corner = rgba.getpixel((0, 0))
        colours = rgba.getcolors(maxcolors=1 << 20)

    # The bug this replaces shipped a flat blue square, so prove the artwork is
    # there: more than one colour, a transparent corner, white mark, blue card.
    assert colours is not None and len(colours) > 1, f"{path.name}: flat single-colour image"
    assert corner[3] == 0, f"{path.name}: corner not transparent ({corner})"
    total = expected * expected
    white = sum(n for n, (r, g, b, a) in colours if a > 200 and min(r, g, b) > 230)
    blue = sum(n for n, (r, g, b, a) in colours if a > 200 and b > 150 and r < 120)
    assert white / total > 0.05, f"{path.name}: mark missing (white {white / total:.3%})"
    assert blue / total > 0.20, f"{path.name}: card missing (blue {blue / total:.3%})"
    return (
        f"{path.name}: {expected}x{expected} RGBA, {path.stat().st_size:,} B, "
        f"{len(colours):,} colours, mark {white / total:.1%}, card {blue / total:.1%}"
    )


def verify_og(path: Path, width: int, height: int) -> str:
    with Image.open(path) as img:
        img.load()
        assert img.size == (width, height), f"{path.name}: {img.size} != {(width, height)}"
        assert img.mode == "RGB", f"{path.name}: mode {img.mode} != RGB"
        colours = img.convert("RGB").getcolors(maxcolors=1 << 20)
    assert colours is not None and len(colours) > 1, f"{path.name}: flat single-colour image"
    return f"{path.name}: {width}x{height} RGB, {path.stat().st_size:,} B, {len(colours):,} colours"


def verify_icns(path: Path) -> str:
    data = path.read_bytes()
    assert data[:4] == b"icns", f"{path.name}: missing icns magic"
    declared = int.from_bytes(data[4:8], "big")
    assert declared == len(data), f"{path.name}: length {declared} != {len(data)} bytes"
    # Walk the top-level chunks so a truncated container cannot pass.
    offset, types = 8, []
    while offset < len(data):
        kind = data[offset : offset + 4].decode("ascii", "replace")
        length = int.from_bytes(data[offset + 4 : offset + 8], "big")
        assert length >= 8, f"{path.name}: bad chunk length at {offset}"
        types.append(kind)
        offset += length
    assert offset == len(data), f"{path.name}: chunk walk overran the file"
    return f"{path.name}: {len(data):,} B, {len(types)} chunks ({' '.join(types)})"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=DEFAULT_ICON_DIR, help="Tauri icons directory")
    parser.add_argument("--og", type=Path, default=DEFAULT_OG_PATH, help="Open Graph card path")
    parser.add_argument("--skip-og", action="store_true", help="only regenerate the app icons")
    parser.add_argument("--skip-icons", action="store_true", help="only regenerate the OG card")
    args = parser.parse_args()

    failures = 0

    if not args.skip_icons:
        print(f"icons -> {args.out}")
        for path in write_pngs(args.out):
            print("  " + verify_icon_png(path, PNG_SIZES[path.name]))
        icns = write_icns(args.out)
        if icns is None:
            failures += 1
        else:
            print("  " + verify_icns(icns))

    if not args.skip_og:
        print(f"social card -> {args.og}")
        args.og.parent.mkdir(parents=True, exist_ok=True)
        card = render_og_card()
        card.save(args.og, "PNG", optimize=True)
        print("  " + verify_og(args.og, card.width, card.height))

    if not args.skip_icons:
        print("note: run tests/make-ico.py for the Windows icon.ico")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
