import { expect, test, type Page } from "@playwright/test";
import { installTheAi, lcd, openApp, readyTheAi, recordCommands, recordFor } from "./support";

test.beforeEach(async ({ page }) => {
  await page.clock.install();
  await openApp(page);
  await page.getByRole("tab", { name: "Voice" }).click();
});

const wizard = (page: Page) => page.getByRole("dialog", { name: "Clone a voice" });

async function giveSample(page: Page) {
  await recordFor(page, 30, wizard(page));
  await expect(wizard(page).getByLabel("Name", { exact: true })).toBeVisible();
}

/** Takes the wizard from the first step to a saved voice with `name`. */
async function cloneVoice(page: Page, name: string) {
  await page.getByRole("button", { name: "Clone a voice" }).click();
  await giveSample(page);
  await wizard(page).getByLabel("Name", { exact: true }).fill(name);
  await wizard(page).getByRole("button", { name: "Save" }).click();
}

test("cloning without the AI asks for the download, and then needs no calibration", async ({ page }) => {
  await page.getByRole("button", { name: "Clone a voice" }).click();

  await expect(wizard(page)).toHaveCount(0);
  await expect(lcd(page)).toHaveText("Error");
  await expect(page.locator(".voice-title")).toHaveText("Clone a voice");
  await expect(page.getByText("one-time download of 180 MB", { exact: false })).toBeVisible();

  await page.getByRole("button", { name: "Download" }).click();
  await expect(lcd(page)).toHaveText("AI voices installed.");
  await expect(page.getByRole("button", { name: "Record" })).toHaveCount(0);

  // The backend does not open it on its own: the user presses the key again.
  await expect(wizard(page)).toHaveCount(0);
  await page.getByRole("button", { name: "Clone a voice" }).click();
  await expect(wizard(page).getByText("Record 20 to 300 seconds of natural speech")).toBeVisible();
});

test("a cloned voice needs only a sample and a name, and joins the effects", async ({ page }) => {
  await installTheAi(page);

  await page.getByRole("button", { name: "Clone a voice" }).click();
  await expect(wizard(page).getByRole("checkbox")).toHaveCount(0);
  await expect(wizard(page).getByText("Record 20 to 300 seconds of natural speech")).toBeVisible();
  await expect(wizard(page).getByText("For a faithful clone")).toBeVisible();
  await expect(wizard(page).locator(".clone-tips li")).toHaveCount(6);
  await expect(wizard(page).getByText("Record in a quiet room with no echo.", { exact: false })).toBeVisible();
  await wizard(page).getByRole("button", { name: "Record" }).click();
  await expect(wizard(page).getByRole("button", { name: "Choose a file..." })).toHaveCount(0);
  await page.clock.fastForward(30_000);
  await wizard(page).getByRole("button", { name: "Stop" }).click();
  await expect(wizard(page).getByText("Listening to the recording")).toBeVisible();

  await expect(wizard(page).getByText("The AI heard the voice. Give it a name.")).toBeVisible();
  await wizard(page).getByLabel("Name", { exact: true }).fill("Ana");
  await wizard(page).getByRole("button", { name: "Save" }).click();

  await expect(wizard(page)).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Ana", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Ana", exact: true }).locator(".effect-ai")).toHaveText("AI");
});

test("a sample that is too short is refused with the minimum, and can be recorded again", async ({ page }) => {
  await installTheAi(page);
  await page.getByRole("button", { name: "Clone a voice" }).click();

  await recordFor(page, 3, wizard(page));

  await expect(wizard(page).getByText("The sample needs at least 20 seconds of speech.")).toBeVisible();
  await giveSample(page);
});

test("a file can stand in for the recording of the sample", async ({ page }) => {
  await installTheAi(page);
  await page.getByRole("button", { name: "Clone a voice" }).click();

  await wizard(page).getByRole("button", { name: "Choose a file..." }).click();

  await expect(wizard(page).getByLabel("Name", { exact: true })).toBeVisible();
});

test("cancel and Esc close the wizard", async ({ page }) => {
  await installTheAi(page);
  await page.getByRole("button", { name: "Clone a voice" }).click();
  await wizard(page).getByRole("button", { name: "Cancel" }).click();
  await expect(wizard(page)).toHaveCount(0);

  await page.getByRole("button", { name: "Clone a voice" }).click();
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
  await expect(page.getByText("Turn the Pitch until it is close to the cloned voice", { exact: false })).toBeVisible();

  await page.getByRole("button", { name: "Delete voice" }).click();

  await expect(voice).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Natural" })).toHaveAttribute("aria-pressed", "true");
});

