import { Bold, Braces, ImagePlus, Italic, RemoveFormatting, Strikethrough, Underline } from 'lucide-react';
import { HexColorField } from './fields';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { imageIndex, imageTags, parse, type Run, type RunStyle } from '../engine/rich';
import { t } from '../i18n';

/*
 * Rich text field (like TinyMCE, kept small): select part of the text, then
 * bold, italic, underline, strike through or a colour. The value is the text
 * with its style tags (engine/rich.ts), so {{key}} and plain texts are kept.
 */

const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/\n/g, '<br>');
const attr = (s: string) => esc(s).replace(/"/g, '&quot;');

/** The variables ({{key}}) and pictures (<img>) of a text, shown as chips in the box. */
export interface Tokens {
  /** A chip's name: the variable's, or the picture's. */
  label: (tok: string) => string;
  /** A chip's tooltip. */
  title?: (tok: string) => string;
  /** The chip shown selected ('{{key}}' or 'img:n'). */
  selected?: string | null;
  onSelect?: (id: string | null) => void;
  /** Makes the selected text (maybe empty) a variable: its {{key}}, null to give up. */
  makeVariable?: (text: string) => string | null;
  /** Chooses a picture: its tag (<img=…>), null to give up. */
  pickImage?: () => Promise<string | null>;
}

/** The HTML of a chip. */
const chip = (tok: string, id: string, label: string, title = '') =>
  `<span class="rt-tok ${id.startsWith('img:') ? 'i' : 'v'}" contenteditable="false" data-tok="${attr(tok)}" data-id="${attr(id)}"${title ? ` title="${attr(title)}"` : ''}>${esc(label)}</span>`;

/** Tags → HTML for the editable box (with `tokens`: variables and pictures as chips). */
export function toHtml(markup: string, tokens?: Tokens): string {
  const imgs = imageTags(markup);
  const body = (text: string) => {
    if (!tokens) return esc(text);
    let h = '';
    let cur = '';
    const flush = () => {
      if (!cur) return;
      h += esc(cur).replace(/\{\{\s*(\w+)\s*\}\}/g, (_, k: string) => chip(`{{${k}}}`, `{{${k}}}`, tokens.label(`{{${k}}}`), tokens.title?.(`{{${k}}}`)));
      cur = '';
    };
    for (const c of text) {
      const n = imageIndex(c);
      if (n < 0 || !imgs[n]) {
        cur += c;
        continue;
      }
      flush();
      h += chip(`<img=${imgs[n].value}>`, `img:${n}`, tokens.label(`img:${n}`));
    }
    flush();
    return h;
  };
  return parse(markup)
    .map(({ text, style: s }) => {
      let h = body(text);
      if (s.color) h = `<font color="${s.color}">${h}</font>`;
      if (s.strike) h = `<s>${h}</s>`;
      if (s.underline) h = `<u>${h}</u>`;
      if (s.italic) h = `<i>${h}</i>`;
      if (s.bold) h = `<b>${h}</b>`;
      return h;
    })
    .join('');
}

const rgbHex = (c: string) => {
  const m = c.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)/);
  return m ? `#${[m[1], m[2], m[3]].map((x) => (+x).toString(16).padStart(2, '0')).join('')}` : c.startsWith('#') ? c : undefined;
};

