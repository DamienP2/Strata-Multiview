#!/usr/bin/env python3
"""Example projects 11-20 of Strata (see kit.py for the format and the API).

Each one is a complete package, ready for air: several templates with their
In and Out, the fields the Operator fills, data sets where live data would
come from (a Push source the automation feeds), a playlist, and its own
identity (palette, fonts, grid, motion).

    11 Podcast & YouTube        16 Quiz TV
    12 Festival live            17 Basket
    13 Documentaire             18 Cyclisme
    14 Keynote tech             19 Matinale
    15 Cérémonie de prix        20 Live shopping

Run: python3 tools/examples/set_b.py
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from kit import *  # noqa: E402,F401,F403

# ------------------------------------------------------------- helpers --


def bind(t, key, source, path):
    """The field `key` takes its value from a data source when it has one."""
    for f in t.t['fields']:
        if f['key'] == key:
            f['binding'] = {'source': source, 'path': path}
            return
    raise KeyError(key)


def dataset(t, key, name, source, lst=''):
    """Rows of a source the Operator steps through (columns -> `key_column`)."""
    t.dataset({'key': key, 'name': name, 'source': source, 'list': lst, 'change': 'outIn'})


def frame(t, name, x, y, w, h, parent=None, tracks=None, **props):
    """A still group (anchor top left) that holds a whole graphic: what the
    other formats move and scale."""
    return t.group(name, x, y, w, h, parent, tracks=tracks, anchorX=0, anchorY=0, **props)


def place(t, lid, x, y, s=1.0, fmt='vertical'):
    """Moves and scales a still group in another format; its box is scaled too
    (anchor top left, not clipped), so the zone check sees what is drawn."""
    pr = t.layer_ref(lid)['props']
    t.override(lid, fmt, x=x, y=y, scaleX=s, scaleY=s, width=pr['width'] * s, height=pr['height'] * s)


def vfull(t, g, backs=(), s=0.5625):
    """9:16 for a full-screen 16:9 composition: scaled to the width, centred,
    its background layers (`backs`, children of `g`) stretched to the frame."""
    y = (1920 - 1080 * s) / 2
    place(t, g, 0, y, s)
    for b in backs:
        t.override(b, 'vertical', x=0.0, y=-y / s, width=1920.0, height=1920 / s)


def pop(on_air, t_in=0, length=16, out_len=10, delay_out=0):
    return merge(inout('scaleX', 1, 0, on_air, t_in, length, out_len, delay_out, ein='back'),
                 inout('scaleY', 1, 0, on_air, t_in, length, out_len, delay_out, ein='back'))


def fadein(on_air, t_in=0, length=12, out_len=10, delay_out=0):
    return fade(on_air, t_in, length, out_len, delay_out)


def rise(on_air, y, dy=40, t_in=0, length=20, out_len=12, delay_out=0):
    """Comes up from `dy` px lower, fading in; leaves the same way."""
    return merge(inout('y', y, y + dy, on_air, t_in, length, out_len, delay_out),
                 fade(on_air, t_in, int(length * 0.7), out_len, delay_out))


def band(color, pad=26, radius=0, gap=8, accent=None):
    """Text settings: a plate behind each line, as wide as the line."""
    d = {'band': True, 'bandColor': color, 'bandPadX': float(pad), 'bandRadius': float(radius), 'bandGap': float(gap),
         'bandAccent': bool(accent)}
    if accent:
        d['bandAccentColor'] = accent
    return d


def table_value(rows):
    """A table field's value: JSON [[header…], [cells…], …]."""
    import json
    return json.dumps(rows, ensure_ascii=False)


def fr(s):
    """French typography: narrow no-break space before ; ! ? and no-break
    space before : and % and inside « »."""
    out = s
    for c in (' :', ' %'):
        out = out.replace(c, NBSP + c[1:])
    for c in (' ;', ' !', ' ?'):
        out = out.replace(c, NBSP + c[1:])
    out = out.replace('« ', '«' + NBSP).replace(' »', NBSP + '»')
    return out.replace("'", '’')


def dashed(t, lid, dash='dashed'):
    ref = t.layer_ref(lid)
    ref['strokeStyle'] = {**ref.get('strokeStyle', {}), 'dash': dash, 'position': 'center'}
    return lid


def poly(t, name, x, y, w, h, fill, verts, parent=None, round_=0, tracks=None, **props):
    """A free polygon, vertices as fractions of its box."""
    return t.widget('polygon', name, x, y, w, h, parent, {'fill': fill, **props}, tracks, mode='free',
                    verts=[[float(a), float(b)] for a, b in verts], round=float(round_), clickAdd=False)


def tri(t, name, x, y, w, h, fill, direction='right', parent=None, tracks=None, **props):
    return t.widget('polygon', name, x, y, w, h, parent, {'fill': fill, **props}, tracks, mode='triangle',
                    tri='isosceles', triDir=direction)


# Icons drawn with shapes (no picture needed).
BELL = [(0.5, 0.0), (0.62, 0.08), (0.78, 0.3), (0.8, 0.62), (0.97, 0.8), (0.97, 0.86), (0.03, 0.86), (0.03, 0.8),
        (0.2, 0.62), (0.22, 0.3), (0.38, 0.08)]
THUMB = [(0.0, 0.42), (0.36, 0.4), (0.5, 0.04), (0.58, 0.0), (0.66, 0.06), (0.66, 0.38), (0.94, 0.4), (1.0, 0.5),
         (0.92, 0.94), (0.84, 1.0), (0.0, 1.0)]
PIN = [(0.5, 1.0), (0.12, 0.5), (0.08, 0.32), (0.2, 0.1), (0.5, 0.0), (0.8, 0.1), (0.92, 0.32), (0.88, 0.5)]
CHECK = [(0.0, 0.55), (0.14, 0.42), (0.38, 0.64), (0.86, 0.12), (1.0, 0.25), (0.38, 0.9)]


# =====================================================================
# 11 · Podcast & vidéaste YouTube — « Hors Champ »
# Warm plum-black, cream and a single coral; Space Grotesk on Inter;
# pills and rounded cards; quick, bouncy motion.
# =====================================================================

def ex11():
    BG, PANEL, CREAM, CORAL, SAND = '#141117', '#1f1a24', '#f6f0e8', '#ff5a3c', '#b9aea3'
    p = Package('ex-11-podcast-youtube', 'Podcast et chaîne YouTube', 'Divertissement',
                ['Podcast', 'YouTube', '9:16', 'Abonnement', 'Chapitres', 'Écran de fin'],
                fr("L’habillage complet d’un podcast filmé ou d’une chaîne YouTube : synthé invité à la largeur du nom, "
                   "chapitres numérotés, appel à s’abonner animé (le bouton se clique et la cloche sonne), « J’aime », "
                   "réseaux en rotation et écran de fin aux emplacements YouTube. Chaque graphique a sa mise en page 9:16 pour les Shorts."),
                en=('Podcast & YouTube channel', 'A complete package for a filmed podcast or a YouTube channel: guest name strap that '
                    'fits the name, numbered chapters, an animated subscribe call (the button gets clicked, the bell rings), '
                    'a like call, rotating socials and an end screen laid out for YouTube’s elements. Every graphic has its 9:16 layout for Shorts.'),
                cover='guest')
    p.vertical()
    p.zone('bug', 'Épisode', 30, '#ff5a3c', area=(0.03, 0.04, 0.34, 0.08), vertical=(0.05, 0.065, 0.9, 0.06))
    p.zone('social', 'Réseaux', 31, '#f5a524', area=(0.64, 0.04, 0.33, 0.1), vertical=(0.05, 0.14, 0.9, 0.076))
    p.zone('lower', 'Synthé', 10, '#7c5cff', area=(0.04, 0.66, 0.62, 0.3), vertical=(0.04, 0.6, 0.92, 0.2))
    p.zone('cta', 'Appel à l’action', 20, '#22c55e', area=(0.6, 0.68, 0.37, 0.28), vertical=(0.05, 0.44, 0.9, 0.14))
    p.zone('full', 'Plein écran', 50, '#38bdf8')
    p.playlist('show', fr('Épisode 42'))
    p.playlist('end', 'Fin de la vidéo')
    p.guides(action_safe=0.035, title_safe=0.05, exclusions=[
        {'id': 'yt', 'name': 'Boutons Shorts (9:16)', 'x': 0.84, 'y': 0.5, 'width': 0.14, 'height': 0.4}])

    # --- Episode bug -------------------------------------------------
    t = p.template('bug', 'Épisode', 'bug', duration=90, on_air=40)
    t.field('ep', 'Numéro', '42', kind='number', min=1, max=999)
    t.field('title', 'Titre', fr('Ce que le cinéma doit aux musiciens'))
    g = frame(t, 'Épisode', 70, 52, 560, 64)
    a = t.group('Plaque', 0, 0, 560, 64, g, clip=True, tracks=wipe(40, length=20))
    t.rect('Fond', 0, 0, 560, 64, hexc(PANEL, 0.88), a, radius=32, stroke=(hexc('#ffffff', 0.08), 1.5))
    t.ellipse('Point', 22, 25, 14, 14, hexc(CORAL), a, tracks=pulse('opacity', 0.35, 1, 50, 90, start=10))
    t.text('Numéro', fr('ÉP. {{ep}}'), 48, 0, 100, 64, 21, a, color=hexc(CORAL), weight=800, spacing=1.5)
    t.rect('Filet', 146, 20, 2, 24, hexc(CREAM, 0.25), a)
    t.text('Titre', '{{title}}', 164, 0, 380, 64, 21, a, color=hexc(CREAM), weight=600, overflow='ellipsis')
    place(t, g, 54, 125, 1.65)

    # --- Socials -----------------------------------------------------
    t = p.template('social', 'Réseaux', 'social', duration=90, on_air=40)
    t.field('handles', 'Comptes (un par ligne)', 'YouTube  ·  @horschamp\nInstagram  ·  @horschamp.podcast\nTikTok  ·  @horschamp\nSpotify  ·  Hors Champ', kind='multiline')
    g = frame(t, 'Réseaux', 1250, 54, 600, 84)
    a = t.group('Pilule', 0, 0, 600, 84, g, tracks=merge(slide('x', 640, 0, 40, length=22), fade(40, length=10)))
    t.rect('Fond', 0, 0, 600, 84, hexc(PANEL, 0.9), a, radius=42, stroke=(hexc('#ffffff', 0.08), 1.5))
    t.ellipse('Pastille', 12, 12, 60, 60, hexc(CORAL), a)
    t.text('@', '@', 12, 10, 60, 60, 32, a, color=hexc(BG), weight=700, family=GROTESK, align='center')
    t.ticker('Comptes', '{{handles}}', 92, 0, 490, 84, 26, a, color=hexc(CREAM), family=GROTESK, weight=500, mode='flip',
             itemDur=3.0, transDur=0.5, trans='slide')
    place(t, g, 60, 280, 1.6)

    # --- Guest name strap -------------------------------------------
    t = p.template('guest', 'Invité', 'lower', duration=110, on_air=60)
    t.field('name', 'Nom', 'Camille Martin')
    t.field('role', 'Fonction', fr('Compositrice de musiques de films'))
    t.field('handle', 'Réseau', '@camille.compose')
    t.field('initials', 'Initiales', 'CM')
    g = frame(t, 'Invité', 96, 790, 1150, 200)
    av = t.group('Avatar', 0, 36, 132, 132, g, tracks=pop(60, 0, 18))
    t.ellipse('Anneau', 0, 0, 132, 132, hexc(CORAL), av)
    t.ellipse('Disque', 7, 7, 118, 118, hexc(PANEL), av)
    t.text('Initiales', '{{initials}}', 7, 7, 118, 118, 46, av, color=hexc(CREAM), family=GROTESK, align='center', upper=True)
    t.text('Nom', '{{name}}', 150, 22, 1000, 96, 56, g, color=hexc(BG), family=GROTESK, weight=700, lineHeight=1.55,
           tracks=wipe(60, t_in=6, length=20), **band(CREAM, pad=34, radius=38))
    t.text('Fonction', '{{role}}   <c=' + CORAL + '>{{handle}}</c>', 150, 120, 1000, 60, 26, g, color=hexc(CREAM), weight=600,
           lineHeight=1.95, tracks=merge(inout('y', 120, 150, 60, 12, 20), fade(60, 12, 12)), **band(hexs(PANEL, 0.94), pad=28, radius=24))
    place(t, g, 44, 1190, 0.86)

    # --- Chapter -------------------------------------------------------
    t = p.template('chapter', 'Chapitre', 'lower', duration=120, on_air=70)
    t.field('chap_num', 'Numéro', '3', kind='number', min=1, max=20)
    t.field('chap_total', 'Nombre de chapitres', '7', kind='number', min=1, max=20)
    t.field('chap_title', 'Titre du chapitre', fr('Les années Morricone'))
    t.field('chap_time', 'Minutage', '24:10')
    dataset(t, 'chap', 'Chapitres', 'horschamp-chapitres', 'chapters')
    g = frame(t, 'Chapitre', 96, 790, 1000, 200)
    card = t.group('Carte', 0, 0, 1000, 200, g, clip=True, tracks=wipe(70, length=22))
    t.rect('Fond', 0, 0, 1000, 200, hexc(PANEL, 0.95), card, radius=28, stroke=(hexc('#ffffff', 0.08), 1.5))
    t.rect('Case', 0, 0, 200, 200, hexc(CORAL), card, corners=(28, 0, 0, 28))
    num = t.group('Numéro', 0, 0, 200, 200, card, clip=True)
    t.text('Chiffre', '{{chap_num}}', 0, 0, 200, 200, 150, num, color=hexc(BG), family=GROTESK, weight=700, align='center',
           tracks=inout('y', 0, 200, 70, 8, 22))
    t.text('Surtitre', fr('CHAPITRE {{chap_num}} / {{chap_total}}   ·   {{chap_time}}'), 236, 34, 720, 34, 20, card, color=hexc(SAND),
           weight=700, spacing=3, tracks=fade(70, 12, 12))
    t.text('Titre', '{{chap_title}}', 234, 66, 730, 78, 54, card, color=hexc(CREAM), family=GROTESK, weight=700,
           tracks=merge(inout('x', 234, 300, 70, 10, 24), fade(70, 10, 12)))
    t.progress('Avancée', '{{chap_num}}', 238, 156, 420, 8, hexc(CORAL), card, max='{{chap_total}}', track=hexs(CREAM, 0.16),
               duration=1.0, shape='segments', segments=7, segGap=8.0, tracks=fade(70, 18, 10),
               settingBindings={'segments': 'chap_total'})
    place(t, g, 50, 1200, 0.98)

    # --- Subscribe -----------------------------------------------------
    t = p.template('cta', 'Abonnez-vous', 'cta', duration=150, on_air=110)
    t.field('channel', 'Chaîne', 'Hors Champ')
    t.field('subs', 'Abonnés', fr('248 k abonnés'))
    g = frame(t, 'Abonnement', 1150, 810, 710, 176)
    c = t.group('Carte', 0, 0, 710, 176, g, tracks=merge(inout('y', 0, 60, 110, 0, 22), fade(110, 0, 12)))
    t.rect('Fond', 0, 0, 710, 176, hexc(PANEL, 0.95), c, radius=30, stroke=(hexc('#ffffff', 0.08), 1.5),
           shadow=(hexc('#000000', 0.45), 0, 14, 40))
    t.ellipse('Logo', 26, 38, 100, 100, hexc(CORAL), c, grad=('radial', '#d23a20'))
    tri(t, 'Lecture', 64, 70, 34, 36, hexc(CREAM), 'right', c)
    t.text('Chaîne', '{{channel}}', 148, 40, 280, 50, 34, c, color=hexc(CREAM), family=GROTESK, weight=700)
    t.text('Abonnés', '{{subs}}', 148, 92, 280, 36, 22, c, color=hexc(SAND), weight=500)
    press = keys((0, 1, 'hold'), (40, 1, SMOOTH), (45, 0.9, SMOOTH), (52, 1, 'linear'))
    btn = t.group('Bouton', 430, 58, 172, 60, c, tracks={'scaleX': press, 'scaleY': [dict(k) for k in press]})
    t.rect('Rouge', 0, 0, 172, 60, hexc(CORAL), btn, radius=30, tracks={'opacity': keys((0, 1, 'hold'), (44, 1, 'linear'), (48, 0, 'linear'))})
    t.rect('Gris', 0, 0, 172, 60, hexc('#3a3340'), btn, radius=30, tracks={'opacity': keys((0, 0, 'hold'), (44, 0, 'linear'), (48, 1, 'linear'))})
    t.text('S’abonner', fr("S'ABONNER"), 0, 0, 172, 60, 20, btn, weight=800, align='center', spacing=1,
           tracks={'opacity': keys((0, 1, 'hold'), (44, 1, 'linear'), (47, 0, 'linear'))})
    t.text('Abonné', fr('ABONNÉ'), 30, 0, 142, 60, 20, btn, weight=800, align='center', spacing=1, color=hexc(CREAM, 0.85),
           tracks={'opacity': keys((0, 0, 'hold'), (45, 0, 'linear'), (49, 1, 'linear'))})
    poly(t, 'Coche', 22, 22, 20, 16, hexc(CREAM, 0.85), CHECK, btn, tracks={'opacity': keys((0, 0, 'hold'), (45, 0, 'linear'), (49, 1, 'linear'))})
    bell = t.group('Cloche', 616, 58, 60, 60, c)
    t.ellipse('Cercle', 0, 0, 60, 60, hexc('#ffffff', 0.06), bell, stroke=(hexc(CREAM, 0.3), 2))
    ring = t.group('Balancier', 14, 13, 32, 34, bell, anchorY=0.0,
                   tracks={'rotation': keys((0, 0, 'hold'), (54, 0, SMOOTH), (58, 18, SMOOTH), (62, -16, SMOOTH), (66, 12, SMOOTH),
                                            (70, -8, SMOOTH), (74, 4, SMOOTH), (78, 0, 'linear'))})
    poly(t, 'Cloche', 0, 0, 32, 30, hexc(CREAM), BELL, ring, round_=3)
    t.ellipse('Battant', 12, 27, 8, 7, hexc(CREAM), ring)
    place(t, g, 70, 870, 1.32)

    # --- Like ----------------------------------------------------------
    t = p.template('like', 'J’aime', 'cta', duration=110, on_air=70)
    t.field('likes', 'J’aime', '12480', kind='number')
    t.field('text', 'Texte', fr('Si l’épisode vous plaît'))
    g = frame(t, 'J’aime', 1320, 860, 540, 120)
    c = t.group('Pilule', 0, 0, 540, 120, g, tracks=merge(inout('y', 0, 50, 70, 0, 20), fade(70, 0, 10)))
    t.rect('Fond', 0, 0, 540, 120, hexc(PANEL, 0.95), c, radius=60, stroke=(hexc('#ffffff', 0.08), 1.5),
           shadow=(hexc('#000000', 0.45), 0, 14, 40))
    t.ellipse('Disque', 14, 14, 92, 92, hexc(CORAL), c)
    th = t.group('Pouce', 36, 34, 48, 48, c, tracks=merge(pop(70, 8, 16),
                                                          {'rotation': keys((0, -25, 'hold'), (8, -25, 'back'), (26, 0, 'linear'))}))
    t.rect('Manche', 0, 20, 10, 28, hexc(CREAM), th, radius=2)
    poly(t, 'Main', 13, 0, 35, 48, hexc(CREAM), THUMB, th, round_=3)
    t.text('Texte', '{{text}}', 128, 18, 390, 40, 22, c, color=hexc(SAND), weight=500)
    t.counter('Compteur', '{{likes}}', 128, 52, 390, 52, 38, c, color=hexc(CREAM), family=GROTESK, weight=700, duration=1.6,
              suffix=fr(' j’aime'), compact=True, decimals=1, locale='fr-FR')
    place(t, g, 110, 900, 1.6)

    # --- End screen ---------------------------------------------------
    t = p.template('end', 'Écran de fin', 'full', duration=130, on_air=80)
    t.field('title', 'Titre', fr('Merci d’avoir regardé'))
    t.field('sub', 'Sous-titre', fr('Abonnez-vous pour ne rien manquer du prochain épisode'))
    t.field('left', 'Case de gauche', fr('ÉPISODE SUIVANT'))
    t.field('right', 'Case de droite', 'TOUTE LA SAISON')
    t.field('channel', 'Chaîne', 'Hors Champ')
    bg = t.group('Fond', 0, 0, 1920, 1080, tracks=fade(80, 0, 14, 14))
    t.rect('Aplat', 0, 0, 1920, 1080, hexc(BG), bg)
    t.ellipse('Halo', -380, -520, 1500, 1300, hexc(CORAL, 0.22), bg, grad=('radial', hexs(BG, 0)),
              tracks={'x': keys((0, -480, SMOOTH), (130, -330, 'linear'))})
    t.ellipse('Halo 2', 1180, 560, 1100, 900, hexc('#7a5cff', 0.12), bg, grad=('radial', hexs(BG, 0)))
    t.text('Titre', '{{title}}', 120, 112, 1300, 110, 88, bg, color=hexc(CREAM), family=GROTESK, weight=700,
           tracks=merge(inout('y', 112, 160, 80, 6, 22), fade(80, 6, 14)))
    t.text('Sous-titre', '{{sub}}', 122, 222, 1300, 50, 30, bg, color=hexc(SAND), weight=500,
           tracks=merge(inout('y', 222, 262, 80, 10, 22), fade(80, 10, 14)))
    for i, (x, key) in enumerate(((120, 'left'), (800, 'right'))):
        s = t.group('Case ' + str(i + 1), x, 330, 640, 420, bg, tracks=merge(inout('y', 330, 400, 80, 14 + i * 5, 24), fade(80, 14 + i * 5, 14)))
        t.text('Étiquette', '{{%s}}' % key, 0, 0, 640, 40, 20, s, color=hexc(CORAL), weight=800, spacing=3)
        dashed(t, t.rect('Cadre', 0, 56, 640, 360, hexc('#ffffff', 0.04), s, radius=18, stroke=(hexc(CREAM, 0.35), 2)))
        tri(t, 'Lecture', 300, 214, 44, 48, hexc(CREAM, 0.3), 'right', s)
    sub = t.group('Abonnement', 1530, 386, 300, 360, bg, tracks=pop(80, 26, 18))
    dashed(t, t.ellipse('Cercle', 0, 0, 300, 300, hexc('#ffffff', 0.04), sub, stroke=(hexc(CREAM, 0.35), 2)))
    t.text('Légende', fr("S'ABONNER"), 0, 316, 300, 40, 20, sub, color=hexc(CORAL), weight=800, spacing=3, align='center')
    foot = t.group('Pied', 120, 880, 1700, 80, bg, tracks=fade(80, 24, 14))
    t.ellipse('Logo', 0, 6, 68, 68, hexc(CORAL), foot)
    tri(t, 'Lecture', 26, 28, 22, 24, hexc(CREAM), 'right', foot)
    t.text('Chaîne', '{{channel}}', 88, 0, 600, 80, 30, foot, color=hexc(CREAM), family=GROTESK, weight=700)
    t.clock('Décompte', 1300, 0, 400, 80, 26, foot, mode='countdown', fmt='m:ss', color=hexc(SAND), weight=600, align='right',
            cdDuration=20000, startMode='auto', prefix=fr('FIN DANS '), affix={'size': 80.0, 'color': SAND, 'weight': 700, 'upper': True})
    # 9:16: one column.
    L = t.t['layers']
    byname = lambda n: next(l['id'] for l in L if l['name'] == n)
    t.override(bg, 'vertical', width=1080.0, height=1920.0)
    t.override(byname('Aplat'), 'vertical', width=1080.0, height=1920.0)
    t.override(byname('Titre'), 'vertical', x=70.0, y=200.0, width=940.0, height=200.0, fontSize=84.0, tracks=inout('y', 200, 250, 80, 6, 22))
    t.layer_ref(byname('Titre'))['wrap'] = True
    t.override(byname('Sous-titre'), 'vertical', x=72.0, y=400.0, width=900.0, height=90.0, fontSize=32.0, tracks=inout('y', 400, 440, 80, 10, 22))
    t.layer_ref(byname('Sous-titre'))['wrap'] = True
    t.override(byname('Case 1'), 'vertical', x=220.0, y=560.0, tracks=inout('y', 560, 620, 80, 14, 24))
    t.override(byname('Case 2'), 'vertical', x=220.0, y=1020.0, tracks=inout('y', 1020, 1080, 80, 19, 24))
    t.override(byname('Abonnement'), 'vertical', visible=False)
    t.override(byname('Pied'), 'vertical', x=70.0, y=1700.0)
    t.override(byname('Décompte'), 'vertical', x=340.0, y=0.0)
    p.item(fr('Épisode 42'), 'bug', 'show')
    p.item('Réseaux', 'social', 'show')
    p.item('Camille Martin', 'guest', 'show')
    p.item('Julien Roy', 'guest', 'show', {'name': 'Julien Roy', 'role': fr('Monteur son, studio Saint-Ouen'), 'handle': '@julienroy', 'initials': 'JR'})
    p.item(fr('Chapitre 1 · L’enfance au piano'), 'chapter', 'show', {'chap_num': '1', 'chap_title': fr('L’enfance au piano'), 'chap_time': '02:15'})
    p.item(fr('Chapitre 3 · Les années Morricone'), 'chapter', 'show')
    p.item('Abonnez-vous', 'cta', 'show')
    p.item('J’aime', 'like', 'show')
    p.item('Écran de fin', 'end', 'end')
    p.source('horschamp-chapitres', 'Hors Champ · chapitres', {'chapters': [
        {'num': '1', 'total': '7', 'title': fr('L’enfance au piano'), 'time': '02:15'},
        {'num': '2', 'total': '7', 'title': fr('Le conservatoire'), 'time': '11:40'},
        {'num': '3', 'total': '7', 'title': fr('Les années Morricone'), 'time': '24:10'},
        {'num': '4', 'total': '7', 'title': fr('Écrire pour l’image'), 'time': '37:05'},
        {'num': '5', 'total': '7', 'title': fr('Hollywood, puis retour'), 'time': '49:30'},
        {'num': '6', 'total': '7', 'title': fr('La musique des séries'), 'time': '58:12'},
        {'num': '7', 'total': '7', 'title': fr('Vos questions'), 'time': '1:06:40'}]})
    return p


