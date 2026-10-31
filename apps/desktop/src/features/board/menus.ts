import type { InterfaceTab, PadView } from "../../shared/ipc";
import { PAD_COLORS } from "../../shared/palette";
import type { Translate } from "../../shared/store/text";
import type { MenuEntry } from "../../shared/ui/ContextMenu";

const VOLUME_PERCENTS = [25, 50, 75, 100, 150];
const SAME_VOLUME = 0.01;

export interface PadMenuActions {
  loadFile: () => void;
  toggleFavorite: () => void;
  download: () => void;
  setShortcut: () => void;
  removeShortcut: () => void;
  rename: () => void;
  setVolume: (volume: number) => void;
  setColor: (color: string) => void;
  sendTo: (interfaceId: string) => void;
  remove: () => void;
}

/** The right-click menu of a pad (BoardMenus.ForPad). `others` are the interfaces but the active one. */
export function padMenu(pad: PadView, others: InterfaceTab[], t: Translate, run: PadMenuActions): MenuEntry[] {
  const { sound } = pad;
  if (!sound) return [{ label: t("MenuLoadFile"), run: run.loadFile }];

  return [
    { label: pad.isFavorite ? t("MenuUnfavorite") : t("MenuFavorite"), run: run.toggleFavorite },
    { label: t("MenuDownload"), run: run.download },
    "separator",
    { label: t("MenuSetShortcut"), run: run.setShortcut },
    ...(sound.hasHotkey ? [{ label: t("MenuRemoveShortcut"), run: run.removeShortcut }] : []),
    { label: t("MenuRename"), run: run.rename },
    {
      label: t("Volume"),
      children: VOLUME_PERCENTS.map((percent) => ({
        label: `${percent}%`,
        checked: Math.abs(sound.volume - percent / 100) < SAME_VOLUME,
        run: () => run.setVolume(percent / 100),
      })),
    },
    {
      label: t("Color"),
      children: PAD_COLORS.map((color) => ({ label: color, swatch: color, run: () => run.setColor(color) })),
    },
    ...(others.length > 0
      ? [
          {
            label: t("MenuSendToInterface"),
            children: others.map((tab) => ({ label: tab.name, run: () => run.sendTo(tab.id) })),
          },
        ]
      : []),
    "separator",
    { label: t("MenuRemoveFromPad"), run: run.remove },
  ];
}

export interface InterfaceMenuActions {
  configure: () => void;
  move: (offset: number) => void;
  remove: () => void;
}

/** The right-click menu of an interface tab (BoardMenus.ForInterface). */
export function interfaceMenu(index: number, count: number, t: Translate, run: InterfaceMenuActions): MenuEntry[] {
  return [
    { label: t("MenuConfigure"), run: run.configure },
    { label: t("MenuMoveLeft"), disabled: index === 0, run: () => run.move(-1) },
    { label: t("MenuMoveRight"), disabled: index === count - 1, run: () => run.move(1) },
    "separator",
    { label: t("DeleteInterfaceTitle"), disabled: count < 2, run: run.remove },
  ];
}
