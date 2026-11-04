import { expect, test, type Page } from "@playwright/test";
import { lcd, openApp, pad, recordCommands, soundAction } from "./support";

test.beforeEach(async ({ page }) => openApp(page));

const editor = (page: Page, name = "Airhorn") => page.getByRole("dialog", { name: `Cut into pads: ${name}` });
const pieces = (page: Page) => page.locator(".cut-piece");
const cutPads = (page: Page) => page.locator(".pad .pad-cut");

async function openAirhorn(page: Page) {
  await soundAction(page, "Airhorn", "Cut into pads...");
  await expect(editor(page)).toBeVisible();
}

/** Clicks the waveform at a share of its width, from 0 to 1, which moves the playhead there. */
async function clickWaveform(page: Page, share: number) {
  const wave = page.getByRole("img", { name: "Waveform" });
  const box = (await wave.boundingBox())!;
  await wave.click({ position: { x: box.width * share, y: box.height / 2 } });
}

test("a sound of MyInstants is cut at its pauses into pads of their own", async ({ page }) => {
  await openAirhorn(page);
  await expect(editor(page).getByText("Pieces (1)")).toBeVisible();

  await editor(page).getByRole("button", { name: "Detect cuts" }).click();

  await expect(editor(page).getByText("Pieces (4)")).toBeVisible();
  await expect(pieces(page)).toHaveCount(4);
  await editor(page).getByRole("button", { name: "Create 4 pads" }).click();

  await expect(editor(page)).toHaveCount(0);
  // The first three pads have sounds, so the pieces go to the next free ones.
  for (const [label, name] of [
    ["04", "Airhorn 1"],
    ["05", "Airhorn 2"],
    ["06", "Airhorn 3"],
    ["07", "Airhorn 4"],
  ]) {
    await expect(pad(page, `${label} ${name}`)).toBeVisible();
  }
  await expect(cutPads(page)).toHaveCount(4);
  await expect(lcd(page)).toHaveText("4 pads created.");
  await expect(pad(page, "05 Airhorn 2")).toHaveAttribute("title", /Piece 0:02\.0-0:04\.2/);
});

test("the keys mark a cue at the playhead, remove it and play each piece", async ({ page }) => {
  const calls = await recordCommands(page);
  await openAirhorn(page);

  await clickWaveform(page, 0.5);
  await expect(editor(page).getByLabel("Playhead")).toContainText("0:04.");
  await page.keyboard.press("c");
  await expect(pieces(page)).toHaveCount(2);
  await expect(editor(page).getByRole("button", { name: "Remove cue" })).toBeEnabled();

  await page.keyboard.press("2");
  await expect.poll(async () => (await calls()).filter((call) => call === "preview_cut").length).toBe(1);

  await page.keyboard.press("Delete");
  await expect(pieces(page)).toHaveCount(1);
  await expect(editor(page).getByRole("button", { name: "Remove cue" })).toBeDisabled();
});

test("a double click on the waveform marks a cue there", async ({ page }) => {
  await openAirhorn(page);
  const wave = page.getByRole("img", { name: "Waveform" });
  const box = (await wave.boundingBox())!;

  await wave.dblclick({ position: { x: box.width * 0.25, y: box.height / 2 } });

  await expect(pieces(page)).toHaveCount(2);
  await expect(pieces(page).nth(1)).toContainText("0:02.");
});

test("pieces can be renamed or left out before the pads are made", async ({ page }) => {
  await openAirhorn(page);
  await editor(page).getByRole("button", { name: "Detect cuts" }).click();

  await editor(page).getByLabel("Name of piece 2").fill("Horn");
  await editor(page).getByLabel("Make a pad of piece 3").uncheck();
  await expect(editor(page).getByRole("button", { name: "Create 3 pads" })).toBeEnabled();
  await editor(page).getByRole("button", { name: "Create 3 pads" }).click();

  await expect(pad(page, "05 Horn")).toBeVisible();
  await expect(page.locator(".pad", { hasText: "Airhorn 3" })).toHaveCount(0);
  await expect(pad(page, "06 Airhorn 4")).toBeVisible();
});

