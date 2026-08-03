#!/usr/bin/env python3
"""Generate a placeholder Windows .ico from the 32x32 PNG."""
import struct, os, sys

PNG_PATH = "apps/autostand-app/src-tauri/icons/32x32.png"
OUT = "apps/autostand-app/src-tauri/icons/icon.ico"

def make_ico(png_path, out_path):
    with open(png_path, "rb") as f:
        png_data = f.read()
    # ICO header: 6 bytes
    header = struct.pack("<HHH", 0, 1, 1)  # reserved, type=1 (icon), count=1
    # Directory entry: 16 bytes
    entry = struct.pack("<BBBBHHII", 32, 32, 0, 0, 1, 32, len(png_data), 22)
    with open(out_path, "wb") as f:
        f.write(header + entry + png_data)
    print(f"wrote {out_path} ({os.path.getsize(out_path)} bytes)")

make_ico(PNG_PATH, OUT)