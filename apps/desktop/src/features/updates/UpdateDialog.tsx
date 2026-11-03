import { useState } from "react";
import { commands } from "../../shared/ipc";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { Dialog } from "../../shared/ui/Dialog";

/**
 * Says that a new version is ready, once per version, unless the user asked for quiet
 * updates. Then the version only shows in the settings, where Restart now stays.
 */
export function UpdateDialog() {
  const { t, render } = useText();
  const update = useBackend((state) => state.update);
  const isQuiet = useBackend((state) => state.setup.quietUpdates);
  // The status names the version, so a later version opens the window again.
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [quiet, setQuiet] = useState(false);

  if (!update?.isReadyToRestart || isQuiet) return null;
  const key = JSON.stringify(update.status);
  if (dismissed === key) return null;

  function later() {
    setDismissed(key);
    if (quiet) void commands.setQuietUpdates(true);
  }

  return (
    <Dialog title={t("UpdateReady")} onCancel={later} holdsHotkeys={false}>
      <p className="dialog-text" role="status">
        {render(update.status)}
      </p>
      <p className="dialog-tip">{t("UpdateReadyHint")}</p>
      <label className="dialog-check">
        <input type="checkbox" checked={quiet} onChange={(event) => setQuiet(event.target.checked)} />
        <span>{t("UpdateQuiet")}</span>
      </label>
      <div className="dialog-buttons">
        <button type="button" className="key" onClick={later}>
          {t("Later")}
        </button>
        <button type="button" className="key key-amber" onClick={() => void commands.restartNow()}>
          {t("RestartNow")}
        </button>
      </div>
    </Dialog>
  );
}
