import { expect, test } from "@playwright/test";
import { interfaceTabs, openApp, pad, recordCommands } from "./support";

test.beforeEach(async ({ page }) => openApp(page));

test("the interface dialog refuses a grid too small for the sounds", async ({ page }) => {
  await page.getByTitle("Configure this interface").click();
  const dialog = page.getByRole("dialog", { name: "Configure interface" });
  await expect(dialog.getByText("24 pads")).toBeVisible();

  await dialog.getByLabel("Rows").selectOption("1");
  await dialog.getByLabel("Columns").selectOption("2");
  await expect(dialog.getByText("2 pads cannot hold the 3 sounds in this interface.")).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Save" })).toBeDisabled();

  await dialog.getByLabel("Columns").selectOption("3");
  await dialog.getByRole("button", { name: "Save" }).click();
  await expect(page.locator(".pad")).toHaveCount(3);
});

test("a new interface needs a name and takes the chosen color", async ({ page }) => {
  await page.getByTitle("New interface").click();
  const dialog = page.getByRole("dialog", { name: "New interface" });
  await expect(dialog.getByLabel("Name")).toHaveValue("Interface 3");

  await dialog.getByLabel("Name").fill("   ");
  await expect(dialog.getByRole("button", { name: "Save" })).toBeDisabled();

  await dialog.getByLabel("Name").fill("Boss");
  await dialog.getByRole("button", { name: "#b45cff" }).click();
  await dialog.getByRole("button", { name: "Save" }).click();

  await expect(page.getByRole("tab", { name: "Boss" })).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("html")).toHaveCSS("--color-accent", "#b45cff");
});

test("Esc cancels the interface dialog", async ({ page }) => {
  await page.getByTitle("New interface").click();
  await page.keyboard.press("Escape");

  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(interfaceTabs(page)).toHaveCount(2);
});

test("the text dialog renames on Enter and cancels on Esc", async ({ page }) => {
  await pad(page, "01 Vine boom").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Rename..." }).click();
  await page.getByRole("dialog").getByRole("textbox").fill("Boom");
  await page.keyboard.press("Escape");
  await expect(pad(page, "01 Vine boom")).toBeVisible();

  await pad(page, "01 Vine boom").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Rename..." }).click();
  await expect(page.getByRole("dialog").getByRole("textbox")).toBeFocused();
  await page.keyboard.type("Boom");
  await page.keyboard.press("Enter");

  await expect(pad(page, "01 Boom")).toBeVisible();
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

test("the hotkey dialog waits past modifiers and takes the code", async ({ page }) => {
  await pad(page, "01 Vine boom").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Set shortcut..." }).click();
  const dialog = page.getByRole("dialog", { name: "Set shortcut" });

  await page.keyboard.press("Shift");
  await expect(dialog).toBeVisible();

  await page.keyboard.press("Control+KeyK");
  await expect(dialog).toHaveCount(0);
  await expect(pad(page, "01 Vine boom").locator(".pad-shortcut")).toHaveText("Ctrl+K");

  // With a shortcut set, the menu can take it away.
  await pad(page, "01 Vine boom").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Remove shortcut" }).click();
  await expect(pad(page, "01 Vine boom").locator(".pad-shortcut")).toHaveText("Num1");
});

test("Esc cancels the hotkey dialog", async ({ page }) => {
  await page.getByRole("tab", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Num," }).click();
  await page.keyboard.press("Escape");

  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Num," })).toBeVisible();
});

test("the global hotkeys stay paused while a dialog or the search box has focus", async ({ page }) => {
  const calls = await recordCommands(page);
  // StrictMode mounts a dialog twice in development, so only the last command says where it ended.
  const lastHotkeyCommand = async () => (await calls()).filter((command) => command.endsWith("_hotkeys")).at(-1);

  await page.getByTitle("New interface").click();
  await expect.poll(lastHotkeyCommand).toBe("pause_hotkeys");
  await page.keyboard.press("Escape");
  await expect.poll(lastHotkeyCommand).toBe("resume_hotkeys");

  await page.getByRole("textbox", { name: "Search" }).focus();
  await expect.poll(lastHotkeyCommand).toBe("pause_hotkeys");
  await page.keyboard.press("Enter");
  await expect.poll(lastHotkeyCommand).toBe("resume_hotkeys");

  await page.getByRole("textbox", { name: "Search" }).focus();
  await page.getByRole("button", { name: "Search" }).click();
  await expect.poll(lastHotkeyCommand).toBe("resume_hotkeys");
});
