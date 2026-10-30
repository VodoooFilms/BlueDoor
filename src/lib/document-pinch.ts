import { tick } from "svelte";

// Two fingers zoom the document rather than the WebView and its controls.
export function pinchDocument(root: HTMLElement, options: { getZoom: () => number; setZoom: (zoom: number) => void; viewerSelector?: string; pageSelector?: string }) {
  let gesture: { viewer: HTMLElement; page: HTMLElement; distance: number; zoom: number; x: number; y: number } | null = null;
  let frame = 0;
  let pending: { zoom: number; x: number; y: number } | null = null;
  const distance = (touches: TouchList) => Math.hypot(touches[0].clientX - touches[1].clientX, touches[0].clientY - touches[1].clientY);
  function start(event: TouchEvent) {
    if (event.touches.length !== 2) { gesture = null; return; }
    const target = event.target as Element;
    const viewer = target.closest<HTMLElement>(options.viewerSelector ?? ".viewer-body, .page-scroll");
    const page = target.closest<HTMLElement>(options.pageSelector ?? ".page-wrapper");
    if (!viewer || !page || !root.contains(viewer)) return;
    const length = distance(event.touches);
    if (length < 1) return;
    event.preventDefault();
    const x = (event.touches[0].clientX + event.touches[1].clientX) / 2;
    const y = (event.touches[0].clientY + event.touches[1].clientY) / 2;
    const rect = page.getBoundingClientRect();
    gesture = { viewer, page, distance: length, zoom: options.getZoom(), x: (x - rect.left) / (options.getZoom() / 100), y: (y - rect.top) / (options.getZoom() / 100) };
  }
  function move(event: TouchEvent) {
    if (!gesture || event.touches.length !== 2) return;
    event.preventDefault();
    pending = { zoom: Math.round(Math.max(25, Math.min(800, gesture.zoom * distance(event.touches) / gesture.distance)) * 100) / 100,
      x: (event.touches[0].clientX + event.touches[1].clientX) / 2, y: (event.touches[0].clientY + event.touches[1].clientY) / 2 };
    if (frame) return;
    frame = requestAnimationFrame(applyPending);
  }
  function applyPending() {
    frame = 0;
    const current = gesture, next = pending;
    if (!current || !next) return;
    options.setZoom(next.zoom);
    void tick().then(() => {
      if (!root.isConnected) return;
      const rect = current.page.getBoundingClientRect();
      current.viewer.scrollLeft += rect.left + current.x * next.zoom / 100 - next.x;
      current.viewer.scrollTop += rect.top + current.y * next.zoom / 100 - next.y;
    });
  }

  function end(event: TouchEvent) {
    if (event.touches.length >= 2) return;
    if (frame) { cancelAnimationFrame(frame); applyPending(); }
    gesture = null;
  }
  root.addEventListener("touchstart", start, { passive: false });
  root.addEventListener("touchmove", move, { passive: false });
  root.addEventListener("touchend", end);
  root.addEventListener("touchcancel", end);
  return { destroy() {
    cancelAnimationFrame(frame);
    root.removeEventListener("touchstart", start); root.removeEventListener("touchmove", move);
    root.removeEventListener("touchend", end); root.removeEventListener("touchcancel", end);
  } };
}
