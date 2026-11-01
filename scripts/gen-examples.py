#!/usr/bin/env python3
"""Writes the example multiviews: crates/strata-mv-server/examples/ex-NN-*.json.

    python3 scripts/gen-examples.py

Each example is a Project with one page; its tiles read `{{input:N}}`, the
N-th input of the server (src/examples.rs puts the inputs in when a copy is
made). The ids are derived from the example and the layer, so running it
again gives the same files. A layer out of the picture, or two widgets over
each other, stops the script.
"""

import copy
import hashlib
import json
import pathlib
import sys

OUT = pathlib.Path(__file__).resolve().parent.parent / 'crates/strata-mv-server/examples'

# The layers as the designer saves them (rect, text, broadcast widgets).
PROTO = json.loads(r'''{
"rect":{"parent":null,"type":"rect","shape":"rect","perCorner":false,"corners":[0.0,0.0,0.0,0.0],"skewUnit":"px","skewPx":120.0,"skewDeg":15.0,"trapMode":"percent","trapTop":70.0,"trapLeft":80.0,"trapRight":200.0,"vertexRound":0.0,"fillStyle":{"kind":"solid","color2":"#ff3b5c","angle":90.0,"image":"","fit":"cover"},"strokeStyle":{"position":"inside","dash":"solid","join":"miter","cap":"butt"},"visible":true,"locked":false,"props":{},"tracks":{},"bindings":{}},
"text":{"parent":null,"type":"text","text":"","fontFamily":"Inter","fontWeight":700,"align":"left","verticalAlign":"middle","uppercase":false,"speed":120.0,"separator":"   •   ","overflow":"fit","squeeze":0.8,"minSize":0.6,"warnBelow":0.8,"maxLines":0,"breakMode":"word","lang":"","fitEnd":"clip","italic":false,"fillType":"solid","color2":"#a594ff","gradAngle":90.0,"strokeOutside":false,"shadow":false,"shadowColor":"#000000aa","shadowX":0.0,"shadowY":4.0,"shadowBlur":12.0,"band":false,"bandColor":"#24164f","bandPadX":26.0,"bandGap":10.0,"bandRadius":0.0,"bandAccent":true,"bandAccentColor":"#ff3b5c","justify":false,"dir":"ltr","visible":true,"locked":false,"props":{},"tracks":{},"bindings":{}},
"status":{"parent":null,"type":"status","ptp":false,"ntp":false,"alarms":true,"background":"#00000000","visible":true,"locked":false,"props":{},"tracks":{},"bindings":{}},
"timecode":{"parent":null,"type":"timecode","input":"","label":"","color":"#fbbf24","background":"#00000000","visible":true,"locked":false,"props":{},"tracks":{},"bindings":{}},
"source":{"parent":null,"type":"source","input":"","fit":"contain","background":"#0b0d12","border":4.0,"tally":{"border":true,"program":"#e5212b","preview":"#17b14a","aux":"#f2b705","off":"#262a33"},"umd":{"position":"bottom","size":11.0,"outside":false,"text":"","font":"Inter","weight":600,"align":"center","color":"#ffffff","background":"#000000b3","lamps":true},"vu":{"position":"right","width":6.0,"gap":2.0,"margin":6.0,"min":-60.0,"yellow":-18.0,"red":-9.0,"hold":true,"max":2,"first":0},"badge":false,"noSignal":"","noSignalLook":"bars","timecode":"off","loudness":false,"alarms":true,"markers":{"action":false,"title":false,"fourThree":false,"fourteenNine":false,"centre":false,"afd":false,"color":"#ffffffb3"},"visible":true,"locked":false,"props":{},"tracks":{},"bindings":{}},
"audioMeter":{"parent":null,"type":"audioMeter","input":"","channels":16,"labels":"","scale":true,"values":false,"hold":true,"loudness":true,"min":-60.0,"yellow":-18.0,"red":-9.0,"background":"#0b0d12d9","visible":true,"locked":false,"props":{},"tracks":{},"bindings":{}},
"loudness":{"parent":null,"type":"loudness","input":"","target":-23.0,"tolerance":1.0,"history":true,"background":"#0b0d12d9","visible":true,"locked":false,"props":{},"tracks":{},"bindings":{}},
"alarmLog":{"parent":null,"type":"alarmLog","rows":6,"ended":true,"background":"#0b0d12d9","visible":true,"locked":false,"props":{},"tracks":{},"bindings":{}},
"scope":{"parent":null,"type":"scope","input":"","kind":"waveform","graticule":true,"labels":true,"intensity":1.0,"color":"#6dff8f","background":"#05070b","visible":true,"locked":false,"props":{},"tracks":{},"bindings":{}},
"signalInfo":{"parent":null,"type":"signalInfo","input":"","video":true,"color":true,"hdr":true,"audio":true,"background":"#0b0d12d9","visible":true,"locked":false,"props":{},"tracks":{},"bindings":{}},
"chart":{"parent":null,"type":"chart","metric":"ptpOffset","input":"","channel":"","title":"","window":120,"min":0.0,"max":0.0,"warn":0.0,"crit":0.0,"color":"#4ed8ff","fill":true,"stats":true,"background":"#0b0d12d9","visible":true,"locked":false,"props":{},"tracks":{},"bindings":{}},
"value":{"parent":null,"type":"value","input":"","kind":"timecode","label":"","showLabel":true,"stack":false,"align":"center","warn":0.0,"crit":0.0,"color":"#ffffff","background":"#0b0d12d9","visible":true,"locked":false,"props":{},"tracks":{},"bindings":{}}
}''')

LAYOUT_ONLY = {'rect', 'text'}
STEREO = 'L, R'
SURROUND = 'L, R, C, LFE, Ls, Rs, L, R, AD, AD, VO, VO, 13, 14, 15, 16'


def rgba(hex_: str):
    h = hex_.lstrip('#')
    a = int(h[6:8], 16) / 255 if len(h) == 8 else 1.0
    return [round(int(h[i:i + 2], 16) / 255, 6) for i in (0, 2, 4)] + [round(a, 6)]


def inp(n):
    return f'{{{{input:{n}}}}}' if n else ''


