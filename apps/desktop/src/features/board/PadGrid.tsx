import { useBackend } from "../../shared/store/backend";
import { Pad } from "./Pad";
import type { useBoardActions } from "./useBoardActions";

type BoardActions = ReturnType<typeof useBoardActions>;

export function PadGrid({ onMenu }: { onMenu: BoardActions["openPadMenu"] }) {
  const pads = useBackend((state) => state.board.pads);
  const targetSlot = useBackend((state) => state.board.targetSlot);
  const active = useBackend((state) => state.board.interfaces.find((tab) => tab.id === state.board.activeId));

  return (
    <div
      className="pad-grid"
      data-nodrag
      style={{
        gridTemplateRows: `repeat(${active?.rows ?? 1}, minmax(0, 1fr))`,
        gridTemplateColumns: `repeat(${active?.columns ?? 1}, minmax(0, 1fr))`,
      }}
    >
      {pads.map((pad) => (
        <Pad key={pad.slot} pad={pad} isTarget={pad.slot === targetSlot} onMenu={onMenu} />
      ))}
    </div>
  );
}
