// Checks the French texts of the interface.
//
//   node scripts/i18n-check.mjs              texts in t()/tn()/tc() without French
//   node scripts/i18n-check.mjs --raw PATH   English texts in PATH not in t()
//
// Texts are looked up by their English wording (src/i18n/index.ts); the
// French ones live in src/i18n/fr/*.ts as { 'English': 'Français' }.
import fs from 'node:fs';
import path from 'node:path';
import ts from 'typescript';

const root = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..', 'src');
const args = process.argv.slice(2);

function files(dir) {
  const out = [];
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) out.push(...files(p));
    else if (/\.tsx?$/.test(e.name)) out.push(p);
  }
  return out;
}
const parse = (f) => ts.createSourceFile(f, fs.readFileSync(f, 'utf8'), ts.ScriptTarget.Latest, true, f.endsWith('x') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
const literal = (n) => (n && (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) ? n.text : null);
const vars = (s) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(',');

// The French dictionary.
const fr = new Map();
for (const f of files(path.join(root, 'i18n', 'fr'))) {
  const visit = (n) => {
    if (ts.isPropertyAssignment(n)) {
      const k = literal(n.name) ?? (ts.isIdentifier(n.name) ? n.name.text : null);
      const v = literal(n.initializer);
      if (k !== null && v !== null) {
        if (fr.has(k) && fr.get(k).value !== v) console.log(`conflict: ${JSON.stringify(k)} in ${path.relative(root, fr.get(k).file)} and ${path.relative(root, f)}`);
        fr.set(k, { value: v, file: f });
      }
    }
    ts.forEachChild(n, visit);
  };
  visit(parse(f));
}

if (args[0] === '--raw') {
  // English left outside t(): JSX text and the texts of some attributes.
  const target = path.resolve(args[1] ?? root);
  const list = fs.statSync(target).isDirectory() ? files(target) : [target];
  const ATTRS = new Set(['title', 'placeholder', 'label', 'aria-label', 'hint', 'confirm', 'message', 'alt']);
  let n = 0;
  for (const f of list) {
    if (f.includes(`${path.sep}i18n${path.sep}`)) continue;
    const sf = parse(f);
    const report = (node, text) => {
      if (!/[A-Za-z]{2}/.test(text)) return;
      const { line } = sf.getLineAndCharacterOfPosition(node.getStart());
      console.log(`${path.relative(root, f)}:${line + 1}: ${text.trim().replace(/\s+/g, ' ').slice(0, 100)}`);
      n++;
    };
    const visit = (node) => {
      if (ts.isJsxText(node) && node.text.trim()) report(node, node.text);
      if (ts.isJsxAttribute(node) && ATTRS.has(node.name.getText()) && node.initializer && ts.isStringLiteral(node.initializer)) report(node, node.initializer.text);
      ts.forEachChild(node, visit);
    };
    visit(sf);
  }
  console.log(`${n} text(s) outside t()`);
  process.exit(0);
}

// Every t('…') / tn(n, '…', '…') / tl(lang, '…') has its French text.
const used = new Map();
for (const f of files(root)) {
  if (f.includes(`${path.sep}i18n${path.sep}`)) continue;
  const sf = parse(f);
  const visit = (node) => {
    if (ts.isCallExpression(node) && ts.isIdentifier(node.expression)) {
      const name = node.expression.text;
      const ctx = name === 'tc' ? literal(node.arguments[0]) : null;
      const texts = name === 't' ? [literal(node.arguments[0])] : name === 'tn' ? [literal(node.arguments[1]), literal(node.arguments[2])] : name === 'tc' || name === 'tl' ? [literal(node.arguments[1])] : [];
      for (const s of texts) {
        if (s === null) continue;
        const { line } = sf.getLineAndCharacterOfPosition(node.getStart());
        // tc(context, text): its own entry 'context|text', else the text's.
        const key = ctx !== null && fr.has(`${ctx}|${s}`) ? `${ctx}|${s}` : s;
        if (!used.has(key)) used.set(key, `${path.relative(root, f)}:${line + 1}`);
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(sf);
}
let missing = 0;
for (const [s, at] of used) {
  const e = fr.get(s);
  if (!e) {
    console.log(`missing: ${at}: ${JSON.stringify(s)}`);
    missing++;
  } else if (vars(s) !== vars(e.value)) console.log(`placeholders differ: ${at}: ${JSON.stringify(s)} → ${JSON.stringify(e.value)}`);
}
console.log(`${used.size} texts, ${missing} without French, ${fr.size} French entries`);
process.exit(missing ? 1 : 0);
