import { useEffect, type ReactNode } from "react";
import "./Dialog.css";
import { holdHotkeys } from "./hotkeyPause";

interface DialogProps {
  title: string;
  onCancel: () => void;
  children: ReactNode;
}

/** The frame of every dialog. Esc cancels, and the global hotkeys stay off while it is open. */
export function Dialog({ title, onCancel, children }: DialogProps) {
  useEffect(() => holdHotkeys(), []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && onCancel();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  return (
    <div className="dialog-backdrop">
      <div className="dialog" role="dialog" aria-modal="true" aria-label={title}>
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
