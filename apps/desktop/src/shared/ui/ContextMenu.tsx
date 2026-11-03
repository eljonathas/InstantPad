import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
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
  /** The button that opened the menu: the menu opens under it and gives the focus back to it. */
  anchor?: HTMLElement;
  onClose: () => void;
}

const ITEMS = ":scope > .menu-row > .menu-item:not(:disabled)";

/** Moves the focus among the entries of one menu level, as the arrows, Home and End do. */
function moveFocus(menu: HTMLElement, key: string) {
  const items = [...menu.querySelectorAll<HTMLButtonElement>(ITEMS)];
  if (items.length === 0) return false;
  const at = items.indexOf(document.activeElement as HTMLButtonElement);
  const last = items.length - 1;
  // The arrows go round from one end to the other.
  const targets: Record<string, number> = {
    ArrowDown: at === last ? 0 : at + 1,
    ArrowUp: at <= 0 ? last : at - 1,
    Home: 0,
    End: last,
  };
  const next = targets[key];
  if (next === undefined) return false;
  items[next].focus();
  return true;
}

/** Arrows, Home and End within the level the key was pressed in; the levels above leave it alone. */
function onMenuKey(event: KeyboardEvent<HTMLDivElement>) {
  if (moveFocus(event.currentTarget, event.key)) {
    event.preventDefault();
    event.stopPropagation();
  }
}

export function ContextMenu({ x, y, entries, anchor, onClose }: ContextMenuProps) {
  const menu = useRef<HTMLDivElement>(null);

  // Measured after layout so a menu near the right or bottom edge opens toward the free side.
  // A menu of a button opens under it, its right edge on the button's, or above it near the bottom.
  useLayoutEffect(() => {
    const box = menu.current!.getBoundingClientRect();
    const style = menu.current!.style;
    if (anchor) {
      const button = anchor.getBoundingClientRect();
      style.left = `${Math.max(0, Math.min(button.right, window.innerWidth) - box.width)}px`;
      const below = button.bottom + box.height <= window.innerHeight;
      style.top = `${below ? button.bottom : Math.max(0, button.top - box.height)}px`;
    } else {
      if (x + box.width > window.innerWidth) style.left = `${Math.max(0, x - box.width)}px`;
      if (y + box.height > window.innerHeight) style.top = `${Math.max(0, y - box.height)}px`;
    }
    menu.current!.querySelector<HTMLElement>(ITEMS)?.focus();
  }, [x, y, anchor]);

  useEffect(() => {
    const onKey = (event: globalThis.KeyboardEvent) => event.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  // Back to the button that opened it, as the keyboard expects of a menu button.
  useEffect(() => () => anchor?.focus(), [anchor]);

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
        aria-label={anchor?.getAttribute("aria-label") ?? undefined}
        style={{ left: x, top: y }}
        onPointerDown={(event) => event.stopPropagation()}
        onContextMenu={(event) => event.preventDefault()}
        onKeyDown={(event) => {
          // Tab leaves the menu, and the focus goes on from the button that opened it.
          if (event.key === "Tab") return onClose();
          onMenuKey(event);
        }}
      >
        <Entries entries={entries} onClose={onClose} />
      </div>
    </div>
  );
}

function Entries({ entries, onClose }: { entries: MenuEntry[]; onClose: () => void }) {
  // `byKey` tells a submenu opened from the keyboard to take the focus.
  const [open, setOpen] = useState<{ index: number; byKey: boolean } | null>(null);

  return entries.map((entry, index) => {
    if (entry === "separator") return <hr key={index} className="menu-separator" />;

    const { label, run, disabled, checked, swatch, children } = entry;
    const isOpen = open?.index === index;
    return (
      <div
        key={index}
        className="menu-row"
        onPointerEnter={() => setOpen(children ? { index, byKey: false } : null)}
      >
        <button
          className="menu-item"
          role={checked === undefined ? "menuitem" : "menuitemradio"}
          aria-haspopup={children ? "menu" : undefined}
          aria-expanded={children ? isOpen : undefined}
          aria-checked={checked}
          aria-label={swatch ? label : undefined}
          disabled={disabled}
          onKeyDown={(event) => {
            if (!children || event.key !== "ArrowRight") return;
            event.preventDefault();
            event.stopPropagation();
            setOpen({ index, byKey: true });
          }}
          onClick={(event) => {
            // Enter and Space click too, and a submenu they open takes the focus.
            if (children) return setOpen({ index, byKey: event.detail === 0 });
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
        {children && isOpen && (
          <SubMenu entries={children} takeFocus={open.byKey} onBack={() => setOpen(null)} onClose={onClose} />
        )}
      </div>
    );
  });
}

interface SubMenuProps {
  entries: MenuEntry[];
  takeFocus: boolean;
  onBack: () => void;
  onClose: () => void;
}

function SubMenu({ entries, takeFocus, onBack, onClose }: SubMenuProps) {
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

  useLayoutEffect(() => {
    if (takeFocus) menu.current!.querySelector<HTMLElement>(ITEMS)?.focus();
  }, [takeFocus]);

  return (
    <div
      ref={menu}
      className="menu menu-sub"
      role="menu"
      onKeyDown={(event) => {
        if (event.key === "ArrowLeft") {
          event.preventDefault();
          event.stopPropagation();
          // The entry that opened it, the button just before this menu.
          menu.current!.parentElement!.querySelector<HTMLElement>(":scope > .menu-item")?.focus();
          return onBack();
        }
        onMenuKey(event);
      }}
    >
      <Entries entries={entries} onClose={onClose} />
    </div>
  );
}
