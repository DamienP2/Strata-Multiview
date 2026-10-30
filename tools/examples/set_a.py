#!/usr/bin/env python3
"""Examples 01-10 (news, continuous news, football, election night, weather,
markets, talk show, gaming stream, motor racing, tennis): complete channel
packages written with kit.py. Run: python3 tools/examples/set_a.py
"""
import json
import os
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from kit import *  # noqa: E402,F403

N = NBSP


def glow(t, name, x, y, size, color, alpha, parent, **kw):
    """A soft light: radial gradient fading to nothing well before its edge."""
    c = color.lstrip('#')[:6]
    return t.ellipse(name, x, y, size, size, hexc(c, alpha), parent,
                     grad=('radial', '#' + c + '00', [(20, hexs(c, alpha * 0.8)), (45, hexs(c, alpha * 0.42)), (70, hexs(c, alpha * 0.14))]), **kw)


# ======================================================================
# 01 · Journal télévisé national
# ======================================================================
def ex01():
    NAVY, NAVY2, BLUE, SKY, RED, MIST = '#08142c', '#0f2450', '#1f5bff', '#7fb2ff', '#ff2b3a', '#c8d4ea'
    p = Package(
        'ex-01-journal-televise', 'Journal télévisé', 'Info',
        ['Synthés', 'Logo et heure', 'Urgent', 'Titres', 'Sommaire', 'Ticker'],
        'L’habillage complet d’un grand journal du soir : synthés nom et fonction dont les cartouches suivent la longueur du texte, lieu en direct, logo et heure, bandeau URGENT, titre plein écran, sommaire et ticker. Bleu nuit, blanc et un bleu électrique, typographie Inter.'.replace(' :', N + ':'),
        en=('National TV news', 'A full evening news package: name and title lower thirds whose plates follow the text, live location, logo and clock, BREAKING banner, full-screen headline, running order and ticker.'),
        cover='headline')
    p.zone('lower', 'Synthé', 20, '#7c5cff', (0.05, 0.68, 0.64, 0.24))
    p.zone('loc', 'Lieu', 25, '#22c55e', (0.05, 0.05, 0.42, 0.09))
    p.zone('bug', 'Logo et heure', 60, '#38bdf8', (0.80, 0.05, 0.15, 0.09))
    p.zone('banner', 'Bandeau urgent', 30, '#ff3b5c', (0, 0.70, 1, 0.21))
    p.zone('ticker', 'Ticker', 40, '#f5a524', (0, 0.905, 1, 0.07))
    p.zone('full', 'Plein écran', 45, '#94a3b8')
    p.guides(exclusions=[{'id': 'bug', 'name': 'Logo', 'x': 0.80, 'y': 0.05, 'width': 0.15, 'height': 0.09}])
    p.playlist('ouverture', 'Ouverture')
    p.playlist('journal', 'Journal')
    p.playlist('urgent', 'Édition spéciale')

    # -- Synthé nom / fonction --------------------------------------------
    t = p.template('lt', 'Synthé nom et fonction', 'lower', duration=70, on_air=45)
    t.field('name', 'Nom', 'Claire Vasseur')
    t.field('role', 'Fonction', 'Ministre de la Transition écologique')
    g = t.group('Synthé', 120, 812, 1180, 150)
    t.rect('Filet', 0, 0, 10, 148, hexc(BLUE), g, tracks=inout('cropBottom', 0, 1, 45, 0, 14, 10, 6))
    plate = t.rect('Cartouche nom', 10, 0, 640, 94, WHITE, g, tracks=inout('cropRight', 0, 1, 45, 4, 20, 12, 4))
    sub = t.rect('Cartouche fonction', 10, 94, 640, 54, hexc(NAVY, 0.94), g, tracks=inout('cropRight', 0, 1, 45, 10, 20, 12, 0))
    name = t.text('Nom', '{{name}}', 42, 0, 1110, 94, 52, g, color=hexc(NAVY), weight=800, spacing=-0.5,
                  tracks=merge(inout('opacity', 1, 0, 45, 12, 14, 8), inout('x', 42, 12, 45, 12, 20, 8)))
    role = t.text('Fonction', '{{role}}', 42, 94, 1110, 54, 27, g, color=WHITE, weight=500,
                  tracks=merge(inout('opacity', 1, 0, 45, 18, 14, 8), inout('x', 42, 12, 45, 18, 20, 8)))
    t.fit(name, [(plate, 'width')], pad=72, minimum=420)
    t.fit(role, [(sub, 'width')], pad=72, minimum=320)
    for n_, r_ in (('Claire Vasseur', 'Ministre de la Transition écologique'), ('Marc Delorme', 'Envoyé spécial à Bruxelles'),
                   ('Inès Benali', 'Économiste, Institut Montaigne-Sud'), ('Paul Girard', 'Maire de Montreuil-sur-Loire')):
        p.item(n_, 'lt', 'journal', {'name': n_, 'role': r_})

    # -- Lieu en direct ------------------------------------------------------
    t = p.template('loc', 'Lieu en direct', 'loc', duration=70, on_air=40)
    t.field('city', 'Ville', 'PARIS')
    t.field('place', 'Lieu', 'Palais de l’Élysée')
    t.field('live', 'En direct', 'true', kind='boolean')
    g = t.group('Lieu', 120, 70, 780, 56, clip=True, tracks=inout('cropRight', 0, 1, 40, 0, 20, 12))
    lv = t.group('Direct', 0, 0, 176, 56, g, bindings={'opacity': 'live'})
    t.rect('Fond direct', 0, 0, 176, 56, hexc(RED), lv)
    t.ellipse('Point', 20, 21, 14, 14, WHITE, lv, tracks=pulse('opacity', 0.25, 1, 50, 2000))
    t.text('DIRECT', 'DIRECT', 44, 0, 124, 56, 22, lv, weight=800, spacing=2)
    cityp = t.rect('Fond ville', 176, 0, 300, 56, WHITE, g)
    city = t.text('Ville', '{{city}}', 196, 0, 400, 56, 24, g, color=hexc(NAVY), weight=800, spacing=2, upper=True)
    placep = t.rect('Fond lieu', 476, 0, 284, 56, hexc(NAVY, 0.9), g)
    place = t.text('Lieu', '{{place}}', 496, 0, 560, 56, 24, g, weight=500)
    t.fit(city, [(placep, 'x')], pad=176 + 40, minimum=176 + 120)
    t.fit(city, [(cityp, 'width')], pad=40, minimum=120)
    t.fit(city, [(place, 'x')], pad=176 + 60, minimum=176 + 140)
    t.fit(place, [(placep, 'width')], pad=40, minimum=100)
    p.item('Paris · Élysée', 'loc', 'journal', {'city': 'PARIS', 'place': 'Palais de l’Élysée'})
    p.item('Bruxelles · Parlement', 'loc', 'journal', {'city': 'BRUXELLES', 'place': 'Parlement européen'})

    # -- Logo et heure --------------------------------------------------------
    t = p.template('bug', 'Logo et heure', 'bug', duration=60, on_air=30)
    t.field('show', 'Émission', 'LE JOURNAL')
    g = t.group('Logo', 1560, 64, 250, 72, tracks=fade(30, 0, 14))
    t.rect('Carré', 0, 0, 72, 72, hexc(BLUE), g, radius=4, grad=('#0b3ad6', 135))
    t.text('20H', '20H', 0, 0, 72, 72, 28, g, align='center', weight=800, spacing=-1)
    t.text('Émission', '{{show}}', 86, 4, 170, 30, 17, g, weight=700, spacing=2.5, upper=True, color=hexc('#ffffff', 0.92), shadow=True, shadowBlur=8.0)
    t.clock('Heure', 86, 32, 120, 38, 30, g, fmt='HH:mm', align='left', weight=600, shadow=True)
    p.item('Logo 20H', 'bug', 'ouverture')

    # -- Bandeau URGENT --------------------------------------------------------
    t = p.template('urgent', 'Bandeau urgent', 'banner', duration=90, on_air=60)
    t.field('label', 'Étiquette', 'URGENT')
    t.field('headline', 'Titre', 'Séisme de magnitude 6,2 au large de la Grèce, des secousses ressenties jusqu’en Italie', kind='multiline')
    t.field('time', 'Heure', '20:12')
    g = t.group('Bandeau', 0, 790, 1920, 180)
    t.rect('Rouge', 96, 0, 300, 58, hexc(RED), g, tracks=inout('cropRight', 0, 1, 60, 0, 16, 12, 8))
    t.text('Étiquette', '{{label}}', 120, 0, 270, 58, 28, g, weight=800, spacing=3, upper=True,
           tracks=inout('opacity', 1, 0, 60, 8, 10, 8, 8))
    t.rect('Heure fond', 396, 0, 120, 58, hexc(NAVY), g, tracks=inout('cropRight', 0, 1, 60, 6, 16, 12, 6))
    t.text('Heure', '{{time}}', 396, 0, 120, 58, 26, g, align='center', weight=600, tracks=inout('opacity', 1, 0, 60, 14, 10, 8, 6))
    t.rect('Plaque', 96, 58, 1728, 112, WHITE, g, tracks=inout('cropRight', 0, 1, 60, 6, 24, 12, 0))
    t.rect('Liseré', 96, 166, 1728, 4, hexc(RED), g, tracks=inout('cropRight', 0, 1, 60, 14, 24, 10, 0))
    t.text('Titre', '{{headline}}', 130, 58, 1660, 108, 44, g, color=hexc(NAVY), weight=800, wrap=True, lineHeight=1.08, spacing=-0.5,
           tracks=merge(inout('opacity', 1, 0, 60, 16, 12, 8), inout('y', 58, 76, 60, 16, 20, 8)))
    p.item('URGENT · Séisme', 'urgent', 'urgent')
    p.item('ÉDITION SPÉCIALE', 'urgent', 'urgent', {'label': 'ÉDITION SPÉCIALE', 'headline': 'Le Premier ministre s’exprimera à 21 heures depuis Matignon', 'time': '20:31'})

    # -- Titre plein écran ----------------------------------------------------------
    t = p.template('headline', 'Titre plein écran', 'full', duration=110, on_air=70)
    t.field('kicker', 'Rubrique', 'INTERNATIONAL')
    t.field('title', 'Titre', 'Sommet de Genève : un accord historique sur le climat', kind='multiline')
    t.field('sub', 'Sous-titre', '196 pays s’engagent à sortir du charbon d’ici 2040', kind='multiline')
    t.field('date', 'Date', 'Mardi 22 octobre')
    bg = t.group('Fond', 0, 0, 1920, 1080, tracks=fade(70, 0, 14, 14, 10))
    t.rect('Dégradé', 0, 0, 1920, 1080, hexc('#15306b'), bg, grad=('radial', NAVY))
    for i in range(7):
        t.rect(f'Trame {i + 1}', 0, 120 + i * 140, 1920, 1, hexc('#ffffff', 0.05), bg)
    t.rect('Forme', 1180, -100, 900, 1300, hexc(BLUE, 0.9), bg, shape='parallelogram', skew=340, grad=('#0a2a9e', 160),
           tracks=inout('x', 1180, 1700, 70, 0, 36, 14))
    t.rect('Forme 2', 1500, -100, 600, 1300, hexc('#ffffff', 0.06), bg, shape='parallelogram', skew=340, tracks=inout('x', 1500, 2000, 70, 6, 40, 14))
    t.text('20H', '20H', 1460, 760, 360, 220, 190, bg, align='right', weight=800, color=hexc('#ffffff', 0.12), spacing=-6, tracks=inout('opacity', 1, 0, 70, 14, 30, 10))
    tx = t.group('Texte', 150, 290, 1150, 520)
    kp = t.rect('Rubrique fond', 0, 0, 240, 46, hexc(BLUE), tx, tracks=inout('cropRight', 0, 1, 70, 10, 16, 10, 6))
    kick = t.text('Rubrique', '{{kicker}}', 18, 0, 600, 46, 22, tx, weight=800, spacing=3, upper=True, tracks=inout('opacity', 1, 0, 70, 16, 10, 8, 6))
    t.fit(kick, [(kp, 'width')], pad=36, minimum=120)
    t.text('Titre', '{{title}}', 0, 72, 1150, 280, 84, tx, weight=800, wrap=True, lineHeight=1.05, spacing=-2, valign='top', maxLines=3,
           tracks=merge(inout('opacity', 1, 0, 70, 16, 18, 10, 2), inout('y', 72, 112, 70, 16, 26, 10, 2)))
    t.rect('Filet', 0, 384, 120, 6, hexc(BLUE), tx, tracks=inout('cropRight', 0, 1, 70, 26, 18, 10))
    t.text('Sous-titre', '{{sub}}', 0, 410, 1050, 110, 36, tx, weight=500, color=hexc(MIST), wrap=True, valign='top', lineHeight=1.25,
           tracks=merge(inout('opacity', 1, 0, 70, 26, 18, 10), inout('y', 410, 440, 70, 26, 24, 10)))
    t.text('Date', '{{date}}', 150, 900, 600, 40, 24, None, weight=600, color=hexc(SKY), spacing=1.5, upper=True, tracks=inout('opacity', 1, 0, 70, 30, 14, 10))
    p.item('Titre · Climat', 'headline', 'journal')
    p.item('Titre · Retraites', 'headline', 'journal', {'kicker': 'POLITIQUE', 'title': 'Retraites : le gouvernement recule sur l’âge légal', 'sub': 'Le texte sera présenté en Conseil des ministres le 4 novembre'})

    # -- Sommaire ---------------------------------------------------------------------
    t = p.template('menu', 'Sommaire', 'full', duration=130, on_air=90)
    t.field('title', 'Titre', 'AU SOMMAIRE')
    heads = [('Climat', 'Un accord historique signé à Genève'), ('Économie', 'L’inflation au plus bas depuis trois ans'),
             ('Société', 'Pénurie de médecins : les villes moyennes s’organisent'), ('Sport', 'Le XV de France dévoile sa liste pour le Tournoi')]
    for i, (k_, h_) in enumerate(heads, 1):
        t.field(f'k{i}', f'Rubrique {i}', k_)
        t.field(f'h{i}', f'Titre {i}', h_)
    panel = t.group('Panneau', 0, 0, 1060, 1080, tracks=inout('x', 0, -1060, 90, 0, 26, 16, 10))
    t.rect('Verre', 0, 0, 1060, 1080, hexc(NAVY, 0.9), panel, grad=('#0b1d42e6', 180))
    t.rect('Bord', 1054, 0, 6, 1080, hexc(BLUE), panel)
    t.text('Titre', '{{title}}', 150, 150, 800, 80, 30, panel, weight=800, spacing=5, color=hexc(SKY), tracks=inout('opacity', 1, 0, 90, 12, 14, 8))
    for i in range(1, 5):
        y = 260 + (i - 1) * 170
        d = 14 + i * 6
        row = t.group(f'Titre {i}', 150, y, 860, 150, panel, tracks=merge(inout('opacity', 1, 0, 90, d, 14, 8, (4 - i) * 2), inout('x', 150, 110, 90, d, 22, 8, (4 - i) * 2)))
        t.text('Numéro', f'0{i}', 0, 0, 100, 80, 56, row, weight=800, color=hexc(BLUE), spacing=-2, valign='top')
        t.text('Rubrique', '{{k%d}}' % i, 110, 2, 700, 34, 20, row, weight=700, color=hexc(SKY), upper=True, spacing=2.5, valign='top')
        t.text('Titre', '{{h%d}}' % i, 110, 36, 740, 96, 36, row, weight=700, wrap=True, lineHeight=1.12, valign='top', maxLines=2)
        if i < 4:
            t.rect('Séparateur', 110, 146, 740, 1, hexc('#ffffff', 0.14), row)
    p.item('Sommaire', 'menu', 'ouverture')

    # -- Ticker -----------------------------------------------------------------------------
    t = p.template('ticker', 'Ticker', 'ticker', duration=60, on_air=30)
    t.field('items', 'Brèves', 'Bourse : le CAC 40 termine en hausse de 0,8 %\nMétéo : vigilance orange orages sur 12 départements\nFootball : Lens se qualifie pour les huitièmes de finale\nCulture : le Louvre prolonge son ouverture en nocturne'.replace(' :', N + ':').replace(' %', N + '%'), kind='multiline')
    t.field('label', 'Étiquette', 'EN BREF')
    g = t.group('Ticker', 0, 990, 1920, 56, tracks=inout('y', 990, 1080, 30, 0, 18, 12))
    t.rect('Barre', 0, 0, 1920, 56, hexc(NAVY, 0.95), g)
    t.rect('Étiquette fond', 0, 0, 260, 56, hexc(BLUE), g, grad=('#123fcf', 90))
    t.text('Étiquette', '{{label}}', 96, 0, 164, 56, 22, g, weight=800, spacing=2.5, upper=True)
    t.ticker('Défilant', '{{items}}', 290, 0, 1630, 56, 27, g, weight=500, speed=120, sepSymbol='■', sepColor=SKY, spacing=28.0, fade=True, fadeW=60.0)
    p.item('Brèves du soir', 'ticker', 'journal')
    return p


def split_bar(t, hkey, akey, rect_id, width):
    """Width of the home part of a two-sided bar: home / (home + away) × width (logic)."""
    k = len(t.t.get('logic', {}).get('nodes', []))
    n = lambda x: f's{k}{x}'
    t.logic([{'id': n('h'), 'type': 'field', 'key': hkey, 'x': 40, 'y': 40}, {'id': n('a'), 'type': 'field', 'key': akey, 'x': 40, 'y': 140},
             {'id': n('s'), 'type': 'math', 'op': 'add', 'x': 260, 'y': 80}, {'id': n('e'), 'type': 'constant', 'value': 0.0001, 'x': 260, 'y': 200},
             {'id': n('m'), 'type': 'math', 'op': 'max', 'x': 480, 'y': 100}, {'id': n('d'), 'type': 'math', 'op': 'div', 'x': 700, 'y': 60},
             {'id': n('w'), 'type': 'constant', 'value': width, 'x': 700, 'y': 200}, {'id': n('x'), 'type': 'math', 'op': 'mul', 'x': 920, 'y': 100},
             {'id': n('t'), 'type': 'target', 'layer': rect_id, 'prop': 'width', 'mode': 'set', 'x': 1140, 'y': 100}],
            [{'from': n('h'), 'to': n('s'), 'port': 'a'}, {'from': n('a'), 'to': n('s'), 'port': 'b'}, {'from': n('s'), 'to': n('m'), 'port': 'a'},
             {'from': n('e'), 'to': n('m'), 'port': 'b'}, {'from': n('h'), 'to': n('d'), 'port': 'a'}, {'from': n('m'), 'to': n('d'), 'port': 'b'},
             {'from': n('d'), 'to': n('x'), 'port': 'a'}, {'from': n('w'), 'to': n('x'), 'port': 'b'}, {'from': n('x'), 'to': n('t'), 'port': 'in'}])


