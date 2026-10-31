// Stands in for the backend when the interface runs in a plain browser.
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { AppEvent, BoardView, MeterFrame, SoundSource, StartView } from "./bindings";

const METER_INTERVAL_MS = 33;

const sounds: SoundSource[] = [
  { name: "Vine boom", color: "#ff3b5c", path: "/mock/vine-boom.mp3", soundUrl: "https://mock/vine-boom.mp3" },
  { name: "Bruh", color: "#1fd6ff", path: "/mock/bruh.mp3", soundUrl: "https://mock/bruh.mp3" },
  { name: "Sad trombone", color: "#ffd21f", path: "/mock/trombone.mp3", soundUrl: null },
];

function seedBoard(): BoardView {
  const pads = Array.from({ length: 24 }, (_, slot) => ({
    slot,
    label: String(slot + 1).padStart(2, "0"),
    shortcut: slot < 10 ? `Num${(slot + 1) % 10}` : "",
    isFavorite: slot === 1,
    sound:
      slot < sounds.length
        ? {
            id: `sound-${slot}`,
            name: sounds[slot].name,
            path: sounds[slot].path,
            color: sounds[slot].color,
            volume: 1,
            hasHotkey: false,
            isLight: slot === 2,
          }
        : null,
  }));

  return {
    interfaces: [
      { id: "main", name: "Main", rows: 4, columns: 6, accent: "#ffb03a", soundCount: 3 },
      { id: "memes", name: "Memes", rows: 2, columns: 4, accent: "#35f27a", soundCount: 0 },
    ],
    activeId: "main",
    pads,
    targetSlot: null,
    stopShortcut: "Num,",
  };
}

function seedStart(): StartView {
  return {
    board: seedBoard(),
    mixer: { micVolume: 1, gameVolume: 0.8, monitorVolume: 0.6, micEnabled: true },
    favorites: [sounds[1]],
    setup: {
      microphones: [{ id: "mic", name: "USB Microphone" }],
      gameOutputs: [{ id: "cable", name: "CABLE Input (VB-Audio Virtual Cable)" }],
      monitorOutputs: [{ id: "phones", name: "Headphones" }],
      microphoneId: "mic",
      gameOutputId: "cable",
      monitorOutputId: "phones",
      hasCable: true,
      isSystemOutputOnCable: false,
      gameMicrophoneName: "CABLE Output",
      language: null,
      numericScheme: "numpad",
      stopHotkey: null,
    },
  };
}

function emitBackend(event: AppEvent) {
  void emit("backend-event", event);
}

export function installMock() {
  let start = seedStart();
  let playing: number[] = [];

  mockWindows("main");
  mockIPC(
    (command, args) => {
      const payload = args as Record<string, never>;
      switch (command) {
        case "start":
          start = seedStart();
          return start;
        case "search":
        case "trending":
          return { sounds, hasMore: false };
        case "suggested_file_name":
          return `${(payload.source as SoundSource).name}.mp3`;
        case "press_pad": {
          const slot = payload.slot as number;
          const pad = start.board.pads[slot];
          if (pad.sound) {
            playing = [slot];
            emitBackend({
              kind: "display",
              data: { kind: "show", message: { kind: "plain", text: `${pad.label} ${pad.sound.name}` } },
            });
          } else {
            start.board = { ...start.board, targetSlot: slot };
            emitBackend({ kind: "board", data: start.board });
          }
          return null;
        }
        case "stop_all":
          playing = [];
          return null;
        // Everything else only changes state the real backend owns.
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );

  setInterval(() => {
    const frame: MeterFrame = { micLevel: 0.3, outputLevel: playing.length ? 0.6 : 0, playingSlots: playing };
    void emit("meter-event", frame);
  }, METER_INTERVAL_MS);
}
