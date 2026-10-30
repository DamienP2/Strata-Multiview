"""kit.py: shared helpers to write the example projects of Strata.

Each example is one file, crates/cg-server/examples/ex-NN-slug.json:
    {"meta": {id, name, category, tags, description, cover?, en?}, "project": Project,
     "sources": [{id, name, type: 'push', format: 'json', value: <sample JSON>}…]}   (sources optional)
The server embeds every ex-*.json of that folder (crates/cg-server/build.rs).
Scripts: set_a.py (01-10) and set_b.py (11-20); run `python3 tools/examples/set_a.py`.

API (stable):
    p = Package('ex-01-journal', 'Journal télévisé', category='Info', tags=[...],
                description='…', en=('TV news', '…'), cover='headline')
    p.zone(id, name, layer, color, area=(x, y, w, h), vertical=None, transition='sequential')
        area: fractions of the picture (0..1); graphics must stay inside it (tested).
    p.vertical()                       adds the 9:16 format (id 'vertical', 1080x1920)
    p.playlist(id, name, auto_next=False, loop=False)
    t = p.template(tid, name, zone, duration=100, on_air=50)   -> T (frames, 50 fps)
    p.item(name, tid, playlist, data=None, zone=None, auto_secs=None)
    p.source(id, name, value, kind='push', fmt='json')   shared data source + sample value (see write())
    p.guides(action_safe=0.035, title_safe=0.05, vertical=[], horizontal=[], exclusions=[])
    p.write()                          writes the JSON (returns the path)

    T (a template):
    t.field(key, label, default, kind='text'|'multiline'|'number'|'color'|'image'|'boolean'|'table', **extra)
    t.pause(frame, name, resume=None)  resume: {'type':'after','seconds':8} | {'type':'trigger','name':'x'}
    t.group(name, x, y, w, h, parent=None, clip=False, tracks=None, **props) -> layer id
    t.rect(name, x, y, w, h, fill, parent=None, radius=0, grad=None, stroke=None, shape=None, tracks=None, bindings=None, **props)
        grad=(color2, angle[, [(at %, color)…]]) linear gradient | ('radial', color2[, stops]); stroke=(color, width)
        shape='parallelogram' (skew=px) | 'trapezoid' | 'diamond'; corners=(tl,tr,br,bl)
    t.tri(name, x, y, w, h, fill, parent=None, dir='up'|'down'|'left'|'right'), t.poly(name, x, y, w, h, fill, verts, parent)
    t.ellipse(name, x, y, w, h, fill, parent=None, grad=None, stroke=None, start=0, end=360, inner=0, tracks=None, **props)
    t.text(name, text, x, y, w, h, size, parent=None, color=WHITE, weight=700, family=SANS, align='left',
           valign='middle', spacing=0, upper=False, tracks=None, bindings=None, **attrs)
        attrs: wrap=True, lineHeight=1.2, italic=True, shadow=True, overflow='fit'|'ellipsis'|'clip', maxChars…
    t.clock(name, x, y, w, h, size, parent=None, mode='clock'|'date'|'stopwatch'|'countdown', fmt='HH:mm', **cfg)
    t.ticker(name, text, x, y, w, h, size, parent=None, speed=110, mode='crawl'|'flip'|'roll', **cfg)
    t.counter(name, to, x, y, w, h, size, parent=None, frm='0', decimals=0, prefix='', suffix='', duration=1.2, **cfg)
    t.progress(name, value, x, y, w, h, fill, parent=None, max='100', track='#ffffff22', duration=1.2, **cfg)
    t.image(name, src, x, y, w, h, parent=None, fit='contain', **cfg)
    t.qr(name, data, x, y, size, parent=None, fill=INK, background='#ffffff')
    t.widget(type, name, x, y, w, h, parent=None, props=None, tracks=None, **attrs)  any other widget
        (chart, table, map, scoreboard, credits, typewriter…: settings as in crates/cg-core/src/wid/*.rs)
    t.fit(text_id, [(plate_id, "width")], pad=60, minimum=400)  plate follows the text width (logic)
    t.choose(key, {1: [ids], 2: [ids]})   shows the layers of the chosen value (field with options=[("1","Soleil"),…])
    t.override(layer_id, 'vertical', x=…, y=…, width=…, height=…, fontSize=…, visible=None)
    t.layer_ref(layer_id) -> the layer dict (to add settings by hand)

    Motion (frames; easings: EXPO_OUT, EXPO_IN, SMOOTH, 'easeOut', 'back', 'linear', 'hold'…):
    keys((frame, value, easing?), ...)
    inout(prop, rest, start, on_air, t_in=0, length=18, out_len=12, delay_out=0)
        prop goes start -> rest at t_in (expo out), back to start after on_air (expo in)
    fade(on_air, t_in=0, length=12), wipe(on_air, prop='cropRight', t_in=0, length=18)
    slide(prop, start, rest, on_air, t_in=0, length=18), merge(*tracks)
    pulse(prop, lo, hi, period, until)    looping blink (live dots)

    French typography (no-break spaces before : ; ! ? %, ’) is applied by write() to names, labels,
    defaults, texts and rundown data; fr(s) does it by hand.

    Colours: hexc('#rrggbb', alpha) -> [r,g,b,a]; hexs('#rrggbb', alpha) -> '#rrggbbaa'
    Fonts (bundled, crates/cg-render/src/text.rs): SANS='Inter' (400-800), COND='Barlow Condensed' (500-800),
    DISPLAY='Bebas Neue' (400), SERIF='Playfair Display' (600-800), MONO='JetBrains Mono' (500, 700),
    TECH='Titillium Web' (600, 700, 900), GEO='Montserrat' (600, 800), GROTESK='Space Grotesk' (500, 700)
"""
import json
import os

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
OUT = os.path.join(ROOT, 'crates', 'cg-server', 'examples')
FPS = 50
W, H = 1920, 1080