# =====================================================================
# 12 · Festival live — « Rives Électriques »
# Black, acid lime and white; Bebas Neue + Barlow Condensed; slanted
# slabs, hard cuts and fast slams — read from the back of the crowd.
# =====================================================================

def ex12():
    K, LIME, WHT, GREY = '#0a0a0a', '#d4ff3a', '#ffffff', '#9a9a9a'
    p = Package('ex-12-festival-live', 'Festival en direct', 'Musique',
                ['Concert', 'Festival', 'Programme', 'Compte à rebours', 'Partenaire', '9:16'],
                fr("La diffusion en direct d’un festival : synthé artiste et titre en très grand, présentation plein écran de l’artiste, "
                   "programme de la soirée (tableau que la régie remplit ou importe), prochain concert avec compte à rebours, "
                   "bug de scène avec l’heure et partenaire en rotation. Style franc et lisible de loin, déclinable en 9:16."),
                en=('Live festival', 'A festival live stream: very large artist and song strap, full screen artist intro, the night’s '
                    'line-up (a table typed or imported), next show with its countdown, a stage bug with the time and rotating sponsors. '
                    'Bold and readable from afar, with a 9:16 layout.'),
                cover='artist')
    p.vertical()
    p.zone('stage', 'Scène', 30, '#d4ff3a', area=(0.025, 0.035, 0.42, 0.09), vertical=(0.04, 0.05, 0.92, 0.06))
    p.zone('sponsor', 'Partenaire', 31, '#f5a524', area=(0.68, 0.035, 0.3, 0.09), vertical=(0.04, 0.12, 0.92, 0.066))
    p.zone('lower', 'Synthé', 10, '#7c5cff', area=(0.03, 0.6, 0.7, 0.37), vertical=(0.03, 0.58, 0.94, 0.2))
    p.zone('next', 'Prochain concert', 20, '#22c55e', area=(0.65, 0.66, 0.33, 0.31), vertical=(0.06, 0.36, 0.88, 0.24))
    p.zone('full', 'Plein écran', 50, '#38bdf8')
    p.playlist('night', fr('Samedi · Scène Éclipse'))
    p.guides(action_safe=0.035, title_safe=0.05)

    # --- Stage bug ----------------------------------------------------
    t = p.template('stage', 'Scène', 'stage', duration=80, on_air=40)
    t.field('stage', 'Scène', fr('SCÈNE ÉCLIPSE'))
    g = frame(t, 'Scène', 60, 48, 760, 74)
    a = t.group('Bloc', 0, 0, 760, 74, g, clip=True, tracks=wipe(40, length=14))
    t.rect('Live', 0, 0, 132, 74, hexc(LIME), a, shape='parallelogram', skew=-16)
    t.ellipse('Point', 26, 29, 16, 16, hexc(K), a, tracks=pulse('opacity', 0.25, 1, 40, 80, start=10))
    t.text('LIVE', 'LIVE', 50, 2, 74, 74, 40, a, color=hexc(K), family=DISPLAY, weight=400, spacing=1)
    t.rect('Plaque', 120, 0, 640, 74, hexc(K, 0.9), a, shape='parallelogram', skew=-16)
    t.text('Scène', '{{stage}}', 162, 0, 400, 74, 34, a, color=hexc(WHT), family=COND, weight=700, spacing=3, upper=True)
    t.clock('Heure', 560, 0, 160, 74, 36, a, fmt='HH:mm', color=hexc(LIME), family=COND, weight=700, align='right')
    place(t, g, 43, 96, 1.2)

    # --- Sponsor ------------------------------------------------------
    t = p.template('sponsor', 'Partenaire', 'sponsor', duration=80, on_air=40)
    t.field('sponsors', 'Partenaires (un par ligne)', 'RADIO ONDE\nBRASSERIE DU QUAI\nVILLE DE NANTES', kind='multiline')
    g = frame(t, 'Partenaire', 1330, 48, 530, 74)
    a = t.group('Bloc', 0, 0, 530, 74, g, clip=True, tracks=wipe(40, prop='cropLeft', length=14))
    t.rect('Plaque', 0, 0, 530, 74, hexc(K, 0.9), a, shape='parallelogram', skew=-16)
    t.text('Présenté par', fr('PRÉSENTÉ\nPAR'), 38, 0, 120, 74, 17, a, color=hexc(LIME), family=COND, weight=700, spacing=2, lineHeight=1.05)
    t.rect('Filet', 150, 18, 3, 38, hexc(LIME), a)
    t.ticker('Noms', '{{sponsors}}', 170, 0, 340, 74, 34, a, color=hexc(WHT), family=COND, weight=800, mode='flip', itemDur=4.0,
             transDur=0.35, trans='push')
    place(t, g, 43, 230, 1.6)

    # --- Artist strap -------------------------------------------------
    t = p.template('artist', 'Artiste et titre', 'lower', duration=110, on_air=60)
    t.field('artist', 'Artiste', 'IBEYI')
    t.field('song', 'Titre', fr('River · extrait de « Ash »'))
    t.field('stage', 'Scène', fr('SCÈNE ÉCLIPSE'))
    g = frame(t, 'Artiste', 80, 700, 1250, 300)
    tag = t.group('Étiquette', 0, 0, 900, 46, g, clip=True, tracks=wipe(60, length=10, delay_out=6))
    t.rect('Live', 0, 0, 104, 46, hexc(WHT), tag, shape='parallelogram', skew=-12)
    t.text('Live', 'LIVE', 0, 0, 104, 46, 26, tag, color=hexc(K), family=COND, weight=800, align='center', spacing=2)
    t.text('Scène', '{{stage}}', 122, 0, 700, 46, 26, tag, color=hexc(WHT), family=COND, weight=700, spacing=4, upper=True,
           shadow=True, shadowColor='#00000099', shadowBlur=10.0, shadowY=2.0)
    t.text('Artiste', '{{artist}}', 0, 56, 1250, 156, 150, g, color=hexc(K), family=DISPLAY, weight=400, lineHeight=1.0, spacing=2,
           upper=True, tracks=merge(wipe(60, t_in=4, length=12, delay_out=3), inout('x', 0, -60, 60, 4, 16, delay_out=3)),
           **band(LIME, pad=34, gap=0))
    t.text('Titre', '{{song}}', 0, 216, 1250, 70, 40, g, color=hexc(WHT), family=COND, weight=600, lineHeight=1.6,
           tracks=merge(wipe(60, t_in=12, length=14), inout('y', 216, 236, 60, 12, 14)), **band(hexs(K, 0.92), pad=34, gap=0))
    place(t, g, 40, 1130, 0.8)

    # --- Full screen intro ----------------------------------------------
    t = p.template('intro', 'Artiste plein écran', 'full', duration=150, on_air=100)
    t.field('artist', 'Artiste', 'IBEYI')
    t.field('time', 'Horaire', '22:15 – 23:30')
    t.field('stage', 'Scène', fr('SCÈNE ÉCLIPSE'))
    t.field('line', 'Ligne', fr('Nouvel album « Ash » · Première date de la tournée'))
    g = frame(t, 'Intro', 0, 0, 1920, 1080)
    v = t.rect('Voile', 0, 0, 1920, 1080, hexc(K, 0.92), g, grad=(hexs(K, 0.15), 0), tracks=fade(100, 0, 12, 14))
    vfull(t, g, [v], 0.5625)
    for i, (y, h, d) in enumerate(((330, 14, 0), (362, 6, 4), (980, 10, 8))):
        t.rect('Trait %d' % (i + 1), -200, y, 2400, h, hexc(LIME), g, shape='parallelogram', skew=-30,
               tracks=merge(inout('x', -200, -2600, 100, d, 22, delay_out=0), fade(100, d, 4)))
    t.text('Surtitre', '{{stage}}   ·   {{time}}', 120, 420, 1700, 56, 38, g, color=hexc(LIME), family=COND, weight=700, spacing=8, upper=True,
           tracks=merge(fade(100, 12, 10), inout('x', 120, 60, 100, 12, 20)))
    clip = t.group('Nom', 100, 470, 1720, 300, g, clip=True)
    t.text('Artiste', '{{artist}}', 0, 0, 1720, 300, 300, clip, color=hexc(WHT), family=DISPLAY, weight=400, spacing=6, upper=True,
           lineHeight=1.0, tracks=inout('y', 0, 310, 100, 8, 22))
    t.rect('Soulignement', 124, 790, 260, 12, hexc(LIME), g, tracks=inout('width', 260, 0, 100, 20, 20))
    t.text('Ligne', '{{line}}', 120, 820, 1600, 60, 36, g, color=hexc(WHT, 0.85), family=COND, weight=500, spacing=1,
           tracks=merge(fade(100, 24, 12), inout('y', 820, 850, 100, 24, 18)))

    # --- Line-up -------------------------------------------------------
    t = p.template('lineup', 'Programme', 'full', duration=170, on_air=110)
    t.field('day', 'Jour', 'SAMEDI 12 JUILLET')
    t.field('title', 'Titre', 'CE SOIR')
    t.field('sets', 'Programme', table_value([
        ['Heure', 'Artiste', 'Scène'], ['19:00', 'Lou Doillon', 'Rivage'], ['20:10', 'Bon Entendeur', 'Éclipse'],
        ['21:00', 'Yseult', 'Rivage'], ['22:15', 'Ibeyi', 'Éclipse'], ['23:40', 'Justice (DJ set)', 'Éclipse'],
        ['01:00', 'Kiddy Smile', 'La Serre'], ['02:15', 'Jennifer Cardini', 'La Serre']]), kind='table')
    g = frame(t, 'Programme', 0, 0, 1920, 1080)
    v = t.rect('Voile', 0, 0, 1920, 1080, hexc(K, 0.94), g, tracks=fade(110, 0, 12, 14, delay_out=10))
    vfull(t, g, [v], 0.5625)
    t.rect('Bande', 120, 120, 14, 170, hexc(LIME), g, tracks=inout('height', 170, 0, 110, 4, 16))
    t.text('Titre', '{{title}}', 160, 104, 900, 160, 170, g, color=hexc(LIME), family=DISPLAY, weight=400, lineHeight=1.0, spacing=3,
           tracks=merge(wipe(110, t_in=6, length=16)))
    t.text('Jour', '{{day}}', 164, 252, 900, 50, 34, g, color=hexc(WHT), family=COND, weight=700, spacing=8,
           tracks=fade(110, 14, 12))
    t.text('Lieu', fr('NANTES · ÎLE DE VERSAILLES'), 1000, 252, 800, 50, 26, g, color=hexc(GREY), family=COND, weight=600, spacing=6,
           align='right', tracks=fade(110, 14, 12))
    t.widget('table', 'Horaires', 120, 330, 1680, 660, g, {'fontSize': 40.0}, tracks=fade(110, 10, 6, 10, delay_out=4),
             link='{{sets}}', headerRow=True, rowH=80.0, pad=28.0, gap=6.0, headSize=24.0, perPage=7, indicator=False,
             widthMode='percent', radius=0.0,
             cols=[{'src': 'Heure', 'label': 'HEURE', 'pct': 16.0, 'align': 'left'},
                   {'src': 'Artiste', 'label': 'ARTISTE', 'pct': 56.0, 'align': 'left'},
                   {'src': 'Scène', 'label': fr('SCÈNE'), 'pct': 28.0, 'align': 'right'}],
             st={'header': {'bg': '#00000000', 'color': LIME}, 'hcol': {'bg': '#1a1a1a', 'color': WHT},
                 'odd': {'bg': '#161616', 'color': WHT}, 'even': {'bg': '#1f1f1f', 'color': WHT},
                 'hl': {'bg': LIME, 'color': K}, 'top3': {'bg': '#161616', 'color': WHT}, 'tableBg': '#00000000'},
             borders={'outer': {'on': False, 'width': 0.0, 'color': '#00000000'}, 'innerH': {'on': False, 'width': 1.0, 'color': '#ffffff14'},
                      'innerV': {'on': False, 'width': 1.0, 'color': '#00000000'}},
             cf={'neg': False, 'top3': False, 'topBar': False, 'hlBar': False},
             txt={'title': {}, 'header': {'family': COND, 'weight': 700, 'spacing': 4.0, 'case': 'upper'},
                  'rows': {'family': COND, 'weight': 700, 'case': 'upper', 'spacing': 1.0, 'size': 40.0}},
             hlKey='22:15', cascade={'on': True, 'style': 'slide', 'offset': 70.0, 'dur': 450.0})

    # --- Next show -------------------------------------------------------
    t = p.template('next', 'Prochain concert', 'next', duration=110, on_air=60)
    t.field('prog_artist', 'Artiste', 'JUSTICE (DJ SET)')
    t.field('prog_stage', 'Scène', fr('SCÈNE ÉCLIPSE'))
    t.field('prog_time', 'Heure de début', '23:40:00')
    dataset(t, 'prog', 'Programme du festival', 'festival-programme', 'sets')
    g = frame(t, 'Prochain', 1270, 740, 580, 260)
    c = t.group('Carte', 0, 0, 580, 260, g, clip=True, tracks=wipe(60, prop='cropTop', length=16))
    t.rect('Fond', 0, 0, 580, 260, hexc(LIME), c)
    t.text('Surtitre', 'PROCHAIN CONCERT', 34, 22, 520, 34, 24, c, color=hexc(K), family=COND, weight=800, spacing=5)
    t.text('Artiste', '{{prog_artist}}', 32, 56, 520, 104, 92, c, color=hexc(K), family=DISPLAY, weight=400, spacing=1, upper=True,
           tracks=merge(inout('y', 56, 120, 60, 8, 18), fade(60, 8, 8)))
    t.rect('Bas', 0, 176, 580, 84, hexc(K), c)
    t.text('Scène', '{{prog_stage}}', 34, 176, 300, 84, 26, c, color=hexc(WHT), family=COND, weight=700, spacing=3, upper=True)
    t.text('Dans', 'DANS', 230, 176, 136, 84, 22, c, color=hexc(GREY), family=COND, weight=700, spacing=3, align='right')
    t.clock('Décompte', 376, 176, 180, 84, 46, c, mode='countdown', fmt='H:mm:ss', color=hexc(LIME), family=COND, weight=800,
            align='right', cdTarget='time', tTime='23:40:00', hoursAuto=True, startMode='auto', settingBindings={'tTime': 'prog_time'})
    place(t, g, 110, 760, 1.48)

    p.item(fr('Bug Scène Éclipse'), 'stage', 'night')
    p.item('Partenaires', 'sponsor', 'night')
    p.item('Programme de la nuit', 'lineup', 'night')
    p.item(fr('Ibeyi · plein écran'), 'intro', 'night')
    p.item('Ibeyi · River', 'artist', 'night')
    p.item('Ibeyi · Deathless', 'artist', 'night', {'song': fr('Deathless · avec Kamasi Washington')})
    p.item('Prochain : Justice', 'next', 'night')
    p.item('Justice · Genesis', 'artist', 'night', {'artist': 'JUSTICE', 'song': 'Genesis (DJ set)'})
    p.source('festival-programme', 'Rives Électriques · programme', {'sets': [
        {'artist': 'JUSTICE (DJ SET)', 'stage': fr('SCÈNE ÉCLIPSE'), 'time': '23:40:00'},
        {'artist': 'KIDDY SMILE', 'stage': 'LA SERRE', 'time': '01:00:00'},
        {'artist': 'JENNIFER CARDINI', 'stage': 'LA SERRE', 'time': '02:15:00'}]})
    return p


# =====================================================================
# 13 · Documentaire / culture — « La Mémoire des pierres »
# Ivory on warm black, a muted gold hairline; Playfair Display over
# spaced Inter capitals; no plates, slow dissolves and tracking.
# =====================================================================