class Page:
    """One example: its size and its layers, from the back to the front."""

    def __init__(self, slug, w=1920, h=1080):
        self.slug, self.w, self.h = slug, w, h
        self.layers = []

    def add(self, widget, name, box, **cfg):
        x, y, w, h = box
        layer = copy.deepcopy(PROTO[widget])
        props = cfg.pop('props', {})
        for k, v in cfg.items():
            if isinstance(v, dict) and isinstance(layer.get(k), dict):
                layer[k].update(v)
            else:
                layer[k] = v
        n = len(self.layers)
        lid = hashlib.sha1(f'{self.slug}/{n}/{name}'.encode()).hexdigest()[:12]
        layer = {'id': lid, 'name': name, **layer}
        layer['props'] = {'x': float(round(x)), 'y': float(round(y)), 'width': float(round(w)), 'height': float(round(h)), **props}
        self.layers.append(layer)
        return layer

    # Layout.
    def rect(self, box, color, name='Fond', radius=0, grad=None):
        cfg = {'props': {'fill': rgba(color)}}
        if radius:
            cfg['corners'] = [float(radius)] * 4
        if grad:
            cfg['fillStyle'] = {'kind': 'linear', 'color2': grad[0], 'angle': float(grad[1])}
        return self.add('rect', name, box, **cfg)

    def text(self, box, text, size=30, color='#f5f7fa', weight=700, align='left', upper=False, name=None):
        return self.add('text', name or text, box, text=text, fontWeight=weight, align=align, uppercase=upper, props={'fill': rgba(color), 'fontSize': float(size)})

    def label(self, box, text, color='#8a93a3', size=14):
        return self.text(box, text, size=size, color=color, weight=600, upper=True, name=f'Titre · {text}')

    def header(self, title, accent, band='#10141b', h=60, clock=True, status=True):
        self.rect((0, 0, self.w, h), band, 'Bandeau')
        self.rect((0, 0, 6, h), accent, 'Accent')
        right = self.w - 16
        if clock:
            self.add('timecode', 'Heure station', (right - 340, 6, 340, h - 12), color=accent)
            right -= 356
        if status:
            self.add('status', 'État', (right - 400, 6, 400, h - 12))
            right -= 416
        self.text((24, 6, right - 24, h - 12), title, size=round(h / 2))

    # Broadcast widgets.
    def source(self, n, box, umd='', tc='off', loud=False, vu=2, first=0, markers=None, border=4, badge=False, umd_pos='bottom', umd_size=11, vu_pos='right', look='bars', fit='contain', name=None):
        return self.add(
            'source', name or (f'Source {n}' if n else 'Source'), box,
            input=inp(n), border=float(border), timecode=tc, loudness=loud, badge=badge, noSignalLook=look, fit=fit,
            umd={'text': umd, 'position': umd_pos, 'size': float(umd_size)},
            vu={'max': vu, 'first': first, 'position': vu_pos},
            markers=markers or {},
        )

    def meter(self, n, box, channels=16, labels=SURROUND, values=False, loudness=True, name='Vu-mètres'):
        return self.add('audioMeter', name, box, input=inp(n), channels=channels, labels=labels, values=values, loudness=loudness)

    def loudness(self, n, box, target=-23.0, history=True, name='Loudness'):
        return self.add('loudness', name, box, input=inp(n), target=target, history=history)

    def alarms(self, box, rows=6, name='Journal des alarmes'):
        return self.add('alarmLog', name, box, rows=rows)

    def scope(self, n, box, kind, color='#6dff8f', labels=True, name=None):
        names = {'waveform': 'Waveform', 'parade': 'Parade RVB', 'ycbcrParade': "Parade Y'CbCr", 'vector': 'Vecteurscope', 'histogram': 'Histogramme', 'cie': 'CIE 1931', 'falseColor': 'Fausses couleurs', 'goniometer': 'Goniomètre'}
        return self.add('scope', name or names[kind], box, input=inp(n), kind=kind, color=color, labels=labels)

    def signal(self, n, box, video=True, color=True, hdr=True, audio=True, name='Fiche signal'):
        return self.add('signalInfo', name, box, input=inp(n), video=video, color=color, hdr=hdr, audio=audio)

    def chart(self, metric, box, n=0, title='', color='#4ed8ff', warn=0, crit=0, window=120, mn=0, mx=0, stats=True, name=None):
        return self.add('chart', name or f'Courbe · {title or metric}', box, metric=metric, input=inp(n), title=title, color=color, warn=float(warn), crit=float(crit), window=window, min=float(mn), max=float(mx), stats=stats)

    def value(self, kind, box, n=0, label='', color='#ffffff', warn=0, crit=0, stack=False, align='center', show=True, bg='#0b0d12d9', name=None):
        return self.add('value', name or f'Valeur · {label or kind}', box, input=inp(n), kind=kind, label=label, color=color, warn=float(warn), crit=float(crit), stack=stack, align=align, showLabel=show, background=bg)

    def values(self, box, items, gap=12, vertical=False, **kw):
        """A row (or a column) of values: items = [(kind, input, label), …]."""
        for (kind, n, label), cell in zip(items, (col if vertical else row)(box, len(items), gap)):
            self.value(kind, cell, n=n, label=label, **kw)

    def check(self):
        """Every layer in the picture; no two widgets over each other."""
        boxes = []
        for l in self.layers:
            p = l['props']
            x, y, w, h = p['x'], p['y'], p['width'], p['height']
            if x < 0 or y < 0 or x + w > self.w + 0.5 or y + h > self.h + 0.5 or w <= 0 or h <= 0:
                sys.exit(f'{self.slug}: “{l["name"]}” out of the picture ({x}, {y}, {w}, {h})')
            if l['type'] in LAYOUT_ONLY:
                continue
            for name, (a, b, c, d) in boxes:
                if x < a + c - 0.5 and a < x + w - 0.5 and y < b + d - 0.5 and b < y + h - 0.5:
                    sys.exit(f'{self.slug}: “{l["name"]}” over “{name}”')
            boxes.append((l['name'], (x, y, w, h)))


def row(box, n, gap=12):
    x, y, w, h = box
    cw = (w - gap * (n - 1)) / n
    return [(x + i * (cw + gap), y, cw, h) for i in range(n)]


def col(box, n, gap=12):
    x, y, w, h = box
    ch = (h - gap * (n - 1)) / n
    return [(x, y + i * (ch + gap), w, ch) for i in range(n)]


def grid(box, cols, rows, gap=12):
    return [c for r in col(box, rows, gap) for c in row(r, cols, gap)]


def split(box, at, gap=12, vertical=False):
    """Two boxes: the first `at` px wide (or high)."""
    x, y, w, h = box
    if vertical:
        return (x, y, w, at), (x, y + at + gap, w, h - at - gap)
    return (x, y, at, h), (x + at + gap, y, w - at - gap, h)


def tile16x9(box):
    """The widest 16:9 box at the top left of `box`."""
    x, y, w, h = box
    tw = min(w, h * 16 / 9)
    return (x, y, tw, tw * 9 / 16)


SAFE = {'action': True, 'title': True}
EXAMPLES = []


def example(id_, name, category, tags, summary, description, en, background='#0b0d12', grad=None, w=1920, h=1080, rate=(50, 1)):
    def wrap(build):
        page = Page(id_, w, h)
        build(page)
        page.check()
        EXAMPLES.append((id_, name, category, tags, summary, description, en, background, grad, page, rate))
        return build

    return wrap


