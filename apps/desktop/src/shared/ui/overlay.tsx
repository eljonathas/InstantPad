import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from "react";
import { ContextMenu, type MenuEntry } from "./ContextMenu";
import { holdHotkeys } from "./hotkeyPause";

type Layer = { kind: "menu"; x: number; y: number; entries: MenuEntry[] } | { kind: "dialog"; content: ReactNode };

interface Overlay {
  menu: (x: number, y: number, entries: MenuEntry[]) => void;
  /** Shows what `render` returns until it calls `finish`. Cancelling resolves to null. */
  dialog: <T>(render: (finish: (result: T | null) => void) => ReactNode) => Promise<T | null>;
}

const OverlayContext = createContext<Overlay>(null!);

export const useOverlay = () => useContext(OverlayContext);

/** Menus and dialogs are drawn at the window's real scale, outside the scaled device. */
export function OverlayProvider({ children }: { children: ReactNode }) {
  const [layer, setLayer] = useState<Layer | null>(null);
  const settle = useRef<(result: unknown) => void>(null);

  const releaseKeys = useRef<(() => void) | null>(null);

  const close = useCallback(() => {
    releaseKeys.current?.();
    releaseKeys.current = null;
    setLayer(null);
  }, []);

  const overlay: Overlay = {
    menu: (x, y, entries) => {
      // A key that changes the interface would leave the menu acting on the wrong pads.
      releaseKeys.current ??= holdHotkeys();
      setLayer({ kind: "menu", x, y, entries });
    },
    dialog: (render) =>
      new Promise((resolve) => {
        const finish = (result: unknown) => {
          // A second call, such as Esc right after a button, finds nothing to settle.
          if (!settle.current) return;
          settle.current = null;
          setLayer(null);
          resolve(result as never);
        };
        settle.current = finish;
        setLayer({ kind: "dialog", content: render(finish) });
      }),
  };

  return (
    <OverlayContext value={overlay}>
      {children}
      {layer?.kind === "menu" && <ContextMenu x={layer.x} y={layer.y} entries={layer.entries} onClose={close} />}
      {layer?.kind === "dialog" && layer.content}
    </OverlayContext>
  );
}