SANS = 'Inter'
COND = 'Barlow Condensed'
DISPLAY = 'Bebas Neue'
SERIF = 'Playfair Display'
MONO = 'JetBrains Mono'
TECH = 'Titillium Web'
GEO = 'Montserrat'
GROTESK = 'Space Grotesk'

NBSP = ' '
NNBSP = ' '  # kept for older scripts: U+202F is missing from several bundled fonts


# ----------------------------------------------------------------- colours --

def hexc(h, a=1.0):
    """'#rrggbb' (or '#rrggbbaa') -> [r, g, b, a] (property value)."""
    h = h.lstrip('#')
    if len(h) == 8:
        a = int(h[6:8], 16) / 255 * a
    return [round(int(h[i:i + 2], 16) / 255, 4) for i in (0, 2, 4)] + [round(a, 4)]


def hexs(h, a=1.0):
    """'#rrggbb' -> '#rrggbbaa' (settings strings)."""
    h = h.lstrip('#')[:6]
    return '#' + h + ('' if a >= 1 else '%02x' % round(a * 255))


WHITE = hexc('#ffffff')
BLACK = hexc('#000000')
INK = hexc('#0b0d17')
CLEAR = hexc('#000000', 0)


# ------------------------------------------------------------------ motion --

EXPO_OUT = {'type': 'bezier', 'x1': 0.16, 'y1': 1.0, 'x2': 0.3, 'y2': 1.0}
EXPO_IN = {'type': 'bezier', 'x1': 0.7, 'y1': 0.0, 'x2': 0.84, 'y2': 0.0}
SMOOTH = {'type': 'bezier', 'x1': 0.65, 'y1': 0.0, 'x2': 0.35, 'y2': 1.0}
QUART_OUT = {'type': 'bezier', 'x1': 0.25, 'y1': 1.0, 'x2': 0.5, 'y2': 1.0}


def ease(e):
    return e if isinstance(e, dict) else {'type': e}


def keys(*pts):
    """keys((frame, value, easing?), ...); the easing shapes the segment leaving the key."""
    out = []
    for p in pts:
        f, v = p[0], p[1]
        out.append({'frame': float(f), 'value': float(v) if isinstance(v, (int, float)) else v,
                    'easing': ease(p[2] if len(p) > 2 else EXPO_OUT)})
    return out


