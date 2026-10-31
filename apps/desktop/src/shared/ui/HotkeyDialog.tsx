import { useEffect } from "react";
import type { Hotkey } from "../ipc";
import { useText } from "../store/text";
import { Dialog } from "./Dialog";
import "./HotkeyDialog.css";
import { useOverlay } from "./overlay";

// A modifier pressed alone is not a shortcut yet.
const MODIFIER_CODES = /^(Control|Shift|Alt|Meta|OS)(Left|Right)$/;

interface HotkeyDialogProps {
  onDone: (hotkey: Hotkey | null) => void;
}

/** Waits for a key combination. Esc cancels, through the dialog frame. */
export function HotkeyDialog({ onDone }: HotkeyDialogProps) {
  const { t } = useText();

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      // Nothing may reach the page behind, not even Tab or F5.
      event.preventDefault();
      if (event.code === "Escape" || MODIFIER_CODES.test(event.code)) return;

      const { ctrlKey: ctrl, altKey: alt, shiftKey: shift, metaKey: meta } = event;
      onDone({ key: event.code, modifiers: { ctrl, alt, shift, meta } });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onDone]);

  return (
    <Dialog title={t("HotkeyTitle")} onCancel={() => onDone(null)}>
      <p className="hotkey-prompt">{t("HotkeyPrompt")}</p>
    </Dialog>
  );
}

export function useAskHotkey() {
  const overlay = useOverlay();
  return () => overlay.dialog<Hotkey>((done) => <HotkeyDialog onDone={done} />);
}