# ======================================================================
# 02 · Chaîne d'info en continu
# ======================================================================
def ex02():
    INK, GRAPH, YEL, RED, GREY = '#0d0e11', '#16181d', '#ffd60a', '#ff2a2a', '#a7adb8'
    p = Package(
        'ex-02-info-continu', 'Info en continu 24/7', 'Info',
        ['Bandeau permanent', 'Alerte info', 'Direct', 'Rubriques'],
        "L'antenne d'une chaîne d'info en continu : bandeau permanent (rubrique, titre, heure et fil info), ALERTE INFO qui clignote, logo, direct avec lieu, titres de rubrique et synthés. Graphite et jaune signal, Barlow Condensed. Bandeau, alerte, logo et direct ont leur version 9:16 pour les réseaux sociaux.",
        en=('24/7 news channel', 'A rolling news channel on air: permanent strap (section, headline, clock and news wire), flashing NEWS ALERT, logo, live tag, section titles and lower thirds. Strap, alert, logo and live tag also exist in 9:16 for social networks.'),
        cover='bandeau')
    p.vertical()
    p.zone('bandeau', 'Bandeau', 30, '#ffd60a', (0, 0.77, 1, 0.23), vertical=(0, 0.76, 1, 0.16))
    p.zone('alert', 'Alerte', 35, '#ff3b5c', (0, 0.60, 1, 0.17), vertical=(0, 0.60, 1, 0.15))
    p.zone('over', 'Au-dessus du bandeau', 20, '#7c5cff', (0.05, 0.60, 0.62, 0.17), vertical=(0.05, 0.62, 0.9, 0.13))
    p.zone('live', 'Direct', 25, '#22c55e', (0.05, 0.05, 0.4, 0.07), vertical=(0.05, 0.08, 0.7, 0.05))
    p.zone('logo', 'Logo', 60, '#38bdf8', (0.86, 0.05, 0.09, 0.12), vertical=(0.80, 0.07, 0.15, 0.08))
    p.playlist('antenne', 'Antenne')
    p.playlist('alertes', 'Alertes')
    p.playlist('emission', 'Émission')

    # -- Bandeau permanent ------------------------------------------------------------
    t = p.template('bandeau', 'Bandeau permanent', 'bandeau', duration=70, on_air=40)
    t.field('section', 'Rubrique', 'POLITIQUE')
    t.field('headline', 'Titre', 'Budget 2027 : le gouvernement engage sa responsabilité à l’Assemblée')
    t.field('wire', 'Fil info', 'Grève SNCF : trafic perturbé jeudi sur les lignes TER\nLe prix de l’électricité baissera de 4 % au 1er février\nLigue des champions : Brest affronte le Real Madrid mardi\nIntempéries : 4 000 foyers privés d’électricité en Bretagne', kind='multiline')
    g = t.group('Bandeau', 0, 856, 1920, 188)
    t.override(g, y=1478, width=1080)
    tab = t.rect('Rubrique fond', 96, 0, 200, 38, hexc(YEL), g, tracks=inout('cropRight', 0, 1, 40, 8, 14, 10, 8))
    t.override(tab, x=60)
    sec = t.text('Rubrique', '{{section}}', 112, 0, 600, 38, 25, g, color=hexc(INK), weight=700, family=COND, spacing=1.5, upper=True,
                 tracks=inout('opacity', 1, 0, 40, 14, 10, 6, 8))
    t.override(sec, x=76)
    t.fit(sec, [(tab, 'width')], pad=32, minimum=120)
    bar = t.rect('Titre fond', 96, 38, 1560, 96, hexc(GRAPH, 0.96), g, grad=('#22252cf5', 180), tracks=inout('cropRight', 0, 1, 40, 0, 20, 12, 4))
    t.override(bar, x=60, width=808, height=124)
    yb = t.rect('Filet', 96, 38, 8, 96, hexc(YEL), g)
    t.override(yb, x=60, height=124)
    hl = t.text('Titre', '{{headline}}', 130, 38, 1500, 96, 52, g, weight=600, family=COND, spacing=0.2,
                tracks=merge(inout('opacity', 1, 0, 40, 10, 14, 8), inout('y', 38, 58, 40, 10, 20, 8)))
    t.override(hl, x=92, width=752, height=124, fontSize=44)
    t.layer_ref(hl).update(wrap=True, lineHeight=1.0, maxLines=2)
    ck = t.rect('Heure fond', 1664, 38, 160, 96, hexc(YEL), g, tracks=inout('cropBottom', 0, 1, 40, 4, 16, 10, 4))
    t.override(ck, x=876, width=144, height=124)
    cl = t.clock('Heure', 1664, 38, 160, 96, 50, g, fmt='HH:mm', color=hexc(INK), family=COND, weight=800, tracks=inout('opacity', 1, 0, 40, 12, 10, 6, 4))
    t.override(cl, x=876, width=144, height=124)
    tk = t.group('Fil info', 96, 140, 1728, 46, g, clip=True, tracks=inout('cropRight', 0, 1, 40, 14, 22, 10))
    t.override(tk, x=60, y=170, width=960)
    wb = t.rect('Fil fond', 0, 0, 1728, 46, WHITE, tk)
    t.override(wb, width=960)
    lb = t.rect('Fil étiquette', 0, 0, 160, 46, hexc(INK), tk)
    t.text('FIL INFO', 'FIL INFO', 0, 0, 160, 46, 24, tk, align='center', color=hexc(YEL), weight=800, family=COND, spacing=1.5)
    tw = t.ticker('Défilant', '{{wire}}', 176, 0, 1552, 46, 25, tk, color=hexc(INK), weight=600, speed=110, sepSymbol='■', sepColor=YEL, spacing=26.0)
    t.override(tw, width=784)
    p.item('Bandeau · Budget', 'bandeau', 'antenne')
    p.item('Bandeau · Météo', 'bandeau', 'antenne', {'section': 'MÉTÉO', 'headline': 'Tempête Amélie : 14 départements en vigilance orange vent violent'})

    # -- Alerte info --------------------------------------------------------------------------
    t = p.template('alerte', 'Alerte info', 'alert', duration=110, on_air=80)
    t.field('headline', 'Titre', 'Explosion dans une usine chimique près de Rouen, un périmètre de sécurité mis en place')
    t.field('time', 'Heure', '14:32')
    g = t.group('Alerte', 0, 676, 1920, 150)
    t.override(g, y=1170, width=1080)
    blink = keys((0, hexc(RED)), (24, hexc(RED), 'hold'), (30, hexc('#b80f0f'), 'hold'), (36, hexc(RED), 'hold'), (42, hexc('#b80f0f'), 'hold'), (48, hexc(RED), 'hold'))
    rb = t.rect('Rouge', 0, 0, 1824, 64, hexc(RED), g, tracks=merge(inout('cropRight', 0, 1, 80, 0, 18, 12, 6), {'fill': blink}))
    t.override(rb, width=1020)
    t.rect('Éclair', 0, 0, 40, 64, hexc('#ffffff', 0.85), g, tracks={'x': keys((0, -60, EXPO_OUT), (22, 1900, 'linear')), 'opacity': keys((0, 1, 'linear'), (20, 1, 'linear'), (24, 0))})
    a1 = t.text('ALERTE INFO', 'ALERTE INFO', 140, 0, 600, 64, 44, g, weight=800, family=COND, spacing=3, tracks=inout('opacity', 1, 0, 80, 8, 8, 6, 6))
    t.override(a1, x=104)
    dot = t.ellipse('Point', 96, 20, 24, 24, WHITE, g, tracks=pulse('opacity', 0.2, 1, 40, 5000))
    t.override(dot, x=60)
    tm = t.text('Heure', '{{time}}', 1600, 0, 200, 64, 34, g, align='right', weight=700, family=COND, tracks=inout('opacity', 1, 0, 80, 12, 8, 6, 6))
    t.override(tm, x=800)
    hb = t.rect('Titre fond', 0, 64, 1824, 86, hexc(INK, 0.97), g, tracks=inout('cropRight', 0, 1, 80, 6, 22, 12, 0))
    t.override(hb, width=1020, height=170)
    ht = t.text('Titre', '{{headline}}', 96, 64, 1700, 86, 46, g, weight=600, family=COND,
                tracks=merge(inout('opacity', 1, 0, 80, 18, 12, 8), inout('x', 96, 130, 80, 18, 20, 8)))
    t.override(ht, x=60, width=940, height=170, fontSize=48, tracks=inout('opacity', 1, 0, 80, 18, 12, 8))
    t.layer_ref(ht).update(wrap=True, lineHeight=1.02, maxLines=3)
    p.item('ALERTE · Rouen', 'alerte', 'alertes')
    p.item('ALERTE · Démission', 'alerte', 'alertes', {'headline': 'Le président du Sénat annonce sa démission', 'time': '17:05'})

    # -- Direct ------------------------------------------------------------------------------------
    t = p.template('direct', 'Direct et lieu', 'live', duration=60, on_air=30)
    t.field('place', 'Lieu', 'MARSEILLE · Vieux-Port')
    g = t.group('Direct', 110, 64, 700, 52, tracks=inout('cropRight', 0, 1, 30, 0, 16, 10))
    t.override(g, x=60, y=160)
    t.rect('Fond direct', 0, 0, 170, 52, hexc(RED), g, radius=3)
    t.ellipse('Point', 18, 19, 14, 14, WHITE, g, tracks=pulse('opacity', 0.25, 1, 50, 3000))
    t.text('DIRECT', 'DIRECT', 42, 0, 128, 52, 30, g, weight=800, family=COND, spacing=2)
    t.text('Lieu', '{{place}}', 190, 0, 520, 52, 30, g, weight=700, family=COND, upper=True, spacing=1, shadow=True, shadowBlur=10.0, shadowY=2.0)
    p.item('Direct Marseille', 'direct', 'antenne')

    # -- Logo -------------------------------------------------------------------------------------
    t = p.template('logo', 'Logo', 'logo', duration=60, on_air=30)
    g = t.group('Logo', 1704, 64, 120, 120, tracks=merge(fade(30, 0, 12), inout('scaleX', 1, 0.8, 30, 0, 16, 10), inout('scaleY', 1, 0.8, 30, 0, 16, 10)))
    t.override(g, x=900, y=134, width=120, height=120)
    t.rect('Carré', 0, 0, 120, 120, hexc(YEL), g, radius=6, grad=('#ffc300', 160), shadow=(hexc('#000000', 0.35), 0, 6, 18))
    t.text('INFO', 'INFO', 0, 10, 120, 32, 30, g, align='center', color=hexc(INK), weight=800, family=COND, spacing=3)
    t.text('24', '24', 0, 30, 120, 90, 92, g, align='center', color=hexc(INK), weight=800, family=COND, spacing=-3)
    p.item('Logo', 'logo', 'antenne')

    # -- Titre de rubrique ------------------------------------------------------------------
    t = p.template('rubrique', 'Titre de rubrique', 'over', duration=80, on_air=50)
    t.field('title', 'Rubrique', 'ÉCONOMIE')
    t.field('sub', 'Sous-titre', 'Le point sur les marchés avec Julie Morel')
    g = t.group('Rubrique', 96, 680, 1100, 140)
    t.override(g, x=60, y=1200, width=960)
    bk = t.rect('Voile', 0, 0, 1100, 140, hexc(INK, 0.75), g, grad=('#0d0e1100', 90), tracks=inout('cropRight', 0, 1, 50, 0, 22, 12, 4))
    t.override(bk, width=960)
    tt = t.text('Titre', '{{title}}', 28, 0, 1000, 92, 92, g, weight=800, family=COND, spacing=1, upper=True,
                tracks=merge(inout('opacity', 1, 0, 50, 6, 12, 8), inout('x', 28, -20, 50, 6, 22, 8)))
    ul = t.rect('Soulignement', 28, 92, 300, 8, hexc(YEL), g, tracks=inout('cropRight', 0, 1, 50, 14, 18, 8))
    t.fit(tt, [(ul, 'width')], pad=0, minimum=120)
    t.text('Sous-titre', '{{sub}}', 28, 102, 1000, 38, 28, g, weight=500, color=hexc('#e8eaee'), tracks=inout('opacity', 1, 0, 50, 18, 12, 8))
    for k_, s_ in (('ÉCONOMIE', 'Le point sur les marchés avec Julie Morel'), ('INTERNATIONAL', 'Notre correspondant à Washington'), ('SPORT', 'Toute l’actualité du week-end')):
        p.item(k_.capitalize(), 'rubrique', 'emission', {'title': k_, 'sub': s_})

    # -- Synthé invité ------------------------------------------------------------------------
    t = p.template('invite', 'Synthé invité', 'over', duration=70, on_air=40)
    t.field('name', 'Nom', 'Antoine Leroy')
    t.field('role', 'Fonction', 'Député, rapporteur du budget')
    g = t.group('Synthé', 96, 700, 1100, 116)
    t.override(g, x=60, y=1206, width=960)
    pl = t.rect('Plaque', 0, 0, 600, 116, hexc(GRAPH, 0.95), g, tracks=inout('cropRight', 0, 1, 40, 0, 18, 10, 4))
    t.rect('Filet', 0, 0, 8, 116, hexc(YEL), g, tracks=inout('cropBottom', 0, 1, 40, 0, 12, 8, 6))
    nm = t.text('Nom', '{{name}}', 32, 6, 1040, 64, 50, g, weight=700, family=COND, spacing=0.5,
                tracks=merge(inout('opacity', 1, 0, 40, 8, 12, 8), inout('x', 32, 10, 40, 8, 18, 8)))
    t.override(nm, width=900)
    rl = t.text('Fonction', '{{role}}', 32, 68, 1040, 38, 26, g, weight=500, color=hexc(GREY), tracks=inout('opacity', 1, 0, 40, 14, 12, 8))
    t.override(rl, width=900)
    t.fit(nm, [(pl, 'width')], pad=72, minimum=360)
    p.item('Antoine Leroy', 'invite', 'emission')
    p.item('Sarah Kemal', 'invite', 'emission', {'name': 'Sarah Kemal', 'role': 'Correspondante à Washington'})
    return p


# ======================================================================
# 03 · Football
# ======================================================================
def ex03():
    NIGHT, PANEL, LIME, WHITE_, MIST = '#090d1a', '#111933', '#c6ff2e', '#ffffff', '#9fb0d0'
    p = Package(
        'ex-03-football', 'Football en direct', 'Sport',
        ['Score et chrono', 'Compositions', 'But', 'Cartons', 'Remplacements', 'Statistiques'],
        "Un match de championnat de bout en bout : score avec chronomètre que l'opérateur lance et arrête, compositions sur le terrain (4-3-3), alerte BUT, cartons jaune et rouge, remplacements, statistiques à barres partagées et score final. Couleurs des équipes en variables, nuit et vert acide, Bebas Neue et Barlow Condensed.",
        en=('Live football', 'A league match from kick-off to full time: score bug with a match clock the operator starts and stops, line-ups on the pitch (4-3-3), GOAL alert, yellow and red cards, substitutions, split-bar statistics and final score. Team colours are variables.'),
        cover='lineup')
    p.zone('score', 'Score', 30, '#c6ff2e', (0.04, 0.04, 0.42, 0.075))
    p.zone('lower', 'Bas d’écran', 20, '#7c5cff', (0.04, 0.66, 0.70, 0.29))
    p.zone('event', 'But', 40, '#ff3b5c', (0, 0.60, 1, 0.35))
    p.zone('full', 'Plein écran', 50, '#94a3b8')
    p.playlist('avant', 'Avant-match')
    p.playlist('match', 'Match')
    p.playlist('fin', 'Après-match')
    TEAMS = {'home': ('Paris FC', 'PFC', '#1d4ed8'), 'away': ('Olympique Lyon', 'OLY', '#e11d48')}

    # -- Score et chrono -----------------------------------------------------------------------
    t = p.template('score', 'Score et chrono', 'score', duration=60, on_air=30)
    t.field('home', 'Domicile (3 lettres)', 'PFC')
    t.field('away', 'Extérieur (3 lettres)', 'OLY')
    t.field('hs', 'Score domicile', '1', kind='number', min=0, max=20)
    t.field('as', 'Score extérieur', '0', kind='number', min=0, max=20)
    t.field('hc', 'Couleur domicile', TEAMS['home'][2], kind='color')
    t.field('ac', 'Couleur extérieur', TEAMS['away'][2], kind='color')
    g = t.group('Score', 96, 54, 706, 60, clip=True, tracks=inout('cropRight', 0, 1, 30, 0, 18, 10))
    t.rect('Fond', 0, 0, 706, 60, hexc(NIGHT, 0.94), g, radius=4)
    t.rect('Marque', 0, 0, 60, 60, hexc(LIME), g, corners=(4, 0, 0, 4))
    t.text('LE', 'LE', 0, 2, 60, 60, 36, g, align='center', color=hexc(NIGHT), family=DISPLAY, weight=400)
    t.rect('Couleur dom', 60, 0, 8, 60, hexc(TEAMS['home'][2]), g, bindings={'fill': 'hc'})
    t.text('Domicile', '{{home}}', 68, 0, 172, 60, 36, g, align='center', family=COND, weight=800, spacing=2, upper=True)
    t.rect('Score fond', 240, 0, 136, 60, WHITE, g)
    t.text('Score', '{{hs}}–{{as}}', 240, 3, 136, 60, 50, g, align='center', color=hexc(NIGHT), family=DISPLAY, weight=400, spacing=2)
    t.text('Extérieur', '{{away}}', 376, 0, 172, 60, 36, g, align='center', family=COND, weight=800, spacing=2, upper=True)
    t.rect('Couleur ext', 548, 0, 8, 60, hexc(TEAMS['away'][2]), g, bindings={'fill': 'ac'})
    t.rect('Chrono fond', 556, 0, 150, 60, hexc('#05070f'), g, corners=(0, 4, 4, 0))
    t.clock('Chrono', 556, 0, 150, 60, 38, g, mode='stopwatch', fmt='mm:ss', color=hexc(LIME), family=COND, weight=700, startMode='manual', hoursAuto=True)
    p.item('Score PFC – OLY', 'score', 'match')

    # -- Composition ------------------------------------------------------------------------------
    t = p.template('lineup', 'Composition', 'full', duration=150, on_air=110)
    t.field('team', 'Équipe', 'Paris FC')
    t.field('color', 'Couleur', TEAMS['home'][2], kind='color')
    t.field('formation', 'Système', '4-3-3')
    t.field('coach', 'Entraîneur', 'Stéphane Gilli')
    t.field('subs', 'Remplaçants', '16 Trinité · 4 Kouassi · 8 Kebbal · 11 Gory · 27 Ikoné', kind='multiline')
    XI = [('1', 'Trinité', .08, .5), ('2', 'Mbow', .28, .14), ('5', 'Kolodziejczak', .26, .38), ('15', 'Sangui', .26, .62), ('3', 'Hamel', .28, .86),
          ('6', 'Lopez', .5, .26), ('10', 'Kebbal', .47, .5), ('8', 'Marchetti', .5, .74), ('7', 'Simon', .74, .2), ('9', 'Krasso', .78, .5), ('17', 'Mandouki', .74, .8)]
    for i, (n_, nm_, _, _) in enumerate(XI, 1):
        t.field(f'n{i}', f'Numéro {i}', n_)
        t.field(f'p{i}', f'Joueur {i}', nm_)
    bg = t.group('Fond', 0, 0, 1920, 1080, tracks=fade(110, 0, 12, 12, 12))
    t.rect('Nuit', 0, 0, 1920, 1080, hexc('#142046'), bg, grad=('radial', NIGHT))
    t.rect('Diagonale', -200, 0, 900, 1080, hexc('#ffffff', 0.03), bg, shape='parallelogram', skew=260)
    col = t.group('Équipe', 120, 200, 600, 680)
    t.rect('Kicker fond', 0, 0, 240, 40, hexc(LIME), col, tracks=inout('cropRight', 0, 1, 110, 6, 14, 10, 10))
    t.text('Kicker', 'COMPOSITION', 14, 0, 230, 40, 26, col, color=hexc(NIGHT), family=COND, weight=800, spacing=2, tracks=inout('opacity', 1, 0, 110, 12, 8, 6, 10))
    t.text('Équipe', '{{team}}', 0, 60, 600, 180, 96, col, family=DISPLAY, weight=400, wrap=True, lineHeight=0.92, valign='top', spacing=1,
           tracks=merge(inout('opacity', 1, 0, 110, 10, 14, 8, 8), inout('y', 60, 100, 110, 10, 22, 8, 8)))
    t.rect('Couleur', 0, 260, 80, 8, hexc(TEAMS['home'][2]), col, bindings={'fill': 'color'}, tracks=inout('cropRight', 0, 1, 110, 18, 14, 8, 6))
    t.text('Système label', 'SYSTÈME', 0, 300, 280, 30, 20, col, color=hexc(MIST), family=COND, weight=700, spacing=2, tracks=inout('opacity', 1, 0, 110, 20, 10, 6, 6))
    t.text('Système', '{{formation}}', 0, 330, 280, 70, 64, col, family=DISPLAY, weight=400, color=hexc(LIME), tracks=inout('opacity', 1, 0, 110, 22, 10, 6, 6))
    t.text('Coach label', 'ENTRAÎNEUR', 300, 300, 300, 30, 20, col, color=hexc(MIST), family=COND, weight=700, spacing=2, tracks=inout('opacity', 1, 0, 110, 22, 10, 6, 6))
    t.text('Coach', '{{coach}}', 300, 330, 300, 70, 36, col, family=COND, weight=700, tracks=inout('opacity', 1, 0, 110, 24, 10, 6, 6))
    t.rect('Séparateur', 0, 430, 600, 1, hexc('#ffffff', 0.2), col, tracks=inout('cropRight', 0, 1, 110, 26, 16, 6, 4))
    t.text('Remplaçants label', 'REMPLAÇANTS', 0, 450, 600, 30, 20, col, color=hexc(MIST), family=COND, weight=700, spacing=2, tracks=inout('opacity', 1, 0, 110, 28, 10, 6, 4))
    t.text('Remplaçants', '{{subs}}', 0, 484, 600, 120, 28, col, family=COND, weight=500, wrap=True, valign='top', lineHeight=1.3, tracks=inout('opacity', 1, 0, 110, 30, 10, 6, 4))
    PX, PY, PW, PH = 780, 150, 1040, 760
    pitch = t.group('Terrain', PX, PY, PW, PH, tracks=merge(inout('opacity', 1, 0, 110, 4, 16, 10, 2), inout('y', PY, PY + 60, 110, 4, 26, 10, 2)))
    t.rect('Pelouse', 0, 0, PW, PH, hexc('#0f3a26'), pitch, radius=10, grad=('#0a2a1c', 180))
    for i in range(0, 10, 2):
        t.rect(f'Bande {i // 2 + 1}', i * PW / 10, 0, PW / 10, PH, hexc('#ffffff', 0.035), pitch)
    L = hexc('#ffffff', 0.5)
    t.rect('Ligne', 24, 24, PW - 48, PH - 48, CLEAR, pitch, stroke=(L, 3))
    t.rect('Médiane', PW / 2 - 1.5, 24, 3, PH - 48, L, pitch)
    t.ellipse('Rond central', PW / 2 - 90, PH / 2 - 90, 180, 180, CLEAR, pitch, stroke=(L, 3))
    for side in (0, 1):
        x0 = 24 if side == 0 else PW - 24 - 160
        t.rect('Surface', x0, PH / 2 - 200, 160, 400, CLEAR, pitch, stroke=(L, 3))
        x1 = 24 if side == 0 else PW - 24 - 56
        t.rect('Petite surface', x1, PH / 2 - 90, 56, 180, CLEAR, pitch, stroke=(L, 3))
    for i, (_, _, fx, fy) in enumerate(XI, 1):
        cx, cy = 24 + fx * (PW - 48), 24 + fy * (PH - 48)
        d = 20 + i * 3
        pl = t.group(f'Joueur {i}', cx - 110, cy - 40, 220, 116, pitch, tracks=merge(inout('opacity', 1, 0, 110, d, 10, 6, (11 - i)), inout('y', cy - 40, cy - 10, 110, d, 18, 6, 11 - i)))
        t.ellipse('Maillot', 78, 0, 64, 64, hexc(TEAMS['home'][2]), pl, stroke=(WHITE, 3), bindings={'fill': 'color'})
        t.text('Numéro', '{{n%d}}' % i, 78, 3, 64, 64, 34, pl, align='center', family=DISPLAY, weight=400)
        t.text('Nom', '{{p%d}}' % i, 0, 70, 220, 36, 26, pl, align='center', family=COND, weight=700, upper=True, spacing=0.5, shadow=True, shadowBlur=8.0, shadowY=2.0)
    p.item('Composition Paris FC', 'lineup', 'avant')

    # -- But ! ------------------------------------------------------------------------------------------
    t = p.template('goal', 'But', 'event', duration=130, on_air=100)
    t.field('scorer', 'Buteur', 'Ilan Kebbal')
    t.field('minute', 'Minute', '23')
    t.field('team', 'Équipe (3 lettres)', 'PFC')
    t.field('score', 'Nouveau score', 'PFC 1–0 OLY')
    t.field('color', 'Couleur', TEAMS['home'][2], kind='color')
    g = t.group('But', 0, 690, 1920, 300)
    pop = merge(inout('scaleX', 1, 0.6, 100, 4, 16, 8, 6, 'back'), inout('scaleY', 1, 0.6, 100, 4, 16, 8, 6, 'back'))
    t.rect('Vert', 96, 0, 760, 220, hexc(LIME), g, shape='parallelogram', skew=50, tracks=inout('x', 96, -900, 100, 0, 20, 12, 8))
    t.text('BUT !', 'BUT !', 150, 12, 680, 220, 220, g, color=hexc(NIGHT), family=DISPLAY, weight=400, spacing=4, tracks=merge(pop, inout('opacity', 1, 0, 100, 4, 6, 6, 6)))
    t.rect('Équipe', 790, 30, 1034, 160, hexc(TEAMS['home'][2]), g, shape='parallelogram', skew=40, bindings={'fill': 'color'}, tracks=inout('x', 790, 1920, 100, 6, 22, 12, 4))
    t.rect('Plaque', 820, 30, 1004, 160, hexc(NIGHT, 0.88), g, shape='parallelogram', skew=40, grad=('#090d1a55', 90), tracks=inout('x', 820, 1920, 100, 8, 22, 12, 4))
    t.text('Buteur', '{{scorer}}', 880, 40, 900, 90, 74, g, family=COND, weight=800, upper=True, spacing=1,
           tracks=merge(inout('opacity', 1, 0, 100, 18, 12, 6), inout('x', 880, 940, 100, 18, 20, 6)))
    t.text('Minute', '{{minute}}e minute  ·  {{score}}', 880, 124, 900, 50, 34, g, family=COND, weight=600, color=hexc(LIME), spacing=1,
           tracks=inout('opacity', 1, 0, 100, 26, 12, 6))
    t.rect('Filet', 96, 236, 1728, 6, hexc(LIME), g, tracks=inout('cropRight', 0, 1, 100, 12, 30, 10))
    p.item('But Kebbal', 'goal', 'match')

    # -- Carton ------------------------------------------------------------------------------------------
    t = p.template('card', 'Carton', 'lower', duration=80, on_air=50)
    t.field('kind', 'Carton', '1', options=[['1', 'Jaune'], ['2', 'Rouge']])
    t.field('player', 'Joueur', 'Corentin Tolisso')
    t.field('info', 'Équipe · minute', 'OLY · 54e')
    g = t.group('Carton', 96, 800, 900, 150)
    y_ = t.rect('Jaune', 10, 18, 76, 108, hexc('#ffd400'), g, radius=6, rotation=-8, shadow=(hexc('#000000', 0.4), 0, 6, 14),
                tracks=merge(inout('y', 18, -40, 50, 0, 18, 10, 0, 'back'), inout('opacity', 1, 0, 50, 0, 8, 8)))
    r_ = t.rect('Rouge', 10, 18, 76, 108, hexc('#ef233c'), g, radius=6, rotation=-8, shadow=(hexc('#000000', 0.4), 0, 6, 14),
                tracks=merge(inout('y', 18, -40, 50, 0, 18, 10, 0, 'back'), inout('opacity', 1, 0, 50, 0, 8, 8)))
    t.choose('kind', {1: [y_], 2: [r_]})
    pl = t.rect('Plaque', 120, 20, 600, 72, WHITE, g, tracks=inout('cropRight', 0, 1, 50, 6, 18, 10, 4))
    nm = t.text('Joueur', '{{player}}', 146, 20, 760, 72, 48, g, color=hexc(NIGHT), family=COND, weight=800, upper=True, spacing=0.5, tracks=inout('opacity', 1, 0, 50, 12, 10, 6, 4))
    t.fit(nm, [(pl, 'width')], pad=56, minimum=300)
    ip = t.rect('Info fond', 120, 92, 260, 44, hexc(NIGHT, 0.94), g, tracks=inout('cropRight', 0, 1, 50, 10, 18, 10))
    inf = t.text('Info', '{{info}}', 146, 92, 500, 44, 28, g, color=hexc(LIME), family=COND, weight=700, spacing=1.5, upper=True, tracks=inout('opacity', 1, 0, 50, 16, 10, 6))
    t.fit(inf, [(ip, 'width')], pad=52, minimum=160)
    p.item('Carton jaune Tolisso', 'card', 'match')
    p.item('Carton rouge Mbow', 'card', 'match', {'kind': '2', 'player': 'Moustapha Mbow', 'info': 'PFC · 78e'})

    # -- Remplacement --------------------------------------------------------------------------------
    t = p.template('sub', 'Remplacement', 'lower', duration=90, on_air=60)
    t.field('team', 'Équipe', 'PARIS FC')
    t.field('inn', 'Entrant (n° et nom)', '27  Jean-Philippe Krasso')
    t.field('out', 'Sortant (n° et nom)', '9  Maxime Lopez')
    t.field('color', 'Couleur', TEAMS['home'][2], kind='color')
    g = t.group('Remplacement', 96, 770, 820, 200, clip=True, tracks=inout('cropRight', 0, 1, 60, 0, 20, 12))
    t.rect('Entête', 0, 0, 820, 50, hexc(NIGHT, 0.95), g)
    t.rect('Couleur', 0, 0, 8, 50, hexc(TEAMS['home'][2]), g, bindings={'fill': 'color'})
    t.text('Entête', 'REMPLACEMENT  ·  {{team}}', 28, 0, 780, 50, 28, g, family=COND, weight=700, color=hexc(LIME), spacing=2, upper=True)
    t.rect('Entrant fond', 0, 50, 820, 76, WHITE, g)
    t.rect('Entrant marque', 0, 50, 76, 76, hexc('#16a34a'), g)
    t.tri('Flèche haut', 20, 70, 36, 34, WHITE, g, dir='up')
    t.text('Entrant', '{{inn}}', 100, 50, 700, 76, 44, g, color=hexc(NIGHT), family=COND, weight=800, upper=True, tracks=inout('opacity', 1, 0, 60, 14, 10, 6))
    t.rect('Sortant fond', 0, 126, 820, 66, hexc('#1a2240', 0.95), g)
    t.rect('Sortant marque', 0, 126, 76, 66, hexc('#dc2626'), g)
    t.tri('Flèche bas', 22, 144, 32, 30, WHITE, g, dir='down')
    t.text('Sortant', '{{out}}', 100, 126, 700, 66, 36, g, color=hexc('#cbd5e1'), family=COND, weight=600, upper=True, tracks=inout('opacity', 1, 0, 60, 20, 10, 6))
    p.item('Remplacement Paris FC', 'sub', 'match')

    # -- Statistiques -----------------------------------------------------------------------------------
    t = p.template('stats', 'Statistiques', 'full', duration=150, on_air=110)
    t.field('title', 'Titre', 'STATISTIQUES · MI-TEMPS')
    t.field('home', 'Domicile', 'PARIS FC')
    t.field('away', 'Extérieur', 'O. LYON')
    t.field('hc', 'Couleur domicile', TEAMS['home'][2], kind='color')
    t.field('ac', 'Couleur extérieur', TEAMS['away'][2], kind='color')
    STATS = [('Possession (%)', 54, 46), ('Tirs', 9, 6), ('Tirs cadrés', 4, 2), ('Corners', 5, 3), ('Fautes', 8, 11), ('Hors-jeu', 1, 3)]
    for i, (lab, h_, a_) in enumerate(STATS, 1):
        t.field(f'l{i}', f'Statistique {i}', lab)
        t.field(f'h{i}', f'{lab} · domicile', h_, kind='number', min=0)
        t.field(f'a{i}', f'{lab} · extérieur', a_, kind='number', min=0)
    panel = t.group('Panneau', 360, 130, 1200, 820, tracks=merge(inout('opacity', 1, 0, 110, 0, 12, 10, 8), inout('y', 130, 170, 110, 0, 24, 10, 8)))
    t.rect('Fond', 0, 0, 1200, 820, hexc(NIGHT, 0.94), panel, radius=8, grad=('#0c1430f0', 180))
    t.rect('Entête', 0, 0, 1200, 120, hexc(PANEL), panel, corners=(8, 8, 0, 0))
    t.rect('Couleur dom', 0, 0, 600, 6, hexc(TEAMS['home'][2]), panel, bindings={'fill': 'hc'}, corners=(8, 0, 0, 0))
    t.rect('Couleur ext', 600, 0, 600, 6, hexc(TEAMS['away'][2]), panel, bindings={'fill': 'ac'}, corners=(0, 8, 0, 0))
    t.text('Domicile', '{{home}}', 48, 6, 400, 114, 54, panel, family=DISPLAY, weight=400, spacing=1)
    t.text('Extérieur', '{{away}}', 752, 6, 400, 114, 54, panel, family=DISPLAY, weight=400, align='right', spacing=1)
    t.text('Titre', '{{title}}', 300, 6, 600, 114, 24, panel, align='center', family=COND, weight=700, color=hexc(LIME), spacing=2.5)
    BW = 1104
    for i in range(1, 7):
        y = 150 + (i - 1) * 108
        d = 16 + i * 5
        row = t.group(f'Ligne {i}', 48, y, BW, 100, panel, tracks=merge(inout('opacity', 1, 0, 110, d, 12, 6, 6 - i), inout('x', 48, 88, 110, d, 20, 6, 6 - i)))
        t.text('Domicile', '{{h%d}}' % i, 0, 0, 200, 64, 56, row, family=DISPLAY, weight=400)
        t.text('Libellé', '{{l%d}}' % i, 200, 0, BW - 400, 64, 28, row, align='center', family=COND, weight=600, upper=True, spacing=1.5, color=hexc('#dfe6f5'))
        t.text('Extérieur', '{{a%d}}' % i, BW - 200, 0, 200, 64, 56, row, align='right', family=DISPLAY, weight=400)
        t.rect('Ext', 0, 70, BW, 10, hexc(TEAMS['away'][2]), row, radius=5, bindings={'fill': 'ac'}, tracks=inout('cropLeft', 0, 1, 110, d + 6, 24, 6))
        hb = t.rect('Dom', 0, 70, BW / 2, 10, hexc(TEAMS['home'][2]), row, radius=5, bindings={'fill': 'hc'}, tracks=inout('cropRight', 0, 1, 110, d + 6, 24, 6))
        split_bar(t, f'h{i}', f'a{i}', hb, BW)
    p.item('Stats mi-temps', 'stats', 'match')

    # -- Score final ------------------------------------------------------------------------------------
    t = p.template('final', 'Score final', 'full', duration=130, on_air=90)
    t.field('label', 'Étiquette', 'SCORE FINAL', options=[['SCORE FINAL', 'Final'], ['MI-TEMPS', 'Mi-temps']])
    t.field('home', 'Domicile', 'PARIS FC')
    t.field('away', 'Extérieur', 'OLYMPIQUE LYON')
    t.field('hs', 'Score domicile', '2', kind='number', min=0)
    t.field('as', 'Score extérieur', '1', kind='number', min=0)
    t.field('hg', 'Buteurs domicile', 'Kebbal 23e\nKrasso 71e', kind='multiline')
    t.field('ag', 'Buteurs extérieur', 'Lacazette 64e (s.p.)', kind='multiline')
    t.field('hc', 'Couleur domicile', TEAMS['home'][2], kind='color')
    t.field('ac', 'Couleur extérieur', TEAMS['away'][2], kind='color')
    bg = t.group('Fond', 0, 0, 1920, 1080, tracks=fade(90, 0, 12, 12, 10))
    t.rect('Nuit', 0, 0, 1920, 1080, hexc('#142046'), bg, grad=('radial', NIGHT))
    t.rect('Dom', 0, 0, 960, 1080, hexc(TEAMS['home'][2], 0.5), bg, bindings={'fill': 'hc'}, opacity=0.35, shape='parallelogram', skew=-160, tracks=inout('x', -100, -1100, 90, 0, 30, 12))
    t.rect('Ext', 1060, 0, 960, 1080, hexc(TEAMS['away'][2], 0.5), bg, bindings={'fill': 'ac'}, opacity=0.35, shape='parallelogram', skew=-160, tracks=inout('x', 1060, 2000, 90, 0, 30, 12))
    mid = t.group('Score', 160, 240, 1600, 600)
    t.rect('Étiquette fond', 640, 0, 320, 52, hexc(LIME), mid, tracks=inout('scaleX', 1, 0, 90, 8, 14, 8, 6))
    t.text('Étiquette', '{{label}}', 640, 0, 320, 52, 30, mid, align='center', color=hexc(NIGHT), family=COND, weight=800, spacing=2.5, tracks=inout('opacity', 1, 0, 90, 14, 8, 6, 6))
    t.text('Domicile', '{{home}}', 0, 110, 560, 120, 76, mid, align='right', family=DISPLAY, weight=400, spacing=1, tracks=merge(inout('opacity', 1, 0, 90, 10, 12, 6, 4), inout('x', 0, -60, 90, 10, 22, 6, 4)))
    t.text('Extérieur', '{{away}}', 1040, 110, 560, 120, 76, mid, family=DISPLAY, weight=400, spacing=1, tracks=merge(inout('opacity', 1, 0, 90, 10, 12, 6, 4), inout('x', 1040, 1100, 90, 10, 22, 6, 4)))
    t.rect('Score fond', 600, 80, 400, 190, WHITE, mid, radius=6, tracks=merge(inout('scaleX', 1, 0.7, 90, 4, 18, 8, 2, 'back'), inout('scaleY', 1, 0.7, 90, 4, 18, 8, 2, 'back')))
    t.text('Score', '{{hs}} – {{as}}', 600, 92, 400, 190, 170, mid, align='center', color=hexc(NIGHT), family=DISPLAY, weight=400, tracks=inout('opacity', 1, 0, 90, 12, 10, 6, 2))
    t.text('Buteurs dom', '{{hg}}', 0, 300, 560, 220, 34, mid, align='right', valign='top', family=COND, weight=600, color=hexc('#dfe6f5'), lineHeight=1.35, tracks=inout('opacity', 1, 0, 90, 24, 12, 6))
    t.text('Buteurs ext', '{{ag}}', 1040, 300, 560, 220, 34, mid, valign='top', family=COND, weight=600, color=hexc('#dfe6f5'), lineHeight=1.35, tracks=inout('opacity', 1, 0, 90, 24, 12, 6))
    p.item('Score final', 'final', 'fin')
    p.item('Mi-temps', 'final', 'match', {'label': 'MI-TEMPS', 'hs': '1', 'as': '0', 'hg': 'Kebbal 23e', 'ag': ''})
    return p


