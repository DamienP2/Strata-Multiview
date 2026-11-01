#!/usr/bin/env python3
"""Writes a test stream with closed captions: H.264 in MPEG-TS, CEA-608 on
CC1 (pop-on, in colour) and on CC3 (roll-up), CEA-708 on service 1 (a
window at the bottom), carried as ATSC A/53 SEI in every picture.

    python3 scripts/cc-test-stream.py captions.ts [seconds]

Then send it to an input, for example over UDP:

    ffmpeg -re -stream_loop -1 -i captions.ts -c copy -f mpegts udp://127.0.0.1:5000

ffmpeg can read the CEA-608 back, to check:

    ffmpeg -f lavfi -i "movie=captions.ts[out0+subcc]" -map 0:1 -f srt -
"""

import os
import subprocess
import sys
import tempfile

RATE = (30000, 1001)
# cc_data triples a picture carries at 29.97 (ATSC A/53).
PER_PICTURE = 20


def parity(b):
    return b | 0x80 if bin(b & 0x7F).count('1') % 2 == 0 else b & 0x7F


# CEA-608's own characters: in the basic set, or special (0x11 0x3n).
BASIC = {'á': 0x2A, 'é': 0x5C, 'í': 0x5E, 'ó': 0x5F, 'ú': 0x60, 'ç': 0x7B, '÷': 0x7C, 'Ñ': 0x7D, 'ñ': 0x7E}
SPECIAL = {'®': 0x30, '°': 0x31, '½': 0x32, '¿': 0x33, '™': 0x34, '¢': 0x35, '£': 0x36, '♪': 0x37, 'à': 0x38, 'è': 0x3A, 'â': 0x3B, 'ê': 0x3C, 'î': 0x3D, 'ô': 0x3E, 'û': 0x3F}


def pairs_608(s, channel2=False):
    """Text as CEA-608 byte pairs (a special character is a pair of its own)."""
    out, held = [], None
    for ch in s:
        if ch in SPECIAL:
            if held is not None:
                out.append((held, 0))
                held = None
            out.append((0x11 | (0x08 if channel2 else 0), SPECIAL[ch]))
            continue
        b = BASIC.get(ch, ord(ch) if 0x20 <= ord(ch) < 0x7F else ord('?'))
        if held is None:
            held = b
        else:
            out.append((held, b))
            held = None
    if held is not None:
        out.append((held, 0))
    return out


def doubled(a, b):
    """A control code, sent twice as CEA-608 asks."""
    return [(a, b), (a, b)]


def pac(row, attr, channel2=False):
    """A preamble address code: row 1-15, attribute bits (colour or indent << 1 | underline)."""
    table = {1: (0x11, 0x40), 2: (0x11, 0x60), 3: (0x12, 0x40), 4: (0x12, 0x60), 5: (0x15, 0x40), 6: (0x15, 0x60), 7: (0x16, 0x40), 8: (0x16, 0x60),
             9: (0x17, 0x40), 10: (0x17, 0x60), 11: (0x10, 0x40), 12: (0x13, 0x40), 13: (0x13, 0x60), 14: (0x14, 0x40), 15: (0x14, 0x60)}
    b1, b2 = table[row]
    return doubled(b1 | (0x08 if channel2 else 0), b2 | attr)


def pop_on(lines):
    """A pop-on caption on CC1: [(row, attr, text)]."""
    out = doubled(0x14, 0x20) + doubled(0x14, 0x2E)
    for row, attr, text in lines:
        out += pac(row, attr) + pairs_608(text)
    return out + doubled(0x14, 0x2F)


def roll_up(text, first=False):
    """A roll-up line (two rows) on CC3 (field 2: control codes 0x15)."""
    out = doubled(0x15, 0x25)
    if first:
        out += [(0x14, 0x60), (0x14, 0x60)]
    out += doubled(0x15, 0x2D) + pairs_608(text)
    return out


