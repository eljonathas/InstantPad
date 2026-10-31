import { describe, expect, it } from "vitest";
import type { Message } from "../ipc";
import en from "./en.json";
import { renderMessage, resolveLanguage, translate } from ".";
import pt from "./pt.json";

const placeholders = (text: string) => (text.match(/\{\d+\}/g) ?? []).sort();

describe("translations", () => {
  it("have the same keys and placeholders in every language", () => {
    expect(Object.keys(pt)).toEqual(Object.keys(en));
    for (const [key, text] of Object.entries(en)) {
      expect(placeholders(pt[key as keyof typeof pt])).toEqual(placeholders(text));
    }
  });

  it("follow the saved language, then the system one", () => {
    expect(resolveLanguage("en", "pt-BR")).toBe("en");
    expect(resolveLanguage(null, "pt-BR")).toBe("pt");
    expect(resolveLanguage(null, "de-DE")).toBe("en");
  });

  it("fill in the placeholders", () => {
    expect(translate("en", "EmptyPad", ["03"])).toBe("Pad 03 is empty");
  });

  it("render messages nested inside other messages", () => {
    const message: Message = {
      kind: "text",
      key: "FailedSound",
      args: [
        { kind: "plain", text: "boom" },
        { kind: "text", key: "SoundHasNoAudio", args: [] },
      ],
    };

    expect(renderMessage("en", message)).toBe("boom failed: The file has no audio.");
  });
});