# ─── 1. Final control ────────────────────────────────────────────────────────
@example(
    'ex-01-final', 'Régie finale d’une chaîne', 'Régie finale', ['8 sources', 'Repères', 'Fiche signal'],
    'PGM avec zones de sécurité, PVW, deux playouts, quatre retours, 16 canaux, loudness et fiche signal.',
    'Le contrôle final d’une chaîne : le programme en grand avec ses zones de sécurité, son timecode et son loudness ; la préparation, les deux playouts et quatre retours ; les mesures clés en valeurs (loudness intégré, true peak, format, pire alarme), la fiche signal du programme, ses 16 canaux audio, son loudness EBU R128 et le journal des alarmes.',
    {'name': 'Final control of a channel', 'category': 'Final control', 'tags': ['8 sources', 'Safe areas', 'Signal sheet'],
     'summary': 'PGM with safe areas, PVW, two playouts, four returns, 16 channels, loudness and signal sheet.',
     'description': 'The final control of a channel: the programme large with its safe areas, timecode and loudness; preview, both playouts and four returns; the key measures as values (integrated loudness, true peak, format, worst fault), the programme’s signal sheet, its 16 audio channels, its EBU R128 loudness and the alarm log.'},
)
def _(p):
    p.header('Régie finale · Canal 1', '#ff3b5c')
    p.source(1, (16, 72, 1024, 576), 'PGM', tc='station', loud=True, vu=0, markers=SAFE)
    p.values((16, 660, 1024, 64), [('integrated', 1, 'Loudness I'), ('truePeak', 1, 'True peak'), ('format', 1, ''), ('alarm', 1, 'PGM')])
    for i, (cell, umd) in enumerate(zip(row((16, 736, 1024, 144), 4), ['RETOUR SAT', 'RETOUR TNT', 'RETOUR IPTV', 'RETOUR OTT'])):
        p.source(i + 5, cell, umd)
    p.alarms((16, 892, 1024, 172), rows=5)
    for i, (cell, umd) in enumerate(zip(col((1052, 72, 432, 760), 3), ['PVW', 'PLAYOUT A', 'PLAYOUT B'])):
        p.source(i + 2, cell, umd)
    p.values((1052, 844, 432, 100), [('ptp', 0, 'Synchro'), ('stationTimecode', 0, 'TC station')], stack=True)
    p.values((1052, 956, 432, 108), [('codec', 1, ''), ('dynamic', 1, '')], stack=True)
    p.signal(1, (1496, 72, 408, 420), hdr=False)
    p.meter(1, (1496, 504, 408, 352))
    p.loudness(1, (1496, 868, 408, 196))


# ─── 2. Sixteen sources ──────────────────────────────────────────────────────
@example(
    'ex-02-sixteen', 'Mur de 16 sources', 'Mosaïque', ['16 sources', 'Valeurs'],
    'Seize sources en 4 × 4 avec UMD, vu-mètres et alarmes, et une barre d’état de l’installation.',
    'La mosaïque classique : seize sources de même taille, chacune avec son UMD, son tally, son vu-mètre et ses alarmes. En bas, une barre d’état : alarmes en cours, charge des processeurs et de la mémoire, synchro PTP et timecode station.',
    {'name': 'Wall of 16 sources', 'category': 'Mosaic', 'tags': ['16 sources', 'Values'],
     'summary': 'Sixteen sources in 4 × 4 with UMD, meters and alarms, and a status bar of the plant.',
     'description': 'The classic mosaic: sixteen sources of the same size, each with its UMD, tally, meter and alarms. At the bottom, a status bar: faults on, processor and memory load, PTP sync and house timecode.'},
)
def _(p):
    p.header('Mosaïque · 16 sources', '#4ed8ff')
    for i, cell in enumerate(grid((16, 72, 1888, 920), 4, 4, 8)):
        p.source(i + 1, cell, f'SOURCE {i + 1:02}')
    p.values((16, 1004, 1888, 60), [('alarmsOn', 0, 'Alarmes'), ('cpu', 0, 'CPU'), ('memory', 0, 'Mémoire'), ('ptp', 0, 'PTP'), ('ptpOffset', 0, 'Écart PTP'), ('stationTimecode', 0, 'TC station')], gap=8)


# ─── 3. Colour control ───────────────────────────────────────────────────────
@example(
    'ex-03-colour', 'Contrôle image et colorimétrie', 'Vision', ['Scopes', 'Waveform', 'Vecteurscope', 'CIE'],
    'Une source en grand et ses six outils de mesure : waveform, parades, vecteurscope, histogramme, CIE.',
    'Le poste de l’ingénieur vision : la source contrôlée en grand avec sa croix de centre, et sous elle sa waveform, sa parade RVB, sa parade Y’CbCr, son vecteurscope, son histogramme et son diagramme CIE 1931 face aux gamuts. À droite, la fiche signal (format, colorimétrie, niveaux) et les valeurs des niveaux légaux.',
    {'name': 'Picture and colour control', 'category': 'Vision', 'tags': ['Scopes', 'Waveform', 'Vectorscope', 'CIE'],
     'summary': 'A source large and its six measuring tools: waveform, parades, vectorscope, histogram, CIE.',
     'description': 'The vision engineer’s desk: the source checked large with its centre cross, and below it its waveform, RGB parade, Y’CbCr parade, vectorscope, histogram and CIE 1931 diagram against the gamuts. On the right, the signal sheet (format, colour, levels) and the legal levels as values.'},
)
def _(p):
    p.header('Vision · contrôle colorimétrique', '#a594ff')
    p.source(1, (16, 72, 1072, 603), 'CAM 1', tc='input', vu=0, markers={'centre': True})
    sc = row((16, 687, 1072, 377), 3)
    p.scope(1, (sc[0][0], sc[0][1], sc[0][2], 182), 'waveform')
    p.scope(1, (sc[0][0], sc[0][1] + 195, sc[0][2], 182), 'histogram', color='#ffffff')
    p.scope(1, (sc[1][0], sc[1][1], sc[1][2], 182), 'parade')
    p.scope(1, (sc[1][0], sc[1][1] + 195, sc[1][2], 182), 'ycbcrParade')
    p.scope(1, sc[2], 'vector', color='#ffe066')
    p.scope(1, (1100, 72, 400, 400), 'cie')
    p.source(2, (1512, 72, 392, 220), 'CAM 2', vu=0)
    p.source(3, (1512, 304, 392, 168), 'CAM 3', vu=0)
    p.signal(1, (1100, 484, 400, 580), hdr=False, audio=False)
    for cell, (kind, label) in zip(col((1512, 484, 392, 580), 8, 8), [('levels', 'Niveaux'), ('colorSpace', ''), ('dynamic', ''), ('picture', ''), ('format', ''), ('codec', ''), ('timecode', 'TC'), ('alarm', 'CAM 1')]):
        p.value(kind, cell, n=1, label=label, align='left')


