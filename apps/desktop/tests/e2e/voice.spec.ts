import { expect, test } from "@playwright/test";
import { lcd, openApp, pad, readyTheAi } from "./support";

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

test("the new effects show, and only the AI ones carry the AI mark", async ({ page }) => {
  for (const name of ["Feminine", "Ghost", "Ogre", "Feminine AI", "Clone a voice"]) {
    await expect(page.getByRole("button", { name, exact: true })).toBeVisible();
  }

  await expect(page.locator(".effect-ai")).toHaveCount(1);
  await expect(page.getByRole("button", { name: "Feminine AI" }).locator(".effect-ai")).toHaveText("AI");
});

test("an effect with three knobs shows them all", async ({ page }) => {
  await page.getByRole("button", { name: "Ghost" }).dispatchEvent("pointerdown");

  for (const name of ["Whisper", "Echo", "Pitch"]) {
    await expect(page.getByRole("slider", { name })).toBeVisible();
  }
});

test("an AI voice asks for the download, then for the user's own voice, then shows its knob", async ({ page }) => {
  const feminineAi = page.getByRole("button", { name: "Feminine AI", exact: true });
  await feminineAi.dispatchEvent("pointerdown");

  // The backend refuses it, and the panel says what is missing.
  await expect(lcd(page)).toHaveText("Error");
  await expect(page.getByText("AI voices are not installed yet", { exact: false })).toBeVisible();
  await expect(page.locator(".voice-title")).toHaveText("Feminine AI");
  await expect(page.getByText("one-time download of 255 MB", { exact: false })).toBeVisible();
  await expect(page.getByRole("slider", { name: "Pitch" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Natural" })).toHaveAttribute("aria-pressed", "true");

  await page.getByRole("button", { name: "Download" }).click();
  await expect(page.getByText(/Downloading AI voices: \d+%/)).toBeVisible();
  await expect(page.getByText("read any text aloud", { exact: false })).toBeVisible();
  await expect(page.getByText("at least 8 seconds", { exact: false })).toBeVisible();

  await page.getByRole("button", { name: "Record" }).click();
  await expect(page.locator(".record-time")).toContainText("s");
  await page.getByRole("button", { name: "Stop" }).click();

  await expect(feminineAi).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("slider", { name: "Pitch" })).toBeVisible();
  await expect(page.getByText("about a quarter of a second of delay")).toBeVisible();
});

test("choosing another effect leaves the AI setup", async ({ page }) => {
  await page.getByRole("button", { name: "Feminine AI", exact: true }).dispatchEvent("pointerdown");
  await expect(page.getByRole("button", { name: "Download" })).toBeVisible();

  await page.getByRole("button", { name: "Thin voice" }).dispatchEvent("pointerdown");

  await expect(page.getByRole("button", { name: "Download" })).toHaveCount(0);
  await expect(page.getByRole("slider", { name: "Intensity" })).toBeVisible();
});

test("with the AI ready, an AI voice goes on at the first press", async ({ page }) => {
  await readyTheAi(page);
  await page.getByRole("button", { name: "Natural" }).dispatchEvent("pointerdown");

  await page.getByRole("button", { name: "Feminine AI", exact: true }).dispatchEvent("pointerdown");

  await expect(page.getByRole("button", { name: "Feminine AI", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("slider", { name: "Pitch" })).toBeVisible();
});
