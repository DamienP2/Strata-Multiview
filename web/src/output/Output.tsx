import { useEffect, useState } from 'react';
import { Viewport } from '../engine/Scene';
import { useApp } from '../store';
import { Program } from './Program';
import { OutputDebug } from './OutputDebug';
import { ListenControls } from './Listen';

/**
 * HTML5 output of one channel picture: a transparent, chrome-less page meant
 * for browser sources (OBS, vMix, CasparCG HTML producer…).
 * URL: /output/<page> (the address set on its web page destination, or the
 * channel's id). Query params: `scale=fit|1`
 * (default fit), `bg=<css color>` for checks, `debug` for motion measures.
 */
export function Output({ channelId: page }: { channelId: string }) {
  const { connect, channels, full, live } = useApp();
  // The channel of this address (a page's own address, else a channel's id).
  const [channelId, setChannelId] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    fetch(`/api/outputs/page/${encodeURIComponent(page)}`, { credentials: 'same-origin' })
      .then((r) => (r.ok ? r.json() : null))
      .then((j: { channel?: string } | null) => alive && setChannelId(j?.channel ?? page))
      .catch(() => alive && setChannelId(page));
    return () => {
      alive = false;
    };
  }, [page]);
  const params = new URLSearchParams(location.search);
  const fixed = params.get('scale') === '1';
  const bg = params.get('bg') ?? 'transparent';
  // Measures of the page's motion where it runs (vMix, OBS…).
  const debug = params.has('debug');
  const [size, setSize] = useState({ w: innerWidth, h: innerHeight });

  useEffect(() => {
    if (!channelId) return;
    document.documentElement.classList.add('output-mode');
    connect(channelId);
    const onResize = () => setSize({ w: innerWidth, h: innerHeight });
    addEventListener('resize', onResize);
    return () => removeEventListener('resize', onResize);
  }, [connect, channelId]);

  const channel = channels.find((c) => c.id === channelId);
  if (!channel || !channelId) return null;
  const picture = channel;
  const project = channel.projectId ? full[channel.projectId] : undefined;
  const scale = fixed ? 1 : Math.min(size.w / picture.width, size.h / picture.height);
  // No project loaded: what the other outputs show (the server draws the
  // built-in designs; a new address when the settings change).
  const pictureId = 'main';
  const key = [channel.name, picture.width, picture.height, channel.frameRate.num, channel.frameRate.den, picture.slateStyle].join(' ');
  const slate = channel.projectId ? '' : picture.slateStyle === 'custom' ? (picture.slate ?? '') : `/api/channels/${encodeURIComponent(channelId)}/slate.png?${new URLSearchParams({ picture: pictureId, v: key })}`;
  return (
    <div style={{ position: 'fixed', inset: 0, background: bg, overflow: 'hidden' }}>
      <Viewport width={picture.width} height={picture.height} scale={scale}>
        {project && <Program project={project} live={live[channelId]} width={picture.width} height={picture.height} />}
        {/* Filling the frame, as on the other outputs. */}
        {slate && <img src={slate} alt="" style={{ position: 'absolute', inset: 0, width: '100%', height: '100%', objectFit: 'cover' }} />}
      </Viewport>
      {/* A click on a tile: its sound (controls while the mouse moves). */}
      {project && <ListenControls project={project} live={live[channelId]} width={picture.width} height={picture.height} scale={scale} />}
      {debug && <OutputDebug fps={channel.frameRate.num / channel.frameRate.den || 25} />}
    </div>
  );
}
