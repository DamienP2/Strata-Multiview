// A live input on a canvas (the HTML5 output): the picture its stream shows now, drawn at the
// display's rate, without React (engine/liveInputs.ts plays the stream out).
import { useEffect, useRef } from 'react';
import { onPaint, type LiveStream } from './liveInputs';

export function LiveCanvas({ stream, style }: { stream: LiveStream; style?: React.CSSProperties }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    let drawn: ImageBitmap | null = null;
    const paint = () => {
      const c = ref.current;
      const b = stream.current;
      if (!c || !b || b === drawn) return;
      if (c.width !== b.width || c.height !== b.height) {
        c.width = b.width;
        c.height = b.height;
      }
      c.getContext('2d')?.drawImage(b, 0, 0);
      drawn = b;
    };
    paint();
    return onPaint(paint);
  }, [stream]);
  return <canvas ref={ref} style={style} />;
}
