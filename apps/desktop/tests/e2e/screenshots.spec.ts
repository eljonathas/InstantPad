import { expect, test } from "@playwright/test";
import { openApp } from "./support";

// The clock is frozen so the meters read the same every run and the pictures only change when
// the interface does.
test("pictures of the three panels", async ({ page }) => {
  await page.clock.install({ time: new Date("2026-01-01T00:00:00Z") });
  await openApp(page);
  await page.clock.pauseAt(new Date("2026-01-01T00:00:01Z"));
  await page.clock.runFor(200);
  await expect(page.locator(".sound-row:visible")).toHaveCount(8);

  await page.screenshot({ path: "tests/e2e/screenshots/myinstants.png" });

  await page.getByRole("tab", { name: /Favorites/ }).click();
  await page.screenshot({ path: "tests/e2e/screenshots/favorites.png" });

  await page.getByRole("tab", { name: "Settings" }).click();
  await page.screenshot({ path: "tests/e2e/screenshots/settings.png" });
});
