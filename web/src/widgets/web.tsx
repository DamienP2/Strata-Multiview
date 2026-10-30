// Web page: a web address or some HTML in the graphic. Only a browser draws it: the HTML5 outputs and
// the previews show it; the engine's own outputs (SDI, NDI, ST 2110) leave its box empty.
// Model: crates/cg-core/src/wid/web.rs.
import { useEffect, useReducer, useRef, useState } from 'react';
import { Globe, RefreshCw } from 'lucide-react';
import type { Data } from '../engine/types';
import { Content, defineWidget, Num, RangeNum, Section, Seg, Toggle, useKeys, type Warning } from './kit';
import { Centre } from './centre';
import { t, tc } from '../i18n';

export interface Config {
  source?: 'url' | 'html';
  url?: string;
  html?: string;
  /** Loaded again every N seconds (0 = never), on the same wall-clock beats in every browser. */
  refresh?: number;
  /** Page zoom, %. */
  zoom?: number;
  /** The page's own background (on a white page); off: the page is made transparent. */
  opaque?: boolean;
  /** Fetched by the server (sites that refuse to be framed); see cg-server/src/webproxy.rs. */
  proxy?: boolean;
}
export interface State {
  reloadedAt?: number;
}
type WebLayer = Config & { id: string };

const cfg = (c: Config) => ({ source: c.source ?? 'url', url: c.url ?? 'https://example.com', html: c.html ?? '', refresh: Math.max(0, c.refresh ?? 0), zoom: Math.max(10, c.zoom ?? 100), opaque: !!c.opaque, proxy: c.proxy ?? true });

/** Only http(s) pages (no javascript: or file: addresses). */
const safeUrl = (u: string) => (/^https?:\/\//i.test(u.trim()) ? u.trim() : '');

const escHtml = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;');
const KEY = /\{\{([^}]*)\}\}/g;
/** The address with the data in: a whole `{{key}}` as it is, else each value encoded (spaces, & …). */
export function urlOf(url: string, data: Data): string {
  const whole = /^\s*\{\{([^}]*)\}\}\s*$/.exec(url);
  if (whole) return data[whole[1].trim()] ?? '';
  return url.replace(KEY, (_, k: string) => encodeURIComponent(data[k.trim()] ?? ''));
}
/** The HTML code with the data in, as text (a value never adds markup). */
export const htmlOf = (html: string, data: Data) => html.replace(KEY, (_, k: string) => escHtml(data[k.trim()] ?? ''));
/** Makes a page transparent: its root and body backgrounds are cleared. */
const CLEAR = '<style>html,body{background:transparent!important}</style>';

// ------------------------------------------------------------- load state --

/** Whether the page of a layer loaded (as this browser sees it), for the Operator's row. */
type Status = { s: 'loading' | 'ok' | 'error'; msg?: string };
const status = new Map<string, Status>();
const watchers = new Set<() => void>();
function report(id: string, st: Status) {
  const cur = status.get(id);
  if (cur?.s === st.s && cur?.msg === st.msg) return;
  status.set(id, st);
  watchers.forEach((f) => f());
}
function useStatus(id: string): Status | undefined {
  const [, rerender] = useReducer((x: number) => x + 1, 0);
  useEffect(() => {
    watchers.add(rerender);
    return () => void watchers.delete(rerender);
  }, []);
  return status.get(id);
}

/** Seconds a page may take before it counts as not loaded. */
const TIMEOUT = 20;

interface Frame {
  key: string;
  src?: string;
  doc?: string;
}
const idOf = (f: Frame | null) => (f ? `${f.key}|${f.src ?? ''}|${f.doc ?? ''}` : '');

