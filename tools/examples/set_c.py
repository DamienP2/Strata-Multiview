#!/usr/bin/env python3
"""Example projects 21- of Strata (see kit.py for the format and the API).

    21 Réseaux sociaux: the messages of the viewers (YouTube, Twitch, Facebook,
       Instagram, X, TikTok, WhatsApp, SMS) moderated in Strata, on air.

The project's social messages are a Push data source of the project itself,
`social-<project id>` (the moderation publishes it): the example writes it
`social-{{project}}`, replaced by the new project's id when a project is made
from it (crates/cg-server/src/examples.rs).

Run: python3 tools/examples/set_c.py
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from kit import *  # noqa: E402,F401,F403

SOCIAL = 'social-{{project}}'

PLATFORM_COLORS = [('youtube', '#ff0033'), ('twitch', '#9146ff'), ('facebook', '#1877f2'), ('instagram', '#e1306c'),
                   ('x', '#000000'), ('tiktok', '#000000'), ('whatsapp', '#25d366'), ('sms', '#64748b')]


def bind(t, key, path, filters=None):
    """The field `key` takes its value from the project's social source."""
    for f in t.t['fields']:
        if f['key'] == key:
            f['binding'] = {'source': SOCIAL, 'path': path}
            if filters:
                f['binding']['filters'] = filters
            return
    raise KeyError(key)


def frame(t, name, x, y, w, h, parent=None, tracks=None, **props):
    return t.group(name, x, y, w, h, parent, tracks=tracks, anchorX=0, anchorY=0, **props)


def compact(v):
    """A JSON value as a field default (no space before a colon: French typography leaves it alone)."""
    return json.dumps(v, ensure_ascii=False, separators=(',', ':'))


def msg(i, platform, pname, name, text, city='', handle='', photo='', at='20:12'):
    return {'id': f'm{i}', 'platform': platform, 'platformName': pname, 'name': name, 'handle': handle or name,
            'city': city, 'text': fr(text), 'avatar': '', 'photo': photo, 'amount': None, 'at': f'2026-10-24T{at}:00+02:00',
            'time': at}


ON_AIR = msg(1, 'facebook', 'Facebook', 'Jean-Marc Ollivier', 'Bravo pour l\'invité, enfin quelqu\'un qui explique clairement.', 'Nantes')
APPROVED = [
    msg(2, 'youtube', 'YouTube', 'Sophie L.', 'Pourquoi la réforme ne concerne pas les indépendants ?', 'Lyon', at='20:13'),
    msg(3, 'x', 'X', '@claire.v', 'Et les retraités agricoles ?', at='20:13'),
    msg(4, 'whatsapp', 'WhatsApp', 'Lecteur', 'Photo des inondations ce soir', 'Quimperlé',
        photo='https://picsum.photos/seed/quimperle/640/360', at='20:14'),
    msg(5, 'twitch', 'Twitch', 'kevin_bzh', 'Super le reportage sur Brest !!', 'Brest', at='20:14'),
    msg(6, 'instagram', 'Instagram', 'Anne', 'Merci pour ce sujet sur les urgences', 'Caen', at='20:15'),
    msg(7, 'tiktok', 'TikTok', '@lea.mtp', 'On veut plus de reportages en région !', 'Montpellier', at='20:15'),
    msg(8, 'sms', 'SMS', 'Michel', 'Et le prix du fioul cet hiver ?', 'Reims', at='20:16'),
]
SERIES = [12, 15, 14, 18, 22, 20, 25, 24, 28, 31, 30, 33, 35, 34, 38, 41, 39, 44, 46, 45, 48, 50, 47, 52, 55, 53, 57, 60,
          58, 62, 61, 59, 63, 66, 64, 62, 65, 68, 66, 63, 61, 64, 60, 58, 62, 59, 57, 60, 56, 58, 61, 57, 55, 59, 56, 58, 60,
          57, 59, 58]


