import { expect, test } from "@playwright/test";
import { openApp, row } from "./support";

test.beforeEach(async ({ page }) => openApp(page));

test("MyInstants shows the trending sounds first", async ({ page }) => {
  await expect(page.getByRole("status").filter({ hasText: "Trending in Brazil (8)" })).toBeVisible();
  await expect(page.locator(".sound-row:visible")).toHaveCount(8);
});

test("search on Enter and on the button, then trending again", async ({ page }) => {
  const box = page.getByRole("textbox", { name: "Search" });
  await box.fill("air");
  await box.press("Enter");
  await expect(page.getByRole("status").filter({ hasText: "air (1)" })).toBeVisible();
  await expect(box).not.toBeFocused();

  await box.fill("zzz");
  await page.getByRole("button", { name: "Search" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Nothing found" })).toBeVisible();

  await page.getByRole("button", { name: "Trending", exact: true }).click();
  await expect(page.locator(".sound-row:visible")).toHaveCount(8);
});

test("a search shows skeletons in place of the old list until it answers", async ({ page }) => {
  await page.getByRole("textbox", { name: "Search" }).fill("vine");
  await page.getByRole("button", { name: "Search" }).click();

  await expect(page.locator(".sound-skeleton").first()).toBeVisible();
  await expect(page.locator(".sound-row:visible")).toHaveCount(0);

  await expect(page.locator(".sound-row:visible")).toHaveCount(1);
  await expect(page.locator(".sound-skeleton")).toHaveCount(0);
});

test("Load more appends the next page", async ({ page }) => {
  await page.getByRole("button", { name: "Load more" }).click();
  await expect(page.locator(".sound-row:visible")).toHaveCount(16);
  await page.getByRole("button", { name: "Load more" }).click();
  await expect(page.locator(".sound-row:visible")).toHaveCount(24);
  await expect(page.getByRole("button", { name: "Load more" })).toHaveCount(0);
});

test("the star follows the favorites", async ({ page }) => {
  const star = row(page, "Vine boom").getByRole("button", { name: "☆" });
  await star.click();
  await expect(row(page, "Vine boom").getByRole("button", { name: "★" })).toBeVisible();

  await page.getByRole("tab", { name: /Favorites/ }).click();
  await expect(page.getByRole("status").filter({ hasText: "2 favorites" })).toBeVisible();
});

test("a sound row has no button to save it to the computer", async ({ page }) => {
  await expect(row(page, "Vine boom").getByRole("button", { name: "↓" })).toHaveCount(0);
});

test("one sound at a time switches on in the settings", async ({ page }) => {
  await page.getByRole("tab", { name: "Settings" }).click();
  const toggle = page.getByRole("button", { name: "One sound at a time" });
  await expect(toggle).toHaveAttribute("aria-pressed", "false");

  await toggle.click();

  await expect(toggle).toHaveAttribute("aria-pressed", "true");
});

test("settings show the routing and follow the numeric scheme", async ({ page }) => {
  await page.getByRole("tab", { name: "Settings" }).click();
  await expect(page.getByText("Driver found.")).toBeVisible();
  await expect(page.getByLabel("1. Your microphone")).toHaveValue("mic");
  await expect(page.getByText("In the game, choose the microphone: CABLE Output")).toBeVisible();
  await expect(page.getByText(/^Num1 to Num0 play/)).toContainText(
    "Num+ and Num- switch interfaces. Num, stops everything.",
  );

  await page.getByLabel("Numeric shortcuts").selectOption("ctrlNumpad");
  await expect(page.getByText(/^Ctrl\+Num1 to Ctrl\+Num0 play/)).toBeVisible();
  await expect(page.getByRole("button", { name: "01 Vine boom" }).locator(".pad-shortcut")).toHaveText("Ctrl+Num1");

  await page.getByLabel("2. Output to the game").selectOption("cable");
  await page.getByRole("button", { name: "Refresh devices" }).click();
  await expect(page.locator(".lcd-line").first()).toHaveText("Mic ok   Output ok   Driver ok");
});

test("the language switches every text without a restart", async ({ page }) => {
  await page.getByRole("tab", { name: "Settings" }).click();
  await page.getByLabel("Language").selectOption("pt");

  await expect(page.getByRole("button", { name: /Parar/ })).toBeVisible();
  await expect(page.getByLabel("Idioma")).toHaveValue("pt");
  await expect(page.locator(".lcd-main")).toHaveText("Nenhum áudio tocando");
  await page.getByLabel("Idioma").selectOption("en");
  await expect(page.locator(".lcd-main")).toHaveText("No audio playing");
});