/** The editable box's HTML → the text with its tags (`plain`: without style tags). */
export function fromHtml(root: HTMLElement, plain = false): string {
  const runs: Run[] = [];
  const push = (text: string, style: RunStyle) => {
    if (!text) return;
    const last = runs[runs.length - 1];
    if (last && JSON.stringify(last.style) === JSON.stringify(style)) last.text += text;
    else runs.push({ text, style: { ...style } });
  };
  const walk = (n: Node, style: RunStyle, first: { v: boolean }) => {
    if (n.nodeType === Node.TEXT_NODE) {
      push((n.textContent ?? '').replace(/ /g, ' ').replace(/[​]/g, ''), style);
      first.v = false;
      return;
    }
    if (!(n instanceof HTMLElement)) return;
    // A chip: the variable or picture it stands for.
    if (n.dataset.tok) {
      push(n.dataset.tok, style);
      first.v = false;
      return;
    }
    const tag = n.tagName;
    if (tag === 'BR') {
      push('\n', style);
      return;
    }
    // A new block (Enter in some browsers): a new line.
    if ((tag === 'DIV' || tag === 'P') && !first.v) push('\n', style);
    const s: RunStyle = { ...style };
    if (tag === 'B' || tag === 'STRONG') s.bold = true;
    if (tag === 'I' || tag === 'EM') s.italic = true;
    if (tag === 'U') s.underline = true;
    if (tag === 'S' || tag === 'STRIKE' || tag === 'DEL') s.strike = true;
    if (tag === 'FONT' && n.getAttribute('color')) s.color = rgbHex(n.getAttribute('color')!);
    const css = n.style;
    if (css.fontWeight === 'bold' || +css.fontWeight >= 600) s.bold = true;
    if (css.fontStyle === 'italic') s.italic = true;
    if (css.textDecorationLine?.includes('underline') || css.textDecoration?.includes('underline')) s.underline = true;
    if (css.textDecorationLine?.includes('line-through') || css.textDecoration?.includes('line-through')) s.strike = true;
    if (css.color) s.color = rgbHex(css.color);
    n.childNodes.forEach((c) => walk(c, s, first));
  };
  walk(root, {}, { v: true });
  // A trailing line break the box keeps for the caret.
  const last = runs[runs.length - 1];
  if (last?.text.endsWith('\n')) last.text = last.text.slice(0, -1);
  return runs
    .filter((r) => r.text)
    .map(({ text, style: s }) => {
      let t = text;
      if (plain) return t;
      if (s.color) t = `<c=${s.color}>${t}</c>`;
      if (s.strike) t = `<s>${t}</s>`;
      if (s.underline) t = `<u>${t}</u>`;
      if (s.italic) t = `<i>${t}</i>`;
      if (s.bold) t = `<b>${t}</b>`;
      return t;
    })
    .join('');
}

const SWATCHES = ['#ffffff', '#ff3b5c', '#facc15', '#22c55e', '#38bdf8', '#7c5cff', '#111111'];