def inout(prop, rest, start, on_air, t_in=0, length=18, out_len=12, delay_out=0, ein=EXPO_OUT, eout=EXPO_IN):
    """`prop`: start -> rest from t_in, back to start after the on-air point."""
    pts = []
    if t_in > 0:
        pts.append((0, start, 'hold'))
    pts += [(t_in, start, ein), (t_in + length, rest, 'linear'), (on_air + delay_out, rest, eout),
            (on_air + delay_out + out_len, start, 'linear')]
    return {prop: keys(*pts)}


def fade(on_air, t_in=0, length=12, out_len=10, delay_out=0):
    return inout('opacity', 1, 0, on_air, t_in, length, out_len, delay_out, 'linear', 'linear')


def wipe(on_air, prop='cropRight', t_in=0, length=18, out_len=12, delay_out=0):
    return inout(prop, 0, 1, on_air, t_in, length, out_len, delay_out)


def slide(prop, start, rest, on_air, t_in=0, length=18, out_len=12, delay_out=0):
    return inout(prop, rest, start, on_air, t_in, length, out_len, delay_out)


def pulse(prop, lo, hi, period, until, start=0):
    pts, f, v = [], start, hi
    while f <= until:
        pts.append((f, v, SMOOTH))
        f += period / 2
        v = lo if v == hi else hi
    return {prop: keys(*pts)}


def merge(*tracks):
    out = {}
    for t in tracks:
        out.update(t or {})
    return out


# ---------------------------------------------------------------- template --

