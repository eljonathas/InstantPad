import { useEffect, useRef } from "react";
import { commands } from "../../shared/ipc";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { ToggleKey } from "../../shared/ui/ToggleKey";
import "./InterfaceBar.css";
import type { useBoardActions } from "./useBoardActions";

type BoardActions = ReturnType<typeof useBoardActions>;

interface InterfaceBarProps {
  actions: BoardActions;
}

/** The interface tabs, with the keys that step through them, add one, configure it and stop everything. */
export function InterfaceBar({ actions }: InterfaceBarProps) {
  const { t } = useText();
  const interfaces = useBackend((state) => state.board.interfaces);
  const activeId = useBackend((state) => state.board.activeId);
  const tabs = useRef<HTMLDivElement>(null);
  const active = interfaces.find((tab) => tab.id === activeId);

  // A step with the arrows may select a tab that is scrolled out of sight.
  useEffect(() => {
    tabs.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [activeId]);

  return (
    <div className="interface-bar" data-nodrag>
      <button
        className="key key-bare interface-step"
        title={t("PreviousInterface")}
        onClick={() => commands.stepInterface(-1)}
      >
        ◀
      </button>
      <div ref={tabs} className="interface-tabs" role="tablist">
        {interfaces.map((tab, index) => (
          <ToggleKey
            key={tab.id}
            kind="tab"
            className="interface-tab"
            label={tab.name}
            on={tab.id === activeId}
            onClick={() => commands.selectInterface(tab.id)}
            onContextMenu={(event) => actions.openInterfaceMenu(event, tab, index)}
          />
        ))}
      </div>
      <div className="interface-keys">
        <button
          className="key key-bare interface-step"
          title={t("NextInterface")}
          onClick={() => commands.stepInterface(1)}
        >
          ▶
        </button>
        <button
          className="key key-bare key-accent interface-plus"
          title={t("NewInterfaceTitle")}
          aria-label={t("NewInterfaceTitle")}
          onClick={() => void actions.addInterface()}
        >
          <i className="icon icon-plus" />
        </button>
        <button
          className="key key-bare interface-plus"
          title={t("ConfigureThisInterface")}
          aria-label={t("ConfigureThisInterface")}
          disabled={!active}
          onClick={() => active && void actions.configureInterface(active)}
        >
          <i className="icon icon-cog" />
        </button>
        <button className="key key-bare key-stop interface-stop" onClick={() => commands.stopAll()}>
          <i className="icon icon-stop" />
          {t("StopAll")}
        </button>
      </div>
    </div>
  );
}
