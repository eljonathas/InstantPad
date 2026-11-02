import { expect, test, type Page } from "@playwright/test";
import { lcd, openApp, readyTheAi } from "./support";

test.beforeEach(async ({ page }) => {
  await openApp(page);
  await page.getByRole("tab", { name: "Voice" }).click();
});

const wizard = (page: Page) => page.getByRole("dialog", { name: "Clone a voice" });

async function acceptTerms(page: Page, ownerName: string) {
  for (const clause of await wizard(page).getByRole("checkbox").all()) await clause.check();
  await wizard(page).getByLabel("Full name of the voice's owner").fill(ownerName);
  await wizard(page).getByRole("button", { name: "Continue" }).click();
}

/** Takes the wizard from the first step to a saved voice with `name`. */
async function cloneVoice(page: Page, name: string) {
  await page.getByRole("button", { name: "Clone a voice" }).click();
  await acceptTerms(page, "Ana Souza");
  await wizard(page).getByRole("button", { name: "Record" }).click();
  await wizard(page).getByRole("button", { name: "Stop" }).click();
  await wizard(page).getByRole("button", { name: "Record" }).click();
  await wizard(page).getByRole("button", { name: "Stop" }).click();
  await wizard(page).getByLabel("Name", { exact: true }).fill(name);
  await wizard(page).getByRole("button", { name: "Save" }).click();
}

test("cloning without the AI asks for the download first", async ({ page }) => {
  await page.getByRole("button", { name: "Clone a voice" }).click();

  await expect(wizard(page)).toHaveCount(0);
  await expect(lcd(page)).toHaveText("Error");
  await expect(page.locator(".voice-title")).toHaveText("Clone a voice");
  await expect(page.getByRole("button", { name: "Download" })).toBeVisible();
});

test("a cloned voice goes through the terms, a sample and the phrase, and joins the effects", async ({ page }) => {
  await readyTheAi(page);

  await page.getByRole("button", { name: "Clone a voice" }).click();
  await expect(wizard(page).getByText("Read and accept each line")).toBeVisible();
  await acceptTerms(page, "Ana Souza");

  await expect(wizard(page).getByText("Record 20 to 300 seconds of natural speech")).toBeVisible();
  await wizard(page).getByRole("button", { name: "Record" }).click();
  await expect(wizard(page).getByRole("button", { name: "Choose a file..." })).toHaveCount(0);
  await wizard(page).getByRole("button", { name: "Stop" }).click();

  await expect(wizard(page).locator(".clone-digits")).toHaveText("482915");
  await wizard(page).getByRole("button", { name: "Record" }).click();
  await wizard(page).getByRole("button", { name: "Stop" }).click();

  await wizard(page).getByLabel("Name", { exact: true }).fill("Ana");
  await wizard(page).getByRole("button", { name: "Save" }).click();

  await expect(wizard(page)).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Ana", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Ana", exact: true }).locator(".effect-ai")).toHaveText("AI");
});

test("a file can stand in for the recording of the sample", async ({ page }) => {
  await readyTheAi(page);
  await page.getByRole("button", { name: "Clone a voice" }).click();
  await acceptTerms(page, "Ana Souza");

  await wizard(page).getByRole("button", { name: "Choose a file..." }).click();

  await expect(wizard(page).locator(".clone-digits")).toBeVisible();
});

test("continue stays off until every clause and the owner's name are given", async ({ page }) => {
  await readyTheAi(page);
  await page.getByRole("button", { name: "Clone a voice" }).click();
  const next = wizard(page).getByRole("button", { name: "Continue" });
  const clauses = await wizard(page).getByRole("checkbox").all();
  expect(clauses).toHaveLength(5);

  await expect(next).toBeDisabled();
  await wizard(page).getByLabel("Full name of the voice's owner").fill("Ana Souza");
  for (const clause of clauses.slice(0, 4)) await clause.check();
  await expect(next).toBeDisabled();

  await clauses[4].check();
  await expect(next).toBeEnabled();

  await wizard(page).getByLabel("Full name of the voice's owner").fill("   ");
  await expect(next).toBeDisabled();
});

test("cancel and Esc close the wizard", async ({ page }) => {
  await readyTheAi(page);
  await page.getByRole("button", { name: "Clone a voice" }).click();
  await wizard(page).getByRole("button", { name: "Cancel" }).click();
  await expect(wizard(page)).toHaveCount(0);

  await page.getByRole("button", { name: "Clone a voice" }).click();
  await acceptTerms(page, "Ana Souza");
  await page.keyboard.press("Escape");
  await expect(wizard(page)).toHaveCount(0);
});

test("a cloned voice is picked, shows its knob and is deleted", async ({ page }) => {
  await readyTheAi(page);
  await cloneVoice(page, "Ana");

  const voice = page.getByRole("button", { name: "Ana", exact: true });
  await voice.dispatchEvent("pointerdown");
  await expect(voice).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator(".voice-title")).toHaveText("Ana");
  await expect(page.getByRole("slider", { name: "Pitch" })).toBeVisible();

  await page.getByRole("button", { name: "Delete voice" }).click();

  await expect(voice).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Natural" })).toHaveAttribute("aria-pressed", "true");
});

test("two cloned voices stay apart, and only the one in use is down", async ({ page }) => {
  await readyTheAi(page);
  await cloneVoice(page, "Ana");
  await cloneVoice(page, "Beto");

  await page.getByRole("button", { name: "Beto", exact: true }).dispatchEvent("pointerdown");

  await expect(page.getByRole("button", { name: "Beto", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("button", { name: "Ana", exact: true })).toHaveAttribute("aria-pressed", "false");
});
