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
