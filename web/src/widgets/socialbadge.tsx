// Social badge: a platform's badge (a rounded square in its colour with a simple glyph, not the brand's logo) or a
// person's avatar (their picture, or their initials on a colour from their name), as the social wall draws them.
// Same picture as crates/cg-render/src/wid/socialbadge.rs; settings in crates/cg-core/src/wid/socialbadge.rs.
import { BadgeCheck } from 'lucide-react';
import { interpolateText } from '../engine/evaluate';
import type { Data } from '../engine/types';
import { Content, defineWidget, Row, Section, Seg } from './kit';
import { Pt } from './scoreboard-paint';
import { avatar, badge, FamilyPick } from './socialwall';
import { t } from '../i18n';

export interface Config {
  show?: 'platform' | 'avatar';
  /** youtube, twitch, facebook, instagram, x, tiktok, whatsapp, sms (`{{key}}` allowed). */
  platform?: string;
  /** Avatar: the picture (web address or asset; empty = the initials). */
  avatar?: string;
  /** Avatar: the name its initials and colour come from (`author`: the layer has its own `name`). */
  author?: string;
  /** Font of the initials (and of an unknown platform's letter). */
  font?: string;
}

/** The platforms and their names (brands, not translated). */
const PLATFORMS: [string, string][] = [
  ['youtube', 'YouTube'],
  ['twitch', 'Twitch'],
  ['facebook', 'Facebook'],
  ['instagram', 'Instagram'],
  ['x', 'X'],
  ['tiktok', 'TikTok'],
  ['whatsapp', 'WhatsApp'],
  ['sms', 'SMS'],
];

const cfg = (c: Config) => ({ show: c.show ?? 'platform', platform: c.platform ?? 'youtube', avatar: c.avatar ?? '', author: c.author ?? 'Jean-Marc Ollivier', font: c.font ?? 'Inter' });

function View({ layer, w, h, data }: { layer: Config; w: number; h: number; data: Data }) {
  const c = cfg(layer);
  const s = Math.min(w, h);
  const p = new Pt();
  p.font = c.font;
  if (c.show === 'avatar') avatar(p, interpolateText(c.avatar, data), interpolateText(c.author, data), w / 2, h / 2, s / 2);
  else badge(p, interpolateText(c.platform, data).trim(), '', (w - s) / 2, (h - s) / 2, s);
  return p.svg(w, h);
}

export default defineWidget({
  id: 'socialbadge',
  type: 'socialBadge',
  label: 'Social badge',
  group: 'Data',
  icon: BadgeCheck,
  create: () => ({ show: 'platform', platform: 'youtube', props: { width: 48, height: 48 } }),
  ownsLook: true,
  content: ['platform', 'avatar', 'author'],
  Render: ({ layer, w, h, data }) => <View layer={layer as unknown as Config} w={w} h={h} data={data} />,
  Inspector: ({ layer, set }) => {
    const c = cfg(layer as unknown as Config);
    return (
      <>
      <Section id="sb-content" title={t('Content')}>
        <Seg
          label={t('Show')}
          path="show"
          value={c.show}
          options={[
            ['platform', t('Platform')],
            ['avatar', t('Avatar')],
          ]}
        />
        {c.show === 'platform' ? (
          <>
            <Content label={t('Platform')} path="platform" value={c.platform} placeholder="{{platform}}" />
            <Row label="">
              <span className="sw-plats">
                {PLATFORMS.map(([id, name]) => (
                  <button key={id} className={`btn small ${c.platform === id ? 'on' : ''}`} onClick={() => set('platform', id)}>
                    {name}
                  </button>
                ))}
              </span>
            </Row>
          </>
        ) : (
          <>
            <Content label={t('Picture')} path="avatar" value={c.avatar} placeholder="{{avatar}}" />
            <Content label={t('Name')} path="author" value={c.author} placeholder="{{name}}" />
          </>
        )}
        <Row label={t('Font')}>
          <FamilyPick value={c.font} onChange={(f) => set('font', f)} />
        </Row>
      </Section>
      </>
    );
  },
});
