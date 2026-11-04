import { expect, type Locator, type Page } from "@playwright/test";

/** Opens the app on the browser mock and waits for the first board. */
export async function openApp(page: Page) {
  page.on("pageerror", (error) => {
    throw error;
  });
  await page.goto("/");
  await expect(pad(page, "01 Vine boom")).toBeVisible();
}

export const pad = (page: Page, name: string) => page.getByRole("button", { name, exact: true });

export const emptyPad = (page: Page, label: string) => pad(page, label);

export const lcd = (page: Page) => page.locator(".lcd-main");

// Search results and favorites both stay mounted; only the visible one counts.
export const row = (page: Page, name: string) => page.locator(".sound-row:visible", { hasText: name });

/** Runs an action of a sound row, which waits in the menu behind the row's options button. */
export async function soundAction(page: Page, name: string, action: string) {
  await row(page, name).getByRole("button", { name: `Options for ${name}` }).click();
  await page.getByRole("menuitem", { name: action }).click();
}

export const interfaceTabs = (page: Page) => page.locator(".interface-tabs").getByRole("tab");

const center = async (target: Locator) => {
  const box = (await target.boundingBox())!;
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
};

/** Presses on `from`, moves in steps, and releases over `to`. */
export async function drag(page: Page, from: Locator, to: Locator) {
  const start = await center(from);
  const end = await center(to);
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(end.x, end.y, { steps: 8 });
  await page.mouse.up();
}

/** Records the commands the interface sends, by wrapping the mock's `invoke`. */
export async function recordCommands(page: Page) {
  await page.evaluate(() => {
    const internals = (
      window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string, ...rest: unknown[]) => unknown } }
    ).__TAURI_INTERNALS__;
    const calls: string[] = [];
    const original = internals.invoke;
    internals.invoke = (command, ...rest) => {
      calls.push(command);
      return original(command, ...rest);
    };
    (window as unknown as { __calls: string[] }).__calls = calls;
  });
  return () => page.evaluate(() => (window as unknown as { __calls: string[] }).__calls);
}

/**
 * Lets the mock's recordings last as long as a test wants without waiting: the page's clock
 * jumps ahead while the recording runs.
 */
export async function recordFor(page: Page, seconds: number, within: Locator = page.locator("body")) {
  await within.getByRole("button", { name: "Record" }).click();
  await page.clock.fastForward(seconds * 1000);
  await within.getByRole("button", { name: "Stop" }).click();
}

/** Downloads the AI, which is all that cloning a voice asks for. */
export async function installTheAi(page: Page) {
  await page.getByRole("button", { name: "Clone a voice" }).click();
  await page.getByRole("button", { name: "Download" }).click();
  await expect(page.getByRole("button", { name: "Download" })).toHaveCount(0);
  await expect(page.locator(".lcd-main")).toHaveText("AI voices installed.");
}

/** The page's clock runs through the seconds the AI needs to hear the user talk, and to learn. */
export async function letTheAiListen(page: Page) {
  await page.clock.runFor(9000);
}

/** Picks Feminine AI, downloads the AI and lets it learn the user's voice, which then turns it on. */
export async function readyTheAi(page: Page) {
  await page.getByRole("button", { name: "Feminine AI", exact: true }).dispatchEvent("pointerdown");
  await page.getByRole("button", { name: "Download" }).click();
  await letTheAiListen(page);
  await expect(page.getByRole("slider", { name: "Pitch" })).toBeVisible();
}
