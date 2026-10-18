# Independent reference for the image-mode goldens (no Rust code mirrored).
import math

def trunc_div(a, b):
    q = abs(a) // b
    return q if a >= 0 else -q

KERNELS = {
 'fs': ([(1,0,7),(-1,1,3),(0,1,5),(1,1,1)], 16),
 'jarvis': ([(1,0,7),(2,0,5),(-2,1,3),(-1,1,5),(0,1,7),(1,1,5),(2,1,3),(-2,2,1),(-1,2,3),(0,2,5),(1,2,3),(2,2,1)], 48),
 'stucki': ([(1,0,8),(2,0,4),(-2,1,2),(-1,1,4),(0,1,8),(1,1,4),(2,1,2),(-2,2,1),(-1,2,2),(0,2,4),(1,2,2),(2,2,1)], 42),
 'atkinson': ([(1,0,1),(2,0,1),(-1,1,1),(0,1,1),(1,1,1),(0,2,1)], 8),
}

def diffuse(img, name):
    taps, div = KERNELS[name]
    h, w = len(img), len(img[0])
    buf = [row[:] for row in img]
    out = [[False]*w for _ in range(h)]
    for y in range(h):
        for x in range(w):
            v = buf[y][x]
            burn = v < 128
            out[y][x] = burn
            err = v - (0 if burn else 255)
            for dx, dy, wt in taps:
                nx, ny = x+dx, y+dy
                if 0 <= nx < w and ny < h:
                    buf[ny][nx] += trunc_div(err*wt, div)
    return out

def show(b):
    return ["".join('#' if c else '.' for c in row) for row in b]

W, H = 8, 4
img = [[30 + 27*x + 10*y for x in range(W)] for y in range(H)]
print("input rows:", img)
for n in KERNELS:
    print(n, show(diffuse(img, n)))

# Ordered 4x4 (standard Bayer matrix, written out, not generated)
B4 = [[0,8,2,10],[12,4,14,6],[3,11,1,9],[15,7,13,5]]
img2 = [[(x*16 + y*64) % 256 for x in range(8)] for y in range(4)]
print("ordered input:", img2)
print("ordered4", show([[ (v+0.5)/256 < (B4[y%4][x%4]+0.5)/16 for x,v in enumerate(r)] for y,r in enumerate(img2)]))

# Halftone: spot function by numeric area (no closed form)
def cum_area(r, n=1200):
    # area of {|p|<=r} inside the unit square centred at 0, by midpoint grid
    cnt = 0
    for i in range(n):
        px = (i+0.5)/n - 0.5
        for j in range(n):
            py = (j+0.5)/n - 0.5
            if px*px+py*py <= r*r: cnt += 1
    return cnt/(n*n)

def halftone(grayrows, cell, ang):
    th = math.radians(ang); c, s = math.cos(th), math.sin(th)
    out = []
    for y, row in enumerate(grayrows):
        o = []
        for x, g in enumerate(row):
            d = 1 - g/255
            px, py = x+0.5, y+0.5
            u = px*c - py*s; v = px*s + py*c
            fu = (u/cell) % 1.0 - 0.5; fv = (v/cell) % 1.0 - 0.5
            r = math.hypot(fu, fv)
            o.append(d > 0 and d >= cum_area(min(r, 0.70710678), 300))
        out.append(o)
    return out

flat = [[191]*8 for _ in range(8)]   # d = 0.2510
print("halftone cell4 a0 g191", show(halftone(flat, 4, 0)))
flat2 = [[128]*8 for _ in range(8)]
print("halftone cell4 a0 g128", show(halftone(flat2, 4, 0)))
print("halftone cell4 a45 g128", show(halftone(flat2, 4, 45)))
print("halftone cell4 a30 g100", show(halftone([[100]*8 for _ in range(8)], 4, 30)))

# Sketch: Sobel, replicate borders, m = isqrt(gx^2+gy^2)//4 clipped 255, burn if m >= level
def sketch(img, level):
    h, w = len(img), len(img[0])
    g = lambda x, y: img[min(max(y,0),h-1)][min(max(x,0),w-1)]
    out = []
    for y in range(h):
        o = []
        for x in range(w):
            gx = (g(x+1,y-1)+2*g(x+1,y)+g(x+1,y+1)) - (g(x-1,y-1)+2*g(x-1,y)+g(x-1,y+1))
            gy = (g(x-1,y+1)+2*g(x,y+1)+g(x+1,y+1)) - (g(x-1,y-1)+2*g(x,y-1)+g(x+1,y-1))
            m = min(255, math.isqrt(gx*gx+gy*gy)//4)
            o.append(m >= level)
        out.append(o)
    return out
sk = [[0,0,0,255,255,255],[0,0,0,255,255,255],[0,0,0,255,255,255],[0,0,0,255,255,255],[0,0,0,255,255,255]]
print("sketch step l128", show(sketch(sk,128)))
sk2 = [[255]*6 for _ in range(3)] + [[0]*6 for _ in range(3)]
print("sketch hstep l200", show(sketch(sk2,200)))
diag = [[(255 if x>y else 0) for x in range(6)] for y in range(6)]
print("sketch diag l100", show(sketch(diag,100)))