def ex13():
    INK, IVORY, GOLD, MUTE = '#0e0c0a', '#f3ede1', '#c8a96a', '#b8ad9c'
    p = Package('ex-13-documentaire', 'Documentaire', 'Culture',
                ['Documentaire', 'Citation', 'Archives', 'Chapitres', 'Carte', 'Générique'],
                fr("Un habillage de documentaire tout en retenue : titre du film, cartons de chapitre en chiffres romains, citations, "
                   "légendes d’archives tapées à la machine (date et lieu), intervenants sans cartouche, carte de situation hors ligne "
                   "et générique de fin en pages. Serif élégant, filets dorés, fondus lents."),
                en=('Documentary', 'A restrained documentary package: film title, chapter cards with roman numerals, quotes, typewritten '
                    'archive captions (date and place), plate-free name straps, an offline location map and paged end credits. '
                    'Elegant serif, gold hairlines, slow dissolves.'),
                cover='title')
    p.zone('caption', 'Légende d’archive', 30, '#c8a96a', area=(0.04, 0.05, 0.55, 0.14))
    p.zone('lower', 'Intervenant', 10, '#7c5cff', area=(0.04, 0.68, 0.62, 0.28))
    p.zone('map', 'Carte', 20, '#22c55e', area=(0.66, 0.06, 0.31, 0.6))
    p.zone('full', 'Plein écran', 50, '#38bdf8')
    p.playlist('film', 'Le film')
    p.playlist('end', 'Générique')
    p.guides(action_safe=0.035, title_safe=0.05)
    SH = dict(shadow=True, shadowColor='#000000b0', shadowBlur=26.0, shadowY=2.0)

    # --- Film title ---------------------------------------------------
    t = p.template('title', 'Titre du film', 'full', duration=200, on_air=140)
    t.field('by', 'Surtitre', 'UN FILM DE CLAIRE DUVAL')
    t.field('title', 'Titre', 'La Mémoire des pierres')
    t.field('sub', 'Sous-titre', fr('Carnac, six mille ans de mystère'))
    g = t.group('Titre', 0, 0, 1920, 1080)
    t.rect('Voile', 0, 0, 1920, 1080, hexc(INK, 0.55), g, tracks=fade(140, 0, 40, 30))
    t.ellipse('Vignette', -480, -600, 2880, 2280, hexc(INK, 0), g, grad=('radial', hexs(INK, 0.85)), tracks=fade(140, 0, 40, 30))
    t.text('Surtitre', '{{by}}', 0, 360, 1920, 40, 22, g, color=hexc(IVORY, 0.8), weight=500, align='center', spacing=10,
           tracks=fade(140, 20, 30, 24))
    t.text('Titre', '{{title}}', 0, 410, 1920, 170, 124, g, color=hexc(IVORY), family=SERIF, weight=600, align='center',
           tracks=merge(fade(140, 30, 40, 30), inout('letterSpacing', 2, 16, 140, 30, 110, 30)), **SH)
    t.rect('Filet', 900, 612, 120, 1.5, hexc(GOLD), g, tracks=inout('scaleX', 1, 0, 140, 50, 40, 24))
    t.text('Sous-titre', '{{sub}}', 0, 640, 1920, 50, 32, g, color=hexc(MUTE), family=SERIF, weight=600, align='center', italic=True,
           tracks=fade(140, 60, 30, 24), **SH)

    # --- Chapter card -------------------------------------------------
    t = p.template('chapter', 'Chapitre', 'full', duration=170, on_air=120)
    t.field('num', 'Numéro (romain)', 'III')
    t.field('title', 'Titre', fr('Les bâtisseurs d’avant l’écriture'))
    t.field('years', 'Période', '– 4500 · – 2500')
    g = t.group('Chapitre', 0, 0, 1920, 1080)
    t.rect('Fond', 0, 0, 1920, 1080, hexc(INK, 0.86), g, tracks=fade(120, 0, 24, 24))
    t.rect('Filet G', 720, 397, 150, 1.5, hexc(GOLD), g, anchorX=1.0, tracks=inout('scaleX', 1, 0, 120, 16, 34))
    t.rect('Filet D', 1050, 397, 150, 1.5, hexc(GOLD), g, anchorX=0.0, tracks=inout('scaleX', 1, 0, 120, 16, 34))
    t.text('Numéro', '{{num}}', 860, 340, 200, 110, 72, g, color=hexc(GOLD), family=SERIF, weight=600, align='center',
           tracks=fade(120, 6, 24, 20))
    t.text('Titre', '{{title}}', 260, 470, 1400, 200, 76, g, color=hexc(IVORY), family=SERIF, weight=600, align='center', wrap=True,
           lineHeight=1.15, tracks=merge(fade(120, 20, 30, 20), inout('y', 470, 490, 120, 20, 50)))
    t.text('Période', '{{years}}', 0, 690, 1920, 40, 22, g, color=hexc(MUTE), weight=500, align='center', spacing=8,
           tracks=fade(120, 40, 24, 16))

    # --- Quote ----------------------------------------------------------
    t = p.template('quote', 'Citation', 'full', duration=260, on_air=200)
    t.field('quote', 'Citation', fr("Les pierres ne parlent pas, mais elles se souviennent de tout ce qu'on leur a confié."), kind='multiline')
    t.field('author', 'Auteur', 'Prosper Mérimée')
    t.field('source', 'Source', fr('Notes d’un voyage dans l’Ouest de la France, 1836'))
    g = t.group('Citation', 0, 0, 1920, 1080)
    t.rect('Fond', 0, 0, 1920, 1080, hexc(INK, 0.84), g, tracks=fade(200, 0, 24, 24))
    t.text('Guillemet', '«', 260, 150, 200, 200, 220, g, color=hexc(GOLD, 0.9), family=SERIF, weight=600, tracks=fade(200, 8, 24, 16))
    t.text('Citation', '{{quote}}', 380, 300, 1160, 380, 58, g, color=hexc(IVORY), family=SERIF, weight=600, wrap=True, lineHeight=1.3,
           valign='top', tracks=merge(fade(200, 16, 36, 20), inout('y', 300, 320, 200, 16, 60)))
    t.rect('Filet', 384, 724, 80, 1.5, hexc(GOLD), g, anchorX=0.0, tracks=inout('scaleX', 1, 0, 200, 40, 30))
    t.text('Auteur', '{{author}}', 384, 748, 1100, 44, 26, g, color=hexc(GOLD), weight=600, spacing=6, upper=True, tracks=fade(200, 46, 24))
    t.text('Source', '{{source}}', 384, 794, 1100, 40, 26, g, color=hexc(MUTE), family=SERIF, weight=600, italic=True, tracks=fade(200, 52, 24))

    # --- Archive caption ---------------------------------------------
    t = p.template('archive', 'Légende d’archive', 'caption', duration=200, on_air=150)
    t.field('date', 'Date', '14 AOÛT 1953')
    t.field('place', 'Lieu', fr('Alignements du Ménec, Carnac'))
    t.field('credit', 'Source', 'INA · Actualités françaises')
    g = frame(t, 'Archive', 96, 66, 980, 130)
    t.rect('Cadre', 0, 0, 148, 34, hexc(INK, 0.35), g, stroke=(hexc(GOLD), 1.5), tracks=fade(150, 0, 16, 16))
    t.text('Archives', 'ARCHIVES', 0, 0, 148, 34, 16, g, color=hexc(GOLD), weight=600, spacing=5, align='center', tracks=fade(150, 0, 16, 16))
    t.widget('typewriter', 'Date et lieu', 0, 46, 980, 48, g, {'fontSize': 32.0, 'fill': hexc(IVORY)}, tracks=fade(150, 0, 1, 16),
             text='{{date}} — {{place}}', speed=26.0, cursor=True, fontFamily=MONO, fontWeight=500, align='left', verticalAlign='middle',
             uppercase=False, cursorColor=GOLD, cursorHide=True, shadow=True)
    t.text('Source', '{{credit}}', 0, 98, 980, 30, 18, g, color=hexc(MUTE), weight=500, spacing=2, tracks=fade(150, 50, 20, 16), **SH)

    # --- Interviewee --------------------------------------------------
    t = p.template('lower', 'Intervenant', 'lower', duration=170, on_air=110)
    t.field('name', 'Nom', 'Hélène Le Goff')
    t.field('role', 'Fonction', fr('Archéologue, Inrap Bretagne'))
    g = frame(t, 'Intervenant', 110, 820, 1000, 160)
    t.rect('Filet', 2, 0, 60, 1.5, hexc(GOLD), g, anchorX=0.0, tracks=inout('scaleX', 1, 0, 110, 0, 30, 20))
    t.text('Nom', '{{name}}', 0, 14, 1000, 80, 58, g, color=hexc(IVORY), family=SERIF, weight=600,
           tracks=merge(fade(110, 6, 30, 20), inout('x', 0, -24, 110, 6, 40)), **SH)
    t.text('Fonction', '{{role}}', 2, 96, 1000, 40, 22, g, color=hexc(IVORY, 0.82), weight=500, spacing=5, upper=True,
           tracks=fade(110, 20, 30, 20), **SH)

    # --- Location map -------------------------------------------------
    t = p.template('map', 'Lieu', 'map', duration=200, on_air=150)
    t.field('place', 'Lieu', 'Carnac')
    t.field('region', 'Région', 'Morbihan, Bretagne')
    t.field('coords', 'Coordonnées', fr('47°35′ N · 3°05′ O'))
    t.field('lat', 'Latitude', '47.584', kind='number')
    t.field('lon', 'Longitude', '-3.078', kind='number')
    g = frame(t, 'Lieu', 1290, 80, 560, 560)
    c = t.group('Carte', 0, 0, 560, 560, g, tracks=fade(150, 0, 30, 20))
    t.rect('Fond', 0, 0, 560, 560, hexc(INK, 0.88), c)
    t.widget('map', 'Carte', 20, 20, 520, 400, c, {}, style='mono', projection='mercator', radius=0.0, showRoads=False,
             graticule=False, view={'lat': 47.3, 'lon': -2.4, 'zoom': 6.4, 'rot': 0.0, 'tilt': 0.0}, views=[],
             colors={'water': '#14120f', 'land': '#2a2621', 'border': '#5d5446', 'grid': ''},
             layers={'markers': True, 'routes': False, 'zones': False, 'labels': True},
             settingBindings={'markers.0.name': 'place'},
             markers=[{'id': 'site', 'on': True, 'name': 'Carnac', 'lat': '{{lat}}', 'lon': '{{lon}}', 'icon': 'dot', 'color': GOLD}],
             mkSize=26.0, routes=[], labels={'cities': True, 'countries': False, 'minImp': 6.0, 'size': 18.0, 'pad': 8.0},
             attribution={'provider': 'own', 'pos': 'br', 'style': 'minimal', 'show': True})
    t.rect('Cadre', 20, 20, 520, 400, hexc(INK, 0), c, stroke=(hexc(GOLD, 0.6), 1))
    t.text('Lieu', '{{place}}', 24, 436, 520, 60, 44, c, color=hexc(IVORY), family=SERIF, weight=600)
    t.text('Région', '{{region}}', 26, 496, 300, 30, 18, c, color=hexc(MUTE), weight=500, spacing=3, upper=True)
    t.text('Coordonnées', '{{coords}}', 300, 496, 236, 30, 18, c, color=hexc(GOLD), family=MONO, weight=500, align='right')

    # --- End credits --------------------------------------------------
    t = p.template('credits', 'Générique', 'full', duration=200, on_air=120)
    t.field('title', 'Titre', 'La Mémoire des pierres')
    E = lambda k, a, b='': {'kind': k, 'a': a, 'b': b}
    g = t.group('Générique', 0, 0, 1920, 1080, tracks=fade(120, 0, 30, 30))
    t.rect('Fond', 0, 0, 1920, 1080, hexc(INK), g)
    t.text('Titre', '{{title}}', 0, 110, 1920, 80, 48, g, color=hexc(IVORY), family=SERIF, weight=600, align='center')
    t.rect('Filet', 910, 206, 100, 1.5, hexc(GOLD), g)
    t.widget('credits', 'Générique', 360, 260, 1200, 700, g, {}, mode='pages', cardDur=5.0, layout='two', align='center',
             font=SANS, gutter=48.0, sectionGap=56.0, lineGap=14.0, panel=False, fade=False,
             st={'section': {'size': 22.0, 'color': GOLD, 'weight': 600, 'upper': True},
                 'role': {'size': 24.0, 'color': MUTE, 'weight': 500, 'upper': False},
                 'name': {'size': 30.0, 'color': IVORY, 'weight': 600, 'upper': False}},
             entries=[E('section', 'Réalisation'), E('line', 'Un film de', 'Claire Duval'),
                      E('line', 'Directeur de la photographie', 'Yann Kerhervé'), E('line', 'Cadreuse', 'Sofia Benali'),
                      E('section', 'Son'), E('line', 'Prise de son', 'Marc Ollivier'), E('line', 'Mixage', 'Studio Grande Ourse'),
                      E('section', 'Musique originale'), E('line', 'Composée par', 'Nils Leclerc'),
                      E('section', 'Archives'), E('line', 'INA', 'Actualités françaises'),
                      E('line', 'Musée de Carnac', 'Fonds Zacharie Le Rouzic')])

    p.item('Titre du film', 'title', 'film')
    p.item(fr('Archive · Le Ménec, 1953'), 'archive', 'film')
    p.item(fr('Hélène Le Goff'), 'lower', 'film')
    p.item('Carte · Carnac', 'map', 'film')
    p.item('Chapitre III', 'chapter', 'film')
    p.item(fr('Citation · Mérimée'), 'quote', 'film')
    p.item('Jean-Luc Moal', 'lower', 'film', {'name': 'Jean-Luc Moal', 'role': fr('Tailleur de pierre à Plouharnel')})
    p.item(fr('Archive · Fouilles de 1922'), 'archive', 'film', {'date': '3 JUIN 1922', 'place': fr('Tumulus Saint-Michel, Carnac'),
                                                              'credit': fr('Musée de Préhistoire de Carnac')})
    p.item(fr('Générique de fin'), 'credits', 'end')
    return p


# =====================================================================
# 14 · Conférence / keynote tech — « Horizon 26 »
# Deep navy glass, an electric blue-to-violet gradient and a mint signal
# colour; Space Grotesk with JetBrains Mono labels; precise, quick slides.
# =====================================================================

def ex14():
    NAVY, PANEL, BLUE, VIOLET, MINT, WHT, MUTE = '#070b1a', '#0f1630', '#3d7bff', '#8b5cff', '#3ef0c3', '#ffffff', '#93a0c4'
    p = Package('ex-14-keynote-tech', 'Conférence tech', 'Événement',
                ['Keynote', 'Conférence', 'Programme', 'Q&R', 'QR code', 'Graphique'],
                fr("Une keynote ou une conférence tech prête à diffuser : carton d’intervenant, session en cours avec la salle et l’heure, "
                   "programme de la journée, question du public avec QR code pour en poser, QR code vers les slides, écran d’attente "
                   "avec compte à rebours et chiffre clé avec son graphique."),
                en=('Tech keynote', 'A tech keynote or conference ready for air: speaker card, current session with room and time, the day’s '
                    'agenda, an audience question with a QR code to ask more, a QR code to the slides, a holding screen with a countdown '
                    'and a key figure with its chart.'),
                cover='speaker')
    p.zone('top', 'Session', 30, '#3ef0c3', area=(0.03, 0.035, 0.62, 0.1))
    p.zone('lower', 'Intervenant', 10, '#7c5cff', area=(0.04, 0.66, 0.62, 0.3))
    p.zone('corner', 'QR code', 20, '#22c55e', area=(0.76, 0.42, 0.21, 0.55))
    p.zone('full', 'Plein écran', 50, '#38bdf8')
    p.playlist('keynote', 'Keynote d’ouverture')
    p.guides(action_safe=0.035, title_safe=0.05)
    GLASS = dict(radius=22, stroke=(hexc('#ffffff', 0.12), 1.5), shadow=(hexc('#000000', 0.5), 0, 18, 50))

    def backdrop(t, g, on_air):
        t.rect('Fond', 0, 0, 1920, 1080, hexc(NAVY), g)
        t.ellipse('Lueur bleue', -300, 380, 1300, 1100, hexc(BLUE, 0.35), g, grad=('radial', hexs(NAVY, 0)),
                  tracks={'x': keys((0, -380, SMOOTH), (on_air + 60, -220, 'linear'))})
        t.ellipse('Lueur violette', 1100, -420, 1200, 1100, hexc(VIOLET, 0.3), g, grad=('radial', hexs(NAVY, 0)),
                  tracks={'y': keys((0, -480, SMOOTH), (on_air + 60, -360, 'linear'))})
        for i in range(1, 12):
            t.rect('Grille %d' % i, i * 160, 0, 1, 1080, hexc('#ffffff', 0.035), g)

    # --- Session bar ----------------------------------------------------
    t = p.template('session', 'Session en cours', 'top', duration=80, on_air=40)
    t.field('title', 'Session', fr("L'IA embarquée, sans le cloud"))
    t.field('room', 'Salle', 'GRAND AUDITORIUM')
    g = frame(t, 'Session', 60, 44, 1100, 76)
    a = t.group('Barre', 0, 0, 1100, 76, g, clip=True, tracks=wipe(40, length=18))
    t.rect('Fond', 0, 0, 1100, 76, hexc(PANEL, 0.9), a, radius=16, stroke=(hexc('#ffffff', 0.12), 1.5))
    t.rect('En scène', 12, 14, 150, 48, hexc(MINT), a, radius=10)
    t.text('En scène', fr('EN SCÈNE'), 12, 14, 150, 48, 18, a, color=hexc(NAVY), family=MONO, weight=700, align='center', spacing=1)
    t.text('Titre', '{{title}}', 184, 0, 620, 76, 28, a, color=hexc(WHT), family=GROTESK, weight=700, overflow='ellipsis')
    t.text('Salle', '{{room}}', 800, 0, 180, 76, 16, a, color=hexc(MUTE), family=MONO, weight=500, align='right', spacing=1)
    t.rect('Filet', 996, 22, 1.5, 32, hexc('#ffffff', 0.2), a)
    t.clock('Heure', 1000, 0, 88, 76, 24, a, fmt='HH:mm', color=hexc(MINT), family=MONO, weight=700, align='center')

    # --- Speaker ----------------------------------------------------------
    t = p.template('speaker', 'Intervenant', 'lower', duration=110, on_air=60)
    t.field('name', 'Nom', 'Inès Ferreira')
    t.field('role', 'Fonction', 'CTO, Mistral Labs')
    t.field('talk', 'Intervention', fr("L'IA embarquée, sans le cloud"))
    g = frame(t, 'Intervenant', 96, 790, 1080, 190)
    c = t.group('Carte', 0, 0, 1080, 190, g, tracks=merge(inout('y', 0, 40, 60, 0, 22), fade(60, 0, 12)))
    t.rect('Verre', 0, 0, 1080, 190, hexc(PANEL, 0.86), c, **GLASS)
    t.rect('Accent', 0, 0, 10, 190, hexc(BLUE), c, grad=(VIOLET, 180), corners=(22, 0, 0, 22),
           tracks=inout('scaleY', 1, 0, 60, 4, 20))
    t.text('Nom', '{{name}}', 46, 28, 980, 70, 54, c, color=hexc(WHT), family=GROTESK, weight=700,
           tracks=merge(fade(60, 6, 12), inout('x', 46, 90, 60, 6, 22)))
    t.text('Fonction', '{{role}}', 48, 96, 980, 36, 26, c, color=hexc(MUTE), weight=500, tracks=fade(60, 10, 12))
    t.text('Intervention', '// {{talk}}', 48, 136, 980, 34, 20, c, color=hexc(MINT), family=MONO, weight=500, tracks=fade(60, 16, 12))

    # --- QR slides ----------------------------------------------------
    t = p.template('slides', 'QR code des slides', 'corner', duration=100, on_air=60)
    t.field('url', 'Lien', 'https://horizon26.events/slides/ia-embarquee')
    t.field('label', 'Légende', 'LES SLIDES')
    t.field('short', 'Lien court', 'hzn.events/s42')
    g = frame(t, 'QR', 1480, 470, 360, 520)
    c = t.group('Carte', 0, 0, 360, 520, g, tracks=merge(inout('y', 0, 50, 60, 0, 20), fade(60, 0, 12)))
    t.rect('Verre', 0, 0, 360, 520, hexc(PANEL, 0.9), c, **GLASS)
    t.text('Légende', '{{label}}', 30, 26, 300, 30, 18, c, color=hexc(MINT), family=MONO, weight=700, spacing=2)
    t.text('Scannez', 'Scannez pour les télécharger', 30, 58, 300, 70, 26, c, color=hexc(WHT), family=GROTESK, weight=700, wrap=True,
           lineHeight=1.15, valign='top')
    t.rect('Blanc', 30, 144, 300, 300, hexc(WHT), c, radius=14)
    t.qr('QR', '{{url}}', 44, 158, 272, c, fill=hexc(NAVY), modStyle='rounded', finderStyle='rounded', quiet=1, ecl='M')
    t.text('Lien court', '{{short}}', 30, 460, 300, 36, 18, c, color=hexc(MUTE), family=MONO, weight=500, align='center')

    # --- Audience question -------------------------------------------
    t = p.template('qa', 'Question du public', 'full', duration=130, on_air=80)
    t.field('q_question', 'Question', fr('Comment garantir la confidentialité des données quand le modèle tourne sur le téléphone ?'), kind='multiline')
    t.field('q_who', 'Posée par', 'Mathis, Lyon · via l’appli')
    t.field('url', 'Lien des questions', 'https://horizon26.events/questions')
    dataset(t, 'q', 'Questions validées', 'horizon26-questions', 'questions')
    g = t.group('Q&R', 96, 600, 1728, 400)
    c = t.group('Carte', 0, 0, 1728, 400, g, tracks=merge(inout('y', 0, 60, 80, 0, 22), fade(80, 0, 12)))
    t.rect('Verre', 0, 0, 1728, 400, hexc(PANEL, 0.9), c, **GLASS)
    t.rect('Pastille', 44, 44, 220, 40, hexc(BLUE), c, radius=20, grad=(VIOLET, 90))
    t.text('Étiquette', 'QUESTION DU PUBLIC', 44, 44, 220, 40, 15, c, color=hexc(WHT), family=MONO, weight=700, align='center', spacing=1)
    t.text('Question', '{{q_question}}', 44, 106, 1280, 200, 50, c, color=hexc(WHT), family=GROTESK, weight=700, wrap=True,
           lineHeight=1.18, valign='top', tracks=merge(fade(80, 8, 14), inout('y', 106, 130, 80, 8, 22)))
    t.text('Auteur', '— {{q_who}}', 46, 320, 1200, 40, 24, c, color=hexc(MUTE), weight=500, tracks=fade(80, 16, 14))
    t.rect('Séparateur', 1380, 44, 1.5, 312, hexc('#ffffff', 0.12), c)
    t.rect('Blanc', 1430, 64, 250, 250, hexc(WHT), c, radius=12)
    t.qr('QR', '{{url}}', 1442, 76, 226, c, fill=hexc(NAVY), modStyle='rounded', finderStyle='rounded', quiet=1)
    t.text('Posez', 'POSEZ LA VÔTRE', 1404, 326, 300, 34, 16, c, color=hexc(MINT), family=MONO, weight=700, align='center', spacing=1)

    # --- Agenda -----------------------------------------------------------
    t = p.template('agenda', 'Programme', 'full', duration=170, on_air=110)
    t.field('title', 'Titre', fr('Programme · Jour 2'))
    t.field('date', 'Date', fr('Jeudi 22 octobre · Station F, Paris'))
    t.field('sessions', 'Sessions', table_value([
        ['Heure', 'Session', 'Intervenant', 'Salle'],
        ['09:30', 'Keynote d’ouverture', 'Claire Moreau', 'Auditorium'],
        ['10:15', 'L’IA embarquée, sans le cloud', 'Inès Ferreira', 'Auditorium'],
        ['11:00', 'Rust en production : retour d’expérience', 'Paul Nguyen', 'Salle Ada'],
        ['11:45', 'Concevoir pour la sobriété', 'Léa Martin', 'Salle Grace'],
        ['14:00', 'Table ronde : souveraineté numérique', '4 invités', 'Auditorium'],
        ['15:30', 'Démo : le studio de demain', 'Équipe Horizon', 'Salle Ada']]), kind='table')
    t.field('now', 'Session en cours (heure)', '10:15')
    g = t.group('Programme', 0, 0, 1920, 1080, tracks=fade(110, 0, 14, 14, delay_out=8))
    backdrop(t, g, 110)
    t.text('Surtitre', 'HORIZON 26', 120, 96, 800, 34, 20, g, color=hexc(MINT), family=MONO, weight=700, spacing=4, tracks=fade(110, 6, 12))
    t.text('Titre', '{{title}}', 116, 130, 1200, 100, 76, g, color=hexc(WHT), family=GROTESK, weight=700,
           tracks=merge(fade(110, 8, 12), inout('x', 116, 160, 110, 8, 22)))
    t.text('Date', '{{date}}', 120, 226, 1200, 40, 26, g, color=hexc(MUTE), weight=500, tracks=fade(110, 12, 12))
    t.widget('table', 'Sessions', 120, 310, 1680, 660, g, {'fontSize': 30.0}, link='{{sessions}}', headerRow=True, rowH=84.0,
             pad=28.0, gap=8.0, headSize=16.0, perPage=6, indicator=False, widthMode='percent', radius=14.0, settingBindings={'hlKey': 'now'},
             cols=[{'src': 'Heure', 'label': 'HEURE', 'pct': 11.0}, {'src': 'Session', 'label': 'SESSION', 'pct': 47.0},
                   {'src': 'Intervenant', 'label': 'INTERVENANT', 'pct': 24.0}, {'src': 'Salle', 'label': 'SALLE', 'pct': 18.0, 'align': 'right'}],
             st={'header': {'bg': '#00000000', 'color': MUTE}, 'hcol': {'bg': PANEL, 'color': WHT},
                 'odd': {'bg': hexs(PANEL, 0.85), 'color': WHT}, 'even': {'bg': hexs('#141d3d', 0.85), 'color': WHT},
                 'hl': {'bg': BLUE, 'color': WHT}, 'top3': {'bg': PANEL, 'color': WHT}, 'tableBg': '#00000000'},
             borders={'outer': {'on': False, 'width': 0.0, 'color': '#00000000'}, 'innerH': {'on': False, 'width': 1.0, 'color': '#00000000'},
                      'innerV': {'on': False, 'width': 1.0, 'color': '#00000000'}},
             cf={'neg': False, 'top3': False, 'topBar': False, 'hlBar': True, 'hlBarColor': MINT},
             txt={'title': {}, 'header': {'family': MONO, 'weight': 700, 'spacing': 2.0},
                  'rows': {'family': GROTESK, 'weight': 500, 'size': 30.0}},
             cascade={'on': True, 'style': 'slide', 'offset': 60.0, 'dur': 420.0})

    # --- Holding screen ---------------------------------------------------
    t = p.template('holding', 'Écran d’attente', 'full', duration=120, on_air=60)
    t.field('event', 'Événement', 'HORIZON 26')
    t.field('title', 'Titre', fr('La keynote commence dans'))
    t.field('place', 'Lieu', fr('Station F, Paris · 22 – 23 octobre'))
    g = t.group('Attente', 0, 0, 1920, 1080, tracks=fade(60, 0, 16, 16))
    backdrop(t, g, 60)
    t.text('Événement', '{{event}}', 0, 250, 1920, 50, 30, g, color=hexc(MINT), family=MONO, weight=700, spacing=12, align='center')
    t.text('Titre', '{{title}}', 0, 330, 1920, 90, 64, g, color=hexc(WHT), family=GROTESK, weight=700, align='center')
    t.clock('Décompte', 360, 420, 1200, 300, 260, g, mode='countdown', fmt='mm:ss', color=hexc(WHT), family=GROTESK, weight=700,
            cdDuration=300000, startMode='auto', tabular=True)
    t.progress('Barre', '0', 660, 760, 600, 6, hexc(MINT), g, max='300', track=hexs('#ffffff', 0.12), input='follow',
               follow='holding-%d' % (len(t.t['layers'])), radius=3, rounded=True, gradient=True, color2=BLUE)
    t.text('Lieu', '{{place}}', 0, 800, 1920, 40, 26, g, color=hexc(MUTE), weight=500, align='center')

    # --- Key figure -------------------------------------------------------
    t = p.template('stat', 'Chiffre clé', 'full', duration=170, on_air=110)
    t.field('value', 'Chiffre', '3.2', kind='number')
    t.field('prefix', 'Avant', '×')
    t.field('label', 'Légende', fr('plus rapide que la génération précédente, à consommation égale'), kind='multiline')
    t.field('chart', 'Graphique', table_value([['Modèle', 'Jetons / s'], ['2024', '38'], ['2025', '71'], ['2026', '122']]), kind='table')
    g = t.group('Chiffre', 0, 0, 1920, 1080, tracks=fade(110, 0, 14, 14, delay_out=6))
    backdrop(t, g, 110)
    t.text('Surtitre', 'PERFORMANCE', 140, 250, 700, 34, 20, g, color=hexc(MINT), family=MONO, weight=700, spacing=4, tracks=fade(110, 6, 12))
    t.counter('Chiffre', '{{value}}', 130, 290, 800, 300, 280, g, prefix='{{prefix}}', decimals=1, duration=1.6, color=hexc(WHT),
              family=GROTESK, weight=700, locale='fr-FR', affixSize=60.0, affixColor=MINT)
    t.text('Légende', '{{label}}', 140, 600, 700, 160, 36, g, color=hexc(MUTE), weight=500, wrap=True, lineHeight=1.3, valign='top',
           tracks=fade(110, 20, 14))
    t.widget('chart', 'Graphique', 1000, 220, 800, 640, g, {'fontSize': 24.0}, chartType='columns', source='csv', csv='{{chart}}',
             title={'text': fr('Jetons par seconde'), 'sub': fr('Sur un téléphone de milieu de gamme'), 'pos': 'left'},
             legend='none', font=SANS, textColor='#c9d2ee', panel={'on': True, 'color': PANEL, 'opacity': 0.85},
             series=[{'color': BLUE}], axis={'auto': True, 'ticks': 4, 'suffix': '', 'grid': True, 'zero': True},
             labels={'value': True, 'pos': 'outside'}, anim={'fx': 'grow', 'dur': 1.0, 'stagger': 160.0, 'upd': 0.8, 'curve': 'ease'})

    p.item('Attente · 5 min', 'holding', 'keynote')
    p.item('Programme du jour', 'agenda', 'keynote')
    p.item(fr('Session · IA embarquée'), 'session', 'keynote')
    p.item(fr('Inès Ferreira'), 'speaker', 'keynote')
    p.item('Slides', 'slides', 'keynote')
    p.item(fr('Chiffre · ×3,2'), 'stat', 'keynote')
    p.item('Question de Mathis', 'qa', 'keynote')
    p.source('horizon26-questions', 'Horizon 26 · questions validées', {'questions': [
        {'question': fr('Comment garantir la confidentialité des données quand le modèle tourne sur le téléphone ?'),
         'who': 'Mathis, Lyon · via l’appli'},
        {'question': fr('Quelle consommation d’énergie pour une heure d’usage intensif ?'), 'who': 'Clara, Nantes · via l’appli'},
        {'question': fr('Le modèle sera-t-il publié en open source ?'), 'who': 'Yanis, Paris · dans la salle'}]})
    return p


