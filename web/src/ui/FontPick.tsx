import { useFonts } from '../engine/fonts';
import { t } from '../i18n';
import { Dropdown } from './Dropdown';

const ORDER = ['uploaded', 'bundled', 'system'];
const GROUP: Record<string, string> = { uploaded: 'Uploaded', bundled: 'Built in', system: 'Server' };

export const WEIGHT_NAMES: Record<number, string> = { 100: 'Thin', 200: 'Extra light', 300: 'Light', 400: 'Regular', 500: 'Medium', 600: 'Semibold', 700: 'Bold', 800: 'Extra bold', 900: 'Black' };

/** Font and weight, from the fonts of Assets: two designed drop-downs, each font shown in itself. */
export function FontPick({ family, weight, onChange }: { family: string; weight: number; onChange: (family: string, weight: number) => void }) {
  const families = useFonts((s) => s.families);
  const current = families.find((f) => f.family.toLowerCase() === family.toLowerCase());
  const weights = [...new Set([...(current?.weights ?? [400, 700]), weight])].sort((a, b) => a - b);
  const famOpts = [...(current ? [] : [{ value: family, label: t('{family} (missing)', { family }) }]), ...[...families].sort((a, b) => ORDER.indexOf(a.source) - ORDER.indexOf(b.source)).map((f) => ({ value: f.family, label: f.family, group: t(GROUP[f.source] ?? 'Server'), style: { fontFamily: `"${f.family}", Inter, sans-serif` } }))];
  return (
    <div className="font-pick">
      <Dropdown
        value={current?.family ?? family}
        options={famOpts}
        onChange={(v) => {
          const fam = families.find((f) => f.family === v);
          const w = fam && !fam.weights.includes(weight) ? fam.weights.reduce((a, b) => (Math.abs(b - weight) < Math.abs(a - weight) ? b : a)) : weight;
          onChange(v, w);
        }}
      />
      <Dropdown className="font-weight" value={String(weight)} options={weights.map((w) => ({ value: String(w), label: WEIGHT_NAMES[w] ? t(WEIGHT_NAMES[w]) : String(w), note: String(w) }))} onChange={(v) => onChange(current?.family ?? family, +v)} />
    </div>
  );
}
