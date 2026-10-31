import { expect, test } from "@playwright/test";
import { openApp, pad } from "./support";

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await page.getByRole("tab", { name: "Voice" }).click();
});

test("the logo rolls to Voice and the pads give way to the effects", async ({ page }) => {
  await expect(page.locator(".logo-pad .rolling-term").last()).toBeVisible();
  await expect(page.locator(".logo-pad")).toContainText("Voice");
  await expect(page.locator(".logo-pad .rolling-leaving")).toHaveCount(0);

  await expect(page.getByRole("button", { name: "Pitch cycle" })).toBeVisible();
  await expect(page.locator(".pad")).toHaveCount(0);
});

test("the natural voice is on at first and has no knobs", async ({ page }) => {
  await expect(page.getByRole("button", { name: "Natural" })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByText("Your voice with no effects.")).toBeVisible();
  await expect(page.getByRole("slider", { name: "Intensity" })).toHaveCount(0);
});

test("picking an effect shows its knobs, and only one stays on", async ({ page }) => {
  await page.getByRole("button", { name: "Pitch cycle" }).dispatchEvent("pointerdown");

  await expect(page.getByRole("button", { name: "Pitch cycle" })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("button", { name: "Natural" })).toHaveAttribute("aria-pressed", "false");
  for (const name of ["Time", "High", "Low"]) {
    await expect(page.getByRole("slider", { name })).toBeVisible();
  }
});

test("a knob shows the value it set", async ({ page }) => {
  await page.getByRole("button", { name: "Deep voice" }).dispatchEvent("pointerdown");
  const knob = page.getByRole("slider", { name: "Intensity" });
  await knob.focus();

  await page.keyboard.press("ArrowUp");

  await expect(page.locator(".voice-value").first()).not.toHaveText("50%");
});

test("the effect keeps working on the Pad screen, and its LED says so", async ({ page }) => {
  await page.getByRole("button", { name: "Thin voice" }).dispatchEvent("pointerdown");

  await page.getByRole("tab", { name: "Pad" }).click();

  await expect(pad(page, "01 Vine boom")).toBeVisible();
  await expect(page.locator(".mode-voice-on")).toHaveCount(1);
});

test("MyInstants keeps its results across the two screens", async ({ page }) => {
  await page.getByRole("tab", { name: "Pad" }).click();

  await expect(page.locator(".sound-row:visible")).toHaveCount(8);
});

test("hearing the voice switches on and shows the keys for the game", async ({ page }) => {
  const toggle = page.getByRole("button", { name: "Hear my voice" });

  await toggle.click();

  await expect(toggle).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByText("Num* switches the effect on and off")).toBeVisible();
});
