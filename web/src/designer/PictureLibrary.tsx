import { useEffect, useState } from 'react';
import { HexColorField } from '../ui/fields';
import { ImageOff } from 'lucide-react';
import { api, useApp, type MediaFile } from '../store';
import { t } from '../i18n';

/*
 * The picture library of the « Choose a picture » window: icons, emojis and
 * flags of the Iconify catalogue, free illustrations, shipped with the app
 * and read by the server (cg-server iconlib.rs) with no Internet. The one
 * used is copied into the project's media.
 */

type Kind = 'icons' | 'emoji' | 'flags' | 'illustrations';
const KINDS: { id: Kind; label: string; sets: [string, string][] }[] = [
  { id: 'icons', label: 'Icons', sets: [['lucide', 'Lucide'], ['material-symbols', 'Material'], ['ph', 'Phosphor'], ['tabler', 'Tabler']] },
  { id: 'emoji', label: 'Emojis', sets: [['noto', 'Noto'], ['twemoji', 'Twemoji'], ['fluent-emoji-flat', 'Fluent']] },
  { id: 'flags', label: 'Flags', sets: [['flag', 'Rectangles'], ['circle-flags', 'Circles']] },
  { id: 'illustrations', label: 'Illustrations', sets: [['open-doodles', 'Open Doodles'], ['flowbite', 'Flowbite']] },
];
const COLORS = ['#ffffff', '#8fa3ff', '#ff3b5c', '#f5c542', '#22c55e', '#38bdf8', '#111111'];
/** The illustrations' own colour (replaced by the one chosen). */
const ACCENT: Record<string, string> = { 'open-doodles': '#ff5678', flowbite: '#2563eb' };

interface Item {
  name: string;
  title: string;
}

/** The picture of an item, as the server reads it. */
const svgUrl = (set: string, name: string, color: string) => `/api/iconlib/svg?set=${encodeURIComponent(set)}&name=${encodeURIComponent(name)}${color ? `&color=${encodeURIComponent(color)}` : ''}`;

/** A picture of the library: asked again once when it fails (a busy catalogue), then left out of the grid. */
function LibImg({ src, eager }: { src: string; eager?: boolean }) {
  const [tries, setTries] = useState(0);
  if (tries > 1) return null;
  return (
    <img
      key={tries}
      src={tries ? `${src}&retry=1` : src}
      alt=""
      loading={eager ? undefined : 'lazy'}
      onError={(e) => {
        if (tries === 0) setTimeout(() => setTries(1), 800);
        else setTries(2);
        if (tries) (e.currentTarget.closest('.pl-cell') as HTMLElement | null)?.style.setProperty('display', 'none');
      }}
    />
  );
}

