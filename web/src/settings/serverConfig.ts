import { useEffect, useMemo, useState } from 'react';
import { t } from '../i18n';
import type { ServerConfig, SystemInfo } from '../outputs/types';
import { api, useApp } from '../store';

export const splitAddr = (a?: string | null): [string, string] => {
  if (!a) return ['', ''];
  const i = a.lastIndexOf(':');
  return i < 0 ? [a, ''] : [a.slice(0, i), a.slice(i + 1)];
};
export const joinAddr = (ip: string, port: string) => `${ip.trim()}:${port.trim()}`;

/**
 * The server's settings being edited (Settings and MOS pages): saving
 * applies them and restarts the engine.
 */
export function useServerConfig(page: string) {
  const toast = useApp((s) => s.toast);
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const [cfg, setCfg] = useState<ServerConfig | null>(null);
  const [saving, setSaving] = useState(false);

  const load = async () => {
    const i = await api<SystemInfo>('GET', '/api/system');
    setInfo(i);
    setCfg(structuredClone(i.config));
  };
  useEffect(() => {
    load().catch((e) => toast(String(e), 'error'));
    const h = setInterval(() => {
      if (document.hidden) return;
      api<SystemInfo>('GET', '/api/system')
        .then(setInfo)
        .catch(() => {});
    }, 5000);
    return () => clearInterval(h);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const dirty = useMemo(() => !!info && !!cfg && JSON.stringify(info.config) !== JSON.stringify(cfg), [info, cfg]);
  const edit = (fn: (c: ServerConfig) => void) => {
    if (!cfg) return;
    const next = structuredClone(cfg);
    fn(next);
    setCfg(next);
  };
  const save = async () => {
    if (!cfg) return;
    setSaving(true);
    try {
      await api('PUT', '/api/system/config', cfg);
      toast(t('Settings saved — the engine is restarting'), 'info');
      const newPort = splitAddr(cfg.bind)[1];
      if (newPort && newPort !== (location.port || (location.protocol === 'https:' ? '443' : '80'))) {
        // The UI moves with the server.
        setTimeout(() => (location.href = `${location.protocol}//${location.hostname}:${newPort}/${page}`), 2000);
        return;
      }
      // The server may still be restarting: try again until it answers.
      const reload = (tries: number) => void load().catch(() => tries > 0 && setTimeout(() => reload(tries - 1), 1000));
      setTimeout(() => reload(15), 1500);
    } catch (e) {
      toast((e as Error).message, 'error');
    } finally {
      setSaving(false);
    }
  };
  const discard = () => info && setCfg(structuredClone(info.config));
  return { info, cfg, edit, dirty, saving, save, discard, reload: load };
}
