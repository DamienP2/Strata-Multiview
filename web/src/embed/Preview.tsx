// Preview page for outside panels (Adobe Premiere…): a template drawn by the
// HTML5 renderer, transparent, at the frame the panel asks for.
//
// URL: /embed/preview?key=pcg_… (API key). The panel drives it with
// postMessage({ type: 'pcg-preview', projectId, templateId, data, frame,
// width, height }) and gets back { type: 'pcg-warnings', list } (texts too
// long for their box) and { type: 'pcg-ready' }.
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { LiveTime } from '../engine/LiveTime';
import { Scene, textWarnings, Viewport } from '../engine/Scene';
import { formatFor, inFormat } from '../engine/formats';
import type { Data, Project } from '../engine/types';
import { api, setApiKey } from '../store';

interface Ask {
  projectId: string;
  templateId: string;
  data: Data;
  frame: number;
  width?: number;
  height?: number;
}

export function EmbedPreview() {
  const [ask, setAsk] = useState<Ask | null>(null);
  const [project, setProject] = useState<Project | null>(null);
  const [error, setError] = useState('');
  const box = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 1, h: 1 });

  useEffect(() => {
    document.documentElement.classList.add('output-mode');
    setApiKey(new URLSearchParams(location.search).get('key'));
    const onMessage = (ev: MessageEvent) => {
      const m = ev.data as ({ type?: string } & Ask) | null;
      if (m && m.type === 'pcg-preview') setAsk(m);
    };
    addEventListener('message', onMessage);
    parent.postMessage({ type: 'pcg-ready' }, '*');
    return () => removeEventListener('message', onMessage);
  }, []);

  // The project, read again when the panel asks for another one.
  useEffect(() => {
    if (!ask?.projectId || project?.id === ask.projectId) return;
    api<Project>('GET', `/api/projects/${encodeURIComponent(ask.projectId)}`)
      .then((p) => (setProject(p), setError('')))
      .catch((e) => setError((e as Error).message));
  }, [ask?.projectId]); // eslint-disable-line react-hooks/exhaustive-deps

  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const template = project?.templates.find((t) => t.id === ask?.templateId);
  // The project layout closest to the sequence (16:9, 9:16…).
  const format = project ? formatFor(project, ask?.width ?? project.width, ask?.height ?? project.height) : null;
  const t = template && format ? inFormat(template, format.id) : undefined;
  const data = ask?.data ?? {};

  // Texts too long for their box, for the panel's warning.
  const warnings = t ? textWarnings(t, data).map((w) => ({ layer: w.layer.name, ratio: w.ratio, overflows: w.overflows, keys: w.keys })) : [];
  const sig = JSON.stringify(warnings);
  useEffect(() => {
    parent.postMessage({ type: 'pcg-warnings', list: warnings }, '*');
  }, [sig]); // eslint-disable-line react-hooks/exhaustive-deps

  const fps = project ? project.frameRate.num / project.frameRate.den || 25 : 25;
  const scale = format ? Math.min(size.w / format.width, size.h / format.height) : 1;
  return (
    <div ref={box} style={{ position: 'fixed', inset: 0, display: 'grid', placeItems: 'center', overflow: 'hidden' }}>
      {error && <p style={{ color: '#ff8a8a', font: '12px system-ui', padding: 12 }}>{error}</p>}
      {t && format && (
        <Viewport width={format.width} height={format.height} scale={scale}>
          <LiveTime ageSecs={(ask?.frame ?? 0) / fps} clientNow={Date.now()} lang={project?.language || 'fr'}>
            <Scene template={t} frame={ask?.frame ?? 0} data={data} />
          </LiveTime>
        </Viewport>
      )}
    </div>
  );
}