REGIONS = ['Hauts-de-France', 'Normandie', 'Île-de-France', 'Grand Est', 'Bretagne', 'Pays de la Loire', 'Centre-Val de Loire',
           'Bourgogne-Franche-Comté', 'Nouvelle-Aquitaine', 'Auvergne-Rhône-Alpes', 'Occitanie', "Provence-Alpes-Côte d'Azur", 'Corse']


def initials(name):
    return ''.join(w[0] for w in name.split()[:2]).upper()


def hemicycle(t, seat_keys, total_key, arcs):
    """Each arc spans its share of a half circle (270° to 450°): angles from the seat fields (logic)."""
    nodes, edges = [], []
    nodes += [{'id': 'hT', 'type': 'field', 'key': total_key, 'x': 40, 'y': 40}, {'id': 'h180', 'type': 'constant', 'value': 180, 'x': 40, 'y': 140},
              {'id': 'h270', 'type': 'constant', 'value': 270, 'x': 40, 'y': 240}]
    prev = None
    for i, (k, arc) in enumerate(zip(seat_keys, arcs)):
        nodes.append({'id': f'hs{i}', 'type': 'field', 'key': k, 'x': 260, 'y': 40 + i * 160})
        if prev is None:
            cum = f'hs{i}'
        else:
            cum = f'hc{i}'
            nodes.append({'id': cum, 'type': 'math', 'op': 'add', 'x': 480, 'y': 40 + i * 160})
            edges += [{'from': prev, 'to': cum, 'port': 'a'}, {'from': f'hs{i}', 'to': cum, 'port': 'b'}]
        # angle(cum) = cum / total * 180 + 270
        nodes += [{'id': f'hd{i}', 'type': 'math', 'op': 'div', 'x': 700, 'y': 40 + i * 160},
                  {'id': f'hm{i}', 'type': 'math', 'op': 'mul', 'x': 900, 'y': 40 + i * 160},
                  {'id': f'ha{i}', 'type': 'math', 'op': 'add', 'x': 1100, 'y': 40 + i * 160},
                  {'id': f'he{i}', 'type': 'target', 'layer': arc, 'prop': 'arcEnd', 'mode': 'set', 'x': 1300, 'y': 40 + i * 160},
                  {'id': f'hb{i}', 'type': 'target', 'layer': arc, 'prop': 'arcStart', 'mode': 'set', 'x': 1300, 'y': 100 + i * 160}]
        edges += [{'from': cum, 'to': f'hd{i}', 'port': 'a'}, {'from': 'hT', 'to': f'hd{i}', 'port': 'b'},
                  {'from': f'hd{i}', 'to': f'hm{i}', 'port': 'a'}, {'from': 'h180', 'to': f'hm{i}', 'port': 'b'},
                  {'from': f'hm{i}', 'to': f'ha{i}', 'port': 'a'}, {'from': 'h270', 'to': f'ha{i}', 'port': 'b'},
                  {'from': f'ha{i}', 'to': f'he{i}', 'port': 'in'}]
        edges.append({'from': f'ha{i - 1}' if i else 'h270', 'to': f'hb{i}', 'port': 'in'})
        prev = cum
    t.logic(nodes, edges)


