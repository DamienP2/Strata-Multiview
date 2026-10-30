import { LayoutTemplate, X } from 'lucide-react';
import { useState } from 'react';
import { locale, t } from '../i18n';
import { confirmDialog } from '../ui/dialogs';
import { Segmented } from '../ui/fields';
import { useApp } from '../store';
import { PlatformBadge, PLATFORMS, type Platform } from './PlatformBadge';
import { act, save, type Desk, type Hashtag } from './socialStore';

/*
 * Hashtags the project counts in every message of the connections it
 * follows (before moderation): the total, per minute, per platform.
 */

type Span = '15' | '60' | 'all';

const num = (n: number) => n.toLocaleString(locale());
const hm = (ms: number) => new Date(ms).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' });

/** The last `n` minutes of a history (zeros before it started). */
function last(history: number[], n: number) {
  return history.length >= n ? history.slice(-n) : [...Array<number>(n - history.length).fill(0), ...history];
}

export function Hashtags({ projectId, desk }: { projectId: string; desk: Desk }) {
  const [sel, setSel] = useState(0);
  const [span, setSpan] = useState<Span>('60');
  const project = useApp((s) => (s.project?.id === projectId ? s.project : null));
  const list = desk.hashtags;
  const h = list[Math.min(sel, list.length - 1)];
  const put = (next: Hashtag[]) => void save(projectId, 'hashtags', next.map((x) => ({ tag: x.tag, running: x.running })));
  const users = h ? (project?.templates ?? []).filter((tp) => JSON.stringify(tp).includes(desk.sourceId) && JSON.stringify(tp).includes(`hashtags.${h.tag.slice(1)}`)) : [];
  if (!list.length) return <div className="empty">{t('No hashtag')}</div>;
  return (
    <div className="mod-tags-page">
      <div className="mod-hcards">
        {list.map((x, i) => (
          <div key={x.tag} className={`mod-hcard ${i === sel ? 'sel' : ''}`} onClick={() => setSel(i)}>
            <div className="mod-hcard-head">
              <b>{x.tag}</b>
              <span className="spacer" />
              <button className={`mod-chip ${x.running ? 'live' : ''}`} onClick={(e) => {
                e.stopPropagation();
                put(list.map((y) => (y.tag === x.tag ? { ...y, running: !y.running } : y)));
              }}>
                {x.running ? <span className="mod-dot" /> : null}
                {x.running ? t('Live') : t('Stopped')}
              </button>
              <button
                className="icon-btn ghost mod-hdel"
                title={t('Remove')}
                onClick={async (e) => {
                  e.stopPropagation();
                  if (await confirmDialog({ title: t('Remove {tag}?', { tag: x.tag }), confirm: t('Remove'), danger: true })) put(list.filter((y) => y.tag !== x.tag));
                }}
              >
                <X size={13} />
              </button>
            </div>
            <div className="mod-hcount">
              <span className="mod-big">{num(x.count)}</span>
              {x.running && <span className="muted">{t('{n} / min', { n: num(x.perMin) })}</span>}
              <span className="spacer" />
              <Bars values={last(x.history, 10)} small />
            </div>
            {i > 0 && list[0].count > 0 && <div className="muted">{t('{x} × {tag}', { x: (x.count / list[0].count).toLocaleString(locale(), { maximumFractionDigits: 1 }), tag: list[0].tag })}</div>}
            {Object.keys(x.platforms).length > 0 && (
              <div className="mod-shares">
                {Object.entries(x.platforms)
                  .sort((a, b) => b[1] - a[1])
                  .map(([p, n]) => (
                    <span key={p} className="mod-chip static" title={PLATFORMS[p as Platform]?.label ?? p}>
                      <PlatformBadge platform={p as Platform} size={14} /> {n} %
                    </span>
                  ))}
              </div>
            )}
            <div className="muted mod-since">
              {t('Since {time}', { time: hm(x.since) })} ·{' '}
              <button
                className="mod-link"
                onClick={(e) => {
                  e.stopPropagation();
                  void act(projectId, 'reset', { tag: x.tag });
                }}
              >
                {t('Reset')}
              </button>
            </div>
          </div>
        ))}
      </div>
      {h && <Chart h={h} span={span} setSpan={setSpan} />}
      {h && users.length > 0 && (
        <div className="mod-users">
          <LayoutTemplate size={15} />
          <b>{t('Used by')}</b>
          {users.map((u) => (
            <span key={u.id} className="mod-chip static">
              {u.name}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

function Chart({ h, span, setSpan }: { h: Hashtag; span: Span; setSpan: (s: Span) => void }) {
  const values = span === 'all' ? h.history : last(h.history, Number(span));
  const now = Date.now();
  const start = now - (values.length - 1) * 60_000;
  const ticks = [0, 0.25, 0.5, 0.75, 1].map((f) => hm(start + f * (now - start)));
  let title = t('{tag} · last 15 min', { tag: h.tag });
  if (span === '60') title = t('{tag} · last hour', { tag: h.tag });
  if (span === 'all') title = t('{tag} · since {time}', { tag: h.tag, time: hm(h.since) });
  return (
    <div className="mod-chart">
      <div className="mod-chart-head">
        <b>{title}</b>
        <span className="spacer" />
        <Segmented
          value={span}
          onChange={setSpan}
          options={[
            { value: '15', label: t('15 min') },
            { value: '60', label: t('1 h') },
            { value: 'all', label: t('Whole show') },
          ]}
        />
      </div>
      <Bars values={values} />
      <div className="mod-axis">
        {ticks.map((x, i) => (
          <span key={i}>{x}</span>
        ))}
      </div>
    </div>
  );
}

function Bars({ values, small }: { values: number[]; small?: boolean }) {
  const max = Math.max(1, ...values);
  return (
    <div className={`mod-bars ${small ? 'small' : ''}`}>
      {values.map((v, i) => (
        <i key={i} style={{ height: `${Math.max(v ? 4 : 0, (v / max) * 100)}%` }} title={small ? undefined : String(v)} />
      ))}
    </div>
  );
}
