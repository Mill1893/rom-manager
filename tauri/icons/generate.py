#!/usr/bin/env python3
"""Generates the application icon set.

The icons are produced from this script rather than checked in as opaque
binaries, for the same reason the format fixtures are: a reviewer can see what
the artwork *is* and change it, instead of taking a blob on trust. Running this
reproduces every file byte-for-byte.

The shape is a cartridge — the thing the application manages — drawn in the warm
archival palette from `ui/src/tokens.ts`. Colours are duplicated here as literals
because a Python script cannot import TypeScript; the test suite checks they
still agree.

Depends on nothing outside the standard library, so it runs anywhere the repo
does.

    python3 tauri/icons/generate.py
"""

from __future__ import annotations

import pathlib
import struct
import zlib

# Must match `light` in ui/src/tokens.ts.
BACKGROUND = (0xF1, 0xEA, 0xDB)
FOREGROUND = (0x27, 0x24, 0x1E)
ACCENT = (0xA0, 0x3D, 0x17)
LINE = (0xCF, 0xC0, 0xA5)

HERE = pathlib.Path(__file__).parent


def cartridge(size: int) -> list[list[tuple[int, int, int, int]]]:
    """One cartridge, drawn proportionally so every size looks deliberate."""
    unit = size / 32.0
    pixels: list[list[tuple[int, int, int, int]]] = []

    body_left, body_right = 6 * unit, 26 * unit
    body_top, body_bottom = 4 * unit, 28 * unit
    # The shoulder where a cartridge narrows towards its connector.
    shoulder = 9 * unit
    notch_left, notch_right = 10 * unit, 22 * unit
    label_top, label_bottom = 8 * unit, 16 * unit
    contact_top = 24 * unit

    for y in range(size):
        row: list[tuple[int, int, int, int]] = []
        for x in range(size):
            colour = (*BACKGROUND, 0)

            inside_body = body_left <= x < body_right and body_top <= y < body_bottom
            # Above the shoulder the cartridge is narrower on both sides.
            if y < shoulder:
                inside_body = inside_body and notch_left <= x < notch_right

            if inside_body:
                colour = (*FOREGROUND, 255)

                if label_top <= y < label_bottom and (
                    body_left + 2 * unit <= x < body_right - 2 * unit
                ):
                    colour = (*ACCENT, 255)

                # Connector contacts: alternating teeth at the bottom edge.
                if y >= contact_top and int((x - body_left) / max(unit, 1)) % 2 == 0:
                    colour = (*LINE, 255)

            row.append(colour)
        pixels.append(row)
    return pixels


def write_png(path: pathlib.Path, pixels: list[list[tuple[int, int, int, int]]]) -> None:
    height, width = len(pixels), len(pixels[0])
    raw = bytearray()
    for row in pixels:
        raw.append(0)  # filter type 0: none
        for red, green, blue, alpha in row:
            raw += bytes((red, green, blue, alpha))

    def chunk(tag: bytes, payload: bytes) -> bytes:
        return (
            struct.pack(">I", len(payload))
            + tag
            + payload
            + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


def write_ico(path: pathlib.Path, sizes: list[int]) -> None:
    """A Windows .ico holding PNG-encoded entries, which Vista and later accept."""
    images = []
    for size in sizes:
        scratch = HERE / f".ico-{size}.png"
        write_png(scratch, cartridge(size))
        images.append((size, scratch.read_bytes()))
        scratch.unlink()

    header = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    directory, body = b"", b""
    for size, data in images:
        directory += struct.pack(
            "<BBBBHHII",
            0 if size >= 256 else size,
            0 if size >= 256 else size,
            0,
            0,
            1,
            32,
            len(data),
            offset,
        )
        body += data
        offset += len(data)
    path.write_bytes(header + directory + body)


def main() -> None:
    for size in (32, 128, 256, 512):
        write_png(HERE / f"{size}x{size}.png", cartridge(size))
    # Tauri looks for these exact names.
    write_png(HERE / "icon.png", cartridge(512))
    write_png(HERE / "128x128@2x.png", cartridge(256))
    write_ico(HERE / "icon.ico", [16, 32, 48, 256])
    print(f"wrote icons to {HERE}")


if __name__ == "__main__":
    main()
