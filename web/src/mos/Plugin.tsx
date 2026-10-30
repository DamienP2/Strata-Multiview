import { GripVertical, Send } from 'lucide-react';
import { Logo } from '../ui/Logo';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { defaultData, holdFrame } from '../engine/evaluate';
import { LiveTime } from '../engine/LiveTime';
import { Scene, Viewport } from '../engine/Scene';
import type { Data, Project, ProjectSummary, Template } from '../engine/types';
import { FieldInput } from '../operator/Operator';
import { api, setApiKey, thumbnailUrl } from '../store';
import { t } from '../i18n';

/*
 * HTML5 MOS plugin (MOS 2.8.5): runs inside the newsroom system (Saga, iNews,
 * ENPS, Octopus…). The journalist picks a template, fills its fields and
 * sends the item to the story — by button (postMessage of an <ncsItem>) or
 * by dragging it into the script. Opening an existing item fills the form
 * back from its payload.
 *
 * URL: /mos/plugin?key=pcg_… (API key of an operator account: the plugin
 * cannot use the web app's login cookie inside another application).
 */

const SCHEMA = 'https://stratagraphics.io/mos/v1';
const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

interface Info {
  mosId: string;
  projectId: string;
}

interface Edited {
  itemId: string;
  project: string;
  template: string;
  slug: string;
  data: Data;
}

/** Parses an <ncsItem> sent by the newsroom system (editing an item). */
function parseNcsItem(xml: string): Edited | null {
  const doc = new DOMParser().parseFromString(xml, 'text/xml');
  const item = doc.querySelector('item');
  const payload = doc.querySelector('strata, pulsecg');
  if (!item || !payload) return null;
  const data: Data = {};
  payload.querySelectorAll('field').forEach((f) => (data[f.getAttribute('key') ?? ''] = f.textContent ?? ''));
  return {
    itemId: item.querySelector('itemID')?.textContent ?? '',
    project: payload.getAttribute('project') ?? '',
    template: payload.getAttribute('template') ?? '',
    slug: item.querySelector('itemSlug')?.textContent ?? '',
    data,
  };
}

function itemXml(info: Info, project: Project, t: Template, data: Data, slug: string, itemId: string): string {
  const fps = project.frameRate.num / project.frameRate.den || 25;
  const summary = t.fields
    .map((f) => data[f.key])
    .filter(Boolean)
    .slice(0, 3)
    .join(' / ');
  const fields = t.fields.map((f) => `<field key="${esc(f.key)}">${esc(data[f.key] ?? '')}</field>`).join('');
  return (
    `<item>` +
    (itemId ? `<itemID>${esc(itemId)}</itemID>` : '') +
    `<itemSlug>${esc(slug)}</itemSlug>` +
    `<objID>${esc(`${project.id}/${t.id}`)}</objID>` +
    `<mosID>${esc(info.mosId)}</mosID>` +
    `<mosAbstract>${esc(`${t.name}${summary ? `: ${summary}` : ''}`)}</mosAbstract>` +
    `<objDur>${Math.round(t.duration)}</objDur><objTB>${Math.round(fps)}</objTB>` +
    `<mosExternalMetadata><mosScope>PLAYLIST</mosScope><mosSchema>${SCHEMA}</mosSchema><mosPayload>` +
    `<strata project="${esc(project.id)}" template="${esc(t.id)}" zone="${esc(t.zone)}">${fields}</strata>` +
    `</mosPayload></mosExternalMetadata></item>`
  );
}

function Preview({ project, template, data }: { project: Project; template: Template; data: Data }) {
  const ref = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(0.2);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setScale(el.clientWidth / project.width));
    ro.observe(el);
    return () => ro.disconnect();
  }, [project.width]);
  const fps = project.frameRate.num / project.frameRate.den || 25;
  const f = holdFrame(template);
  return (
    <div ref={ref} className="mos-preview" style={{ aspectRatio: `${project.width} / ${project.height}` }}>
      <Viewport width={project.width} height={project.height} scale={scale}>
        <LiveTime ageSecs={f / fps} clientNow={Date.now()} lang={project.language || 'fr'}>
          <Scene template={template} frame={f} data={data} />
        </LiveTime>
      </Viewport>
    </div>
  );
}

