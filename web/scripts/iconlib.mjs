// The picture library shipped with the app (cg-server iconlib.rs reads it):
// dist/iconlib/<set>.json.gz, one per set, in the Iconify JSON format
// (prefix, info, icons, aliases, categories, width, height), and
// dist/iconlib/manifest.json saying what each one is.
//
// Icons, emojis and flags come from the @iconify-json packages (updated later
// by the server from npm, in System › Updates); the illustrations (Open
// Doodles, Flowbite) are made SVG here from their components.
import { mkdirSync, readFileSync, readdirSync, writeFileSync, existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';

const here = dirname(fileURLToPath(import.meta.url));
const web = join(here, '..');
const out = join(web, 'dist', 'iconlib');
const require = createRequire(import.meta.url);

/** Sets of the Iconify catalogue: id, kind, name shown. */
const ICONIFY = [
  ['lucide', 'icons', 'Lucide'],
  ['material-symbols', 'icons', 'Material'],
  ['ph', 'icons', 'Phosphor'],
  ['tabler', 'icons', 'Tabler'],
  ['noto', 'emoji', 'Noto'],
  ['twemoji', 'emoji', 'Twemoji'],
  ['fluent-emoji-flat', 'emoji', 'Fluent'],
  ['flag', 'flags', 'Rectangles'],
  ['circle-flags', 'flags', 'Circles'],
];

const manifest = { sets: [] };
mkdirSync(out, { recursive: true });

const write = (id, data) => {
  const gz = gzipSync(JSON.stringify(data), { level: 9 });
  writeFileSync(join(out, `${id}.json.gz`), gz);
  return gz.length;
};

/** An Iconify package as one file: its icons, aliases, categories and info. */
export function packIconify(dir) {
  const icons = JSON.parse(readFileSync(join(dir, 'icons.json'), 'utf8'));
  const info = existsSync(join(dir, 'info.json')) ? JSON.parse(readFileSync(join(dir, 'info.json'), 'utf8')) : (icons.info ?? {});
  const meta = existsSync(join(dir, 'metadata.json')) ? JSON.parse(readFileSync(join(dir, 'metadata.json'), 'utf8')) : {};
  const keep = { prefix: icons.prefix, info, icons: icons.icons, aliases: icons.aliases ?? {}, categories: meta.categories ?? {} };
  for (const k of ['width', 'height', 'left', 'top']) if (icons[k] !== undefined) keep[k] = icons[k];
  return keep;
}

for (const [id, kind, title] of ICONIFY) {
  const dir = dirname(require.resolve(`@iconify-json/${id}/icons.json`));
  const pkg = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'));
  const data = packIconify(dir);
  const bytes = write(id, data);
  manifest.sets.push({ id, kind, title, version: pkg.version, package: pkg.name, license: data.info.license?.title ?? pkg.license, author: data.info.author?.name ?? '', total: Object.keys(data.icons).length, bytes });
}

// ---- Illustrations

/** Attribute names of React (camelCase) as SVG writes them. */
const attr = (k) => (k === 'className' ? 'class' : k === 'xlinkHref' ? 'xlink:href' : /^(viewBox|preserveAspectRatio|gradientTransform|gradientUnits|patternUnits|patternTransform|clipPathUnits|maskUnits|maskContentUnits|stdDeviation|spreadMethod)$/.test(k) ? k : k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`));
const esc = (v) => String(v).replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;');
/** A stand-in for React that writes the markup. */
const fakeReact = {
  createElement(type, props, ...children) {
    const a = Object.entries(props ?? {})
      .filter(([, v]) => v !== undefined && v !== null && v !== false)
      .map(([k, v]) => ` ${attr(k)}="${esc(v)}"`)
      .join('');
    const inner = children.flat(Infinity).filter((c) => c !== null && c !== undefined && c !== false).join('');
    return `<${type}${a}>${inner}</${type}>`;
  },
};

/** An SVG as an Iconify icon: what is inside, its size, the root's own look on a group. */
function asIcon(svg) {
  const open = /^<svg\b([^>]*)>/s.exec(svg.trim());
  if (!open) return null;
  const attrs = Object.fromEntries([...open[1].matchAll(/([\w:-]+)="([^"]*)"/g)].map((m) => [m[1], m[2]]));
  const vb = (attrs.viewBox ?? `0 0 ${attrs.width} ${attrs.height}`).split(/[\s,]+/).map(Number);
  const inner = svg.trim().slice(open[0].length).replace(/<\/svg>\s*$/s, '');
  const look = Object.entries(attrs).filter(([k]) => !/^(xmlns.*|viewBox|width|height|class|style|aria-.*|role)$/.test(k));
  const body = look.length ? `<g${look.map(([k, v]) => ` ${k}="${v}"`).join('')}>${inner}</g>` : inner;
  const icon = { body, width: vb[2], height: vb[3] };
  if (vb[0]) icon.left = vb[0];
  if (vb[1]) icon.top = vb[1];
  return icon;
}

const kebab = (s) => s.replace(/([a-z0-9])([A-Z])/g, '$1-$2').toLowerCase();

// Open Doodles (CC0, by Pablo Stanley), from react-open-doodles' components.
{
  const id = 'open-doodles';
  const accent = '#ff5678';
  const file = require.resolve('react-open-doodles');
  const src = readFileSync(file, 'utf8');
  const mod = { exports: {} };
  new Function('require', 'module', 'exports', src)((n) => (n === 'react' ? fakeReact : require(n)), mod, mod.exports);
  const icons = {};
  for (const [name, comp] of Object.entries(mod.exports)) {
    if (typeof comp !== 'function' || !name.endsWith('Doodle')) continue;
    const icon = asIcon(comp({ ink: '#000000', accent }));
    if (icon) icons[kebab(name.replace(/Doodle$/, ''))] = icon;
  }
  const pkg = JSON.parse(readFileSync(join(dirname(file), '..', 'package.json'), 'utf8'));
  const data = { prefix: id, info: { name: 'Open Doodles', author: { name: 'Pablo Stanley' }, license: { title: 'CC0 1.0' } }, icons, aliases: {}, categories: {} };
  const bytes = write(id, data);
  manifest.sets.push({ id, kind: 'illustrations', title: 'Open Doodles', version: pkg.version, package: null, license: 'CC0 1.0', author: 'Pablo Stanley', total: Object.keys(icons).length, bytes, accent });
}

// Flowbite illustrations (MIT), from their Svelte components (light ones).
{
  const id = 'flowbite';
  const accent = '#2563eb';
  const dir = join(dirname(require.resolve('flowbite-svelte-illustrations/package.json')), 'dist');
  const icons = {};
  for (const f of readdirSync(dir).filter((f) => f.endsWith('.svelte') && !f.endsWith('Dark.svelte'))) {
    const src = readFileSync(join(dir, f), 'utf8');
    const defaults = Object.fromEntries([...src.matchAll(/(color\d+)\s*=\s*'([^']+)'/g)].map((m) => [m[1], m[2]]));
    const start = src.indexOf('<svg');
    const end = src.lastIndexOf('</svg>');
    if (start < 0 || end < 0) continue;
    const svg = src
      .slice(start, end + 6)
      .replace(/\{#if[\s\S]*?\{\/if\}/g, '')
      .replace(/\s\{\.\.\.\w+\}/g, '')
      .replace(/\s[\w:-]+=\{(?!color\d+\})[^}]*\}/g, '')
      .replace(/=\{(color\d+)\}/g, (_, c) => `="${defaults[c] ?? '#000000'}"`)
      .replace(/\{(color\d+)\}/g, (_, c) => defaults[c] ?? '#000000');
    const icon = asIcon(svg);
    if (icon && !/[{}]/.test(icon.body)) icons[kebab(f.replace(/\.svelte$/, ''))] = icon;
  }
  const pkg = JSON.parse(readFileSync(join(dir, '..', 'package.json'), 'utf8'));
  const data = { prefix: id, info: { name: 'Flowbite Illustrations', author: { name: 'Flowbite' }, license: { title: 'MIT' } }, icons, aliases: {}, categories: {} };
  const bytes = write(id, data);
  manifest.sets.push({ id, kind: 'illustrations', title: 'Flowbite', version: pkg.version, package: null, license: 'MIT', author: 'Flowbite', total: Object.keys(icons).length, bytes, accent });
}

writeFileSync(join(out, 'manifest.json'), JSON.stringify(manifest, null, 1));
const total = manifest.sets.reduce((n, s) => n + s.bytes, 0);
console.log(`iconlib: ${manifest.sets.length} sets, ${manifest.sets.reduce((n, s) => n + s.total, 0)} pictures, ${(total / 1e6).toFixed(1)} MB`);
