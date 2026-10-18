#!/usr/bin/env python3
"""Independent reference for burnburn-text tests: reads head/cmap/hmtx/glyf of a TrueType font
with nothing but `struct` (no fontTools, no skrifa). Prints units-per-em and, per character,
glyph id, advance width and the glyf bounding box in font units.

Usage: python3 -I reference_metrics.py LiberationSans-Regular.ttf "HAVfio"
"""
import struct
import sys


def tables(data):
    num = struct.unpack(">H", data[4:6])[0]
    out = {}
    for i in range(num):
        tag, _, off, length = struct.unpack(">4sIII", data[12 + 16 * i:28 + 16 * i])
        out[tag.decode("latin-1")] = (off, length)
    return out


def cmap_format4(data, off):
    num = struct.unpack(">H", data[off + 2:off + 4])[0]
    for i in range(num):
        pid, eid, sub = struct.unpack(">HHI", data[off + 4 + 8 * i:off + 12 + 8 * i])
        if (pid, eid) in ((3, 1), (0, 3)):
            s = off + sub
            if struct.unpack(">H", data[s:s + 2])[0] != 4:
                continue
            segx2 = struct.unpack(">H", data[s + 6:s + 8])[0]
            seg = segx2 // 2
            ends = struct.unpack(">%dH" % seg, data[s + 14:s + 14 + segx2])
            starts = struct.unpack(">%dH" % seg, data[s + 16 + segx2:s + 16 + 2 * segx2])
            deltas = struct.unpack(">%dh" % seg, data[s + 16 + 2 * segx2:s + 16 + 3 * segx2])
            ro_off = s + 16 + 3 * segx2
            ranges = struct.unpack(">%dH" % seg, data[ro_off:ro_off + segx2])

            def lookup(cp):
                for k in range(seg):
                    if starts[k] <= cp <= ends[k]:
                        if ranges[k] == 0:
                            return (cp + deltas[k]) & 0xFFFF
                        a = ro_off + 2 * k + ranges[k] + 2 * (cp - starts[k])
                        g = struct.unpack(">H", data[a:a + 2])[0]
                        return (g + deltas[k]) & 0xFFFF if g else 0
                return 0

            return lookup
    raise SystemExit("no format-4 unicode cmap")


def main():
    data = open(sys.argv[1], "rb").read()
    t = tables(data)
    head = t["head"][0]
    upem = struct.unpack(">H", data[head + 18:head + 20])[0]
    loca_long = struct.unpack(">h", data[head + 50:head + 52])[0] == 1
    hhea = t["hhea"][0]
    n_hmetrics = struct.unpack(">H", data[hhea + 34:hhea + 36])[0]
    lookup = cmap_format4(data, t["cmap"][0])
    print("units_per_em", upem)
    for ch in sys.argv[2]:
        g = lookup(ord(ch))
        hm = t["hmtx"][0] + 4 * min(g, n_hmetrics - 1)
        adv = struct.unpack(">H", data[hm:hm + 2])[0]
        loca = t["loca"][0]
        if loca_long:
            a, b = struct.unpack(">II", data[loca + 4 * g:loca + 4 * g + 8])
        else:
            a, b = (2 * x for x in struct.unpack(">HH", data[loca + 2 * g:loca + 2 * g + 4]))
        bbox = None
        if b > a:
            gl = t["glyf"][0] + a
            bbox = struct.unpack(">hhhh", data[gl + 2:gl + 10])
        print(repr(ch), "gid", g, "advance", adv, "bbox(xMin,yMin,xMax,yMax)", bbox)


main()
