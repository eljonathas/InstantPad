import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect } from "react";
import { commands } from "../shared/ipc";
import { padSlotAt } from "../shared/ui/drag";

const padAt = (position: { x: number; y: number }) =>
  // The webview reports physical pixels; the page works in CSS pixels.
  padSlotAt(position.x / window.devicePixelRatio, position.y / window.devicePixelRatio);

function highlight(slot: number | null) {
  document.querySelector("[data-drop]")?.removeAttribute("data-drop");
  if (slot !== null) document.querySelector(`[data-slot="${slot}"]`)?.setAttribute("data-drop", "");
}

/** Files dragged from the file manager come through Tauri, which also gives their paths. */
export function useFileDrop() {
  useEffect(() => {
    // A plain browser has no webview to ask.
    if (!isTauri()) return;

    let leaving = false;
    let stop: (() => void) | undefined;

    void getCurrentWebview()
      .onDragDropEvent(({ payload }) => {
        if (payload.type === "over" || payload.type === "enter") return highlight(padAt(payload.position));

        highlight(null);
        if (payload.type !== "drop" || payload.paths.length === 0) return;
        const slot = padAt(payload.position);
        if (slot !== null) void commands.importFile(payload.paths[0], slot);
      })
      .then((unlisten) => (leaving ? unlisten() : (stop = unlisten)));

    return () => {
      leaving = true;
      stop?.();
    };
  }, []);
}