# =====================================================================
# 15 · Cérémonie de remise de prix — « Les Étoiles d’or »
# Black and gold, art-deco hairlines and diamonds; Playfair Display
# for the names, wide-tracked Montserrat capitals; slow, luminous.
# =====================================================================

def ex15():
    K, GOLD, GOLD2, CHAMP, MUTE = '#060504', '#e8c36a', '#9c7425', '#f5ead0', '#a39a88'
    p = Package('ex-15-ceremonie-prix', 'Cérémonie de remise de prix', 'Événement',
                ['Cérémonie', 'Prix', 'Nommés', 'Révélation', 'Luxe'],
                fr("Une cérémonie de prix en noir et or : carton de catégorie, liste des nommés, révélation du lauréat qui attend "
                   "le déclencheur « reveal » (l’enveloppe s’ouvre en régie), synthés lauréat et remettant, carton d’applaudissements "
                   "pour les écrans de salle et logo de la soirée. Les nommés peuvent venir d’une source de données."),
                en=('Awards ceremony', 'An awards night in black and gold: category card, the nominees, the winner reveal waiting for the '
                    '“reveal” trigger (the envelope is opened in the gallery), winner and presenter straps, an applause card for the '
                    'screens in the hall and the night’s logo bug. The nominees may come from a data source.'),
                cover='reveal')
    p.zone('bug', 'Logo', 30, '#e8c36a', area=(0.72, 0.035, 0.26, 0.11))
    p.zone('lower', 'Synthé', 10, '#7c5cff', area=(0.04, 0.68, 0.62, 0.28))
    p.zone('full', 'Plein écran', 50, '#38bdf8')
    p.playlist('ceremony', fr('Cérémonie · 12e édition'))
    p.guides(action_safe=0.035, title_safe=0.05)
    GOLDTXT = dict(fillType='gradient', color2=GOLD2, gradAngle=180.0)

    def diamond(t, name, x, y, size, parent, tracks=None, fill=GOLD):
        return t.rect(name, x, y, size, size, hexc(fill), parent, shape='diamond', tracks=tracks)

    def ornament(t, parent, cx, y, w, t_in, on_air):
        """—— ◆ ——"""
        t.rect('Filet G', cx - 22 - w, y, w, 1.5, hexc(GOLD), parent, anchorX=1.0, tracks=inout('scaleX', 1, 0, on_air, t_in, 34))
        t.rect('Filet D', cx + 22, y, w, 1.5, hexc(GOLD), parent, anchorX=0.0, tracks=inout('scaleX', 1, 0, on_air, t_in, 34))
        diamond(t, 'Losange', cx - 9, y - 8.25, 18, parent, tracks=pop(on_air, t_in, 20))

    def stage(t, g, on_air, glow=0.22):
        t.rect('Fond', 0, 0, 1920, 1080, hexc(K, 0.94), g)
        t.ellipse('Lumière', 260, -300, 1400, 1100, hexc(GOLD, glow), g, grad=('radial', hexs(K, 0)),
                  tracks={'opacity': keys((0, 0.6, SMOOTH), (on_air, 1, SMOOTH), (on_air + 40, 0.8, 'linear'))})
        t.rect('Cadre', 60, 60, 1800, 960, hexc(K, 0), g, stroke=(hexc(GOLD, 0.35), 1.5))
        t.rect('Cadre 2', 72, 72, 1776, 936, hexc(K, 0), g, stroke=(hexc(GOLD, 0.18), 1))

    # --- Logo bug -------------------------------------------------------
    t = p.template('bug', 'Logo de la soirée', 'bug', duration=90, on_air=50)
    t.field('name', 'Nom', 'LES ÉTOILES D’OR')
    t.field('line', 'Ligne', fr('EN DIRECT DU THÉÂTRE DU CHÂTELET'))
    g = frame(t, 'Logo', 1400, 48, 460, 100, tracks=fade(50, 0, 24, 16))
    diamond(t, 'Losange', 420, 18, 26, g, tracks={'rotation': keys((0, 0, SMOOTH), (50, 90, 'linear'))})
    t.text('Nom', '{{name}}', 0, 6, 400, 50, 30, g, family=SERIF, weight=700, align='right', spacing=3, color=hexc(GOLD), **GOLDTXT)
    t.text('Ligne', '{{line}}', 0, 56, 400, 30, 13, g, family=GEO, weight=600, align='right', spacing=3, color=hexc(CHAMP, 0.8))

    # --- Category card ------------------------------------------------
    t = p.template('category', 'Catégorie', 'full', duration=180, on_air=120)
    t.field('edition', 'Surtitre', fr('LES ÉTOILES D’OR · 12e CÉRÉMONIE'))
    t.field('category', 'Catégorie', 'Meilleure réalisation')
    t.field('sponsor', 'Présenté par', fr('Prix présenté par la Maison Delorme'))
    g = t.group('Catégorie', 0, 0, 1920, 1080, tracks=fade(120, 0, 24, 24))
    stage(t, g, 120)
    t.text('Surtitre', '{{edition}}', 0, 330, 1920, 40, 22, g, family=GEO, weight=600, align='center', spacing=10, color=hexc(CHAMP, 0.85),
           tracks=fade(120, 10, 24))
    t.text('Catégorie', '{{category}}', 0, 400, 1920, 180, 128, g, family=SERIF, weight=700, align='center', color=hexc(GOLD),
           tracks=merge(fade(120, 16, 30), inout('letterSpacing', 0, 14, 120, 16, 80)), **GOLDTXT)
    ornament(t, g, 960, 620, 260, 30, 120)
    t.text('Partenaire', '{{sponsor}}', 0, 660, 1920, 40, 22, g, family=SERIF, weight=600, italic=True, align='center', color=hexc(MUTE),
           tracks=fade(120, 40, 24))

    # --- Nominees ---------------------------------------------------------
    NOMS = [('Justine Triet', 'Le Ciel d’après'), ('Ladj Ly', 'Les Remparts'), ('Céline Sciamma', 'Marées basses'),
            ('Thomas Cailley', 'Les Oiseaux de feu'), ('Alice Diop', 'La Traversée')]
    t = p.template('nominees', 'Les nommés', 'full', duration=200, on_air=140)
    t.field('category', 'Catégorie', 'Meilleure réalisation')
    for i, (n, w) in enumerate(NOMS, 1):
        t.field('nom%d' % i, 'Nommé %d' % i, n)
        t.field('work%d' % i, 'Œuvre %d' % i, fr(w))
        bind(t, 'nom%d' % i, 'etoiles-or-nommes', 'categories[{category_id}].nominees[%d].name' % (i - 1))
        bind(t, 'work%d' % i, 'etoiles-or-nommes', 'categories[{category_id}].nominees[%d].work' % (i - 1))
    t.field('category_id', 'Identifiant de catégorie (données)', '0')
    g = t.group('Nommés', 0, 0, 1920, 1080, tracks=fade(140, 0, 20, 20, delay_out=10))
    stage(t, g, 140, 0.14)
    t.text('Surtitre', 'LES NOMMÉS', 0, 130, 1920, 40, 22, g, family=GEO, weight=600, align='center', spacing=12, color=hexc(CHAMP, 0.85),
           tracks=fade(140, 6, 20))
    t.text('Catégorie', '{{category}}', 0, 170, 1920, 110, 76, g, family=SERIF, weight=700, align='center', color=hexc(GOLD),
           tracks=fade(140, 10, 24), **GOLDTXT)
    ornament(t, g, 960, 300, 160, 16, 140)
    for i in range(1, 6):
        y = 350 + (i - 1) * 124
        row = t.group('Nommé %d' % i, 360, y, 1200, 112, g, tracks=merge(fade(140, 20 + i * 6, 18, 10, delay_out=(5 - i) * 2),
                                                                      inout('y', y, y + 30, 140, 20 + i * 6, 26, 10, (5 - i) * 2)))
        t.text('Nom', '{{nom%d}}' % i, 0, 6, 1200, 64, 48, row, family=SERIF, weight=600, align='center', color=hexc(CHAMP))
        t.text('Œuvre', '{{work%d}}' % i, 0, 70, 1200, 30, 18, row, family=GEO, weight=600, align='center', spacing=6, upper=True,
               color=hexc(GOLD, 0.85))
        if i < 5:
            t.rect('Séparateur', 570, 111, 60, 1, hexc(GOLD, 0.4), row)

    # --- Winner reveal ----------------------------------------------------
    t = p.template('reveal', 'Révélation du lauréat', 'full', duration=240, on_air=170)
    t.field('category', 'Catégorie', 'Meilleure réalisation')
    t.field('winner', 'Lauréat', 'Céline Sciamma')
    t.field('work', 'Œuvre', 'Marées basses')
    t.pause(60, 'Enveloppe', {'type': 'trigger', 'name': 'reveal'})
    g = t.group('Révélation', 0, 0, 1920, 1080, tracks=fade(170, 0, 20, 24))
    stage(t, g, 170, 0.18)
    t.text('Catégorie', '{{category}}', 0, 250, 1920, 44, 24, g, family=GEO, weight=600, align='center', spacing=10, upper=True,
           color=hexc(CHAMP, 0.85), tracks=fade(170, 10, 20))
    t.text('Attribué à', fr('ET L’ÉTOILE D’OR EST ATTRIBUÉE À'), 0, 470, 1920, 60, 30, g, family=GEO, weight=600, align='center', spacing=8,
           color=hexc(CHAMP), tracks={'opacity': keys((0, 0, 'hold'), (16, 0, 'linear'), (36, 1, 'hold'), (60, 1, 'linear'), (72, 0, 'hold'))})
    t.ellipse('Éclat', 360, 140, 1200, 800, hexc(GOLD, 0.7), g, grad=('radial', hexs(K, 0)),
              tracks={'opacity': keys((0, 0, 'hold'), (62, 0, EXPO_OUT), (70, 1, SMOOTH), (110, 0.25, 'linear')),
                      'scaleX': keys((0, 0.2, 'hold'), (62, 0.2, EXPO_OUT), (100, 1.2, 'linear')),
                      'scaleY': keys((0, 0.2, 'hold'), (62, 0.2, EXPO_OUT), (100, 1.2, 'linear'))})
    for k, (dx, dy, sz, d) in enumerate(((-560, -150, 22, 0), (520, -190, 16, 4), (610, 120, 26, 8), (-640, 140, 14, 6), (-300, -260, 12, 10),
                                         (330, 250, 12, 12))):
        diamond(t, 'Étincelle %d' % (k + 1), 960 + dx, 520 + dy, sz, g,
                tracks=merge({'opacity': keys((0, 0, 'hold'), (66 + d, 0, EXPO_OUT), (80 + d, 1, SMOOTH), (150, 0.5, 'linear'))},
                             {'rotation': keys((0, 0, 'hold'), (66 + d, 0, SMOOTH), (170, 90, 'linear'))}))
    clip = t.group('Nom', 0, 400, 1920, 200, g, clip=True)
    t.text('Lauréat', '{{winner}}', 0, 0, 1920, 200, 140, clip, family=SERIF, weight=700, align='center', color=hexc(GOLD),
           tracks={'y': keys((0, 200, 'hold'), (64, 200, EXPO_OUT), (96, 0, 'linear'), (170, 0, EXPO_IN), (182, -200, 'linear'))}, **GOLDTXT)
    t.text('Œuvre', '{{work}}', 0, 616, 1920, 60, 40, g, family=SERIF, weight=600, italic=True, align='center', color=hexc(CHAMP),
           tracks={'opacity': keys((0, 0, 'hold'), (84, 0, 'linear'), (104, 1, 'linear'), (170, 1, 'linear'), (180, 0, 'linear'))})
    t.rect('Filet', 880, 700, 160, 1.5, hexc(GOLD), g,
           tracks={'scaleX': keys((0, 0, 'hold'), (90, 0, EXPO_OUT), (120, 1, 'linear'), (170, 1, EXPO_IN), (182, 0, 'linear'))})

    # --- Strap: winner / presenter ---------------------------------------
    t = p.template('lower', 'Synthé lauréat', 'lower', duration=130, on_air=80)
    t.field('label', 'Étiquette', fr('LAURÉATE · MEILLEURE RÉALISATION'))
    t.field('name', 'Nom', 'Céline Sciamma')
    t.field('work', 'Œuvre', fr('pour « Marées basses »'))
    g = frame(t, 'Synthé', 96, 800, 1100, 190)
    t.rect('Voile', 0, 0, 1100, 190, hexc(K, 0.85), g, grad=(hexs(K, 0), 90), tracks=fade(80, 0, 20, 16))
    t.rect('Filet', 0, 0, 760, 1.5, hexc(GOLD), g, grad=(hexs(GOLD, 0), 90), anchorX=0.0, tracks=inout('scaleX', 1, 0, 80, 0, 30))
    diamond(t, 'Losange', 36, 36, 14, g, tracks=pop(80, 6, 16))
    t.text('Étiquette', '{{label}}', 66, 22, 1000, 42, 18, g, family=GEO, weight=600, spacing=6, color=hexc(GOLD), tracks=fade(80, 8, 20))
    t.text('Nom', '{{name}}', 34, 62, 1050, 76, 60, g, family=SERIF, weight=700, color=hexc(CHAMP),
           tracks=merge(fade(80, 12, 24), inout('x', 34, 0, 80, 12, 30)))
    t.text('Œuvre', '{{work}}', 36, 138, 1050, 40, 26, g, family=SERIF, weight=600, italic=True, color=hexc(MUTE), tracks=fade(80, 20, 24))

    # --- Applause card ----------------------------------------------------
    t = p.template('applause', 'Applaudissements', 'full', duration=150, on_air=100)
    t.field('text', 'Texte', 'APPLAUDISSEZ')
    t.field('sub', 'Sous-titre', fr('Céline Sciamma rejoint la scène'))
    g = t.group('Applaudissements', 0, 0, 1920, 1080, tracks=fade(100, 0, 16, 20))
    stage(t, g, 100, 0.3)
    t.text('Texte', '{{text}}', 0, 420, 1920, 160, 132, g, family=GEO, weight=800, align='center', spacing=24, color=hexc(GOLD),
           tracks=merge({'scaleX': keys((0, 0.92, EXPO_OUT), (40, 1, SMOOTH), (70, 1.03, SMOOTH), (100, 1, 'linear')),
                         'scaleY': keys((0, 0.92, EXPO_OUT), (40, 1, SMOOTH), (70, 1.03, SMOOTH), (100, 1, 'linear'))}), **GOLDTXT)
    ornament(t, g, 960, 610, 300, 12, 100)
    t.text('Sous-titre', '{{sub}}', 0, 650, 1920, 50, 34, g, family=SERIF, weight=600, italic=True, align='center', color=hexc(CHAMP),
           tracks=fade(100, 24, 20))

    p.item('Logo', 'bug', 'ceremony')
    p.item(fr('Catégorie · Réalisation'), 'category', 'ceremony')
    p.item(fr('Nommés · Réalisation'), 'nominees', 'ceremony')
    p.item(fr('Remettant · Isabelle Huppert'), 'lower', 'ceremony', {'label': 'REMET LE PRIX', 'name': 'Isabelle Huppert',
                                                                     'work': fr('Présidente de l’Académie')})
    p.item(fr('Révélation · Réalisation'), 'reveal', 'ceremony')
    p.item('Applaudissements', 'applause', 'ceremony')
    p.item(fr('Lauréate · Céline Sciamma'), 'lower', 'ceremony')
    p.source('etoiles-or-nommes', 'Étoiles d’or · nommés', {'categories': [
        {'name': 'Meilleure réalisation', 'nominees': [{'name': n, 'work': fr(w)} for n, w in NOMS]},
        {'name': 'Meilleure actrice', 'nominees': [{'name': 'Adèle Exarchopoulos', 'work': 'Les Remparts'},
                                                   {'name': 'Léa Drucker', 'work': fr('Le Ciel d’après')},
                                                   {'name': 'Virginie Efira', 'work': fr('Marées basses')},
                                                   {'name': 'Sandrine Kiberlain', 'work': 'La Traversée'},
                                                   {'name': 'Noémie Merlant', 'work': 'Les Oiseaux de feu'}]}]})
    return p


# =====================================================================
# 16 · Jeu TV / quiz — « Le Grand Défi »
# Studio blue, cyan edges, orange for a locked answer and green for the
# right one; Titillium Web; hexagonal plates and a radiating sunburst.
# =====================================================================

