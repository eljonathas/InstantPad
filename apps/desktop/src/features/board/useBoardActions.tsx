import { ask, open } from "@tauri-apps/plugin-dialog";
import type { MouseEvent } from "react";
import { commands, type InterfaceLayout, type InterfaceTab, type PadView } from "../../shared/ipc";
import { ACCENT_COLORS } from "../../shared/palette";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { useAskHotkey } from "../../shared/ui/HotkeyDialog";
import { whileHeld } from "../../shared/ui/hotkeyPause";
import { useOverlay } from "../../shared/ui/overlay";
import { saveToComputer } from "../../shared/ui/SoundRow";
import { InterfaceDialog } from "./InterfaceDialog";
import { interfaceMenu, padMenu } from "./menus";
import { TextDialog } from "./TextDialog";

const AUDIO_EXTENSIONS = ["mp3", "wav", "m4a", "aac", "flac"];
const NEW_INTERFACE = { rows: 4, columns: 6 };

/** What the menus and keys of the board do: ask the user what is needed, then call the backend. */
export function useBoardActions() {
  const overlay = useOverlay();
  const askHotkey = useAskHotkey();
  const { t } = useText();
  const interfaces = useBackend((state) => state.board.interfaces);
  const activeId = useBackend((state) => state.board.activeId);

  const askInterface = (title: string, initial: InterfaceLayout, soundCount: number) =>
    overlay.dialog<InterfaceLayout>((done) => (
      <InterfaceDialog title={title} initial={initial} soundCount={soundCount} onDone={done} />
    ));

  async function addInterface() {
    const initial = {
      name: t("DefaultNewInterfaceName", [String(interfaces.length + 1)]),
      ...NEW_INTERFACE,
      accent: ACCENT_COLORS[interfaces.length % ACCENT_COLORS.length],
    };
    const layout = await askInterface(t("NewInterfaceTitle"), initial, 0);
    if (layout) await commands.addInterface(layout);
  }

  async function configureInterface(tab: InterfaceTab) {
    const layout = await askInterface(t("ConfigureInterfaceTitle"), tab, tab.soundCount);
    if (layout) await commands.configureInterface(tab.id, layout);
  }

  async function deleteInterface(tab: InterfaceTab) {
    if (tab.soundCount > 0) {
      const question = t("DeleteInterfaceQuestion", [tab.name, String(tab.soundCount)]);
      const confirmed = await whileHeld(() => ask(question, { title: t("DeleteInterfaceTitle"), kind: "warning" }));
      if (!confirmed) return;
    }
    await commands.deleteInterface(tab.id);
  }

  async function loadFile(slot: number) {
    const path = await whileHeld(() => open({ filters: [{ name: t("AudioFileType"), extensions: AUDIO_EXTENSIONS }] }));
    if (path) await commands.importFile(path, slot);
  }

  async function setShortcut(slot: number) {
    const hotkey = await askHotkey();
    if (hotkey) await commands.setPadHotkey(slot, hotkey);
  }

  async function rename(slot: number, current: string) {
    const name = await overlay.dialog<string>((done) => (
      <TextDialog title={t("PadNameTitle")} initial={current} onDone={done} />
    ));
    if (name !== null) await commands.renamePad(slot, name);
  }

  return {
    addInterface,
    configureInterface,

    openInterfaceMenu(event: MouseEvent, tab: InterfaceTab, index: number) {
      event.preventDefault();
      const run = {
        configure: () => void configureInterface(tab),
        move: (offset: number) => void commands.moveInterface(tab.id, offset),
        remove: () => void deleteInterface(tab),
      };
      overlay.menu(event.clientX, event.clientY, interfaceMenu(index, interfaces.length, t, run));
    },

    openPadMenu(event: MouseEvent, pad: PadView) {
      event.preventDefault();
      const { slot, sound } = pad;
      const source = sound && { name: sound.name, color: sound.color, path: sound.path, soundUrl: null };
      const run = {
        loadFile: () => void loadFile(slot),
        toggleFavorite: () => void commands.toggleFavorite(source!),
        download: () => void saveToComputer(source!),
        setShortcut: () => void setShortcut(slot),
        removeShortcut: () => void commands.clearPadHotkey(slot),
        rename: () => void rename(slot, sound!.name),
        setVolume: (volume: number) => void commands.setPadVolume(slot, volume),
        setColor: (color: string) => void commands.setPadColor(slot, color),
        sendTo: (interfaceId: string) => void commands.sendToInterface(slot, interfaceId),
        remove: () => void commands.removePad(slot),
      };
      const others = interfaces.filter((tab) => tab.id !== activeId);
      overlay.menu(event.clientX, event.clientY, padMenu(pad, others, t, run));
    },
  };
}
