import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";
import type { AudioDevice } from "../../shared/ipc";
import { DeviceSelect } from "./Settings";

// Tells React that `act` drives the updates here, as it does in a test.
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const MICROPHONES: AudioDevice[] = [
  { id: "usb", name: "USB Microphone" },
  { id: "headset", name: "Headset Microphone" },
];

let unmount = () => {};

/** A microphone that opens only when the test says so, like one on a slow driver. */
function slowDriver() {
  let open = () => {};
  const chosen: (string | null)[] = [];
  const onChoose = (id: string | null) => {
    chosen.push(id);
    return new Promise<null>((resolve) => (open = () => resolve(null)));
  };
  return { onChoose, chosen, open: () => open() };
}

function draw(selected: string | null, onChoose: (id: string | null) => Promise<unknown>) {
  const host = document.createElement("div");
  const root = createRoot(host);
  const render = (selected: string | null) =>
    act(() => root.render(<DeviceSelect title="Mic" devices={MICROPHONES} selected={selected} onChoose={onChoose} />));
  render(selected);
  unmount = () => act(() => root.unmount());
  const select = host.querySelector("select")!;
  const pick = (id: string) =>
    act(() => {
      select.value = id;
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
  return { select, pick, render };
}

describe("a device select", () => {
  afterEach(() => unmount());

  it("keeps the device the user picked while it opens", async () => {
    const driver = slowDriver();
    const screen = draw("usb", driver.onChoose);

    screen.pick("headset");

    expect(driver.chosen).toEqual(["headset"]);
    expect(screen.select.value).toBe("headset");
    screen.render("headset");
    await act(async () => driver.open());
    expect(screen.select.value).toBe("headset");
  });

  it("goes back to the device in use when the one picked did not open", async () => {
    const driver = slowDriver();
    const screen = draw("usb", driver.onChoose);

    screen.pick("headset");
    await act(async () => driver.open());

    expect(screen.select.value).toBe("usb");
  });
});
