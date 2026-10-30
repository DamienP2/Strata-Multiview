// The variables and pictures of a Text, edited in its Content box: a part
// of the text made a variable ({{key}}, a field the Operator fills in), a
// picture in the line (<img=src|height>, engine/rich.ts) the Operator may
// change when exposed. The chips of the box are selected to set them here.
import { useState, type ReactNode } from 'react';
import { ImagePlus, Radio, Trash2, X } from 'lucide-react';
import { useShallow } from 'zustand/react/shallow';
import { useDesigner } from '../designer/designerStore';
import { AssetLibrary } from '../designer/AssetPicker';
import { imageTags, imgTag, imgValue, plain, type ImgHeight } from '../engine/rich';
import { newFieldKey } from '../engine/settings';
import type { Template } from '../engine/types';
import { Segmented, TextInput } from '../ui/fields';
import type { Tokens } from '../ui/RichText';
import { t } from '../i18n';

const single = (s: string) => /^\{\{\s*(\w+)\s*\}\}$/.exec(s.trim())?.[1];
const fileName = (src: string) => decodeURIComponent(src.split('?')[0].split('/').pop() || src).replace(/\.[a-z0-9]+$/i, '');
/** Other layers still showing {{key}}. */
const usedElsewhere = (tpl: Template, layerId: string, key: string) => tpl.layers.some((l) => l.id !== layerId && JSON.stringify(l).includes(`{{${key}}}`));

