import type { MouseEvent } from "react";

interface ToggleKeyProps {
  label: string;
  on: boolean;
  /** A key that picks one of several (tabs) or one that switches by itself. */
  kind: "tab" | "switch";
  className?: string;
  title?: string;
  onClick: () => void;
  onContextMenu?: (event: MouseEvent) => void;
}

/** A hardware key with a LED strip that lights in the accent color. */
export function ToggleKey({ label, on, kind, className, title, onClick, onContextMenu }: ToggleKeyProps) {
  const state = kind === "tab" ? { role: "tab", "aria-selected": on } : { "aria-pressed": on };

  return (
    <button
      type="button"
      className={className ? `toggle ${className}` : "toggle"}
      title={title}
      onClick={onClick}
      onContextMenu={onContextMenu}
      {...state}
    >
      <i className="toggle-led" />
      <span className="toggle-label">{label}</span>
    </button>
  );
}