test("more pieces than free pads are refused until some are left out", async ({ page }) => {
  // Fills the main interface up to its last two pads.
  await page.evaluate(() =>
    (
      window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string, args: unknown) => unknown } }
    ).__TAURI_INTERNALS__.invoke("import_files", {
      paths: Array.from({ length: 19 }, (_, index) => `/mock/Music/Filler ${index + 1}.mp3`),
      slot: null,
    }),
  );
  await expect(page.locator(".pad", { hasText: "Filler 19" })).toBeVisible();
  await openAirhorn(page);
  await editor(page).getByRole("button", { name: "Detect cuts" }).click();

  await expect(editor(page).getByText("Only 2 free pads on this interface.", { exact: false })).toBeVisible();
  await expect(editor(page).getByRole("button", { name: "Create 4 pads" })).toBeDisabled();

  await editor(page).getByLabel("Make a pad of piece 1").uncheck();
  await editor(page).getByLabel("Make a pad of piece 2").uncheck();

  await expect(editor(page).getByRole("button", { name: "Create 2 pads" })).toBeEnabled();
  await expect(editor(page).getByText("Only 2 free pads", { exact: false })).toHaveCount(0);
});

test("Cancel and Esc close the editor and let the sound go", async ({ page }) => {
  const calls = await recordCommands(page);
  await openAirhorn(page);
  await editor(page).getByRole("button", { name: "Cancel" }).click();
  await expect(editor(page)).toHaveCount(0);

  await openAirhorn(page);
  await page.keyboard.press("Escape");
  await expect(editor(page)).toHaveCount(0);

  await expect.poll(async () => (await calls()).filter((call) => call === "close_cut_editor").length).toBe(2);
  await expect(cutPads(page)).toHaveCount(0);
});

test("a pad opens in the editor with its cut as cue points", async ({ page }) => {
  await openAirhorn(page);
  await editor(page).getByRole("button", { name: "Detect cuts" }).click();
  await editor(page).getByRole("button", { name: "Create 4 pads" }).click();

  await pad(page, "05 Airhorn 2").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Cut into pads..." }).click();

  await expect(editor(page, "Airhorn 2")).toBeVisible();
  await expect(pieces(page)).toHaveCount(2);
  await expect(pieces(page).first()).toContainText("0:02.09–0:04.29");
});

test("a file of the computer can be cut too", async ({ page }) => {
  await page.getByRole("button", { name: "Cut a file from the computer..." }).click();

  await expect(editor(page, "Applause")).toBeVisible();
  await editor(page, "Applause").getByRole("button", { name: "Create 1 pad" }).click();

  await expect(pad(page, "04 Applause 1")).toBeVisible();
});

test("a piece is played from its key in the list", async ({ page }) => {
  const calls = await recordCommands(page);
  await openAirhorn(page);
  await editor(page).getByRole("button", { name: "Detect cuts" }).click();

  await editor(page).getByRole("button", { name: "Play piece 3" }).click();

  await expect.poll(async () => (await calls()).filter((call) => call === "preview_cut").length).toBe(1);
  await expect(editor(page).getByLabel("Playhead")).toContainText("0:04.");
});

test("playing again after stopping halfway plays", async ({ page }) => {
  const calls = await recordCommands(page);
  await openAirhorn(page);
  const play = editor(page).locator(".cut-play");

  await play.click();
  await expect(play).toHaveText("Stop");
  await page.waitForTimeout(250);
  await play.click();
  await expect(play).toHaveText("Play");
  await play.click();

  await expect(play).toHaveText("Stop");
  await expect.poll(async () => (await calls()).filter((call) => call === "preview_cut").length).toBe(2);
});

test("Space on a focused key presses that key instead of playing", async ({ page }) => {
  const calls = await recordCommands(page);
  await openAirhorn(page);

  await editor(page).getByRole("button", { name: "Cancel" }).focus();
  await page.keyboard.press("Space");

  await expect(editor(page)).toHaveCount(0);
  expect((await calls()).filter((call) => call === "preview_cut")).toHaveLength(0);
});

test("a name and a left out piece stay with their piece when a cue is added before it", async ({ page }) => {
  await openAirhorn(page);
  await editor(page).getByRole("button", { name: "Detect cuts" }).click();
  await editor(page).getByLabel("Name of piece 3").fill("Horn");
  await editor(page).getByLabel("Make a pad of piece 4").uncheck();

  // A cue early in the first piece makes every later piece one further down the list.
  await clickWaveform(page, 0.08);
  await page.keyboard.press("c");

  await expect(pieces(page)).toHaveCount(5);
  await expect(editor(page).getByLabel("Name of piece 4")).toHaveValue("Horn");
  await expect(editor(page).getByLabel("Make a pad of piece 5")).not.toBeChecked();
  await expect(editor(page).getByLabel("Make a pad of piece 4")).toBeChecked();
});
