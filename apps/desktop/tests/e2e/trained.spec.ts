import { expect, test, type Page } from "@playwright/test";
import { lcd, openApp, readyTheAi } from "./support";

test.beforeEach(async ({ page }) => {
  await page.clock.install();
  await openApp(page);
  await page.getByRole("tab", { name: "Voice" }).click();
  await readyTheAi(page);
});

const engines = (page: Page) => page.getByRole("group", { name: "Engine" });
/** What the panel says about the model, apart from the display, which says it too. */
const modelNote = (page: Page) => page.locator(".trained").getByText('Model "feminine"', { exact: false });

/** Adds the model the training kit exported for the voice in use, which takes a moment to check. */
async function addModel(page: Page) {
  await page.getByRole("button", { name: "Add a trained model..." }).click();
  await expect(page.getByRole("button", { name: "Checking the model..." })).toBeDisabled();
  await page.clock.runFor(400);
}

test("an AI voice offers to add a trained model, and says where one comes from", async ({ page }) => {
  await expect(page.getByText("trained by the InstantPad team", { exact: false })).toBeVisible();
  await expect(engines(page)).toHaveCount(0);
  await expect(page.getByText("about a third of a second of delay")).toBeVisible();
});

test("a trained model added to the voice turns its engine to Trained", async ({ page }) => {
  await addModel(page);

  await expect(lcd(page)).toHaveText('Trained model "feminine" added.');
  await expect(engines(page).getByRole("button", { name: "Trained" })).toHaveAttribute("aria-pressed", "true");
  await expect(engines(page).getByRole("button", { name: "Instant" })).toHaveAttribute("aria-pressed", "false");
  await expect(modelNote(page)).toBeVisible();
  await expect(page.getByText("about a third of a second of delay")).toHaveCount(0);
  await expect(page.getByRole("slider", { name: "Pitch" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Replace model..." })).toBeVisible();
  // A trained model needs no knowledge of the user's voice.
  await expect(page.getByRole("button", { name: "Learn my voice again" })).toHaveCount(0);
});

test("the engine switches back and forth between Instant and Trained", async ({ page }) => {
  await addModel(page);

  await engines(page).getByRole("button", { name: "Instant" }).click();

  await expect(engines(page).getByRole("button", { name: "Instant" })).toHaveAttribute("aria-pressed", "true");
  await expect(modelNote(page)).toHaveCount(0);
  await expect(page.getByText("about a third of a second of delay")).toBeVisible();
  await expect(page.getByRole("button", { name: "Feminine AI", exact: true })).toHaveAttribute("aria-pressed", "true");

  await engines(page).getByRole("button", { name: "Trained" }).click();

  await expect(engines(page).getByRole("button", { name: "Trained" })).toHaveAttribute("aria-pressed", "true");
});

test("removing the model asks first, then the voice goes back to the instant engine", async ({ page }) => {
  await addModel(page);

  await page.getByRole("button", { name: "Remove model" }).click();

  await expect(lcd(page)).toHaveText("Trained model removed.");
  await expect(engines(page)).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Add a trained model..." })).toBeVisible();
});

test("a folder the kit did not export is refused with the reason", async ({ page }) => {
  await page.evaluate(() =>
    (
      window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string, args: unknown) => unknown } }
    ).__TAURI_INTERNALS__.invoke("mock_pick_folder", { folder: "/home/broken" }),
  );

  await addModel(page);

  await expect(lcd(page)).toHaveText("Error");
  await expect(page.getByText("no voice exported by the training kit", { exact: false })).toBeVisible();
  await expect(engines(page)).toHaveCount(0);
});

test("an effect without the AI has no trained model", async ({ page }) => {
  await page.getByRole("button", { name: "Ogre" }).dispatchEvent("pointerdown");

  await expect(page.getByRole("button", { name: "Add a trained model..." })).toHaveCount(0);
});