# ─── 4. HDR ──────────────────────────────────────────────────────────────────
@example(
    'ex-04-hdr', 'Contrôle HDR / SDR', 'Vision', ['HDR', 'Fausses couleurs', 'CIE', 'Fiche signal'],
    'Les versions HDR et SDR côte à côte, fausses couleurs, CIE, parade et métadonnées HDR.',
    'Pour une production en HDR avec une version SDR : les deux programmes côte à côte, leurs fausses couleurs pour juger l’exposition, le diagramme CIE pour vérifier que le BT.2020 reste dans son gamut, la parade Y’CbCr et les fiches signal avec les métadonnées HDR (PQ / HLG, MaxCLL, MaxFALL, mastering display).',
    {'name': 'HDR / SDR control', 'category': 'Vision', 'tags': ['HDR', 'False colour', 'CIE', 'Signal sheet'],
     'summary': 'HDR and SDR versions side by side, false colour, CIE, parade and HDR metadata.',
     'description': 'For a production in HDR with an SDR version: both programmes side by side, their false colour to judge exposure, the CIE diagram to check BT.2020 stays in its gamut, the Y’CbCr parade and the signal sheets with the HDR metadata (PQ / HLG, MaxCLL, MaxFALL, mastering display).'},
    background='#08090c',
)
def _(p):
    p.header('HDR · PQ / HLG et version SDR', '#ffd84e')
    left, right = (16, 72, 938, 528), (966, 72, 938, 528)
    p.source(1, left, 'HDR · BT.2100', vu=0)
    p.source(2, right, 'SDR · BT.709', vu=0)
    p.scope(1, (16, 612, 300, 220), 'falseColor', name='Fausses couleurs HDR')
    p.scope(1, (328, 612, 300, 220), 'cie', name='CIE HDR')
    p.scope(1, (640, 612, 314, 220), 'ycbcrParade', name="Parade Y'CbCr HDR")
    p.scope(2, (966, 612, 300, 220), 'falseColor', name='Fausses couleurs SDR')
    p.scope(2, (1278, 612, 300, 220), 'cie', name='CIE SDR')
    p.scope(2, (1590, 612, 314, 220), 'waveform', name='Waveform SDR')
    p.signal(1, (16, 844, 616, 220), audio=False, name='Fiche HDR')
    p.values((644, 844, 310, 220), [('dynamic', 1, ''), ('colorSpace', 1, '')], stack=True)
    p.signal(2, (966, 844, 616, 220), audio=False, hdr=False, name='Fiche SDR')
    p.values((1594, 844, 310, 220), [('dynamic', 2, ''), ('levels', 2, '')], stack=True)


# ─── 5. Audio ────────────────────────────────────────────────────────────────
@example(
    'ex-05-audio', 'Console audio 5.1', 'Son', ['16 canaux', 'Goniomètre', 'Loudness', 'Courbe'],
    '16 canaux, loudness R128, courbe du loudness court terme, goniomètre et toutes les valeurs.',
    'Le contrôle du son d’un programme : ses 16 canaux (5.1, stéréo, audiodescription, version originale), son loudness EBU R128 et sa courbe sur deux minutes, le goniomètre et la corrélation de phase de la stéréo, et chaque mesure en valeur (M, S, I, LRA, true peak, crêtes gauche et droite, phase, piste écoutée).',
    {'name': '5.1 audio console', 'category': 'Sound', 'tags': ['16 channels', 'Goniometer', 'Loudness', 'Chart'],
     'summary': '16 channels, R128 loudness, short-term loudness chart, goniometer and every value.',
     'description': 'Checking the sound of a programme: its 16 channels (5.1, stereo, audio description, original version), its EBU R128 loudness and its curve over two minutes, the goniometer and phase correlation of the stereo, and each measure as a value (M, S, I, LRA, true peak, left and right peaks, phase, track listened to).'},
)
def _(p):
    p.header('Son · programme 5.1', '#3ddc97')
    p.meter(1, (16, 72, 760, 680), values=True)
    p.source(1, (788, 72, 560, 315), 'PGM', vu=0)
    p.scope(1, (1360, 72, 544, 452), 'goniometer', color='#3ddc97')
    p.loudness(1, (788, 399, 560, 353))
    p.chart('loudness', (1360, 536, 544, 216), n=1, title='Loudness court terme', color='#3ddc97', warn=-22, crit=-20, mn=-40, mx=-10)
    items = [('momentary', 'M'), ('shortTerm', 'S'), ('integrated', 'I'), ('loudnessRange', 'LRA'), ('truePeak', 'TP'), ('peakLeft', ''), ('peakRight', ''), ('phase', ''), ('audio', '')]
    cells = grid((16, 764, 1888, 300), 5, 2)
    for (kind, label), cell in zip(items, cells):
        p.value(kind, cell, n=1, label=label, stack=True)
    p.value('alarm', cells[-1], n=1, label='Son', stack=True)


# ─── 6. Dante ────────────────────────────────────────────────────────────────
@example(
    'ex-06-dante', 'Supervision Dante / AES67', 'Son', ['Dante', 'AES67', 'Audio seul', 'Goniomètres'],
    'Quatre flux Dante / AES67 de 8 canaux : vu-mètres, goniomètres, loudness et alarmes.',
    'Pour un réseau audio sur IP : quatre flux Dante ou AES67 de 8 canaux (des entrées audio seules, découvertes par SAP), chacun avec ses vu-mètres, son goniomètre et son loudness court terme. Les tuiles des flux montrent leur nom et leur état ; le journal garde les silences et les pertes de flux.',
    {'name': 'Dante / AES67 supervision', 'category': 'Sound', 'tags': ['Dante', 'AES67', 'Audio only', 'Goniometers'],
     'summary': 'Four 8-channel Dante / AES67 flows: meters, goniometers, loudness and alarms.',
     'description': 'For an audio over IP network: four 8-channel Dante or AES67 flows (audio-only inputs, found by SAP), each with its meters, goniometer and short-term loudness. The flows’ tiles show their name and state; the log keeps silences and lost flows.'},
)
def _(p):
    p.header('Dante · AES67 · 4 flux', '#ff8a3d')
    labels = '1, 2, 3, 4, 5, 6, 7, 8'
    for i, box in enumerate(grid((16, 72, 1888, 800), 2, 2)):
        n = i + 1
        x, y, w, h = box
        p.rect(box, '#141922', f'Cadre flux {n}', radius=8)
        p.source(n, (x + 12, y + 12, 288, 162), f'FLUX {n}', vu=0)
        p.scope(n, (x + 12, y + 186, 288, 196), 'goniometer', color='#ff8a3d', name=f'Goniomètre {n}')
        p.meter(n, (x + 312, y + 12, 330, h - 24), channels=8, labels=labels, values=True, name=f'Vu-mètres {n}')
        p.chart('loudness', (x + 654, y + 12, w - 666, 180), n=n, title=f'Flux {n}', color='#ff8a3d', mn=-50, mx=-10, stats=False, name=f'Loudness {n}')
        p.value('audio', (x + 654, y + 204, w - 666, 82), n=n, label='Flux', align='left')
        p.value('alarm', (x + 654, y + 298, w - 666, h - 310), n=n, label='État', align='left')
    p.alarms((16, 884, 1888, 180), rows=4)


# ─── 7. SRT contributions ────────────────────────────────────────────────────
@example(
    'ex-07-srt', 'Contributions SRT', 'Réseau', ['SRT', 'RTT', 'Pertes', 'Courbes'],
    'Six arrivées SRT avec leur RTT et leurs pertes en courbes, et le journal des alarmes.',
    'Les arrivées des reporters et des sites distants en SRT : six sources, chacune avec deux courbes sur deux minutes, le temps aller-retour (RTT) et le taux de pertes, avec leurs seuils d’alerte. L’alarme « pertes SRT » se déclenche sur les paquets perdus non récupérés ; le journal garde l’historique.',
    {'name': 'SRT contributions', 'category': 'Network', 'tags': ['SRT', 'RTT', 'Loss', 'Charts'],
     'summary': 'Six SRT feeds with their RTT and loss as charts, and the alarm log.',
     'description': 'Feeds from reporters and remote sites over SRT: six sources, each with two charts over two minutes, the round trip time (RTT) and the loss rate, with their warning thresholds. The “SRT loss” alarm fires on packets lost and not recovered; the log keeps the history.'},
)
def _(p):
    p.header('Contributions · SRT', '#4ed8ff')
    names = ['REPORTER 1', 'REPORTER 2', 'STADE', 'PARIS', 'LYON', 'BRUXELLES']
    for i, box in enumerate(grid((16, 72, 1888, 800), 3, 2)):
        x, y, w, h = box
        n = i + 1
        p.source(n, (x, y, w, 290), names[i])
        a, b = row((x, y + 302, w, h - 302), 2, 8)
        p.chart('srtRtt', a, n=n, title='RTT', color='#4ed8ff', warn=120, crit=250, stats=False, name=f'RTT {names[i]}')
        p.chart('srtLoss', b, n=n, title='Pertes', color='#ff6b6b', warn=1, crit=5, stats=False, name=f'Pertes {names[i]}')
    p.alarms((16, 884, 1240, 180), rows=4)
    p.values((1268, 884, 636, 180), [('alarmsOn', 0, 'Alarmes'), ('ntpOffset', 0, 'Écart NTP')], stack=True)


