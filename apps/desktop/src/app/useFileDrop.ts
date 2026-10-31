import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect } from "react";
import { commands } from "../shared/ipc";
import { markDropPad, padSlotAt } from "../shared/ui/drag";

// Only WebView2 reports physical pixels. The other webviews already report what the page uses.
const pixelsPerPoint = () => (navigator.userAgent.includes("Windows") ? window.devicePixelRatio : 1);

const padAt = (position: { x: number; y: number }) =>
  padSlotAt(position.x / pixelsPerPoint(), position.y / pixelsPerPoint());

/** Files dragged from the file manager come through Tauri, which also gives their paths. */
export function useFileDrop() {
  useEffect(() => {
    // A plain browser has no webview to ask.
    if (!isTauri()) return;

    let leaving = false;
    let stop: (() => void) | undefined;

    void getCurrentWebview()
      .onDragDropEvent(({ payload }) => {
        if (payload.type === "over" || payload.type === "enter") {
          document.body.dataset.dragging = "place";
          return markDropPad(padAt(payload.position));
        }

        delete document.body.dataset.dragging;
        markDropPad(null);
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