class T:
    """A template being built (frames at 50 fps)."""

    def __init__(self, tid, name, zone, duration=100, on_air=50):
        self.t = {
            'id': tid, 'name': name, 'duration': float(duration),
            'markers': [{'id': f'{tid}-air', 'frame': float(on_air), 'name': 'On air', 'kind': 'out'}],
            'fields': [], 'layers': [], 'zone': zone, 'revision': 1, 'markerModel': 2,
        }
        self.id = tid
        self.on_air = on_air
        self.n = 0
        self._by_id = {}

    # fields and markers
    def field(self, key, label, default='', kind='text', **extra):
        self.t['fields'].append({'key': key, 'label': label, 'kind': kind, 'default': str(default), **extra})
        return '{{' + key + '}}'

    def pause(self, frame, name='Pause', resume=None):
        m = {'id': f"{self.id}-p{len(self.t['markers'])}", 'frame': float(frame), 'name': name, 'kind': 'pause'}
        if resume:
            m['resume'] = resume
        self.t['markers'].append(m)

    def label(self, frame, name):
        self.t['markers'].append({'id': f"{self.id}-l{len(self.t['markers'])}", 'frame': float(frame), 'name': name, 'kind': 'label'})

    def dataset(self, ds):
        self.t.setdefault('datasets', []).append(ds)

    def logic(self, nodes, edges):
        lg = self.t.setdefault('logic', {'nodes': [], 'edges': []})
        lg['nodes'] += nodes
        lg['edges'] += edges

    def fit(self, text_id, targets, pad=60, minimum=0, metric='width'):
        """Plates that follow a text: each target (layer id, prop) gets text size + pad (at least minimum)."""
        k = len(self.t.get('logic', {}).get('nodes', []))
        n = lambda s: f'n{k}{s}'
        nodes = [{'id': n('m'), 'type': 'textSize', 'layer': text_id, 'metric': metric, 'x': 40, 'y': 60 + k * 40},
                 {'id': n('p'), 'type': 'constant', 'value': pad, 'x': 40, 'y': 160 + k * 40},
                 {'id': n('s'), 'type': 'math', 'op': 'add', 'x': 300, 'y': 100 + k * 40},
                 {'id': n('c'), 'type': 'constant', 'value': minimum, 'x': 300, 'y': 220 + k * 40},
                 {'id': n('x'), 'type': 'math', 'op': 'max', 'x': 560, 'y': 120 + k * 40}]
        edges = [{'from': n('m'), 'to': n('s'), 'port': 'a'}, {'from': n('p'), 'to': n('s'), 'port': 'b'},
                 {'from': n('s'), 'to': n('x'), 'port': 'a'}, {'from': n('c'), 'to': n('x'), 'port': 'b'}]
        for i, tg in enumerate(targets):
            lid, prop = tg[0], tg[1]
            nodes.append({'id': n(f't{i}'), 'type': 'target', 'layer': lid, 'prop': prop, 'mode': 'set', 'x': 820, 'y': 60 + i * 100 + k * 40})
            edges.append({'from': n('x'), 'to': n(f't{i}'), 'port': 'in'})
        self.logic(nodes, edges)

    def choose(self, key, layers_by_value):
        """Shows the layers of the value the field `key` has (options of a field), hides the others."""
        k = len(self.t.get('logic', {}).get('nodes', []))
        nodes = [{'id': f'c{k}f', 'type': 'field', 'key': key, 'x': 40, 'y': 60},
                 {'id': f'c{k}1', 'type': 'constant', 'value': 1, 'x': 40, 'y': 180},
                 {'id': f'c{k}0', 'type': 'constant', 'value': 0, 'x': 40, 'y': 300}]
        edges = []
        for i, (val, lids) in enumerate(layers_by_value.items()):
            v, c = f'c{k}v{i}', f'c{k}q{i}'
            nodes += [{'id': v, 'type': 'constant', 'value': float(val), 'x': 300, 'y': 60 + i * 140},
                      {'id': c, 'type': 'condition', 'cmp': 'eq', 'x': 560, 'y': 60 + i * 140}]
            edges += [{'from': f'c{k}f', 'to': c, 'port': 'a'}, {'from': v, 'to': c, 'port': 'b'},
                      {'from': f'c{k}1', 'to': c, 'port': 'then'}, {'from': f'c{k}0', 'to': c, 'port': 'else'}]
            for j, lid in enumerate(lids if isinstance(lids, (list, tuple)) else [lids]):
                tg = f'c{k}t{i}_{j}'
                nodes.append({'id': tg, 'type': 'target', 'layer': lid, 'prop': 'opacity', 'mode': 'multiply', 'x': 820, 'y': 60 + i * 140 + j * 40})
                edges.append({'from': c, 'to': tg, 'port': 'in'})
        self.logic(nodes, edges)

    # layers
    def layer(self, kind, name, parent=None, props=None, tracks=None, bindings=None, **attrs):
        self.n += 1
        lid = f'{self.id}-{self.n}'
        clean = {}
        for k, v in (props or {}).items():
            clean[k] = float(v) if isinstance(v, (int, float)) and not isinstance(v, bool) else v
        layer = {'id': lid, 'name': name, 'parent': parent, 'type': kind, 'visible': True, 'locked': False,
                 'props': clean, 'tracks': tracks or {}, 'bindings': bindings or {}, **attrs}
        self.t['layers'].append(layer)
        self._by_id[lid] = layer
        return lid

    def layer_ref(self, lid):
        return self._by_id[lid]

    def override(self, lid, fmt='vertical', visible=None, tracks=None, **props):
        o = self._by_id[lid].setdefault('overrides', {}).setdefault(fmt, {})
        if props:
            o.setdefault('props', {}).update({k: float(v) for k, v in props.items()})
        if tracks:
            o['tracks'] = tracks
        if visible is not None:
            o['visible'] = visible

    def group(self, name, x, y, w, h, parent=None, clip=False, tracks=None, bindings=None, **props):
        return self.layer('group', name, parent, {'x': x, 'y': y, 'width': w, 'height': h, **props}, tracks, bindings, clip=clip)

    @staticmethod
    def _fill_style(grad):
        if not grad:
            return None
        if grad[0] == 'radial':
            fs = {'kind': 'radial', 'color2': grad[1]}
            if len(grad) > 2:
                fs['stops'] = [{'at': float(a), 'color': c} for a, c in grad[2]]
            return fs
        fs = {'kind': 'linear', 'color2': grad[0], 'angle': float(grad[1]) if len(grad) > 1 else 90.0}
        if len(grad) > 2:
            fs['stops'] = [{'at': float(a), 'color': c} for a, c in grad[2]]
        return fs

    def rect(self, name, x, y, w, h, fill, parent=None, radius=0, grad=None, stroke=None, shape=None, skew=None,
             corners=None, tracks=None, bindings=None, shadow=None, **props):
        p = {'x': x, 'y': y, 'width': w, 'height': h, 'fill': fill, **props}
        attrs = {}
        if radius:
            p['cornerRadius'] = radius
        if corners:
            attrs['perCorner'] = True
            attrs['corners'] = [float(c) for c in corners]
        fs = self._fill_style(grad)
        if fs:
            attrs['fillStyle'] = fs
        if stroke:
            p['stroke'] = stroke[0]
            p['strokeWidth'] = stroke[1]
        if shape:
            attrs['shape'] = shape
            if skew is not None:
                attrs['skewPx'] = float(skew)
        if shadow:
            attrs['dropShadow'] = True
            c, sx, sy, blur = shadow
            p.update({'shadowColor': c, 'shadowX': sx, 'shadowY': sy, 'shadowBlur': blur})
        return self.layer('rect', name, parent, p, tracks, bindings, **attrs)

    def ellipse(self, name, x, y, w, h, fill, parent=None, grad=None, stroke=None, start=0, end=360, inner=0,
                tracks=None, bindings=None, **props):
        p = {'x': x, 'y': y, 'width': w, 'height': h, 'fill': fill, **props}
        attrs = {}
        fs = self._fill_style(grad)
        if fs:
            attrs['fillStyle'] = fs
        if stroke:
            p['stroke'] = stroke[0]
            p['strokeWidth'] = stroke[1]
        if start or end != 360 or inner:
            attrs.update({'start': float(start), 'end': float(end), 'inner': float(inner)})
        return self.layer('ellipse', name, parent, p, tracks, bindings, **attrs)

    def tri(self, name, x, y, w, h, fill, parent=None, dir='up', tracks=None, bindings=None, **props):
        """Isosceles triangle pointing up / down / left / right (polygon widget)."""
        p = {'x': x, 'y': y, 'width': w, 'height': h, 'fill': fill, **props}
        return self.layer('polygon', name, parent, p, tracks, bindings, mode='triangle', tri='isosceles', triDir=dir)

    def poly(self, name, x, y, w, h, fill, verts, parent=None, tracks=None, bindings=None, **props):
        """Free polygon: verts as fractions of the box [[x, y], …]."""
        p = {'x': x, 'y': y, 'width': w, 'height': h, 'fill': fill, **props}
        return self.layer('polygon', name, parent, p, tracks, bindings, mode='free', verts=[[float(a), float(b)] for a, b in verts])

    def text(self, name, text, x, y, w, h, size, parent=None, color=WHITE, weight=700, family=SANS, align='left',
             valign='middle', spacing=0, upper=False, tracks=None, bindings=None, **attrs):
        p = {'x': x, 'y': y, 'width': w, 'height': h, 'fill': color, 'fontSize': size}
        if spacing:
            p['letterSpacing'] = spacing
        attrs.setdefault('lang', 'fr')
        return self.layer('text', name, parent, p, tracks, bindings, text=text, fontFamily=family, fontWeight=weight,
                          align=align, verticalAlign=valign, uppercase=upper, **attrs)

    @staticmethod
    def _font(family, weight, align, valign='middle', upper=False):
        return {'fontFamily': family, 'fontWeight': weight, 'align': align, 'verticalAlign': valign, 'uppercase': upper}

    def clock(self, name, x, y, w, h, size, parent=None, mode='clock', fmt='HH:mm', color=WHITE, family=SANS,
              weight=700, align='center', tz='Europe/Paris', tracks=None, **cfg):
        legacy = {'clock': 'time', 'date': 'time', 'stopwatch': 'elapsed', 'countdown': 'timer'}[mode]
        attrs = dict(mode=legacy, format=fmt, timezone=tz, target=cfg.pop('target', ''), timerMode=mode,
                     lang=cfg.pop('lang', 'fr-FR'), **self._font(family, weight, align))
        attrs.update(cfg)
        return self.layer('clock', name, parent, {'x': x, 'y': y, 'width': w, 'height': h, 'fill': color, 'fontSize': size},
                          tracks, None, **attrs)

    def ticker(self, name, text, x, y, w, h, size, parent=None, color=WHITE, family=SANS, weight=600, speed=110,
               mode='crawl', tracks=None, **cfg):
        attrs = dict(text=text, separator=cfg.pop('separator', '   '), speed=float(speed), mode=mode,
                     **self._font(family, weight, cfg.pop('align', 'left')))
        attrs.setdefault('sepType', 'symbol')
        attrs.setdefault('startFull', True)
        attrs.update(cfg)
        return self.layer('ticker', name, parent, {'x': x, 'y': y, 'width': w, 'height': h, 'fill': color, 'fontSize': size},
                          tracks, None, **attrs)

    def counter(self, name, to, x, y, w, h, size, parent=None, frm='0', decimals=0, prefix='', suffix='', duration=1.2,
                color=WHITE, family=SANS, weight=800, align='left', grouping=True, tracks=None, **cfg):
        attrs = {'from': str(frm), 'to': str(to), 'duration': float(duration), 'decimals': decimals, 'prefix': prefix,
                 'suffix': suffix, 'grouping': grouping, **self._font(family, weight, align)}
        attrs.update(cfg)
        return self.layer('counter', name, parent, {'x': x, 'y': y, 'width': w, 'height': h, 'fill': color, 'fontSize': size},
                          tracks, None, **attrs)

    def progress(self, name, value, x, y, w, h, fill, parent=None, max='100', track='#ffffff22', duration=1.2,
                 radius=0, tracks=None, **cfg):
        p = {'x': x, 'y': y, 'width': w, 'height': h, 'fill': fill}
        if radius:
            p['cornerRadius'] = radius
        return self.layer('progress', name, parent, p, tracks, None, value=str(value), max=str(max), track=track,
                          duration=float(duration), **cfg)

    def image(self, name, src, x, y, w, h, parent=None, fit='contain', tracks=None, **cfg):
        return self.layer('image', name, parent, {'x': x, 'y': y, 'width': w, 'height': h}, tracks, None, src=src, fit=fit, **cfg)

    def qr(self, name, data, x, y, size, parent=None, fill=INK, background='#ffffff', tracks=None, **cfg):
        return self.layer('qrCode', name, parent, {'x': x, 'y': y, 'width': size, 'height': size, 'fill': fill}, tracks,
                          None, data=data, background=background, **cfg)

    def widget(self, kind, name, x, y, w, h, parent=None, props=None, tracks=None, **attrs):
        return self.layer(kind, name, parent, {'x': x, 'y': y, 'width': w, 'height': h, **(props or {})}, tracks, **attrs)

    def done(self):
        # Layers repeated in several groups (rows, cards…) are named after their group, so the
        # Operator's tabs read « Joueur 3 · Nom » rather than eleven « Nom ».
        names = [l['name'] for l in self.t['layers']]
        for l in self.t['layers']:
            par = self._by_id.get(l.get('parent') or '')
            if par and names.count(l['name']) > 1 and not l['name'].startswith(par['name']):
                l['name'] = f"{par['name']} · {l['name']}"
        return self.t