export function PictureLibrary({ q, onChoose }: { q: string; onChoose: (src: string) => void }) {
  const projectId = useApp((s) => s.projectId);
  const toast = useApp((s) => s.toast);
  const [kind, setKind] = useState<Kind>('icons');
  const [set, setSet] = useState('lucide');
  const [items, setItems] = useState<Item[]>([]);
  const [page, setPage] = useState(0);
  const [more, setMore] = useState(false);
  const [state, setState] = useState<'loading' | 'ok' | 'offline'>('loading');
  const [sel, setSel] = useState<Item | null>(null);
  const [color, setColor] = useState('#ffffff');
  const [query, setQuery] = useState(q);
  const [busy, setBusy] = useState(false);
  // Typing settles before the catalogue is asked.
  useEffect(() => {
    const h = setTimeout(() => (setQuery(q), setPage(0)), 300);
    return () => clearTimeout(h);
  }, [q]);
  useEffect(() => {
    let live = true;
    setState('loading');
    api<{ items: Item[]; more: boolean }>('GET', `/api/iconlib/search?set=${encodeURIComponent(set)}&q=${encodeURIComponent(query)}&page=${page}`)
      .then((r) => {
        if (!live) return;
        setItems((x) => (page ? [...x, ...r.items] : r.items));
        setMore(r.more);
        setState('ok');
      })
      .catch(() => live && setState('offline'));
    return () => {
      live = false;
    };
  }, [set, query, page]);

  const coloured = kind === 'icons' || kind === 'illustrations';
  const shownColor = coloured ? color : '';
  const pickKind = (k: Kind) => {
    setKind(k);
    setSet(KINDS.find((x) => x.id === k)!.sets[0][0]);
    setPage(0);
    setSel(null);
    setColor(k === 'illustrations' ? ACCENT[KINDS.find((x) => x.id === k)!.sets[0][0]] : '#ffffff');
  };
  const use = async (it = sel) => {
    if (!it || !projectId) return;
    setBusy(true);
    try {
      const m = await api<MediaFile>('POST', `/api/projects/${projectId}/iconlib`, { set, name: it.name, color: shownColor, title: it.title });
      onChoose(m.src);
    } catch (e) {
      toast(t('The picture could not be copied: {error}', { error: (e as Error).message }), 'error');
    }
    setBusy(false);
  };
  const kindDef = KINDS.find((x) => x.id === kind)!;
  return (
    <div className="pl">
      <div className="pl-main">
        <div className="pl-bar">
          <div className="pl-kinds">
            {KINDS.map((k) => (
              <button key={k.id} className={k.id === kind ? 'on' : ''} onClick={() => pickKind(k.id)}>
                {t(k.label)}
              </button>
            ))}
          </div>
          {kindDef.sets.length > 1 && (
            <div className="pl-sets">
              {kindDef.sets.map(([id, label]) => (
                <button key={id} className={id === set ? 'on' : ''} onClick={() => (setSet(id), setPage(0), setSel(null), ACCENT[id] && setColor(ACCENT[id]))}>
                  {label}
                </button>
              ))}
            </div>
          )}
        </div>
        {state === 'offline' ? (
          <div className="lib-empty pl-off">
            <ImageOff size={22} />
            {t('This library is not available.')}
          </div>
        ) : (
          <div className={`pl-grid ${kind === 'illustrations' ? 'big' : ''}`}>
            {items.map((it) => (
              <button key={`${set}:${it.name}:${shownColor}`} className={`pl-cell ${sel?.name === it.name ? 'on' : ''}`} title={it.title} onClick={() => setSel(it)} onDoubleClick={() => (setSel(it), void use(it))}>
                <LibImg src={svgUrl(set, it.name, shownColor)} />
              </button>
            ))}
            {state === 'loading' && <div className="pl-wait">{t('Loading…')}</div>}
            {state === 'ok' && !items.length && <div className="lib-empty">{t('No picture matches.')}</div>}
            {state === 'ok' && more && (
              <button className="btn ghost pl-more" onClick={() => setPage((p) => p + 1)}>
                {t('More')}
              </button>
            )}
          </div>
        )}
      </div>
      <div className="pl-side">
        <div className="pl-prev">{sel && <LibImg key={sel.name} src={svgUrl(set, sel.name, shownColor)} eager />}</div>
        {sel && <b className="pl-name">{sel.title}</b>}
        {coloured && (
          <div className="pl-row">
            <span>{t('Colour')}</span>
            <div className="pl-sw">
              {(kind === 'illustrations' ? [ACCENT[set], ...COLORS] : COLORS).map((c) => (
                <button key={c} className={c === color ? 'on' : ''} style={{ background: c }} onClick={() => setColor(c)} />
              ))}
              <HexColorField compact title={t('Another colour')} value={color} onChange={(c) => setColor(c.slice(0, 7))} />
            </div>
          </div>
        )}
        <button className="btn violet" disabled={!sel || busy || !projectId} onClick={() => void use()}>
          {busy ? t('Copying…') : t('Use')}
        </button>
      </div>
    </div>
  );
}
