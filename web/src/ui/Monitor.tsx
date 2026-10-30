import { useLayoutEffect, useRef, useState } from 'react';
import { Viewport } from '../engine/Scene';

/** A 16:9 (or project ratio) monitor that scales its content to fit. */
export function Monitor({
  width,
  height,
  label,
  tally,
  children,
  extra,
}: {
  width: number;
  height: number;
  label: string;
  tally?: 'pvw' | 'pgm';
  children: React.ReactNode;
  extra?: React.ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(0.3);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setScale(Math.min(el.clientWidth / width, el.clientHeight / height)));
    ro.observe(el);
    return () => ro.disconnect();
  }, [width, height]);
  return (
    <div className={`monitor ${tally ?? ''}`}>
      <div className="monitor-head">
        <span className="monitor-label">{label}</span>
        {extra}
      </div>
      <div className={`monitor-screen ${height > width ? 'portrait' : ''}`} ref={ref} style={{ ['--ratio' as string]: `${width} / ${height}` }}>
        <Viewport width={width} height={height} scale={scale} className="monitor-viewport">
          {children}
        </Viewport>
      </div>
    </div>
  );
}
