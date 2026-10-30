import { Clock, CloudDownload, Cpu, Keyboard, Network, RotateCcw, Server, SlidersHorizontal, Timer } from 'lucide-react';
import { useEffect, useState } from 'react';
import { t } from '../i18n';
import { NavItem, NavSection, PageHead, SidePage } from '../pages/SidePage';
import { api, can, useApp } from '../store';
import type { HardwareInfo } from '../outputs/types';
import { Card, Field, SaveBar } from './ui';
import { useServerConfig } from './serverConfig';
import { Networks } from './Networks';
import { TimePage } from './TimePage';
import { VideoSync } from './VideoSync';
import { Updates } from './Updates';
import { LibraryUpdates } from './LibraryUpdates';
import { ShortcutsPage } from './Shortcuts';

const TIMEZONES: string[] = (() => {
  try {
    return (Intl as unknown as { supportedValuesOf(k: string): string[] }).supportedValuesOf('timeZone');
  } catch {
    return ['UTC', 'Europe/Paris', 'Europe/London', 'America/New_York'];
  }
})();

function uptime(s: number) {
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  return d ? `${d}d ${h}h` : h ? `${h}h ${m}m` : `${m}m ${s % 60}s`;
}

type Section = 'shortcuts' | 'general' | 'network' | 'time' | 'sync' | 'updates' | 'system';
const SECTIONS: Section[] = ['shortcuts', 'general', 'network', 'time', 'sync', 'updates', 'system'];
const sectionOf = (hash: string): Section => SECTIONS.find((s) => `#${s}` === hash) ?? 'general';

/** Your own settings; an admin's also the server's. */
export function Settings() {
  const user = useApp((s) => s.auth?.user);
  if (can.admin(user)) return <ServerSettings />;
  const nav = (
    <>
      <NavSection>{t('You')}</NavSection>
      <NavItem on icon={<Keyboard size={15} />} label={t('Shortcuts')} />
    </>
  );
  return (
    <SidePage nav={nav}>
      <ShortcutsPage />
    </SidePage>
  );
}

/** The server: name, clocks, network, updates and what it runs on. */
function ServerSettings() {
  const toast = useApp((s) => s.toast);
  const { info, cfg, edit, dirty, saving, save, discard, reload } = useServerConfig('settings');
  const [section, setSection] = useState<Section>(() => sectionOf(location.hash));
  const [det, setDet] = useState<HardwareInfo | null>(null);
  useEffect(() => {
    const follow = () => setSection(sectionOf(location.hash));
    window.addEventListener('hashchange', follow);
    return () => window.removeEventListener('hashchange', follow);
  }, []);
  // Network cards and clock status (Networks, Time, Video sync).
  const detect = () =>
    api<HardwareInfo>('GET', '/api/hardware')
      .then(setDet)
      .catch(() => {});
  useEffect(() => {
    if (section === 'network' || section === 'time' || section === 'sync') void detect();
  }, [section]);
  if (!info || !cfg) return <div className="loading">{t('Loading settings…')}</div>;

  const restart = async () => {
    await api('POST', '/api/system/restart').catch((e) => toast((e as Error).message, 'error'));
    toast(t('Engine restarting…'), 'info');
    setTimeout(() => reload().catch(() => {}), 1500);
  };

  const go = (s: Section) => {
    setSection(s);
    history.replaceState(null, '', `#${s}`);
  };
  const item = (s: Section, icon: React.ReactNode, label: string) => <NavItem on={section === s} icon={icon} label={label} onClick={() => go(s)} />;
  const nav = (
    <>
      <NavSection>{t('You')}</NavSection>
      {item('shortcuts', <Keyboard size={15} />, t('Shortcuts'))}
      <NavSection>{t('Server')}</NavSection>
      {item('general', <SlidersHorizontal size={15} />, t('General'))}
      {item('network', <Network size={15} />, t('Networks'))}
      {item('time', <Clock size={15} />, t('Time'))}
      {item('sync', <Timer size={15} />, t('Video sync'))}
      {item('updates', <CloudDownload size={15} />, t('Updates'))}
      {item('system', <Cpu size={15} />, t('System'))}
      <div className="lib-nav-foot">
        <span className="nav-dot ok" /> {t('Up {time}', { time: uptime(info.uptimeSecs) })}
        <span className="muted">
          Strata {info.version} · {info.os}/{info.arch}
        </span>
      </div>
    </>
  );
  if (section === 'shortcuts') return <SidePage nav={nav}><ShortcutsPage /></SidePage>;

  return (
    <SidePage nav={nav}>
      <PageHead title={info.config.name} sub={`Strata ${info.version} · ${info.hostname} · ${info.os}/${info.arch} · ${t('up {time}', { time: uptime(info.uptimeSecs) })}`}>
        <button className="btn" onClick={restart} title={t('Restart the playout engine and outputs')}>
          <RotateCcw size={14} /> {t('Restart engine')}
        </button>
      </PageHead>

      {section === 'general' && (
        <Card icon={<Server size={16} />} title={t('General')}>
          <div className="set-grid">
            <Field label={t('Server name')} hint={t('Shown in the UI and the menu bar app.')}>
              <input value={cfg.name} onChange={(e) => edit((c) => void (c.name = e.target.value))} />
            </Field>
            <Field label={t('HTML5 outputs')} hint={t('Browser sources (OBS, vMix, CasparCG) can play /output/<channel> without logging in.')}>
              <label className="check">
                <input type="checkbox" checked={cfg.publicOutputs} onChange={(e) => edit((c) => void (c.publicOutputs = e.target.checked))} />
                {t('Public output pages')}
              </label>
            </Field>
          </div>
        </Card>
      )}

      {section === 'network' && <Networks info={info} cfg={cfg} edit={edit} det={det} />}

      {section === 'time' && <TimePage cfg={cfg} edit={edit} det={det} zone={info.timezone} timezones={TIMEZONES} onRefresh={() => void detect()} />}

      {section === 'sync' && <VideoSync cfg={cfg} edit={edit} det={det} onRefresh={() => void detect()} />}

      {section === 'updates' && (
        <>
          <Updates Card={Card} />
          <LibraryUpdates Card={Card} />
        </>
      )}

      {section === 'system' && (
        <Card icon={<Cpu size={16} />} title={t('System')}>
          <dl className="sys-list">
            <dt>{t('Version')}</dt>
            <dd>
              Strata {info.version} · {info.os}/{info.arch}
            </dd>
            <dt>{t('Machine')}</dt>
            <dd>{info.hostname}</dd>
            <dt>{t('Data folder')}</dt>
            <dd>
              <code>{info.dataDir}</code>
            </dd>
            <dt>{t('Listening on')}</dt>
            <dd>
              <code>{info.bind}</code>
            </dd>
            <dt>{t('CPU threads')}</dt>
            <dd>{info.cpus}</dd>
            <dt>{t('Running for')}</dt>
            <dd>{uptime(info.uptimeSecs)}</dd>
          </dl>
        </Card>
      )}

      <SaveBar dirty={dirty} saving={saving} onSave={save} onDiscard={discard} />
    </SidePage>
  );
}