export function MosPlugin() {
  const params = new URLSearchParams(location.search);
  const key = params.get('key');
  const [info, setInfo] = useState<Info | null>(null);
  const [error, setError] = useState('');
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [projectId, setProjectId] = useState('');
  const [project, setProject] = useState<Project | null>(null);
  const [templateId, setTemplateId] = useState('');
  const [data, setData] = useState<Data>({});
  const [slug, setSlug] = useState('');
  // The rundown name follows the content until the journalist types one.
  const [slugEdited, setSlugEdited] = useState(false);
  const [itemId, setItemId] = useState('');
  const [sent, setSent] = useState(false);
  const pending = useRef<Edited | null>(null);

  useEffect(() => {
    document.documentElement.classList.add('mos-mode');
    setApiKey(key);
    Promise.all([api<Info>('GET', '/api/mos/info'), api<ProjectSummary[]>('GET', '/api/projects')])
      .then(([i, ps]) => {
        setInfo(i);
        setProjects(ps);
        setProjectId((cur) => cur || (ps.some((p) => p.id === i.projectId) ? i.projectId : (ps[0]?.id ?? '')));
      })
      .catch((e) => setError(key ? t('Cannot reach Strata: {error}', { error: (e as Error).message }) : t('Add ?key=pcg_… (an API key) to the plugin URL.')));
    // Editing an existing item: the newsroom system sends it to us.
    const onMessage = (ev: MessageEvent) => {
      if (typeof ev.data !== 'string' || !ev.data.includes('ncsItem')) return;
      const e = parseNcsItem(ev.data);
      if (!e) return;
      pending.current = e;
      setItemId(e.itemId);
      setSlug(e.slug);
      setSlugEdited(true);
      setProjectId(e.project);
      setTemplateId(e.template);
      setData(e.data);
    };
    addEventListener('message', onMessage);
    if (window.parent !== window) window.parent.postMessage('<mos><ncsReqAppInfo/></mos>', '*');
    return () => removeEventListener('message', onMessage);
  }, []);

  useEffect(() => {
    if (!projectId) return;
    api<Project>('GET', `/api/projects/${encodeURIComponent(projectId)}`)
      .then((p) => {
        setProject(p);
        const keep = pending.current?.project === p.id ? pending.current.template : '';
        setTemplateId((cur) => (p.templates.some((t) => t.id === (keep || cur)) ? keep || cur : (p.templates[0]?.id ?? '')));
      })
      .catch((e) => setError((e as Error).message));
  }, [projectId]);

  const template = useMemo(() => project?.templates.find((t) => t.id === templateId), [project, templateId]);
  useEffect(() => {
    if (!template) return;
    const p = pending.current;
    if (p && p.template === template.id) {
      setData({ ...defaultData(template), ...p.data });
      pending.current = null;
    } else {
      setData(defaultData(template));
      setSlugEdited(false);
      setItemId('');
    }
    setSent(false);
  }, [template?.id]);

  const first = template?.fields.map((f) => data[f.key]).find((v) => v?.trim());
  const autoSlug = template ? `${template.name}${first ? ` – ${first.split('\n')[0]}` : ''}` : '';
  const shownSlug = slugEdited ? slug : autoSlug;
  const xml = info && project && template ? itemXml(info, project, template, data, shownSlug || template.name, itemId) : '';
  const send = () => {
    if (!xml) return;
    if (window.parent !== window) window.parent.postMessage(`<mos><ncsItem>${xml}</ncsItem></mos>`, '*');
    else navigator.clipboard?.writeText(`<mos><ncsItem>${xml}</ncsItem></mos>`);
    setSent(true);
  };

  if (error) return <div className="mos-plugin mos-error">{error}</div>;
  if (!info) return <div className="mos-plugin loading">{t('Connecting…')}</div>;
  return (
    <div className="mos-plugin">
      <header>
        <Logo size={20} />
        <strong>Strata</strong>
        <select value={projectId} onChange={(e) => setProjectId(e.target.value)}>
          {projects.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </header>
      <div className="mos-templates">
        {project?.templates.map((tpl) => (
          <button key={tpl.id} className={tpl.id === templateId ? 'on' : ''} onClick={() => setTemplateId(tpl.id)}>
            <img src={`${thumbnailUrl(project.id, tpl)}&key=${encodeURIComponent(key ?? '')}`} alt="" loading="lazy" />
            <span>{tpl.name}</span>
          </button>
        ))}
      </div>
      {project && template && (
        <div className="mos-editor">
          <Preview project={project} template={template} data={data} />
          <label className="form-row">
            <span>{t('Item name in the rundown')}</span>
            <input
              value={shownSlug}
              onChange={(e) => {
                setSlug(e.target.value);
                setSlugEdited(e.target.value !== '');
              }}
            />
          </label>
          {template.fields.map((f) => (
            <label key={f.key} className="form-row">
              <span>{f.label}</span>
              <FieldInput field={f} value={data[f.key] ?? ''} onChange={(v) => setData({ ...data, [f.key]: v })} />
            </label>
          ))}
          <div className="mos-actions">
            <span
              className="mos-drag"
              draggable
              onDragStart={(e) => {
                // NCS drag and drop: the MOS item as XML.
                e.dataTransfer.setData('text', `<mos><ncsItem>${xml}</ncsItem></mos>`);
                e.dataTransfer.setData('text/plain', `<mos><ncsItem>${xml}</ncsItem></mos>`);
              }}
              title={t('Drag into the story')}
            >
              <GripVertical size={14} /> {t('Drag into the story')}
            </span>
            <button className="btn primary" onClick={send}>
              <Send size={14} /> {itemId ? t('Update the item') : t('Add to the story')}
            </button>
          </div>
          {sent && <p className="hint">{t('Sent to the newsroom system.')}</p>}
        </div>
      )}
    </div>
  );
}