export function RichInput({
  value,
  onChange,
  multiline,
  placeholder,
  maxLength,
  tokens,
  plain,
  bare,
}: {
  value: string;
  onChange: (v: string) => void;
  multiline?: boolean;
  placeholder?: string;
  /** Most characters that can be typed. */
  maxLength?: number;
  tokens?: Tokens;
  /** No style bar nor looks of its own: variables and pictures from a menu on right-click. */
  plain?: boolean;
  /** No style bar: the styles there are kept and shown (Ctrl+B / I / U still work). */
  bare?: boolean;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; text: string } | null>(null);
  const sent = useRef<string | null>(null);
  const [color, setColor] = useState('#ff3b5c');
  const [state, setState] = useState<Record<string, boolean>>({});
  // The value from outside (undo, another field…): the box shows it; its own edits are left as they are.
  // The chips' names (a variable renamed): drawn again.
  const names = tokens ? [...value.matchAll(/\{\{\s*(\w+)\s*\}\}/g)].map((m) => tokens.label(`{{${m[1]}}}`)).join('|') + imageTags(value).map((_, i) => tokens.label(`img:${i}`)).join('|') : '';
  const shownNames = useRef(names);
  useLayoutEffect(() => {
    if (box.current && (value !== sent.current || names !== shownNames.current)) {
      box.current.innerHTML = toHtml(value, tokens);
      sent.current = value;
      shownNames.current = names;
    }
  }, [value, names, tokens]);
  // The selected chip.
  useEffect(() => {
    box.current?.querySelectorAll<HTMLElement>('.rt-tok').forEach((el) => el.classList.toggle('on', el.dataset.id === tokens?.selected));
  });
  // Where the caret was (a picture is chosen in a window: put back after).
  const saved = useRef<Range | null>(null);
  const insert = (html: string) => {
    const el = box.current;
    if (!el) return;
    el.focus();
    const sel = document.getSelection();
    if (saved.current && sel) {
      sel.removeAllRanges();
      sel.addRange(saved.current);
    }
    if (!sel?.rangeCount || !el.contains(sel.anchorNode)) {
      const r = document.createRange();
      r.selectNodeContents(el);
      r.collapse(false);
      sel?.removeAllRanges();
      sel?.addRange(r);
    }
    document.execCommand('insertHTML', false, html);
    emit();
  };
  const selectionIn = () => {
    const sel = document.getSelection();
    return sel?.rangeCount && box.current?.contains(sel.anchorNode) ? sel : null;
  };
  const emit = () => {
    if (!box.current) return;
    const v = fromHtml(box.current, plain);
    sent.current = v;
    if (v !== value) onChange(v);
  };
  const refresh = () =>
    setState({
      bold: document.queryCommandState('bold'),
      italic: document.queryCommandState('italic'),
      underline: document.queryCommandState('underline'),
      strikeThrough: document.queryCommandState('strikeThrough'),
    });
  // At the most characters, typing stops (replacing a selection still works).
  const max = useRef(maxLength);
  max.current = maxLength;
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const before = (e: InputEvent) => {
      if (!max.current || !e.inputType.startsWith('insert') || e.inputType === 'insertFromPaste') return;
      const room = max.current - (el.textContent ?? '').length + (document.getSelection()?.toString().length ?? 0);
      if (room < Math.max(1, [...(e.data ?? '')].length)) e.preventDefault();
    };
    el.addEventListener('beforeinput', before);
    return () => el.removeEventListener('beforeinput', before);
  }, []);
  useEffect(() => {
    const sel = () => box.current?.contains(document.getSelection()?.anchorNode ?? null) && refresh();
    document.addEventListener('selectionchange', sel);
    return () => document.removeEventListener('selectionchange', sel);
  }, []);
  const run = (cmd: string, arg?: string) => {
    box.current?.focus();
    document.execCommand('styleWithCSS', false, 'false');
    document.execCommand(cmd, false, arg);
    emit();
    refresh();
  };
  // Buttons keep the selection (no focus change on press).
  const keep = (e: React.MouseEvent) => e.preventDefault();
  /** The selection (maybe empty) becomes a variable. */
  const makeVariable = (text: string) => {
    const tok = tokens?.makeVariable?.(text);
    if (!tok) return;
    insert(chip(tok, tok, tokens!.label(tok), tokens!.title?.(tok)));
    tokens!.onSelect?.(tok);
  };
  /** A picture where the caret is. */
  const addPicture = async () => {
    const tag = await tokens?.pickImage?.();
    if (!tag) return;
    insert(chip(tag, 'img:new', '▣'));
    saved.current = null;
    // Its rank: the chip gets its id when the box is drawn again.
    const n = [...(box.current?.querySelectorAll<HTMLElement>('.rt-tok') ?? [])].filter((x) => x.dataset.tok?.startsWith('<img=')).findIndex((x) => x.dataset.id === 'img:new');
    tokens?.onSelect?.(`img:${Math.max(0, n)}`);
  };
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const key = (e: KeyboardEvent) => e.key === 'Escape' && close();
    window.addEventListener('pointerdown', close);
    // Before the editor and the Designer, which keep Escape for themselves.
    window.addEventListener('keydown', key, true);
    return () => {
      window.removeEventListener('pointerdown', close);
      window.removeEventListener('keydown', key, true);
    };
  }, [menu]);
  const B = ({ cmd, icon, title }: { cmd: string; icon: React.ReactNode; title: string }) => (
    <button type="button" className={state[cmd] ? 'on' : ''} title={title} onMouseDown={keep} onClick={() => run(cmd)}>
      {icon}
    </button>
  );
  return (
    <div className={`rt ${plain ? 'plain' : ''} ${bare ? 'bare' : ''}`}>
      {!plain && !bare && (
      <div className="rt-bar">
        <B cmd="bold" icon={<Bold size={13} />} title={t('Bold (Ctrl+B)')} />
        <B cmd="italic" icon={<Italic size={13} />} title={t('Italic (Ctrl+I)')} />
        <B cmd="underline" icon={<Underline size={13} />} title={t('Underline (Ctrl+U)')} />
        <B cmd="strikeThrough" icon={<Strikethrough size={13} />} title={t('Strike through')} />
        <span className="rt-sep" />
        {SWATCHES.map((c) => (
          <button key={c} type="button" className="rt-sw" title={t('Colour {c}', { c })} style={{ background: c }} onMouseDown={keep} onClick={() => run('foreColor', c)} />
        ))}
        <span className="rt-pick" onMouseDown={keep}>
          <HexColorField compact title={t('Another colour')} value={color} onChange={(c) => (setColor(c), run('foreColor', c.slice(0, 7)))} />
        </span>
        <span className="rt-sep" />
        <B cmd="removeFormat" icon={<RemoveFormatting size={13} />} title={t('Back to the style of the whole text')} />
        {tokens?.makeVariable && (
          <button
            type="button"
            className="rt-new v"
            title={t('Make the selection a variable the Operator fills in')}
            onMouseDown={keep}
            onClick={() => {
              const sel = selectionIn();
              saved.current = sel?.rangeCount ? sel.getRangeAt(0).cloneRange() : null;
              makeVariable(sel?.toString() ?? '');
            }}
          >
            <Braces size={13} /> {t('Variable')}
          </button>
        )}
        {tokens?.pickImage && (
          <button
            type="button"
            className="rt-new i"
            title={t('Insert a picture in the text')}
            onMouseDown={keep}
            onClick={() => {
              const sel = selectionIn();
              saved.current = sel?.rangeCount ? sel.getRangeAt(0).cloneRange() : null;
              void addPicture();
            }}
          >
            <ImagePlus size={13} /> {t('Picture')}
          </button>
        )}
      </div>
      )}
      <div
        ref={box}
        className={`rt-box ${multiline ? 'multi' : ''}`}
        contentEditable
        suppressContentEditableWarning
        role="textbox"
        aria-multiline={multiline}
        data-placeholder={placeholder}
        onInput={emit}
        onContextMenu={(e) => {
          if (!plain || !tokens) return;
          e.preventDefault();
          const sel = selectionIn();
          const text = sel?.toString() ?? '';
          if (text.trim() && sel?.rangeCount) saved.current = sel.getRangeAt(0).cloneRange();
          else {
            // No selection: the picture goes where the click is.
            const at = document.caretRangeFromPoint?.(e.clientX, e.clientY);
            saved.current = at && box.current?.contains(at.startContainer) ? at : null;
          }
          setMenu({ x: e.clientX, y: e.clientY, text: text.trim() ? text : '' });
        }}
        onClick={(e) => {
          if (!tokens?.onSelect) return;
          const c = (e.target as HTMLElement).closest<HTMLElement>('.rt-tok');
          tokens.onSelect(c?.dataset.id ?? null);
        }}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === 'Enter') {
            e.preventDefault();
            const full = !!maxLength && (box.current?.textContent ?? '').length >= maxLength;
            if (multiline && !full) document.execCommand('insertLineBreak');
            else box.current?.blur();
            emit();
          }
          if ((e.ctrlKey || e.metaKey) && ['b', 'i', 'u'].includes(e.key.toLowerCase())) {
            e.preventDefault();
            if (!plain) run({ b: 'bold', i: 'italic', u: 'underline' }[e.key.toLowerCase() as 'b' | 'i' | 'u']);
          }
        }}
        onPaste={(e) => {
          // Pasted text keeps no outside style (and stops at the most characters).
          e.preventDefault();
          let text = e.clipboardData.getData('text/plain');
          if (maxLength) text = [...text].slice(0, Math.max(0, maxLength - (box.current?.textContent ?? '').length + (document.getSelection()?.toString().length ?? 0))).join('');
          if (text) document.execCommand('insertText', false, text);
        }}
      />
      {menu &&
        createPortal(
          <div className="rt-menu" style={{ left: Math.min(menu.x, window.innerWidth - 260), top: menu.y }} onPointerDown={(e) => e.stopPropagation()}>
            {menu.text ? (
              <>
                <div className="rt-menu-h">« {menu.text.length > 32 ? `${menu.text.slice(0, 32)}…` : menu.text} »</div>
                <button
                  type="button"
                  onClick={() => {
                    setMenu(null);
                    makeVariable(menu.text);
                  }}
                >
                  <Braces size={13} /> {t('Expose the selection to the Operator')}
                </button>
              </>
            ) : (
              <button
                type="button"
                onClick={() => {
                  setMenu(null);
                  void addPicture();
                }}
              >
                <ImagePlus size={13} /> {t('Insert a picture…')}
              </button>
            )}
          </div>,
          document.body,
        )}
    </div>
  );
}
