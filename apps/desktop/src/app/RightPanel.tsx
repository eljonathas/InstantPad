import { useState } from "react";
import { Browser } from "../features/browser/Browser";
import { Favorites } from "../features/favorites/Favorites";
import { Settings } from "../features/setup/Settings";
import { UpdateStatus } from "../features/updates/UpdateStatus";
import { useBackend } from "../shared/store/backend";
import { useText } from "../shared/store/text";
import { ToggleKey } from "../shared/ui/ToggleKey";
import "./RightPanel.css";

type Tab = "browse" | "favorites" | "settings";

/** The built-in screen: MyInstants, Favorites and Settings. All three stay alive while hidden. */
export function RightPanel() {
  const { t } = useText();
  const [tab, setTab] = useState<Tab>("browse");
  const targetSlot = useBackend((state) => state.board.targetSlot);
  const [seenTarget, setSeenTarget] = useState(targetSlot);

  // Picking an empty pad means a sound is wanted, so the settings give way to the sources.
  if (seenTarget !== targetSlot) {
    setSeenTarget(targetSlot);
    if (targetSlot !== null && tab === "settings") setTab("browse");
  }

  const tabs: { id: Tab; label: string }[] = [
    { id: "browse", label: "MyInstants" },
    { id: "favorites", label: t("FavoritesTab") },
    { id: "settings", label: t("SettingsTab") },
  ];

  return (
    <div className="right-panel">
      <div className="panel-tabs" role="tablist">
        {tabs.map(({ id, label }) => (
          <ToggleKey key={id} kind="tab" label={label} on={tab === id} onClick={() => setTab(id)} />
        ))}
      </div>
      <div className="panel-screen" data-nodrag>
        <div className="panel-page" hidden={tab !== "browse"}>
          <Browser />
        </div>
        <div className="panel-page" hidden={tab !== "favorites"}>
          <Favorites />
        </div>
        <div className="panel-page" hidden={tab !== "settings"}>
          <Settings>
            <UpdateStatus />
          </Settings>
        </div>
      </div>
    </div>
  );
}