# ─── 8. Redundancy ───────────────────────────────────────────────────────────
@example(
    'ex-08-redundancy', 'Redondance principal / secours', 'Régie finale', ['Secours', 'Comparaison', '12 sources'],
    'Six chaînes en double, principal et secours côte à côte, avec la pire alarme de chacun.',
    'Pour vérifier qu’un secours est prêt : six chaînes, chacune avec son flux principal et son flux de secours côte à côte, la pire alarme de chacun et leur format. Une entrée avec un secours configuré bascule seule ; sa tuile l’indique (badge SECOURS).',
    {'name': 'Main / backup redundancy', 'category': 'Final control', 'tags': ['Backup', 'Comparison', '12 sources'],
     'summary': 'Six channels twice, main and backup side by side, with the worst fault of each.',
     'description': 'To check a backup is ready: six channels, each with its main and backup flows side by side, the worst fault of each and their format. An input with a backup set switches on its own; its tile shows it (BACKUP badge).'},
)
def _(p):
    p.header('Redondance · principal / secours', '#f2b705')
    for i, box in enumerate(grid((16, 72, 1888, 992), 2, 3, 16)):
        x, y, w, h = box
        p.rect(box, '#141922', f'Cadre chaîne {i + 1}', radius=8)
        p.text((x + 14, y + 6, w - 28, 30), f'Chaîne {i + 1}', size=18, name=f'Chaîne {i + 1}')
        a, b = row((x + 12, y + 40, w - 24, 210), 2, 10)
        p.source(2 * i + 1, a, 'PRINCIPAL', badge=True)
        p.source(2 * i + 2, b, 'SECOURS', badge=True)
        va, vb = row((x + 12, y + 260, w - 24, h - 272), 2, 10)
        p.values(va, [('alarm', 2 * i + 1, 'Principal'), ('format', 2 * i + 1, '')], gap=6)
        p.values(vb, [('alarm', 2 * i + 2, 'Secours'), ('format', 2 * i + 2, '')], gap=6)


# ─── 9. Sync ─────────────────────────────────────────────────────────────────
@example(
    'ex-09-sync', 'Synchro et infrastructure ST 2110', 'Réseau', ['PTP', 'NTP', 'ST 2110', 'Courbes'],
    'L’écart PTP et NTP en courbes, l’état de la synchro, quatre flux ST 2110 et leurs formats.',
    'Pour une installation SMPTE ST 2110 : l’écart au grand maître PTP et l’écart NTP en courbes sur deux minutes, avec leurs seuils, l’état de la synchro, les alarmes en cours et quatre flux avec leur format et leur pire alarme. La charge du serveur reste sous les yeux.',
    {'name': 'Sync and ST 2110 plant', 'category': 'Network', 'tags': ['PTP', 'NTP', 'ST 2110', 'Charts'],
     'summary': 'PTP and NTP offset as charts, sync state, four ST 2110 flows and their formats.',
     'description': 'For a SMPTE ST 2110 plant: the offset to the PTP grandmaster and the NTP offset as charts over two minutes, with their thresholds, the state of sync, the faults on and four flows with their format and worst fault. The load of the server stays in sight.'},
)
def _(p):
    p.header('Synchro · PTP / NTP · ST 2110', '#3ddc97')
    p.chart('ptpOffset', (16, 72, 1240, 300), title='Écart PTP', color='#3ddc97', warn=1000, crit=10000)
    p.chart('ntpOffset', (16, 384, 1240, 220), title='Écart NTP', color='#4ed8ff', warn=5, crit=50)
    for cell, (kind, label) in zip(grid((1268, 72, 636, 532), 2, 2), [('ptp', 'PTP'), ('ptpOffset', 'Écart PTP'), ('ntpOffset', 'Écart NTP'), ('alarmsOn', 'Alarmes')]):
        p.value(kind, cell, label=label, stack=True)
    for i, box in enumerate(row((16, 616, 1888, 300), 4)):
        x, y, w, h = box
        p.source(i + 1, (x, y, w, 250), f'ST 2110 · {i + 1}')
        p.values((x, y + 258, w, 42), [('format', i + 1, ''), ('alarm', i + 1, '')], gap=6, show=False)
    p.chart('cpu', (16, 928, 940, 136), title='Processeurs', color='#a594ff', warn=70, crit=90, mn=0, mx=100)
    p.chart('memory', (968, 928, 936, 136), title='Mémoire', color='#ff8a3d', warn=80, crit=95, mn=0, mx=100)


# ─── 10. UHD ─────────────────────────────────────────────────────────────────
@example(
    'ex-10-uhd', 'Contrôle UHD 2160p', 'Vision', ['UHD', '3840 × 2160', 'Waveform', 'Repères'],
    'Un multiview UHD : la source 4K en grand avec ses repères, waveform, vecteurscope et fiche signal.',
    'Un multiview en 3840 × 2160 pour un écran 4K : la source UHD en grand avec ses zones de sécurité et sa croix de centre, à côté de sa waveform, de son vecteurscope, de sa fiche signal et de ses vu-mètres, et trois sources HD en bas.',
    {'name': 'UHD 2160p control', 'category': 'Vision', 'tags': ['UHD', '3840 × 2160', 'Waveform', 'Markers'],
     'summary': 'A UHD multiview: the 4K source large with its markers, waveform, vectorscope and signal sheet.',
     'description': 'A 3840 × 2160 multiview for a 4K screen: the UHD source large with its safe areas and centre cross, next to its waveform, vectorscope, signal sheet and meters, and three HD sources at the bottom.'},
    w=3840, h=2160,
)
def _(p):
    p.header('UHD · 2160p50', '#ff3b5c', h=110)
    p.source(1, (32, 140, 2560, 1440), 'UHD PGM', tc='station', loud=True, vu=0, markers={'action': True, 'title': True, 'centre': True}, border=6)
    p.scope(1, (2624, 140, 1184, 560), 'waveform')
    p.scope(1, (2624, 724, 576, 576), 'vector', color='#ffe066')
    p.scope(1, (3224, 724, 584, 576), 'histogram', color='#ffffff')
    p.signal(1, (2624, 1324, 1184, 804))
    for i, cell in enumerate(row((32, 1604, 2560, 524), 3, 24)):
        p.source(i + 2, tile16x9(cell), f'HD {i + 1}')


