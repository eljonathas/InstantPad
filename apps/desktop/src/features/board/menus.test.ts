import { describe, expect, it, vi } from "vitest";
import { translate, type TextId } from "../../shared/i18n";
import type { InterfaceTab, PadView, SoundView } from "../../shared/ipc";
import type { MenuEntry } from "../../shared/ui/ContextMenu";
import { interfaceMenu, padMenu, type PadMenuActions } from "./menus";

const colors = Array.from({ length: 8 }, (_, index) => `#00000${index}`);

const t = (id: TextId, args?: string[]) => translate("en", id, args);

const sound: SoundView = {
  id: "s",
  name: "Bruh",
  path: "/bruh.mp3",
  color: "#1fd6ff",
  volume: 1,
  hasHotkey: false,
  isLight: false,
};
const loaded: PadView = { slot: 1, label: "02", shortcut: "Num2", isFavorite: false, isLoading: false, sound };
const empty: PadView = { slot: 2, label: "03", shortcut: "Num3", isFavorite: false, isLoading: false, sound: null };
const memes: InterfaceTab = { id: "memes", name: "Memes", rows: 2, columns: 4, accent: "#35f27a", soundCount: 0 };

type Item = Exclude<MenuEntry, "separator">;

const actions = (): PadMenuActions => ({
  loadFile: vi.fn(),
  toggleFavorite: vi.fn(),
  download: vi.fn(),
  setShortcut: vi.fn(),
  removeShortcut: vi.fn(),
  rename: vi.fn(),
  setVolume: vi.fn(),
  setColor: vi.fn(),
  sendTo: vi.fn(),
  remove: vi.fn(),
});

const labels = (entries: MenuEntry[]) => entries.map((entry) => (entry === "separator" ? "-" : entry.label));
const item = (entries: MenuEntry[], label: string) =>
  entries.find((entry) => entry !== "separator" && entry.label === label) as Item;

describe("pad menu", () => {
  it("only offers to load a file when the pad is empty", () => {
    const run = actions();
    const menu = padMenu(empty, [], colors, t, run);

    expect(labels(menu)).toEqual(["Load audio file..."]);
    item(menu, "Load audio file...").run!();
    expect(run.loadFile).toHaveBeenCalled();
  });

  it("offers everything for a pad with a sound", () => {
    const menu = padMenu(loaded, [memes], colors, t, actions());

    expect(labels(menu)).toEqual([
      "Add to favorites",
      "Download to computer...",
      "-",
      "Set shortcut...",
      "Rename...",
      "Volume",
      "Color",
      "Send to interface",
      "-",
      "Remove from pad",
    ]);
  });

  it("changes the favorite wording, offers to remove a shortcut, and hides Send to without other interfaces", () => {
    const favorite = { ...loaded, isFavorite: true, sound: { ...sound, hasHotkey: true } };
    const menu = padMenu(favorite, [], colors, t, actions());

    expect(labels(menu)).toContain("Remove from favorites");
    expect(labels(menu)).toContain("Remove shortcut");
    expect(labels(menu)).not.toContain("Send to interface");
  });

  it("checks the current volume among 25, 50, 75, 100 and 150%", () => {
    const run = actions();
    const volume = item(padMenu({ ...loaded, sound: { ...sound, volume: 1.5 } }, [], colors, t, run), "Volume");
    const choices = volume.children as Item[];

    expect(choices.map((choice) => choice.label)).toEqual(["25%", "50%", "75%", "100%", "150%"]);
    expect(choices.map((choice) => choice.checked)).toEqual([false, false, false, false, true]);
    choices[0].run!();
    expect(run.setVolume).toHaveBeenCalledWith(0.25);
  });

  it("lists the other interfaces to send the sound to, and eight colors", () => {
    const run = actions();
    const menu = padMenu(loaded, [memes], colors, t, run);

    (item(menu, "Send to interface").children![0] as Item).run!();
    expect(run.sendTo).toHaveBeenCalledWith("memes");
    expect(item(menu, "Color").children).toHaveLength(8);
  });
});

describe("interface menu", () => {
  const run = { configure: vi.fn(), move: vi.fn(), remove: vi.fn() };

  it("cannot move the first tab left, the last right, or delete the only one", () => {
    const first = interfaceMenu(0, 2, t, run);
    const only = interfaceMenu(0, 1, t, run);

    expect(item(first, "Move left").disabled).toBe(true);
    expect(item(first, "Move right").disabled).toBe(false);
    expect(item(first, "Delete interface").disabled).toBe(false);
    expect(item(only, "Move right").disabled).toBe(true);
    expect(item(only, "Delete interface").disabled).toBe(true);
  });
});
