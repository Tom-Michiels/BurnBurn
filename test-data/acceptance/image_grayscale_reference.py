# Independent reference for the grayscale image mode (G6-machine, C05): laser power follows the
# gray value of every dot. Pure Python 3 standard library with exact rational arithmetic; no Rust
# code is mirrored and nothing is read from the Rust implementation.
#
#   python3 -I test-data/acceptance/image_grayscale_reference.py > test-data/acceptance/image-grayscale-golden.txt
#
# Definition used (LightBurn "Grayscale" image mode, published behaviour): a black pixel (0) burns at
# the layer's maximum power, a white pixel (255) does not burn, and in between the power is
# linear in the darkness: power = min + (max - min) * (255 - gray) / 255. "Negative" swaps dark and
# light first (gray := 255 - gray).
#
# The layer used by the test is 10 % .. 80 %. Output: "<variant> <scale>: <row>/<row>/..." with
# one cell per dot, top row first, a cell being the power in thousandths of full power
# (round half up) or "-" for a dot that does not burn.
from fractions import Fraction

W, H = 12, 6
LO, HI = Fraction(10, 100), Fraction(80, 100)


def source():
    # The bitmap of image_modes_reference.py: a ramp, a black vertical line and a flat mid-gray block.
    img = [[min(255, 20 + 19 * x + 7 * y) for x in range(W)] for y in range(H)]
    for y in range(H):
        img[y][6] = 0
    for y in (4, 5):
        for x in range(8, 12):
            img[y][x] = 128
    return img


def upscale(img, k):
    return [[v for v in row for _ in range(k)] for row in img for _ in range(k)]


def thousandths(power):
    # round half up on an exact rational
    return int((power * 1000 * 2 + 1) // 2)


def cell(gray, negative):
    if negative:
        gray = 255 - gray
    if gray >= 255:
        return "-"
    power = LO + (HI - LO) * Fraction(255 - gray, 255)
    return str(thousandths(power))


def show(img, negative):
    return "/".join(",".join(cell(v, negative) for v in row) for row in img)


img = source()
print("# grayscale golden: written by image_grayscale_reference.py; layer power 10 % .. 80 %;")
print("# source rows (same bitmap as image-modes-golden.txt):")
for row in img:
    print("# " + " ".join(str(v) for v in row))
for scale in (1, 2):
    s = upscale(img, scale) if scale > 1 else img
    print("grayscale %d: %s" % (scale, show(s, False)))
    print("grayscale-negative %d: %s" % (scale, show(s, True)))