# ======================================================================
# 04 · Soirée électorale
# ======================================================================
def ex04():
    PAPER, INK, BLUE, GOLD, LINE, MUTED = '#f6f3ec', '#151b2c', '#1c3f94', '#b8902f', '#ddd6c6', '#6b6f7b'
    CANDS = [('Hélène Marchal', 'Alliance centrale', '#e9a100', 27.8), ('Bastien Roux', 'Union de la droite', '#2b5cd6', 22.1),
             ('Nadia Ferhat', 'Gauche unie', '#e0315b', 19.6), ('Julien Moreau', 'Parti de la Nation', '#1d2b5c', 15.3),
             ('Camille Dufresne', 'Les Écologistes', '#2e9e57', 8.9), ('Thomas Lenoir', 'Nouvelle République', '#8b5cf6', 6.3)]
    p = Package(
        'ex-04-soiree-electorale', 'Soirée électorale', 'Élections',
        ['Résultats', 'Hémicycle', 'Carte des régions', 'Projection', 'Participation'],
        "Une soirée électorale au ton éditorial, sur fond papier : résultats par candidat (barres et pourcentages, lus dans une source de données que l'automatisation met à jour), hémicycle dont les arcs se calculent à partir des sièges, carte des régions coloriée par candidat en tête, bandeau de projection, participation et dépouillement en direct, synthés des analystes. Playfair Display et Inter.",
        en=('Election night', 'An editorial election night on a paper background: results by candidate, a hemicycle whose arcs are computed from the seats, a map of the regions coloured by leading candidate, projection banner, live turnout and count, analyst lower thirds.'),
        cover='results')
    p.zone('full', 'Plein écran', 50, '#94a3b8')
    p.zone('lower', 'Bandeau', 20, '#7c5cff', (0.04, 0.66, 0.92, 0.29))
    p.zone('corner', 'Participation', 30, '#22c55e', (0.70, 0.05, 0.26, 0.24))
    p.playlist('soir', 'Soirée')
    p.playlist('analyse', 'Plateau')

    # Results as an automation (the ministry feed, a newsroom system) pushes them:
    # PUT /api/data/sources/election-resultats/value with this JSON.
    p.source('election-resultats', 'Élection · résultats nationaux', {
        'tour': 'Premier tour', 'estimation': '20:00', 'depouilles': 87.4, 'participation': 61.3,
        'candidats': [{'nom': n_, 'parti': pa_, 'score': v_} for n_, pa_, _, v_ in CANDS]})

    def bound(t, key, path):
        t.t['fields'][-1]['binding'] = {'source': 'election-resultats', 'path': path}

    def cand_fields(t):
        for i, (n_, pa_, c_, v_) in enumerate(CANDS, 1):
            t.field(f'n{i}', f'Candidat {i}', n_)
            bound(t, f'n{i}', f'candidats[{i - 1}].nom')
            t.field(f'i{i}', f'Initiales {i}', initials(n_))
            t.field(f'pa{i}', f'Parti {i}', pa_)
            bound(t, f'pa{i}', f'candidats[{i - 1}].parti')
            t.field(f'c{i}', f'Couleur {i}', c_, kind='color')
            t.field(f'p{i}', f'Score {i} (%)', v_, kind='number', min=0, max=100)
            bound(t, f'p{i}', f'candidats[{i - 1}].score')

    # -- Résultats ---------------------------------------------------------------------------
    t = p.template('results', 'Résultats par candidat', 'full', duration=160, on_air=120)
    t.field('title', 'Titre', 'Premier tour')
    t.field('sub', 'Sous-titre', 'Résultats nationaux · estimation à 20 h 00')
    t.field('count', 'Dépouillement', '87,4 % des bulletins dépouillés')
    cand_fields(t)
    pn = t.group('Panneau', 160, 100, 1600, 880, tracks=merge(inout('opacity', 1, 0, 120, 0, 14, 10, 10), inout('y', 100, 140, 120, 0, 26, 10, 10)))
    t.rect('Papier', 0, 0, 1600, 880, hexc(PAPER), pn, radius=4, shadow=(hexc('#000000', 0.35), 0, 18, 50))
    t.rect('Filet or', 64, 64, 64, 4, hexc(GOLD), pn)
    t.text('Titre', '{{title}}', 64, 74, 900, 90, 68, pn, color=hexc(INK), family=SERIF, weight=700)
    t.text('Sous-titre', '{{sub}}', 64, 160, 1000, 40, 26, pn, color=hexc(MUTED), weight=500)
    t.text('Dépouillement', '{{count}}', 900, 160, 636, 40, 22, pn, color=hexc(BLUE), weight=700, align='right', spacing=0.5)
    t.rect('Règle', 64, 218, 1472, 2, hexc(INK), pn)
    for i in range(1, 7):
        y = 240 + (i - 1) * 104
        d = 18 + i * 6
        row = t.group(f'Candidat {i}', 64, y, 1472, 104, pn, tracks=merge(inout('opacity', 1, 0, 120, d, 12, 8, 6 - i), inout('x', 64, 24, 120, d, 22, 8, 6 - i)))
        t.ellipse('Pastille', 0, 16, 72, 72, hexc(CANDS[i - 1][2]), row, bindings={'fill': f'c{i}'})
        t.text('Initiales', '{{i%d}}' % i, 0, 16, 72, 72, 26, row, align='center', weight=800, spacing=0.5)
        t.text('Nom', '{{n%d}}' % i, 100, 14, 400, 44, 32, row, color=hexc(INK), weight=700)
        t.text('Parti', '{{pa%d}}' % i, 100, 56, 400, 32, 21, row, color=hexc(MUTED), weight=500)
        t.progress('Barre', '{{p%d}}' % i, 520, 38, 700, 28, hexc(CANDS[i - 1][2]), row, max='35', track='#e7e1d3', duration=1.6, rounded=True,
                   tracks=None)
        t.layer_ref(t.t['layers'][-1]['id'])['bindings'] = {'fill': f'c{i}'}
        t.counter('Score', '{{p%d}}' % i, 1240, 10, 232, 84, 52, row, decimals=1, suffix=N + '%', color=hexc(INK), weight=800, align='right', duration=1.6, affixSize=55.0, affixColor=MUTED)
        if i < 6:
            t.rect('Séparateur', 100, 103, 1372, 1, hexc(LINE), row)
    p.item('Résultats 20 h', 'results', 'soir')

    # -- Hémicycle ----------------------------------------------------------------------------
    t = p.template('seats', 'Hémicycle', 'full', duration=170, on_air=130)
    t.field('title', 'Titre', 'Projection en sièges')
    t.field('sub', 'Sous-titre', 'Assemblée nationale · 577 sièges')
    t.field('total', 'Total des sièges', '577', kind='number')
    t.field('maj', 'Majorité', 'Majorité absolue : 289 sièges')
    GROUPS = [('Gauche unie', '#e0315b', 151), ('Écologistes', '#2e9e57', 38), ('Alliance centrale', '#e9a100', 168), ('Union de la droite', '#2b5cd6', 92), ('Parti de la Nation', '#1d2b5c', 128)]
    for i, (n_, c_, v_) in enumerate(GROUPS, 1):
        t.field(f'g{i}', f'Groupe {i}', n_)
        t.field(f'gc{i}', f'Couleur {i}', c_, kind='color')
        t.field(f's{i}', f'Sièges {i}', v_, kind='number', min=0, max=577)
    pn = t.group('Panneau', 160, 100, 1600, 880, tracks=merge(inout('opacity', 1, 0, 130, 0, 14, 10, 10), inout('y', 100, 140, 130, 0, 26, 10, 10)))
    t.rect('Papier', 0, 0, 1600, 880, hexc(PAPER), pn, radius=4, shadow=(hexc('#000000', 0.35), 0, 18, 50))
    t.rect('Filet or', 64, 64, 64, 4, hexc(GOLD), pn)
    t.text('Titre', '{{title}}', 64, 74, 1000, 90, 68, pn, color=hexc(INK), family=SERIF, weight=700)
    t.text('Sous-titre', '{{sub}}', 64, 160, 1000, 40, 26, pn, color=hexc(MUTED), weight=500)
    R, CX, CY = 400, 800, 640
    hemi = t.group('Hémicycle', CX - R, CY - R, 2 * R, R, pn, clip=True,
                   tracks=merge(inout('scaleX', 1, 0.85, 130, 8, 30, 10), inout('scaleY', 1, 0.85, 130, 8, 30, 10), inout('opacity', 1, 0, 130, 8, 16, 10)))
    arcs = []
    for i, (_, c_, _) in enumerate(GROUPS, 1):
        arcs.append(t.ellipse(f'Arc {i}', 0, 0, 2 * R, 2 * R, hexc(c_), hemi, start=270, end=330, inner=52, bindings={'fill': f'gc{i}'},
                              stroke=(hexc(PAPER), 4)))
    hemicycle(t, [f's{i}' for i in range(1, 6)], 'total', arcs)
    t.rect('Majorité', CX - 1.5, CY - R - 24, 3, 210, hexc(INK), pn, tracks=inout('cropBottom', 0, 1, 130, 30, 16, 8))
    t.counter('Total', '{{total}}', CX - 200, CY - 150, 400, 110, 96, pn, color=hexc(INK), family=SERIF, weight=700, align='center', duration=1.4)
    t.text('Sièges', 'sièges', CX - 200, CY - 46, 400, 40, 24, pn, color=hexc(MUTED), align='center', weight=500)
    t.text('Majorité texte', '{{maj}}', CX - 400, CY + 10, 800, 36, 22, pn, color=hexc(BLUE), align='center', weight=700, spacing=0.5)
    t.rect('Règle', 64, 700, 1472, 2, hexc(INK), pn)
    for i in range(1, 6):
        x = 64 + (i - 1) * 300
        lg = t.group(f'Légende {i}', x, 720, 280, 130, pn, tracks=merge(inout('opacity', 1, 0, 130, 40 + i * 5, 12, 8), inout('y', 720, 750, 130, 40 + i * 5, 20, 8)))
        t.rect('Couleur', 0, 14, 36, 8, hexc(GROUPS[i - 1][1]), lg, bindings={'fill': f'gc{i}'})
        t.text('Groupe', '{{g%d}}' % i, 0, 28, 280, 40, 23, lg, color=hexc(INK), weight=600)
        t.counter('Sièges', '{{s%d}}' % i, 0, 64, 280, 70, 58, lg, color=hexc(INK), family=SERIF, weight=700, duration=1.6)
    p.item('Projection en sièges', 'seats', 'soir')

    # -- Carte des régions ---------------------------------------------------------------------
    t = p.template('map', 'Carte des régions', 'full', duration=150, on_air=110)
    t.field('title', 'Titre', 'En tête par région')
    t.field('sub', 'Sous-titre', 'Premier tour · résultats provisoires')
    lead = ['Bastien Roux', 'Bastien Roux', 'Hélène Marchal', 'Julien Moreau', 'Nadia Ferhat', 'Hélène Marchal', 'Bastien Roux',
            'Julien Moreau', 'Hélène Marchal', 'Hélène Marchal', 'Nadia Ferhat', 'Julien Moreau', 'Bastien Roux']
    opts = [[c[0], c[0]] for c in CANDS[:4]]
    for i, (r_, l_) in enumerate(zip(REGIONS, lead), 1):
        t.field(f'r{i}', r_, l_, options=opts)
    pn = t.group('Panneau', 160, 100, 1600, 880, tracks=merge(inout('opacity', 1, 0, 110, 0, 14, 10, 10), inout('y', 100, 140, 110, 0, 26, 10, 10)))
    t.rect('Papier', 0, 0, 1600, 880, hexc(PAPER), pn, radius=4, shadow=(hexc('#000000', 0.35), 0, 18, 50))
    t.rect('Filet or', 64, 64, 64, 4, hexc(GOLD), pn)
    t.text('Titre', '{{title}}', 64, 74, 600, 160, 64, pn, color=hexc(INK), family=SERIF, weight=700, wrap=True, valign='top', lineHeight=1.05)
    t.text('Sous-titre', '{{sub}}', 64, 240, 560, 40, 24, pn, color=hexc(MUTED), weight=500)
    for i, (n_, pa_, c_, _) in enumerate(CANDS[:4]):
        lg = t.group(f'Légende {i + 1}', 64, 340 + i * 86, 520, 70, pn, tracks=merge(inout('opacity', 1, 0, 110, 30 + i * 5, 12, 8), inout('x', 64, 30, 110, 30 + i * 5, 20, 8)))
        t.rect('Couleur', 0, 10, 50, 50, hexc(c_), lg, radius=3)
        t.text('Nom', n_, 70, 4, 450, 36, 28, lg, color=hexc(INK), weight=700)
        t.text('Parti', pa_, 70, 38, 450, 28, 19, lg, color=hexc(MUTED), weight=500)
    t.widget('map', 'Carte', 660, 24, 900, 832, pn, {}, style='vector', colors={'water': PAPER, 'land': '#e9e3d5', 'border': '#ffffff', 'grid': ''},
             showBorders=True, showRoads=False, graticule=False, radius=0.0, view={'lat': 46.5, 'lon': 2.5, 'zoom': 6.0, 'rot': 0.0, 'tilt': 0.0}, views=[],
             layers={'markers': False, 'routes': False, 'zones': True, 'labels': False}, markers=[], routes=[],
             zones={'level': 'regions', 'mode': 'category', 'rows': [['Région', 'En tête']] + [[r_, '{{r%d}}' % i] for i, r_ in enumerate(REGIONS, 1)],
                    'categories': [{'name': c[0], 'color': c[2]} for c in CANDS[:4]], 'scale': [], 'unit': '', 'title': '', 'opacity': 1.0, 'legend': False, 'legendPos': 'bl'},
             labels={'cities': False, 'countries': False, 'minImp': 3.0, 'size': 22.0, 'collide': True, 'pad': 8.0, 'showHidden': False},
             attribution={'provider': 'own', 'pos': 'br', 'style': 'light', 'show': True},
             tracks=inout('opacity', 1, 0, 110, 14, 24, 10))
    p.item('Carte des régions', 'map', 'soir')

    # -- Bandeau projection ---------------------------------------------------------------------
    t = p.template('projection', 'Bandeau projection', 'lower', duration=120, on_air=90)
    t.field('label', 'Étiquette', 'PROJECTION · 20:00')
    t.field('name', 'Nom', 'Hélène Marchal')
    t.field('init', 'Initiales', 'HM')
    t.field('caption', 'Légende', 'arrive en tête du premier tour')
    t.field('pct', 'Score (%)', '27.8', kind='number', min=0, max=100)
    t.field('color', 'Couleur', '#e9a100', kind='color')
    g = t.group('Projection', 96, 770, 1728, 210)
    t.rect('Étiquette fond', 0, 0, 330, 50, hexc(BLUE), g, tracks=inout('cropRight', 0, 1, 90, 0, 14, 10, 8))
    t.text('Étiquette', '{{label}}', 22, 0, 400, 50, 22, g, weight=800, spacing=2.5, tracks=inout('opacity', 1, 0, 90, 8, 10, 6, 8))
    t.rect('Papier', 0, 50, 1728, 150, hexc(PAPER), g, shadow=(hexc('#000000', 0.3), 0, 10, 30), tracks=inout('cropRight', 0, 1, 90, 4, 26, 12, 2))
    t.rect('Couleur', 0, 50, 10, 150, hexc('#e9a100'), g, bindings={'fill': 'color'}, tracks=inout('cropBottom', 0, 1, 90, 10, 14, 8, 2))
    t.ellipse('Pastille', 44, 75, 100, 100, hexc('#e9a100'), g, bindings={'fill': 'color'}, tracks=merge(inout('scaleX', 1, 0, 90, 14, 16, 8, 0, 'back'), inout('scaleY', 1, 0, 90, 14, 16, 8, 0, 'back')))
    t.text('Initiales', '{{init}}', 44, 75, 100, 100, 36, g, align='center', weight=800, tracks=inout('opacity', 1, 0, 90, 20, 8, 6))
    t.text('Nom', '{{name}}', 180, 60, 1100, 90, 66, g, color=hexc(INK), family=SERIF, weight=700, tracks=merge(inout('opacity', 1, 0, 90, 16, 12, 6), inout('x', 180, 220, 90, 16, 20, 6)))
    t.text('Légende', '{{caption}}', 182, 142, 1000, 44, 28, g, color=hexc(MUTED), weight=500, tracks=inout('opacity', 1, 0, 90, 22, 12, 6))
    t.counter('Score', '{{pct}}', 1290, 56, 390, 140, 104, g, decimals=1, suffix=N + '%', color=hexc(INK), family=SERIF, weight=700, align='right', duration=1.6, affixSize=50.0,
              tracks=inout('opacity', 1, 0, 90, 20, 10, 6))
    p.item('Projection Marchal', 'projection', 'soir')
    p.item('Projection Roux', 'projection', 'soir', {'name': 'Bastien Roux', 'init': 'BR', 'caption': 'qualifié pour le second tour', 'pct': '22.1', 'color': '#2b5cd6'})

    # -- Participation ---------------------------------------------------------------------------
    t = p.template('turnout', 'Participation et dépouillement', 'corner', duration=120, on_air=90)
    t.field('label', 'Libellé', 'PARTICIPATION À 17 H')
    t.field('turnout', 'Participation (%)', '61.3', kind='number', min=0, max=100)
    bound(t, 'turnout', 'participation')
    t.field('delta', 'Comparaison', '+2,1 pts par rapport à 2022')
    t.field('counted', 'Dépouillés (%)', '87.4', kind='number', min=0, max=100)
    bound(t, 'counted', 'depouilles')
    g = t.group('Participation', 1360, 70, 464, 236, tracks=merge(inout('opacity', 1, 0, 90, 0, 12, 8), inout('x', 1360, 1420, 90, 0, 22, 8)))
    t.rect('Papier', 0, 0, 464, 236, hexc(PAPER), g, radius=4, shadow=(hexc('#000000', 0.3), 0, 10, 30))
    t.rect('Filet', 0, 0, 464, 6, hexc(BLUE), g, corners=(4, 4, 0, 0))
    t.text('Libellé', '{{label}}', 28, 22, 420, 30, 19, g, color=hexc(BLUE), weight=800, spacing=2)
    t.counter('Participation', '{{turnout}}', 28, 50, 420, 86, 76, g, decimals=1, suffix=N + '%', color=hexc(INK), family=SERIF, weight=700, duration=1.4, affixSize=50.0)
    t.text('Comparaison', '{{delta}}', 28, 134, 420, 28, 19, g, color=hexc(MUTED), weight=500)
    t.text('Dépouillement', 'Dépouillement', 28, 172, 250, 26, 18, g, color=hexc(INK), weight=700)
    t.counter('Dépouillés', '{{counted}}', 300, 172, 136, 26, 18, g, decimals=1, suffix=N + '%', color=hexc(INK), weight=700, align='right', duration=1.4)
    t.progress('Barre', '{{counted}}', 28, 204, 408, 10, hexc(BLUE), g, track='#e2dccd', rounded=True, duration=1.6)
    p.item('Participation 17 h', 'turnout', 'soir')

    # -- Synthé analyste ---------------------------------------------------------------------------
    t = p.template('analyst', 'Synthé analyste', 'lower', duration=70, on_air=40)
    t.field('name', 'Nom', 'Jérôme Fourquet-Lambert')
    t.field('role', 'Fonction', 'Politologue, directeur d’études')
    g = t.group('Synthé', 96, 820, 1100, 130)
    pl = t.rect('Papier', 0, 0, 700, 130, hexc(PAPER), g, shadow=(hexc('#000000', 0.3), 0, 10, 30), tracks=inout('cropRight', 0, 1, 40, 0, 20, 10, 4))
    t.rect('Or', 0, 0, 8, 130, hexc(GOLD), g, tracks=inout('cropBottom', 0, 1, 40, 0, 12, 8, 6))
    nm = t.text('Nom', '{{name}}', 40, 10, 1040, 70, 50, g, color=hexc(INK), family=SERIF, weight=700, tracks=merge(inout('opacity', 1, 0, 40, 8, 12, 8), inout('x', 40, 16, 40, 8, 18, 8)))
    t.text('Fonction', '{{role}}', 42, 78, 1040, 36, 24, g, color=hexc(BLUE), weight=600, tracks=inout('opacity', 1, 0, 40, 14, 12, 8))
    t.fit(nm, [(pl, 'width')], pad=84, minimum=480)
    p.item('Analyste', 'analyst', 'analyse')
    p.item('Éditorialiste', 'analyst', 'analyse', {'name': 'Anne Rosier', 'role': 'Éditorialiste politique'})
    return p


def wx_icon(t, kind, parent, x, y, s, name=None):
    """Weather icon (1 sun, 2 sunny spells, 3 cloudy, 4 rain, 5 storm, 6 snow) in an s × s box."""
    g = t.group(name or f'Icône {kind}', x, y, s, s, parent)

    def sun(cx, cy, r):
        t.ellipse('Halo', cx - r * 1.5, cy - r * 1.5, r * 3, r * 3, hexc('#ffd84d', 0.5), g, grad=('radial', '#ffd84d00'))
        t.ellipse('Soleil', cx - r, cy - r, 2 * r, 2 * r, hexc('#ffd84d'), g, grad=('radial', '#ffa31a'))

    def cloud(ox, oy, k, col='#ffffff', col2='#dce7f5'):
        t.ellipse('Nuage 1', ox + 0.02 * k, oy + 0.30 * k, 0.40 * k, 0.40 * k, hexc(col), g)
        t.ellipse('Nuage 2', ox + 0.22 * k, oy + 0.08 * k, 0.52 * k, 0.52 * k, hexc(col), g)
        t.ellipse('Nuage 3', ox + 0.56 * k, oy + 0.28 * k, 0.40 * k, 0.40 * k, hexc(col), g)
        t.rect('Base', ox + 0.18 * k, oy + 0.42 * k, 0.62 * k, 0.28 * k, hexc(col), g, radius=0.14 * k)
    if kind == 1:
        sun(s / 2, s / 2, s * 0.28)
    elif kind == 2:
        sun(s * 0.36, s * 0.36, s * 0.22)
        cloud(s * 0.22, s * 0.32, s * 0.74)
    elif kind == 3:
        cloud(s * 0.2, s * 0.1, s * 0.62, '#c9d6e6', '#a9b9cf')
        cloud(s * 0.04, s * 0.26, s * 0.82)
    elif kind in (4, 5, 6):
        dark = kind == 5
        cloud(s * 0.06, s * 0.04, s * 0.86, '#aebdd2' if dark else '#e8eef7', '#8396b3' if dark else '#c6d3e6')
        if kind == 4:
            for i in range(3):
                t.rect('Goutte', s * (0.3 + i * 0.17), s * 0.68, s * 0.05, s * 0.2, hexc('#5ec8ff'), g, radius=s * 0.025, rotation=18)
        elif kind == 5:
            t.poly('Éclair', s * 0.38, s * 0.56, s * 0.26, s * 0.42, hexc('#ffd400'), [[0.55, 0], [0, 0.58], [0.45, 0.58], [0.3, 1], [1, 0.38], [0.55, 0.38], [0.8, 0]], g)
        else:
            for i in range(3):
                t.ellipse('Flocon', s * (0.28 + i * 0.18), s * (0.72 + (i % 2) * 0.1), s * 0.09, s * 0.09, WHITE, g)
    return g


WX_OPTS = [['1', 'Soleil'], ['2', 'Éclaircies'], ['3', 'Nuageux'], ['4', 'Pluie'], ['5', 'Orage'], ['6', 'Neige']]


def wx_choice(t, key, parent, x, y, s):
    t.choose(key, {k: [wx_icon(t, k, parent, x, y, s)] for k in range(1, 7)})