# ------------------------------------------------------------ typography --

def fr(s):
    """French typography: no-break space before : ; ! ? % and inside « », ’ apostrophe."""
    import re
    s = s.replace("'", '’').replace('\u202f', NBSP)
    s = re.sub(r' ([:;!?%»])', NBSP + r'\1', s)
    s = s.replace('« ', '«' + NBSP)
    return s


def _fr_tree(o, key=None):
    """fr() on the texts people read (not ids, keys, URLs or settings)."""
    texts = {'name', 'label', 'default', 'description', 'text', 'title', 'sub'}
    if isinstance(o, dict):
        return {k: (_fr_tree(v, k) if not (k == 'data' and isinstance(v, dict)) else {dk: fr(dv) if isinstance(dv, str) else dv for dk, dv in v.items()}) for k, v in o.items()}
    if isinstance(o, list):
        return [_fr_tree(v, key) for v in o]
    if isinstance(o, str) and key in texts and '://' not in o:
        return fr(o)
    return o


# ----------------------------------------------------------------- package --

# One-line card text of each example (the description is the long text).
SUMMARIES = {
    'ex-01-journal-televise': ('Synthés, lieu, logo, bandeau urgent et titres d’un grand JT.', 'Name straps, location, logo, breaking strap and titles for a main newscast.'),
    'ex-02-info-continu': ('Bandeau permanent, alerte info et fil d’une chaîne d’info.', 'Permanent strap, breaking alert and news ticker for a news channel.'),
    'ex-03-football': ('Score et chrono, compositions, buts et cartons d’un match.', 'Score and clock, line-ups, goals and cards for a match.'),
    'ex-04-soiree-electorale': ('Résultats par candidat, participation et sièges en direct.', 'Live results per candidate, turnout and seats.'),
    'ex-05-meteo': ('Carte des températures, villes et prévisions sur plusieurs jours.', 'Temperature map, cities and multi-day forecast.'),
    'ex-06-bourse': ('Ticker des indices, cours et graphiques des marchés.', 'Index ticker, prices and market charts.'),
    'ex-07-talk-show': ('Synthés des invités, thème du jour et citations à l’écran.', 'Guest straps, topic of the day and on-screen quotes.'),
    'ex-08-esport-stream': ('Bandeau de match, caméras joueurs et partenaires en rotation.', 'Match bar, player cams and rotating sponsors.'),
    'ex-09-grand-prix': ('Classement des pilotes, écarts et chrono de la course.', 'Driver standings, gaps and race timing.'),
    'ex-10-tennis': ('Score jeux-sets-points, fiches et statistiques des joueurs.', 'Games-sets-points score, player cards and stats.'),
    'ex-11-podcast-youtube': ('Synthé invité, chapitres et appel à s’abonner.', 'Guest strap, chapters and subscribe call to action.'),
    'ex-12-festival-live': ('Artiste à l’antenne, programme de la soirée et messages.', 'Artist on stage, tonight’s line-up and messages.'),
    'ex-13-documentaire': ('Titre, chapitres, citations et légendes d’archives.', 'Title, chapters, quotes and archive captions.'),
    'ex-14-keynote-tech': ('Intervenant, session en cours, programme et questions du public.', 'Speaker, current session, schedule and audience questions.'),
    'ex-15-ceremonie-prix': ('Catégories, nommés et révélation du lauréat en noir et or.', 'Categories, nominees and winner reveal in black and gold.'),
    'ex-16-quiz-tv': ('Question, quatre réponses, chrono et scores des candidats.', 'Question, four answers, timer and contestant scores.'),
    'ex-17-basket': ('Score, horloge de match, 24 secondes, fautes et temps morts.', 'Score, game clock, shot clock, fouls and timeouts.'),
    'ex-18-cyclisme': ('Profil de l’étape, parcours, écarts et kilomètres restants.', 'Stage profile, route, gaps and distance to go.'),
    'ex-19-matinale': ('Bandeau heure, météo et trafic, sujets du jour et messages.', 'Time, weather and traffic strap, top stories and messages.'),
    'ex-20-live-shopping': ('Fiche produit, offre flash et stock pour une vente en direct.', 'Product card, flash offer and stock for live selling.'),
    'ex-21-reseaux-sociaux': ('Messages validés des réseaux sociaux : carte, mur, bandeau, compteur.', 'Approved social messages: card, wall, ticker, counter.'),
}