# ─── 11. Newsroom ────────────────────────────────────────────────────────────
@example(
    'ex-11-news', 'Régie de journal télévisé', 'Production', ['9 sources', '4:3 / 14:9', 'Duplex'],
    'Programme avec repères 4:3 et 14:9, préparation, duplex, plateau, sujets et retour antenne.',
    'Pour le journal : le programme avec les repères 4:3 et 14:9 et la zone de titre pour placer les synthés, la préparation, deux duplex, trois caméras plateau et deux serveurs de sujets. En bas, l’heure, le loudness du programme et le journal des alarmes.',
    {'name': 'News control room', 'category': 'Production', 'tags': ['9 sources', '4:3 / 14:9', 'Live links'],
     'summary': 'Programme with 4:3 and 14:9 markers, preview, live links, studio, packages and air return.',
     'description': 'For the news: the programme with the 4:3 and 14:9 markers and the title area to place the graphics, preview, two live links, three studio cameras and two package servers. At the bottom, the time, the programme’s loudness and the alarm log.'},
)
def _(p):
    p.header('Journal · 20 h', '#ff3b5c')
    p.source(1, (16, 72, 928, 522), 'PGM', tc='station', loud=True, vu=0, markers={'title': True, 'fourThree': True, 'fourteenNine': True, 'color': '#ffd84ecc'})
    p.source(2, (956, 72, 928, 522), 'PVW', vu=0)
    names = ['DUPLEX 1', 'DUPLEX 2', 'CAM 1', 'CAM 2', 'CAM 3', 'SUJETS A', 'SUJETS B']
    for i, cell in enumerate(row((16, 606, 1888, 200), 7, 10)):
        p.source(i + 3, tile16x9(cell), names[i])
    p.value('time', (16, 818, 360, 246), label='Heure', stack=True, color='#ffd84e')
    p.loudness(1, (388, 818, 556, 246), history=True)
    p.alarms((956, 818, 948, 246), rows=6)


# ─── 12. Sport OB ────────────────────────────────────────────────────────────
@example(
    'ex-12-sport', 'Car régie sport', 'Production', ['14 sources', 'Ralentis', 'Tally'],
    'Programme et préparation, dix caméras, deux ralentis : une régie de match avec tally.',
    'Une régie mobile pour un match : programme et préparation en grand, dix caméras et deux serveurs de ralentis avec leur tally rouge et vert pilotés par le mélangeur, le score et le chronomètre en valeurs, et le loudness du programme.',
    {'name': 'Sport OB truck', 'category': 'Production', 'tags': ['14 sources', 'Replays', 'Tally'],
     'summary': 'Programme and preview, ten cameras, two replays: a match control room with tally.',
     'description': 'An outside broadcast for a match: programme and preview large, ten cameras and two replay servers with their red and green tally driven by the switcher, the score and clock as values, and the programme’s loudness.'},
    background='#0a0f0b',
)
def _(p):
    p.header('Car régie · match', '#17b14a')
    p.source(1, (16, 72, 760, 428), 'PGM', tc='station', loud=True, vu=0, markers={'title': True})
    p.source(2, (788, 72, 760, 428), 'PVW', vu=0)
    p.values((1560, 72, 344, 428), [('timecode', 1, 'TC programme'), ('integrated', 1, 'Loudness I'), ('truePeak', 1, 'True peak'), ('tally', 1, 'PGM')], stack=True, vertical=True)
    cams = [f'CAM {i}' for i in range(1, 11)] + ['RALENTI A', 'RALENTI B']
    for i, cell in enumerate(grid((16, 512, 1888, 552), 6, 2, 10)):
        p.source(i + 3, cell, cams[i])


# ─── 13. Studio ──────────────────────────────────────────────────────────────
@example(
    'ex-13-studio', 'Plateau et prompteur', 'Production', ['Plateau', 'Prompteur', 'Tally', 'UMD en haut'],
    'Six caméras plateau avec tally et UMD en haut, prompteur, programme et retour antenne.',
    'Pour l’écran du plateau : six caméras avec leur tally bien visible et leur nom en haut, le prompteur, le programme et le retour antenne, avec l’heure en grand pour le présentateur.',
    {'name': 'Studio floor and prompter', 'category': 'Production', 'tags': ['Studio', 'Prompter', 'Tally', 'UMD on top'],
     'summary': 'Six studio cameras with tally and UMD on top, prompter, programme and air return.',
     'description': 'For the studio floor screen: six cameras with their tally clearly visible and their name on top, the prompter, the programme and the air return, with the time large for the presenter.'},
)
def _(p):
    p.header('Plateau B', '#e5212b', clock=False)
    p.source(1, (16, 72, 1120, 630), 'PROGRAMME', tc='station', vu=0, umd_pos='top', border=8)
    p.source(2, (1148, 72, 756, 425), 'PROMPTEUR', vu=0, umd_pos='top')
    p.value('time', (1148, 509, 756, 193), label='', show=False, color='#ffffff')
    for i, cell in enumerate(grid((16, 714, 1120, 350), 3, 2, 12)):
        p.source(3 + i, cell, f'CAM {i + 1}', umd_pos='top', border=8)
    p.source(9, (1148, 714, 756, 350), 'RETOUR ANTENNE', tc='station', umd_pos='top', border=8)


# ─── 14. QC ──────────────────────────────────────────────────────────────────
@example(
    'ex-14-qc', 'Contrôle qualité (QC)', 'Vision', ['QC', 'Niveaux', 'Loudness', 'Fiche signal'],
    'Tout ce qu’on vérifie avant diffusion : image, waveform, histogramme, niveaux, loudness et fiche.',
    'Le poste de contrôle qualité avant diffusion : la source avec ses zones de sécurité et son code AFD, sa waveform, son histogramme et ses niveaux (EBU R103), son loudness intégré et sa courbe, sa fiche signal complète (vidéo, couleur, HDR, audio) et toutes les valeurs à cocher.',
    {'name': 'Quality control (QC)', 'category': 'Vision', 'tags': ['QC', 'Levels', 'Loudness', 'Signal sheet'],
     'summary': 'Everything checked before air: picture, waveform, histogram, levels, loudness and sheet.',
     'description': 'The quality control desk before air: the source with its safe areas and AFD code, its waveform, histogram and levels (EBU R103), its integrated loudness and curve, its full signal sheet (video, colour, HDR, audio) and every value to tick.'},
)
def _(p):
    p.header('QC · contrôle avant diffusion', '#a594ff')
    p.source(1, (16, 72, 1024, 576), 'QC', tc='input', loud=True, vu=0, markers={'action': True, 'title': True, 'afd': True})
    p.scope(1, (16, 660, 506, 220), 'waveform')
    p.scope(1, (534, 660, 506, 220), 'histogram', color='#ffffff')
    p.chart('loudness', (16, 892, 1024, 172), n=1, title='Loudness court terme', color='#3ddc97', warn=-22, crit=-20, mn=-40, mx=-10)
    p.signal(1, (1052, 72, 460, 992))
    items = [('codec', ''), ('picture', ''), ('format', ''), ('colorSpace', ''), ('dynamic', ''), ('levels', 'Niveaux R103'), ('integrated', 'Loudness I'), ('loudnessRange', 'LRA'), ('truePeak', 'True peak'), ('phase', ''), ('audio', ''), ('alarm', 'Pire alarme')]
    for (kind, label), cell in zip(items, col((1524, 72, 380, 992), 12, 8)):
        p.value(kind, cell, n=1, label=label, align='left')