/** The chips of a Text's Content box and the settings of the selected one. */
export function useTextTokens(layerId: string | undefined, markup: string, setMarkup: (v: string) => void): { tokens?: Tokens; panel: ReactNode } {
  const { template, update } = useDesigner(useShallow((s) => ({ template: s.template, update: s.update })));
  const [selected, select] = useState<string | null>(null);
  const [picking, setPicking] = useState<{ value: string; done: (src: string | null) => void } | null>(null);
  if (!template || !layerId) return { panel: null };
  // One undo step with the text's own edit.
  const coalesce = `text-${layerId}`;
  const field = (key: string) => template.fields.find((f) => f.key === key);
  const imgs = imageTags(markup);
  const pick = (value: string) => new Promise<string | null>((done) => setPicking({ value, done }));

  const tokens: Tokens = {
    label: (tok) => {
      if (tok.startsWith('img:')) {
        const v = imgs[+tok.slice(4)]?.value;
        if (!v) return '▣';
        const src = imgValue(v).src;
        const key = single(src);
        return `▣ ${key ? (field(key)?.label ?? key) : fileName(src)}`;
      }
      // A variable shows its text (the value set so far); its name in the tooltip.
      const key = single(tok)!;
      const f = field(key);
      return f?.default || f?.label || key;
    },
    title: (tok) => {
      const key = single(tok);
      return key ? (field(key)?.label ?? key) : '';
    },
    selected,
    onSelect: select,
    makeVariable: (text) => {
      const n = template.fields.length + 1;
      const label = t('Variable {n}', { n });
      const key = newFieldKey(template, `var ${n}`);
      update((tpl) => void tpl.fields.push({ key, label, kind: 'text', default: plain(text) }), coalesce);
      return `{{${key}}}`;
    },
    pickImage: async () => {
      const src = await pick('');
      return src ? imgTag(src, 'size') : null;
    },
  };

  // Its variables: name, value, back to text.
  const keys = [...new Set([...markup.matchAll(/\{\{\s*(\w+)\s*\}\}/g)].map((m) => m[1]))].filter((k) => field(k) && !field(k)!.binding?.source && field(k)!.kind !== 'image');
  const backToText = (key: string) => {
    const f = field(key);
    if (!f) return;
    setMarkup(markup.split(`{{${key}}}`).join(f.default));
    update((tpl) => {
      const l = tpl.layers.find((x) => x.id === layerId) as { varStyles?: Record<string, unknown> } | undefined;
      if (l?.varStyles) delete l.varStyles[key];
      if (!usedElsewhere(tpl, layerId, key)) tpl.fields = tpl.fields.filter((x) => x.key !== key);
    }, coalesce);
    if (selected === `{{${key}}}`) select(null);
  };
  const list = keys.length ? (
    <div className="tv-vars">
      {keys.map((key) => {
        const f = field(key)!;
        const edit = (patch: Partial<typeof f>) => update((tpl) => void Object.assign(tpl.fields.find((x) => x.key === key)!, patch), `field-${key}`);
        return (
          <div key={key} className={`tv-var ${selected === `{{${key}}}` ? 'on' : ''}`}>
            <i />
            <TextInput value={f.label} commitOnBlur onChange={(x) => x.trim() && edit({ label: x.trim() })} />
            <TextInput value={f.default} onChange={(x) => edit({ default: x })} />
            <button className="icon-btn ghost" title={t('Back to text')} onClick={() => backToText(key)}>
              <X size={13} />
            </button>
          </div>
        );
      })}
    </div>
  ) : null;
  let panel: ReactNode = null;
  const n = selected?.startsWith('img:') ? +selected.slice(4) : -1;
  const tag = n >= 0 ? imgs[n] : undefined;
  if (tag) {
    const { src, height } = imgValue(tag.value);
    const key = single(src);
    const f = key ? field(key) : undefined;
    const file = f ? f.default : src;
    const put = (s: string, h: ImgHeight) => setMarkup(markup.slice(0, tag.start) + imgTag(s, h) + markup.slice(tag.end));
    const setFile = async () => {
      const x = await pick(file);
      if (!x) return;
      if (f) update((tpl) => void (tpl.fields.find((y) => y.key === f.key)!.default = x), `field-${f.key}`);
      else put(x, height);
    };
    const toggleOp = () => {
      if (f) {
        put(f.default, height);
        if (!usedElsewhere(template, layerId, f.key)) update((tpl) => void (tpl.fields = tpl.fields.filter((y) => y.key !== f.key)), coalesce);
        return;
      }
      const k = newFieldKey(template, `picture ${imgs.length}`);
      update((tpl) => void tpl.fields.push({ key: k, label: t('Picture {n}', { n: n + 1 }), kind: 'image', default: src }), coalesce);
      put(`{{${k}}}`, height);
    };
    panel = (
      <div className="tv-pop">
        <div className="tv-pop-h">
          <ImagePlus size={12} /> {t('Picture')}
          <button
            className="tv-pop-x"
            onClick={() => {
              setMarkup(markup.slice(0, tag.start) + markup.slice(tag.end));
              if (f && !usedElsewhere(template, layerId, f.key)) update((tpl) => void (tpl.fields = tpl.fields.filter((y) => y.key !== f.key)), coalesce);
              select(null);
            }}
          >
            <Trash2 size={12} /> {t('Remove')}
          </button>
        </div>
        <div className="tv-row">
          <span>{t('File')}</span>
          <button className="tv-file" onClick={() => void setFile()} title={file}>
            {file ? <img src={file} alt="" /> : <ImagePlus size={14} />}
            <em>{file ? fileName(file) : t('Choose a picture')}</em>
          </button>
        </div>
        <div className="tv-row">
          <span>{t('Height')}</span>
          <Segmented
            value={height}
            onChange={(h) => put(src, h as ImgHeight)}
            options={[
              { value: 'cap', label: t('Capitals'), title: t('Height of the capitals') },
              { value: 'size', label: t('Text'), title: t('Font size') },
              { value: 'line', label: t('Line'), title: t('Height of the line') },
            ]}
          />
        </div>
        {f && (
          <label className="tv-row">
            <span>{t('Name')}</span>
            <TextInput value={f.label} commitOnBlur onChange={(x) => x.trim() && update((tpl) => void (tpl.fields.find((y) => y.key === f.key)!.label = x.trim()), `field-${f.key}`)} />
          </label>
        )}
        <div className="tv-row">
          <span>{t('Operator')}</span>
          <button className={`wk-chip op ${f ? 'on' : ''}`} onClick={toggleOp}>
            <Radio size={12} /> {f ? t('Exposed to the Operator') : t('Expose to the Operator')}
          </button>
        </div>
      </div>
    );
  }
  return {
    tokens,
    panel: (
      <>
        {list}
        {panel}
        {picking && (
          <AssetLibrary
            title={t('Choose a picture')}
            value={picking.value}
            onClose={() => (picking.done(null), setPicking(null))}
            onChoose={(src) => (picking.done(src), setPicking(null))}
          />
        )}
      </>
    ),
  };
}