# ======================================================================
# 05 · Météo
# ======================================================================
def ex05():
    SKY1, SKY2, CARD, SUN, COLD, WARM = '#1d6fd6', '#0a2a5e', '#ffffff', '#ffcc33', '#7fd6ff', '#ff7a3d'
    p = Package(
        'ex-05-meteo', 'Météo', 'Météo',
        ['Carte', 'Villes', 'Semaine', 'Vigilance', 'Icônes'],
        "Un bulletin météo complet : carte de France des températures par région (échelle de couleurs), cartes des villes avec icônes au choix (soleil, éclaircies, nuages, pluie, orage, neige), prévisions sur sept jours, bandeau de vigilance jaune, orange ou rouge et synthé du présentateur. Bleu ciel et verre dépoli, Montserrat. Version 9:16 pour les réseaux sociaux.",
        en=('Weather', 'A complete weather bulletin: temperature map of France by region, city cards with selectable icons (sun, sunny spells, clouds, rain, storm, snow), seven-day forecast, yellow / orange / red warning banner and presenter lower third. Also in 9:16 for social networks.'),
        cover='cities')
    p.vertical()
    p.zone('full', 'Plein écran', 50, '#94a3b8')
    p.zone('lower', 'Bas d’écran', 20, '#7c5cff', (0.04, 0.64, 0.92, 0.31), vertical=(0.04, 0.68, 0.92, 0.28))
    p.zone('top', 'Vigilance', 30, '#f5a524', (0.04, 0.04, 0.92, 0.14), vertical=(0.04, 0.06, 0.92, 0.12))
    p.playlist('meteo', 'Bulletin')

    def sky(t, on_air):
        bg = t.group('Ciel', 0, 0, 1920, 1080, tracks=fade(on_air, 0, 14, 14, 10))
        t.override(bg, width=1080, height=1920)
        r = t.rect('Dégradé', 0, 0, 1920, 1080, hexc(SKY1), bg, grad=(SKY2, 180))
        t.override(r, width=1080, height=1920)
        e = glow(t, 'Lumière', 1100, -500, 1300, '#ffffff', 0.16, bg)
        t.override(e, x=200, y=-600)
        return bg

    # -- Carte ----------------------------------------------------------------------------------------
    t = p.template('map', 'Carte des températures', 'full', duration=150, on_air=110)
    t.field('title', 'Titre', 'Demain')
    t.field('date', 'Date', 'Mercredi 23 octobre')
    t.field('summary', 'Résumé', 'Doux et ensoleillé au sud, averses de la Bretagne aux Hauts-de-France.', kind='multiline')
    temps = [13, 14, 15, 13, 14, 16, 16, 15, 19, 17, 21, 22, 21]
    for i, (r_, v_) in enumerate(zip(REGIONS, temps), 1):
        t.field(f't{i}', f'{r_} (°C)', v_, kind='number', min=-20, max=45)
    sky(t, 110)
    col = t.group('Texte', 120, 140, 560, 700, tracks=merge(inout('opacity', 1, 0, 110, 10, 14, 8), inout('x', 120, 80, 110, 10, 24, 8)))
    t.override(col, x=80, y=120, width=920, height=400)
    t.text('Titre', '{{title}}', 0, 0, 560, 110, 96, col, family=GEO, weight=800, spacing=-2)
    t.text('Date', '{{date}}', 0, 110, 560, 50, 32, col, family=GEO, weight=600, color=hexc(SUN))
    sm = t.text('Résumé', '{{summary}}', 0, 190, 520, 200, 32, col, weight=500, wrap=True, valign='top', lineHeight=1.3, color=hexc('#e6f0ff'))
    t.override(sm, width=900, height=130, fontSize=36)
    leg = t.group('Échelle', 0, 470, 520, 70, col)
    t.override(leg, y=290, width=900)
    sc = t.rect('Échelle', 0, 0, 520, 14, hexc('#5ec8ff'), leg, radius=7, grad=('#ff5a36', 90, [(35, '#7dd3a8'), (65, '#ffcc33')]))
    t.override(sc, width=900)
    for k, (lab, x) in enumerate((('0°', 0), ('10°', 0.33), ('20°', 0.66), ('30°', 1))):
        tl = t.text('Graduation', lab, x * 520 - 40, 22, 80, 30, 22, leg, align='center', weight=600)
        t.override(tl, x=x * 900 - 40)
    mp = t.widget('map', 'Carte', 700, 60, 1120, 960, None, {}, style='vector', colors={'water': '#0b2f66', 'land': '#1a4a8c', 'border': '#ffffff', 'grid': ''},
                  showBorders=True, showRoads=False, graticule=False, radius=0.0, view={'lat': 46.5, 'lon': 2.5, 'zoom': 5.7, 'rot': 0.0, 'tilt': 0.0}, views=[],
                  layers={'markers': False, 'routes': False, 'zones': True, 'labels': True}, markers=[], routes=[],
                  zones={'level': 'regions', 'mode': 'scale', 'rows': [['Région', 'Température']] + [[r_, '{{t%d}}' % i] for i, r_ in enumerate(REGIONS, 1)],
                         'categories': [], 'scale': [{'v': 0, 'color': '#5ec8ff'}, {'v': 10, 'color': '#7dd3a8'}, {'v': 20, 'color': '#ffcc33'}, {'v': 30, 'color': '#ff5a36'}],
                         'unit': '°', 'title': '', 'opacity': 0.92, 'legend': False, 'legendPos': 'bl'},
                  labels={'cities': True, 'countries': False, 'minImp': 8.0, 'size': 26.0, 'collide': True, 'pad': 8.0, 'showHidden': False},
                  attribution={'provider': 'own', 'pos': 'br', 'style': 'light', 'show': True},
                  tracks=merge(inout('opacity', 1, 0, 110, 6, 20, 10), inout('scaleX', 1, 1.08, 110, 6, 40, 10), inout('scaleY', 1, 1.08, 110, 6, 40, 10)))
    t.override(mp, x=0, y=560, width=1080, height=1240)
    p.item('Carte de demain', 'map', 'meteo')

    # -- Villes ----------------------------------------------------------------------------------------
    t = p.template('cities', 'Villes', 'lower', duration=130, on_air=90)
    CITIES = [('Lille', 2, 14, 8, 'Éclaircies'), ('Paris', 4, 16, 10, 'Averses'), ('Lyon', 1, 19, 9, 'Ensoleillé'), ('Marseille', 1, 23, 14, 'Grand soleil')]
    for i, (c_, ic_, mx_, mn_, cd_) in enumerate(CITIES, 1):
        t.field(f'c{i}', f'Ville {i}', c_)
        t.field(f'ic{i}', f'Ciel {i}', ic_, options=WX_OPTS)
        t.field(f'mx{i}', f'Max {i} (°C)', mx_, kind='number', min=-30, max=50)
        t.field(f'mn{i}', f'Min {i} (°C)', mn_, kind='number', min=-30, max=50)
        t.field(f'cd{i}', f'Temps {i}', cd_)
    for i in range(1, 5):
        x = 96 + (i - 1) * 438
        d = 4 + i * 5
        cd = t.group(f'Ville {i}', x, 700, 414, 320, tracks=merge(inout('opacity', 1, 0, 90, d, 12, 8, (4 - i) * 2), inout('y', 700, 760, 90, d, 24, 8, (4 - i) * 2)))
        t.override(cd, x=60, y=1330 + (i - 1) * 124, width=960, height=112,
                   tracks=merge(inout('opacity', 1, 0, 90, d, 12, 8, (4 - i) * 2), inout('x', 60, 120, 90, d, 24, 8, (4 - i) * 2)))
        bgc = t.rect('Verre', 0, 0, 414, 320, hexc(SKY2, 0.72), cd, radius=18, grad=('#0a2a5ecc', 180), stroke=(hexc('#ffffff', 0.25), 1.5))
        t.override(bgc, width=960, height=112)
        cn = t.text('Ville', '{{c%d}}' % i, 28, 18, 360, 50, 34, cd, family=GEO, weight=800)
        t.override(cn, x=130, y=8, width=360, height=56)
        cc = t.text('Temps', '{{cd%d}}' % i, 28, 64, 360, 34, 22, cd, weight=500, color=hexc('#cfe2ff'))
        t.override(cc, x=130, y=62, width=360, height=38)
        ic = t.group('Icône', 24, 110, 160, 160, cd)
        t.override(ic, visible=False)
        wx_choice(t, f'ic{i}', ic, 0, 0, 160)
        ic2 = t.group('Icône 9:16', 14, 6, 100, 100, cd)
        t.layer_ref(ic2)['visible'] = False
        t.override(ic2, visible=True)
        wx_choice(t, f'ic{i}', ic2, 0, 0, 100)
        mx = t.counter('Max', '{{mx%d}}' % i, 190, 104, 200, 120, 104, cd, suffix='°', family=GEO, weight=800, align='right', grouping=False)
        t.override(mx, x=640, y=0, width=200, height=112, fontSize=84)
        mn = t.counter('Min', '{{mn%d}}' % i, 190, 222, 200, 60, 40, cd, suffix='°', family=GEO, weight=600, color=hexc(COLD), align='right', grouping=False)
        t.override(mn, x=840, y=26, width=100, height=60, fontSize=40)
    p.item('Villes demain', 'cities', 'meteo')

    # -- Semaine ------------------------------------------------------------------------------------------
    t = p.template('week', 'Prévisions sur 7 jours', 'lower', duration=130, on_air=90)
    t.field('city', 'Ville', 'PARIS')
    DAYS = [('MER', 4, 16, 10), ('JEU', 2, 17, 9), ('VEN', 1, 19, 8), ('SAM', 1, 21, 10), ('DIM', 3, 18, 11), ('LUN', 5, 15, 9), ('MAR', 2, 16, 8)]
    for i, (d_, ic_, mx_, mn_) in enumerate(DAYS, 1):
        t.field(f'd{i}', f'Jour {i}', d_)
        t.field(f'ic{i}', f'Ciel {i}', ic_, options=WX_OPTS)
        t.field(f'mx{i}', f'Max {i}', mx_, kind='number', min=-30, max=50)
        t.field(f'mn{i}', f'Min {i}', mn_, kind='number', min=-30, max=50)
    g = t.group('Semaine', 96, 730, 1728, 280, tracks=merge(inout('opacity', 1, 0, 90, 0, 12, 8), inout('y', 730, 780, 90, 0, 24, 8)))
    t.override(g, x=44, y=1400, width=992, height=440)
    pl = t.rect('Verre', 0, 0, 1728, 280, hexc(SKY2, 0.78), g, radius=18, grad=('#0a2a5ecc', 180), stroke=(hexc('#ffffff', 0.25), 1.5))
    t.override(pl, width=1000, height=440)
    vf = t.rect('Ville fond', 0, 0, 260, 280, hexc(SUN), g, corners=(18, 0, 0, 18))
    t.override(vf, width=1000, height=80)
    vc = t.text('Ville', '{{city}}', 0, 40, 260, 120, 36, g, align='center', family=GEO, weight=800, color=hexc(SKY2), wrap=True)
    t.override(vc, x=0, y=0, width=700, height=80, fontSize=34)
    sv = t.text('7 JOURS', '7 JOURS', 0, 160, 260, 50, 22, g, align='center', weight=700, color=hexc(SKY2), spacing=3)
    t.override(sv, x=700, y=0, width=260, height=80)
    for i in range(1, 8):
        x = 260 + (i - 1) * 209
        d = 8 + i * 4
        col = t.group(f'Jour {i}', x, 0, 209, 280, g, tracks=merge(inout('opacity', 1, 0, 90, d, 10, 6), inout('y', 0, 30, 90, d, 18, 6)))
        t.override(col, x=(i - 1) * 142.8, y=100, width=142.8, height=330, tracks=merge(inout('opacity', 1, 0, 90, d, 10, 6), inout('y', 100, 130, 90, d, 18, 6)))
        if i > 1:
            t.rect('Séparateur', 0, 30, 1, 220, hexc('#ffffff', 0.18), col)
        dl = t.text('Jour', '{{d%d}}' % i, 0, 18, 209, 40, 26, col, align='center', weight=800, spacing=2)
        t.override(dl, width=142.8)
        ic = t.group('Icône', 52, 64, 104, 104, col)
        t.override(ic, x=20, y=70)
        wx_choice(t, f'ic{i}', ic, 0, 0, 104)
        mx = t.counter('Max', '{{mx%d}}' % i, 0, 174, 209, 60, 50, col, suffix='°', family=GEO, weight=800, align='center', grouping=False)
        t.override(mx, width=142.8, y=190)
        mn = t.counter('Min', '{{mn%d}}' % i, 0, 230, 209, 36, 28, col, suffix='°', family=GEO, weight=600, color=hexc(COLD), align='center', grouping=False)
        t.override(mn, width=142.8, y=250)
    p.item('Paris 7 jours', 'week', 'meteo')

    # -- Vigilance ----------------------------------------------------------------------------------------
    t = p.template('alert', 'Vigilance', 'top', duration=90, on_air=60)
    t.field('level', 'Niveau', '2', options=[['1', 'Jaune'], ['2', 'Orange'], ['3', 'Rouge']])
    t.field('kind', 'Phénomène', 'ORAGES')
    t.field('where', 'Départements', 'Gironde, Landes, Pyrénées-Atlantiques, Gers et Lot-et-Garonne')
    t.field('until', 'Validité', 'Jusqu’à jeudi 6' + N + 'h')
    g = t.group('Vigilance', 96, 60, 1728, 120, clip=True, tracks=inout('cropRight', 0, 1, 60, 0, 22, 12))
    t.override(g, x=44, y=120, width=992, height=200)
    pl = t.rect('Fond', 0, 0, 1728, 120, hexc('#0b1d3d', 0.92), g, radius=14)
    t.override(pl, width=1000, height=200)
    chips = {}
    for k, (c_, lab, tc) in enumerate((('#ffd400', 'VIGILANCE JAUNE', '#1a1400'), ('#ff8a00', 'VIGILANCE ORANGE', '#1a0d00'), ('#ff2a2a', 'VIGILANCE ROUGE', '#ffffff')), 1):
        ch = t.group(lab.title(), 0, 0, 360, 120, g)
        t.override(ch, width=1000, height=80)
        r = t.rect('Pastille', 0, 0, 360, 120, hexc(c_), ch, corners=(14, 0, 0, 14))
        t.override(r, width=1000, height=80)
        tx = t.text('Niveau', lab, 30, 0, 300, 120, 32, ch, align='center', weight=800, family=GEO, spacing=1, color=hexc(tc), wrap=True, lineHeight=1.0)
        t.override(tx, x=0, width=1000, height=80)
        chips[k] = [ch]
    t.choose('level', chips)
    kd = t.text('Phénomène', '{{kind}}', 392, 10, 1300, 56, 44, g, family=GEO, weight=800, spacing=1)
    t.override(kd, x=30, y=86, width=940, height=50, fontSize=40)
    wh = t.text('Départements', '{{where}} · {{until}}', 392, 64, 1300, 42, 24, g, weight=500, color=hexc('#cfe2ff'))
    t.override(wh, x=30, y=134, width=940, height=56, fontSize=24)
    t.layer_ref(wh).update(wrap=True)
    p.item('Vigilance orange orages', 'alert', 'meteo')

    # -- Présentateur -----------------------------------------------------------------------------------
    t = p.template('presenter', 'Présentateur', 'lower', duration=70, on_air=40)
    t.field('name', 'Nom', 'Lucie Garnier')
    t.field('role', 'Fonction', 'Prévisionniste')
    g = t.group('Synthé', 120, 840, 900, 120, tracks=merge(inout('opacity', 1, 0, 40, 0, 12, 8), inout('x', 120, 80, 40, 0, 22, 8)))
    t.override(g, x=60, y=1560, width=960, height=120)
    pl = t.rect('Verre', 0, 0, 640, 120, hexc(SKY2, 0.82), g, radius=60, grad=('#1d6fd6cc', 90), stroke=(hexc('#ffffff', 0.3), 1.5))
    wx_icon(t, 1, g, 6, 6, 108, 'Soleil')
    nm = t.text('Nom', '{{name}}', 130, 12, 760, 56, 42, g, family=GEO, weight=800)
    t.text('Fonction', 'La météo · {{role}}', 132, 66, 760, 36, 24, g, weight=500, color=hexc('#cfe2ff'))
    t.fit(nm, [(pl, 'width')], pad=190, minimum=520)
    p.item('Lucie Garnier', 'presenter', 'meteo')
    return p


def sign_switch(t, key, up_ids, down_ids):
    """Layers shown when the number `key` is ≥ 0, others when it is < 0 (logic)."""
    k = len(t.t.get('logic', {}).get('nodes', []))
    n = lambda x: f'g{k}{x}'
    nodes = [{'id': n('f'), 'type': 'field', 'key': key, 'x': 40, 'y': 60}, {'id': n('z'), 'type': 'constant', 'value': 0, 'x': 40, 'y': 180},
             {'id': n('1'), 'type': 'constant', 'value': 1, 'x': 40, 'y': 300}, {'id': n('0'), 'type': 'constant', 'value': 0, 'x': 40, 'y': 420},
             {'id': n('u'), 'type': 'condition', 'cmp': 'ge', 'x': 300, 'y': 100}, {'id': n('d'), 'type': 'condition', 'cmp': 'lt', 'x': 300, 'y': 300}]
    edges = []
    for c in ('u', 'd'):
        edges += [{'from': n('f'), 'to': n(c), 'port': 'a'}, {'from': n('z'), 'to': n(c), 'port': 'b'},
                  {'from': n('1'), 'to': n(c), 'port': 'then'}, {'from': n('0'), 'to': n(c), 'port': 'else'}]
    for c, ids in (('u', up_ids), ('d', down_ids)):
        for j, lid in enumerate(ids):
            nodes.append({'id': n(f'{c}{j}'), 'type': 'target', 'layer': lid, 'prop': 'opacity', 'mode': 'multiply', 'x': 560, 'y': 60 + j * 60})
            edges.append({'from': n(c), 'to': n(f'{c}{j}'), 'port': 'in'})
    t.logic(nodes, edges)


# ======================================================================
# 06 · Bourse et marchés
# ======================================================================
def ex06():
    BG, PANEL, LINE, TXT, DIM, UP, DOWN, AMBER = '#07090d', '#10141b', '#252c38', '#e8edf3', '#8a94a6', '#19d47c', '#ff4d5e', '#ffb020'
    p = Package(
        'ex-06-bourse', 'Bourse et marchés', 'Économie',
        ['Ticker des indices', 'Graphique', 'Fiche valeur', 'Tableau', 'Devises'],
        "Un habillage d'économie au look terminal : ticker des indices avec flèches de hausse et de baisse, alimenté par une source de données (ou saisi par l'opérateur), graphique de séance (données collées en CSV), fiche valeur dont la couleur suit le signe de la variation, tableau des marchés (négatifs en rouge) et devises en rotation. Noir, vert et rouge de marché, JetBrains Mono et Space Grotesk.",
        en=('Stock markets', 'A terminal-style business package: index ticker with up / down arrows chosen by the operator, intraday chart (data pasted as CSV), stock quote card whose colour follows the sign of the change, market table (negatives in red) and rotating currencies.'),
        cover='chart')
    p.zone('ticker', 'Ticker', 40, '#f5a524', (0, 0.905, 1, 0.07))
    p.zone('lower', 'Bas d’écran', 20, '#7c5cff', (0.04, 0.62, 0.66, 0.29))
    p.zone('corner', 'Coin', 30, '#22c55e', (0.72, 0.05, 0.24, 0.16))
    p.zone('full', 'Plein écran', 50, '#94a3b8')
    p.playlist('bourse', 'Point bourse')

    # -- Ticker des indices -------------------------------------------------------------------------
    t = p.template('ticker', 'Ticker des indices', 'ticker', duration=60, on_air=30)
    Q = [('CAC 40   7 512,30   +0,84 %', 'u'), ('DAX   18 120,55   −0,62 %', 'd'), ('FTSE 100   8 204,10   +0,31 %', 'u'), ('EURO STOXX 50   4 982,77   +0,45 %', 'u'),
         ('DOW JONES   39 412,08   −0,18 %', 'd'), ('NASDAQ   16 870,40   +1,12 %', 'u'), ('NIKKEI   38 640,95   −0,95 %', 'd'), ('BRENT   82,14 $   +1,40 %', 'u')]
    p.source('bourse-indices', 'Bourse · indices', {'indices': [{'ligne': q_, 'sens': '▲' if d_ == 'u' else '▼'} for q_, d_ in Q]})
    for i, (q_, d_) in enumerate(Q, 1):
        t.field(f'q{i}', f'Valeur {i}', q_)
        t.t['fields'][-1]['binding'] = {'source': 'bourse-indices', 'path': f'indices[{i - 1}].ligne'}
        t.field(f'd{i}', f'Sens {i}', '▲' if d_ == 'u' else '▼', options=[['▲', 'Hausse'], ['▼', 'Baisse']])
        t.t['fields'][-1]['binding'] = {'source': 'bourse-indices', 'path': f'indices[{i - 1}].sens'}
    g = t.group('Ticker', 0, 990, 1920, 56, tracks=inout('y', 990, 1080, 30, 0, 18, 12))
    t.rect('Barre', 0, 0, 1920, 56, hexc(BG, 0.96), g)
    t.rect('Filet', 0, 0, 1920, 2, hexc(AMBER), g)
    t.rect('Étiquette', 0, 2, 280, 54, hexc(PANEL), g)
    t.text('MARCHÉS', 'MARCHÉS', 96, 2, 184, 54, 24, g, family=GROTESK, weight=700, color=hexc(AMBER), spacing=2)
    tk = t.ticker('Indices', '', 300, 2, 1620, 54, 26, g, color=hexc(TXT), family=MONO, weight=500, speed=110, sepType='none', spacing=56.0, fade=True, fadeW=50.0,
                  items=[{'cat': '▲' if d_ == 'u' else '▼', 'text': '{{q%d}}' % i, 'on': True} for i, (_, d_) in enumerate(Q, 1)],
                  cats=[{'name': '▲', 'bg': UP, 'color': BG, 'text': ''}, {'name': '▼', 'bg': DOWN, 'color': '#ffffff', 'text': ''}])
    t.layer_ref(tk)['settingBindings'] = {f'items.{i - 1}.cat': f'd{i}' for i in range(1, len(Q) + 1)}
    p.item('Indices', 'ticker', 'bourse')

    # -- Graphique de séance ---------------------------------------------------------------------------
    t = p.template('chart', 'Graphique de séance', 'full', duration=150, on_air=110)
    t.field('index', 'Indice', 'CAC 40')
    t.field('value', 'Clôture', '7 512,30')
    t.field('change', 'Variation (%)', '0.84', kind='number', min=-20, max=20)
    t.field('date', 'Séance', 'Séance du mardi 22 octobre')
    t.field('high', 'Plus haut', '7 538,12')
    t.field('low', 'Plus bas', '7 441,85')
    t.field('volume', 'Volume', '3,4 Md€')
    t.field('csv', 'Données (heure, valeur)', 'Heure,CAC 40\n09:00,7449\n09:30,7461\n10:00,7458\n10:30,7476\n11:00,7470\n11:30,7489\n12:00,7495\n12:30,7486\n13:00,7493\n13:30,7502\n14:00,7498\n14:30,7517\n15:00,7509\n15:30,7524\n16:00,7531\n16:30,7519\n17:00,7514\n17:30,7512', kind='multiline')
    bg = t.group('Fond', 0, 0, 1920, 1080, tracks=fade(110, 0, 12, 12, 10))
    t.rect('Noir', 0, 0, 1920, 1080, hexc(BG), bg, grad=('radial', '#030405'))
    for i in range(1, 12):
        t.rect(f'Grille {i}', i * 160, 0, 1, 1080, hexc('#ffffff', 0.025), bg)
    hd = t.group('Entête', 120, 90, 1680, 170, tracks=merge(inout('opacity', 1, 0, 110, 6, 12, 8), inout('y', 90, 60, 110, 6, 22, 8)))
    t.rect('Ambre', 0, 6, 6, 104, hexc(AMBER), hd)
    t.text('Indice', '{{index}}', 30, 0, 700, 80, 64, hd, family=GROTESK, weight=700, color=hexc(TXT))
    t.text('Séance', '{{date}}', 32, 78, 700, 40, 26, hd, family=GROTESK, weight=500, color=hexc(DIM))
    t.text('Clôture', '{{value}}', 760, 0, 600, 110, 92, hd, family=MONO, weight=700, color=hexc(TXT), align='right')
    up = t.group('Hausse', 1380, 22, 300, 70, hd)
    t.rect('Fond', 0, 0, 300, 70, hexc(UP, 0.16), up, radius=6)
    t.counter('Variation', '{{change}}', 0, 0, 300, 70, 40, up, decimals=2, prefix='▲ +', suffix=N + '%', family=MONO, weight=700, color=hexc(UP), align='center', duration=1.0)
    dn = t.group('Baisse', 1380, 22, 300, 70, hd)
    t.rect('Fond', 0, 0, 300, 70, hexc(DOWN, 0.16), dn, radius=6)
    t.counter('Variation', '{{change}}', 0, 0, 300, 70, 40, dn, decimals=2, prefix='▼ ', suffix=N + '%', family=MONO, weight=700, color=hexc(DOWN), align='center', duration=1.0)
    sign_switch(t, 'change', [up], [dn])
    t.rect('Règle', 120, 280, 1680, 1, hexc(LINE), None, tracks=inout('cropRight', 0, 1, 110, 10, 24, 8))
    t.widget('chart', 'Courbe', 100, 300, 1340, 660, None, {'fill': hexc(UP)}, chartType='lines', source='csv', csv='{{csv}}', smooth=True, areaOpacity=0.22,
             axis={'auto': True, 'ticks': 5, 'decimals': 0, 'prefix': '', 'suffix': '', 'grid': True, 'vgrid': False, 'zero': False, 'xTitle': '', 'yTitle': ''},
             series=[{'color': UP, 'width': 5.0}], labels={'value': False, 'pct': False, 'name': False}, title={'text': '', 'sub': ''}, legend='none',
             panel={'on': False, 'color': BG, 'opacity': 0.0}, font=GROTESK, textColor=DIM, tracks=inout('opacity', 1, 0, 110, 10, 10, 8))
    side = t.group('Repères', 1500, 330, 300, 600, tracks=merge(inout('opacity', 1, 0, 110, 20, 12, 8), inout('x', 1500, 1540, 110, 20, 22, 8)))
    for k, (lab, key) in enumerate((('PLUS HAUT', 'high'), ('PLUS BAS', 'low'), ('VOLUME', 'volume'))):
        t.rect('Séparateur', 0, k * 150, 300, 1, hexc(LINE), side)
        t.text(lab, lab, 0, k * 150 + 20, 300, 30, 20, side, family=GROTESK, weight=700, color=hexc(DIM), spacing=2.5)
        t.text(lab.title(), '{{%s}}' % key, 0, k * 150 + 54, 300, 60, 46, side, family=MONO, weight=500, color=hexc(TXT))
    p.item('CAC 40 · séance', 'chart', 'bourse')

    # -- Fiche valeur -------------------------------------------------------------------------------------
    t = p.template('quote', 'Fiche valeur', 'lower', duration=90, on_air=60)
    t.field('symbol', 'Code', 'AIR')
    t.field('name', 'Société', 'Airbus')
    t.field('sector', 'Secteur', 'Aéronautique · CAC 40')
    t.field('price', 'Cours (€)', '142,10')
    t.field('change', 'Variation (%)', '2.35', kind='number', min=-50, max=50)
    t.field('low', 'Plus bas', '138,40')
    t.field('high', 'Plus haut', '143,05')
    t.field('pos', 'Position dans la séance (%)', '80', kind='number', min=0, max=100)
    g = t.group('Fiche', 96, 700, 1180, 270, clip=True, tracks=inout('cropRight', 0, 1, 60, 0, 22, 12))
    t.rect('Fond', 0, 0, 1180, 270, hexc(BG, 0.94), g, radius=6, stroke=(hexc(LINE), 1.5))
    t.rect('Code fond', 36, 36, 120, 56, hexc(AMBER), g, radius=4)
    t.text('Code', '{{symbol}}', 36, 36, 120, 56, 30, g, align='center', family=MONO, weight=700, color=hexc(BG))
    t.text('Société', '{{name}}', 180, 26, 560, 50, 42, g, family=GROTESK, weight=700, color=hexc(TXT), tracks=inout('opacity', 1, 0, 60, 10, 12, 6))
    t.text('Secteur', '{{sector}}', 182, 74, 560, 30, 22, g, family=GROTESK, weight=500, color=hexc(DIM), tracks=inout('opacity', 1, 0, 60, 14, 12, 6))
    t.text('Cours', '{{price}} €', 720, 18, 420, 90, 72, g, family=MONO, weight=700, color=hexc(TXT), align='right', tracks=inout('opacity', 1, 0, 60, 12, 12, 6))
    t.rect('Séparateur', 36, 130, 1108, 1, hexc(LINE), g)
    up = t.group('Hausse', 36, 156, 300, 80, g)
    t.rect('Fond', 0, 0, 300, 80, hexc(UP, 0.16), up, radius=6)
    t.counter('Variation', '{{change}}', 0, 0, 300, 80, 40, up, decimals=2, prefix='▲ +', suffix=N + '%', family=MONO, weight=700, color=hexc(UP), align='center', duration=0.8)
    dn = t.group('Baisse', 36, 156, 300, 80, g)
    t.rect('Fond', 0, 0, 300, 80, hexc(DOWN, 0.16), dn, radius=6)
    t.counter('Variation', '{{change}}', 0, 0, 300, 80, 40, dn, decimals=2, prefix='▼ ', suffix=N + '%', family=MONO, weight=700, color=hexc(DOWN), align='center', duration=0.8)
    t.text('Séance', 'SÉANCE', 372, 152, 300, 30, 18, g, family=GROTESK, weight=700, color=hexc(DIM), spacing=2.5)
    t.text('Bas', '{{low}}', 372, 196, 200, 40, 26, g, family=MONO, weight=500, color=hexc(TXT))
    t.text('Haut', '{{high}}', 944, 196, 200, 40, 26, g, family=MONO, weight=500, color=hexc(TXT), align='right')
    t.rect('Plage', 580, 213, 350, 6, hexc(LINE), g, radius=3)
    pb = t.progress('Position', '{{pos}}', 580, 213, 350, 6, hexc(UP), g, track='#00000000', rounded=True, duration=0.8)
    pd = t.progress('Position', '{{pos}}', 580, 213, 350, 6, hexc(DOWN), g, track='#00000000', rounded=True, duration=0.8)
    sign_switch(t, 'change', [up, pb], [dn, pd])
    p.item('Airbus', 'quote', 'bourse')
    p.item('Kering', 'quote', 'bourse', {'symbol': 'KER', 'name': 'Kering', 'sector': 'Luxe · CAC 40', 'price': '231,60', 'change': '-3.12', 'low': '229,10', 'high': '240,85', 'pos': '15'})

    # -- Tableau des marchés --------------------------------------------------------------------------------
    t = p.template('board', 'Tableau des marchés', 'full', duration=150, on_air=110)
    t.field('title', 'Titre', 'Les marchés à la clôture')
    MK = [('CAC 40', 'Paris', '7 512,30', '+0,84 %'), ('DAX', 'Francfort', '18 120,55', '−0,62 %'), ('FTSE 100', 'Londres', '8 204,10', '+0,31 %'),
          ('EURO STOXX 50', 'Zone euro', '4 982,77', '+0,45 %'), ('DOW JONES', 'New York', '39 412,08', '−0,18 %'), ('NASDAQ', 'New York', '16 870,40', '+1,12 %'),
          ('NIKKEI 225', 'Tokyo', '38 640,95', '−0,95 %'), ('HANG SENG', 'Hong Kong', '17 902,33', '+0,27 %')]
    for i, (a, b, c, d) in enumerate(MK, 1):
        t.field(f'v{i}', f'{a} · cours', c)
        t.field(f'c{i}', f'{a} · variation', d)
        t.field(f's{i}', f'{a} · sens', UP if d.startswith('+') else DOWN, options=[[UP, 'Hausse'], [DOWN, 'Baisse'], [TXT, 'Stable']])
    bg = t.group('Fond', 0, 0, 1920, 1080, tracks=fade(110, 0, 12, 12, 10))
    t.rect('Noir', 0, 0, 1920, 1080, hexc(BG), bg, grad=('radial', '#030405'))
    t.rect('Ambre', 260, 120, 6, 70, hexc(AMBER), None, tracks=inout('cropBottom', 0, 1, 110, 4, 12, 8))
    t.text('Titre', '{{title}}', 290, 110, 1300, 90, 60, None, family=GROTESK, weight=700, color=hexc(TXT), tracks=inout('opacity', 1, 0, 110, 6, 14, 8))
    grid = [['Indice', 'Place', 'Cours', 'Var. %', 'Sens']] + [[a, b, '{{v%d}}' % i, '{{c%d}}' % i, '{{s%d}}' % i] for i, (a, b, _, _) in enumerate(MK, 1)]
    t.widget('table', 'Tableau', 260, 230, 1400, 760, None, {}, grid=grid, headerRow=True, showRank=False,
             cols=[{'src': 'Indice', 'label': 'INDICE', 'type': 'text', 'align': 'left', 'w': 34.0, 'on': True},
                   {'src': 'Place', 'label': 'PLACE', 'type': 'text', 'align': 'left', 'w': 22.0, 'on': True},
                   {'src': 'Cours', 'label': 'COURS', 'type': 'text', 'align': 'right', 'w': 24.0, 'on': True},
                   {'src': 'Var. %', 'label': 'VAR.', 'type': 'text', 'align': 'right', 'w': 20.0, 'on': True, 'colorFrom': 'Sens'}],
             widthMode='percent', rowH=76.0, perPage=8, pad=24.0, gap=4.0, radius=4.0,
             st={'header': {'bg': '#00000000', 'color': DIM}, 'hcol': {'bg': PANEL, 'color': TXT}, 'odd': {'bg': PANEL, 'color': TXT}, 'even': {'bg': '#0c1016', 'color': TXT},
                 'hl': {'bg': AMBER, 'color': BG}, 'top3': {'bg': PANEL, 'color': TXT}, 'tableBg': '#00000000'},
             cf={'neg': True, 'negColor': DOWN, 'top3': False, 'topN': 3, 'topBar': False, 'hlBar': False, 'hlBarColor': AMBER},
             txt={'title': {'family': GROTESK, 'weight': 700, 'size': 30.0}, 'header': {'family': GROTESK, 'weight': 700, 'size': 20.0, 'spacing': 2.0},
                  'rows': {'family': MONO, 'weight': 500, 'size': 32.0}},
             title={'on': False, 'text': ''},
             tracks=merge(inout('opacity', 1, 0, 110, 10, 14, 8), inout('y', 230, 260, 110, 10, 24, 8)))
    p.item('Clôture des marchés', 'board', 'bourse')

    # -- Devises -------------------------------------------------------------------------------------------------
    t = p.template('fx', 'Devises', 'corner', duration=60, on_air=30)
    t.field('items', 'Devises', 'EUR/USD   1,0842   ▲\nEUR/GBP   0,8517   ▼\nEUR/CHF   0,9621   ▲\nEUR/JPY   162,38   ▲\nBITCOIN   61 240 $   ▼', kind='multiline')
    g = t.group('Devises', 1404, 64, 420, 110, tracks=merge(inout('opacity', 1, 0, 30, 0, 10, 8), inout('x', 1404, 1450, 30, 0, 20, 8)))
    t.rect('Fond', 0, 0, 420, 110, hexc(BG, 0.94), g, radius=6, stroke=(hexc(LINE), 1.5))
    t.text('DEVISES', 'DEVISES', 24, 10, 300, 30, 18, g, family=GROTESK, weight=700, color=hexc(AMBER), spacing=3)
    t.ticker('Cours', '{{items}}', 24, 42, 372, 56, 32, g, color=hexc(TXT), family=MONO, weight=700, mode='flip', itemDur=3.0, transDur=0.5, trans='flip', sepType='none')
    p.item('Devises', 'fx', 'bourse')
    return p


