import { Info } from 'lucide-react';

/** A small "i" after a label: its text shows as a tooltip (hover or keyboard focus). */
export function InfoTip({ text, size = 13 }: { text: string; size?: number }) {
  return (
    <span className="ux3-info" title={text} aria-label={text} role="img" tabIndex={0}>
      <Info size={size} />
    </span>
  );
}
