// Tauri's own drag and drop handler stays on (it gives the paths of files dragged from the file
// manager), and on Windows that switches HTML5 drag and drop off. So pads and rows are dragged
// with pointer events.
const MOVE_THRESHOLD = 5;
const GHOST_OFFSET = 14;
const LANDING = "pad-land";

export type DragKind = "place" | "swap";

const padElement = (slot: number) => document.querySelector<HTMLElement>(`[data-slot="${slot}"]`);

/** The pad under a point of the viewport, if any. */
export function padSlotAt(x: number, y: number): number | null {
  const pad = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-slot]");
  return pad ? Number(pad.dataset.slot) : null;
}

/** Shows which pad a drop would land on, if any. */
export function markDropPad(slot: number | null) {
  const marked = document.querySelector("[data-drop]");
  const next = slot === null ? null : padElement(slot);
  if (marked === next) return;

  marked?.removeAttribute("data-drop");
  next?.setAttribute("data-drop", "");
}

/** Plays the thud of something arriving on a pad. */
export function landOnPad(slot: number) {
  const pad = padElement(slot);
  if (!pad) return;

  const finish = (event: AnimationEvent) => {
    if (event.animationName !== LANDING) return;
    pad.removeAttribute("data-landed");
    pad.removeEventListener("animationend", finish);
  };
  pad.addEventListener("animationend", finish);
  pad.setAttribute("data-landed", "");
}

interface DragItem {
  label: string;
  color: string;
  kind: DragKind;
  /** What is being dragged, which steps back while it is away. */
  source: HTMLElement;
}

function createGhost({ label, color }: DragItem) {
  const ghost = document.createElement("div");
  ghost.className = "drag-ghost";
  ghost.style.setProperty("--ghost-color", color);
  ghost.append(Object.assign(document.createElement("span"), { textContent: label }));
  document.body.append(ghost);
  return ghost;
}

/**
 * Follows the pointer once it moves past a small threshold, with a ghost carrying the label, and
 * reports the pad it was released over. A press that never moves far enough drags nothing.
 */
export function beginDrag(from: { x: number; y: number }, item: DragItem, onDrop: (slot: number) => void) {
  let ghost: HTMLElement | null = null;

  const move = (event: PointerEvent) => {
    if (!ghost) {
      if (Math.hypot(event.clientX - from.x, event.clientY - from.y) < MOVE_THRESHOLD) return;
      ghost = createGhost(item);
      document.body.dataset.dragging = item.kind;
      item.source.toggleAttribute("data-drag-source", true);
    }
    ghost.style.translate = `${event.clientX + GHOST_OFFSET}px ${event.clientY + GHOST_OFFSET}px`;
    markDropPad(padSlotAt(event.clientX, event.clientY));
  };

  const end = (event: PointerEvent) => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", end);
    window.removeEventListener("pointercancel", end);
    if (!ghost) return;

    ghost.remove();
    delete document.body.dataset.dragging;
    item.source.removeAttribute("data-drag-source");
    markDropPad(null);

    const slot = event.type === "pointerup" ? padSlotAt(event.clientX, event.clientY) : null;
    if (slot !== null && String(slot) !== item.source.dataset.slot) onDrop(slot);
  };

  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", end);
  window.addEventListener("pointercancel", end);
}