# ======================================================================
# 07 · Talk-show
# ======================================================================
def ex07():
    AUB, AUB2, CORAL, CREAM, GOLD, ROSE = '#1a0f24', '#2b1839', '#ff5a4e', '#fff3e3', '#f2b84b', '#ffb4a8'
    p = Package(
        'ex-07-talk-show', 'Talk-show', 'Divertissement',
        ['Invités', 'Thème', 'Réseaux sociaux', 'Logo', 'Citation', 'Vote QR'],
        "L'habillage chaleureux d'une émission de plateau : synthés des invités dont le cartouche suit le nom, bandeau du thème, comptes sociaux, logo de l'émission, citation plein écran, vote par QR code et « À suivre ». Aubergine, corail et crème, Playfair Display et Montserrat. Chaque graphique a sa mise en page 9:16 pour les extraits sur les réseaux.",
        en=('Talk show', 'A warm studio show package: guest lower thirds whose plate follows the name, topic banner, social handles, show logo, full-screen quote, QR code vote and "Coming up". Every graphic also has a 9:16 layout for social clips.'),
        cover='guest')
    p.vertical()
    p.zone('lower', 'Synthé', 20, '#7c5cff', (0.05, 0.68, 0.65, 0.26), vertical=(0.05, 0.66, 0.9, 0.2))
    p.zone('topic', 'Thème', 25, '#22c55e', (0.05, 0.05, 0.62, 0.11), vertical=(0.05, 0.12, 0.9, 0.11))
    p.zone('bug', 'Logo', 60, '#38bdf8', (0.79, 0.05, 0.16, 0.11), vertical=(0.6, 0.035, 0.35, 0.07))
    p.zone('corner', 'Coin', 30, '#e879f9', (0.70, 0.38, 0.26, 0.56), vertical=(0.1, 0.28, 0.8, 0.36))
    p.zone('full', 'Plein écran', 50, '#94a3b8')
    p.playlist('plateau', 'Plateau')
    p.playlist('social', 'Réseaux')

    # -- Invité --------------------------------------------------------------------------------------------
    t = p.template('guest', 'Invité', 'lower', duration=80, on_air=50)
    t.field('name', 'Nom', 'Léa Fontaine')
    t.field('role', 'Présentation', 'Romancière, prix Goncourt des lycéens')
    g = t.group('Invité', 120, 800, 1100, 160)
    t.override(g, x=60, y=1300, width=960, height=300)
    pl = t.rect('Plaque', 0, 0, 600, 112, hexc(AUB, 0.92), g, radius=56, grad=('#2b1839e6', 90), tracks=inout('cropRight', 0, 1, 50, 0, 22, 12, 4))
    t.override(pl, height=112)
    dot = t.ellipse('Point', 38, 44, 24, 24, hexc(CORAL), g, tracks=merge(inout('scaleX', 1, 0, 50, 8, 14, 8, 0, 'back'), inout('scaleY', 1, 0, 50, 8, 14, 8, 0, 'back')))
    nm = t.text('Nom', '{{name}}', 84, 4, 1000, 104, 58, g, color=hexc(CREAM), family=SERIF, weight=700, italic=True,
                tracks=merge(inout('opacity', 1, 0, 50, 10, 14, 8), inout('x', 84, 60, 50, 10, 22, 8)))
    t.override(nm, width=860)
    t.fit(nm, [(pl, 'width')], pad=128, minimum=420)
    rl = t.text('Présentation', '{{role}}', 86, 120, 1000, 36, 22, g, color=hexc(ROSE), family=GEO, weight=600, spacing=2.5, upper=True, shadow=True, shadowColor='#1a0f24cc', shadowBlur=10.0, shadowY=2.0,
                tracks=inout('opacity', 1, 0, 50, 18, 14, 8))
    t.override(rl, width=860, height=70)
    t.layer_ref(rl).update(wrap=True)
    p.item('Léa Fontaine', 'guest', 'plateau')
    p.item('Karim Haddad', 'guest', 'plateau', {'name': 'Karim Haddad', 'role': 'Chef étoilé, Le Comptoir des Halles'})
    p.item('Margaux Petit', 'guest', 'plateau', {'name': 'Margaux Petit', 'role': 'Humoriste, en tournée « Tout va bien »'})

    # -- Thème -------------------------------------------------------------------------------------------------
    t = p.template('topic', 'Thème', 'topic', duration=70, on_air=40)
    t.field('label', 'Étiquette', 'LE DÉBAT')
    t.field('topic', 'Thème', 'Faut-il interdire les écrans avant 6 ans ?')
    g = t.group('Thème', 110, 66, 1080, 92, clip=True, tracks=inout('cropRight', 0, 1, 40, 0, 22, 12))
    t.override(g, x=60, y=240, width=960, height=200)
    lb = t.rect('Étiquette fond', 0, 0, 200, 92, hexc(CORAL), g, corners=(46, 0, 0, 46))
    t.override(lb, width=960, height=60)
    t.layer_ref(lb)['overrides']['vertical']['props'].update({'cornerTl': 30.0, 'cornerTr': 30.0, 'cornerBl': 0.0, 'cornerBr': 0.0})
    lt_ = t.text('Étiquette', '{{label}}', 30, 0, 170, 92, 22, g, color=hexc(AUB), family=GEO, weight=800, spacing=2.5, align='center')
    t.override(lt_, x=0, width=960, height=60)
    pl = t.rect('Thème fond', 200, 0, 880, 92, hexc(AUB, 0.92), g, corners=(0, 46, 46, 0))
    t.override(pl, x=0, y=60, width=960, height=140)
    t.layer_ref(pl)['overrides']['vertical']['props'].update({'cornerTl': 0.0, 'cornerTr': 0.0, 'cornerBl': 30.0, 'cornerBr': 30.0})
    tp = t.text('Thème', '{{topic}}', 232, 0, 820, 92, 38, g, color=hexc(CREAM), family=SERIF, weight=700, tracks=inout('opacity', 1, 0, 40, 12, 12, 8))
    t.override(tp, x=40, y=60, width=880, height=140, fontSize=44)
    t.layer_ref(tp).update(wrap=True, lineHeight=1.1)
    p.item('Débat écrans', 'topic', 'plateau')

    # -- Réseaux sociaux ---------------------------------------------------------------------------------------
    t = p.template('social', 'Réseaux sociaux', 'lower', duration=90, on_air=60)
    S = [('IG', '@legrandsalon'), ('TT', '@legrandsalon.tv'), ('X', '#LeGrandSalon')]
    for i, (a, b) in enumerate(S, 1):
        t.field(f'p{i}', f'Réseau {i}', a, options=[['IG', 'Instagram'], ['TT', 'TikTok'], ['X', 'X'], ['YT', 'YouTube'], ['FB', 'Facebook']])
        t.field(f'h{i}', f'Compte {i}', b)
    for i in range(1, 4):
        d = (i - 1) * 6
        row = t.group(f'Compte {i}', 120, 760 + (i - 1) * 74, 700, 64, tracks=merge(inout('opacity', 1, 0, 60, d, 12, 8, (3 - i) * 3), inout('x', 120, 80, 60, d, 22, 8, (3 - i) * 3)))
        t.override(row, x=60, y=1290 + (i - 1) * 84, width=960, height=72, tracks=merge(inout('opacity', 1, 0, 60, d, 12, 8, (3 - i) * 3), inout('x', 60, 20, 60, d, 22, 8, (3 - i) * 3)))
        pl = t.rect('Pilule', 0, 0, 460, 64, hexc(AUB, 0.92), row, radius=32)
        t.ellipse('Pastille', 6, 6, 52, 52, hexc(CORAL), row)
        t.text('Réseau', '{{p%d}}' % i, 6, 6, 52, 52, 20, row, align='center', color=hexc(AUB), family=GEO, weight=800)
        hd = t.text('Compte', '{{h%d}}' % i, 76, 0, 620, 64, 30, row, color=hexc(CREAM), family=GEO, weight=600)
        t.fit(hd, [(pl, 'width')], pad=104, minimum=260)
    p.item('Nos réseaux', 'social', 'social')

    # -- Logo ----------------------------------------------------------------------------------------------------
    t = p.template('bug', 'Logo de l’émission', 'bug', duration=60, on_air=30)
    g = t.group('Logo', 1530, 62, 290, 100, tracks=fade(30, 0, 14))
    t.override(g, x=680, y=72, width=340, height=110)
    t.text('LE GRAND', 'LE GRAND', 0, 0, 290, 30, 20, g, align='right', family=GEO, weight=600, spacing=6, color=hexc(CREAM), shadow=True, shadowBlur=10.0)
    t.text('Salon', 'Salon', 0, 18, 262, 78, 64, g, align='right', family=SERIF, weight=700, italic=True, color=hexc(CREAM), shadow=True, shadowBlur=12.0)
    t.ellipse('Point', 266, 66, 16, 16, hexc(CORAL), g)
    p.item('Logo', 'bug', 'plateau')

    # -- Citation -------------------------------------------------------------------------------------------------
    t = p.template('quote', 'Citation', 'full', duration=150, on_air=110)
    t.field('quote', 'Citation', 'On n’écrit pas pour être lu, on écrit pour ne plus être seul avec ce que l’on a vu.', kind='multiline')
    t.field('author', 'Auteur', 'Léa Fontaine')
    t.field('source', 'Source', 'Dans « Les Heures claires », 2026')
    bg = t.group('Fond', 0, 0, 1920, 1080, tracks=fade(110, 0, 14, 14, 10))
    t.override(bg, width=1080, height=1920)
    r = t.rect('Aubergine', 0, 0, 1920, 1080, hexc(AUB2), bg, grad=('radial', AUB))
    t.override(r, width=1080, height=1920)
    e = glow(t, 'Lueur', 1200, -300, 1000, CORAL, 0.3, bg)
    t.override(e, x=300, y=-300)
    q = t.group('Citation', 260, 200, 1400, 700, tracks=merge(inout('opacity', 1, 0, 110, 10, 16, 10), inout('y', 200, 240, 110, 10, 30, 10)))
    t.override(q, x=80, y=420, width=920, height=1100, tracks=merge(inout('opacity', 1, 0, 110, 10, 16, 10), inout('y', 420, 460, 110, 10, 30, 10)))
    t.text('Guillemet', '“', -20, -80, 300, 300, 320, q, color=hexc(CORAL), family=SERIF, weight=700, valign='top')
    qt = t.text('Citation', '{{quote}}', 0, 120, 1400, 380, 66, q, color=hexc(CREAM), family=SERIF, weight=600, italic=True, wrap=True, valign='top', lineHeight=1.22)
    t.override(qt, width=920, height=700, fontSize=62)
    rl = t.rect('Filet', 0, 540, 80, 4, hexc(CORAL), q, tracks=inout('cropRight', 0, 1, 110, 30, 16, 8))
    t.override(rl, y=850)
    au = t.text('Auteur', '{{author}}', 0, 566, 1200, 50, 34, q, color=hexc(CREAM), family=GEO, weight=800, spacing=1)
    t.override(au, y=876, width=920)
    so = t.text('Source', '{{source}}', 0, 616, 1200, 40, 24, q, color=hexc(ROSE), family=GEO, weight=600)
    t.override(so, y=926, width=920)
    p.item('Citation Léa Fontaine', 'quote', 'plateau')

    # -- Vote QR ---------------------------------------------------------------------------------------------------
    t = p.template('vote', 'Vote par QR code', 'corner', duration=90, on_air=60)
    t.field('question', 'Question', 'Pour ou contre les écrans avant 6 ans ?')
    t.field('url', 'Lien', 'https://example.com/vote')
    t.field('cta', 'Appel', 'Votez maintenant')
    g = t.group('Vote', 1380, 420, 440, 560, tracks=merge(inout('opacity', 1, 0, 60, 0, 12, 8), inout('y', 420, 460, 60, 0, 24, 8)))
    t.override(g, x=230, y=600, width=620, height=600, tracks=merge(inout('opacity', 1, 0, 60, 0, 12, 8), inout('y', 600, 640, 60, 0, 24, 8)))
    c = t.rect('Carte', 0, 0, 440, 560, hexc(CREAM), g, radius=24, shadow=(hexc('#000000', 0.35), 0, 14, 40))
    t.override(c, width=620, height=600)
    h = t.rect('Entête', 0, 0, 440, 70, hexc(CORAL), g, corners=(24, 24, 0, 0))
    t.override(h, width=620)
    ct = t.text('Appel', '{{cta}}', 0, 0, 440, 70, 26, g, align='center', color=hexc(AUB), family=GEO, weight=800, spacing=1, upper=True)
    t.override(ct, width=620)
    qr = t.qr('QR', '{{url}}', 90, 96, 260, g, fill=hexc(AUB), background=CREAM)
    t.override(qr, x=170, y=96, width=280, height=280)
    qq = t.text('Question', '{{question}}', 30, 370, 380, 170, 32, g, align='center', color=hexc(AUB), family=SERIF, weight=700, wrap=True, lineHeight=1.15)
    t.override(qq, x=40, y=390, width=540, height=190, fontSize=40)
    p.item('Vote écrans', 'vote', 'plateau')

    # -- À suivre ------------------------------------------------------------------------------------------------------
    t = p.template('next', 'À suivre', 'corner', duration=80, on_air=50)
    t.field('label', 'Étiquette', 'À SUIVRE')
    t.field('title', 'Séquence', 'Le live musical de Pomme')
    t.field('time', 'Heure', '22:40')
    g = t.group('À suivre', 1380, 820, 440, 150, clip=True, tracks=inout('cropRight', 0, 1, 50, 0, 20, 10))
    t.override(g, x=130, y=900, width=820, height=180)
    c = t.rect('Carte', 0, 0, 440, 150, hexc(AUB, 0.94), g, radius=20)
    t.override(c, width=820, height=180)
    t.text('Étiquette', '{{label}}', 30, 18, 260, 30, 20, g, color=hexc(CORAL), family=GEO, weight=800, spacing=3)
    tm = t.text('Heure', '{{time}}', 280, 18, 130, 30, 22, g, color=hexc(GOLD), family=GEO, weight=700, align='right')
    t.override(tm, x=660)
    tt = t.text('Séquence', '{{title}}', 30, 52, 390, 84, 34, g, color=hexc(CREAM), family=SERIF, weight=700, wrap=True, valign='top', lineHeight=1.1)
    t.override(tt, width=760, height=110, fontSize=44)
    p.item('À suivre : Pomme', 'next', 'plateau')
    return p



