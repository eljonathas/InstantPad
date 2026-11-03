import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";
import type { AiView, Calibration } from "../../shared/ipc";
import { AiSetup } from "./AiSetup";

// Tells React that `act` drives the updates here, as it does in a test.
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function waitingForTheOwnVoice(calibration: Calibration): AiView {
  return {
    status: { kind: "ready" },
    downloadMegabytes: 260,
    calibration,
    heardPercent: 0,
    waiting: { kind: "plain", text: "Lula" },
  };
}

let unmount = () => {};

function draw(ai: AiView) {
  const host = document.createElement("div");
  const root = createRoot(host);
  act(() => root.render(<AiSetup ai={ai} />));
  unmount = () => act(() => root.unmount());
  return host;
}

describe("the setup of an AI voice", () => {
  afterEach(() => unmount());

  it("shows the progress and the microphone while the AI listens", () => {
    const screen = draw(waitingForTheOwnVoice("listening"));

    expect(screen.querySelector("progress")).not.toBeNull();
    expect(screen.querySelector("meter")).not.toBeNull();
  });

  it("does not ask the user to talk while nothing records the microphone", () => {
    const screen = draw(waitingForTheOwnVoice("idle"));

    expect(screen.querySelector("progress")).toBeNull();
    expect(screen.querySelector("meter")).toBeNull();
    expect(screen.textContent).toBe("The AI starts learning your voice as soon as the microphone is free.");
  });
});