def ex16():
    D1, D2, CYAN, ORANGE, GREEN, WHT, MUTE, RED = '#050c33', '#0d2178', '#3fd2ff', '#ff9a1f', '#22d68b', '#ffffff', '#9fb4ff', '#ff4d6d'
    p = Package('ex-16-quiz-tv', 'Jeu télévisé', 'Divertissement',
                ['Jeu', 'Quiz', 'Chrono', 'Scores', 'Déclencheur'],
                fr("Un jeu de questions-réponses de prime : question et quatre réponses (la bonne s’allume en vert quand la régie "
                   "continue), chrono de 30 secondes en anneau qui passe au rouge, tableau des scores, titre de manche sur fond "
                   "de rayons, présentation des candidats et vainqueur. Les questions peuvent venir d’une source de données."),
                en=('TV quiz show', 'A prime-time quiz: a question and four answers (the right one lights up green when the gallery '
                    'continues), a 30-second ring timer that turns red, the scores, a round title over a sunburst, contestant straps and '
                    'the winner. The questions may come from a data source.'),
                cover='question')
    p.zone('board', 'Question', 10, '#7c5cff', area=(0.025, 0.41, 0.95, 0.57))
    p.zone('timer', 'Chrono', 30, '#ff4d6d', area=(0.82, 0.05, 0.15, 0.3))
    p.zone('scores', 'Scores', 20, '#22c55e', area=(0.025, 0.05, 0.3, 0.42))
    p.zone('lower', 'Candidat', 15, '#f5a524', area=(0.04, 0.7, 0.6, 0.26))
    p.zone('full', 'Plein écran', 50, '#38bdf8')
    p.playlist('round2', 'Manche 2')
    p.playlist('final', 'Finale')
    p.guides(action_safe=0.035, title_safe=0.05)
    HEX = [(0, 0.5), (0.03, 0), (0.97, 0), (1, 0.5), (0.97, 1), (0.03, 1)]

    def plate(t, name, x, y, w, h, parent, fill=D2, fill2=D1, edge=CYAN, tracks=None, edge_a=0.9):
        g = t.group(name, x, y, w, h, parent, tracks=tracks)
        poly(t, 'Tranche', 0, 0, w, h, hexc(edge, edge_a), HEX, g)
        lid = poly(t, 'Plaque', 3, 3, w - 6, h - 6, hexc(fill), HEX, g)
        t.layer_ref(lid)['fillStyle'] = {'kind': 'linear', 'color2': fill2, 'angle': 180.0}
        return g

    def sunburst(t, g, on_air, color=CYAN, a=0.1):
        t.widget('polygon', 'Rayons', 960 - 1400, 540 - 1400, 2800, 2800, g, {'fill': hexc(color, a)},
                 {'rotation': keys((0, 0, 'linear'), (on_air + 60, 24, 'linear'))}, mode='regular', sides=24.0, star=True, inner=4.0)

    # --- Question + 4 answers ------------------------------------------
    t = p.template('question', 'Question', 'board', duration=140, on_air=90)
    t.field('quiz_num', 'Numéro', 'QUESTION 7')
    t.field('quiz_q', 'Question', fr('Quel peintre a réalisé « Impression, soleil levant » en 1872 ?'), kind='multiline')
    for k, a in zip('ABCD', ('Édouard Manet', 'Claude Monet', 'Auguste Renoir', 'Paul Cézanne')):
        t.field('quiz_a' + k, 'Réponse ' + k, a)
    for k in 'ABCD':
        t.field('quiz_ok' + k, 'Bonne réponse ' + k, 'true' if k == 'B' else 'false', kind='boolean')
    t.field('lock', 'Réponse verrouillée (A–D)', 'B')
    dataset(t, 'quiz', 'Questions de la manche', 'grand-defi-questions', 'questions')
    t.pause(46, 'Réponses à l’écran')
    g = frame(t, 'Question', 60, 510, 1800, 540)
    qp = plate(t, 'Question', 0, 0, 1800, 170, g, tracks=merge(inout('scaleX', 1, 0.2, 90, 0, 18), fade(90, 0, 8)))
    t.rect('Onglet', 760, -40, 280, 40, hexc(ORANGE), qp, corners=(12, 12, 0, 0))
    t.text('Numéro', '{{quiz_num}}', 760, -40, 280, 40, 22, qp, color=hexc(D1), family=TECH, weight=900, align='center', spacing=5)
    t.text('Question', '{{quiz_q}}', 90, 3, 1620, 164, 50, qp, family=TECH, weight=700, align='center', wrap=True, lineHeight=1.12)
    for i, k in enumerate('ABCD'):
        x, y = (0 if i % 2 == 0 else 920), (200 if i < 2 else 320)
        d = 10 + i * 4
        a = t.group('Réponse ' + k, x, y, 880, 100, g, tracks=merge(fade(90, d, 8), inout('x', x, x + (-80 if i % 2 == 0 else 80), 90, d, 16)))
        plate(t, 'Plaque', 0, 0, 880, 100, a)
        ok = t.group('Bonne réponse', 0, 0, 880, 100, a, bindings={'opacity': 'quiz_ok' + k})
        flash = t.group('Allumage', 0, 0, 880, 100, ok, tracks={'opacity': keys((0, 0, 'hold'), (46, 0, 'linear'), (50, 1, 'linear'),
                                                                                    (54, 0.3, 'linear'), (58, 1, 'linear'), (62, 0.3, 'linear'),
                                                                                    (66, 1, 'linear'))})
        plate(t, 'Verte', 0, 0, 880, 100, flash, fill=GREEN, fill2='#0f8f58', edge=WHT)
        t.text('Lettre', k + ' :', 52, 0, 80, 100, 36, a, color=hexc(ORANGE), family=TECH, weight=900)
        t.text('Réponse', '{{quiz_a%s}}' % k, 130, 0, 700, 100, 38, a, family=TECH, weight=700)

    # --- Timer ring ---------------------------------------------------------
    t = p.template('timer', 'Chrono 30 s', 'timer', duration=80, on_air=30)
    t.field('secs', 'Durée (s)', '30', kind='number', min=5, max=120)
    g = frame(t, 'Chrono', 1600, 70, 260, 260, tracks=pop(30, 0, 16))
    t.ellipse('Disque', 0, 0, 260, 260, hexc(D1, 0.92), g, grad=('radial', D2), stroke=(hexc(CYAN, 0.5), 3))
    clk = t.clock('Secondes', 30, 70, 200, 120, 96, g, mode='countdown', fmt='ss', color=hexc(WHT), family=TECH, weight=900,
                  cdDuration=30000, startMode='auto', tabular=True, steps=[{'on': True, 'at': 10, 'color': RED, 'bg': '', 'blink': False}])
    t.progress('Anneau', '0', 14, 14, 232, 232, hexc(CYAN), g, max='30', track=hexs('#ffffff', 0.1), shape='ring', input='follow',
               follow=clk, thickness=14.0, rounded=True, steps=[{'on': True, 'below': 10.0, 'color': RED}], duration=0.3)
    t.text('Unité', 'SECONDES', 30, 176, 200, 30, 16, g, color=hexc(MUTE), family=TECH, weight=700, align='center', spacing=3)

    # --- Scores -----------------------------------------------------------
    PL = [('Inès', 'Rennes', 340, '#ff9a1f'), ('Karim', 'Marseille', 280, '#3fd2ff'), ('Léa', 'Lille', 210, '#e45bff'), ('Hugo', 'Dijon', 150, '#22d68b')]
    t = p.template('scores', 'Scores', 'scores', duration=110, on_air=60)
    for i, (n, c, sc, col) in enumerate(PL, 1):
        t.field('p%d' % i, 'Candidat %d' % i, n)
        t.field('s%d' % i, 'Score %d' % i, str(sc), kind='number')
        bind(t, 's%d' % i, 'grand-defi-scores', 'players[%d].score' % (i - 1))
    g = frame(t, 'Scores', 60, 60, 540, 420)
    t.text('Titre', 'SCORES', 4, 0, 400, 44, 26, g, color=hexc(CYAN), family=TECH, weight=900, spacing=6, tracks=fade(60, 0, 10))
    for i, (n, c, sc, col) in enumerate(PL, 1):
        y = 52 + (i - 1) * 88
        r = t.group('Candidat %d' % i, 0, y, 540, 76, g, tracks=merge(fade(60, i * 4, 8, 8, (4 - i) * 2), inout('x', 0, -100, 60, i * 4, 16, 10, (4 - i) * 2)))
        plate(t, 'Plaque', 0, 0, 540, 76, r, edge_a=0.6)
        t.ellipse('Couleur', 26, 22, 32, 32, hexc(col), r)
        t.text('Nom', '{{p%d}}' % i, 76, 0, 300, 76, 34, r, family=TECH, weight=700, upper=True)
        t.counter('Score', '{{s%d}}' % i, 330, 0, 180, 76, 40, r, color=hexc(ORANGE if i == 1 else WHT), family=TECH, weight=900,
                  align='right', duration=1.2, grouping=False)

    # --- Round title --------------------------------------------------------
    t = p.template('round', 'Titre de manche', 'full', duration=150, on_air=100)
    t.field('round', 'Manche', 'MANCHE 2')
    t.field('title', 'Titre', 'Le sprint')
    t.field('rule', 'Règle', fr('8 questions · 30 secondes · 20 points par bonne réponse'))
    g = t.group('Manche', 0, 0, 1920, 1080, tracks=fade(100, 0, 10, 14))
    t.rect('Fond', 0, 0, 1920, 1080, hexc(D1), g, grad=('radial', D2))
    t.layer_ref(t.t['layers'][-1]['id'])['fillStyle'] = {'kind': 'radial', 'color2': D1}
    t.layer_ref(t.t['layers'][-1]['id'])['props']['fill'] = hexc('#1a3bd1')
    sunburst(t, g, 100)
    t.text('Manche', '{{round}}', 0, 330, 1920, 80, 56, g, color=hexc(ORANGE), family=TECH, weight=900, align='center', spacing=14,
           tracks=pop(100, 6, 18))
    t.text('Titre', '{{title}}', 0, 410, 1920, 220, 190, g, family=TECH, weight=900, align='center', upper=True,
           shadow=True, shadowColor='#00000080', shadowBlur=30.0, shadowY=8.0, tracks=pop(100, 12, 20))
    plate(t, 'Règle', 360, 680, 1200, 80, g, tracks=merge(fade(100, 24, 10), inout('y', 680, 720, 100, 24, 16)))
    t.text('Règle', '{{rule}}', 360, 680, 1200, 80, 30, g, family=TECH, weight=700, align='center',
           tracks=merge(fade(100, 24, 10), inout('y', 680, 720, 100, 24, 16)))

    # --- Contestant -------------------------------------------------------
    t = p.template('lower', 'Candidat', 'lower', duration=110, on_air=60)
    t.field('name', 'Nom', 'Inès Morvan')
    t.field('town', 'Ville', fr('Rennes · professeure de musique'))
    t.field('wins', 'Victoires', fr('3 VICTOIRES'))
    g = frame(t, 'Candidat', 96, 820, 1050, 150)
    plate(t, 'Nom', 0, 0, 1050, 96, g, tracks=merge(inout('scaleX', 1, 0.3, 60, 0, 16), fade(60, 0, 8)))
    t.text('Nom', '{{name}}', 60, 0, 700, 96, 52, g, family=TECH, weight=900, upper=True, tracks=fade(60, 6, 10))
    t.rect('Pastille', 780, 24, 220, 48, hexc(ORANGE), g, radius=24, tracks=pop(60, 12, 14))
    t.text('Victoires', '{{wins}}', 780, 24, 220, 48, 22, g, color=hexc(D1), family=TECH, weight=900, align='center', spacing=2, tracks=pop(60, 12, 14))
    t.text('Ville', '{{town}}', 64, 104, 900, 40, 26, g, color=hexc(WHT), family=TECH, weight=600,
           shadow=True, shadowColor='#000000a0', shadowBlur=12.0, tracks=merge(fade(60, 12, 10), inout('y', 104, 84, 60, 12, 14)))

    # --- Winner -------------------------------------------------------------
    t = p.template('winner', 'Vainqueur', 'full', duration=200, on_air=140)
    t.field('name', 'Vainqueur', 'Inès Morvan')
    t.field('score', 'Score final', '520', kind='number')
    t.field('prize', 'Gain', fr('remporte 25 000 €'))
    g = t.group('Vainqueur', 0, 0, 1920, 1080, tracks=fade(140, 0, 10, 16))
    t.rect('Fond', 0, 0, 1920, 1080, hexc('#2a1a6e'), g, grad=('radial', D1))
    sunburst(t, g, 140, ORANGE, 0.14)
    t.text('Titre', 'VAINQUEUR', 0, 250, 1920, 80, 50, g, color=hexc(ORANGE), family=TECH, weight=900, align='center', spacing=20,
           tracks=pop(140, 4, 16))
    t.text('Nom', '{{name}}', 0, 330, 1920, 200, 150, g, family=TECH, weight=900, align='center', upper=True,
           shadow=True, shadowColor='#00000080', shadowBlur=30.0, shadowY=8.0, tracks=pop(140, 10, 22))
    t.counter('Score', '{{score}}', 560, 560, 800, 140, 110, g, color=hexc(CYAN), family=TECH, weight=900, align='center',
              suffix=' PTS', duration=2.0, affixSize=40.0)
    t.text('Gain', '{{prize}}', 0, 720, 1920, 60, 40, g, family=TECH, weight=700, align='center', tracks=fade(140, 40, 14))
    for k in range(18):
        import math
        a = k * 20 * math.pi / 180
        x, y = 960 + math.cos(a) * (520 + (k % 3) * 120), 520 + math.sin(a) * (330 + (k % 4) * 60)
        c = (ORANGE, CYAN, GREEN, '#e45bff')[k % 4]
        t.rect('Confetti %d' % (k + 1), x, y, 16, 30 if k % 2 else 16, hexc(c), g, radius=3,
               tracks=merge({'opacity': keys((0, 0, 'hold'), (14 + k % 6, 0, 'linear'), (20 + k % 6, 1, 'linear'))},
                            {'rotation': keys((0, k * 37 % 90, 'linear'), (200, k * 37 % 90 + 240, 'linear'))},
                            {'y': keys((0, y - 120, EXPO_OUT), (40 + k % 6, y, 'linear'), (200, y + 60, 'linear'))}))

    p.item('Manche 2 · Le sprint', 'round', 'round2')
    p.item('Scores', 'scores', 'round2')
    p.item(fr('Inès Morvan'), 'lower', 'round2')
    p.item('Question 7 · Monet', 'question', 'round2')
    p.item('Chrono 30 s', 'timer', 'round2')
    p.item('Question 8 · Fleuves', 'question', 'round2', {'quiz__row': '2', 'quiz_num': 'QUESTION 8', 'quiz_q': fr('Quel est le plus long fleuve de France ?'),
                                                        'quiz_aA': 'La Seine', 'quiz_aB': 'Le Rhône', 'quiz_aC': 'La Loire',
                                                        'quiz_aD': 'La Garonne', 'quiz_okA': 'false', 'quiz_okB': 'false',
                                                        'quiz_okC': 'true', 'quiz_okD': 'false'})
    p.item('Vainqueur', 'winner', 'final')
    p.source('grand-defi-questions', 'Le Grand Défi · questions', {'questions': [
        {'num': 'QUESTION 7', 'q': fr('Quel peintre a réalisé « Impression, soleil levant » en 1872 ?'), 'aA': 'Édouard Manet',
         'aB': 'Claude Monet', 'aC': 'Auguste Renoir', 'aD': 'Paul Cézanne', 'okA': 'false', 'okB': 'true', 'okC': 'false', 'okD': 'false'},
        {'num': 'QUESTION 8', 'q': fr('Quel est le plus long fleuve de France ?'), 'aA': 'La Seine', 'aB': 'Le Rhône',
         'aC': 'La Loire', 'aD': 'La Garonne', 'okA': 'false', 'okB': 'false', 'okC': 'true', 'okD': 'false'},
        {'num': 'QUESTION 9', 'q': fr('En quelle année a eu lieu la première Coupe du monde de football ?'), 'aA': '1924',
         'aB': '1930', 'aC': '1934', 'aD': '1938', 'okA': 'false', 'okB': 'true', 'okC': 'false', 'okD': 'false'}]})
    p.source('grand-defi-scores', 'Le Grand Défi · scores', {'players': [
        {'name': n, 'score': sc} for n, c, sc, col in PL]})
    return p


# =====================================================================
# 17 · Basket NBA-style — « Elite Basket »
# Black, court orange and the two team colours; Barlow Condensed in
# heavy italics; hard diagonals and fast, punchy slides.
# =====================================================================

