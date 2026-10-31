import { useEffect } from "react";
import { useBackend } from "../shared/store/backend";
import { useText } from "../shared/store/text";
import { OverlayProvider } from "../shared/ui/overlay";
import { Device } from "./Device";
import { useFileDrop } from "./useFileDrop";
import { useStartup } from "./startup";

export function App() {
  const { language } = useText();
  const accent = useBackend((state) => state.board.interfaces.find((tab) => tab.id === state.board.activeId)?.accent);

  useStartup();
  useFileDrop();

  // The accent follows the active interface; until one is known the token's own value stands.
  useEffect(() => {
    if (accent) document.documentElement.style.setProperty("--color-accent", accent);
  }, [accent]);

  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

  // The window is a device, not a web page: no browser menu.
  useEffect(() => {
    const block = (event: Event) => event.preventDefault();
    window.addEventListener("contextmenu", block);
    return () => window.removeEventListener("contextmenu", block);
  }, []);

  return (
    <OverlayProvider>
      <Device />
    </OverlayProvider>
  );
}
