import { describe, expect, it } from "vitest";
import { commands } from ".";
import { installMock } from "./mock";

describe("the browser mock", () => {
  installMock();

  it("starts with a board to draw", async () => {
    const view = await commands.start("Main", "pt-BR");

    expect(view.board.pads).toHaveLength(24);
    expect(view.board.pads[0].sound?.name).toBe("Vine boom");
  });

  it("starts with the AI voices waiting for their download", async () => {
    const { voice } = await commands.start("Main", "pt-BR");

    expect(voice.ai.status).toEqual({ kind: "missing" });
    expect(voice.effects.find((effect) => effect.effect === "feminineAi")?.needsAi).toBe(true);
    expect(voice.effects.filter((effect) => effect.needsAi)).toHaveLength(1);
  });

  it("follows the country of the system for the trending sounds", async () => {
    const view = await commands.start("Main", "de-DE");

    expect(view.setup.trendingRegion).toBe("de");
  });

  it("answers the catalog with sounds", async () => {
    const page = await commands.trending(1);

    expect(page.status === "ok" && page.data.sounds.length).toBeGreaterThan(0);
  });
});
