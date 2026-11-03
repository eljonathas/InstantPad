import { useEffect, useId, type ReactNode } from "react";
import "./Dialog.css";
import { holdHotkeys } from "./hotkeyPause";

interface DialogProps {
  title: string;
  onCancel: () => void;
  /** For text that needs more room than a form. */
  isWide?: boolean;
  /** For an editor, which needs most of the window. */
  isLarge?: boolean;
  /**
   * Whether the global keys stay off while it is open, as they should while the user types or
   * works in a dialog. A dialog that opens by itself, such as the one of an update, leaves them
   * on, since the user may be in a game when it opens.
   */
  holdsHotkeys?: boolean;
  children: ReactNode;
}

/** The dialogs open, the one on top last. Only the one on top answers to the keys. */
const open: { id: string; title: string }[] = [];

/** The title of the dialog on top, or null when none is open. */
export function topDialog() {
  return open.length > 0 ? open[open.length - 1].title : null;
}

/** The frame of every dialog. Esc cancels the one on top, and the global hotkeys stay off while it is open. */
export function Dialog({ title, onCancel, isWide, isLarge, holdsHotkeys = true, children }: DialogProps) {
  const id = useId();

  useEffect(() => {
    open.push({ id, title });
    return () => void open.splice(
      open.findIndex((item) => item.id === id),
      1,
    );
  }, [id, title]);

  useEffect(() => (holdsHotkeys ? holdHotkeys() : undefined), [holdsHotkeys]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && open[open.length - 1]?.id === id) onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel, id]);

  return (
    <div className="dialog-backdrop">
      <div
        className="dialog"
        data-wide={isWide ? "" : undefined}
        data-large={isLarge ? "" : undefined}
        role="dialog"
        aria-modal="true"
        aria-label={title}
      >
        <h2 className="dialog-title">{title}</h2>
        {children}
      </div>
    </div>
  );
}

/** A dialog label: gray, small, above its field. */
export function DialogLabel({ children }: { children: ReactNode }) {
  return <div className="dialog-label">{children}</div>;
}