def ex17():
    K, PANEL, ORANGE, WHT, MUTE = '#0a0a0e', '#16161d', '#ff6a13', '#ffffff', '#9a9aa8'
    HOME, HOME2, AWAY, AWAY2 = '#1b2f73', '#ff6a13', '#c8102e', '#ffffff'
    p = Package('ex-17-basket', 'Basket', 'Sport',
                ['Basket', 'Tableau de score', 'Horloge des tirs', 'Statistiques', 'Cinq de départ'],
                fr("Un match de basket à l’américaine : tableau de score avec horloge de match, horloge des 24 secondes, fautes, "
                   "temps morts et possession (pilotés au clavier par l’opérateur ou par le flux des stats), fiche joueur avec ses "
                   "statistiques, score par quart-temps, comparatif des équipes et cinq de départ."),
                en=('Basketball', 'An NBA-style basketball game: scoreboard with game clock, 24-second shot clock, fouls, timeouts and '
                    'possession (driven by the operator or the stats feed), player card with his stats, score by quarter, team '
                    'comparison and starting five.'),
                cover='player')
    p.zone('score', 'Tableau de score', 40, '#ff6a13', area=(0.1, 0.75, 0.8, 0.23))
    p.zone('lower', 'Fiche joueur', 10, '#7c5cff', area=(0.04, 0.6, 0.66, 0.36))
    p.zone('full', 'Plein écran', 50, '#38bdf8')
    p.playlist('pre', 'Avant-match')
    p.playlist('game', 'Match')
    p.guides(action_safe=0.035, title_safe=0.05)
    IT = dict(italic=True)
    TEAMS = [{'name': 'Paris Basketball', 'short': 'PAR', 'crest': 'round', 'logo': '', 'c1': HOME, 'c2': HOME2, 'score': '{{home_score}}'},
             {'name': 'AS Monaco', 'short': 'MON', 'crest': 'shield', 'logo': '', 'c1': AWAY, 'c2': AWAY2, 'score': '{{away_score}}'}]

    # --- Scoreboard ---------------------------------------------------------
    t = p.template('scorebug', 'Tableau de score', 'score', duration=80, on_air=40)
    t.field('home_score', 'Score Paris (flux)', '78', kind='number')
    t.field('away_score', 'Score Monaco (flux)', '74', kind='number')
    bind(t, 'home_score', 'elite-basket-live', 'game.home.score')
    bind(t, 'away_score', 'elite-basket-live', 'game.away.score')
    g = frame(t, 'Tableau', 426, 880, 1240, 170)
    t.widget('scoreboard', 'Tableau', 0, 0, 1240, 170, g, {'fontSize': 40.0}, tracks=merge(inout('y', 0, 250, 40, 0, 18), fade(40, 0, 8)),
             preset='basketball', teams=TEAMS, digits=3, layout='horizontal', order='clockLast', version='full', nameW=3.6, radius=6.0,
             showCrest=True, bg=K, font=COND,
             fields={'period': True, 'clock': True, 'fouls': True, 'timeouts': True, 'cards': False, 'poss': True, 'sets': False,
                     'shot': True, 'shootout': False, 'susp': False},
             clock={'dir': 'down', 'len': 10.0, 'extra': 5.0, 'tenths': True}, shot={'len': 24.0, 'reset': 14.0},
             anim={'score': 'flip', 'banner': True, 'evDur': 3.0}, source={'rule': 'override', 'overS': 20.0})

    # --- Player card --------------------------------------------------------
    t = p.template('player', 'Fiche joueur', 'lower', duration=130, on_air=80)
    t.field('num', 'Numéro', '1')
    t.field('first', 'Prénom', 'Tyson')
    t.field('last', 'Nom', 'Ward')
    t.field('info', 'Poste · taille · âge', fr('ARRIÈRE · 1,96 M · 26 ANS'))
    t.field('pts', 'Points', '24', kind='number')
    t.field('reb', 'Rebonds', '6', kind='number')
    t.field('ast', 'Passes', '8', kind='number')
    t.field('eff', 'Évaluation', '27', kind='number')
    t.field('color', 'Couleur de l’équipe', HOME, kind='color')
    for k in ('pts', 'reb', 'ast', 'eff'):
        bind(t, k, 'elite-basket-live', 'players[number={num}].' + k)
    g = frame(t, 'Joueur', 96, 690, 1180, 300)
    num = t.group('Numéro', 0, 0, 300, 300, g, clip=True, tracks=wipe(80, prop='cropBottom', length=16, delay_out=6))
    t.rect('Couleur', 0, 0, 300, 300, hexc(HOME), num, bindings={'fill': 'color'})
    t.rect('Biais', 150, 0, 300, 300, hexc('#000000', 0.18), num, shape='parallelogram', skew=-80)
    t.text('Numéro', '{{num}}', 0, 20, 300, 260, 230, num, color=hexc(WHT), family=COND, weight=800, align='center', **IT,
           tracks=inout('y', 20, 300, 80, 4, 18))
    c = t.group('Fiche', 300, 0, 880, 300, g, clip=True, tracks=wipe(80, length=18, t_in=6))
    t.rect('Fond', 0, 0, 880, 300, hexc(K, 0.94), c)
    t.rect('Orange', 0, 0, 880, 6, hexc(ORANGE), c)
    t.text('Prénom', '{{first}}', 40, 22, 800, 46, 38, c, color=hexc(MUTE), family=COND, weight=600, upper=True, **IT)
    t.text('Nom', '{{last}}', 38, 58, 820, 100, 96, c, color=hexc(WHT), family=COND, weight=800, upper=True, **IT,
           tracks=merge(inout('x', 38, 120, 80, 10, 18), fade(80, 10, 8)))
    t.text('Infos', '{{info}}', 40, 152, 800, 36, 24, c, color=hexc(ORANGE), family=COND, weight=700, spacing=2)
    for i, (k, lab) in enumerate((('pts', 'POINTS'), ('reb', 'REBONDS'), ('ast', 'PASSES D.'), ('eff', 'ÉVAL.'))):
        x = 40 + i * 205
        b = t.group('Stat ' + lab, x, 200, 190, 84, c, tracks=merge(fade(80, 16 + i * 3, 8), inout('y', 200, 240, 80, 16 + i * 3, 14)))
        t.rect('Case', 0, 0, 190, 84, hexc(PANEL), b)
        t.counter('Valeur', '{{%s}}' % k, 16, 0, 90, 84, 58, b, color=hexc(WHT), family=COND, weight=800, duration=1.0, grouping=False, **IT)
        t.text('Libellé', lab, 96, 0, 90, 84, 18, b, color=hexc(MUTE), family=COND, weight=700, spacing=1, lineHeight=1.0)

    # --- Score by quarter ------------------------------------------------
    t = p.template('quarter', 'Score par quart-temps', 'full', duration=150, on_air=100)
    t.field('title', 'Titre', fr('FIN DU 3e QUART-TEMPS'))
    t.field('table', 'Scores', table_value([['Équipe', 'Q1', 'Q2', 'Q3', 'Q4', 'Total'], ['PARIS', '24', '19', '27', '–', '70'],
                                            ['MONACO', '21', '25', '18', '–', '64']]), kind='table')
    g = t.group('Quart-temps', 310, 330, 1300, 420, tracks=merge(fade(100, 0, 10, 10), inout('y', 330, 380, 100, 0, 18)))
    t.rect('Fond', 0, 0, 1300, 420, hexc(K, 0.95), g)
    t.rect('Orange', 0, 0, 1300, 8, hexc(ORANGE), g)
    t.text('Titre', '{{title}}', 50, 30, 1200, 70, 54, g, color=hexc(WHT), family=COND, weight=800, **IT)
    t.text('Compétition', fr('BETCLIC ÉLITE · J12'), 50, 30, 1200, 70, 24, g, color=hexc(MUTE), family=COND, weight=700, spacing=3, align='right')
    t.widget('table', 'Score', 50, 130, 1200, 250, g, {'fontSize': 44.0}, link='{{table}}', headerRow=True, rowH=86.0, pad=24.0,
             gap=6.0, headSize=22.0, indicator=False, widthMode='percent', perPage=4,
             cols=[{'src': 'Équipe', 'label': '', 'pct': 34.0}] + [{'src': q, 'label': q, 'pct': 11.0, 'align': 'center'} for q in ('Q1', 'Q2', 'Q3', 'Q4')]
             + [{'src': 'Total', 'label': 'TOTAL', 'pct': 22.0, 'align': 'center', 'txt': {'weight': 800}}],
             rowFrom='', st={'header': {'bg': '#00000000', 'color': MUTE}, 'hcol': {'bg': PANEL, 'color': WHT},
                             'odd': {'bg': PANEL, 'color': WHT}, 'even': {'bg': PANEL, 'color': WHT}, 'hl': {'bg': ORANGE, 'color': WHT},
                             'top3': {'bg': PANEL, 'color': WHT}, 'tableBg': '#00000000'},
             borders={'outer': {'on': False, 'width': 0.0, 'color': '#00000000'}, 'innerH': {'on': False, 'width': 1.0, 'color': '#00000000'},
                      'innerV': {'on': False, 'width': 1.0, 'color': '#00000000'}},
             cf={'neg': False, 'top3': False, 'topBar': False, 'hlBar': False},
             txt={'title': {}, 'header': {'family': COND, 'weight': 700, 'spacing': 3.0},
                  'rows': {'family': COND, 'weight': 700, 'size': 44.0, 'italic': True}},
             cascade={'on': True, 'style': 'slide', 'offset': 90.0, 'dur': 400.0})
    for i, (col, y) in enumerate(((HOME, 172), (AWAY, 264))):
        t.rect('Couleur %d' % (i + 1), 50, y, 10, 86, hexc(col), g, tracks=fade(100, 12, 6))

    # --- Team comparison --------------------------------------------------
    STATS = [('Tirs réussis', '48 %', 48, '43 %', 43), ('À 3 points', '38 %', 38, '31 %', 31), ('Rebonds', '34', 34, '29', 29),
             ('Passes décisives', '19', 19, '14', 14), ('Balles perdues', '9', 9, '13', 13)]
    t = p.template('compare', 'Comparatif des équipes', 'full', duration=160, on_air=110)
    t.field('home', 'Équipe 1', 'PARIS')
    t.field('away', 'Équipe 2', 'MONACO')
    for i, (lab, hv, hn, av, an) in enumerate(STATS, 1):
        t.field('l%d' % i, 'Statistique %d' % i, lab)
        t.field('h%d' % i, '%s · équipe 1' % lab, hv)
        t.field('a%d' % i, '%s · équipe 2' % lab, av)
        t.field('hn%d' % i, '%s · équipe 1 (barre)' % lab, str(hn), kind='number')
        t.field('an%d' % i, '%s · équipe 2 (barre)' % lab, str(an), kind='number')
    g = t.group('Comparatif', 260, 170, 1400, 740, tracks=merge(fade(110, 0, 10, 10), inout('y', 170, 220, 110, 0, 18)))
    t.rect('Fond', 0, 0, 1400, 740, hexc(K, 0.95), g)
    t.rect('Équipe 1', 0, 0, 700, 110, hexc(HOME), g)
    t.rect('Équipe 2', 700, 0, 700, 110, hexc(AWAY), g)
    t.rect('Biais', 640, 0, 120, 110, hexc(K), g, shape='parallelogram', skew=-40)
    t.text('Nom 1', '{{home}}', 50, 0, 560, 110, 64, g, color=hexc(WHT), family=COND, weight=800, **IT)
    t.text('Nom 2', '{{away}}', 790, 0, 560, 110, 64, g, color=hexc(WHT), family=COND, weight=800, align='right', **IT)
    t.text('VS', 'VS', 640, 0, 120, 110, 34, g, color=hexc(ORANGE), family=COND, weight=800, align='center', **IT)
    for i in range(1, 6):
        y = 150 + (i - 1) * 116
        r = t.group('Ligne %d' % i, 50, y, 1300, 100, g, tracks=merge(fade(110, 8 + i * 4, 8), inout('x', 50, 120, 110, 8 + i * 4, 16)))
        t.text('Libellé', '{{l%d}}' % i, 0, 0, 1300, 44, 24, r, color=hexc(MUTE), family=COND, weight=700, align='center', spacing=3, upper=True)
        t.text('Valeur 1', '{{h%d}}' % i, 0, 0, 300, 44, 40, r, color=hexc(WHT), family=COND, weight=800, **IT)
        t.text('Valeur 2', '{{a%d}}' % i, 1000, 0, 300, 44, 40, r, color=hexc(WHT), family=COND, weight=800, align='right', **IT)
        t.progress('Barre 1', '{{hn%d}}' % i, 0, 56, 640, 16, hexc(HOME2), r, max='60', track=hexs('#ffffff', 0.08), dir='end', duration=1.2)
        t.progress('Barre 2', '{{an%d}}' % i, 660, 56, 640, 16, hexc(AWAY), r, max='60', track=hexs('#ffffff', 0.08), duration=1.2)

    # --- Starting five ------------------------------------------------------
    FIVE = [('1', 'T. Ward', 'Arrière'), ('7', 'N. Hifi', 'Meneur'), ('11', 'S. Hervey', 'Ailier'), ('21', 'J. Toupane', 'Ailier fort'),
            ('34', 'M. Kahudi', 'Pivot')]
    t = p.template('five', 'Cinq de départ', 'full', duration=170, on_air=110)
    t.field('team', 'Équipe', 'PARIS BASKETBALL')
    t.field('coach', 'Entraîneur', fr('Entraîneur : Tiago Splitter'))
    t.field('color', 'Couleur', HOME, kind='color')
    for i, (n, name, pos) in enumerate(FIVE, 1):
        t.field('n%d' % i, 'Numéro %d' % i, n)
        t.field('p%d' % i, 'Joueur %d' % i, name)
        t.field('r%d' % i, 'Poste %d' % i, pos)
    g = t.group('Cinq', 0, 0, 1920, 1080, tracks=fade(110, 0, 10, 12, delay_out=8))
    t.rect('Fond', 0, 0, 1920, 1080, hexc(K, 0.92), g)
    t.rect('Couleur', 0, 150, 1920, 200, hexc(HOME), g, bindings={'fill': 'color'}, tracks=inout('scaleX', 1, 0, 110, 0, 18))
    t.text('Surtitre', fr('CINQ DE DÉPART'), 120, 170, 1000, 50, 34, g, color=hexc(ORANGE), family=COND, weight=800, spacing=6, **IT,
           tracks=fade(110, 8, 8))
    t.text('Équipe', '{{team}}', 116, 210, 1700, 120, 110, g, color=hexc(WHT), family=COND, weight=800, **IT,
           tracks=merge(fade(110, 10, 8), inout('x', 116, 200, 110, 10, 18)))
    for i in range(1, 6):
        x = 120 + (i - 1) * 344
        cd = t.group('Joueur %d' % i, x, 420, 320, 440, g, tracks=merge(fade(110, 16 + i * 4, 8, 8, (5 - i) * 2),
                                                                         inout('y', 420, 500, 110, 16 + i * 4, 16, 10, (5 - i) * 2)))
        t.rect('Carte', 0, 0, 320, 440, hexc(PANEL), cd)
        t.rect('Haut', 0, 0, 320, 8, hexc(ORANGE), cd)
        t.text('Numéro', '{{n%d}}' % i, 0, 30, 320, 220, 200, cd, color=hexc(WHT, 0.1), family=COND, weight=800, align='center', **IT)
        t.text('Numéro net', '{{n%d}}' % i, 30, 40, 260, 80, 64, cd, color=hexc(ORANGE), family=COND, weight=800, **IT)
        t.text('Joueur', '{{p%d}}' % i, 30, 270, 280, 70, 50, cd, color=hexc(WHT), family=COND, weight=800, upper=True, **IT)
        t.text('Poste', '{{r%d}}' % i, 30, 340, 280, 40, 26, cd, color=hexc(MUTE), family=COND, weight=700, spacing=2, upper=True)
    t.text('Entraîneur', '{{coach}}', 120, 900, 1680, 50, 30, g, color=hexc(MUTE), family=COND, weight=700, spacing=2, upper=True,
           tracks=fade(110, 40, 10))

    p.item('Cinq de départ · Paris', 'five', 'pre')
    p.item('Comparatif', 'compare', 'game')
    p.item('Tableau de score', 'scorebug', 'game')
    p.item('Tyson Ward', 'player', 'game')
    p.item('Elie Okobo (Monaco)', 'player', 'game', {'num': '25', 'first': 'Elie', 'last': 'Okobo', 'info': fr('MENEUR · 1,91 M · 27 ANS'),
                                                    'pts': '18', 'reb': '3', 'ast': '9', 'eff': '21', 'color': AWAY})
    p.item(fr('Fin du 3e quart-temps'), 'quarter', 'game')
    p.source('elite-basket-live', 'Elite Basket · match en direct', {
        'game': {'home': {'name': 'Paris Basketball', 'score': 78}, 'away': {'name': 'AS Monaco', 'score': 74}, 'period': 'Q3'},
        'players': [{'number': 1, 'name': 'Tyson Ward', 'pts': 24, 'reb': 6, 'ast': 8, 'eff': 27},
                    {'number': 7, 'name': 'Nadir Hifi', 'pts': 15, 'reb': 2, 'ast': 5, 'eff': 14},
                    {'number': 25, 'name': 'Elie Okobo', 'pts': 18, 'reb': 3, 'ast': 9, 'eff': 21}]})
    return p


# =====================================================================
# 18 · Cyclisme — « Grande Boucle », étape de montagne
# Race yellow, black and white with the jersey colours (green, polka
# dots); Inter heavy for words, Barlow Condensed for figures; a clean
# timing grid, data first.
# =====================================================================

def ex18():
    K, PANEL, YEL, WHT, MUTE, GREEN, RED = '#101114', '#1b1d22', '#ffd400', '#ffffff', '#a2a6b0', '#1fae4b', '#e2231a'
    p = Package('ex-18-cyclisme', 'Cyclisme · étape de montagne', 'Sport',
                ['Cyclisme', 'Profil', 'Écarts', 'Classement', 'Carte', 'Kilomètres'],
                fr("Une étape de grand tour en direct : profil de l’étape avec ses cols, carte du parcours (hors ligne), kilomètres "
                   "restants, écarts entre l’échappée et le peloton, classement général (tableau importé ou lu dans le flux "
                   "des temps) et fiche coureur. Les temps et écarts peuvent venir d’une source de données."),
                en=('Cycling · mountain stage', 'A grand tour stage live: stage profile with its climbs, an offline route map, kilometres '
                    'to go, gaps between the breakaway and the bunch, general classification (a table imported or read from the timing '
                    'feed) and a rider card. Times and gaps may come from a data source.'),
                cover='profile')
    p.zone('km', 'Kilomètres', 30, '#ffd400', area=(0.76, 0.035, 0.21, 0.13))
    p.zone('lower', 'Bas d’écran', 10, '#7c5cff', area=(0.035, 0.66, 0.75, 0.31))
    p.zone('map', 'Carte', 20, '#22c55e', area=(0.6, 0.18, 0.37, 0.6))
    p.zone('full', 'Plein écran', 50, '#38bdf8')
    p.playlist('stage', fr('Étape 14 · Pau › Superbagnères'))
    p.guides(action_safe=0.035, title_safe=0.05)
    C = dict(family=COND, weight=700)

    # --- Kilometres to go ----------------------------------------------
    t = p.template('km', 'Kilomètres restants', 'km', duration=80, on_air=40)
    t.field('km', 'Kilomètres', '42.6', kind='number', min=0, max=250)
    bind(t, 'km', 'grande-boucle-course', 'race.kmToGo')
    g = frame(t, 'Km', 1470, 46, 380, 120)
    a = t.group('Bloc', 0, 0, 380, 120, g, clip=True, tracks=wipe(40, prop='cropLeft', length=16))
    t.rect('Fond', 0, 0, 380, 120, hexc(K, 0.92), a)
    t.rect('Jaune', 0, 0, 12, 120, hexc(YEL), a)
    for r in range(3):
        for c in range(4):
            t.rect('Damier', 36 + c * 12, 28 + r * 12, 12, 12, hexc(WHT if (r + c) % 2 == 0 else K), a)
    t.rect('Mât', 34, 28, 3, 66, hexc(WHT), a)
    t.counter('Km', '{{km}}', 96, 6, 260, 76, 70, a, decimals=1, duration=0.8, color=hexc(WHT), family=COND, weight=800, align='right',
              suffix=' km', affixSize=50.0, affixColor=YEL, locale='fr-FR', frm='{{km}}')
    t.text('Arrivée', fr('AVANT L’ARRIVÉE'), 96, 78, 260, 28, 18, a, color=hexc(MUTE), weight=700, align='right', spacing=3)

    # --- Stage profile ---------------------------------------------------
    PTS = [(0, 205), (12, 260), (25, 330), (38, 420), (48, 560), (56, 760), (62, 980), (66, 1250), (69, 1600), (72, 2115), (76, 1700),
           (82, 1200), (88, 820), (93, 760), (98, 1000), (104, 1490), (108, 1150), (114, 820), (120, 640), (126, 700), (132, 1050),
           (140, 1569), (145, 1250), (152, 900), (158, 630), (164, 700), (170, 1050), (176, 1450), (182.6, 1800)]
    COLS = [(72, 2115, 'HC', 'Col du Tourmalet', '2 115 m'), (104, 1490, '1', 'Col d’Aspin', '1 490 m'),
            (140, 1569, '1', 'Col de Peyresourde', '1 569 m'), (182.6, 1800, '1', 'Superbagnères', '1 800 m')]
    t = p.template('profile', 'Profil de l’étape', 'full', duration=200, on_air=140)
    t.field('stage', 'Étape', fr('ÉTAPE 14 · SAMEDI 19 JUILLET'))
    t.field('route', 'Parcours', fr('Pau › Luchon-Superbagnères'))
    t.field('dist', 'Distance', '182,6 km')
    t.field('dplus', 'Dénivelé', '+ 4 950 m')
    g = t.group('Profil', 100, 140, 1720, 800, tracks=merge(fade(140, 0, 12, 12, 6), inout('y', 140, 190, 140, 0, 20, 12, 6)))
    t.rect('Fond', 0, 0, 1720, 800, hexc(K, 0.94), g)
    t.rect('Jaune', 0, 0, 1720, 10, hexc(YEL), g)
    t.text('Étape', '{{stage}}', 60, 40, 1000, 40, 24, g, color=hexc(YEL), weight=800, spacing=4)
    t.text('Parcours', '{{route}}', 58, 78, 1100, 80, 64, g, color=hexc(WHT), weight=800)
    for i, (k, lab) in enumerate((('dist', 'DISTANCE'), ('dplus', 'DÉNIVELÉ'))):
        x = 1300 + i * 200
        t.text('Libellé', lab, x, 50, 190, 28, 16, g, color=hexc(MUTE), weight=700, spacing=3, align='right')
        t.text('Valeur', '{{%s}}' % k, x, 76, 190, 60, 46, g, color=hexc(WHT), align='right', **C)
    PX, PY, PW, PH = 60, 230, 1600, 470
    top, km_max = 2400.0, 182.6
    verts = [(0.0, 1.0)] + [(k / km_max, 1 - a / top) for k, a in PTS] + [(1.0, 1.0)]
    for i in range(5):
        y = PY + PH * (1 - i * 500 / top)
        t.rect('Grille %d' % i, PX, y, PW, 1, hexc(WHT, 0.08), g)
        t.text('Alt %d' % i, '%d m' % (i * 500), PX - 60, y - 14, 52, 28, 14, g, color=hexc(MUTE), weight=500, align='right')
    prof = t.group('Relief', PX, PY, PW, PH, g, clip=True, tracks=wipe(140, length=50, t_in=12, delay_out=0))
    lid = poly(t, 'Relief', 0, 0, PW, PH, hexc(YEL, 0.95), verts, prof)
    t.layer_ref(lid)['fillStyle'] = {'kind': 'linear', 'color2': hexs('#c7a200', 0.35), 'angle': 180.0}
    lid = poly(t, 'Arête', 0, -3, PW, PH, hexc(WHT), verts, prof)
    t.layer_ref(lid)['props']['opacity'] = 0.0
    for i, (k, a, cat, name, alt) in enumerate(COLS):
        x = PX + PW * k / km_max
        y = PY + PH * (1 - a / top)
        m = t.group('Col %d' % (i + 1), x - 120, y - 92, 240, 84, g, tracks=merge(fade(140, 40 + i * 8, 10), inout('y', y - 92, y - 60, 140, 40 + i * 8, 14)))
        col = RED if cat == 'HC' else K
        t.rect('Catégorie', 100, 0, 40, 26, hexc(col), m, stroke=(hexc(WHT), 1.5))
        t.text('Cat', cat, 100, 0, 40, 26, 15, m, color=hexc(WHT), weight=800, align='center')
        t.text('Nom', fr(name), 0, 28, 240, 26, 19, m, color=hexc(WHT), weight=700, align='center')
        t.text('Altitude', alt, 0, 52, 240, 22, 15, m, color=hexc(MUTE), weight=500, align='center')
    for km in (0, 50, 100, 150):
        x = PX + PW * km / km_max
        t.rect('Tic', x, PY + PH, 1, 10, hexc(MUTE), g)
        t.text('Km', 'km %d' % km, x - 40, PY + PH + 12, 80, 24, 15, g, color=hexc(MUTE), weight=500, align='center')
    t.text('Départ', 'PAU', PX, PY + PH + 40, 400, 30, 18, g, color=hexc(WHT), weight=800, spacing=2)
    t.text('Arrivée', 'SUPERBAGNÈRES', PX + PW - 400, PY + PH + 40, 400, 30, 18, g, color=hexc(YEL), weight=800, spacing=2, align='right')

    # --- Gaps ---------------------------------------------------------
    t = p.template('gaps', 'Écarts', 'lower', duration=110, on_air=60)
    t.field('lead', 'Tête de course', fr('ÉCHAPPÉE · 6 COUREURS'))
    t.field('names', 'Coureurs échappés', 'Alaphilippe, Madouas, Skjelmose, Carapaz, Healy, Mohorič')
    t.field('chase', 'Groupe 2', 'CONTRE-ATTAQUE · 3')
    t.field('gap2', 'Écart groupe 2', '+ 0:48')
    t.field('bunch', 'Groupe 3', 'PELOTON · MAILLOT JAUNE')
    t.field('gap3', 'Écart peloton', '+ 2:45')
    for k, path in (('gap2', 'race.groups[1].gap'), ('gap3', 'race.groups[2].gap')):
        bind(t, k, 'grande-boucle-course', path)
    g = frame(t, 'Écarts', 80, 790, 1340, 220)
    t.rect('Fond', 0, 0, 1340, 210, hexc(K, 0.92), g, tracks=inout('scaleX', 1, 0, 60, 0, 18), anchorX=0.0)
    t.rect('Jaune', 0, 0, 12, 210, hexc(YEL), g)
    t.text('Titre', fr('SITUATION DE COURSE'), 40, 16, 600, 30, 18, g, color=hexc(MUTE), weight=800, spacing=4, tracks=fade(60, 6, 8))
    rows = (('lead', None, YEL, 'names'), ('chase', 'gap2', WHT, None), ('bunch', 'gap3', YEL, None))
    for i, (lab, gap, col, sub) in enumerate(rows):
        y = 52 + i * 52
        r = t.group('Groupe %d' % (i + 1), 40, y, 1270, 46, g, tracks=merge(fade(60, 8 + i * 4, 8), inout('x', 40, 100, 60, 8 + i * 4, 14)))
        t.rect('Plot', 0, 14, 18, 18, hexc(col), r, radius=9)
        t.text('Groupe', '{{%s}}' % lab, 34, 0, 480, 46, 26, r, color=hexc(WHT), weight=800)
        if sub:
            t.text('Coureurs', '{{%s}}' % sub, 520, 0, 740, 46, 20, r, color=hexc(MUTE), weight=500, align='right', overflow='ellipsis')
        else:
            t.rect('Trait', 520, 22, 560, 2, hexc(WHT, 0.12), r)
            t.text('Écart', '{{%s}}' % gap, 1090, 0, 180, 46, 38, r, color=hexc(col), align='right', **C)

    # --- General classification ----------------------------------------
    t = p.template('gc', 'Classement général', 'full', duration=180, on_air=120)
    t.field('title', 'Titre', fr('CLASSEMENT GÉNÉRAL'))
    t.field('after', 'Sous-titre', fr('après la 14e étape'))
    t.field('rows', 'Classement', table_value([
        ['Rang', 'Coureur', 'Équipe', 'Temps'], ['1', 'Tadej Pogačar', 'UAE Team Emirates', '58 h 12 min 40 s'],
        ['2', 'Jonas Vingegaard', 'Visma-Lease a Bike', '+ 1:52'], ['3', 'Remco Evenepoel', 'Soudal Quick-Step', '+ 3:16'],
        ['4', 'Primož Roglič', 'Red Bull-Bora', '+ 4:40'], ['5', 'Carlos Rodríguez', 'Ineos Grenadiers', '+ 6:05'],
        ['6', 'Lenny Martinez', 'Bahrain Victorious', '+ 7:31'], ['7', 'Mikel Landa', 'Soudal Quick-Step', '+ 8:02'],
        ['8', 'Felix Gall', 'Decathlon AG2R', '+ 9:47']]), kind='table')
    bind(t, 'rows', 'grande-boucle-classement', 'general')
    g = t.group('Classement', 260, 110, 1400, 860, tracks=merge(fade(120, 0, 10, 12, 8), inout('y', 110, 160, 120, 0, 18, 12, 8)))
    t.rect('Fond', 0, 0, 1400, 860, hexc(K, 0.95), g)
    t.rect('Maillot', 0, 0, 1400, 120, hexc(YEL), g)
    t.text('Titre', '{{title}}', 50, 14, 1000, 64, 52, g, color=hexc(K), weight=800)
    t.text('Après', '{{after}}', 52, 70, 1000, 36, 24, g, color=hexc(K, 0.75), weight=600)
    t.widget('table', 'Classement', 50, 150, 1300, 680, g, {'fontSize': 30.0}, link='{{rows}}', headerRow=False, rowH=78.0, pad=24.0,
             gap=6.0, perPage=8, indicator=False, widthMode='percent',
             cols=[{'src': 'Rang', 'label': '', 'pct': 8.0, 'align': 'center', 'txt': {'family': COND, 'weight': 800, 'size': 38.0}},
                   {'src': 'Coureur', 'label': '', 'pct': 38.0, 'txt': {'weight': 800}},
                   {'src': 'Équipe', 'label': '', 'pct': 30.0, 'txt': {'weight': 500, 'size': 24.0}},
                   {'src': 'Temps', 'label': '', 'pct': 24.0, 'align': 'right', 'txt': {'family': COND, 'weight': 700, 'size': 36.0}}],
             st={'header': {'bg': '#00000000', 'color': MUTE}, 'hcol': {'bg': PANEL, 'color': WHT},
                 'odd': {'bg': PANEL, 'color': WHT}, 'even': {'bg': '#22252b', 'color': WHT}, 'hl': {'bg': YEL, 'color': K},
                 'top3': {'bg': PANEL, 'color': WHT}, 'tableBg': '#00000000'},
             borders={'outer': {'on': False, 'width': 0.0, 'color': '#00000000'}, 'innerH': {'on': False, 'width': 1.0, 'color': '#00000000'},
                      'innerV': {'on': False, 'width': 1.0, 'color': '#00000000'}},
             cf={'neg': False, 'top3': False, 'topBar': False, 'hlBar': False}, hlKey='Tadej Pogačar', headerColOn=True, headerCol='Coureur',
             txt={'title': {}, 'header': {}, 'rows': {'family': SANS, 'weight': 600, 'size': 30.0}},
             cascade={'on': True, 'style': 'slide', 'offset': 60.0, 'dur': 400.0})

    # --- Rider card ---------------------------------------------------------
    t = p.template('rider', 'Fiche coureur', 'lower', duration=120, on_air=70)
    t.field('jersey', 'Maillot', fr('MAILLOT JAUNE'))
    t.field('jcolor', 'Couleur du maillot', YEL, kind='color')
    t.field('name', 'Coureur', fr('Tadej Pogačar'))
    t.field('team', 'Équipe', 'UAE Team Emirates')
    t.field('nat', 'Pays', 'SLO')
    t.field('s1', 'Stat 1', '3'), t.field('l1', 'Libellé 1', fr('VICTOIRES D’ÉTAPE'))
    t.field('s2', 'Stat 2', '1er'), t.field('l2', 'Libellé 2', fr('AU GÉNÉRAL'))
    t.field('s3', 'Stat 3', '+ 1:52'), t.field('l3', 'Libellé 3', fr('D’AVANCE'))
    g = frame(t, 'Coureur', 80, 760, 1320, 240)
    t.rect('Fond', 0, 0, 1320, 240, hexc(K, 0.94), g, anchorX=0.0, tracks=inout('scaleX', 1, 0, 70, 0, 18))
    j = t.group('Maillot', 0, 0, 240, 240, g, clip=True, tracks=wipe(70, prop='cropBottom', t_in=4, length=16))
    t.rect('Couleur', 0, 0, 240, 240, hexc(YEL), j, bindings={'fill': 'jcolor'})
    poly(t, 'Maillot', 50, 52, 140, 136, hexc(K, 0.85), [(0.3, 0), (0.42, 0.1), (0.58, 0.1), (0.7, 0), (1, 0.18), (0.88, 0.42), (0.78, 0.36),
                                                           (0.78, 1), (0.22, 1), (0.22, 0.36), (0.12, 0.42), (0, 0.18)], j, round_=6)
    t.text('Maillot', '{{jersey}}', 270, 26, 700, 30, 20, g, color=hexc(YEL), weight=800, spacing=4, tracks=fade(70, 8, 8))
    t.text('Nom', '{{name}}', 268, 56, 900, 84, 66, g, color=hexc(WHT), weight=800, tracks=merge(fade(70, 8, 8), inout('x', 268, 330, 70, 8, 16)))
    t.text('Équipe', '{{team}}  ·  {{nat}}', 270, 138, 900, 36, 24, g, color=hexc(MUTE), weight=600, tracks=fade(70, 12, 8))
    for i in range(3):
        x = 270 + i * 340
        st = t.group('Stat %d' % (i + 1), x, 180, 330, 48, g, tracks=fade(70, 16 + i * 3, 8))
        t.text('Valeur', '{{s%d}}' % (i + 1), 0, 0, 110, 48, 38, st, color=hexc(WHT), **C)
        t.text('Libellé', '{{l%d}}' % (i + 1), 116, 0, 214, 48, 15, st, color=hexc(MUTE), weight=700, spacing=2)

    # --- Route map -------------------------------------------------------
    t = p.template('map', 'Carte de l’étape', 'map', duration=200, on_air=140)
    t.field('title', 'Titre', fr('Étape 14'))
    t.field('route', 'Parcours', fr('Pau › Superbagnères'))
    g = frame(t, 'Carte', 1170, 200, 680, 620)
    c = t.group('Carte', 0, 0, 680, 620, g, tracks=merge(fade(140, 0, 12), inout('x', 0, 60, 140, 0, 18)))
    t.rect('Fond', 0, 0, 680, 620, hexc(K, 0.94), c)
    t.rect('Jaune', 0, 0, 680, 8, hexc(YEL), c)
    t.text('Titre', '{{title}}', 30, 22, 300, 40, 20, c, color=hexc(YEL), weight=800, spacing=3, upper=True)
    t.text('Parcours', '{{route}}', 30, 56, 620, 50, 36, c, color=hexc(WHT), weight=800)
    t.widget('map', 'Carte', 20, 120, 640, 480, c, {}, style='vector', projection='mercator', radius=0.0, showRoads=False,
             view={'lat': 42.98, 'lon': 0.12, 'zoom': 8.4, 'rot': 0.0, 'tilt': 0.0}, views=[],
             colors={'water': '#0e2236', 'land': '#2a2e36', 'border': '#6b7280', 'grid': ''},
             layers={'markers': True, 'routes': True, 'zones': False, 'labels': True},
             markers=[{'id': 'start', 'on': True, 'name': 'Pau', 'lat': '43.2951', 'lon': '-0.3708', 'icon': 'dot', 'color': WHT},
                      {'id': 'tourmalet', 'on': True, 'name': 'Tourmalet', 'lat': '42.9086', 'lon': '0.1450', 'icon': 'dot', 'color': RED},
                      {'id': 'finish', 'on': True, 'name': 'Superbagnères', 'lat': '42.7600', 'lon': '0.5800', 'icon': 'pin', 'color': YEL}],
             mkSize=28.0, routes=[{'id': 'stage', 'on': True, 'name': 'Étape 14',
                                   'path': '43.2951,-0.3708 > 43.1,-0.05 > 42.9086,0.1450 > 42.92,0.33 > 42.83,0.40 > 42.79,0.53 > 42.76,0.58',
                                   'color': YEL, 'dur': 4.0}], curve='straight', routeW=6.0, planeHead=False,
             labels={'cities': True, 'countries': False, 'minImp': 8.0, 'size': 18.0, 'pad': 8.0},
             attribution={'provider': 'own', 'pos': 'br', 'style': 'minimal', 'show': True})

    p.item(fr('Profil de l’étape'), 'profile', 'stage')
    p.item('Carte du parcours', 'map', 'stage')
    p.item('Kilomètres restants', 'km', 'stage')
    p.item(fr('Écarts · échappée'), 'gaps', 'stage')
    p.item(fr('Tadej Pogačar'), 'rider', 'stage')
    p.item('Lenny Martinez · pois', 'rider', 'stage', {'jersey': fr('MAILLOT À POIS'), 'jcolor': '#ffffff', 'name': 'Lenny Martinez',
                                                      'team': 'Bahrain Victorious', 'nat': 'FRA', 's1': '64', 'l1': 'POINTS MONTAGNE',
                                                      's2': '6e', 'l2': fr('AU GÉNÉRAL'), 's3': '2', 'l3': 'COLS EN TÊTE'})
    p.item(fr('Classement général'), 'gc', 'stage')
    p.source('grande-boucle-course', 'Grande Boucle · course en direct', {'race': {
        'kmToGo': 42.6, 'groups': [{'name': fr('Échappée'), 'riders': 6, 'gap': '0:00'},
                                   {'name': 'Contre-attaque', 'riders': 3, 'gap': '+ 0:48'},
                                   {'name': 'Peloton', 'riders': 98, 'gap': '+ 2:45'}]}})
    p.source('grande-boucle-classement', 'Grande Boucle · classement général', {'general': [
        ['Rang', 'Coureur', 'Équipe', 'Temps'], ['1', fr('Tadej Pogačar'), 'UAE Team Emirates', '58 h 12 min 40 s'],
        ['2', 'Jonas Vingegaard', 'Visma-Lease a Bike', '+ 1:52'], ['3', 'Remco Evenepoel', 'Soudal Quick-Step', '+ 3:16'],
        ['4', fr('Primož Roglič'), 'Red Bull-Bora', '+ 4:40'], ['5', 'Carlos Rodríguez', 'Ineos Grenadiers', '+ 6:05'],
        ['6', 'Lenny Martinez', 'Bahrain Victorious', '+ 7:31'], ['7', 'Mikel Landa', 'Soudal Quick-Step', '+ 8:02'],
        ['8', 'Felix Gall', 'Decathlon AG2R', '+ 9:47']]})
    return p


