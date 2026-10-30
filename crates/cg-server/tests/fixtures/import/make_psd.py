#!/usr/bin/env python3
"""Writes the small Photoshop fixtures of the PSD importer (psd.rs tests).

Run from anywhere: python3 make_psd.py — the files land next to it.
The bytes follow Adobe's « Photoshop File Formats Specification ».
"""
import os
import struct
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))


def be(fmt, *v):
    return struct.pack(">" + fmt, *v)


# ---------------------------------------------------------------- pieces --

def pascal(s, pad):
    b = s.encode("latin-1", "replace")[:255]
    out = bytes([len(b)]) + b
    while len(out) % pad:
        out += b"\0"
    return out


def ustr(s, null=True):
    u = (s + ("\0" if null else "")).encode("utf-16-be")
    return be("I", len(u) // 2) + u


def key(k):
    kb = k.encode()
    return be("I", 0) + kb if len(kb) == 4 else be("I", len(kb)) + kb


def desc(cls, items):
    out = ustr("") + key(cls) + be("I", len(items))
    for k, v in items:
        out += key(k) + v
    return out


def objc(cls, items):
    return b"Objc" + desc(cls, items)


def doub(v):
    return b"doub" + be("d", v)


def untf(unit, v):
    return b"UntF" + unit.encode() + be("d", v)


def long_(v):
    return b"long" + be("i", v)


def bool_(v):
    return b"bool" + bytes([1 if v else 0])


def enum(t, v):
    return b"enum" + key(t) + key(v)


def text(s):
    return b"TEXT" + ustr(s)


def tdta(b):
    return b"tdta" + be("I", len(b)) + b


def vlls(items):
    return b"VlLs" + be("I", len(items)) + b"".join(items)


def rgbc(r, g, b):
    return objc("RGBC", [("Rd  ", doub(r)), ("Grn ", doub(g)), ("Bl  ", doub(b))])


def block(k, data):
    if len(data) % 2:
        data += b"\0"
    return b"8BIM" + k + be("I", len(data)) + data


def packbits(data):
    out = bytearray()
    i, n = 0, len(data)
    while i < n:
        j = i
        while j + 1 < n and data[j + 1] == data[i] and j - i < 127:
            j += 1
        run = j - i + 1
        if run >= 3:
            out += bytes([(257 - run) & 0xFF, data[i]])
            i += run
            continue
        start = i
        while i < n and i - start < 128:
            if i + 2 < n and data[i] == data[i + 1] == data[i + 2]:
                break
            i += 1
        out += bytes([i - start - 1]) + bytes(data[start:i])
    return bytes(out)


def rle(planes, w, h, big=False):
    """Compression 1: the row lengths of every plane, then the rows."""
    rows = [packbits(p[y * w:(y + 1) * w]) for p in planes for y in range(h)]
    fmt = "I" if big else "H"
    return be("H", 1) + b"".join(be(fmt, len(r)) for r in rows) + b"".join(rows)


def raw(planes):
    return be("H", 0) + b"".join(bytes(p) for p in planes)


def zip16_pred(plane16, w, h):
    """Compression 3 of 16-bit samples: row deltas, then zlib."""
    out = bytearray()
    for y in range(h):
        prev = 0
        for x in range(w):
            v = plane16[y * w + x]
            out += be("H", (v - prev) & 0xFFFF)
            prev = v
    return be("H", 3) + zlib.compress(bytes(out))


def fill(w, h, v):
    return bytes([v]) * (w * h)


# ------------------------------------------------------------------ paths --

def fixed(v, size):
    return int(round(v / size * (1 << 24)))


def knot(pre, anchor, leave, W, H, sel=2):
    out = be("H", sel)
    for (x, y) in (pre, anchor, leave):
        out += be("ii", fixed(y, H), fixed(x, W))
    return out


def path(knots, W, H):
    out = be("H", 6) + b"\0" * 24
    out += be("H", 8) + be("H", 0) + b"\0" * 22
    out += be("H", 0) + be("HhHII", len(knots), 1, 1, 0, 0) + b"\0" * 10
    for k in knots:
        out += knot(*k, W, H)
    return be("II", 3, 0) + out


def rect_path(x0, y0, x1, y1, W, H):
    pts = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    return path([(p, p, p) for p in pts], W, H)


def rounded_path(x0, y0, x1, y1, r, W, H):
    k = 0.5523 * r
    return path([
        ((x0 + r - k, y0), (x0 + r, y0), (x0 + r, y0)),
        ((x1 - r, y0), (x1 - r, y0), (x1 - r + k, y0)),
        ((x1, y0 + r - k), (x1, y0 + r), (x1, y0 + r)),
        ((x1, y1 - r), (x1, y1 - r), (x1, y1 - r + k)),
        ((x1 - r + k, y1), (x1 - r, y1), (x1 - r, y1)),
        ((x0 + r, y1), (x0 + r, y1), (x0 + r - k, y1)),
        ((x0, y1 - r + k), (x0, y1 - r), (x0, y1 - r)),
        ((x0, y0 + r), (x0, y0 + r), (x0, y0 + r - k)),
    ], W, H)


def poly_path(pts, W, H):
    return path([(p, p, p) for p in pts], W, H)


def ellipse_path(cx, cy, r, W, H):
    k = 0.5523 * r
    return path([
        ((cx - k, cy - r), (cx, cy - r), (cx + k, cy - r)),
        ((cx + r, cy - k), (cx + r, cy), (cx + r, cy + k)),
        ((cx + k, cy + r), (cx, cy + r), (cx - k, cy + r)),
        ((cx - r, cy + k), (cx - r, cy), (cx - r, cy - k)),
    ], W, H)


# ------------------------------------------------------------------- text --

def estr(s):
    b = b"\xfe\xff" + s.encode("utf-16-be")
    b = b.replace(b"\\", b"\\\\").replace(b"(", b"\\(").replace(b")", b"\\)")
    return b"(" + b + b")"


def engine(s, font, size, rgb, just, tracking=0):
    r, g, b = rgb
    style = (b"<< /Font 1 /FontSize %.1f /AutoLeading true /Tracking %d "
             b"/FillColor << /Type 1 /Values [ 1.0 %.4f %.4f %.4f ] >> >>"
             % (size, tracking, r, g, b))
    return (
        b"\n\n<<\n\t/EngineDict\n\t<<\n\t\t/Editor\n\t\t<<\n\t\t\t/Text " + estr(s + "\r") +
        b"\n\t\t>>\n\t\t/ParagraphRun << /RunArray [ << /ParagraphSheet << /DefaultStyleSheet 0"
        b" /Properties << /Justification %d >> >> >> ] >>" % just +
        b"\n\t\t/StyleRun << /RunArray [ << /StyleSheet << /StyleSheetData " + style +
        b" >> >> ] /RunLengthArray [ %d ] >>" % (len(s) + 1) +
        b"\n\t\t/Rendered << /Shapes << /Children [ << /ShapeType 0 >> ] >> >>"
        b"\n\t>>\n\t/ResourceDict\n\t<<\n\t\t/FontSet [ << /Name " + estr("AdobeInvisFont") +
        b" /Script 0 >> << /Name " + estr(font) + b" /Script 0 >> ]"
        b"\n\t\t/StyleSheetSet [ << /Name " + estr("Normal RGB") +
        b" /StyleSheetData << /Font 0 /FontSize 12.0 /FillColor << /Type 1 /Values [ 1.0 0.0 0.0 0.0 ] >> >> >> ]"
        b"\n\t>>\n>>"
    )


def type_tool(s, font, size, rgb, just, t, bounds, tracking=0):
    l, tp, r, bt = bounds
    d = desc("TxLr", [
        ("Txt ", text(s)),
        ("textGridding", enum("textGridding", "None")),
        ("Ornt", enum("Ornt", "Hrzn")),
        ("AntA", enum("Annt", "antiAliasSharp")),
        ("bounds", objc("bounds", [("Left", untf("#Pnt", l)), ("Top ", untf("#Pnt", tp)),
                                   ("Rght", untf("#Pnt", r)), ("Btom", untf("#Pnt", bt))])),
        ("TextIndex", long_(0)),
        ("EngineData", tdta(engine(s, font, size, rgb, just, tracking))),
    ])
    warp = desc("warp", [("warpStyle", enum("warpStyle", "warpNone")),
                         ("warpValue", doub(0)), ("warpPerspective", doub(0)),
                         ("warpPerspectiveOther", doub(0)),
                         ("warpRotate", enum("Ornt", "Hrzn"))])
    return (be("H", 1) + be("6d", *t) + be("H", 50) + be("I", 16) + d +
            be("H", 1) + be("I", 16) + warp + be("4i", 0, 0, 0, 0))


# ----------------------------------------------------------------- layers --

class L:
    def __init__(self, name, rect=(0, 0, 0, 0), chans=(), blend=b"norm", opacity=255,
                 clipping=0, flags=0, mask=None, blocks=b""):
        self.name, self.rect, self.chans = name, rect, list(chans)
        self.blend, self.opacity, self.clipping, self.flags = blend, opacity, clipping, flags
        self.mask, self.blocks = mask, blocks


def empty_chans(n=4):
    ids = [-1, 0, 1, 2][:n]
    return [(i, be("H", 0)) for i in ids]


def records(layers, big=False):
    lf = "Q" if big else "I"
    recs, data = b"", b""
    for l in layers:
        t, le, b, r = l.rect
        rec = be("4i", t, le, b, r) + be("H", len(l.chans))
        for cid, d in l.chans:
            rec += be("h", cid) + be(lf, len(d))
        rec += b"8BIM" + l.blend + bytes([l.opacity, l.clipping, l.flags, 0])
        if l.mask:
            mt, ml, mb, mr, default = l.mask
            m = be("4i", mt, ml, mb, mr) + bytes([default, 0]) + b"\0\0"
        else:
            m = b""
        extra = be("I", len(m)) + m + be("I", 0) + pascal(l.name, 4)
        extra += block(b"luni", ustr(l.name, False)) + l.blocks
        rec += be("I", len(extra)) + extra
        recs += rec
        data += b"".join(d for _, d in l.chans)
    return recs, data


def document(w, h, layers, composite, mode=3, depth=8, channels=3, big=False, angle=120,
             negative=False):
    lf = "Q" if big else "I"
    recs, data = records(layers, big)
    count = -len(layers) if negative else len(layers)
    info = be("h", count) + recs + data
    if len(info) % 2:
        info += b"\0"
    lm = be(lf, len(info)) + info + be("I", 0)
    res = b"8BIM" + be("H", 1037) + b"\0\0" + be("I", 4) + be("i", angle)
    return (b"8BPS" + be("H", 2 if big else 1) + b"\0" * 6 + be("H", channels) +
            be("I", h) + be("I", w) + be("H", depth) + be("H", mode) +
            be("I", 0) + be("I", len(res)) + res + be(lf, len(lm)) + lm + composite)


def section(kind, extra=b""):
    data = be("I", kind)
    if kind in (1, 2):
        data += b"8BIM" + b"pass"
    return block(b"lsct", data + extra)


def divider():
    return L("</Layer group>", chans=empty_chans(), blocks=section(3))


def pixel(name, top, left, bottom, right, rgba, compress="rle", **kw):
    w, h = right - left, bottom - top
    planes = [bytes(rgba[c::4]) for c in range(4)]
    chans = []
    for cid, p in zip([-1, 0, 1, 2], [planes[3], planes[0], planes[1], planes[2]]):
        chans.append((cid, rle([p], w, h) if compress == "rle" else raw([p])))
    return L(name, (top, left, bottom, right), chans, **kw)


def solid(w, h, rgb, a=255):
    return bytes([rgb[0], rgb[1], rgb[2], a]) * (w * h)


def effects(items):
    return block(b"lfx2", be("II", 0, 16) + desc("null", [
        ("Scl ", untf("#Prc", 100)), ("masterFXSwitch", bool_(True))] + items))


def drop_shadow(opacity, distance, size, angle=120, use_global=True):
    return ("DrSh", objc("DrSh", [
        ("enab", bool_(True)), ("Md  ", enum("BlnM", "Mltp")), ("Clr ", rgbc(0, 0, 0)),
        ("Opct", untf("#Prc", opacity)), ("uglg", bool_(use_global)),
        ("lagl", untf("#Ang", angle)), ("Dstn", untf("#Pxl", distance)),
        ("Ckmt", untf("#Pxl", 0)), ("blur", untf("#Pxl", size)),
        ("Nose", untf("#Prc", 0)), ("AntA", bool_(False))]))


def soco(rgb):
    return block(b"SoCo", be("I", 16) + desc("null", [("Clr ", rgbc(*rgb))]))


def composite_of(w, h, layers_rgba, channels=3):
    """A plain RGB composite: the planes given as RGBA bytes."""
    planes = [bytes(layers_rgba[c::4]) for c in range(channels)]
    return rle(planes, w, h)


# ------------------------------------------------------------- fixtures ----

def layers_psd():
    W, H = 200, 120
    bg = solid(W, H, (20, 30, 80))
    # Plate: red rectangle drawn as a shape (its pixels too).
    plate = pixel("Plate", 70, 20, 110, 180, solid(160, 40, (220, 20, 60)),
                  blocks=soco((220, 20, 60)) + block(b"vmsk", rect_path(20, 70, 180, 110, W, H)) +
                  effects([drop_shadow(50, 4, 6),
                           ("OrGl", objc("OrGl", [("enab", bool_(True)),
                                                  ("Md  ", enum("BlnM", "Scrn")),
                                                  ("Clr ", rgbc(255, 255, 0)),
                                                  ("Opct", untf("#Prc", 75))]))]))
    title = pixel("Title", 75, 30, 100, 160, solid(130, 25, (255, 255, 255), 0),
                  blocks=block(b"TySh", type_tool("Hello world", "Montserrat-Bold", 24.0,
                                                  (1.0, 1.0, 1.0), 0, (1, 0, 0, 1, 30, 95),
                                                  (0, -22, 130, 6), tracking=50)))
    subtitle = pixel("Subtitle", 20, 30, 80, 150, solid(120, 60, (255, 255, 255), 0),
                     blocks=block(b"TySh", type_tool("Line one\rLine two", "Helvetica-Bold", 12.0,
                                                     (1.0, 0.8, 0.0), 2, (2, 0, 0, 2, 90, 40),
                                                     (-30, -10, 30, 20))))
    group = L("Lower third", chans=empty_chans(), blend=b"pass", opacity=204,
              blocks=section(1))
    # Logo: a 40 × 30 picture, its mask hiding the right half.
    lw, lh = 40, 30
    logo_px = bytearray()
    for y in range(lh):
        for x in range(lw):
            logo_px += bytes([x * 6, y * 8, 200, 255])
    logo = pixel("Logo", 10, 150, 40, 190, bytes(logo_px))
    mplane = bytes([255 if x < 20 else 0 for y in range(lh) for x in range(lw)])
    logo.chans.append((-2, rle([mplane], lw, lh)))
    logo.mask = (10, 150, 40, 190, 0)
    clipped = pixel("Clipped", 0, 140, 50, 200, solid(60, 50, (255, 0, 0)), clipping=1)
    hidden = pixel("Hidden dot", 5, 5, 15, 15, solid(10, 10, (0, 255, 0)), compress="raw",
                   flags=2)
    levels = L("Levels", chans=empty_chans(),
               blocks=block(b"levl", be("H", 2) + b"\0" * 290))
    dot = L("Dot", chans=empty_chans(),
            blocks=soco((0, 0, 255)) + block(b"vmsk", ellipse_path(100, 30, 15, W, H)))
    tint = L("Tint", chans=empty_chans(), blend=b"mul ", opacity=128,
             blocks=soco((255, 255, 0)))
    bgl = pixel("Background", 0, 0, H, W, bg, compress="raw")
    layers = [bgl, divider(), plate, title, subtitle, group, logo, clipped, hidden, levels,
              dot, tint]
    return document(W, H, layers, composite_of(W, H, bg), angle=90)


def artboards_psd():
    W, H = 400, 200

    def artb(l, t, r, b):
        return block(b"artb", be("I", 16) + desc("artboard", [
            ("artboardRect", objc("classFloatRect", [("Top ", doub(t)), ("Left", doub(l)),
                                                     ("Btom", doub(b)), ("Rght", doub(r))])),
            ("guideIndeces", vlls([])), ("artboardPresetName", text("")),
            ("Clr ", rgbc(255, 255, 255)), ("artboardBackgroundType", long_(1))]))

    red = pixel("Red square", 50, 50, 150, 150, solid(100, 100, (255, 0, 0)))
    board_a = L("Board A", chans=empty_chans(), blend=b"pass",
                blocks=section(1) + artb(0, 0, 200, 200))
    grad = block(b"GdFl", be("I", 16) + desc("null", [
        ("Grad", objc("Grdn", [
            ("Nm  ", text("Custom")), ("GrdF", enum("GrdF", "CstS")), ("Intr", doub(4096)),
            ("Clrs", vlls([
                objc("Clrt", [("Clr ", rgbc(0, 0, 255)), ("Type", enum("Clry", "UsrS")),
                              ("Lctn", long_(0)), ("Mdpn", long_(50))]),
                objc("Clrt", [("Clr ", rgbc(0, 255, 0)), ("Type", enum("Clry", "UsrS")),
                              ("Lctn", long_(2048)), ("Mdpn", long_(50))]),
                objc("Clrt", [("Clr ", rgbc(255, 255, 255)), ("Type", enum("Clry", "UsrS")),
                              ("Lctn", long_(4096)), ("Mdpn", long_(50))])])),
            ("Trns", vlls([
                objc("TrnS", [("Opct", untf("#Prc", 100)), ("Lctn", long_(0)),
                              ("Mdpn", long_(50))]),
                objc("TrnS", [("Opct", untf("#Prc", 50)), ("Lctn", long_(4096)),
                              ("Mdpn", long_(50))])]))])),
        ("Angl", untf("#Ang", 90)), ("Type", enum("GrdT", "Lnr ")),
        ("Algn", bool_(True)), ("Scl ", untf("#Prc", 100))]))
    sky = L("Sky", chans=empty_chans(), blocks=grad)
    caption = pixel("Caption", 40, 240, 70, 380, solid(140, 30, (0, 0, 0), 0),
                    blocks=block(b"TySh", type_tool("Bonjour", "Montserrat-Regular", 20.0,
                                                    (0.0, 0.0, 0.0), 0, (1, 0, 0, 1, 240, 60),
                                                    (0, -18, 100, 5))) +
                    effects([drop_shadow(80, 3, 2, angle=180, use_global=False)]))
    badge = L("Badge", chans=empty_chans(),
              blocks=soco((255, 128, 0)) + block(b"vmsk", rounded_path(230, 80, 290, 110, 8, W, H)))
    triangle = L("Triangle", chans=empty_chans(),
                 blocks=soco((0, 128, 0)) +
                 block(b"vmsk", poly_path([(320, 80), (350, 110), (290, 110)], W, H)))
    photo = pixel("Photo", 10, 360, 30, 390, solid(30, 20, (10, 200, 10)),
                  blocks=block(b"SoLd", b"soLD" + be("II", 4, 16) +
                               desc("null", [("Idnt", text("1234"))])))
    board_b = L("Board B", chans=empty_chans(), blend=b"pass",
                blocks=section(1) + artb(220, 0, 400, 120))
    loose = pixel("Loose", 150, 205, 160, 215, solid(10, 10, (0, 0, 0)))
    layers = [divider(), red, board_a, divider(), sky, caption, badge, triangle, photo,
              board_b, loose]
    comp = bytearray(solid(W, H, (255, 255, 255)))
    for y in range(50, 150):
        for x in range(50, 150):
            comp[(y * W + x) * 4:(y * W + x) * 4 + 3] = bytes([255, 0, 0])
    return document(W, H, layers, composite_of(W, H, bytes(comp)))


def gray16_psb():
    """A large-document (PSB) 16-bit grayscale file, its layer ZIP-predicted."""
    W, H = 32, 16
    lw, lh = 16, 8
    values = [(x * 4096 + y * 1000) & 0xFFFF for y in range(lh) for x in range(lw)]
    alpha = [0xFFFF] * (lw * lh)
    layer = L("Ramp", (4, 8, 4 + lh, 8 + lw),
              [(-1, zip16_pred(alpha, lw, lh)), (0, zip16_pred(values, lw, lh))])
    comp = be("H", 0) + b"".join(be("H", 0x8000) for _ in range(W * H))
    return document(W, H, [layer], comp, mode=1, depth=16, channels=1, big=True)


def cmyk_psd():
    """A flat CMYK document (no layers): red then blue halves."""
    W, H = 8, 4
    left = [x < 4 for y in range(H) for x in range(W)]
    # Stored inverted: 255 = no ink.
    c = bytes(255 if l else 0 for l in left)
    m = bytes(0 if l else 0 for l in left)
    yy = bytes(0 if l else 255 for l in left)
    k = bytes([255] * (W * H))
    return document(W, H, [], rle([c, m, yy, k], W, H), mode=4, channels=4)


if __name__ == "__main__":
    for name, data in [("layers.psd", layers_psd()), ("artboards.psd", artboards_psd()),
                       ("gray16.psb", gray16_psb()), ("cmyk.psd", cmyk_psd())]:
        with open(os.path.join(HERE, name), "wb") as f:
            f.write(data)
        print(name, len(data))
