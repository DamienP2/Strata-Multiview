/*
 * Esc closes the dialog on top, everywhere: a dialog closes when its
 * backdrop (`.modal-back`) is pressed, so Esc presses it. A page or a
 * dialog that handles Esc itself calls preventDefault (or stops it).
 */
export function installEscape() {
  if (typeof window === 'undefined') return;
  window.addEventListener('keydown', (e) => {
    if (e.key !== 'Escape' || e.defaultPrevented) return;
    const backs = document.querySelectorAll<HTMLElement>('.modal-back');
    const top = backs[backs.length - 1];
    if (!top) return;
    e.preventDefault();
    top.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true }));
  });
}