# =====================================================================
# 19 · Matinale — « Le 6-9 »
# Daylight: white cards, sunrise coral, sunshine yellow and a sky blue
# on deep ink; Montserrat over Inter; soft shadows, friendly bounces.
# =====================================================================

def ex19():
    INK, CORAL, SUN, SKY, CREAM, WHT, GREY = '#1d1b2e', '#ff6b4a', '#ffc145', '#3aa0ff', '#fff6ec', '#ffffff', '#6b6880'
    p = Package('ex-19-matinale', 'Matinale', 'Magazine',
                ['Matinale', 'Météo', 'Trafic', 'Réseaux sociaux', 'Horoscope', 'Agenda'],
                fr("L’habillage lumineux d’une matinale : bandeau permanent avec l’heure, la météo des villes et l’info trafic qui "
                   "défile, sujets à la une, mur des messages des téléspectateurs, carte météo des régions, horoscope du jour et "
                   "programme de l’émission. Trafic, météo et messages peuvent venir de sources de données."),
                en=('Morning show', 'A bright morning show package: a permanent strip with the time, city weather and scrolling traffic news, '
                    'topic banners, a viewers’ message wall, a regional weather map, the daily horoscope and the show’s running order. '
                    'Traffic, weather and messages may come from data sources.'),
                cover='strip')
    p.zone('strip', 'Bandeau', 40, '#ff6b4a', area=(0.0, 0.88, 1.0, 0.12))
    p.zone('lower', 'Sujet', 10, '#7c5cff', area=(0.04, 0.62, 0.8, 0.26))
    p.zone('side', 'Programme', 20, '#22c55e', area=(0.66, 0.06, 0.31, 0.8))
    p.zone('full', 'Plein écran', 50, '#38bdf8')
    p.playlist('morning', 'Le 6-9 · ce matin')
    p.guides(action_safe=0.035, title_safe=0.05)
    CARD = dict(shadow=(hexc('#1d1b2e', 0.25), 0, 12, 36))

    def soft(t, g, on_air):
        t.rect('Fond', 0, 0, 1920, 1080, hexc(CREAM), g)
        t.ellipse('Soleil', 1100, -500, 1300, 1200, hexc(SUN, 0.45), g, grad=('radial', hexs(CREAM, 0)),
                  tracks={'y': keys((0, -560, SMOOTH), (on_air + 40, -460, 'linear'))})
        t.ellipse('Corail', -400, 500, 1300, 1000, hexc(CORAL, 0.25), g, grad=('radial', hexs(CREAM, 0)))
        t.ellipse('Ciel', 600, 700, 1200, 900, hexc(SKY, 0.16), g, grad=('radial', hexs(CREAM, 0)))

    def sun_icon(t, x, y, size, parent):
        sg = t.group('Soleil', x, y, size, size, parent, tracks={'rotation': keys((0, 0, 'linear'), (400, 90, 'linear'))})
        t.widget('polygon', 'Rayons', 0, 0, size, size, sg, {'fill': hexc(SUN)}, mode='regular', sides=8.0, star=True, inner=62.0)
        t.ellipse('Disque', size * 0.24, size * 0.24, size * 0.52, size * 0.52, hexc(SUN), sg, stroke=(hexc(WHT), 3))
        return sg

    # --- Permanent strip -------------------------------------------------
    t = p.template('strip', 'Bandeau horaire', 'strip', duration=80, on_air=40)
    t.field('cities', 'Météo des villes', fr('PARIS  14°\nLYON  16°\nMARSEILLE  19°\nLILLE  11°\nBORDEAUX  17°'), kind='multiline')
    t.field('traffic', 'Trafic', fr('A86 : bouchon de 6 km à Nanterre\nPériphérique intérieur : ralentissements porte de Bagnolet\n'
                                     'RER B : trafic interrompu entre Gare du Nord et Aéroport CDG\nA7 : circulation fluide vers Lyon'), kind='multiline')
    bind(t, 'traffic', 'matinale-trafic', 'items[*].title')
    g = frame(t, 'Bandeau', 48, 966, 1824, 82)
    a = t.group('Barre', 0, 0, 1824, 82, g, tracks=merge(inout('y', 0, 110, 40, 0, 18), fade(40, 0, 8)))
    t.rect('Blanc', 0, 0, 1824, 82, hexc(WHT, 0.97), a, radius=22, **CARD)
    t.rect('Heure', 0, 0, 210, 82, hexc(CORAL), a, corners=(22, 0, 0, 22))
    t.clock('Horloge', 0, 0, 210, 82, 44, a, fmt='HH:mm', color=hexc(WHT), family=GEO, weight=800, sepBlink=True)
    sun_icon(t, 232, 17, 48, a)
    t.ticker('Villes', '{{cities}}', 292, 0, 230, 82, 30, a, color=hexc(INK), family=GEO, weight=800, mode='flip', itemDur=3.5,
             transDur=0.5, trans='slide')
    t.rect('Séparation', 532, 20, 2, 42, hexc(INK, 0.12), a)
    t.rect('Trafic', 556, 19, 132, 44, hexc(SKY), a, radius=22)
    t.text('Trafic', 'TRAFIC', 556, 19, 132, 44, 18, a, color=hexc(WHT), family=GEO, weight=800, align='center', spacing=2)
    t.ticker('Infos trafic', '{{traffic}}', 710, 0, 1090, 82, 28, a, color=hexc(INK), weight=600, speed=120, sepSymbol='●',
             sepColor=CORAL, spacing=26.0, fade=True, fadeW=60.0, startFull=True)

    # --- Topic banner -----------------------------------------------------
    t = p.template('topic', 'Sujet', 'lower', duration=110, on_air=60)
    t.field('tag', 'Étiquette', fr('À LA UNE'))
    t.field('title', 'Titre', fr('Canicule : les écoles adaptent leurs horaires dès lundi'))
    t.field('sub', 'Précision', fr('Ouverture à 7 h 30 dans 14 départements · avec notre envoyée spéciale à Lyon'))
    g = frame(t, 'Sujet', 96, 720, 1400, 220)
    c = t.group('Carte', 0, 40, 1400, 170, g, tracks=merge(inout('y', 40, 90, 60, 0, 20), fade(60, 0, 10)))
    t.rect('Blanc', 0, 0, 1400, 170, hexc(WHT, 0.97), c, radius=26, **CARD)
    t.rect('Accent', 0, 0, 12, 170, hexc(SUN), c, corners=(26, 0, 0, 26))
    t.text('Titre', '{{title}}', 46, 28, 1320, 70, 46, c, color=hexc(INK), family=GEO, weight=800,
           tracks=merge(fade(60, 6, 10), inout('x', 46, 90, 60, 6, 18)))
    t.text('Précision', '{{sub}}', 48, 100, 1320, 44, 26, c, color=hexc(GREY), weight=500, tracks=fade(60, 10, 10))
    tag = t.group('Étiquette', 34, 18, 240, 48, g, tracks=pop(60, 8, 16))
    t.rect('Corail', 0, 0, 240, 48, hexc(CORAL), tag, radius=24, shadow=(hexc(CORAL, 0.4), 0, 6, 16))
    t.text('Étiquette', '{{tag}}', 0, 0, 240, 48, 20, tag, color=hexc(WHT), family=GEO, weight=800, align='center', spacing=2)

    # --- Social wall -------------------------------------------------------
    POSTS = [('Margaux', '@margaux_b', 'Déjà 3 cafés et toujours pas réveillée… merci pour la bonne humeur de l’équipe ce matin !', '248', CORAL),
             ('Rachid', '@rachid.k', 'Sur l’A86 depuis 40 minutes, heureusement que vous êtes là pour passer le temps 😅', '131', SKY),
             ('Lucie', '@lulu_dij', 'Bonjour de Dijon ! Il fait déjà 22° à 7 h, on prépare les glaces pour cet après-midi.', '97', SUN)]
    t = p.template('social', 'Mur des messages', 'full', duration=170, on_air=110)
    t.field('title', 'Titre', 'VOS MESSAGES')
    t.field('hashtag', 'Mot-dièse', '#Le69')
    for i, (n, h, txt, likes, col) in enumerate(POSTS, 1):
        t.field('n%d' % i, 'Nom %d' % i, n)
        t.field('h%d' % i, 'Compte %d' % i, h)
        t.field('t%d' % i, 'Message %d' % i, fr(txt), kind='multiline')
        t.field('l%d' % i, 'J’aime %d' % i, likes, kind='number')
        for k, path in (('n', 'name'), ('h', 'handle'), ('t', 'text'), ('l', 'likes')):
            bind(t, '%s%d' % (k, i), 'matinale-messages', 'posts[%d].%s' % (i - 1, path))
    g = t.group('Messages', 0, 0, 1920, 1080, tracks=fade(110, 0, 12, 12, 10))
    soft(t, g, 110)
    t.text('Titre', '{{title}}', 120, 110, 1000, 90, 72, g, color=hexc(INK), family=GEO, weight=800, tracks=fade(110, 4, 10))
    t.text('Mot-dièse', '{{hashtag}}', 900, 110, 900, 90, 52, g, color=hexc(CORAL), family=GEO, weight=800, align='right',
           tracks=pop(110, 10, 16))
    for i, (n, h, txt, likes, col) in enumerate(POSTS, 1):
        x = 120 + (i - 1) * 572
        c = t.group('Message %d' % i, x, 260, 540, 620, g, tracks=merge(fade(110, 8 + i * 5, 10, 10, (3 - i) * 3),
                                                                         inout('y', 260, 340, 110, 8 + i * 5, 20, 12, (3 - i) * 3)))
        t.rect('Carte', 0, 0, 540, 620, hexc(WHT), c, radius=28, **CARD)
        t.ellipse('Avatar', 40, 40, 84, 84, hexc(col), c)
        t.text('Initiale', '{{n%d}}' % i, 40, 40, 84, 84, 40, c, color=hexc(WHT), family=GEO, weight=800, align='center', maxChars=1,
               overflow='clip')
        t.layer_ref(t.t['layers'][-1]['id'])['text'] = 'ABC'[i - 1] if False else n[0]
        t.text('Nom', '{{n%d}}' % i, 144, 44, 360, 44, 32, c, color=hexc(INK), family=GEO, weight=800)
        t.text('Compte', '{{h%d}}' % i, 144, 88, 360, 32, 22, c, color=hexc(GREY), weight=500)
        t.text('Message', '{{t%d}}' % i, 40, 160, 460, 360, 36, c, color=hexc(INK), weight=500, wrap=True, lineHeight=1.35, valign='top')
        t.rect('Filet', 40, 540, 460, 1.5, hexc(INK, 0.08), c)
        t.ellipse('Cœur', 40, 562, 26, 26, hexc(CORAL), c)
        t.counter('J’aime', '{{l%d}}' % i, 80, 556, 300, 40, 24, c, color=hexc(GREY), family=SANS, weight=600, suffix=fr(' j’aime'), duration=1.2)

    # --- Weather map -------------------------------------------------------
    TEMPS = [('Hauts-de-France', 11), ('Normandie', 12), ('Île-de-France', 14), ('Grand Est', 10), ('Bretagne', 13), ('Pays de la Loire', 14),
             ('Centre-Val de Loire', 13), ('Bourgogne-Franche-Comté', 12), ('Nouvelle-Aquitaine', 17), ('Auvergne-Rhône-Alpes', 15),
             ('Occitanie', 18), ("Provence-Alpes-Côte d'Azur", 19), ('Corse', 20)]
    t = p.template('weather', 'Météo des régions', 'full', duration=170, on_air=110)
    t.field('title', 'Titre', fr('MÉTÉO · CE MATIN'))
    t.field('date', 'Date', fr('Jeudi 22 octobre'))
    t.field('text', 'Prévision', fr('Grand soleil au sud, quelques nuages sur la Manche. Jusqu’à 24° l’après-midi à Marseille.'), kind='multiline')
    g = t.group('Météo', 0, 0, 1920, 1080, tracks=fade(110, 0, 12, 12, 8))
    soft(t, g, 110)
    sun_icon(t, 120, 130, 90, g)
    t.text('Titre', '{{title}}', 240, 120, 560, 70, 52, g, color=hexc(INK), family=GEO, weight=800, tracks=fade(110, 4, 10))
    t.text('Date', '{{date}}', 242, 186, 560, 40, 28, g, color=hexc(CORAL), family=GEO, weight=600, tracks=fade(110, 8, 10))
    t.text('Prévision', '{{text}}', 120, 300, 640, 300, 36, g, color=hexc(INK), weight=500, wrap=True, lineHeight=1.35, valign='top',
           tracks=merge(fade(110, 12, 10), inout('y', 300, 340, 110, 12, 18)))
    m = t.group('Carte', 860, 80, 960, 900, g, tracks=merge(fade(110, 6, 14), inout('x', 860, 940, 110, 6, 22)))
    t.rect('Carte', 0, 0, 960, 900, hexc(WHT), m, radius=32, **CARD)
    t.widget('map', 'France', 20, 20, 920, 860, m, {}, style='vector', projection='mercator', radius=22.0, showRoads=False,
             view={'lat': 46.5, 'lon': 2.6, 'zoom': 5.95, 'rot': 0.0, 'tilt': 0.0}, views=[],
             colors={'water': '#dcecff', 'land': '#f3eee6', 'border': '#ffffff', 'grid': ''},
             layers={'markers': False, 'routes': False, 'zones': True, 'labels': False}, markers=[], routes=[],
             zones={'level': 'regions', 'mode': 'scale', 'rows': [['Région', 'Température']] + [[r, str(v)] for r, v in TEMPS],
                    'scale': [{'v': 8, 'color': '#7cc4ff'}, {'v': 12, 'color': '#b9e3a1'}, {'v': 16, 'color': '#ffd166'},
                              {'v': 20, 'color': '#ff8c5a'}, {'v': 26, 'color': '#e8453c'}],
                    'unit': '°', 'title': fr('Températures à 8 h'), 'opacity': 0.92, 'legend': True, 'legendPos': 'bl', 'categories': []},
             labels={'cities': False, 'countries': False, 'minImp': 9.0, 'size': 18.0, 'pad': 8.0},
             attribution={'provider': 'own', 'pos': 'br', 'style': 'light', 'show': True})

    # --- Horoscope ---------------------------------------------------------
    SIGNS = [('Bélier', 'Une rencontre change vos plans.', 4), ('Taureau', 'Patience, la réponse arrive.', 3),
             ('Gémeaux', 'Journée idéale pour négocier.', 5), ('Cancer', 'Prenez du temps pour vous.', 2),
             ('Lion', 'Votre énergie fait mouche.', 5), ('Vierge', 'Rangez avant de vous lancer.', 3),
             ('Balance', 'Un choix se précise.', 4), ('Scorpion', 'Fiez-vous à votre instinct.', 4),
             ('Sagittaire', 'Un voyage se dessine.', 5), ('Capricorne', 'Les efforts paient enfin.', 4),
             ('Verseau', 'Osez l’imprévu.', 3), ('Poissons', 'Douceur et créativité.', 4)]
    t = p.template('horoscope', 'Horoscope', 'full', duration=180, on_air=120)
    t.field('title', 'Titre', 'HOROSCOPE DU JOUR')
    for i, (n, txt, mood) in enumerate(SIGNS, 1):
        t.field('t%d' % i, n, fr(txt))
        t.field('m%d' % i, n + ' · humeur (1-5)', str(mood), kind='number', min=0, max=5)
    g = t.group('Horoscope', 0, 0, 1920, 1080, tracks=fade(120, 0, 12, 12, 8))
    soft(t, g, 120)
    t.text('Titre', '{{title}}', 0, 80, 1920, 90, 64, g, color=hexc(INK), family=GEO, weight=800, align='center', tracks=fade(120, 4, 10))
    for i, (n, txt, mood) in enumerate(SIGNS, 1):
        col, row = (i - 1) % 4, (i - 1) // 4
        x, y = 120 + col * 428, 210 + row * 262
        c = t.group(n, x, y, 404, 238, g, tracks=merge(fade(120, 6 + i * 2, 10, 8, (12 - i)), inout('y', y, y + 50, 120, 6 + i * 2, 18, 10, 12 - i)))
        t.rect('Carte', 0, 0, 404, 238, hexc(WHT), c, radius=24, **CARD)
        t.text('Signe', n.upper(), 30, 22, 344, 44, 28, c, color=hexc(INK), family=GEO, weight=800, spacing=1)
        t.text('Texte', '{{t%d}}' % i, 30, 74, 344, 100, 25, c, color=hexc(GREY), weight=500, wrap=True, lineHeight=1.3, valign='top')
        t.progress('Humeur', '{{m%d}}' % i, 30, 190, 180, 14, hexc(CORAL), c, max='5', track=hexs(INK, 0.08), shape='segments',
                   segments=5, segGap=8.0, rounded=True)

    # --- Running order -------------------------------------------------
    t = p.template('agenda', 'Au programme', 'side', duration=150, on_air=90)
    t.field('title', 'Titre', 'AU PROGRAMME')
    t.field('rows', 'Programme', table_value([['Heure', 'Rubrique'], ['7:45', 'Le journal'], ['8:00', 'La météo des plages'],
                                              ['8:10', 'L’invité : Thomas Pesquet'], ['8:30', 'Le marché de Clara'],
                                              ['8:45', 'Les jeux de Max'], ['8:55', 'Le replay du week-end']]), kind='table')
    t.field('now', 'En cours (heure)', '8:10')
    g = frame(t, 'Programme', 1290, 90, 560, 820)
    c = t.group('Carte', 0, 0, 560, 820, g, tracks=merge(fade(90, 0, 10), inout('x', 0, 80, 90, 0, 20)))
    t.rect('Carte', 0, 0, 560, 820, hexc(WHT, 0.97), c, radius=30, **CARD)
    t.rect('Haut', 0, 0, 560, 120, hexc(CORAL), c, corners=(30, 30, 0, 0))
    t.text('Titre', '{{title}}', 40, 0, 480, 120, 36, c, color=hexc(WHT), family=GEO, weight=800, spacing=2)
    t.widget('table', 'Programme', 24, 150, 512, 640, c, {'fontSize': 28.0}, link='{{rows}}', headerRow=False, rowH=96.0, pad=18.0,
             gap=8.0, perPage=6, indicator=False, widthMode='percent', radius=16.0, settingBindings={'hlKey': 'now'},
             cols=[{'src': 'Heure', 'label': '', 'pct': 24.0, 'txt': {'family': GEO, 'weight': 800}},
                   {'src': 'Rubrique', 'label': '', 'pct': 76.0, 'txt': {'lines': 'multi', 'max': 2, 'size': 26.0}}],
             st={'header': {'bg': '#00000000', 'color': GREY}, 'hcol': {'bg': CREAM, 'color': INK}, 'odd': {'bg': CREAM, 'color': INK},
                 'even': {'bg': CREAM, 'color': INK}, 'hl': {'bg': SUN, 'color': INK}, 'top3': {'bg': CREAM, 'color': INK}, 'tableBg': '#00000000'},
             borders={'outer': {'on': False, 'width': 0.0, 'color': '#00000000'}, 'innerH': {'on': False, 'width': 1.0, 'color': '#00000000'},
                      'innerV': {'on': False, 'width': 1.0, 'color': '#00000000'}},
             cf={'neg': False, 'top3': False, 'topBar': False, 'hlBar': True, 'hlBarColor': CORAL},
             txt={'title': {}, 'header': {}, 'rows': {'family': SANS, 'weight': 600, 'size': 26.0}},
             cascade={'on': True, 'style': 'slide', 'offset': 60.0, 'dur': 400.0})

    p.item('Bandeau horaire', 'strip', 'morning')
    p.item(fr('À la une · canicule'), 'topic', 'morning')
    p.item('Au programme', 'agenda', 'morning')
    p.item(fr('Météo des régions'), 'weather', 'morning')
    p.item('Vos messages', 'social', 'morning')
    p.item(fr('Sujet · Pesquet en plateau'), 'topic', 'morning', {'tag': fr('L’INVITÉ'), 'title': fr('Thomas Pesquet : « La Lune, c’est pour 2028 »'),
                                                                   'sub': fr('L’astronaute est notre invité à 8 h 10')})
    p.item('Horoscope', 'horoscope', 'morning')
    p.source('matinale-trafic', 'Matinale · info trafic', {'items': [
        {'title': fr('A86 : bouchon de 6 km à Nanterre')},
        {'title': fr('Périphérique intérieur : ralentissements porte de Bagnolet')},
        {'title': fr('RER B : trafic interrompu entre Gare du Nord et Aéroport CDG')},
        {'title': fr('A7 : circulation fluide vers Lyon')}]})
    p.source('matinale-messages', 'Matinale · messages des téléspectateurs', {'posts': [
        {'name': n, 'handle': h, 'text': fr(txt), 'likes': int(likes)} for n, h, txt, likes, col in POSTS]})
    return p


