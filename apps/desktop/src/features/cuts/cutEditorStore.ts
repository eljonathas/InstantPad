import { useSyncExternalStore } from "react";
import { commands, type CutEditorView, type CutSource } from "../../shared/ipc";

type CutEditorState = { status: "closed" } | { status: "opening" } | { status: "open"; id: number; view: CutEditorView };

let state: CutEditorState = { status: "closed" };
let opened = 0;
const listeners = new Set<() => void>();

function set(next: CutEditorState) {
  state = next;
  listeners.forEach((listener) => listener());
}

/**
 * Which sound the cut editor shows. The backend keeps the sound itself; this only says whether
 * the dialog is open, and with what. A failure to open is shown on the display by the backend.
 */
export const cutEditor = {
  read: () => state,
  subscribe(listener: () => void) {
    listeners.add(listener);
    return () => void listeners.delete(listener);
  },
  async open(source: CutSource) {
    if (state.status === "opening") return;
    set({ status: "opening" });
    const answer = await commands.openCutEditor(source);
    set(answer.status === "ok" ? { status: "open", id: ++opened, view: answer.data } : { status: "closed" });
  },
  /** The backend already let the sound go: after placing the pieces, or on `cancel`. */
  closed() {
    set({ status: "closed" });
  },
  cancel() {
    void commands.closeCutEditor();
    set({ status: "closed" });
  },
};

export function useCutEditor() {
  return useSyncExternalStore(cutEditor.subscribe, cutEditor.read);
}
