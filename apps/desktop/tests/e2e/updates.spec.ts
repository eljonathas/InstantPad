import { expect, test, type Page } from "@playwright/test";
import { openApp, recordCommands, soundAction } from "./support";

test.beforeEach(async ({ page }) => {
  await openApp(page);
});

const dialog = (page: Page) => page.getByRole("dialog", { name: "Update ready" });

/** Tells the mock that a version finished downloading, as the backend would. */
async function updateArrives(page: Page, version: string) {
  await page.evaluate(
    (version) =>
      (
        window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string, args: unknown) => unknown } }
      ).__TAURI_INTERNALS__.invoke("mock_update_ready", { version }),
    version,
  );
}

test("a version that is ready opens a window, which Later closes until the next version", async ({ page }) => {
  await expect(dialog(page)).toHaveCount(0);

  await updateArrives(page, "9.9.9");

  await expect(dialog(page)).toBeVisible();
  await expect(dialog(page).getByText("Version 9.9.9 is installed", { exact: false })).toBeVisible();
  await expect(dialog(page).getByRole("button", { name: "Restart now" })).toBeVisible();

  await dialog(page).getByRole("button", { name: "Later" }).click();
  await expect(dialog(page)).toHaveCount(0);

  // The same version again stays quiet; a newer one shows.
  await updateArrives(page, "9.9.9");
  await expect(dialog(page)).toHaveCount(0);
  await updateArrives(page, "9.9.10");
  await expect(dialog(page)).toBeVisible();
});

test("Restart now in the window restarts", async ({ page }) => {
  const calls = await recordCommands(page);
  await updateArrives(page, "9.9.9");

  await dialog(page).getByRole("button", { name: "Restart now" }).click();

  await expect.poll(async () => (await calls()).filter((call) => call === "restart_now").length).toBe(1);
});

test("the checkbox silences the window, and the settings keep the restart and the switch", async ({ page }) => {
  await updateArrives(page, "9.9.9");
  await dialog(page).getByRole("checkbox").check();
  await dialog(page).getByRole("button", { name: "Later" }).click();
  await expect(dialog(page)).toHaveCount(0);

  await updateArrives(page, "9.9.10");
  await expect(dialog(page)).toHaveCount(0);

  await page.getByRole("tab", { name: "Settings" }).click();
  await expect(page.getByText("Version 9.9.10 is installed", { exact: false })).toBeVisible();
  await expect(page.getByRole("button", { name: "Restart now" })).toBeVisible();
  const window = page.getByRole("switch", { name: "Window when an update is ready" });
  await expect(window).toHaveAttribute("aria-checked", "false");

  await window.click();
  await expect(window).toHaveAttribute("aria-checked", "true");
  await updateArrives(page, "9.9.11");
  await expect(dialog(page)).toBeVisible();
});

test("Esc closes the window without silencing it", async ({ page }) => {
  await updateArrives(page, "9.9.9");
  await expect(dialog(page)).toBeVisible();

  await page.keyboard.press("Escape");

  await expect(dialog(page)).toHaveCount(0);
  await updateArrives(page, "9.9.10");
  await expect(dialog(page)).toBeVisible();
});

test("the window opens by itself, so it leaves the global keys on", async ({ page }) => {
  const calls = await recordCommands(page);

  await updateArrives(page, "9.9.9");

  await expect(dialog(page)).toBeVisible();
  expect((await calls()).filter((call) => call === "pause_hotkeys")).toHaveLength(0);
});

test("Esc closes only the window on top", async ({ page }) => {
  await soundAction(page, "Airhorn", "Cut into pads...");
  const cutEditor = page.getByRole("dialog", { name: "Cut into pads: Airhorn" });
  await expect(cutEditor).toBeVisible();
  await updateArrives(page, "9.9.9");

  await page.keyboard.press("Escape");

  await expect(dialog(page)).toHaveCount(0);
  await expect(cutEditor).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(cutEditor).toHaveCount(0);
});
