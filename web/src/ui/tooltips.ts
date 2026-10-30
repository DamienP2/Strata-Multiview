/*
 * Designed tooltips for the whole app, instead of the browser's: any element
 * with a `title` shows it in a small card that follows the pointer. The
 * title moves to `data-tip` on first hover (the browser then shows nothing);
 * an element without a visible name keeps it as its aria-label.
 */

const DELAY = 380;

export function installTooltips() {
  if (typeof document === 'undefined' || document.getElementById('tip')) return;
  const tip = document.createElement('div');
  tip.id = 'tip';
  tip.className = 'tip';
  tip.setAttribute('role', 'tooltip');
  document.body.appendChild(tip);

  let target: HTMLElement | null = null;
  let timer = 0;
  let x = 0;
  let y = 0;
  let shown = false;

  const place = () => {
    const pad = 8;
    const w = tip.offsetWidth;
    const h = tip.offsetHeight;
    let left = x + 14;
    let top = y + 18;
    if (left + w > window.innerWidth - pad) left = Math.max(pad, x - w - 10);
    if (top + h > window.innerHeight - pad) top = Math.max(pad, y - h - 12);
    tip.style.transform = `translate(${Math.round(left)}px, ${Math.round(top)}px)`;
  };
  const hide = () => {
    window.clearTimeout(timer);
    target = null;
    if (shown) {
      shown = false;
      tip.classList.remove('on');
    }
  };
  const show = () => {
    if (!target || !target.isConnected) return hide();
    const text = target.dataset.tip;
    if (!text) return;
    tip.textContent = text;
    shown = true;
    tip.classList.add('on');
    place();
  };

  /** The element's tip, taking over its title. */
  const tipOf = (el: HTMLElement): string | undefined => {
    const t = el.getAttribute('title');
    if (t) {
      el.dataset.tip = t;
      el.removeAttribute('title');
      if (!el.getAttribute('aria-label') && !el.textContent?.trim()) el.setAttribute('aria-label', t);
    }
    return el.dataset.tip;
  };

  document.addEventListener(
    'pointerover',
    (e) => {
      if (e.pointerType === 'touch') return;
      let el = e.target as HTMLElement | null;
      // The nearest element with a tip (SVG titles stay native).
      while (el && el !== document.body && !(el instanceof HTMLElement && (el.hasAttribute('title') || el.dataset.tip))) el = el.parentElement;
      if (!el || el === document.body) return hide();
      if (el === target) return;
      hide();
      if (!tipOf(el)) return;
      target = el;
      x = e.clientX;
      y = e.clientY;
      timer = window.setTimeout(show, DELAY);
    },
    true,
  );
  document.addEventListener(
    'pointermove',
    (e) => {
      x = e.clientX;
      y = e.clientY;
      if (shown) {
        // Follows the pointer; leaves when it leaves the element.
        if (target && !target.contains(e.target as Node)) return hide();
        place();
      }
    },
    { capture: true, passive: true },
  );
  for (const ev of ['pointerdown', 'wheel', 'keydown', 'blur'] as const) window.addEventListener(ev, hide, { capture: true, passive: true });
  document.addEventListener('pointerleave', hide);
}