# ─── 15. OTT ─────────────────────────────────────────────────────────────────
@example(
    'ex-15-ott', 'Diffusion OTT et plateformes', 'Réseau', ['YouTube', 'HLS', 'Images / s', 'Courbes'],
    'Le programme et ses sorties en ligne (YouTube, Twitch, HLS, Facebook) avec leurs images par seconde.',
    'Pour suivre une diffusion en ligne : le programme et ses quatre versions publiées (YouTube Live, Twitch, un flux HLS et Facebook Live) reprises en entrée, chacune avec sa courbe d’images par seconde et son format. Un flux qui ralentit ou se fige se voit tout de suite.',
    {'name': 'OTT and platforms', 'category': 'Network', 'tags': ['YouTube', 'HLS', 'Frames / s', 'Charts'],
     'summary': 'The programme and its online outputs (YouTube, Twitch, HLS, Facebook) with their frame rates.',
     'description': 'To follow an online broadcast: the programme and its four published versions (YouTube Live, Twitch, an HLS stream and Facebook Live) taken back as inputs, each with its frames a second as a chart and its format. A stream slowing or freezing shows at once.'},
)
def _(p):
    p.header('OTT · plateformes', '#ff3b5c')
    p.source(1, (16, 72, 928, 522), 'PGM', tc='station', loud=True, vu=0)
    p.chart('inputFps', (956, 72, 948, 255), n=1, title='Programme · images / s', color='#3ddc97', crit=20, mn=0, mx=60)
    p.chart('renderTime', (956, 339, 948, 255), title='Temps de rendu', color='#a594ff')
    names = ['YOUTUBE LIVE', 'TWITCH', 'HLS', 'FACEBOOK LIVE']
    for i, cell in enumerate(row((16, 606, 1888, 458), 4)):
        x, y, w, h = cell
        p.source(i + 2, (x, y, w, 260), names[i])
        p.chart('inputFps', (x, y + 272, w, 120), n=i + 2, title='Images / s', color='#4ed8ff', crit=20, mn=0, mx=60, stats=False, name=f'Images / s {names[i]}')
        p.value('format', (x, y + 404, w, h - 404), n=i + 2, show=False)


# ─── 16. System ──────────────────────────────────────────────────────────────
@example(
    'ex-16-system', 'Tableau de bord du serveur', 'Supervision', ['CPU', 'Mémoire', 'Rendu', 'Alarmes'],
    'La santé de l’installation : processeurs, mémoire, décodeurs, rendu, alarmes et quatre sources.',
    'Pour le centre de supervision : la charge des processeurs (totale, de Strata Multiview et des décodeurs ffmpeg), la mémoire et le temps de rendu en courbes, les valeurs clés et le journal des alarmes en grand, avec quatre sources pour garder un œil sur l’antenne.',
    {'name': 'Server dashboard', 'category': 'Supervision', 'tags': ['CPU', 'Memory', 'Render', 'Alarms'],
     'summary': 'The health of the plant: processors, memory, decoders, render, alarms and four sources.',
     'description': 'For the network operations centre: processor load (total, of Strata Multiview and of the ffmpeg decoders), memory and render time as charts, the key values and the alarm log large, with four sources to keep an eye on air.'},
    background='#0d1016',
)
def _(p):
    p.header('Supervision · serveur', '#4ed8ff')
    charts = [('cpu', 'Processeurs', '#a594ff', 70, 90), ('serverCpu', 'Strata Multiview', '#4ed8ff', 60, 85), ('ffmpegCpu', 'Décodeurs ffmpeg', '#ff8a3d', 60, 85), ('memory', 'Mémoire', '#3ddc97', 80, 95)]
    for (metric, title, color, warn, crit), cell in zip(charts, grid((16, 72, 1240, 520), 2, 2)):
        p.chart(metric, cell, title=title, color=color, warn=warn, crit=crit, mn=0, mx=100)
    p.chart('renderTime', (16, 604, 1240, 200), title='Temps de rendu d’une image', color='#ffd84e')
    p.values((1268, 72, 636, 160), [('cpu', 0, 'CPU'), ('memory', 0, 'Mémoire'), ('alarmsOn', 0, 'Alarmes')], stack=True)
    for i, cell in enumerate(grid((1268, 244, 636, 360), 2, 2, 12)):
        p.source(i + 1, cell, f'ANTENNE {i + 1}')
    p.value('date', (1268, 616, 310, 188), label='Date', stack=True)
    p.value('time', (1590, 616, 314, 188), label='Heure', stack=True)
    p.alarms((16, 816, 1888, 248), rows=6)


# ─── 17. Visual radio ────────────────────────────────────────────────────────
@example(
    'ex-17-radio', 'Radio filmée', 'Son', ['Radio', 'Stéréo', 'Goniomètre', 'Loudness'],
    'Trois caméras de studio radio, le programme stéréo, son goniomètre, son loudness et ses crêtes.',
    'Pour une matinale radio filmée : le programme vidéo et trois caméras du studio, le son stéréo avec ses vu-mètres et ses valeurs crête, le goniomètre et la corrélation de phase, le loudness avec une cible à -23 LUFS et sa courbe, et l’heure pour les tops horaires.',
    {'name': 'Visual radio', 'category': 'Sound', 'tags': ['Radio', 'Stereo', 'Goniometer', 'Loudness'],
     'summary': 'Three radio studio cameras, the stereo programme, its goniometer, loudness and peaks.',
     'description': 'For a filmed radio morning show: the video programme and three studio cameras, the stereo sound with its meters and peak values, the goniometer and phase correlation, the loudness with a −23 LUFS target and its curve, and the time for the top of the hour.'},
)
def _(p):
    p.header('Radio filmée · matinale', '#ff8a3d', clock=False)
    p.source(1, (16, 72, 1104, 621), 'PROGRAMME', tc='station', vu=0)
    for i, cell in enumerate(row((16, 705, 1104, 200), 3)):
        p.source(i + 2, cell, ['CAM ANIMATEUR', 'CAM INVITÉS', 'CAM LARGE'][i])
    p.value('time', (1132, 72, 772, 180), show=False, color='#ff8a3d')
    p.meter(1, (1132, 264, 240, 641), channels=2, labels=STEREO, values=True)
    p.scope(1, (1384, 264, 520, 420), 'goniometer', color='#ff8a3d')
    p.values((1384, 696, 520, 209), [('peakLeft', 1, ''), ('peakRight', 1, ''), ('phase', 1, '')], stack=True)
    p.loudness(1, (16, 917, 940, 147), history=False)
    p.chart('loudness', (968, 917, 936, 147), n=1, title='Loudness', color='#ff8a3d', mn=-40, mx=-10, warn=-22, crit=-20)


