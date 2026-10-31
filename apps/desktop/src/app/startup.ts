import { useEffect } from "react";
import { resolveLanguage, translate } from "../shared/i18n";
import { commands, events } from "../shared/ipc";
import { applyEvent, loadStart } from "../shared/store/backend";
import { meter } from "../shared/store/meter";

// The events last as long as the window, so they are subscribed once, not per effect run.
let listening: Promise<unknown> | undefined;

function listenOnce() {
  listening ??= Promise.all([
    events.backendEvent.listen((event) => applyEvent(event.payload)),
    events.meterEvent.listen((event) => meter.publish(event.payload)),
  ]);
  return listening;
}

/**
 * Listens first, so no event is lost, then asks the backend to start. StrictMode runs this
 * twice in development; `start` is safe to call again.
 */
export function useStartup() {
  useEffect(() => {
    let leaving = false;

    void (async () => {
      await listenOnce();

      // The saved language is only known after `start`, so the first interface is named in the system's.
      const language = resolveLanguage(null, navigator.language);
      const view = await commands.start(translate(language, "DefaultInterfaceName"));
      if (leaving) return;

      loadStart(view);
      await commands.loadDevices();
    })();

    return () => {
      leaving = true;
    };
  }, []);
}
