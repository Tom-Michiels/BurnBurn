# Independent reference for acceptance test IM02 (all image modes on the same bitmap).
# Pure Python 3 standard library, no Rust code mirrored. The kernels are the transcriptions of
# docs/dev-notes/IM18-image-modes-reference.py (each from its published definition); this file only
# runs them on ONE common bitmap and writes the burn pattern per mode.
#
#   python3 -I test-data/acceptance/image_modes_reference.py > test-data/acceptance/image-modes-golden.txt
#
# Output: "<mode> <scale>: <row>/<row>/..." with '#' = laser on. Rows are top row first.
import math

W, H = 12, 6


def source():
    # A bitmap that has a gradient, a hard vertical edge, a one-pixel black line and a flat mid-gray block:
    # v(x, y) = 20 + 19 x + 7 y  (smooth ramp), then
    #   column 6 is a vertical black line, rows 4-5 of columns 8-11 are mid gray 128.
    img = [[min(255, 20 + 19 * x + 7 * y) for x in range(W)] for y in range(H)]
    for y in range(H):
        img[y][6] = 0
    for y in (4, 5):
        for x in range(8, 12):
            img[y][x] = 128
    return img


def trunc_div(a, b):
    q = abs(a) // b
    return q if a >= 0 else -q


KERNELS = {
    'floyd': ([(1, 0, 7), (-1, 1, 3), (0, 1, 5), (1, 1, 1)], 16),
    'jarvis': ([(1, 0, 7), (2, 0, 5), (-2, 1, 3), (-1, 1, 5), (0, 1, 7), (1, 1, 5), (2, 1, 3),
                (-2, 2, 1), (-1, 2, 3), (0, 2, 5), (1, 2, 3), (2, 2, 1)], 48),
    'stucki': ([(1, 0, 8), (2, 0, 4), (-2, 1, 2), (-1, 1, 4), (0, 1, 8), (1, 1, 4), (2, 1, 2),
                (-2, 2, 1), (-1, 2, 2), (0, 2, 4), (1, 2, 2), (2, 2, 1)], 42),
    'atkinson': ([(1, 0, 1), (2, 0, 1), (-1, 1, 1), (0, 1, 1), (1, 1, 1), (0, 2, 1)], 8),
}


def diffuse(img, name):
    taps, div = KERNELS[name]
    h, w = len(img), len(img[0])
    buf = [row[:] for row in img]
    out = [[False] * w for _ in range(h)]
    for y in range(h):
        for x in range(w):
            v = buf[y][x]
            burn = v < 128
            out[y][x] = burn
            err = v - (0 if burn else 255)
            for dx, dy, wt in taps:
                nx, ny = x + dx, y + dy
                if 0 <= nx < w and ny < h:
                    buf[ny][nx] += trunc_div(err * wt, div)
    return out


def threshold(img, level):
    return [[v < level for v in row] for row in img]


B4 = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]


def ordered4(img):
    return [[(v + 0.5) / 256 < (B4[y % 4][x % 4] + 0.5) / 16 for x, v in enumerate(r)] for y, r in enumerate(img)]


def cum_area(r, n=300):
    cnt = 0
    for i in range(n):
        px = (i + 0.5) / n - 0.5
        for j in range(n):
            py = (j + 0.5) / n - 0.5
            if px * px + py * py <= r * r:
                cnt += 1
    return cnt / (n * n)


def halftone(grayrows, cell, ang):
    th = math.radians(ang)
    c, s = math.cos(th), math.sin(th)
    out = []
    for y, row in enumerate(grayrows):
        o = []
        for x, g in enumerate(row):
            d = 1 - g / 255
            px, py = x + 0.5, y + 0.5
            u = px * c - py * s
            v = px * s + py * c
            fu = (u / cell) % 1.0 - 0.5
            fv = (v / cell) % 1.0 - 0.5
            r = math.hypot(fu, fv)
            o.append(d > 0 and d >= cum_area(min(r, 0.70710678), 300))
        out.append(o)
    return out


def sketch(img, level):
    h, w = len(img), len(img[0])
    g = lambda x, y: img[min(max(y, 0), h - 1)][min(max(x, 0), w - 1)]
    out = []
    for y in range(h):
        o = []
        for x in range(w):
            gx = (g(x + 1, y - 1) + 2 * g(x + 1, y) + g(x + 1, y + 1)) - (g(x - 1, y - 1) + 2 * g(x - 1, y) + g(x - 1, y + 1))
            gy = (g(x - 1, y + 1) + 2 * g(x, y + 1) + g(x + 1, y + 1)) - (g(x - 1, y - 1) + 2 * g(x, y - 1) + g(x + 1, y - 1))
            m = min(255, math.isqrt(gx * gx + gy * gy) // 4)
            o.append(m >= level)
        out.append(o)
    return out


def show(b):
    return "/".join("".join('#' if c else '.' for c in row) for row in b)


def upscale(img, k):
    return [[v for v in row for _ in range(k)] for row in img for _ in range(k)]


img = source()
print("# image-modes golden: written by image_modes_reference.py; source rows:")
for row in img:
    print("# " + " ".join(str(v) for v in row))
for scale in (1, 2):
    s = upscale(img, scale) if scale > 1 else img
    # the screens are specified in mm: 0.4 mm = 4 dots of 0.1 mm at either scale
    cell = 4
    print("threshold128 %d: %s" % (scale, show(threshold(s, 128))))
    for n in ('floyd', 'jarvis', 'stucki', 'atkinson'):
        print("%s %d: %s" % (n, scale, show(diffuse(s, n))))
    print("ordered4 %d: %s" % (scale, show(ordered4(s))))
    print("halftone0 %d: %s" % (scale, show(halftone(s, cell, 0))))
    print("halftone30 %d: %s" % (scale, show(halftone(s, cell, 30))))
    print("newsprint %d: %s" % (scale, show(halftone(s, cell, 45))))
    print("sketch100 %d: %s" % (scale, show(sketch(s, 100))))
    print("passthrough %d: %s" % (scale, show(threshold(s, 128))))
