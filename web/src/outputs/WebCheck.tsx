import { CheckCircle2, Loader2, Stethoscope, XCircle } from 'lucide-react';
import { useState } from 'react';
import { api } from '../store';
import { t } from '../i18n';

/** What the server found on a live's page (mirror of weblive::Check). */
interface Check {
  found?: { platform: string; title: string; channel?: string; live: boolean; width: number; height: number; fps?: number; protocol: string };
  error?: string;
  ytdlp: string;
  ytdlpPath: string;
  deno?: string;
  seconds: number;
}

/** « Test the link »: the server looks for the stream of the page now and says how it went. */
export function WebCheck({ url, height }: { url: string; height: number }) {
  const [busy, setBusy] = useState(false);
  const [res, setRes] = useState<Check | { error: string } | null>(null);
  const test = async () => {
    setBusy(true);
    setRes(null);
    try {
      setRes(await api<Check>('GET', `/api/inputs/web-check?url=${encodeURIComponent(url)}&height=${height}`));
    } catch (e) {
      setRes({ error: (e as Error).message });
    } finally {
      setBusy(false);
    }
  };
  const c = res as Check | null;
  return (
    <div className="wc">
      <button type="button" className="btn small" disabled={busy || !url.trim()} onClick={test} title={t('The server looks for the stream of this page now (the first time, it downloads yt-dlp)')}>
        {busy ? <Loader2 size={13} className="spin" /> : <Stethoscope size={13} />} {busy ? t('Testing…') : t('Test the link')}
      </button>
      {c && (
        <div className={`wc-res ${c.found ? 'ok' : 'bad'}`}>
          {c.found ? (
            <>
              <b>
                <CheckCircle2 size={14} /> {c.found.platform} · {c.found.live ? t('Live') : t('Not live')}
              </b>
              <span>{[c.found.title, c.found.channel].filter(Boolean).join(' — ')}</span>
              <span className="mono">
                {c.found.width && c.found.height ? `${c.found.width}×${c.found.height}` : ''}
                {c.found.fps ? ` · ${c.found.fps} fps` : ''} · {c.found.protocol}
              </span>
            </>
          ) : (
            <>
              <b>
                <XCircle size={14} /> {t('No stream found')}
              </b>
              <span className="wc-err">{c.error}</span>
            </>
          )}
          {'ytdlp' in c && (
            <span className="mono dim">
              yt-dlp {c.ytdlp || '—'}
              {c.deno ? ` · Deno ${c.deno === 'absent' ? t('absent') : t('present')}` : ''} · {c.seconds.toFixed(1)} s
            </span>
          )}
        </div>
      )}
    </div>
  );
}
