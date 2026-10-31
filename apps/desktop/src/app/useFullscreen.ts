import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useState } from "react";

/** Full screen, from the key on the device or F11. The system can leave it too, so the window is asked. */
export function useFullscreen() {
  const [isFullscreen, setIsFullscreen] = useState(false);

  const toggle = useCallback(() => void getCurrentWindow().setFullscreen(!isFullscreen), [isFullscreen]);

  useEffect(() => {
    const sync = () => void getCurrentWindow().isFullscreen().then(setIsFullscreen);
    window.addEventListener("resize", sync);
    return () => window.removeEventListener("resize", sync);
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "F11") return;
      event.preventDefault();
      toggle();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggle]);

  return { isFullscreen, toggle };
}
