import { expect, test, type Page } from "@playwright/test";
import { lcd, openApp, readyTheAi, recordFor } from "./support";

test.beforeEach(async ({ page }) => {
  await page.clock.install();
  await openApp(page);
  await page.getByRole("tab", { name: "Voice" }).click();
  await readyTheAi(page);
  await page.getByRole("button", { name: "Clone a voice" }).click();
  await recordFor(page, 30, page.getByRole("dialog", { name: "Clone a voice" }));
  await page.getByLabel("Name", { exact: true }).fill("Ana");
  await page.getByRole("button", { name: "Save" }).click();
  await page.getByRole("button", { name: "Ana", exact: true }).dispatchEvent("pointerdown");
});

const dialog = (page: Page) => page.getByRole("dialog", { name: "Train a model" });
const engines = (page: Page) => page.getByRole("group", { name: "Engine" });

/** Opens the training, downloads its tools and gives it 20 minutes of speech from two files. */
async function gatherSpeech(page: Page) {
  await page.getByRole("button", { name: "Train a model..." }).click();
  await dialog(page).getByRole("button", { name: "Download and continue" }).click();
  await page.clock.runFor(400);
  await dialog(page).getByRole("button", { name: "Choose files..." }).click();
  await expect(dialog(page).getByText("Speech: 20:00")).toBeVisible();
}

test("a cloned voice offers to train its model, and says what that takes first", async ({ page }) => {
  await page.getByRole("button", { name: "Train a model..." }).click();

  await expect(dialog(page).getByText("It trains on the NVIDIA GeForce RTX 3070 Ti.", { exact: false })).toBeVisible();
  await expect(dialog(page).getByText("a 4200 MB download, once", { exact: false })).toBeVisible();
  await expect(dialog(page).getByText("does not check whose voice it is", { exact: false })).toBeVisible();
  await expect(dialog(page).getByText("deleted once the model is added", { exact: false })).toBeVisible();
  await expect(dialog(page).getByRole("checkbox")).toHaveCount(0);

  await dialog(page).getByRole("button", { name: "Close" }).click();
  await expect(dialog(page)).toHaveCount(0);
});

test("the built-in AI voice is not trained in the app", async ({ page }) => {
  await page.getByRole("button", { name: "Feminine AI", exact: true }).dispatchEvent("pointerdown");

  await expect(page.getByRole("button", { name: "Add a trained model..." })).toBeVisible();
  await expect(page.getByRole("button", { name: "Train a model..." })).toHaveCount(0);
});

test("speech is gathered in takes, and training needs at least ten minutes of it", async ({ page }) => {
  await page.getByRole("button", { name: "Train a model..." }).click();
  await dialog(page).getByRole("button", { name: "Download and continue" }).click();
  await expect(dialog(page).getByText("Downloading the training tools...", { exact: false })).toBeVisible();
  await page.clock.runFor(400);

  await expect(dialog(page).getByText("Record or choose 10 to 40 minutes", { exact: false })).toBeVisible();
  await recordFor(page, 2, dialog(page));
  await expect(dialog(page).getByText("That take is too short.", { exact: false })).toBeVisible();
  await recordFor(page, 90, dialog(page));
  await expect(dialog(page).getByText("Take 1: 1:30")).toBeVisible();
  await expect(dialog(page).getByRole("button", { name: "Start training" })).toBeDisabled();

  await dialog(page).getByRole("button", { name: "Choose files..." }).click();

  await expect(dialog(page).getByText("Speech: 21:30")).toBeVisible();
  await expect(dialog(page).getByRole("button", { name: "Start training" })).toBeEnabled();
  await dialog(page).getByRole("button", { name: "Remove" }).first().click();
  await expect(dialog(page).getByText("Speech: 20:00")).toBeVisible();
});

test("a recording stops by itself at the longest a take can be", async ({ page }) => {
  await gatherSpeech(page);

  await dialog(page).getByRole("button", { name: "Record" }).click();
  await page.clock.fastForward(601_000);

  await expect(dialog(page).getByText("Speech: 30:00")).toBeVisible();
  await expect(dialog(page).getByRole("button", { name: "Record" })).toBeVisible();
});

test("a finished training puts its model on the voice, on the trained engine", async ({ page }) => {
  await gatherSpeech(page);

  await dialog(page).getByRole("button", { name: "Start training" }).click();
  await page.clock.runFor(40);
  await expect(dialog(page).getByText("Learning the voice: epoch 1 of 200.")).toBeVisible();
  await page.clock.runFor(8000);

  await expect(dialog(page)).toHaveCount(0);
  await expect(lcd(page)).toHaveText('The model of "Ana" is trained and in use.');
  await expect(engines(page).getByRole("button", { name: "Trained" })).toHaveAttribute("aria-pressed", "true");
});

test("a training goes on with its window closed, and pauses and resumes", async ({ page }) => {
  await gatherSpeech(page);
  await dialog(page).getByRole("button", { name: "Start training" }).click();
  await dialog(page).getByRole("button", { name: "Close" }).click();
  await page.clock.runFor(400);

  await expect(page.getByText(/^Training: epoch \d+ of 200\.$/)).toBeVisible();
  await page.getByRole("button", { name: "Open training" }).click();
  await dialog(page).getByRole("button", { name: "Pause" }).click();

  await expect(lcd(page)).toHaveText("Training paused.");
  await expect(dialog(page).getByText(/^Paused at epoch \d+ of 200\.$/)).toBeVisible();
  await dialog(page).getByRole("button", { name: "Resume" }).click();
  await expect(dialog(page).getByText(/^Learning the voice: epoch \d+ of 200\.$/)).toBeVisible();
});

test("discarding a training asks first and starts over", async ({ page }) => {
  await gatherSpeech(page);

  await dialog(page).getByRole("button", { name: "Discard training" }).click();

  await expect(dialog(page)).toHaveCount(0);
  await page.getByRole("button", { name: "Train a model..." }).click();
  await expect(dialog(page).getByText("The training tools are installed.", { exact: false })).toBeVisible();
});

test("the training tools can be removed once nothing uses them", async ({ page }) => {
  await gatherSpeech(page);
  await dialog(page).getByRole("button", { name: "Close" }).click();

  await page.getByRole("button", { name: "Remove training tools" }).click();

  await expect(lcd(page)).toHaveText("Training tools removed.");
  await expect(page.getByRole("button", { name: "Remove training tools" })).toHaveCount(0);
});
