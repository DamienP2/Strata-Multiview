import type { CSSProperties } from 'react';

/** Social platforms a connection can read (same ids as the server's `Platform`). */
export type Platform = 'youtube' | 'twitch' | 'facebook' | 'instagram' | 'x' | 'tiktok' | 'whatsapp' | 'sms';

export const PLATFORMS: Record<Platform, { label: string; colour: string; glyph: string }> = {
  youtube: { label: 'YouTube', colour: '#ff0033', glyph: '▶' },
  twitch: { label: 'Twitch', colour: '#9146ff', glyph: '◆' },
  facebook: { label: 'Facebook', colour: '#1877f2', glyph: 'f' },
  instagram: { label: 'Instagram', colour: 'linear-gradient(45deg,#f9a825,#e91e63,#7b1fa2)', glyph: '◎' },
  x: { label: 'X', colour: '#000', glyph: '𝕏' },
  tiktok: { label: 'TikTok Live', colour: '#111', glyph: '♪' },
  whatsapp: { label: 'WhatsApp', colour: '#25d366', glyph: '✆' },
  sms: { label: 'SMS', colour: '#64748b', glyph: '✉' },
};

export const PLATFORM_IDS = Object.keys(PLATFORMS) as Platform[];

/** Square platform badge in the platform's colour. */
export function PlatformBadge({ platform, size = 22, title }: { platform: Platform; size?: number; title?: string }) {
  const p = PLATFORMS[platform] ?? { label: platform, colour: 'var(--panel-3)', glyph: '?' };
  const style: CSSProperties = {
    width: size,
    height: size,
    borderRadius: Math.round(size * 0.27),
    background: p.colour,
    color: '#fff',
    display: 'inline-grid',
    placeItems: 'center',
    flex: 'none',
    fontSize: Math.round(size * 0.5),
    fontWeight: 800,
    lineHeight: 1,
    boxSizing: 'border-box',
    ...(platform === 'x' ? { border: '1px solid #444' } : {}),
    ...(platform === 'tiktok' ? { boxShadow: 'inset 2px 0 #25f4ee, inset -2px 0 #fe2c55' } : {}),
  };
  return (
    <span className="soc-badge" style={style} title={title ?? p.label} aria-label={p.label}>
      {p.glyph}
    </span>
  );
}
