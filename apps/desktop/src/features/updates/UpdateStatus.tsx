import { commands } from "../../shared/ipc";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { ToggleKey } from "../../shared/ui/ToggleKey";

/** How the background update is going, and the key that restarts into it. */
export function UpdateStatus() {
  const { t, render } = useText();
  const update = useBackend((state) => state.update);
  const isQuiet = useBackend((state) => state.setup.quietUpdates);

  return (
    <section>
      <h3 className="setting-title screen-text">{t("Update")}</h3>
      {update && (
        <p className="screen-text setting-note" role="status">
          {render(update.status)}
        </p>
      )}
      {update?.isReadyToRestart && (
        <button className="key key-amber setting-restart" onClick={() => commands.restartNow()}>
          {t("RestartNow")}
        </button>
      )}
      <ToggleKey
        kind="switch"
        className="setting-spaced"
        label={t("UpdateWindow")}
        on={!isQuiet}
        onClick={() => void commands.setQuietUpdates(!isQuiet)}
      />
      <p className="screen-text setting-note">{t("UpdateWindowHint")}</p>
    </section>
  );
}
