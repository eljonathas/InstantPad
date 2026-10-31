import { expect, test } from "@playwright/test";
import { openApp } from "./support";

test.beforeEach(async ({ page }) => openApp(page));

const value = async (knob: import("@playwright/test").Locator) => Number(await knob.getAttribute("aria-valuenow"));

test("full screen fills the window and comes back", async ({ page }) => {
  const key = page.getByTitle("Full screen (F11)");
  const window = page.viewportSize()!;

  await key.click();
  await expect(page.getByTitle("Leave full screen (F11)")).toBeVisible();
  await expect(page.locator(".grip")).toHaveCount(0);
  const device = (await page.locator(".device").boundingBox())!;
  expect(device.width).toBeCloseTo(window.width, 0);
  expect(device.height).toBeCloseTo(window.height, 0);

  await page.keyboard.press("F11");
  await expect(key).toBeVisible();
  await expect(page.locator(".grip")).toBeVisible();
  expect((await page.locator(".device").boundingBox())!.width).toBeLessThan(window.width);
});

test("a knob follows a vertical drag, even outside the knob", async ({ page }) => {
  const mic = page.getByRole("slider", { name: "Mic" });
  expect(await value(mic)).toBe(100);

  const box = (await mic.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  // The device is scaled to the window, so 90 screen pixels are a little more in knob pixels.
  await page.mouse.move(box.x + 300, box.y + box.height / 2 + 90, { steps: 6 });
  await page.mouse.up();

  expect(await value(mic)).toBeGreaterThan(44);
  expect(await value(mic)).toBeLessThan(54);
});

test("the wheel moves a knob by 1/25 and a double click resets it", async ({ page }) => {
  const game = page.getByRole("slider", { name: "Game" });
  expect(await value(game)).toBe(80);

  await game.hover();
  await page.mouse.wheel(0, -100);
  await expect.poll(() => value(game)).toBe(84);
  await page.mouse.wheel(0, 100);
  await page.mouse.wheel(0, 100);
  await expect.poll(() => value(game)).toBe(76);

  await game.dblclick();
  await expect.poll(() => value(game)).toBe(80);
  await expect(page.getByRole("slider", { name: "Headphones" })).toHaveAttribute("aria-valuenow", "60");
});

test("the mic switch mutes and opens the mic", async ({ page }) => {
  const toggle = page.getByRole("button", { name: "Mic open" });
  await toggle.click();

  await expect(page.getByRole("button", { name: "Mic muted" })).toHaveAttribute("aria-pressed", "false");
  await expect(page.getByRole("meter", { name: "Mic" }).locator("[data-lit]")).toHaveCount(0);
});

test("the mic meter paints its segments from the frames", async ({ page }) => {
  const lit = page.getByRole("meter", { name: "Mic" }).locator("[data-lit]");
  await expect.poll(() => lit.count()).toBeGreaterThan(3);
  await expect(page.getByRole("meter", { name: "Mic" }).locator(".led")).toHaveCount(16);
});

test("the device is scaled to the window without cropping", async ({ page }) => {
  await page.setViewportSize({ width: 880, height: 533 });

  await expect
    .poll(async () => {
      const box = (await page.locator(".chassis").boundingBox())!;
      return box.x >= 0 && box.y >= 0 && box.x + box.width <= 880 && box.y + box.height <= 533;
    })
    .toBe(true);
  await expect(page.getByTitle("Minimize")).toBeVisible();
});

test("a held knob shows its exact value and glows, and both stop on release", async ({ page }) => {
  const knob = page.getByRole("slider", { name: "Game" });
  const box = (await knob.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();

  await expect(page.locator(".knob-readout")).toHaveText("80%");
  await expect(knob).toHaveAttribute("data-held", "");

  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2 - 18);
  await expect(page.locator(".knob-readout")).toHaveText("90%");

  await page.mouse.up();
  await expect(page.locator(".knob-readout")).toHaveCount(0);
  await expect(knob).not.toHaveAttribute("data-held", "");
});