def dtvcc(seq, block):
    """A CEA-708 packet for service 1, as (type, byte, byte)."""
    assert len(block) <= 31
    data = bytes([(1 << 5) | len(block)]) + bytes(block)
    p = bytearray([0]) + data
    if len(p) % 2:
        p.append(0)
    p[0] = (seq << 6) | (len(p) // 2)
    return [(3 if i == 0 else 2, p[i], p[i + 1]) for i in range(0, len(p), 2)]


def window_708(seq, text):
    """Window 0 at the bottom centre, cleared (FF: the pen back at the start), the text in yellow, shown."""
    b = [0x98, 0x20, 70, 105, (7 << 4) | 0, 31, 0x00]
    b += [0x0C, 0x91, 0b00111100, 0x00, 0x00]
    b += list(text.encode('latin-1', 'replace'))[:31 - len(b) - 2]
    b += [0x89, 0x01]
    return dtvcc(seq, b)


def sei(triples):
    m = bytearray([0xB5, 0x00, 0x31]) + b'GA94' + bytes([0x03, 0x40 | len(triples), 0xFF])
    for valid, kind, a, b in triples:
        m += bytes([0xF8 | (valid << 2) | kind, a, b])
    m.append(0xFF)
    payload = bytearray([4])
    size = len(m)
    while size >= 255:
        payload.append(255)
        size -= 255
    payload.append(size)
    payload += m
    payload.append(0x80)
    # Emulation prevention.
    out = bytearray()
    zeros = 0
    for v in payload:
        if zeros >= 2 and v <= 3:
            out.append(3)
            zeros = 0
        out.append(v)
        zeros = zeros + 1 if v == 0 else 0
    return b'\x00\x00\x00\x01\x06' + bytes(out)


def schedule(pictures):
    """What each picture carries: CC1 (field 1), CC3 (field 2) and DTVCC queues."""
    fps = RATE[0] / RATE[1]
    f1, f2, d = {}, {}, {}
    captions = [
        [(14, 0x0A, 'Strata Multiview'), (15, 0x00, 'Sous-titres CEA-608 - CC1')],
        [(14, 0x00, 'Caractères : é à ç ♪'), (15, 0x03, 'en vert, souligné')],
        [(13, 0x12, 'Pop-on : trois lignes'), (14, 0x0E, 'en italique'), (15, 0x00, 'et en blanc')],
    ]
    t, n, seq = 0, 0, 0
    while t < pictures:
        f1[t] = pop_on(captions[n % len(captions)])
        f2[t] = roll_up(f'Roll-up CC3 : ligne {n + 1}', first=n == 0)
        d[t] = window_708(seq, f'Service 708 n\xb0{n + 1}')
        seq = (seq + 1) % 4
        n += 1
        t += int(fps * 3)
    # Each queue spread over the pictures from its start.
    q1, q2, qd = [], [], []
    per = []
    for i in range(pictures):
        q1 += f1.get(i, [])
        q2 += f2.get(i, [])
        qd += d.get(i, [])
        tr = []
        a = q1.pop(0) if q1 else (0, 0)
        tr.append((1, 0, parity(a[0]), parity(a[1])))
        b = q2.pop(0) if q2 else (0, 0)
        tr.append((1, 1, parity(b[0]), parity(b[1])))
        while len(tr) < PER_PICTURE:
            if qd:
                k, x, y = qd.pop(0)
                tr.append((1, k, x, y))
            else:
                tr.append((0, 2, 0, 0))
        per.append(tr)
    return per


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    out = sys.argv[1]
    seconds = float(sys.argv[2]) if len(sys.argv) > 2 else 30
    pictures = int(seconds * RATE[0] / RATE[1])
    with tempfile.TemporaryDirectory() as tmp:
        raw = os.path.join(tmp, 'raw.h264')
        subprocess.run(['ffmpeg', '-v', 'error', '-y', '-f', 'lavfi', '-i', f'testsrc2=size=1280x720:rate={RATE[0]}/{RATE[1]}', '-f', 'lavfi', '-i', 'sine=f=440:r=48000',
                        '-frames:v', str(pictures), '-c:v', 'libx264', '-bf', '0', '-g', '30', '-x264-params', 'aud=1', '-pix_fmt', 'yuv420p', '-an', '-f', 'h264', raw], check=True)
        data = open(raw, 'rb').read()
        per = schedule(pictures)
        # After each access unit delimiter, the picture's SEI.
        aud = b'\x00\x00\x00\x01\x09'
        parts = data.split(aud)
        body = bytearray(parts[0])
        for i, p in enumerate(parts[1:]):
            # The delimiter's own byte, then the SEI.
            body += aud + p[:1] + sei(per[min(i, len(per) - 1)]) + p[1:]
        cc = os.path.join(tmp, 'cc.h264')
        open(cc, 'wb').write(body)
        subprocess.run(['ffmpeg', '-v', 'error', '-y', '-r', f'{RATE[0]}/{RATE[1]}', '-f', 'h264', '-i', cc, '-f', 'lavfi', '-t', str(seconds), '-i', 'sine=f=440:r=48000',
                        '-map', '0:v', '-map', '1:a', '-c:v', 'copy', '-c:a', 'aac', '-f', 'mpegts', out], check=True)
    print(f'{out}: {pictures} pictures with CC1, CC3 and CEA-708 service 1')


if __name__ == '__main__':
    main()
