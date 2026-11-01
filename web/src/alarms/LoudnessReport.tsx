// The loudness log of an input and its EBU R128 report (crates/strata-mv-server/src/loudlog.rs):
// integrated loudness, range, true peak, time above the target, the curve minute by minute.
import { Download, Gauge } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { useInputs } from '../engine/liveInputs';
import { t } from '../i18n';
import { PageHead } from '../pages/SidePage';
import { Card } from '../settings/ui';
import { api } from '../store';

interface Report {
  input: string;
  name: string;
  from: number;
  to: number;
  seconds: number;
  integrated: number | null;
  range: number | null;
  maxTruePeak: number | null;
  maxMomentary: number | null;
  maxShortTerm: number | null;
  target: number;
  loudSeconds: number;
  peakSeconds: number;
  compliant: boolean | null;
  minutes: [number, number | null, number | null, number | null][];
}

const PERIODS: [string, number][] = [
  ['Last hour', 3_600_000],
  ['Last 6 hours', 6 * 3_600_000],
  ['Last 24 hours', 24 * 3_600_000],
  ['Last 7 days', 7 * 24 * 3_600_000],
];

const n1 = (v: number | null, unit: string) => (v == null ? '—' : `${v.toFixed(1)} ${unit}`);
const dur = (s: number) => (s < 60 ? `${s} s` : s < 3600 ? `${Math.floor(s / 60)} min ${s % 60} s` : `${Math.floor(s / 3600)} h ${Math.floor((s % 3600) / 60)} min`);

/** The short-term curve (mean and highest of each minute), the target and its tolerance. */
function Curve({ r }: { r: Report }) {
  const W = 900;
  const H = 220;
  const lo = -40;
  const hi = -10;
  const x = (t: number) => ((t - r.from) / Math.max(1, r.to - r.from)) * W;
  const y = (v: number) => H - ((Math.min(hi, Math.max(lo, v)) - lo) / (hi - lo)) * H;
  const path = (k: 1 | 2) => {
    let d = '';
    let pen = false;
    for (const m of r.minutes) {
      const v = m[k];
      if (v == null) {
        pen = false;
        continue;
      }
      d += `${pen ? 'L' : 'M'}${x(m[0] + 30_000).toFixed(1)},${y(v).toFixed(1)}`;
      pen = true;
    }
    return d;
  };
  return (
    <svg className="lr-curve" viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none">
      <rect x={0} y={y(r.target + 1)} width={W} height={y(r.target - 1) - y(r.target + 1)} className="lr-band" />
      {[-40, -30, -23, -20, -10].map((v) => (
        <g key={v}>
          <line x1={0} x2={W} y1={y(v)} y2={y(v)} className={v === r.target ? 'lr-target' : 'lr-grid'} />
          <text x={4} y={y(v) - 3} className="lr-label">
            {v}
          </text>
        </g>
      ))}
      <path d={path(2)} className="lr-max" />
      <path d={path(1)} className="lr-mean" />
    </svg>
  );
}

export function LoudnessReport() {
  const inputs = useInputs();
  const [input, setInput] = useState('');
  const [span, setSpan] = useState(PERIODS[0][1]);
  const [r, setR] = useState<Report | null>(null);
  const [busy, setBusy] = useState(false);
  const id = input || inputs[0]?.id || '';
  const [to, setTo] = useState(() => Date.now());
  const from = to - span;
  useEffect(() => {
    if (!id) return;
    setBusy(true);
    api<Report>('GET', `/api/loudness/report?input=${encodeURIComponent(id)}&from=${from}&to=${to}`)
      .then(setR)
      .catch(() => setR(null))
      .finally(() => setBusy(false));
  }, [id, from, to]);
  const verdict = useMemo(() => {
    if (!r || r.compliant == null) return null;
    return r.compliant ? t('Complies with EBU R128 (−23 LUFS ±1 LU, true peak ≤ −1 dBTP)') : t('Does not comply with EBU R128');
  }, [r]);
  return (
    <>
      <PageHead title={t('Loudness')} sub={t('The loudness of each input, logged every second and kept 90 days: EBU R128 report of a period.')} />
      <div className="al-filters">
        <select value={id} onChange={(e) => setInput(e.target.value)}>
          {inputs.map((i) => (
            <option key={i.id} value={i.id}>
              {i.name}
            </option>
          ))}
        </select>
        <select value={span} onChange={(e) => setSpan(Number(e.target.value))}>
          {PERIODS.map(([l, v]) => (
            <option key={v} value={v}>
              {t(l)}
            </option>
          ))}
        </select>
        <button className="btn small" onClick={() => setTo(Date.now())} disabled={busy}>
          {t('Refresh')}
        </button>
        {id && (
          <a className="btn small" href={`/api/loudness/report?input=${encodeURIComponent(id)}&from=${from}&to=${to}&csv=true`} download>
            <Download size={13} /> CSV
          </a>
        )}
      </div>
      {!r || !r.seconds ? (
        <p className="muted">{busy ? t('Loading…') : t('Nothing logged for this input in this period.')}</p>
      ) : (
        <>
          {verdict && <div className={`lr-verdict ${r.compliant ? 'ok' : 'bad'}`}>{verdict}</div>}
          <div className="lr-stats">
            <div>
              <span>{t('Integrated')}</span>
              <b>{n1(r.integrated, 'LUFS')}</b>
            </div>
            <div>
              <span>{t('Loudness range')}</span>
              <b>{n1(r.range, 'LU')}</b>
            </div>
            <div>
              <span>{t('Highest true peak')}</span>
              <b>{n1(r.maxTruePeak, 'dBTP')}</b>
            </div>
            <div>
              <span>{t('Highest short-term')}</span>
              <b>{n1(r.maxShortTerm, 'LUFS')}</b>
            </div>
            <div>
              <span>{t('Above target +3 LU')}</span>
              <b>{dur(r.loudSeconds)}</b>
            </div>
            <div>
              <span>{t('True peak over −1 dBTP')}</span>
              <b>{dur(r.peakSeconds)}</b>
            </div>
            <div>
              <span>{t('Measured')}</span>
              <b>{dur(r.seconds)}</b>
            </div>
          </div>
          <Card icon={<Gauge size={16} />} title={t('Short-term loudness, minute by minute')}>
            <Curve r={r} />
            <p className="muted">
              {new Date(r.from).toLocaleString()} → {new Date(r.to).toLocaleString()} · {t('line: mean of the minute; light: its highest; band: target ±1 LU')}
            </p>
          </Card>
        </>
      )}
    </>
  );
}
