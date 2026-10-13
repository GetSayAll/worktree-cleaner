#!/usr/bin/env python3
"""Generate the Worktree Cleaner app icon (rounded square + branch glyph).

Written with the standard library only: this machine has no Pillow or numpy.
Anti-aliasing comes from signed-distance coverage evaluated at 3x and box-downsampled.
"""
import math
import os
import struct
import zlib

OUT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src-tauri", "icons")

TOP = (0x35, 0x62, 0xdb)      # accent, top of the gradient
BOTTOM = (0x1b, 0x3c, 0x96)   # accent, bottom of the gradient
WHITE = (0xFF, 0xFF, 0xFF)
GLYPH_ALPHA = 0.94

# Glyph geometry in normalised icon coordinates (y grows downwards).
TRUNK = ((0.345, 0.245), (0.345, 0.755))
BRANCH_A = ((0.345, 0.415), (0.665, 0.300))
BRANCH_B = ((0.345, 0.585), (0.700, 0.715))
DOTS = [(0.345, 0.245), (0.665, 0.300), (0.700, 0.715)]
STROKE = 0.072   # half width of a stroke, normalised
DOT_R = 0.052    # dot radius, normalised
CORNER = 0.225   # rounded-square radius, normalised


def clamp01(value):
    return 0.0 if value < 0.0 else (1.0 if value > 1.0 else value)


def coverage(distance, half_extent, aa):
    return clamp01((half_extent + aa - distance) / aa)


def segment_alpha(px, py, x1, y1, x2, y2, half, aa):
    dx, dy = x2 - x1, y2 - y1
    length_sq = dx * dx + dy * dy
    if length_sq == 0.0:
        return math.hypot(px - x1, py - y1)
    t = ((px - x1) * dx + (py - y1) * dy) / length_sq
    t = 0.0 if t < 0.0 else (1.0 if t > 1.0 else t)
    return math.hypot(px - (x1 + t * dx), py - (y1 + t * dy))


def render(size, supersample=3):
    """Render an RGBA raster of `size` pixels."""
    span = size * supersample
    aa = 1.15 * supersample
    stroke = STROKE * span
    dot = DOT_R * span
    corner = CORNER * span
    segments = [
        (TRUNK[0][0] * span, TRUNK[0][1] * span, TRUNK[1][0] * span, TRUNK[1][1] * span),
        (BRANCH_A[0][0] * span, BRANCH_A[0][1] * span, BRANCH_A[1][0] * span, BRANCH_A[1][1] * span),
        (BRANCH_B[0][0] * span, BRANCH_B[0][1] * span, BRANCH_B[1][0] * span, BRANCH_B[1][1] * span),
    ]
    dots = [(x * span, y * span) for x, y in DOTS]

    pixels = bytearray(span * span * 4)
    index = 0
    for py in range(span):
        # Rounded-square coverage for this row: distance from the row to the
        # nearest corner circle centre, measured against the corner radius.
        cy = min(py, span - 1 - py)
        for px in range(span):
            cx = min(px, span - 1 - px)
            if cx >= corner or cy >= corner:
                rect = 1.0
            else:
                ox, oy = corner - cx, corner - cy
                rect = coverage(math.hypot(ox, oy), corner, aa)
            if rect <= 0.0:
                index += 4
                continue

            mix = py / (span - 1)
            red = TOP[0] + (BOTTOM[0] - TOP[0]) * mix
            green = TOP[1] + (BOTTOM[1] - TOP[1]) * mix
            blue = TOP[2] + (BOTTOM[2] - TOP[2]) * mix

            for x1, y1, x2, y2 in segments:
                distance = segment_alpha(px, py, x1, y1, x2, y2, stroke, aa)
                alpha = coverage(distance, stroke, aa) * GLYPH_ALPHA
                if alpha > 0.0:
                    red += (WHITE[0] - red) * alpha
                    green += (WHITE[1] - green) * alpha
                    blue += (WHITE[2] - blue) * alpha
            for dx, dy in dots:
                distance = math.hypot(px - dx, py - dy)
                alpha = coverage(distance, dot, aa) * GLYPH_ALPHA
                if alpha > 0.0:
                    red += (WHITE[0] - red) * alpha
                    green += (WHITE[1] - green) * alpha
                    blue += (WHITE[2] - blue) * alpha

            pixels[index] = int(red + 0.5)
            pixels[index + 1] = int(green + 0.5)
            pixels[index + 2] = int(blue + 0.5)
            pixels[index + 3] = int(rect * 255.0 + 0.5)
            index += 4

    # Box-filter the supersampled raster down to the requested size.
    if supersample == 1:
        return size, size, pixels
    small = bytearray(size * size * 4)
    area = supersample * supersample
    target = 0
    for y in range(size):
        for x in range(size):
            total = [0, 0, 0, 0]
            for sy in range(supersample):
                row = ((y * supersample) + sy) * span * 4
                base = row + x * supersample * 4
                for sx in range(supersample):
                    source = base + sx * 4
                    total[0] += pixels[source]
                    total[1] += pixels[source + 1]
                    total[2] += pixels[source + 2]
                    total[3] += pixels[source + 3]
            small[target] = total[0] // area
            small[target + 1] = total[1] // area
            small[target + 2] = total[2] // area
            small[target + 3] = total[3] // area
            target += 4
    return size, size, small


def write_png(path, width, height, rgba):
    def chunk(tag, payload):
        body = tag + payload
        return struct.pack(">I", len(payload)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    stride = width * 4
    raw = bytearray()
    for row in range(height):
        raw.append(0)  # filter type: none
        raw += rgba[row * stride:(row + 1) * stride]
    with open(path, "wb") as handle:
        handle.write(b"\x89PNG\r\n\x1a\n")
        handle.write(chunk(b"IHDR", header))
        handle.write(chunk(b"IDAT", zlib.compress(bytes(raw), 9)))
        handle.write(chunk(b"IEND", b""))


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    master = 512
    width, height, pixels = render(master)
    write_png(os.path.join(OUT_DIR, "icon.png"), width, height, pixels)
    for size in (32, 128, 256):
        small = render_smaller(pixels, master, size)
        name = "128x128@2x.png" if size == 256 else f"{size}x{size}.png"
        write_png(os.path.join(OUT_DIR, name), size, size, small)
    print("wrote icons to", OUT_DIR)


def render_smaller(source, source_size, size):
    """Box-filter an existing raster down to a smaller size."""
    if size >= source_size:
        return source
    step = source_size / size
    out = bytearray(size * size * 4)
    target = 0
    for y in range(size):
        y0, y1 = int(y * step), int((y + 1) * step)
        for x in range(size):
            x0, x1 = int(x * step), int((x + 1) * step)
            total = [0, 0, 0, 0]
            count = 0
            for sy in range(y0, y1):
                row = sy * source_size * 4
                for sx in range(x0, x1):
                    base = row + sx * 4
                    total[0] += source[base]
                    total[1] += source[base + 1]
                    total[2] += source[base + 2]
                    total[3] += source[base + 3]
                    count += 1
            out[target] = total[0] // count
            out[target + 1] = total[1] // count
            out[target + 2] = total[2] // count
            out[target + 3] = total[3] // count
            target += 4
    return out


if __name__ == "__main__":
    main()