# ======================================================================
# 08 · Esport / stream gaming
# ======================================================================
def ex08():
    BG, PANEL, VIO, CYAN, TXT, DIM = '#07060c', '#120f1f', '#7b5cff', '#22e4ff', '#f4f2ff', '#9a95bd'
    p = Package(
        'ex-08-esport-stream', 'Esport et stream', 'Gaming',
        ['Scoreboard', 'Caméras joueurs', 'Partenaires', 'Écran d’attente', 'Pause', 'Alertes'],
        "Un tournoi diffusé sur YouTube ou Twitch : bandeau de match (équipes, cartes gagnées, carte en cours), cadres des caméras joueurs, barre des partenaires en rotation, écrans « Le stream commence » et « On revient vite » avec compte à rebours, alertes d'abonnement et de don. Noir profond, violet et cyan électriques, Bebas Neue et Space Grotesk.",
        en=('Esports & streaming', 'A tournament streamed on YouTube or Twitch: match header (teams, maps won, current map), player camera frames, rotating sponsor bar, "Starting soon" and "Be right back" screens with countdown, follow and donation alerts.'),
        cover='soon')
    p.zone('top', 'Bandeau de match', 30, '#22e4ff', (0.25, 0.02, 0.5, 0.12))
    p.zone('cam', 'Caméra', 20, '#7c5cff', (0.02, 0.6, 0.32, 0.38))
    p.zone('sponsor', 'Partenaires', 25, '#f5a524', (0.70, 0.88, 0.28, 0.1))
    p.zone('alert', 'Alertes', 40, '#ff3b5c', (0.70, 0.04, 0.28, 0.2))
    p.zone('full', 'Plein écran', 50, '#94a3b8')
    p.playlist('avant', 'Avant le match')
    p.playlist('match', 'Match')
    p.playlist('alertes', 'Alertes')

    def neon_bg(t, on_air):
        bg = t.group('Fond', 0, 0, 1920, 1080, tracks=fade(on_air, 0, 14, 14, 10))
        t.rect('Noir', 0, 0, 1920, 1080, hexc(BG), bg)
        glow(t, 'Halo violet', -300, -400, 1300, VIO, 0.6, bg,
                  tracks={'x': keys((0, -300, SMOOTH), (300, -150, SMOOTH), (600, -300, SMOOTH))})
        glow(t, 'Halo cyan', 1100, 300, 1200, CYAN, 0.4, bg,
                  tracks={'y': keys((0, 300, SMOOTH), (300, 180, SMOOTH), (600, 300, SMOOTH))})
        for i in range(13):
            t.rect(f'Grille v{i}', 80 + i * 150, 0, 1, 1080, hexc('#ffffff', 0.04), bg)
        for i in range(8):
            t.rect(f'Grille h{i}', 0, 60 + i * 140, 1920, 1, hexc('#ffffff', 0.04), bg)
        t.rect('Cadre', 40, 40, 1840, 1000, CLEAR, bg, stroke=(hexc('#ffffff', 0.12), 2), radius=18)
        return bg

    # -- Bandeau de match -----------------------------------------------------------------------------
    t = p.template('match', 'Bandeau de match', 'top', duration=70, on_air=40)
    for k, v, lab in (('ta', 'NEXUS', 'Équipe A'), ('tb', 'VOLTA', 'Équipe B'), ('ia', 'NX', 'Sigle A'), ('ib', 'VT', 'Sigle B')):
        t.field(k, lab, v)
    t.field('ma', 'Cartes A', '1', kind='number', min=0, max=5)
    t.field('mb', 'Cartes B', '1', kind='number', min=0, max=5)
    t.field('map', 'Carte en cours', 'CARTE 3 · ASCENT')
    t.field('bo', 'Format', 'BO3')
    t.field('ca', 'Couleur A', VIO, kind='color')
    t.field('cb', 'Couleur B', CYAN, kind='color')
    g = t.group('Bandeau', 500, 30, 920, 116, tracks=merge(inout('y', 30, -130, 40, 0, 20, 12), inout('opacity', 1, 0, 40, 0, 10, 10)))
    t.rect('Fond', 0, 0, 920, 80, hexc(PANEL, 0.95), g, radius=10, stroke=(hexc('#ffffff', 0.1), 1.5))
    t.rect('A', 0, 0, 360, 80, hexc(VIO), g, shape='parallelogram', skew=-30, corners=None, bindings={'fill': 'ca'}, grad=('#120f1f', 90, [(70, '#7b5cff')]))
    t.rect('B', 560, 0, 360, 80, hexc(CYAN), g, shape='parallelogram', skew=30, bindings={'fill': 'cb'}, grad=('#120f1f', 270, [(70, '#22e4ff')]))
    t.rect('Sigle A fond', 14, 14, 52, 52, hexc('#ffffff', 0.15), g, radius=8)
    t.text('Sigle A', '{{ia}}', 14, 14, 52, 52, 26, g, align='center', family=GROTESK, weight=700)
    t.text('Équipe A', '{{ta}}', 80, 4, 260, 80, 52, g, family=DISPLAY, weight=400, spacing=2)
    t.rect('Sigle B fond', 854, 14, 52, 52, hexc('#ffffff', 0.15), g, radius=8)
    t.text('Sigle B', '{{ib}}', 854, 14, 52, 52, 26, g, align='center', family=GROTESK, weight=700)
    t.text('Équipe B', '{{tb}}', 580, 4, 260, 80, 52, g, family=DISPLAY, weight=400, spacing=2, align='right')
    t.text('Score', '{{ma}}  {{mb}}', 360, 2, 200, 80, 64, g, align='center', family=DISPLAY, weight=400)
    t.rect('Séparateur', 456, 30, 8, 8, hexc(DIM), g, radius=4)
    t.rect('Séparateur 2', 456, 44, 8, 8, hexc(DIM), g, radius=4)
    t.rect('Carte fond', 260, 80, 400, 36, hexc(BG, 0.95), g, corners=(0, 0, 10, 10))
    t.text('Carte', '{{map}}  ·  {{bo}}', 260, 80, 400, 36, 19, g, align='center', family=GROTESK, weight=700, spacing=2, color=hexc(DIM), upper=True)
    p.item('NEXUS vs VOLTA', 'match', 'match')

    # -- Caméra joueur --------------------------------------------------------------------------------------
    t = p.template('cam', 'Caméra joueur', 'cam', duration=70, on_air=40)
    t.field('player', 'Joueur', 'Kairo')
    t.field('real', 'Nom', 'Lucas Martin')
    t.field('team', 'Sigle équipe', 'NX')
    t.field('role', 'Rôle', 'DUELLISTE')
    t.field('color', 'Couleur', VIO, kind='color')
    g = t.group('Caméra', 60, 664, 560, 380, tracks=merge(inout('opacity', 1, 0, 40, 0, 12, 8), inout('scaleX', 1, 0.94, 40, 0, 18, 8), inout('scaleY', 1, 0.94, 40, 0, 18, 8)))
    t.rect('Cadre', 0, 0, 560, 315, CLEAR, g, radius=12, stroke=(hexc('#ffffff', 0.25), 2))
    L, Wd = 46, 6
    for nm_, x, y, w, h in (('Coin 1', 0, 0, L, Wd), ('Coin 2', 0, 0, Wd, L), ('Coin 3', 560 - L, 0, L, Wd), ('Coin 4', 560 - Wd, 0, Wd, L),
                            ('Coin 5', 0, 315 - Wd, L, Wd), ('Coin 6', 0, 315 - L, Wd, L), ('Coin 7', 560 - L, 315 - Wd, L, Wd), ('Coin 8', 560 - Wd, 315 - L, Wd, L)):
        t.rect(nm_, x, y, w, h, hexc(VIO), g, bindings={'fill': 'color'})
    t.rect('Plaque', 24, 290, 512, 76, hexc(PANEL, 0.96), g, radius=10, stroke=(hexc('#ffffff', 0.1), 1.5))
    t.rect('Sigle fond', 36, 302, 52, 52, hexc(VIO), g, radius=8, bindings={'fill': 'color'})
    t.text('Sigle', '{{team}}', 36, 302, 52, 52, 24, g, align='center', family=GROTESK, weight=700)
    t.text('Joueur', '{{player}}', 104, 296, 300, 44, 40, g, family=DISPLAY, weight=400, spacing=1.5)
    t.text('Nom', '{{real}}', 104, 334, 300, 26, 18, g, family=GROTESK, weight=500, color=hexc(DIM))
    t.text('Rôle', '{{role}}', 360, 302, 160, 52, 18, g, align='right', family=GROTESK, weight=700, color=hexc(CYAN), spacing=2)
    p.item('Cam Kairo', 'cam', 'match')
    p.item('Cam Vexa', 'cam', 'match', {'player': 'Vexa', 'real': 'Inès Morel', 'team': 'VT', 'role': 'CONTRÔLEUR', 'color': CYAN})

    # -- Partenaires ---------------------------------------------------------------------------------------------
    t = p.template('sponsors', 'Partenaires', 'sponsor', duration=60, on_air=30)
    t.field('list', 'Partenaires', 'HYPERION GEAR\nVOLT ENERGY\nPIXELHOST\nNOVA CHAIRS', kind='multiline')
    g = t.group('Partenaires', 1360, 960, 470, 72, tracks=merge(inout('opacity', 1, 0, 30, 0, 10, 8), inout('y', 960, 1000, 30, 0, 20, 8)))
    t.rect('Fond', 0, 0, 470, 72, hexc(PANEL, 0.92), g, radius=36, stroke=(hexc('#ffffff', 0.12), 1.5))
    t.text('Avec', 'AVEC', 28, 0, 90, 72, 16, g, family=GROTESK, weight=700, color=hexc(DIM), spacing=3)
    t.rect('Trait', 104, 22, 2, 28, hexc(VIO), g)
    t.ticker('Noms', '{{list}}', 124, 0, 320, 72, 34, g, color=hexc(TXT), family=DISPLAY, weight=400, mode='flip', itemDur=4.0, transDur=0.5, trans='slide', sepType='none', spacing=0.0)
    p.item('Partenaires', 'sponsors', 'match')

    # -- Le stream commence ------------------------------------------------------------------------------------------
    t = p.template('soon', 'Le stream commence', 'full', duration=150, on_air=110)
    t.field('title', 'Titre', 'LE STREAM COMMENCE')
    t.field('event', 'Événement', 'NEXUS ARENA · SAISON 4 · DEMI-FINALES')
    for i, (h_, m_) in enumerate((('18:00', 'NEXUS vs VOLTA'), ('19:30', 'ORBIT vs KITSUNE'), ('21:00', 'GRANDE FINALE')), 1):
        t.field(f'h{i}', f'Heure {i}', h_)
        t.field(f'm{i}', f'Match {i}', m_)
    neon_bg(t, 110)
    t.text('Événement', '{{event}}', 160, 220, 1100, 40, 26, None, family=GROTESK, weight=700, color=hexc(CYAN), spacing=4, tracks=inout('opacity', 1, 0, 110, 10, 14, 8))
    t.text('Titre', '{{title}}', 150, 260, 1200, 330, 170, None, family=DISPLAY, weight=400, wrap=True, lineHeight=0.9, valign='top', spacing=2,
           tracks=merge(inout('opacity', 1, 0, 110, 14, 16, 8), inout('x', 150, 110, 110, 14, 26, 8)))
    cd = t.group('Compte à rebours', 160, 640, 520, 150, tracks=inout('opacity', 1, 0, 110, 24, 14, 8))
    t.rect('Fond', 0, 0, 520, 150, hexc(PANEL, 0.85), cd, radius=16, stroke=(hexc(VIO, 0.6), 2))
    t.text('Dans', 'DÉBUT DANS', 30, 18, 300, 30, 20, cd, family=GROTESK, weight=700, color=hexc(DIM), spacing=3)
    t.clock('Minuteur', 30, 46, 460, 96, 96, cd, mode='countdown', fmt='mm:ss', family=DISPLAY, weight=400, align='left', cdDuration=300000, startMode='auto', atZero='freeze')
    pr = t.group('Programme', 1300, 300, 480, 460, tracks=merge(inout('opacity', 1, 0, 110, 30, 14, 8), inout('x', 1300, 1360, 110, 30, 24, 8)))
    t.text('AU PROGRAMME', 'AU PROGRAMME', 0, 0, 480, 40, 22, pr, family=GROTESK, weight=700, color=hexc(VIO), spacing=4)
    for i in range(1, 4):
        y = 60 + (i - 1) * 130
        t.rect('Ligne', 0, y, 480, 110, hexc(PANEL, 0.8), pr, radius=12, stroke=(hexc('#ffffff', 0.08), 1.5))
        t.text('Heure', '{{h%d}}' % i, 24, y, 140, 110, 44, pr, family=DISPLAY, weight=400, color=hexc(CYAN))
        t.text('Match', '{{m%d}}' % i, 160, y, 300, 110, 30, pr, family=GROTESK, weight=700)
    p.item('Le stream commence', 'soon', 'avant')

    # -- On revient vite --------------------------------------------------------------------------------------------------
    t = p.template('brb', 'On revient vite', 'full', duration=130, on_air=90)
    t.field('title', 'Titre', 'ON REVIENT VITE')
    t.field('sub', 'Message', 'Pause technique · suivez-nous sur @nexusarena')
    neon_bg(t, 90)
    t.rect('Barre', 760, 380, 400, 6, hexc(VIO), None, grad=(CYAN, 90), tracks=inout('scaleX', 1, 0, 90, 8, 24, 10))
    t.text('Titre', '{{title}}', 160, 400, 1600, 200, 190, None, align='center', family=DISPLAY, weight=400, spacing=4,
           tracks=merge(inout('opacity', 1, 0, 90, 12, 14, 8), inout('scaleX', 1, 0.9, 90, 12, 26, 8), inout('scaleY', 1, 0.9, 90, 12, 26, 8)))
    t.text('Message', '{{sub}}', 160, 610, 1600, 50, 32, None, align='center', family=GROTESK, weight=500, color=hexc(DIM), tracks=inout('opacity', 1, 0, 90, 20, 14, 8))
    t.clock('Minuteur', 760, 700, 400, 90, 72, None, mode='countdown', fmt='mm:ss', color=hexc(CYAN), family=DISPLAY, weight=400, cdDuration=180000, startMode='auto', tracks=inout('opacity', 1, 0, 90, 26, 14, 8))
    p.item('Pause', 'brb', 'match')

    # -- Alertes -----------------------------------------------------------------------------------------------------
    t = p.template('alert', 'Alerte abonné', 'alert', duration=150, on_air=120)
    t.field('kind', 'Type', 'NOUVEL ABONNÉ', options=[['NOUVEL ABONNÉ', 'Abonné'], ['NOUVEAU MEMBRE', 'Membre'], ['MERCI POUR LE DON', 'Don'], ['RAID EN COURS', 'Raid']])
    t.field('name', 'Pseudo', 'PixelWarden')
    t.field('msg', 'Message', 'Allez NEXUS, on y croit !')
    g = t.group('Alerte', 1360, 60, 480, 180, tracks=merge(inout('x', 1360, 1940, 120, 0, 22, 14), inout('opacity', 1, 0, 120, 0, 8, 10)))
    t.rect('Lueur', 0, 0, 480, 180, hexc(VIO), g, radius=18, grad=(CYAN, 120), shadow=(hexc(VIO, 0.6), 0, 0, 40))
    t.rect('Fond', 4, 4, 472, 172, hexc(PANEL, 0.97), g, radius=15)
    t.text('Type', '{{kind}}', 30, 22, 420, 30, 20, g, family=GROTESK, weight=700, color=hexc(CYAN), spacing=3)
    t.text('Pseudo', '{{name}}', 30, 52, 420, 76, 66, g, family=DISPLAY, weight=400, spacing=1.5,
           tracks=merge(inout('scaleX', 1, 0.7, 120, 8, 16, 8, 0, 'back'), inout('scaleY', 1, 0.7, 120, 8, 16, 8, 0, 'back')))
    t.text('Message', '{{msg}}', 30, 126, 420, 36, 22, g, family=GROTESK, weight=500, color=hexc(DIM), tracks=inout('opacity', 1, 0, 120, 16, 12, 8))
    p.item('Nouvel abonné', 'alert', 'alertes')
    p.item('Don', 'alert', 'alertes', {'kind': 'MERCI POUR LE DON', 'name': 'Moka_42', 'msg': '20 € · GG les gars !'})
    return p


# ======================================================================
# 09 · Formule 1 / sport mécanique
# ======================================================================
def ex09():
    BG, PANEL, RED, TXT, DIM, PURPLE, GREEN, YEL = '#0b0c10', '#16181e', '#ff1e2d', '#ffffff', '#9aa0ab', '#a43cf5', '#16d17a', '#ffd400'
    DRIVERS = [('LAU', 'Vortex Racing', '#3671c6', 'LEADER'), ('MER', 'Rossa Corse', '#e8002d', '+1.204'), ('KOV', 'Apex GP', '#ff8000', '+3.877'),
               ('DAL', 'Vortex Racing', '#3671c6', '+5.120'), ('FER', 'Northstar', '#27f4d2', '+9.442'), ('NAK', 'Rossa Corse', '#e8002d', '+11.031'),
               ('BRE', 'Apex GP', '#ff8000', '+14.598'), ('OKA', 'Lumen', '#229971', '+18.220'), ('SIL', 'Kestrel', '#0093cc', '+21.765'),
               ('HAR', 'Northstar', '#27f4d2', '+24.010'), ('VDB', 'Halcyon', '#64c4ff', '+27.348'), ('MOR', 'Lumen', '#229971', '+29.901'),
               ('PET', 'Torque', '#b6babd', '+33.452'), ('ROS', 'Kestrel', '#0093cc', '+35.008'), ('ALM', 'Meridian', '#52e252', '+38.773'),
               ('WEI', 'Halcyon', '#64c4ff', '+41.660'), ('COS', 'Torque', '#b6babd', '+44.105'), ('JAN', 'Falco', '#ffffff', '+47.892'),
               ('LIN', 'Meridian', '#52e252', '1 TOUR'), ('ADE', 'Falco', '#ffffff', 'ABANDON')]
    p = Package(
        'ex-09-grand-prix', 'Grand Prix automobile', 'Sport',
        ['Classement en piste', 'Tours', 'Pilote', 'Duel', 'Meilleur tour', 'Drapeaux', 'Arrêt au stand'],
        "La course comme à la télévision : tour de classement à gauche (20 pilotes, couleurs d'écurie, écarts) alimenté par un tableau que l'opérateur colle ou qu'une source met à jour, compteur de tours, carte pilote, duel avec écart, meilleur tour en violet, drapeaux et safety car, arrêt au stand avec pneus. Noir carbone et rouge, Titillium Web.",
        en=('Motor racing', 'The race as on TV: timing tower (20 drivers, team colours, gaps) fed by a table the operator pastes or a source updates, lap counter, driver card, battle with interval, purple fastest lap, flags and safety car, pit stop with tyres.'),
        cover='tower')
    p.zone('tower', 'Classement', 30, '#ff3b5c', (0.025, 0.04, 0.19, 0.92))
    p.zone('lower', 'Bas d’écran', 20, '#7c5cff', (0.25, 0.70, 0.71, 0.26))
    p.zone('top', 'Direction de course', 35, '#ffd400', (0.30, 0.04, 0.66, 0.11))
    p.zone('full', 'Plein écran', 50, '#94a3b8')
    p.playlist('course', 'Course')
    p.playlist('direction', 'Direction de course')

    # -- Tour de classement -------------------------------------------------------------------------------------
    t = p.template('tower', 'Classement en piste', 'tower', duration=140, on_air=110)
    t.field('gp', 'Grand Prix', 'GP DE FRANCE')
    t.field('lap', 'Tour', '34', kind='number', min=0, max=99)
    t.field('laps', 'Tours au total', '53', kind='number', min=1, max=99)
    t.field('timing', 'Classement (code, écart, couleur)', json.dumps([['Code', 'Écart', 'Couleur']] + [[c, g_, col] for c, _, col, g_ in DRIVERS], ensure_ascii=False), kind='table')
    g = t.group('Tour', 64, 60, 330, 966, clip=True, tracks=inout('cropBottom', 0, 1, 110, 0, 26, 14))
    t.rect('Fond', 0, 0, 330, 966, hexc(BG, 0.9), g, radius=8)
    t.rect('Entête', 0, 0, 330, 92, hexc(RED), g, corners=(8, 8, 0, 0), grad=('#c40018', 90))
    t.text('GP', '{{gp}}', 20, 10, 290, 30, 20, g, family=TECH, weight=700, spacing=2.5)
    t.text('TOUR', 'TOUR', 20, 42, 100, 40, 22, g, family=TECH, weight=600, color=hexc('#ffffffcc'))
    t.text('Tours', '{{lap}}', 96, 36, 120, 50, 44, g, family=TECH, weight=900)
    t.text('Total', '/ {{laps}}', 168, 42, 140, 40, 26, g, family=TECH, weight=600, color=hexc('#ffffffcc'), align='right')
    t.widget('table', 'Classement', 0, 100, 330, 860, g, {}, link='{{timing}}', headerRow=False, showRank=True,
             cols=[{'src': 'Code', 'label': '', 'type': 'text', 'align': 'left', 'w': 48.0, 'on': True},
                   {'src': 'Écart', 'label': '', 'type': 'text', 'align': 'right', 'w': 52.0, 'on': True}],
             widthMode='percent', rowH=41.0, pad=12.0, gap=2.0, radius=3.0, perPage=20, rowFrom='Couleur', rowPaint='bar',
             cascade={'on': True, 'style': 'slide', 'offset': 35.0, 'dur': 300.0},
             st={'header': {'bg': '#00000000', 'color': DIM}, 'hcol': {'bg': PANEL, 'color': TXT}, 'odd': {'bg': PANEL, 'color': TXT}, 'even': {'bg': '#111318', 'color': TXT},
                 'hl': {'bg': TXT, 'color': BG}, 'top3': {'bg': PANEL, 'color': TXT}, 'tableBg': '#00000000'},
             cf={'neg': False, 'negColor': RED, 'top3': False, 'topN': 3, 'topBar': False, 'hlBar': False, 'hlBarColor': RED},
             txt={'title': {'family': TECH, 'weight': 700, 'size': 22.0}, 'header': {'family': TECH, 'weight': 700, 'size': 16.0},
                  'rows': {'family': TECH, 'weight': 700, 'size': 24.0}, 'rank': {'family': TECH, 'weight': 900, 'size': 22.0}},
             title={'on': False, 'text': ''})
    p.item('Classement tour 34', 'tower', 'course')

    # -- Carte pilote -------------------------------------------------------------------------------------------------
    t = p.template('driver', 'Carte pilote', 'lower', duration=80, on_air=50)
    t.field('num', 'Numéro', '16')
    t.field('first', 'Prénom', 'Hugo')
    t.field('last', 'Nom', 'MERCIER')
    t.field('team', 'Écurie', 'Rossa Corse')
    t.field('pos', 'Position', 'P2')
    t.field('nat', 'Nationalité', 'FRA')
    t.field('color', 'Couleur écurie', '#e8002d', kind='color')
    g = t.group('Pilote', 520, 800, 1100, 170, clip=True, tracks=inout('cropRight', 0, 1, 50, 0, 22, 12))
    t.rect('Fond', 0, 0, 1100, 170, hexc(BG, 0.92), g, radius=8)
    t.rect('Couleur', 0, 0, 220, 170, hexc('#e8002d'), g, bindings={'fill': 'color'}, corners=(8, 0, 0, 8))
    t.rect('Diagonale', 170, 0, 120, 170, hexc(BG, 0.92), g, shape='parallelogram', skew=50)
    t.text('Numéro', '{{num}}', 10, 0, 190, 170, 120, g, align='center', family=TECH, weight=900, italic=True, tracks=inout('opacity', 1, 0, 50, 8, 10, 6))
    t.text('Prénom', '{{first}}', 280, 22, 600, 50, 40, g, family=TECH, weight=600, tracks=inout('opacity', 1, 0, 50, 10, 10, 6))
    t.text('Nom', '{{last}}', 280, 62, 600, 66, 62, g, family=TECH, weight=900, upper=True, spacing=1, tracks=inout('opacity', 1, 0, 50, 12, 10, 6))
    t.text('Écurie', '{{team}}  ·  {{nat}}', 282, 124, 600, 34, 24, g, family=TECH, weight=600, color=hexc(DIM), upper=True, spacing=1.5, tracks=inout('opacity', 1, 0, 50, 14, 10, 6))
    t.rect('Position fond', 930, 35, 130, 100, hexc(TXT), g, radius=6)
    t.text('Position', '{{pos}}', 930, 35, 130, 100, 64, g, align='center', family=TECH, weight=900, color=hexc(BG))
    p.item('Mercier P2', 'driver', 'course')
    p.item('Laurent P1', 'driver', 'course', {'num': '1', 'first': 'Théo', 'last': 'LAURENT', 'team': 'Vortex Racing', 'pos': 'P1', 'nat': 'FRA', 'color': '#3671c6'})

    # -- Duel -----------------------------------------------------------------------------------------------------------
    t = p.template('battle', 'Duel', 'lower', duration=80, on_air=50)
    t.field('a', 'Pilote devant', 'MERCIER')
    t.field('b', 'Pilote derrière', 'KOVAČ')
    t.field('ca', 'Couleur devant', '#e8002d', kind='color')
    t.field('cb', 'Couleur derrière', '#ff8000', kind='color')
    t.field('gap', 'Écart', '+0.412')
    t.field('drs', 'DRS', 'true', kind='boolean')
    g = t.group('Duel', 560, 840, 1000, 120, tracks=merge(inout('opacity', 1, 0, 50, 0, 10, 8), inout('y', 840, 880, 50, 0, 20, 8)))
    t.rect('A', 0, 10, 380, 100, hexc(BG, 0.92), g, radius=8)
    t.rect('A couleur', 0, 10, 10, 100, hexc('#e8002d'), g, bindings={'fill': 'ca'}, corners=(8, 0, 0, 8))
    t.text('Pilote A', '{{a}}', 34, 10, 330, 100, 50, g, family=TECH, weight=900, upper=True)
    t.rect('Écart fond', 390, 0, 220, 120, hexc(TXT), g, radius=8)
    t.text('INTERVALLE', 'INTERVALLE', 390, 12, 220, 26, 16, g, align='center', family=TECH, weight=700, color=hexc(DIM), spacing=2)
    t.text('Écart', '{{gap}}', 390, 32, 220, 80, 56, g, align='center', family=TECH, weight=900, color=hexc(BG))
    t.rect('B', 620, 10, 380, 100, hexc(BG, 0.92), g, radius=8)
    t.rect('B couleur', 990, 10, 10, 100, hexc('#ff8000'), g, bindings={'fill': 'cb'}, corners=(0, 8, 8, 0))
    t.text('Pilote B', '{{b}}', 640, 10, 330, 100, 50, g, family=TECH, weight=900, upper=True, align='right')
    dr = t.group('DRS', 650, 20, 90, 30, g, bindings={'opacity': 'drs'})
    t.rect('DRS fond', 0, 0, 90, 30, hexc(GREEN), dr, radius=4)
    t.text('DRS', 'DRS', 0, 0, 90, 30, 20, dr, align='center', family=TECH, weight=900, color=hexc(BG), spacing=2)
    p.item('Mercier / Kovač', 'battle', 'course')

    # -- Meilleur tour --------------------------------------------------------------------------------------------------
    t = p.template('fastest', 'Meilleur tour', 'lower', duration=90, on_air=60)
    t.field('driver', 'Pilote', 'KOVAČ')
    t.field('team', 'Écurie', 'Apex GP')
    t.field('time', 'Temps', '1:32.418')
    t.field('lap', 'Tour', 'TOUR 31')
    g = t.group('Meilleur tour', 560, 830, 1000, 130, clip=True, tracks=inout('cropRight', 0, 1, 60, 0, 22, 12))
    t.rect('Violet', 0, 0, 1000, 130, hexc(PURPLE), g, radius=8, grad=('#6d1fb0', 90))
    t.text('TOUR LE PLUS RAPIDE', 'TOUR LE PLUS RAPIDE', 36, 14, 500, 34, 22, g, family=TECH, weight=700, spacing=3, color=hexc('#ffffffd0'))
    t.text('Pilote', '{{driver}}', 36, 44, 500, 70, 58, g, family=TECH, weight=900, upper=True)
    t.text('Écurie', '{{team}} · {{lap}}', 560, 14, 400, 34, 20, g, family=TECH, weight=600, align='right', color=hexc('#ffffffd0'), upper=True, spacing=1.5)
    t.text('Temps', '{{time}}', 560, 40, 404, 80, 66, g, family=MONO, weight=700, align='right')
    p.item('Meilleur tour Kovač', 'fastest', 'course')

    # -- Drapeaux -------------------------------------------------------------------------------------------------------
    t = p.template('flag', 'Drapeaux et safety car', 'top', duration=90, on_air=60)
    t.field('kind', 'Situation', '2', options=[['1', 'Drapeau jaune'], ['2', 'Safety car'], ['3', 'VSC'], ['4', 'Drapeau rouge'], ['5', 'Drapeau vert']])
    t.field('msg', 'Message', 'Incident au virage 7 · voiture arrêtée')
    g = t.group('Direction', 600, 50, 1224, 100, clip=True, tracks=inout('cropRight', 0, 1, 60, 0, 20, 12))
    t.rect('Fond', 0, 0, 1224, 100, hexc(BG, 0.94), g, radius=8)
    chips = {}
    for k, (c_, lab, tc) in enumerate(((YEL, 'DRAPEAU JAUNE', BG), (YEL, 'SAFETY CAR', BG), (YEL, 'VIRTUAL SAFETY CAR', BG), (RED, 'DRAPEAU ROUGE', TXT), (GREEN, 'PISTE LIBRE', BG)), 1):
        ch = t.group(lab.title(), 0, 0, 420, 100, g)
        t.rect('Pastille', 0, 0, 420, 100, hexc(c_), ch, corners=(8, 0, 0, 8), tracks=pulse('opacity', 0.75, 1, 50, 2000))
        t.text('Libellé', lab, 0, 0, 420, 100, 40 if len(lab) < 16 else 32, ch, align='center', family=TECH, weight=900, color=hexc(tc), spacing=1.5)
        chips[k] = [ch]
    t.choose('kind', chips)
    t.text('Message', '{{msg}}', 450, 0, 750, 100, 32, g, family=TECH, weight=600)
    p.item('Safety car', 'flag', 'direction')
    p.item('Drapeau rouge', 'flag', 'direction', {'kind': '4', 'msg': 'Course suspendue · retour aux stands'})

    # -- Arrêt au stand -------------------------------------------------------------------------------------------------
    t = p.template('pit', 'Arrêt au stand', 'lower', duration=90, on_air=60)
    t.field('driver', 'Pilote', 'LAURENT')
    t.field('color', 'Couleur écurie', '#3671c6', kind='color')
    t.field('time', 'Immobilisation', '2.4 s')
    t.field('from', 'Pneus quittés', '2', options=[['1', 'Tendres'], ['2', 'Médiums'], ['3', 'Durs'], ['4', 'Intermédiaires'], ['5', 'Pluie']])
    t.field('to', 'Pneus chaussés', '3', options=[['1', 'Tendres'], ['2', 'Médiums'], ['3', 'Durs'], ['4', 'Intermédiaires'], ['5', 'Pluie']])
    g = t.group('Stand', 560, 830, 1000, 130, clip=True, tracks=inout('cropRight', 0, 1, 60, 0, 22, 12))
    t.rect('Fond', 0, 0, 1000, 130, hexc(BG, 0.92), g, radius=8)
    t.rect('Couleur', 0, 0, 10, 130, hexc('#3671c6'), g, bindings={'fill': 'color'}, corners=(8, 0, 0, 8))
    t.text('ARRÊT AU STAND', 'ARRÊT AU STAND', 36, 14, 400, 34, 20, g, family=TECH, weight=700, spacing=3, color=hexc(DIM))
    t.text('Pilote', '{{driver}}', 36, 44, 360, 70, 54, g, family=TECH, weight=900, upper=True)
    TY = [('S', '#ff2d3a'), ('M', '#ffd400'), ('H', '#f2f2f2'), ('I', '#43b02a'), ('W', '#0067ad')]

    def tyre(key, x):
        variants = {}
        for k, (l_, c_) in enumerate(TY, 1):
            tg = t.group(f'Pneu {l_}', x, 25, 80, 80, g)
            t.ellipse('Gomme', 0, 0, 80, 80, hexc('#1c1d22'), tg, stroke=(hexc(c_), 7))
            t.text('Lettre', l_, 0, 0, 80, 80, 34, tg, align='center', family=TECH, weight=900, color=hexc(c_))
            variants[k] = [tg]
        t.choose(key, variants)
    tyre('from', 440)
    t.tri('Flèche', 540, 50, 30, 30, hexc(DIM), g, dir='right')
    tyre('to', 590)
    t.rect('Temps fond', 730, 20, 240, 90, hexc(TXT), g, radius=6)
    t.text('Temps', '{{time}}', 730, 20, 240, 90, 54, g, align='center', family=MONO, weight=700, color=hexc(BG))
    p.item('Stand Laurent', 'pit', 'course')
    return p


