import { expect, test, type Page } from "@playwright/test";
import { emptyPad, openApp, pad, row } from "./support";

test.beforeEach(async ({ page }) => openApp(page));

test("MyInstants shows the trending sounds first", async ({ page }) => {
  await expect(page.getByRole("status").filter({ hasText: "Trending in United States" })).toBeVisible();
  await expect(row(page, "Vine boom")).toBeVisible();
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
  await expect(page.getByRole("status").filter({ hasText: "Trending in United States" })).toBeVisible();
});

test("a search shows skeletons in place of the old list until it answers", async ({ page }) => {
  await page.getByRole("textbox", { name: "Search" }).fill("vine");
  await page.getByRole("button", { name: "Search" }).click();

  await expect(page.locator(".sound-skeleton").first()).toBeVisible();
  await expect(page.locator(".sound-row:visible")).toHaveCount(0);

  await expect(page.locator(".sound-row:visible")).toHaveCount(1);
  await expect(page.locator(".sound-skeleton")).toHaveCount(0);
});

async function scrollToTheEnd(page: Page) {
  const list = page.locator(".sound-list:visible");
  await list.evaluate((element) => element.scrollTo({ top: element.scrollHeight }));
}

test("a list too short to scroll keeps loading until there is no more", async ({ page }) => {
  await expect(page.getByRole("status").filter({ hasText: "(24)" })).toBeVisible();
  await expect(page.locator(".sound-row:visible")).toHaveCount(24);
});

/** Sends a command that only the mock knows, for what a test cannot do through the interface. */
async function mock(page: Page, command: string) {
  await page.evaluate(
    (command) =>
      (
        window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string, args: unknown) => unknown } }
      ).__TAURI_INTERNALS__.invoke(command, {}),
    command,
  );
}

const rows = (page: Page) => page.locator(".sound-row:visible");

/** How many rows the list holds once it stopped loading. */
async function settledRows(page: Page) {
  await expect(page.locator(".sound-skeleton")).toHaveCount(0);
  await page.waitForTimeout(300);
  await expect(page.locator(".sound-skeleton")).toHaveCount(0);
  return rows(page).count();
}

test.describe("with more trending sounds than the list shows", () => {
  test.beforeEach(async ({ page }) => {
    await mock(page, "mock_long_catalog");
    await page.getByRole("button", { name: "Trending", exact: true }).click();
  });

  test("the list fills the panel and stops, then scrolls inside it", async ({ page }) => {
    const shown = await settledRows(page);
    expect(shown).toBeGreaterThan(8);
    expect(shown).toBeLessThan(100);

    const list = page.locator(".sound-list:visible");
    const box = (await list.boundingBox())!;
    expect(box.y + box.height).toBeLessThanOrEqual(page.viewportSize()!.height);
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.wheel(0, 100_000);
    await expect(rows(page).last()).toBeInViewport();
  });

  test("scrolling to the end loads the next page, until there is no more", async ({ page }) => {
    let shown = await settledRows(page);

    while (shown < 100) {
      await scrollToTheEnd(page);
      await expect.poll(() => rows(page).count()).toBeGreaterThan(shown);
      shown = await settledRows(page);
    }

    await expect(page.getByRole("status").filter({ hasText: "(100)" })).toBeVisible();
    await expect(page.locator(".sound-list-end")).toHaveCount(0);
    await scrollToTheEnd(page);
    expect(await settledRows(page)).toBe(100);
  });

  test("a next page that fails waits for Try again instead of asking again and again", async ({ page }) => {
    const shown = await settledRows(page);
    await mock(page, "mock_fail_next_page");

    await scrollToTheEnd(page);

    const again = page.getByRole("button", { name: "Try again" });
    await expect(again).toBeVisible();
    await scrollToTheEnd(page);
    expect(await settledRows(page)).toBe(shown);

    await again.click();
    await expect.poll(() => rows(page).count()).toBeGreaterThan(shown);
    await expect(again).toHaveCount(0);
  });
});

test("the star follows the favorites", async ({ page }) => {
  const star = row(page, "Vine boom").getByRole("button", { name: "☆" });
  await star.click();
  await expect(row(page, "Vine boom").getByRole("button", { name: "★" })).toBeVisible();

  await page.getByRole("tab", { name: /Favorites/ }).click();
  await expect(page.getByRole("status").filter({ hasText: "2 favorites" })).toBeVisible();
});

test("sounds from the computer go to the chosen pad and the next free one", async ({ page }) => {
  await emptyPad(page, "04").click();

  await page.getByRole("button", { name: "From computer..." }).click();

  await expect(pad(page, "04 Applause")).toBeVisible();
  await expect(page.locator(".pad", { hasText: "Drum roll" })).toHaveCount(1);
});

test("a sound row has no button to save it to the computer", async ({ page }) => {
  await expect(row(page, "Vine boom").getByRole("button", { name: "↓" })).toHaveCount(0);
});

test("one sound at a time starts on and switches off in the settings", async ({ page }) => {
  await page.getByRole("tab", { name: "Settings" }).click();
  const toggle = page.getByRole("switch", { name: "One sound at a time" });
  await expect(toggle).toHaveAttribute("aria-checked", "true");

  await toggle.click();

  await expect(toggle).toHaveAttribute("aria-checked", "false");
});

test("press again to stop starts off and switches on in the settings", async ({ page }) => {
  await page.getByRole("tab", { name: "Settings" }).click();
  const toggle = page.getByRole("switch", { name: "Press again to stop" });
  await expect(toggle).toHaveAttribute("aria-checked", "false");

  await toggle.click();

  await expect(toggle).toHaveAttribute("aria-checked", "true");
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

test("the trending sounds follow the country chosen in the settings", async ({ page }) => {
  await page.getByRole("tab", { name: "Settings" }).click();
  await page.getByLabel("Trending sounds from").selectOption("br");
  await page.getByRole("tab", { name: "MyInstants" }).click();

  await expect(page.getByRole("status").filter({ hasText: "Trending in Brazil" })).toBeVisible();

  await page.getByRole("tab", { name: "Settings" }).click();
  await page.getByLabel("Trending sounds from").selectOption("");
  await page.getByRole("tab", { name: "MyInstants" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Trending in United States" })).toBeVisible();
});