function View({ layer, w, h, data, state, nowMs, designer }: { layer: WebLayer; w: number; h: number; data: Data; state: unknown; nowMs: number; designer?: boolean }) {
  const k = cfg(layer);
  // Wall-clock beats of the periodic reload (server time): every output reloads at the same moment.
  const offset = nowMs - Date.now();
  const beatOf = () => (k.refresh ? Math.floor((Date.now() + offset) / (k.refresh * 1000)) : 0);
  const [beat, setBeat] = useState(beatOf);
  useEffect(() => {
    if (!k.refresh) return;
    let tm = 0;
    const next = () => {
      const ms = k.refresh * 1000;
      const now = Date.now() + offset;
      tm = window.setTimeout(() => (setBeat(beatOf()), next()), ms - (now % ms) + 20);
    };
    next();
    return () => clearTimeout(tm);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [k.refresh, Math.round(offset / 1000)]);
  const z = k.zoom / 100;
  const url = safeUrl(urlOf(k.url, data));
  const key = `${(state as State | undefined)?.reloadedAt ?? 0}-${beat}`;
  const want: Frame | null =
    k.source === 'html'
      ? { key: `${key}|h`, doc: (k.opaque ? '' : CLEAR) + htmlOf(k.html, data) }
      : url
        ? { key: `${key}|${url}`, src: k.proxy ? `/api/web/proxy?url=${encodeURIComponent(url)}${k.opaque ? '' : '&transparent=1'}` : url }
        : null;
  // Reload without a white frame: the new page loads hidden over the shown one, then takes its place.
  const [shown, setShown] = useState<Frame | null>(want);
  const [loading, setLoading] = useState<Frame | null>(null);
  const wantId = idOf(want);
  const shownId = idOf(shown);
  const frames = useRef(new Map<string, HTMLIFrameElement>());
  // Frames showing the proxy's error page (by key): never counted as loaded.
  const failed = useRef(new Set<string>());
  const [, bump] = useReducer((x: number) => x + 1, 0);
  const loadedId = useRef('');
  useEffect(() => {
    if (wantId === shownId) return setLoading(null);
    if (!want || !shown) {
      setShown(want);
      setLoading(null);
      return;
    }
    setLoading(want);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wantId]);
  // Load state for the Operator; a page that never loads keeps the one shown.
  useEffect(() => {
    if (!want) return void status.delete(layer.id);
    if (loadedId.current !== wantId) report(layer.id, { s: 'loading' });
    const tm = window.setTimeout(() => {
      if (loadedId.current === wantId) return;
      report(layer.id, { s: 'error', msg: t('No answer after {n} s', { n: TIMEOUT }) });
      setLoading((l) => (idOf(l) === wantId ? null : l));
    }, TIMEOUT * 1000);
    return () => clearTimeout(tm);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wantId]);
  // The proxy's error page says why (cg-server/src/webproxy.rs).
  useEffect(() => {
    const on = (e: MessageEvent) => {
      const m = e.data as { strataWeb?: string; msg?: string } | null;
      if (!m || m.strataWeb !== 'error') return;
      const hit = [...frames.current.entries()].find(([, f]) => f.contentWindow === e.source);
      if (!hit) return;
      failed.current.add(hit[0]);
      report(layer.id, { s: 'error', msg: m.msg });
      bump();
    };
    addEventListener('message', on);
    return () => removeEventListener('message', on);
  }, [layer.id]);
  const loaded = (f: Frame) => {
    loadedId.current = idOf(f);
    if (failed.current.has(f.key)) {
      // A reload that failed keeps the page shown.
      if (loading && f.key === loading.key) setLoading(null);
      return;
    }
    if (status.get(layer.id)?.s !== 'error' || idOf(f) === wantId) report(layer.id, { s: 'ok' });
    if (loading && f.key === loading.key) {
      setShown(loading);
      setLoading(null);
    }
  };
  const style = (hidden: boolean) => ({ position: 'absolute' as const, left: 0, top: 0, width: w / z, height: h / z, border: 0, transform: `scale(${z})`, transformOrigin: '0 0', background: k.opaque ? '#fff' : 'transparent', colorScheme: 'normal', visibility: hidden ? ('hidden' as const) : undefined, pointerEvents: designer || hidden ? ('none' as const) : undefined });
  const iframe = (f: Frame, hidden: boolean) => (
    <iframe
      key={idOf(f)}
      ref={(el) => void (el ? frames.current.set(f.key, el) : frames.current.delete(f.key))}
      title={layer.id}
      onLoad={() => loaded(f)}
      {...(f.doc != null ? { srcDoc: f.doc, sandbox: 'allow-scripts' } : { src: f.src, sandbox: k.proxy ? 'allow-scripts allow-forms allow-popups' : 'allow-scripts allow-same-origin allow-forms allow-popups', referrerPolicy: 'no-referrer' as const })}
      // The error page is for the Designer: on air the frame stays empty (the Operator's dot says why).
      style={style(hidden || (!designer && failed.current.has(f.key)))}
    />
  );
  return (
    <div style={{ position: 'absolute', inset: 0, overflow: 'hidden' }}>
      {/* One keyed list: the page that loaded keeps its element (moving an iframe reloads it). */}
      {[shown, loading && idOf(loading) !== shownId ? loading : null].filter((f): f is Frame => !!f).map((f) => iframe(f, f !== shown))}
      {designer && !want && <div style={{ position: 'absolute', inset: 0, display: 'grid', placeItems: 'center', border: '4px dashed rgba(255,255,255,.35)', color: 'rgba(255,255,255,.6)', font: '600 26px Inter, sans-serif' }}>{t('Web page: an http(s) address')}</div>}
    </div>
  );
}

/** The address without its scheme, cut to `max` characters. */
const shortUrl = (u: string, max = 48) => {
  const s = u.replace(/^https?:\/\//i, '').replace(/\/$/, '');
  return s.length > max ? `${s.slice(0, max - 1)}…` : s;
};

/**
 * The control centre (web/src/widgets/centre.tsx). Line 1: the address and whether the page
 * loaded (a dot). Line 2: Refresh. Line 3: the periodic reload (exposable).
 */
function WebBar({ layer, data, act }: { layer: WebLayer; data: Data; act: (op: string, args?: unknown) => void }) {
  const k = cfg(layer);
  const st = useStatus(layer.id);
  useKeys({ r: () => act('reload') });
  const dot = st?.s === 'error' ? { c: 'var(--live)', tip: st.msg || t('Page not loaded') } : st?.s === 'loading' ? { c: 'var(--warn)', tip: t('Loading…') } : st ? { c: 'var(--pvw)', tip: t('Page loaded') } : null;
  const address = k.source === 'html' ? t('HTML code') : shortUrl(urlOf(k.url, data)) || '—';
  return (
    <Centre
      name={(layer as { name?: string }).name}
      readout={{ v: null, sub: <span title={k.source === 'html' ? undefined : urlOf(k.url, data)}>{address}</span> }}
      status={
        dot && (
          <span className="cc-st" title={dot.tip} style={{ color: dot.c }}>
            ●
          </span>
        )
      }
      actions={[{ icon: RefreshCw, label: tc('cc', 'Refresh'), key: 'R', onClick: () => act('reload') }]}
    >
      <RangeNum label={t('Refresh every')} path="refresh" value={k.refresh} min={0} max={3600} step={1} unit="s" onRelease />
    </Centre>
  );
}

export default defineWidget({
  id: 'web',
  type: 'web',
  label: 'Iframe',
  group: 'Data',
  icon: Globe,
  create: () => ({ source: 'url', url: 'https://example.com', zoom: 100, proxy: true, props: { width: 1280, height: 720 } }),
  ownsLook: true,
  content: ['url', 'html'],
  Render: ({ layer, w, h, data, state, time, designer }) => <View layer={layer as unknown as WebLayer} w={w} h={h} data={data} state={state} nowMs={time.nowMs} designer={designer} />,
  Inspector: ({ layer }) => {
    const k = cfg(layer as unknown as WebLayer);
    return (
      <>
        <Section id="web-source" title={t('Page')}>
          <Seg
            label={t('Source')}
            path="source"
            value={k.source}
            options={[
              ['url', t('Web address')],
              ['html', t('HTML code')],
            ]}
          />
          {k.source === 'url' ? <Content label={t('Address')} path="url" value="" placeholder="https://…" expose /> : <Content label="HTML" path="html" value="" multiline placeholder="<div>…</div>" />}
          {k.source === 'url' && <Toggle label={t('Through the server')} path="proxy" value={true} />}
          <Num label={t('Zoom')} path="zoom" value={k.zoom} min={10} max={400} unit="%" />
          {(k.source === 'html' || k.proxy) && (
            <Seg
              label={t('Background')}
              path="opaque"
              value={k.opaque}
              options={[
                [false, t('Transparent')],
                [true, t("The page's")],
              ]}
            />
          )}
        </Section>
      </>
    );
  },
  Operator: ({ layer, data, act }) => <WebBar layer={layer as unknown as WebLayer} data={data} act={act} />,
  warnings: (l) => {
    const k = cfg(l as unknown as WebLayer);
    const out: Warning[] = [{ level: 'info', op: true, text: t('Web page: HTML5 outputs and previews only (not SDI, NDI, ST 2110).') }];
    if (k.source === 'url' && k.url.trim() && !k.url.includes('{{') && !safeUrl(k.url)) out.push({ level: 'error', text: t('The address must start with http:// or https://.') });
    if (k.source === 'url' && !k.proxy && !k.opaque) out.push({ level: 'info', text: t('Loaded by the browser itself: the page cannot be made transparent.') });
    return out;
  },
});
