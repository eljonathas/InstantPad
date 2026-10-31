import { describe, expect, it } from "vitest";
import { translate, type TextId } from "../../shared/i18n";
import { describeNumericKeys } from "./numericHint";

const t = (id: TextId, args?: string[]) => translate("en", id, args);

describe("numeric shortcut hint", () => {
  it("names the keys of the chosen scheme", () => {
    expect(describeNumericKeys("ctrlNumpad", ["Ctrl+Num1", "Ctrl+Num0", "Ctrl+Num+", "Ctrl+Num-", "Ctrl+Num,"], t)).toBe(
      "Ctrl+Num1 to Ctrl+Num0 play the first 10 pads of the active interface. Ctrl+Num+ and Ctrl+Num- switch interfaces. Ctrl+Num, stops everything.",
    );
  });

  it("adds the NumLock advice only for the plain numeric keypad", () => {
    expect(describeNumericKeys("numpad", ["Num1", "Num0", "Num+", "Num-", "Num,"], t)).toContain("Keep NumLock on");
    expect(describeNumericKeys("altDigits", ["Alt+1", "Alt+0", "Alt+=", "Alt+-", "Alt+."], t)).not.toContain("NumLock");
  });

  it("says only the pad shortcuts work when the scheme is off", () => {
    expect(describeNumericKeys("off", [], t)).toBe("Only the shortcuts you set on each pad work.");
  });
});
