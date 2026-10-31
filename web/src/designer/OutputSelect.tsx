// The output a multiview is shown on, in the Designer's top bar: the channels showing it, the free
// ones, those showing another multiview. Picking
// one shows the multiview there at once; « No output » takes it off the channel it was on.
import { ExternalLink } from 'lucide-react';
import { memo } from 'react';
import { useShallow } from 'zustand/react/shallow';
import type { ChannelInfo, Project } from '../engine/types';
import { t } from '../i18n';
import { useApp } from '../store';
import { Dropdown, type DropOption } from '../ui/Dropdown';
import { confirmDialog } from '../ui/dialogs';

export const OutputSelect = memo(function OutputSelect({ project }: { project: Project }) {
  const { channels, projects, loadOnChannel, toast } = useApp(useShallow((s) => ({ channels: s.channels, projects: s.projects, loadOnChannel: s.loadOnChannel, toast: s.toast })));
  const mine = channels.filter((c) => c.projectId === project.id);
  const current = mine[0] ?? null;
  const nameOf = (id?: string | null) => projects.find((p) => p.id === id)?.name ?? t('another multiview');
  const pick = async (id: string) => {
    try {
      if (!id) {
        if (!current) return;
        if (!(await confirmDialog({ title: t('Take “{name}” off {channel}?', { name: project.name, channel: current.name }), message: t('{channel} shows its still picture instead.', { channel: current.name }), confirm: t('Take off') }))) return;
        await loadOnChannel(current.id, null);
        return;
      }
      const c = channels.find((x) => x.id === id);
      if (!c || c.projectId === project.id) return;
      if (c.projectId && !(await confirmDialog({ title: t('{channel} shows “{other}”', { channel: c.name, other: nameOf(c.projectId) }), message: t('Show “{name}” on it instead?', { name: project.name }), confirm: t('Show here'), danger: true }))) return;
      await loadOnChannel(c.id, project.id);
    } catch (e) {
      toast((e as Error).message, 'error');
    }
  };
  const dot = (c?: ChannelInfo) => <span className={`opx-dot ${c && c.projectId === project.id ? 'shown' : !c ? 'none' : ''}`} />;
  const format = (c: ChannelInfo) => `${c.height}${c.interlaced ? 'i' : 'p'}${+(c.frameRate.num / Math.max(1, c.frameRate.den)).toFixed(2)}`;
  const kinds = (c: ChannelInfo) => [...new Set(c.outputs.map((o) => o.split(' · ')[0]))].join(' · ') || t('No destination');
  const item = (c: ChannelInfo, group: string, sub: string): DropOption<string> => ({ value: c.id, label: c.name, sub, icon: dot(c), note: format(c), detail: format(c), group });
  const options: DropOption<string>[] = [
    { value: '', label: t('No output'), sub: t('Shown on no channel'), icon: dot() },
    ...mine.map((c) => item(c, t('Shows this multiview'), kinds(c))),
    ...channels.filter((c) => !c.projectId).map((c) => item(c, t('Free'), kinds(c))),
    ...channels.filter((c) => c.projectId && c.projectId !== project.id).map((c) => item(c, t('Shows another multiview'), `« ${nameOf(c.projectId)} »`)),
  ];
  return (
    <div className="opx-top">
      <Dropdown value={current?.id ?? ''} options={options} title={t('Output')} className="opx-output" onChange={(v) => void pick(v)} />
      {current && (
        <a className="top-tool opx-html5" href={`/output/${encodeURIComponent(current.id)}`} target="_blank" rel="noreferrer" title={t('Open the web page of {channel}', { channel: current.name })}>
          HTML5 <ExternalLink size={12} />
        </a>
      )}
    </div>
  );
});
