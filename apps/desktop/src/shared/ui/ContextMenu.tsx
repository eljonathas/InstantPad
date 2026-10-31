import { useEffect, useLayoutEffect, useRef, useState } from "react";
import "./ContextMenu.css";

export type MenuEntry =
  | "separator"
  | {
      label: string;
      run?: () => void;
      disabled?: boolean;
      checked?: boolean;
      /** Draws a color chip instead of the label, which stays as its accessible name. */
      swatch?: string;
      children?: MenuEntry[];
    };

interface ContextMenuProps {
  x: number;
  y: number;
  entries: MenuEntry[];
  onClose: () => void;
}

export function ContextMenu({ x, y, entries, onClose }: ContextMenuProps) {
  const menu = useRef<HTMLDivElement>(null);

  // Measured after layout so a menu near the right or bottom edge opens toward the free side.
  useLayoutEffect(() => {
    const box = menu.current!.getBoundingClientRect();
    const style = menu.current!.style;
    if (x + box.width > window.innerWidth) style.left = `${Math.max(0, x - box.width)}px`;
    if (y + box.height > window.innerHeight) style.top = `${Math.max(0, y - box.height)}px`;
  }, [x, y]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="menu-backdrop"
      onPointerDown={onClose}
      onContextMenu={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      <div
        ref={menu}
        className="menu"
        role="menu"
        style={{ left: x, top: y }}
        onPointerDown={(event) => event.stopPropagation()}
        onContextMenu={(event) => event.preventDefault()}
      >
        <Entries entries={entries} onClose={onClose} />
      </div>
    </div>
  );
}

function Entries({ entries, onClose }: { entries: MenuEntry[]; onClose: () => void }) {
  const [open, setOpen] = useState<number | null>(null);

  return entries.map((entry, index) => {
    if (entry === "separator") return <hr key={index} className="menu-separator" />;

    const { label, run, disabled, checked, swatch, children } = entry;
    return (
      <div key={index} className="menu-row" onPointerEnter={() => setOpen(children ? index : null)}>
        <button
          className="menu-item"
          role={checked === undefined ? "menuitem" : "menuitemradio"}
          aria-haspopup={children ? "menu" : undefined}
          aria-checked={checked}
          aria-label={swatch ? label : undefined}
          disabled={disabled}
          onClick={() => {
            if (children) return setOpen(index);
            // Closed first: running the entry may open a dialog in this same layer.
            onClose();
            run?.();
          }}
        >
          <span className="menu-check">{checked ? "✓" : ""}</span>
          {swatch ? (
            <span className="menu-swatch" style={{ background: swatch }} />
          ) : (
            <span className="menu-label">{label}</span>
          )}
        </button>
        {children && open === index && <SubMenu entries={children} onClose={onClose} />}
      </div>
    );
  });
}

function SubMenu({ entries, onClose }: { entries: MenuEntry[]; onClose: () => void }) {
  const menu = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const box = menu.current!.getBoundingClientRect();
    const style = menu.current!.style;
    if (box.right > window.innerWidth) {
      style.left = "auto";
      style.right = "100%";
    }
    if (box.bottom > window.innerHeight) style.top = `${window.innerHeight - box.bottom}px`;
  }, []);

  return (
    <div ref={menu} className="menu menu-sub" role="menu">
      <Entries entries={entries} onClose={onClose} />
    </div>
  );
}