test("a cloned voice picked before the AI knows the user waits, then comes on by itself", async ({ page }) => {
  await installTheAi(page);
  await cloneVoice(page, "Ana");

  await page.getByRole("button", { name: "Ana", exact: true }).dispatchEvent("pointerdown");
  await expect(page.locator(".voice-title")).toHaveText("Ana");
  await expect(page.getByText("read any text aloud", { exact: false })).toBeVisible();

  await recordFor(page, 8);

  await expect(page.getByRole("button", { name: "Ana", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("button", { name: "Delete voice" })).toBeVisible();
});

test("two cloned voices stay apart, and only the one in use is down", async ({ page }) => {
  await readyTheAi(page);
  await cloneVoice(page, "Ana");
  await cloneVoice(page, "Beto");

  await page.getByRole("button", { name: "Beto", exact: true }).dispatchEvent("pointerdown");

  await expect(page.getByRole("button", { name: "Beto", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("button", { name: "Ana", exact: true })).toHaveAttribute("aria-pressed", "false");
});

const designer = (page: Page) => page.getByRole("dialog", { name: "Design a voice" });
const designKey = (page: Page) => page.getByRole("button", { name: "Design a voice" });

async function openDesigner(page: Page) {
  await designKey(page).click();
  await expect(designer(page)).toBeVisible();
}

test("the designer only shows once a voice is cloned", async ({ page }) => {
  await readyTheAi(page);
  await expect(designKey(page)).toHaveCount(0);

  await cloneVoice(page, "Ana");

  await expect(designKey(page)).toBeVisible();
});

test("a voice designed from a clone and a mix is saved, shows up and is on", async ({ page }) => {
  await readyTheAi(page);
  await cloneVoice(page, "Ana");
  await cloneVoice(page, "Beto");
  await openDesigner(page);
  await expect(designer(page).getByText("Start with a small blend", { exact: false })).toBeVisible();
  await expect(designer(page).getByRole("slider", { name: "Amount" })).toHaveCount(0);

  await designer(page).getByLabel("Base voice").selectOption({ label: "Ana" });
  await expect(designer(page).getByLabel("Mix with").getByRole("option", { name: "Ana" })).toHaveCount(0);
  await designer(page).getByLabel("Mix with").selectOption({ label: "Beto" });
  await expect(designer(page).getByRole("slider", { name: "Amount" })).toBeVisible();
  await designer(page).getByRole("slider", { name: "Timbre" }).focus();
  await page.keyboard.press("ArrowUp");
  await expect(designer(page).getByText("+8%").first()).toBeVisible();
  await designer(page).getByLabel("Name", { exact: true }).fill("Ana and Beto");
  await designer(page).getByRole("button", { name: "Save" }).click();

  await expect(designer(page)).toHaveCount(0);
  const design = page.getByRole("button", { name: "Ana and Beto", exact: true });
  await expect(design).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("slider", { name: "Pitch" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Delete voice" })).toBeVisible();
});

test("the preview sends the recipe while it is on, and ends however the dialog closes", async ({ page }) => {
  await readyTheAi(page);
  await cloneVoice(page, "Ana");
  const calls = await recordCommands(page);
  const count = async (command: string) => (await calls()).filter((call) => call === command).length;
  await openDesigner(page);

  await designer(page).getByRole("slider", { name: "Timbre" }).focus();
  await page.keyboard.press("ArrowUp");
  expect(await count("preview_designed_voice")).toBe(0);

  await designer(page).getByRole("button", { name: "Hear preview" }).click();
  await expect.poll(() => count("preview_designed_voice")).toBe(1);
  await expect(designer(page).getByRole("button", { name: "Hear preview" })).toHaveAttribute("aria-pressed", "true");

  await designer(page).getByRole("slider", { name: "Timbre" }).focus();
  await page.keyboard.press("ArrowUp");
  await expect.poll(() => count("preview_designed_voice")).toBe(2);

  await designer(page).getByRole("button", { name: "Hear preview" }).click();
  await expect.poll(() => count("end_voice_preview")).toBe(1);
  await expect(designer(page).getByRole("button", { name: "Hear preview" })).toHaveAttribute("aria-pressed", "false");

  // The dialog tells the backend it closed, and nothing else: the backend ends the preview itself.
  await page.keyboard.press("Escape");
  await expect(designer(page)).toHaveCount(0);
  expect(await count("close_designer")).toBe(1);
  expect(await count("set_hearing_voice")).toBe(0);
});

test("the preview turns hearing on, and gives it back as the user had it", async ({ page }) => {
  await readyTheAi(page);
  await cloneVoice(page, "Ana");
  const hearing = page.getByRole("button", { name: "Hear my voice" });
  await openDesigner(page);

  await designer(page).getByRole("button", { name: "Hear preview" }).click();
  await expect(hearing).toHaveAttribute("aria-pressed", "true");
  await designer(page).getByRole("button", { name: "Hear preview" }).click();
  await expect(hearing).toHaveAttribute("aria-pressed", "false");

  await designer(page).getByRole("button", { name: "Hear preview" }).click();
  await expect(hearing).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("Escape");
  await expect(hearing).toHaveAttribute("aria-pressed", "false");

  await hearing.click();
  await openDesigner(page);
  await designer(page).getByRole("button", { name: "Hear preview" }).click();
  await page.keyboard.press("Escape");
  await expect(hearing).toHaveAttribute("aria-pressed", "true");
});

test("a preview the backend refuses stays off", async ({ page }) => {
  await installTheAi(page);
  await cloneVoice(page, "Ana");
  await openDesigner(page);

  await designer(page).getByRole("button", { name: "Hear preview" }).click();

  await expect(lcd(page)).toHaveText("Error");
  await expect(page.getByText("must learn your own voice first", { exact: false })).toBeVisible();
  await expect(designer(page).getByRole("button", { name: "Hear preview" })).toHaveAttribute("aria-pressed", "false");
  await expect(page.getByRole("button", { name: "Hear my voice" })).toHaveAttribute("aria-pressed", "false");
});

test("a save the backend refuses leaves the designer open", async ({ page }) => {
  await readyTheAi(page);
  await cloneVoice(page, "Ana");
  await openDesigner(page);

  await page.evaluate(() =>
    (
      window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string, args: unknown) => unknown } }
    ).__TAURI_INTERNALS__.invoke("save_designed_voice", {
      name: "  ",
      recipe: { baseId: "x", timbre: 0.5, blendId: null, blend: 0.3 },
    }),
  );

  await expect(page.getByText("A name is needed.")).toBeVisible();
  await expect(designer(page)).toBeVisible();
});

test("a design needs a name before it can be saved", async ({ page }) => {
  await readyTheAi(page);
  await cloneVoice(page, "Ana");
  await openDesigner(page);

  await expect(designer(page).getByRole("button", { name: "Save" })).toBeDisabled();
  await designer(page).getByLabel("Name", { exact: true }).fill("Ana deeper");
  await expect(designer(page).getByRole("button", { name: "Save" })).toBeEnabled();
});

test("deleting a cloned voice deletes the voices designed from it", async ({ page }) => {
  await readyTheAi(page);
  await cloneVoice(page, "Ana");
  await cloneVoice(page, "Beto");
  await openDesigner(page);
  await designer(page).getByLabel("Base voice").selectOption({ label: "Ana" });
  await designer(page).getByLabel("Name", { exact: true }).fill("Ana deeper");
  await designer(page).getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("button", { name: "Ana deeper", exact: true })).toBeVisible();

  await page.getByRole("button", { name: "Ana", exact: true }).dispatchEvent("pointerdown");
  await page.getByRole("button", { name: "Delete voice" }).click();

  await expect(page.getByRole("button", { name: "Ana", exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Ana deeper", exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Beto", exact: true })).toBeVisible();
});

test("deleting a designed voice leaves the voices it came from", async ({ page }) => {
  await readyTheAi(page);
  await cloneVoice(page, "Ana");
  await openDesigner(page);
  await designer(page).getByLabel("Name", { exact: true }).fill("Ana deeper");
  await designer(page).getByRole("button", { name: "Save" }).click();

  await page.getByRole("button", { name: "Delete voice" }).click();

  await expect(page.getByRole("button", { name: "Ana deeper", exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Ana", exact: true })).toBeVisible();
});

test("deleting a voice that is waiting for the AI leaves nothing waiting", async ({ page }) => {
  await installTheAi(page);
  await cloneVoice(page, "Ana");
  await page.getByRole("button", { name: "Ana", exact: true }).dispatchEvent("pointerdown");
  await expect(page.locator(".voice-title")).toHaveText("Ana");
  await expect(page.getByText("read any text aloud", { exact: false })).toBeVisible();

  // Its key is out of reach while the AI is being set up, so the command goes straight to the backend.
  await page.evaluate(() =>
    (
      window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string, args: unknown) => unknown } }
    ).__TAURI_INTERNALS__.invoke("delete_cloned_voice", { id: "clone-1" }),
  );

  await expect(page.getByRole("button", { name: "Ana", exact: true })).toHaveCount(0);
  await expect(page.getByText("read any text aloud", { exact: false })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Natural" })).toHaveAttribute("aria-pressed", "true");
});

test("the microphone is not shared between a calibration and the cloning dialog", async ({ page }) => {
  await installTheAi(page);
  await page.getByRole("button", { name: "Feminine AI", exact: true }).dispatchEvent("pointerdown");
  await page.getByRole("button", { name: "Record" }).click();
  await page.getByRole("button", { name: "Clone a voice" }).click();

  await wizard(page).getByRole("button", { name: "Record" }).click();

  await expect(page.getByText("The microphone is already recording.", { exact: false })).toBeVisible();
  await expect(wizard(page).getByRole("button", { name: "Record" })).toBeVisible();
});
