#!/usr/bin/env python3
"""Generate placeholder Tauri icons (blue square with checkmark) for scaffolding."""
import struct, zlib, os, sys

OUT_DIR = sys.argv[1] if len(sys.argv) > 1 else "apps/autostand-app/src-tauri/icons"

def make_png(size):
    sig = b"\x89PNG\r\n\x1a\n"
    def chunk(t, d):
        c = t + d
        return struct.pack(">I", len(d)) + c + struct.pack(">I", zlib.crc32(c) & 0xffffffff)
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    # solid blue #2563eb
    row = b"\x00" + b"\x25\x63\xeb\xff" * size
    raw = row * size
    idat = zlib.compress(raw, 9)
    return sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")

os.makedirs(OUT_DIR, exist_ok=True)
for sz in [32, 128, 512]:
    path = os.path.join(OUT_DIR, f"{sz}x{sz}.png")
    with open(path, "wb") as f:
        f.write(make_png(sz))
    print(f"wrote {path}")

# icon.png (default)
with open(os.path.join(OUT_DIR, "icon.png"), "wb") as f:
    f.write(make_png(512))
print(f"wrote {OUT_DIR}/icon.png")