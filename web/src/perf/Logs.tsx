import { ArrowDownToLine, Download, Eraser, FileText, Pause, Play, Search, X } from 'lucide-react';
import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { LogEntry, LogsResponse } from '../engine/metrics';
import { t, tc, tn } from '../i18n';
import { api } from '../store';
import { clock } from './format';

/*
 * The server's log, live: polled every 1.5 s from the last line seen. The
 * view keeps the last MAX lines, follows the end while scrolled to the
 * bottom, and stops asking while paused.
 */

const MAX = 2000;
const LIMIT = 500;
type Level = 'ERROR' | 'WARN' | 'INFO' | 'DEBUG';
/** Labels translated where shown. */
const LEVELS: { value: Level; label: string }[] = [
  { value: 'ERROR', label: 'Errors' },
  { value: 'WARN', label: 'Warnings' },
  { value: 'INFO', label: 'Info' },
  { value: 'DEBUG', label: 'Debug' },
];
const LEVEL_KEY = 'strata.perf.logLevel';

const readLevel = (): Level => {
  try {
    const v = localStorage.getItem(LEVEL_KEY);
    return LEVELS.some((l) => l.value === v) ? (v as Level) : 'INFO';
  } catch {
    return 'INFO';
  }
};

export function Logs() {
  const [rows, setRows] = useState<LogEntry[]>([]);
  const [level, setLevelState] = useState<Level>(readLevel);
  const [paused, setPaused] = useState(false);
  const [q, setQ] = useState('');
  const [file, setFile] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [follow, setFollow] = useState(true);
  const last = useRef(0);
  const box = useRef<HTMLDivElement>(null);

  const setLevel = (l: Level) => {
    if (l === level) return;
    try {
      localStorage.setItem(LEVEL_KEY, l);
    } catch {
      /* private mode */
    }
    // Asked again from the start of what the server keeps.
    last.current = 0;
    setRows([]);
    setFollow(true);
    setLevelState(l);
  };

  useEffect(() => {
    if (paused) return;
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const tick = async () => {
      let again = 1500;
      try {
        const r = await api<LogsResponse>('GET', `/api/system/logs?after=${last.current}&limit=${LIMIT}&level=${level}`);
        if (!alive) return;
        setError(null);
        setFile(r.file);
        // The server started again: its numbering too.
        if (r.last < last.current) {
          last.current = 0;
          again = 0;
        } else {
          const fresh = r.entries.filter((e) => e.seq > last.current);
          const top = fresh.length ? fresh[fresh.length - 1].seq : last.current;
          // A full page: there is more to catch up.
          if (r.entries.length >= LIMIT) {
            last.current = top;
            again = 50;
          } else last.current = Math.max(top, r.last);
          if (fresh.length) setRows((rs) => (rs.length + fresh.length > MAX ? [...rs, ...fresh].slice(-MAX) : [...rs, ...fresh]));
        }
      } catch (e) {
        if (!alive) return;
        setError((e as Error).message);
      }
      if (alive) timer = setTimeout(tick, again);
    };
    void tick();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [paused, level]);

  const needle = q.trim().toLowerCase();
  const shown = useMemo(() => (needle ? rows.filter((e) => `${e.message} ${e.target} ${e.fields}`.toLowerCase().includes(needle)) : rows), [rows, needle]);

  // Follows the end while scrolled to the bottom.
  useLayoutEffect(() => {
    const el = box.current;
    if (el && follow) el.scrollTop = el.scrollHeight;
  }, [shown, follow]);
  const onScroll = () => {
    const el = box.current;
    if (!el) return;
    const bottom = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
    if (bottom !== follow) setFollow(bottom);
  };

  return (
    <section className="set-card perf-logs">
      <div className="log-toolbar">
        <div className="seg log-levels" role="group" aria-label={t('Level')}>
          {LEVELS.map((l) => (
            <button key={l.value} className={`${level === l.value ? 'on' : ''} lv-${l.value}`} onClick={() => setLevel(l.value)} title={l.value === 'DEBUG' ? t('Everything') : t('{level} and more serious', { level: t(l.label) })}>
              <span className="log-chip-dot" />
              {t(l.label)}
            </button>
          ))}
        </div>
        <label className="search log-search">
          <Search size={14} />
          <input value={q} placeholder={t('Search the log')} onChange={(e) => setQ(e.target.value)} />
          {q && (
            <button className="icon-btn ghost" title={t('Clear the search')} onClick={() => setQ('')}>
              <X size={13} />
            </button>
          )}
        </label>
        <div className="spacer" />
        <button className={`btn small ${paused ? 'on' : ''}`} onClick={() => setPaused((p) => !p)} title={paused ? t('Show new lines again') : t('Stop adding new lines')}>
          {paused ? <Play size={13} /> : <Pause size={13} />} {paused ? tc('logs', 'Resume') : t('Pause')}
        </button>
        <button className="btn small" onClick={() => setRows([])} title={t('Empty this view (the log on the server stays)')}>
          <Eraser size={13} /> {t('Clear')}
        </button>
        <a className="btn small" href="/api/system/logs/download" download>
          <Download size={13} /> {t('Download')}
        </a>
      </div>

      <div className="log-box-wrap">
        <div ref={box} className="log-box" onScroll={onScroll}>
          {shown.length === 0 ? (
            <div className="log-empty">{error ? t('Cannot read the log: {error}', { error }) : needle ? t('No line matches.') : paused ? t('Paused.') : t('Waiting for log lines…')}</div>
          ) : (
            shown.map((e) => <Row key={e.seq} e={e} />)
          )}
        </div>
        {!follow && shown.length > 0 && (
          <button className="btn small log-jump" onClick={() => setFollow(true)}>
            <ArrowDownToLine size={13} /> {t('Latest lines')}
          </button>
        )}
      </div>

      <div className="log-foot muted">
        <span className={`log-live ${paused ? 'paused' : error ? 'err' : ''}`} />
        <span>{paused ? t('Paused') : error ? t('Not connected') : t('Live')}</span>
        <span>·</span>
        <span>{needle ? t('{n} of {total} lines', { n: shown.length, total: rows.length }) : tn(rows.length, '{n} line', '{n} lines')}</span>
        <span className="spacer" />
        {file && (
          <span className="log-file" title={file}>
            <FileText size={12} /> <code>{file}</code>
          </span>
        )}
      </div>
    </section>
  );
}

/** "cg_output::stream" → "output::stream". */
const shortTarget = (s: string) => s.replace(/^cg_/, '');

const Row = memo(function Row({ e }: { e: LogEntry }) {
  return (
    <div className={`log-row lv-${e.level}`}>
      <span className="log-time">{clock(e.at)}</span>
      <span className="log-lv">{e.level}</span>
      <span className="log-target" title={e.target}>
        {shortTarget(e.target)}
      </span>
      <span className="log-msg">
        {e.message}
        {e.fields && <span className="log-fields"> {e.fields}</span>}
      </span>
    </div>
  );
});
