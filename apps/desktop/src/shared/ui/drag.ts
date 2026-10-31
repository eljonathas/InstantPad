// Tauri's own drag and drop handler stays on (it gives the paths of files dragged from the file
// manager), and on Windows that switches HTML5 drag and drop off. So pads and rows are dragged
// with pointer events.
const MOVE_THRESHOLD = 5;
const GHOST_OFFSET = 14;

/** The pad under a point of the viewport, if any. */
export function padSlotAt(x: number, y: number): number | null {
  const pad = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-slot]");
  return pad ? Number(pad.dataset.slot) : null;
}

/**
 * Follows the pointer once it moves past a small threshold, with a ghost carrying `label`, and
 * reports the pad it was released over. A press that never moves far enough drags nothing.
 */
export function beginDrag(from: { x: number; y: number }, label: string, onDrop: (slot: number) => void) {
  let ghost: HTMLElement | null = null;

  const move = (event: PointerEvent) => {
    if (!ghost) {
      if (Math.hypot(event.clientX - from.x, event.clientY - from.y) < MOVE_THRESHOLD) return;
      ghost = document.createElement("div");
      ghost.className = "drag-ghost";
      ghost.textContent = label;
      document.body.append(ghost);
      document.body.dataset.dragging = "";
    }
    ghost.style.transform = `translate(${event.clientX + GHOST_OFFSET}px, ${event.clientY + GHOST_OFFSET}px)`;
  };

  const end = (event: PointerEvent) => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", end);
    window.removeEventListener("pointercancel", end);
    if (!ghost) return;

    ghost.remove();
    delete document.body.dataset.dragging;
    const slot = event.type === "pointerup" ? padSlotAt(event.clientX, event.clientY) : null;
    if (slot !== null) onDrop(slot);
  };

  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", end);
  window.addEventListener("pointercancel", end);
}
