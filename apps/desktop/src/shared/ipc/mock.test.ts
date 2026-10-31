import { describe, expect, it } from "vitest";
import { commands } from ".";
import { installMock } from "./mock";

describe("the browser mock", () => {
  installMock();

  it("starts with a board to draw", async () => {
    const view = await commands.start("Main");

    expect(view.board.pads).toHaveLength(24);
    expect(view.board.pads[0].sound?.name).toBe("Vine boom");
  });

  it("answers the catalog with sounds", async () => {
    const page = await commands.trending(1);

    expect(page.status === "ok" && page.data.sounds.length).toBeGreaterThan(0);
  });
});
