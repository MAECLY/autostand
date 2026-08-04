#!/usr/bin/env python3
"""Generate the multi-size Windows icon for the Tauri bundle.

Writes `apps/autostand-app/src-tauri/icons/icon.ico` containing 16/32/48/64/
128/256 px frames. Every frame is rendered from the same vector geometry as the
PNGs and the .icns — `render_mark()` in `make-icons.py` — instead of being
downsampled from one bitmap, so the small sizes stay crisp.

Usage:
    python3 tests/make-ico.py [--out PATH]
"""

from __future__ import annotations

import argparse
import importlib.util
import struct
import sys
from pathlib import Path

from PIL import IcoImagePlugin, Image

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_OUT = REPO_ROOT / "apps" / "autostand-app" / "src-tauri" / "icons" / "icon.ico"
ICO_SIZES = (16, 32, 48, 64, 128, 256)


def _load_artwork():
    """Import make-icons.py by path (the hyphen makes it un-importable normally)."""
    path = Path(__file__).with_name("make-icons.py")
    sys.dont_write_bytecode = True  # keep a stray tests/__pycache__ out of the repo
    spec = importlib.util.spec_from_file_location("autostand_make_icons", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    # Registering first is required: the module defines a dataclass, and
    # dataclasses resolve their own module from sys.modules while executing.
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def write_ico(out_path: Path) -> Path:
    artwork = _load_artwork()
    frames = [artwork.render_mark(size) for size in sorted(ICO_SIZES, reverse=True)]
    out_path.parent.mkdir(parents=True, exist_ok=True)
    frames[0].save(
        out_path,
        format="ICO",
        sizes=[(s, s) for s in ICO_SIZES],
        append_images=frames[1:],  # exact-size frames are stored verbatim
    )
    return out_path


def verify_ico(path: Path) -> str:
    """Reopen the .ico and prove every declared frame is real artwork."""
    data = path.read_bytes()
    reserved, kind, count = struct.unpack("<HHH", data[:6])
    assert (reserved, kind) == (0, 1), f"{path.name}: not an icon file ({reserved}, {kind})"
    assert count == len(ICO_SIZES), f"{path.name}: {count} frames, expected {len(ICO_SIZES)}"

    declared = []
    for index in range(count):
        offset = 6 + index * 16
        width, height, _colors, _res, _planes, _bits, length, data_offset = struct.unpack(
            "<BBBBHHII", data[offset : offset + 16]
        )
        declared.append((width or 256, height or 256))
        assert data_offset + length <= len(data), f"{path.name}: frame {index} truncated"
    assert sorted(declared) == sorted((s, s) for s in ICO_SIZES), (
        f"{path.name}: sizes {sorted(declared)} != {sorted((s, s) for s in ICO_SIZES)}"
    )

    with path.open("rb") as handle:
        ico = IcoImagePlugin.IcoFile(handle)
        for size in sorted(ICO_SIZES):
            frame = ico.getimage((size, size)).convert("RGBA")
            assert frame.size == (size, size), f"{path.name}: {size}px frame is {frame.size}"
            colours = frame.getcolors(maxcolors=1 << 20)
            assert colours is not None and len(colours) > 1, (
                f"{path.name}: {size}px frame is a flat single colour"
            )
            total = size * size
            white = sum(n for n, (r, g, b, a) in colours if a > 200 and min(r, g, b) > 230)
            blue = sum(n for n, (r, g, b, a) in colours if a > 200 and b > 150 and r < 120)
            assert white / total > 0.05, f"{path.name}: {size}px frame has no mark"
            assert blue / total > 0.20, f"{path.name}: {size}px frame has no card"
            assert frame.getpixel((0, 0))[3] == 0, f"{path.name}: {size}px corner not transparent"

    listed = ", ".join(f"{w}x{h}" for w, h in sorted(declared))
    return f"{path.name}: {len(data):,} B, {count} frames ({listed}), RGBA"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUT, help="path to icon.ico")
    args = parser.parse_args()

    print(f"windows icon -> {args.out}")
    print("  " + verify_ico(write_ico(args.out)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
