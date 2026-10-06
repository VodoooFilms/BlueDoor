/** Animate view changes without remounting editors or losing their state. */
export function enterView(node: HTMLElement, key: unknown) {
  const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
  let animation: Animation | undefined;
  function enter() {
    animation?.cancel();
    if (preference.matches || !node.getClientRects().length) return;
    animation = node.animate([
      // Document bounds drive zoom and editing: never transform this ancestor.
      { opacity: 0 },
      { opacity: 1 },
    ], { duration: 180, easing: "cubic-bezier(.2,.7,.3,1)" });
  }
  const cancel = () => { if (preference.matches) animation?.cancel(); };
  preference.addEventListener("change", cancel);
  enter();
  return {
    update(next: unknown) { if (next !== key) { key = next; enter(); } },
    destroy() { animation?.cancel(); preference.removeEventListener("change", cancel); },
  };
}
