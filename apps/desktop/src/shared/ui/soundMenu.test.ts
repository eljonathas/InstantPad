import { describe, expect, it, vi } from "vitest";
import { translate, type TextId } from "../i18n";
import type { MenuEntry } from "./ContextMenu";
import { soundMenu, type SoundMenuActions } from "./soundMenu";

const t = (id: TextId, args?: string[]) => translate("en", id, args);

type Item = Exclude<MenuEntry, "separator">;

const actions = (): SoundMenuActions => ({
  playInGame: vi.fn(),
  addToPad: vi.fn(),
  cut: vi.fn(),
});

const labels = (entries: MenuEntry[]) => entries.map((entry) => (entry === "separator" ? "-" : entry.label));

describe("the menu of a sound", () => {
  it("holds the actions that left the row", () => {
    expect(labels(soundMenu(t, actions()))).toEqual(["Play in the game now", "Save to a pad", "-", "Cut into pads..."]);
  });

  it("runs the action of each entry", () => {
    const run = actions();
    for (const entry of soundMenu(t, run)) if (entry !== "separator") (entry as Item).run?.();

    for (const action of Object.values(run)) expect(action).toHaveBeenCalledOnce();
  });
});
