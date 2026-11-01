// Settings › Integrations: the server in a broadcast IP plant (AMWA NMOS IS-04 / IS-05) and under
// a control system (Ember+). Mirrors crates/strata-mv-server/src/nmos.rs and ember.rs.
import { Network, SlidersVertical } from 'lucide-react';
import { useEffect, useState } from 'react';
import { t } from '../i18n';
import type { ServerConfig } from '../outputs/types';
import { api } from '../store';
import { Card, Field } from './ui';

type Edit = (fn: (c: ServerConfig) => void) => void;

interface NmosInfo {
  nodeId: string;
  nodeApi: string;
  connectionApi: string;
  senders: number;
  receivers: number;
}

export function Integrations({ cfg, edit, saved }: { cfg: ServerConfig; edit: Edit; saved?: ServerConfig }) {
  const nmos = cfg.nmos ?? { enabled: false, registry: '', host: '' };
  const ember = cfg.ember ?? { enabled: false, port: 9000 };
  const setN = (p: Partial<typeof nmos>) => edit((c) => void (c.nmos = { ...nmos, ...p }));
  const setE = (p: Partial<typeof ember>) => edit((c) => void (c.ember = { ...ember, ...p }));
  const [info, setInfo] = useState<NmosInfo | null>(null);
  const live = saved?.nmos?.enabled;
  useEffect(() => {
    if (!live) return setInfo(null);
    api<NmosInfo>('GET', '/api/nmos')
      .then(setInfo)
      .catch(() => setInfo(null));
  }, [live]);
  return (
    <>
      <Card icon={<Network size={16} />} title="AMWA NMOS">
        <div className="set-grid">
          <Field label="IS-04 / IS-05" hint={t('Controllers and registries of the IP plant see the server: its ST 2110 outputs as senders, its UDP / RTP inputs as receivers.')}>
            <label className="check">
              <input type="checkbox" checked={nmos.enabled} onChange={(e) => setN({ enabled: e.target.checked })} />
              {t('On')}
            </label>
          </Field>
          <Field label={t('Registry')} hint={t('Its address (IS-04 registration API). Empty: no registration, the Node API is still served for direct control.')}>
            <input value={nmos.registry} placeholder="http://registry.exemple.fr:8010" spellCheck={false} onChange={(e) => setN({ registry: e.target.value.trim() })} />
          </Field>
          <Field label={t('Address announced')} hint={t('The address controllers reach the server at. Empty: the first one of the machine.')}>
            <input value={nmos.host} placeholder="10.0.0.20" spellCheck={false} onChange={(e) => setN({ host: e.target.value.trim() })} />
          </Field>
        </div>
        {info && (
          <p className="muted">
            {t('Node {id}: {s} sender(s), {r} receiver(s).', { id: info.nodeId, s: info.senders, r: info.receivers })} <code>{info.nodeApi}</code> · <code>{info.connectionApi}</code>
          </p>
        )}
        <p className="muted">
          {t(
            'Activating a sender changes its destination or turns it on or off; a receiver, the multicast group its input joins. Both are saved in the settings. The NMOS APIs are open (no key), as is usual on a media network.',
          )}
        </p>
      </Card>

      <Card icon={<SlidersVertical size={16} />} title="Ember+">
        <div className="set-grid">
          <Field
            label={t('Provider')}
            hint={t('Control panels and control systems (VSM, Lawo, Cerebrum…) browse the server: multiview on air of each channel, state and faults of each input, acknowledgement.')}
          >
            <label className="check">
              <input type="checkbox" checked={ember.enabled} onChange={(e) => setE({ enabled: e.target.checked })} />
              {t('On')}
            </label>
          </Field>
          <Field label={t('TCP port')}>
            <input type="number" min={1} max={65535} value={ember.port} onChange={(e) => setE({ port: Math.max(1, Math.min(65535, Number(e.target.value) || 9000)) })} />
          </Field>
        </div>
        <p className="muted">{t('Tree: strata › info (name, version, uptime, faults, acknowledgeAll), channels › <channel> › multiview, inputs › <input> › live, status, alarms, loudness.')}</p>
      </Card>
    </>
  );
}
