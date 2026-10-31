import { describe, expect, it } from "vitest";
import { translate, type TextId } from "../../shared/i18n";
import { describeNumericKeys } from "./numericHint";

const t = (id: TextId, args?: string[]) => translate("en", id, args);

describe("numeric shortcut hint", () => {
  it("names the keys of the chosen scheme and the stop key shown on the board", () => {
    expect(describeNumericKeys("ctrlNumpad", "Ctrl+Num,", t)).toBe(
      "Ctrl+Num1 to Ctrl+Num0 play the first 10 pads of the active interface. Ctrl+Num+ and Ctrl+Num- switch interfaces. Ctrl+Num, stops everything.",
    );
  });

  it("adds the NumLock advice only for the plain numeric keypad", () => {
    expect(describeNumericKeys("numpad", "Num,", t)).toContain("Keep NumLock on");
    expect(describeNumericKeys("altDigits", "Alt+.", t)).not.toContain("NumLock");
  });

  it("says only the pad shortcuts work when the scheme is off", () => {
    expect(describeNumericKeys("off", null, t)).toBe("Only the shortcuts you set on each pad work.");
  });
});
