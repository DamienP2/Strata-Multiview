import { Headphones, Volume2, VolumeX } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { formatFor, inFormat } from '../engine/formats';
import { listen, setListenMode, setVolume, useListening, type ListenMode } from '../engine/listen';
import type { Project } from '../engine/types';
import type { Live } from '../store';
import { t } from '../i18n';
import './listen.css';

interface Tile {
  id: string;
  input: string;
  name: string;
  /** In the page, px. */
  rect: [number, number, number, number];
}

/**
 * Listening on the web page output: a click on a tile plays its input's sound (a click again:
 * silence); the controls (what is heard, the volume, the sources) show while the mouse moves, then
 * hide, so the picture stays clean.
 */
export function ListenControls({ project, live, width, height, scale }: { project: Project; live: Live | undefined; width: number; height: number; scale: number }) {
  const { id: heard, volume, mode } = useListening();
  const [shown, setShown] = useState(false);
  const [menu, setMenu] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  // The tiles of the multiview on air, where they are in the page (as Program lays them out).
  const tiles = useMemo<Tile[]>(() => {
    const items = Object.entries(live?.state.layers ?? {}).sort(([a], [b]) => Number(a) - Number(b));
    const f = formatFor(project, width, height);
    const k = Math.min(width / f.width, height / f.height);
    const ox = (width - f.width * k) / 2;
    const oy = (height - f.height * k) / 2;
    const out: Tile[] = [];
    for (const [, item] of items) {
      const tpl = project.templates.find((x) => x.id === item.templateId);
      if (!tpl) continue;
      for (const l of inFormat(tpl, f.id).layers) {
        const input = String((l as { input?: string }).input ?? '').trim();
        if (l.type !== 'source' || !l.visible || !input) continue;
        const p = l.props as Record<string, unknown>;
        const [x, y, w, h] = ['x', 'y', 'width', 'height'].map((key) => Number(p[key] ?? 0));
        out.push({ id: l.id, input, name: l.name, rect: [(ox + x * k) * scale, (oy + y * k) * scale, w * k * scale, h * k * scale] });
      }
    }
    return out;
  }, [project, live?.state.layers, width, height, scale]);
  const heardTile = tiles.find((x) => x.input === heard);
  const nameOf = (input: string) => tiles.find((x) => x.input === input)?.name ?? input;

  const wake = () => {
    setShown(true);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => (setShown(false), setMenu(false)), 3000);
  };
  useEffect(() => () => clearTimeout(timer.current), []);
  // Off the page: silence.
  useEffect(() => () => listen(null), []);

  return (
    <div
      className={`lsn ${shown ? 'shown' : ''}`}
      onPointerMove={wake}
      onClick={(e) => {
        wake();
        const tile = tiles.find(({ rect: [x, y, w, h] }) => e.clientX >= x && e.clientX < x + w && e.clientY >= y && e.clientY < y + h);
        if (tile) listen(heard === tile.input ? null : tile.input);
      }}
    >
      {shown && heardTile && (
        <div className="lsn-tile" style={{ left: heardTile.rect[0], top: heardTile.rect[1], width: heardTile.rect[2], height: heardTile.rect[3] }}>
          <span>
            <Volume2 size={14} /> {t('Listening')}
          </span>
        </div>
      )}
      {shown && (
        <div className="lsn-bar" onClick={(e) => e.stopPropagation()} onPointerMove={wake}>
          <button className={`lsn-btn ${heard ? 'on' : ''}`} title={heard ? t('Silence') : t('Listen to a source')} onClick={() => (heard ? listen(null) : setMenu(!menu))}>
            {heard ? <Volume2 size={16} /> : <Headphones size={16} />}
          </button>
          <button className="lsn-name" onClick={() => setMenu(!menu)}>
            {heard ? nameOf(heard) : t('Click a tile to listen to it')}
          </button>
          {heard && (
            <>
              <VolumeX size={13} className="lsn-dim" />
              <input type="range" min={0} max={1} step={0.01} value={volume} title={t('Volume')} onChange={(e) => setVolume(+e.target.value)} />
              <span className="lsn-modes">
                {(
                  [
                    ['left', 'L', t('Left alone, in both ears')],
                    ['stereo', 'L R', t('Stereo')],
                    ['right', 'R', t('Right alone, in both ears')],
                    ['mono', 'M', t('Mono: left and right mixed')],
                  ] as [ListenMode, string, string][]
                ).map(([m, l, tip]) => (
                  <button key={m} className={mode === m ? 'on' : ''} title={tip} onClick={() => setListenMode(m)}>
                    {l}
                  </button>
                ))}
              </span>
            </>
          )}
          {menu && (
            <div className="lsn-menu">
              <button className={!heard ? 'on' : ''} onClick={() => (listen(null), setMenu(false))}>
                <VolumeX size={13} /> {t('Silence')}
              </button>
              {tiles.map((x) => (
                <button key={x.id} className={heard === x.input ? 'on' : ''} onClick={() => (listen(x.input), setMenu(false))}>
                  <Volume2 size={13} /> {x.name}
                </button>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