# =====================================================================
# 20 · Live shopping — « Drop Live »
# Night-black glass, hot magenta and a sale yellow; Inter heavy with
# Bebas Neue prices; built for 9:16 first, with its 16:9 layout too.
# =====================================================================

def ex20():
    K, GLASS, MAG, YEL, WHT, MUTE = '#0c0b10', '#17151e', '#ff2e7e', '#ffd84d', '#ffffff', '#b3adc2'
    p = Package('ex-20-live-shopping', 'Live shopping', 'Commerce',
                ['Live shopping', 'Produit', 'Compte à rebours', 'Stock', 'QR code', '9:16'],
                fr("Une vente en direct pour les réseaux et le web, pensée d’abord en 9:16 : fiche produit avec prix barré et "
                   "remise, offre flash avec compte à rebours, stock restant qui baisse, QR code pour acheter, animatrice et "
                   "nombre de spectateurs, achats récents qui défilent et code promo. Prix et stock peuvent venir du catalogue."),
                en=('Live shopping', 'A live sale for social and the web, designed 9:16 first: product card with struck price and discount, '
                    'flash offer with a countdown, remaining stock going down, a QR code to buy, the host with the viewer count, recent '
                    'purchases scrolling and a promo code. Prices and stock may come from the catalogue.'),
                cover='product')
    p.vertical()
    p.zone('product', 'Produit', 20, '#ff2e7e', area=(0.6, 0.06, 0.37, 0.72), vertical=(0.04, 0.3, 0.92, 0.415))
    p.zone('offer', 'Offre', 30, '#ffd84d', area=(0.6, 0.79, 0.37, 0.18), vertical=(0.03, 0.72, 0.94, 0.135))
    p.zone('host', 'Animatrice', 10, '#7c5cff', area=(0.03, 0.04, 0.5, 0.13), vertical=(0.04, 0.03, 0.92, 0.09))
    p.zone('buyers', 'Achats', 15, '#22c55e', area=(0.03, 0.84, 0.55, 0.12), vertical=(0.03, 0.86, 0.94, 0.07))
    p.zone('qr', 'QR code', 25, '#38bdf8', area=(0.03, 0.4, 0.2, 0.42), vertical=(0.03, 0.115, 0.5, 0.18))
    p.playlist('live', fr('Drop Live · jeudi 20 h'))
    p.guides(action_safe=0.035, title_safe=0.05, exclusions=[
        {'id': 'ui', 'name': 'Boutons de la plateforme (9:16)', 'x': 0.84, 'y': 0.5, 'width': 0.14, 'height': 0.35}])
    CARD = dict(radius=28, stroke=(hexc('#ffffff', 0.1), 1.5), shadow=(hexc('#000000', 0.5), 0, 16, 44))

    # --- Host ------------------------------------------------------------
    t = p.template('host', 'Animatrice', 'host', duration=100, on_air=50)
    t.field('name', 'Nom', 'Sarah Benali')
    t.field('handle', 'Compte', '@sarah.drop')
    t.field('viewers', 'Spectateurs', '12840', kind='number')
    bind(t, 'viewers', 'droplive-stats', 'live.viewers')
    g = frame(t, 'Animatrice', 60, 50, 900, 120)
    c = t.group('Pilule', 0, 0, 900, 120, g, tracks=merge(inout('x', 0, -120, 50, 0, 18), fade(50, 0, 10)))
    t.rect('Verre', 0, 0, 640, 120, hexc(GLASS, 0.88), c, radius=60, stroke=(hexc('#ffffff', 0.1), 1.5))
    t.ellipse('Anneau', 12, 12, 96, 96, hexc(MAG), c, grad=(YEL, 135))
    t.ellipse('Avatar', 18, 18, 84, 84, hexc(GLASS), c)
    t.text('Initiales', 'SB', 18, 18, 84, 84, 32, c, color=hexc(WHT), weight=800, align='center')
    t.text('Nom', '{{name}}', 126, 18, 340, 46, 32, c, color=hexc(WHT), weight=800)
    t.text('Compte', '{{handle}}', 126, 62, 340, 36, 22, c, color=hexc(MUTE), weight=500)
    t.rect('Live', 470, 34, 150, 52, hexc(MAG), c, radius=26)
    t.ellipse('Point', 490, 54, 12, 12, hexc(WHT), c, tracks=pulse('opacity', 0.3, 1, 40, 100, start=10))
    t.text('Live', 'EN DIRECT', 508, 34, 104, 52, 16, c, color=hexc(WHT), weight=800, align='center', spacing=1)
    t.rect('Vues', 656, 34, 220, 52, hexc(GLASS, 0.88), c, radius=26, stroke=(hexc('#ffffff', 0.1), 1.5))
    t.counter('Spectateurs', '{{viewers}}', 656, 34, 220, 52, 24, c, color=hexc(WHT), weight=800, align='center', suffix=' en ligne',
              affixColor=MUTE, affixWeight=500, duration=1.4, compact=True, decimals=1)
    place(t, g, 44, 60, 1.08)

    # --- Product card --------------------------------------------------
    t = p.template('product', 'Fiche produit', 'product', duration=130, on_air=70)
    t.field('brand', 'Marque', 'MAISON LUNE')
    t.field('name', 'Produit', fr('Trench oversize en coton bio'))
    t.field('variant', 'Variante', fr('Beige sable · du XS au XL'))
    t.field('was', 'Prix barré', fr('189 €'))
    t.field('price', 'Prix', '129')
    t.field('cents', 'Centimes', ',90 €')
    t.field('off', 'Remise', '-32 %')
    t.field('photo', 'Photo du produit', '', kind='image')
    t.field('showPhoto', 'Afficher la photo', 'false', kind='boolean')
    for k, path in (('price', 'product.price'), ('was', 'product.was'), ('name', 'product.name')):
        bind(t, k, 'droplive-catalogue', 'products[sku={sku}].' + path.split('.')[1])
    t.field('sku', 'Référence (catalogue)', 'ML-TR-042')
    g = frame(t, 'Produit', 1160, 70, 690, 760)
    c = t.group('Carte', 0, 0, 690, 760, g, tracks=merge(inout('x', 0, 140, 70, 0, 20), fade(70, 0, 10)))
    t.rect('Verre', 0, 0, 690, 760, hexc(GLASS, 0.92), c, **CARD)
    vis = t.group('Visuel', 24, 24, 642, 360, c, clip=True)
    t.rect('Dégradé', 0, 0, 642, 360, hexc('#2a1f3a'), vis, radius=20, grad=('#5b1d43', 135))
    t.ellipse('Lueur', 280, -120, 520, 520, hexc(MAG, 0.45), vis, grad=('radial', hexs(MAG, 0)))
    t.text('Monogramme', '{{brand}}', 0, 0, 642, 360, 64, vis, color=hexc(WHT, 0.16), family=DISPLAY, weight=400, align='center', spacing=12)
    t.image('Photo', '{{photo}}', 0, 0, 642, 360, vis, fit='cover', mask='rounded', radius=20.0)
    t.layer_ref(t.t['layers'][-1]['id'])['bindings'] = {'opacity': 'showPhoto'}
    t.rect('Remise', 482, 20, 140, 64, hexc(YEL), vis, radius=32, tracks=pop(70, 14, 16))
    t.text('Remise', '{{off}}', 482, 20, 140, 64, 28, vis, color=hexc(K), weight=800, align='center', tracks=pop(70, 14, 16))
    t.text('Marque', '{{brand}}', 34, 404, 620, 32, 18, c, color=hexc(MAG), weight=800, spacing=4)
    t.text('Produit', '{{name}}', 32, 436, 626, 110, 42, c, color=hexc(WHT), weight=800, wrap=True, lineHeight=1.12, valign='top', maxLines=2)
    t.text('Variante', '{{variant}}', 34, 548, 620, 34, 22, c, color=hexc(MUTE), weight=500)
    t.rect('Filet', 34, 598, 622, 1.5, hexc('#ffffff', 0.1), c)
    t.text('Prix barré', '{{was}}', 34, 624, 200, 50, 30, c, color=hexc(MUTE), weight=600, decoration='lineThrough')
    pr = t.group('Prix', 200, 604, 460, 130, c, tracks=pop(70, 10, 18))
    t.text('Prix', '{{price}}<c=' + YEL + '>{{cents}}</c>', 0, 0, 456, 130, 128, pr, color=hexc(WHT), family=DISPLAY, weight=400, align='right')
    place(t, g, 195, 590, 1.0)
    t.override(c, 'vertical', tracks=merge(inout('y', 0, 140, 70, 0, 20), fade(70, 0, 10)))

    # --- Flash offer ----------------------------------------------------
    t = p.template('offer', 'Offre flash', 'offer', duration=110, on_air=50)
    t.field('label', 'Texte', 'OFFRE FLASH')
    t.field('mins', 'Durée (min)', '10', kind='number')
    t.field('left', 'Stock restant', '37', kind='number')
    t.field('stock', 'Stock de départ', '200', kind='number')
    bind(t, 'left', 'droplive-catalogue', 'products[sku=ML-TR-042].stock')
    g = frame(t, 'Offre', 1160, 860, 690, 170)
    c = t.group('Carte', 0, 0, 690, 170, g, tracks=merge(inout('y', 0, 80, 50, 0, 18), fade(50, 0, 10)))
    t.rect('Fond', 0, 0, 690, 170, hexc(MAG), c, radius=28, grad=('#c2185b', 90), shadow=(hexc(MAG, 0.45), 0, 14, 40))
    t.text('Libellé', '{{label}}', 30, 18, 330, 40, 26, c, color=hexc(WHT), weight=800, spacing=3)
    t.text('Fin dans', 'SE TERMINE DANS', 30, 56, 330, 30, 16, c, color=hexc(WHT, 0.8), weight=700, spacing=2)
    t.clock('Décompte', 430, 6, 240, 100, 84, c, mode='countdown', fmt='mm:ss', color=hexc(WHT), family=DISPLAY, weight=400,
            align='right', cdDuration=600000, startMode='auto', tabular=True, zeroTrigger='fin-offre',
            steps=[{'on': True, 'at': 60, 'color': YEL, 'bg': '', 'blink': True}])
    t.text('Stock', fr('Plus que <c=' + YEL + '>{{left}}</c> pièces'), 30, 104, 420, 40, 24, c, color=hexc(WHT), weight=700)
    t.progress('Stock', '{{left}}', 30, 146, 630, 10, hexc(YEL), c, max='{{stock}}', track=hexs('#000000', 0.25), rounded=True,
               duration=1.4, steps=[{'on': True, 'below': 20.0, 'color': WHT}])
    place(t, g, 64, 1405, 1.38)

    # --- QR to buy ------------------------------------------------------
    t = p.template('qr', 'QR code d’achat', 'qr', duration=100, on_air=50)
    t.field('url', 'Lien d’achat', 'https://droplive.shop/p/ML-TR-042?utm=live')
    t.field('short', 'Lien court', 'droplive.shop/trench')
    g = frame(t, 'QR', 60, 440, 360, 440)
    c = t.group('Carte', 0, 0, 360, 440, g, tracks=pop(50, 0, 18))
    t.rect('Verre', 0, 0, 360, 440, hexc(GLASS, 0.92), c, **CARD)
    t.text('Titre', 'SCANNEZ\nPOUR ACHETER', 30, 22, 300, 70, 24, c, color=hexc(WHT), weight=800, lineHeight=1.15, spacing=1)
    t.rect('Blanc', 30, 104, 300, 260, hexc(WHT), c, radius=18)
    t.qr('QR', '{{url}}', 60, 114, 240, c, fill=hexc(K), modStyle='dots', finderStyle='rounded', quiet=1, ecl='Q')
    t.text('Lien', '{{short}}', 20, 378, 320, 40, 20, c, color=hexc(MAG), weight=700, align='center')
    place(t, g, 44, 232, 0.74)

    # --- Recent purchases -------------------------------------------------
    t = p.template('buyers', 'Achats récents', 'buyers', duration=100, on_air=50)
    t.field('feed', 'Achats', fr('Julie (Lyon) vient de commander le trench\nKarim (Lille) en a pris 2\nEmma (Nantes) vient de commander le trench\n'
                                 'Léo (Bordeaux) l’a ajouté à son panier'), kind='multiline')
    bind(t, 'feed', 'droplive-stats', 'orders[*].text')
    g = frame(t, 'Achats', 60, 920, 1000, 100)
    c = t.group('Pilule', 0, 0, 1000, 100, g, tracks=merge(inout('y', 0, 60, 50, 0, 16), fade(50, 0, 10)))
    t.rect('Verre', 0, 0, 1000, 100, hexc(GLASS, 0.88), c, radius=50, stroke=(hexc('#ffffff', 0.1), 1.5))
    t.ellipse('Sac', 16, 16, 68, 68, hexc(YEL), c)
    t.rect('Sac anse', 38, 30, 24, 16, hexc(K, 0), c, radius=8, stroke=(hexc(K), 3))
    t.rect('Sac corps', 33, 40, 34, 28, hexc(K), c, radius=4)
    t.ticker('Achats', '{{feed}}', 104, 0, 870, 100, 28, c, color=hexc(WHT), weight=700, mode='roll', itemDur=3.0, transDur=0.45)
    place(t, g, 40, 1665, 1.0)

    # --- Promo code -------------------------------------------------------
    t = p.template('promo', 'Code promo', 'offer', duration=110, on_air=60)
    t.field('code', 'Code', 'LIVE20')
    t.field('text', 'Texte', fr('-20 % sur tout le live avec le code'))
    t.field('until', 'Validité', fr('valable jusqu’à 22 h'))
    g = frame(t, 'Code', 1160, 860, 690, 170)
    c = t.group('Carte', 0, 0, 690, 170, g, tracks=merge(inout('y', 0, 80, 60, 0, 18), fade(60, 0, 10)))
    t.rect('Fond', 0, 0, 690, 170, hexc(YEL), c, radius=28, shadow=(hexc(YEL, 0.35), 0, 14, 40))
    t.text('Texte', '{{text}}', 30, 20, 630, 40, 26, c, color=hexc(K), weight=800)
    dashed(t, t.rect('Ticket', 30, 70, 380, 80, hexc(K, 0.06), c, radius=16, stroke=(hexc(K), 3)))
    t.text('Code', '{{code}}', 30, 70, 380, 80, 64, c, color=hexc(K), family=DISPLAY, weight=400, align='center', spacing=8,
           tracks=pop(60, 12, 16))
    t.text('Validité', '{{until}}', 430, 70, 230, 80, 20, c, color=hexc(K, 0.75), weight=600, wrap=True, lineHeight=1.2)
    place(t, g, 64, 1405, 1.38)

    p.item('Animatrice', 'host', 'live')
    p.item('Trench Maison Lune', 'product', 'live')
    p.item('QR code d’achat', 'qr', 'live')
    p.item('Offre flash 10 min', 'offer', 'live')
    p.item('Achats récents', 'buyers', 'live')
    p.item('Code LIVE20', 'promo', 'live')
    p.item('Sneakers Pas de Côté', 'product', 'live', {'brand': fr('PAS DE CÔTÉ'), 'name': 'Sneakers en cuir recyclé', 'variant': fr('Blanc cassé · du 36 au 46'),
                                                      'was': fr('139 €'), 'price': '89', 'cents': ',00 €', 'off': '-36 %', 'sku': 'PDC-SN-310'})
    p.source('droplive-stats', 'Drop Live · audience et commandes', {
        'live': {'viewers': 12840},
        'orders': [{'text': fr('Julie (Lyon) vient de commander le trench')}, {'text': 'Karim (Lille) en a pris 2'},
                   {'text': fr('Emma (Nantes) vient de commander le trench')},
                   {'text': fr('Léo (Bordeaux) l’a ajouté à son panier')}]})
    p.source('droplive-catalogue', 'Drop Live · catalogue', {'products': [
        {'sku': 'ML-TR-042', 'name': fr('Trench oversize en coton bio'), 'price': '129', 'was': fr('189 €'), 'stock': 37},
        {'sku': 'PDC-SN-310', 'name': 'Sneakers en cuir recyclé', 'price': '89', 'was': fr('139 €'), 'stock': 112}]})
    return p


EXAMPLES = [ex11, ex12, ex13, ex14, ex15, ex16, ex17, ex18, ex19, ex20]

if __name__ == '__main__':
    only = sys.argv[1:]
    for build in EXAMPLES:
        pkg = build()
        if only and not any(o in pkg.meta['id'] for o in only):
            continue
        print(pkg.write())
