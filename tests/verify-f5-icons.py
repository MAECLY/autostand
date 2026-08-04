#!/usr/bin/env python3
"""F5 gate: the Tauri app icons must be real artwork, not a flat colour square.

Asserts every PNG under src-tauri/icons/ has the size Tauri expects for that
filename and more than one distinct colour — a placeholder solid fill is the
regression this guards against.
"""

from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image

ICONS = Path(__file__).resolve().parent.parent / "apps/autostand-app/src-tauri/icons"

EXPECTED_SIZE = {
    "32x32.png": (32, 32),
    "128x128.png": (128, 128),
    "128x128@2x.png": (256, 256),
    "512x512.png": (512, 512),
    "icon.png": (512, 512),
}

failures: list[str] = []

for name, size in EXPECTED_SIZE.items():
    path = ICONS / name
    if not path.is_file():
        failures.append(f"{name}: missing")
        continue

    with Image.open(path) as img:
        img.load()
        if img.size != size:
            failures.append(f"{name}: size {img.size}, expected {size}")

        rgba = img.convert("RGBA")
        # getcolors(0) means "no cap", so this is the true distinct-colour count.
        colors = rgba.getcolors(maxcolors=rgba.width * rgba.height)
        distinct = len(colors) if colors else 0
        if distinct <= 1:
            failures.append(f"{name}: {distinct} distinct colour(s) — flat fill")
        else:
            print(f"ok  {name}  {img.size}  {distinct} distinct colours  mode={img.mode}")

for name in sorted(p.name for p in ICONS.glob("*.png")):
    if name not in EXPECTED_SIZE:
        failures.append(f"{name}: unexpected PNG, add it to EXPECTED_SIZE")

if failures:
    print("\nFAIL")
    for line in failures:
        print(f"  {line}")
    sys.exit(1)

print("\nPASS — every app icon is sized correctly and carries real artwork")