class Package:
    """One example project."""

    def __init__(self, eid, name, category, tags, description, en=None, cover=None, width=W, height=H):
        assert eid.startswith('ex-'), 'example ids are ex-NN-slug'
        self.meta = {'id': eid, 'name': name, 'category': category, 'tags': tags, 'description': description}
        if en:
            self.meta['en'] = {'name': en[0], 'description': en[1]}
        if cover:
            self.meta['cover'] = cover
        self.p = {'id': eid, 'name': name, 'description': description, 'width': width, 'height': height,
                  'frameRate': {'num': FPS, 'den': 1}, 'templates': [], 'rundown': [], 'zones': [],
                  'playlists': [], 'formats': [], 'language': 'fr'}
        self._templates = []
        self.sources = []

    def source(self, sid, name, value, kind='push', fmt='json', **extra):
        """A shared data source the example reads (field bindings, data sets), created with the
        project when missing, holding `value` (sample JSON) until automation pushes another one.
        kind='push' (format json|csv|xml) or a fetched one: kind='json'|'csv'|'rss'|'xml' with url=…"""
        src = {'id': sid, 'name': name, 'type': kind, **extra}
        if kind == 'push':
            src['format'] = fmt
        if value is not None:
            src['value'] = value
        self.sources.append(src)
        return sid

    def zone(self, zid, name, layer, color, area=(0, 0, 1, 1), vertical=None, transition='sequential'):
        box = lambda a: dict(zip(('x', 'y', 'width', 'height'), [float(v) for v in a]))
        z = {'id': zid, 'name': name, 'layer': layer, 'color': color, 'transition': transition, 'areas': {'main': box(area)}}
        if vertical:
            z['areas']['vertical'] = box(vertical)
        self.p['zones'].append(z)
        return zid

    def vertical(self):
        self.p['formats'].append({'id': 'vertical', 'name': '9:16', 'width': 1080, 'height': 1920})

    def playlist(self, pid, name, auto_next=False, loop=False):
        pl = {'id': pid, 'name': name}
        if auto_next:
            pl['autoNext'] = True
        if loop:
            pl['loop'] = True
        self.p['playlists'].append(pl)
        return pid

    def template(self, tid, name, zone, duration=100, on_air=50):
        t = T(tid, name, zone, duration, on_air)
        self._templates.append(t)
        return t

    def item(self, name, tid, playlist, data=None, zone=None, auto_secs=None):
        t = next(t for t in self._templates if t.id == tid)
        slug = ''.join(c if c.isalnum() else '-' for c in name.lower()).strip('-')
        n = len(self.p['rundown'])
        it = {'id': f'{playlist}-{n:02d}-{slug}'[:60], 'name': name, 'templateId': tid, 'zone': zone or t.t['zone'],
              'playlist': playlist, 'layer': 0, 'data': {k: str(v) for k, v in (data or {}).items()}}
        if auto_secs:
            it['autoSecs'] = float(auto_secs)
        self.p['rundown'].append(it)

    def guides(self, action_safe=0.035, title_safe=0.05, vertical=None, horizontal=None, exclusions=None):
        self.p['guides'] = {'vertical': vertical or [], 'horizontal': horizontal or [], 'actionSafe': action_safe,
                            'titleSafe': title_safe, 'exclusions': exclusions or []}

    def write(self):
        zl = {z['id']: z['layer'] for z in self.p['zones']}
        for r in self.p['rundown']:
            r['layer'] = zl[r['zone']]
        self.p['templates'] = [t.done() for t in self._templates]
        os.makedirs(OUT, exist_ok=True)
        path = os.path.join(OUT, self.meta['id'] + '.json')
        with open(path, 'w') as fh:
            meta = dict(self.meta, name=fr(self.meta['name']), description=fr(self.meta['description']))
            if self.meta['id'] in SUMMARIES:
                summary, summary_en = SUMMARIES[self.meta['id']]
                meta['summary'] = summary
                if 'en' in meta:
                    meta['en'] = dict(meta['en'], summary=summary_en)
            out = {'meta': meta, 'project': _fr_tree(self.p)}
            if self.sources:
                out['sources'] = self.sources
            json.dump(out, fh, ensure_ascii=False, indent=1)
            fh.write('\n')
        return path
