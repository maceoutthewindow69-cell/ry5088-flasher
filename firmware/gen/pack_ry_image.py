#!/usr/bin/env python3
"""Pack an app-only RY5088 build into the slice expected by the resident DFU.

Input is linked for 0x08005200. Output starts at 0x08005000 with the 0x200-byte
keyboard chip-ID header, preserving ry-flash's image/type/vector safety checks.
"""
from __future__ import annotations

import struct
import sys
from pathlib import Path

CHIP_ID = b"AT32F405 8KMKB"
HEADER_SIZE = 0x200
APP_MIN = 0x08005200
APP_MAX = 0x08028000
SRAM_MIN = 0x20000000
SRAM_MAX = 0x20020000


def main() -> int:
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} APP.bin OUTPUT_flash.bin", file=sys.stderr)
        return 2

    src, dst = map(Path, sys.argv[1:])
    app = src.read_bytes()
    if len(app) < 8:
        raise SystemExit("app image is too short")

    sp, rv = struct.unpack_from("<II", app, 0)
    reset = rv & ~1
    if not (SRAM_MIN <= sp <= SRAM_MAX):
        raise SystemExit(f"invalid initial SP 0x{sp:08x}")
    if not (rv & 1 and APP_MIN <= reset < APP_MAX):
        raise SystemExit(f"invalid reset vector 0x{rv:08x}")
    if HEADER_SIZE + len(app) > APP_MAX - 0x08005000:
        raise SystemExit("image exceeds resident bootloader application region")

    header = bytearray(HEADER_SIZE)
    # Match the factory 16-byte field exactly: string + two spaces, then zero fill.
    header[:16] = CHIP_ID + b"  "
    out = bytes(header) + app
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_bytes(out)
    print(f"packed {src} -> {dst} ({len(out)} bytes, SP=0x{sp:08x}, reset=0x{rv:08x})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