def ex21():
    INK, PANEL, WHT, MUTE, RED = '#0b0f1a', '#0b0f1a', '#ffffff', '#9aa3b5', '#e11d48'
    p = Package('ex-21-reseaux-sociaux', 'Réseaux sociaux', 'Info',
                ['Réseaux sociaux', 'Messages', 'Modération', 'Mot-dièse', 'Bandeau'],
                fr("Les messages des téléspectateurs à l'antenne, venus de YouTube, Twitch, Facebook, Instagram, X, TikTok, "
                   "WhatsApp ou SMS et validés par la modération : carte du message à l'antenne, mur des messages validés "
                   "qui arrivent en douceur, bandeau « VOS MESSAGES » et compteur du mot-dièse avec sa courbe. Tout se lit "
                   "dans la source Social du projet."),
                en=('Social media', 'The viewers\' messages on air, from YouTube, Twitch, Facebook, Instagram, X, TikTok, WhatsApp or '
                    'SMS, approved by the moderation: the card of the message on air, a wall of approved messages sliding in, a '
                    '"YOUR MESSAGES" band and the hashtag counter with its curve. Everything reads the project\'s Social source.'),
                cover='card')
    p.zone('card', 'Carte message', 20, '#1877f2', area=(0.03, 0.03, 0.5, 0.22))
    p.zone('wall', 'Mur', 15, '#9146ff', area=(0.63, 0.04, 0.34, 0.66))
    p.zone('tag', 'Compteur', 25, '#22c55e', area=(0.72, 0.7, 0.26, 0.17))
    p.zone('band', 'Bandeau', 10, '#e11d48', area=(0, 0.88, 1, 0.09))
    p.playlist('jt', fr('JT 20 h · réseaux sociaux'))
    p.guides()

    # --- Message card ---------------------------------------------------
    t = p.template('card', 'Carte message', 'card', duration=100, on_air=50)
    t.field('name', 'Nom', ON_AIR['name'])
    t.field('platform', 'Plateforme', ON_AIR['platform'])
    t.field('city', 'Ville', ON_AIR['city'])
    t.field('text', 'Message', ON_AIR['text'], kind='multiline')
    t.field('avatar', 'Avatar', '', kind='image')
    t.field('edge', 'Couleur de la plateforme', '#1877f2', kind='color')
    for k in ('name', 'platform', 'city', 'text', 'avatar'):
        bind(t, k, 'onAir.' + k)
    bind(t, 'edge', 'onAir.platform', [{'type': 'map', 'when': pid, 'then': c} for pid, c in PLATFORM_COLORS])
    g = frame(t, 'Carte', 100, 70, 860, 140)
    c = t.group('Carte', 0, 0, 860, 140, g, tracks=merge(inout('x', 0, -80, 50, 0, 18), fade(50, 0, 10)))
    t.rect('Fond', 0, 0, 860, 140, hexc(PANEL, 0.94), c, radius=16, shadow=(hexc('#000000', 0.45), 0, 12, 36))
    t.rect('Bord', 0, 0, 8, 140, hexc('#1877f2'), c, corners=(16, 0, 0, 16), bindings={'fill': 'edge'})
    t.widget('socialBadge', 'Avatar', 32, 30, 80, 80, c, show='avatar', avatar='{{avatar}}', author='{{name}}', font=SANS)
    name = t.text('Nom', '{{name}}', 136, 22, 520, 44, 30, c, color=hexc(WHT), weight=700, overflow='ellipsis')
    badge = t.widget('socialBadge', 'Plateforme', 0, 30, 28, 28, c, show='platform', platform='{{platform}}', font=SANS)
    city = t.text('Ville', fr('· {{city}}'), 0, 22, 220, 44, 22, c, color=hexc(MUTE), weight=500)
    t.fit(name, [(badge, 'x')], pad=148)
    t.fit(name, [(city, 'x')], pad=188)
    t.text('Message', '{{text}}', 136, 68, 690, 52, 28, c, color=hexc(WHT), weight=500, overflow='ellipsis')

    # --- Wall -----------------------------------------------------------
    t = p.template('wall', 'Mur', 'wall', duration=100, on_air=50)
    t.field('wall', 'Messages validés', compact(APPROVED[:3]), kind='multiline')
    bind(t, 'wall', 'approved')
    g = frame(t, 'Mur', 1220, 60, 620, 640)
    c = t.group('Cartes', 0, 0, 620, 640, g, tracks=merge(inout('x', 0, 80, 50, 0, 20), fade(50, 0, 12)))
    t.widget('socialWall', 'Messages', 0, 0, 620, 640, c, items='{{wall}}', max=4, gap=12, pad=16, radius=14,
             bg='#f2f3f5', nameSize=20, textSize=20, lines=3, avatarSize=40, photoH=150, anim=0.6, font=SANS)

    # --- Band -----------------------------------------------------------
    t = p.template('band', 'Bandeau messages', 'band', duration=100, on_air=50)
    t.field('msgs', 'Messages validés', compact(APPROVED), kind='multiline')
    bind(t, 'msgs', 'approved')
    g = frame(t, 'Bandeau', 0, 952, 1920, 82)
    c = t.group('Bande', 0, 0, 1920, 82, g, tracks=merge(inout('y', 0, 90, 50, 0, 16), fade(50, 0, 10)))
    t.ticker('Messages', '{{msgs}}', 0, 0, 1920, 82, 30, c, color=hexc(WHT), weight=600, speed=120, itemFormat='{name} · {text}',
             barBg=INK, sepType='symbol', sepSymbol='◆', sepColor='#ffffff80', spacing=40,
             title={'on': True, 'text': 'VOS MESSAGES', 'bg': RED, 'color': WHT, 'w': 250.0})

    # --- Hashtag counter ------------------------------------------------
    t = p.template('tag', 'Compteur #JT20H', 'tag', duration=100, on_air=50)
    t.field('tag', 'Mot-dièse', '#JT20H')
    t.field('count', 'Messages', '3412', kind='number')
    t.field('perMin', 'Par minute', '58', kind='number')
    t.field('series', 'Messages par minute', compact(SERIES), kind='multiline')
    for k in ('tag', 'count', 'perMin', 'series'):
        bind(t, k, 'hashtags.JT20H.' + k)
    g = frame(t, 'Compteur', 1430, 790, 420, 120)
    c = t.group('Pastille', 0, 0, 420, 120, g, tracks=merge(inout('y', 0, 40, 50, 0, 16), fade(50, 0, 10)))
    t.rect('Fond', 0, 0, 420, 120, hexc(PANEL, 0.94), c, radius=18, shadow=(hexc('#000000', 0.4), 0, 6, 20))
    t.text('Mot-dièse', '{{tag}}', 24, 0, 120, 120, 24, c, color=hexc(WHT), weight=800)
    t.counter('Total', '{{count}}', 150, 14, 180, 66, 52, c, color=hexc(WHT), weight=800, duration=1.0)
    t.text('Par minute', '{{perMin}} / min', 152, 80, 170, 26, 18, c, color=hexc(MUTE), weight=600)
    t.widget('chart', 'Courbe', 334, 34, 64, 52, c, chartType='areas', source='csv', csv='{{series}}', spark=True, smooth=True,
             onAir='auto', areaOpacity=0.25, series=[{'color': WHT, 'width': 3.0}], anim={'fx': 'draw', 'dur': 1.2})

    p.item('Message à l\'antenne', 'card', 'jt')
    p.item('Mur des messages', 'wall', 'jt')
    p.item('Bandeau VOS MESSAGES', 'band', 'jt')
    p.item('Compteur #JT20H', 'tag', 'jt')
    p.source(SOCIAL, 'Social', {
        'onAir': ON_AIR, 'next': APPROVED[0], 'approved': APPROVED, 'count': len(APPROVED),
        'hashtags': {'JT20H': {'tag': '#JT20H', 'count': 3412, 'perMin': 58, 'series': SERIES,
                               'platforms': {'x': 61, 'youtube': 24, 'instagram': 9, 'tiktok': 6}}}})
    return p


EXAMPLES = [ex21]

if __name__ == '__main__':
    only = sys.argv[1:]
    for build in EXAMPLES:
        pkg = build()
        if only and not any(o in pkg.meta['id'] for o in only):
            continue
        print(pkg.write())