# ─── 18. Wall of two screens ─────────────────────────────────────────────────
@example(
    'ex-18-wall', 'Mur de deux écrans', 'Mosaïque', ['Mur', '3840 × 1080', '20 sources'],
    'Un multiview de 3840 × 1080 découpé sur deux écrans : 20 sources et une barre d’état.',
    'Pour un mur d’écrans : un multiview de 3840 × 1080 que deux sorties se partagent (régions gauche et droite). Le programme et la préparation en grand au milieu, à cheval sur les deux écrans, et 16 sources autour, avec le journal des alarmes et l’état de l’installation.',
    {'name': 'Wall of two screens', 'category': 'Mosaic', 'tags': ['Wall', '3840 × 1080', '20 sources'],
     'summary': 'A 3840 × 1080 multiview cut over two screens: 20 sources and a status bar.',
     'description': 'For a video wall: a 3840 × 1080 multiview shared by two outputs (left and right regions). The programme and preview large in the middle, across both screens, and 16 sources around them, with the alarm log and the state of the plant.'},
    w=3840, h=1080,
)
def _(p):
    p.header('Mur · régie centrale', '#4ed8ff')
    for i, cell in enumerate(grid((16, 72, 1116, 992), 3, 4, 10)):
        p.source(i + 3, cell, f'SOURCE {i + 1:02}')
    p.source(1, (1144, 72, 768, 432), 'PGM', tc='station', loud=True, vu=0)
    p.source(2, (1928, 72, 768, 432), 'PVW', vu=0)
    p.meter(1, (1144, 516, 768, 300))
    p.loudness(1, (1928, 516, 768, 300))
    p.alarms((1144, 828, 1552, 236), rows=5)
    for i, cell in enumerate(grid((2708, 72, 1116, 992), 3, 4, 10)):
        p.source(i + 15, cell, f'SOURCE {i + 13:02}')


# ─── 19. Night watch ─────────────────────────────────────────────────────────
@example(
    'ex-19-night', 'Veille de nuit · 30 chaînes', 'Supervision', ['30 sources', 'Alarmes', 'Fond sobre'],
    '30 chaînes en petit pour la veille de nuit, alarmes visibles de loin, journal et état.',
    'Pour la veille de nuit d’une tête de réseau : trente chaînes en petit, sans vu-mètre, où seules les alarmes (noir, figé, silence, perte de signal) se voient de loin. À droite, le nombre d’alarmes en cours en grand, le journal et l’état de la synchro.',
    {'name': 'Night watch · 30 channels', 'category': 'Supervision', 'tags': ['30 sources', 'Alarms', 'Quiet background'],
     'summary': '30 channels small for the night watch, alarms visible from afar, log and state.',
     'description': 'For the night watch of a headend: thirty channels small, without meters, where only the faults (black, frozen, silence, lost signal) show from afar. On the right, the number of faults on large, the log and the state of sync.'},
    background='#050608',
)
def _(p):
    p.header('Veille de nuit · tête de réseau', '#8a93a3', band='#0b0d12')
    for i, cell in enumerate(grid((16, 72, 1480, 992), 5, 6, 8)):
        p.source(i + 1, cell, f'CH {i + 1:02}', vu=0, vu_pos='off', look='plain', umd_size=10, border=3)
    p.value('alarmsOn', (1508, 72, 396, 220), label='Alarmes en cours', stack=True, warn=1, crit=3)
    p.alarms((1508, 304, 396, 552), rows=12)
    p.values((1508, 868, 396, 196), [('ptp', 0, 'Synchro'), ('time', 0, 'Heure')], stack=True)


# ─── 20. Vertical ────────────────────────────────────────────────────────────
@example(
    'ex-20-vertical', 'Écran vertical réseaux sociaux', 'Réseau', ['Vertical', '1080 × 1920', 'TikTok', 'Instagram'],
    'Un multiview vertical en 1080 × 1920 pour suivre les directs TikTok, Instagram et YouTube Shorts.',
    'Un multiview en portrait pour un écran vertical : le direct vertical en grand (TikTok, Instagram, YouTube Shorts) avec ses zones de sécurité, deux autres directs à côté, le programme horizontal, le loudness et les images par seconde de chaque flux.',
    {'name': 'Vertical social media screen', 'category': 'Network', 'tags': ['Vertical', '1080 × 1920', 'TikTok', 'Instagram'],
     'summary': 'A vertical 1080 × 1920 multiview to follow TikTok, Instagram and YouTube Shorts lives.',
     'description': 'A portrait multiview for a vertical screen: the vertical live large (TikTok, Instagram, YouTube Shorts) with its safe areas, two other lives beside it, the horizontal programme, the loudness and the frames a second of each stream.'},
    w=1080, h=1920, rate=(30, 1),
)
def _(p):
    p.header('Social · vertical', '#ff3b5c', clock=False, status=False)
    p.source(1, (16, 72, 640, 1138), 'TIKTOK LIVE', loud=True, vu=0, markers={'action': True, 'title': True}, fit='cover')
    p.source(2, (668, 72, 396, 563), 'INSTAGRAM', vu=0, fit='cover')
    p.source(3, (668, 647, 396, 563), 'SHORTS', vu=0, fit='cover')
    p.source(4, (16, 1222, 640, 360), 'PGM HORIZONTAL', tc='station')
    p.value('time', (668, 1222, 396, 110), show=False)
    p.values((668, 1344, 396, 238), [('integrated', 1, 'Loudness I'), ('alarm', 1, 'TikTok')], stack=True)
    for i, cell in enumerate(row((16, 1594, 1048, 310), 3, 12)):
        p.chart('inputFps', cell, n=i + 1, title=['TikTok', 'Instagram', 'Shorts'][i], color=['#ff3b5c', '#a594ff', '#ffd84e'][i], crit=20, mn=0, mx=60, stats=False)


def write():
    for old in OUT.glob('ex-*.json'):
        old.unlink()
    for id_, name, category, tags, summary, description, en, background, grad, page, (num, den) in EXAMPLES:
        bg = {'color': background, 'fillStyle': {'kind': 'linear' if grad else 'solid', 'color2': grad[0] if grad else '#000000', 'angle': float(grad[1]) if grad else 90.0, 'image': '', 'fit': 'cover'}}
        tid = hashlib.sha1(f'{id_}/page'.encode()).hexdigest()[:12]
        doc = {
            'meta': {'id': id_, 'name': name, 'category': category, 'tags': tags, 'description': description, 'summary': summary, 'en': en},
            'project': {
                'id': '', 'name': name, 'description': summary, 'width': page.w, 'height': page.h, 'frameRate': {'num': num, 'den': den},
                'templates': [{'id': tid, 'name': name, 'duration': 50.0, 'markers': [], 'fields': [], 'layers': page.layers, 'zone': 'full-screen', 'revision': 1, 'markerModel': 2, 'background': bg}],
                'zones': [{'id': 'full-screen', 'name': 'Full screen', 'layer': 10, 'color': '#8fa3ff', 'transition': 'sequential'}],
                'formats': [], 'language': 'fr',
            },
        }
        (OUT / f'{id_}.json').write_text(json.dumps(doc, ensure_ascii=False, indent=1) + '\n')
    print(f'{len(EXAMPLES)} examples written to {OUT}')


if __name__ == '__main__':
    write()
