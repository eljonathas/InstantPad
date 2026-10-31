import { expect, test } from "@playwright/test";
import { drag, emptyPad, interfaceTabs, lcd, openApp, pad, row } from "./support";

test.beforeEach(async ({ page }) => openApp(page));

test("a pad fires on press and lights up", async ({ page }) => {
  await pad(page, "01 Vine boom").dispatchEvent("pointerdown", { button: 0 });

  await expect(lcd(page)).toHaveText("01 Vine boom");
  await expect(pad(page, "01 Vine boom")).toHaveAttribute("data-playing", "");
  // The meters are painted without React, from the same frames.
  await expect(page.getByRole("meter", { name: "Output" }).locator("[data-lit]").first()).toBeVisible();
});

test("a press on an empty pad targets it and leaves the settings", async ({ page }) => {
  await page.getByRole("tab", { name: "Settings" }).click();
  await emptyPad(page, "04").click();

  await expect(lcd(page)).toHaveText("Pad 04 selected: choose a sound");
  await expect(emptyPad(page, "04")).toHaveAttribute("data-target", "");
  await expect(page.getByRole("tab", { name: "MyInstants" })).toHaveAttribute("aria-selected", "true");

  await row(page, "Airhorn").getByRole("button", { name: "+ Pad" }).click();
  await expect(pad(page, "04 Airhorn")).toBeVisible();
});

test("interface tabs switch the grid and take the accent", async ({ page }) => {
  await page.getByRole("tab", { name: "Memes" }).click();

  await expect(lcd(page)).toHaveText("Memes");
  await expect(page.locator(".pad")).toHaveCount(8);
  await expect(page.locator("html")).toHaveCSS("--color-accent", "#35f27a");

  await page.getByTitle("Previous interface").click();
  await expect(page.locator(".pad")).toHaveCount(24);
});

test("the menu of a pad with a sound", async ({ page }) => {
  await pad(page, "01 Vine boom").click({ button: "right" });
  await expect(page.getByRole("menuitem")).toHaveText([
    "Add to favorites",
    "Download to computer...",
    "Set shortcut...",
    "Rename...",
    "Volume",
    "Color",
    "Send to interface",
    "Remove from pad",
  ]);

  await page.getByRole("menuitem", { name: "Volume" }).hover();
  await page.getByRole("menuitemradio", { name: "150%" }).click();
  await pad(page, "01 Vine boom").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Volume" }).hover();
  await expect(page.getByRole("menuitemradio", { name: "150%" })).toHaveAttribute("aria-checked", "true");
  await page.keyboard.press("Escape");

  await pad(page, "01 Vine boom").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Add to favorites" }).click();
  await expect(pad(page, "01 Vine boom").locator(".pad-star")).toBeVisible();

  await pad(page, "01 Vine boom").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Send to interface" }).hover();
  await page.getByRole("menuitem", { name: "Memes" }).click();
  await expect(lcd(page)).toHaveText("Sent to Memes");
  await expect(emptyPad(page, "01")).toBeVisible();
});

test("the menu of an empty pad only loads a file", async ({ page }) => {
  await emptyPad(page, "04").click({ button: "right" });
  await expect(page.getByRole("menuitem")).toHaveText(["Load audio file..."]);

  await page.getByRole("menuitem").click();
  await expect(pad(page, "04 Applause")).toBeVisible();
});

test("a menu opens toward the free side at the window edge", async ({ page }) => {
  const last = emptyPad(page, "24");
  const box = (await last.boundingBox())!;
  await last.click({ button: "right", position: { x: box.width - 4, y: box.height - 4 } });

  const menu = page.getByRole("menu");
  const { x, y, width, height } = (await menu.boundingBox())!;
  const viewport = page.viewportSize()!;
  expect(x + width).toBeLessThanOrEqual(viewport.width);
  expect(y + height).toBeLessThanOrEqual(viewport.height);
});

test("the menu of an interface moves and deletes it", async ({ page }) => {
  await page.getByRole("tab", { name: "Memes" }).click({ button: "right" });
  await expect(page.getByRole("menuitem", { name: "Move right" })).toBeDisabled();
  await page.getByRole("menuitem", { name: "Move left" }).click();
  await expect(interfaceTabs(page).first()).toHaveText("Memes");

  // Main has sounds, so the question is asked; the mock answers yes.
  await page.getByRole("tab", { name: "Main" }).click({ button: "right" });
  await page.getByRole("menuitem", { name: "Delete interface" }).click();
  await expect(interfaceTabs(page)).toHaveText(["Memes"]);
  await expect(page.getByRole("menuitem")).toHaveCount(0);
});

test("Ctrl+drag swaps two pads, a plain press never drags", async ({ page }) => {
  await page.keyboard.down("Control");
  await drag(page, pad(page, "01 Vine boom"), emptyPad(page, "05"));
  await page.keyboard.up("Control");

  await expect(pad(page, "05 Vine boom")).toBeVisible();
  await expect(emptyPad(page, "01")).toBeVisible();
  // Ctrl+press does not fire the sound.
  await expect(lcd(page)).toHaveText("Ready");
});

test("a row dragged onto a pad puts its sound there", async ({ page }) => {
  await drag(page, row(page, "Airhorn").locator(".sound-name"), emptyPad(page, "06"));

  await expect(pad(page, "06 Airhorn")).toBeVisible();
  await expect(page.locator(".drag-ghost")).toHaveCount(0);
});

test("the pad under a dragged row is marked until the row lands on it", async ({ page }) => {
  const start = (await row(page, "Airhorn").locator(".sound-name").boundingBox())!;
  const target = (await emptyPad(page, "06").boundingBox())!;
  await page.mouse.move(start.x + 4, start.y + 4);
  await page.mouse.down();
  await page.mouse.move(target.x + target.width / 2, target.y + target.height / 2, { steps: 8 });

  await expect(emptyPad(page, "06")).toHaveAttribute("data-drop", "");
  await expect(page.locator(".sound-row[data-drag-source]")).toHaveCount(1);

  await page.mouse.up();

  await expect(page.locator("[data-drop]")).toHaveCount(0);
  await expect(page.locator("[data-drag-source]")).toHaveCount(0);
  await expect(page.locator("[data-landed]")).toHaveCount(0);
});

test("the right-click menu of a pad saves a copy to the computer", async ({ page }) => {
  await pad(page, "01 Vine boom").click({ button: "right" });
  await page.getByText("Download to computer...").click();

  await expect(lcd(page)).toHaveText("Saved: sound.mp3");
});

test("a small movement on a row drags nothing", async ({ page }) => {
  const name = row(page, "Airhorn").locator(".sound-name");
  const box = (await name.boundingBox())!;
  await page.mouse.move(box.x + 4, box.y + 4);
  await page.mouse.down();
  await page.mouse.move(box.x + 6, box.y + 5);
  await expect(page.locator(".drag-ghost")).toHaveCount(0);
  await page.mouse.up();
});
