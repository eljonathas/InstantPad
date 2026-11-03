import { describe, expect, it } from "vitest";
import { HINT_AFTER_MS, listeningHint } from "./listeningHint";

describe("the hint while the AI listens", () => {
  it("says nothing while the listening is young, whatever the microphone does", () => {
    expect(listeningHint(0, 0, 0)).toBeNull();
    expect(listeningHint(0, 0.8, HINT_AFTER_MS - 1)).toBeNull();
  });

  it("says nothing once some talk was counted", () => {
    expect(listeningHint(1, 0, HINT_AFTER_MS * 10)).toBeNull();
    expect(listeningHint(50, 0.5, HINT_AFTER_MS * 10)).toBeNull();
  });

  it("blames a silent microphone when nothing came through it", () => {
    expect(listeningHint(0, 0, HINT_AFTER_MS)).toBe("AiNoSound");
    expect(listeningHint(0, 0.009, HINT_AFTER_MS)).toBe("AiNoSound");
  });

  it("asks for a voice when the microphone heard something that was not one", () => {
    expect(listeningHint(0, 0.01, HINT_AFTER_MS)).toBe("AiNotAVoiceYet");
    expect(listeningHint(0, 0.6, HINT_AFTER_MS * 3)).toBe("AiNotAVoiceYet");
  });
});