# ======================================================================
# 10 · Tennis
# ======================================================================
def ex10():
    FOREST, FOREST2, CLAY, CREAM, BALL, DIM = '#0f3b2e', '#0a2b21', '#c8582b', '#f6efe2', '#dff24a', '#a9c2b8'
    p = Package(
        'ex-10-tennis', 'Tennis', 'Sport',
        ['Tableau de score', 'Fiche joueur', 'Statistiques', 'Balle de break', 'Face-à-face', 'Résultat'],
        "Un match de tennis en direct : tableau de score (jeux, sets, points 15-30-40, avantage et tie-break comptés par le widget, balle de service), fiche joueur, statistiques du match à barres partagées, alertes balle de break, de set et de match, face-à-face avant le match et résultat final. Vert gazon profond, terre battue et jaune balle, Montserrat.",
        en=('Tennis', 'A live tennis match: scoreboard (games, sets, 15-30-40 points, advantage and tie-break counted by the widget, serve ball), player card, split-bar match statistics, break / set / match point alerts, head-to-head before the match and final result.'),
        cover='h2h')
    p.zone('score', 'Score', 30, '#dff24a', (0.035, 0.035, 0.40, 0.21))
    p.zone('alert', 'Alerte', 35, '#ff3b5c', (0.035, 0.195, 0.30, 0.075))
    p.zone('lower', 'Bas d’écran', 20, '#7c5cff', (0.04, 0.66, 0.70, 0.29))
    p.zone('full', 'Plein écran', 50, '#94a3b8')
    p.playlist('avant', 'Avant-match')
    p.playlist('match', 'Match')
    p.playlist('fin', 'Après-match')

    # -- Tableau de score ---------------------------------------------------------------------------------------------
    t = p.template('score', 'Tableau de score', 'score', duration=60, on_air=30)
    t.field('event', 'Tournoi', 'OPEN DE LYON · QUART DE FINALE')
    g = t.group('Score', 70, 46, 560, 220, tracks=inout('cropRight', 0, 1, 30, 0, 20, 12))
    t.rect('Tournoi fond', 0, 0, 560, 34, hexc(CLAY), g, corners=(8, 8, 0, 0))
    t.text('Tournoi', '{{event}}', 16, 0, 540, 34, 17, g, family=GEO, weight=800, spacing=2, color=hexc(CREAM))
    t.widget('scoreboard', 'Score', 0, 34, 560, 186, g, {}, preset='tennis', layout='stacked', version='full', bestOf=3, nameW=8.0, radius=0.0, showCrest=False,
             teams=[{'name': 'Arthur Fils', 'short': 'FIL', 'crest': 'round', 'logo': '', 'c1': FOREST, 'c2': CREAM, 'score': ''},
                    {'name': 'Holger Rune', 'short': 'RUN', 'crest': 'round', 'logo': '', 'c1': FOREST, 'c2': CREAM, 'score': ''}],
             fields={'period': True, 'clock': False, 'fouls': False, 'timeouts': False, 'cards': False, 'poss': True, 'sets': True, 'shot': False, 'shootout': False, 'susp': False},
             bg=FOREST2, font=GEO, anim={'score': 'flip', 'banner': True, 'evDur': 3.2})
    p.item('Fils – Rune', 'score', 'match')

    # -- Fiche joueur ----------------------------------------------------------------------------------------------------
    t = p.template('player', 'Fiche joueur', 'lower', duration=90, on_air=60)
    t.field('first', 'Prénom', 'Arthur')
    t.field('last', 'Nom', 'FILS')
    t.field('nat', 'Pays', 'FRA')
    for k, lab, v in (('rank', 'Classement', '14'), ('age', 'Âge', '22 ans'), ('titles', 'Titres', '4'), ('season', 'Saison', '38 V – 14 D')):
        t.field(k, lab, v)
    g = t.group('Fiche', 96, 790, 1200, 180, clip=True, tracks=inout('cropRight', 0, 1, 60, 0, 24, 12))
    t.rect('Fond', 0, 0, 1200, 180, hexc(FOREST, 0.95), g, radius=10, grad=(FOREST2, 90))
    t.rect('Terre', 0, 0, 12, 180, hexc(CLAY), g, corners=(10, 0, 0, 10))
    t.rect('Pays fond', 44, 30, 86, 40, hexc(CREAM), g, radius=4)
    t.text('Pays', '{{nat}}', 44, 30, 86, 40, 22, g, align='center', family=GEO, weight=800, color=hexc(FOREST))
    t.text('Prénom', '{{first}}', 150, 20, 520, 56, 40, g, family=GEO, weight=600, color=hexc(CREAM))
    t.text('Nom', '{{last}}', 44, 74, 640, 90, 80, g, family=GEO, weight=800, upper=True, spacing=1, color=hexc(CREAM))
    for x, w, lab, txt, size, col in ((600, 140, 'CLASSEMENT', 'N°{{rank}}', 44, BALL), (740, 140, 'ÂGE', '{{age}}', 34, CREAM),
                                      (880, 120, 'TITRES', '{{titles}}', 44, CREAM), (1000, 190, 'SAISON', '{{season}}', 30, CREAM)):
        t.rect('Séparateur', x, 40, 1, 100, hexc('#ffffff', 0.15), g)
        t.text(lab.title(), lab, x + 20, 40, w - 24, 26, 15, g, family=GEO, weight=700, color=hexc(DIM), spacing=2)
        t.text(lab.title() + ' valeur', txt, x + 20, 70, w - 24, 66, size, g, family=GEO, weight=800, color=hexc(col))
    p.item('Arthur Fils', 'player', 'avant')
    p.item('Holger Rune', 'player', 'avant', {'first': 'Holger', 'last': 'RUNE', 'nat': 'DEN', 'rank': '11', 'age': '23 ans', 'titles': '5', 'season': '41 V – 15 D'})

    # -- Statistiques -----------------------------------------------------------------------------------------------------
    t = p.template('stats', 'Statistiques du match', 'full', duration=150, on_air=110)
    t.field('title', 'Titre', 'STATISTIQUES DU MATCH')
    t.field('a', 'Joueur A', 'A. FILS')
    t.field('b', 'Joueur B', 'H. RUNE')
    STATS = [('Aces', 9, 6), ('Doubles fautes', 2, 4), ('1re balle (%)', 64, 58), ('Points gagnés sur 1re balle (%)', 78, 71), ('Coups gagnants', 31, 24),
             ('Fautes directes', 18, 27), ('Balles de break converties', 3, 1)]
    for i, (lab, a_, b_) in enumerate(STATS, 1):
        t.field(f'l{i}', f'Statistique {i}', lab)
        t.field(f'h{i}', f'{lab} · A', a_, kind='number', min=0)
        t.field(f'v{i}', f'{lab} · B', b_, kind='number', min=0)
    bg = t.group('Fond', 0, 0, 1920, 1080, tracks=fade(110, 0, 12, 12, 10))
    t.rect('Vert', 0, 0, 1920, 1080, hexc(FOREST), bg, grad=('radial', FOREST2))
    for i in range(8):
        t.rect(f'Bande {i}', i * 240, 0, 120, 1080, hexc('#ffffff', 0.02), bg)
    pn = t.group('Panneau', 360, 110, 1200, 860, tracks=merge(inout('opacity', 1, 0, 110, 4, 12, 10, 6), inout('y', 110, 150, 110, 4, 24, 10, 6)))
    t.text('Titre', '{{title}}', 0, 0, 1200, 40, 22, pn, align='center', family=GEO, weight=800, spacing=4, color=hexc(BALL))
    t.text('Joueur A', '{{a}}', 0, 40, 600, 90, 64, pn, family=GEO, weight=800, color=hexc(CREAM))
    t.text('Joueur B', '{{b}}', 600, 40, 600, 90, 64, pn, family=GEO, weight=800, align='right', color=hexc(CREAM))
    t.rect('Règle', 0, 140, 1200, 2, hexc(CLAY), pn)
    for i in range(1, 8):
        y = 170 + (i - 1) * 96
        d = 14 + i * 5
        row = t.group(f'Ligne {i}', 0, y, 1200, 90, pn, tracks=merge(inout('opacity', 1, 0, 110, d, 12, 6, 7 - i), inout('x', 0, 40, 110, d, 20, 6, 7 - i)))
        t.text('A', '{{h%d}}' % i, 0, 0, 160, 56, 46, row, family=GEO, weight=800, color=hexc(CREAM))
        t.text('Libellé', '{{l%d}}' % i, 160, 0, 880, 56, 24, row, align='center', family=GEO, weight=600, color=hexc(DIM), upper=True, spacing=1.5)
        t.text('B', '{{v%d}}' % i, 1040, 0, 160, 56, 46, row, family=GEO, weight=800, color=hexc(CREAM), align='right')
        t.rect('Piste B', 0, 64, 1200, 10, hexc('#ffffff', 0.22), row, radius=5, tracks=inout('cropLeft', 0, 1, 110, d + 6, 24, 6))
        hb = t.rect('Piste A', 0, 64, 600, 10, hexc(BALL), row, radius=5, tracks=inout('cropRight', 0, 1, 110, d + 6, 24, 6))
        split_bar(t, f'h{i}', f'v{i}', hb, 1200)
    p.item('Statistiques', 'stats', 'match')

    # -- Alerte balle de break --------------------------------------------------------------------------------------------
    t = p.template('alert', 'Balle de break / set / match', 'alert', duration=80, on_air=50)
    t.field('kind', 'Alerte', 'BALLE DE BREAK', options=[['BALLE DE BREAK', 'Break'], ['BALLE DE SET', 'Set'], ['BALLE DE MATCH', 'Match'], ['TIE-BREAK', 'Tie-break']])
    t.field('count', 'Nombre', '2 BALLES')
    g = t.group('Alerte', 70, 214, 520, 56, clip=True, tracks=inout('cropBottom', 0, 1, 50, 0, 14, 10))
    t.rect('Jaune', 0, 0, 380, 56, hexc(BALL), g, corners=(0, 0, 0, 8))
    t.ellipse('Balle', 16, 16, 24, 24, hexc(FOREST), g, tracks=pulse('opacity', 0.3, 1, 40, 2000))
    t.text('Alerte', '{{kind}}', 54, 0, 320, 56, 26, g, family=GEO, weight=800, color=hexc(FOREST), spacing=1)
    t.rect('Nombre fond', 380, 0, 140, 56, hexc(FOREST2, 0.95), g, corners=(0, 0, 8, 0))
    t.text('Nombre', '{{count}}', 380, 0, 140, 56, 18, g, align='center', family=GEO, weight=700, color=hexc(CREAM), spacing=1)
    p.item('Balle de break', 'alert', 'match')
    p.item('Balle de match', 'alert', 'match', {'kind': 'BALLE DE MATCH', 'count': '1 BALLE'})

    # -- Face-à-face ----------------------------------------------------------------------------------------------------
    t = p.template('h2h', 'Face-à-face', 'full', duration=150, on_air=110)
    t.field('event', 'Tournoi', 'OPEN DE LYON · QUART DE FINALE')
    t.field('a', 'Joueur A', 'Arthur FILS')
    t.field('b', 'Joueur B', 'Holger RUNE')
    t.field('na', 'Pays A', 'FRA')
    t.field('nb', 'Pays B', 'DEN')
    t.field('h2h', 'Face-à-face', '2 – 1')
    ROWS = [('Classement', '14', '11'), ('Âge', '22', '23'), ('Taille', '1,85 m', '1,88 m'), ('Titres', '4', '5'), ('Saison', '38 – 14', '41 – 15')]
    for i, (lab, a_, b_) in enumerate(ROWS, 1):
        t.field(f'l{i}', f'Ligne {i}', lab)
        t.field(f'a{i}', f'{lab} · A', a_)
        t.field(f'b{i}', f'{lab} · B', b_)
    bg = t.group('Fond', 0, 0, 1920, 1080, tracks=fade(110, 0, 12, 12, 10))
    t.rect('Vert', 0, 0, 1920, 1080, hexc(FOREST), bg, grad=('radial', FOREST2))
    t.rect('Terre', 0, 0, 640, 1080, hexc(CLAY, 0.9), bg, shape='parallelogram', skew=-140, grad=('#8f3a17', 180), tracks=inout('x', -40, -700, 110, 0, 30, 12))
    t.rect('Terre 2', 1320, 0, 640, 1080, hexc(CLAY, 0.9), bg, shape='parallelogram', skew=-140, grad=('#8f3a17', 180), tracks=inout('x', 1320, 1980, 110, 0, 30, 12))
    hd = t.group('Entête', 360, 110, 1200, 300, tracks=merge(inout('opacity', 1, 0, 110, 8, 14, 8), inout('y', 110, 80, 110, 8, 24, 8)))
    t.text('Tournoi', '{{event}}', 0, 0, 1200, 40, 22, hd, align='center', family=GEO, weight=800, spacing=4, color=hexc(BALL))
    t.text('A', '{{a}}', 0, 60, 520, 130, 60, hd, align='right', family=GEO, weight=800, color=hexc(CREAM), wrap=True, lineHeight=1.0)
    t.text('B', '{{b}}', 680, 60, 520, 130, 60, hd, family=GEO, weight=800, color=hexc(CREAM), wrap=True, lineHeight=1.0)
    t.ellipse('VS', 540, 70, 120, 120, hexc(BALL), hd)
    t.text('VS', 'VS', 540, 70, 120, 120, 44, hd, align='center', family=GEO, weight=800, color=hexc(FOREST))
    t.text('Pays A', '{{na}}', 0, 196, 520, 40, 26, hd, align='right', family=GEO, weight=700, color=hexc(DIM), spacing=3)
    t.text('Pays B', '{{nb}}', 680, 196, 520, 40, 26, hd, family=GEO, weight=700, color=hexc(DIM), spacing=3)
    tb = t.group('Comparaison', 460, 440, 1000, 520, tracks=merge(inout('opacity', 1, 0, 110, 18, 14, 8), inout('y', 440, 480, 110, 18, 24, 8)))
    t.rect('Fond', 0, 0, 1000, 520, hexc(FOREST2, 0.9), tb, radius=12)
    for i in range(1, 6):
        y = 20 + (i - 1) * 80
        if i > 1:
            t.rect('Séparateur', 40, y, 920, 1, hexc('#ffffff', 0.1), tb)
        t.text('A', '{{a%d}}' % i, 40, y, 260, 80, 38, tb, family=GEO, weight=800, color=hexc(CREAM))
        t.text('Libellé', '{{l%d}}' % i, 300, y, 400, 80, 22, tb, align='center', family=GEO, weight=600, color=hexc(DIM), upper=True, spacing=2)
        t.text('B', '{{b%d}}' % i, 700, y, 260, 80, 38, tb, align='right', family=GEO, weight=800, color=hexc(CREAM))
    t.rect('H2H fond', 0, 430, 1000, 90, hexc(BALL), tb, corners=(0, 0, 12, 12))
    t.text('H2H', 'FACE-À-FACE  {{h2h}}', 0, 430, 1000, 90, 40, tb, align='center', family=GEO, weight=800, color=hexc(FOREST), spacing=2)
    p.item('Face-à-face Fils – Rune', 'h2h', 'avant')

    # -- Résultat -----------------------------------------------------------------------------------------------------------
    t = p.template('result', 'Résultat', 'lower', duration=100, on_air=70)
    t.field('winner', 'Vainqueur', 'Arthur FILS')
    t.field('loser', 'Battu', 'Holger RUNE')
    t.field('sets', 'Score', '6-4  3-6  7-6 (5)')
    t.field('time', 'Durée', '2 h 41')
    t.field('next', 'Prochain tour', 'En demi-finale contre Jannik Sinner')
    g = t.group('Résultat', 96, 780, 1300, 190, clip=True, tracks=inout('cropRight', 0, 1, 70, 0, 24, 12))
    t.rect('Fond', 0, 0, 1300, 190, hexc(FOREST, 0.95), g, radius=10, grad=(FOREST2, 90))
    t.rect('Jaune', 0, 0, 220, 190, hexc(BALL), g, corners=(10, 0, 0, 10))
    t.text('VICTOIRE', 'VICTOIRE', 0, 0, 220, 190, 32, g, align='center', family=GEO, weight=800, color=hexc(FOREST), spacing=2)
    t.text('Vainqueur', '{{winner}}', 256, 18, 700, 70, 56, g, family=GEO, weight=800, color=hexc(CREAM))
    t.text('Battu', 'bat {{loser}}', 258, 84, 700, 40, 28, g, family=GEO, weight=600, color=hexc(DIM))
    t.text('Prochain', '{{next}}', 258, 132, 700, 40, 24, g, family=GEO, weight=600, color=hexc(BALL))
    t.text('Score', '{{sets}}', 900, 30, 370, 80, 46, g, family=GEO, weight=800, color=hexc(CREAM), align='right')
    t.text('Durée', '{{time}}', 900, 112, 370, 40, 24, g, family=GEO, weight=600, color=hexc(DIM), align='right')
    p.item('Victoire Fils', 'result', 'fin')
    return p


EXAMPLES = [ex01, ex02, ex03, ex04, ex05, ex06, ex07, ex08, ex09, ex10]

if __name__ == '__main__':
    only = sys.argv[1:]
    for f in EXAMPLES:
        if only and not any(f.__name__.endswith(o) for o in only):
            continue
        print(f().write())
