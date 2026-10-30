import { Network, Router, TriangleAlert } from 'lucide-react';
import { t } from '../i18n';
import { hardwareOf, speedLabel } from '../hardware/model';
import { emptyNetwork, type HardwareInfo, type NetworkRoles, type Nic, type ServerConfig, type SystemInfo } from '../outputs/types';
import { Card } from './ui';
import { joinAddr, splitAddr } from './serverConfig';
import '../hardware/hardware.css';

/*
 * Settings › Networks: which network card does what (web interface and
 * API, NDI, the ST 2110 red and blue networks, MOS), then the cards of
 * the machine. Saving restarts the server on its new addresses.
 */

type RoleKey = 'control' | 'ndi' | 'mediaRed' | 'mediaBlue' | 'mos' | 'ptp' | 'buttons';

const ipOf = (n?: Nic) => n?.ipv4[0]?.split('/')[0];

export function Networks({ info, cfg, edit, det }: { info: SystemInfo; cfg: ServerConfig; edit: (fn: (c: ServerConfig) => void) => void; det: HardwareInfo | null }) {
  const net: NetworkRoles = { ...emptyNetwork(), ...hardwareOf(structuredClone(cfg)).network };
  const nics = det?.nics ?? [];
  const usable = nics.filter((n) => n.ipv4.length);
  const [bindIp, bindPort] = splitAddr(cfg.bind ?? info.bind);
  const anyBind = !bindIp || bindIp === '0.0.0.0' || bindIp === '::';
  const set = (k: keyof NetworkRoles, v: string) =>
    edit((c) => {
      c.hardware = hardwareOf(c);
      c.hardware.network = { ...emptyNetwork(), ...c.hardware.network, [k]: v };
    });
  const nicOf = (name: string) => nics.find((n) => n.name === name);
  const all = usable.filter((n) => n.up !== false).map(ipOf).filter(Boolean) as string[];

  // The web interface: a card by name (the control role), or every card;
  // an address set by hand before (bind) shows as it is.
  const control = net.control || (anyBind ? '' : `ip:${bindIp}`);
  const setControl = (v: string) =>
    edit((c) => {
      c.hardware = hardwareOf(c);
      c.hardware.network = { ...emptyNetwork(), ...c.hardware.network, control: v.startsWith('ip:') ? '' : v };
      c.bind = joinAddr(v.startsWith('ip:') ? v.slice(3) : '0.0.0.0', bindPort || '8080');
    });

  const picker = (value: string, onChange: (v: string) => void, none: string, label: string, extra?: React.ReactNode) => (
    <select value={value} aria-label={label} onChange={(e) => onChange(e.target.value)}>
      <option value="">{none}</option>
      {extra}
      {value && !value.startsWith('ip:') && !usable.some((n) => n.name === value) && <option value={value}>{t('{name} (not found)', { name: value })}</option>}
      {usable.map((n) => (
        <option key={n.name} value={n.name}>
          {[n.name, n.speed ? speedLabel(n.speed) : ipOf(n)].filter(Boolean).join(' · ')}
        </option>
      ))}
    </select>
  );
  const address = (name: string, every: boolean) => (name ? (ipOf(nicOf(name)) ?? '—') : every ? all.join(' · ') || '—' : '—');

  const rows: { key: RoleKey; label: string; cell: React.ReactNode; addr: string; port: React.ReactNode }[] = [
    {
      key: 'control',
      label: t('Web interface and API'),
      cell: picker(control, setControl, t('Every interface'), t('Web interface and API'), control.startsWith('ip:') ? <option value={control}>{control.slice(3)}</option> : null),
      addr: [...new Set([control.startsWith('ip:') ? control.slice(3) : address(net.control, true), '127.0.0.1'])].join(' · '),
      port: <input type="number" min={1} max={65535} value={bindPort} aria-label={t('Port')} onChange={(e) => edit((c) => void (c.bind = joinAddr(control.startsWith('ip:') ? control.slice(3) : '0.0.0.0', e.target.value)))} />,
    },
    { key: 'ndi', label: 'NDI', cell: picker(net.ndi, (v) => set('ndi', v), t('Every interface'), 'NDI'), addr: address(net.ndi, true), port: <span className="muted">—</span> },
    { key: 'mediaRed', label: t('2110 red'), cell: picker(net.mediaRed, (v) => set('mediaRed', v), t('Not set'), t('2110 red')), addr: address(net.mediaRed, false), port: <span className="muted">—</span> },
    { key: 'mediaBlue', label: t('2110 blue'), cell: picker(net.mediaBlue, (v) => set('mediaBlue', v), t('Not set'), t('2110 blue')), addr: address(net.mediaBlue, false), port: <span className="muted">—</span> },
    {
      key: 'mos',
      label: 'MOS',
      cell: picker(net.mos ?? '', (v) => set('mos', v), t('Every interface'), 'MOS'),
      addr: address(net.mos ?? '', true),
      port: (
        <span className="net-ports">
          <input type="number" min={1} max={65535} value={cfg.mos.lowerPort} aria-label={t('MOS lower port')} onChange={(e) => edit((c) => void (c.mos.lowerPort = +e.target.value))} />
          <input type="number" min={1} max={65535} value={cfg.mos.upperPort} aria-label={t('MOS upper port')} onChange={(e) => edit((c) => void (c.mos.upperPort = +e.target.value))} />
        </span>
      ),
    },
    {
      // OSC and TCP control of Bitfocus Buttons (API page), on every interface.
      key: 'buttons',
      label: 'OSC · TCP',
      cell: <span className="muted">{t('Every interface')}</span>,
      addr: address('', true),
      port: (
        <span className="net-ports">
          <input type="number" min={1} max={65535} value={cfg.control?.oscPort ?? 9000} aria-label={t('OSC port')} onChange={(e) => edit((c) => void (c.control = { oscEnabled: false, tcpEnabled: false, tcpPort: 9001, allow: [], ...c.control, oscPort: +e.target.value }))} />
          <input type="number" min={1} max={65535} value={cfg.control?.tcpPort ?? 9001} aria-label={t('TCP port')} onChange={(e) => edit((c) => void (c.control = { oscEnabled: false, oscPort: 9000, tcpEnabled: false, allow: [], ...c.control, tcpPort: +e.target.value }))} />
        </span>
      ),
    },
  ];

  /** The roles a card carries (a role on every card counts for each). */
  const rolesOf = (n: Nic) => {
    const on = n.ipv4.length > 0 && n.up !== false;
    const out: { label: string; cls: string }[] = [];
    if (net.control === n.name || (!net.control && anyBind && on)) out.push({ label: 'Web', cls: 'on' });
    if (net.ndi === n.name || (!net.ndi && on)) out.push({ label: 'NDI', cls: 'on' });
    if (cfg.mos.enabled && (net.mos === n.name || (!net.mos && on))) out.push({ label: 'MOS', cls: 'on' });
    if (net.mediaRed === n.name) out.push({ label: t('2110 red'), cls: 'red' });
    if (net.mediaBlue === n.name) out.push({ label: t('2110 blue'), cls: 'blue' });
    if (net.ptp === n.name) out.push({ label: 'PTP', cls: 'on' });
    return out;
  };

  return (
    <div className="net-page">
      {det?.ndiRestart && (
        <p className="hw-note warn hw-banner">
          <TriangleAlert size={14} /> {t('The NDI network settings changed: they apply when the server restarts (the NDI runtime reads them once).')}
        </p>
      )}
      <Card icon={<Router size={16} />} title={t('Roles')}>
        <table className="fio-table net-table">
          <thead>
            <tr>
              <th style={{ width: 200 }}>{t('Role')}</th>
              <th>{t('Card')}</th>
              <th>{t('Address')}</th>
              <th>{t('Port')}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.key}>
                <td>
                  <b>{r.label}</b>
                </td>
                <td>{r.cell}</td>
                <td className="mono">{r.addr}</td>
                <td>{r.port}</td>
              </tr>
            ))}
            <tr>
              <td>
                <b>{t('NDI machine name')}</b>
              </td>
              <td>
                <input className="net-discovery" value={net.ndiMachine ?? ''} placeholder={info.hostname ? info.hostname.toUpperCase() : undefined} spellCheck={false} aria-label={t('NDI machine name')} onChange={(e) => set('ndiMachine', e.target.value)} />
              </td>
              <td />
              <td />
            </tr>
            <tr>
              <td>
                <b>{t('NDI Discovery Server')}</b>
              </td>
              <td>
                <input className="net-discovery" value={net.ndiDiscovery} placeholder="10.0.0.2" spellCheck={false} aria-label={t('NDI Discovery Server')} onChange={(e) => set('ndiDiscovery', e.target.value)} />
              </td>
              <td />
              <td />
            </tr>
          </tbody>
        </table>
        {net.mediaRed && net.mediaRed === net.mediaBlue && (
          <p className="hw-note warn">
            <TriangleAlert size={13} /> {t('Red and blue must be two different interfaces.')}
          </p>
        )}
      </Card>

      <Card icon={<Network size={16} />} title={t('Network cards')}>
        <table className="fio-table">
          <thead>
            <tr>
              <th>{t('Card')}</th>
              <th>{t('IPv4 address')}</th>
              <th>{t('Link')}</th>
              <th>MTU</th>
              <th>MAC</th>
              <th>{t('Roles')}</th>
            </tr>
          </thead>
          <tbody>
            {!det && (
              <tr>
                <td colSpan={6} className="muted">
                  {t('Detecting…')}
                </td>
              </tr>
            )}
            {nics.map((n) => {
              const roles = rolesOf(n);
              const up = n.up !== false && n.ipv4.length > 0;
              return (
                <tr key={n.name}>
                  <td>
                    <span className="net-name">
                      <span className={`fio-dot ${up ? 'ok' : ''}`} />
                      <b className="mono">{n.name}</b>
                      <span className="muted small">{n.model ?? (n.kind === 'wifi' ? 'Wi-Fi' : n.kind === 'virtual' ? t('virtual') : n.kind === 'ethernet' ? 'Ethernet' : '')}</span>
                    </span>
                  </td>
                  <td className={`mono ${n.ipv4.length ? '' : 'muted'}`}>{n.ipv4.join(' · ') || '—'}</td>
                  <td className={n.up === false ? 'muted' : ''}>{n.up === false ? t('disconnected') : speedLabel(n.speed) || '—'}</td>
                  <td className={n.mtu ? '' : 'muted'}>{n.mtu ?? '—'}</td>
                  <td className="mono">{n.mac ?? '—'}</td>
                  <td>
                    {roles.length ? (
                      <div className="fio-chips">
                        {roles.map((r) => (
                          <span key={r.label} className={`fio-chip ${r.cls}`}>
                            {r.label}
                          </span>
                        ))}
                      </div>
                    ) : (
                      <span className="muted">—</span>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </Card>
    </div>
  );
}
