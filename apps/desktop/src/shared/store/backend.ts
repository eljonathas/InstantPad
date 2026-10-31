import { useSyncExternalStore } from "react";
import type { TextId } from "../i18n";
import type { AppEvent, BoardView, Message, MixerView, SetupView, SoundSource, StartView, UpdateView } from "../ipc";

/** What the LCD shows. A bare text id is one of the interface's own words ("Ready", "Error"). */
export interface DisplayState {
  main: Message | TextId;
  status: Message | null;
  hint: Message | null;
}

/** A mirror of what the backend sent. The interface never changes it on its own. */
export interface BackendState {
  started: boolean;
  board: BoardView;
  mixer: MixerView;
  setup: SetupView;
  favorites: SoundSource[];
  display: DisplayState;
  driverStatus: Message | null;
  update: UpdateView | null;
}

let state: BackendState = {
  started: false,
  board: { interfaces: [], activeId: "", pads: [], targetSlot: null, stopShortcut: null },
  mixer: { micVolume: 1, gameVolume: 0.8, monitorVolume: 0.6, micEnabled: true },
  setup: {
    microphones: [],
    gameOutputs: [],
    monitorOutputs: [],
    microphoneId: null,
    gameOutputId: null,
    monitorOutputId: null,
    hasCable: false,
    isSystemOutputOnCable: false,
    gameMicrophoneName: "",
    language: null,
    numericScheme: "numpad",
    stopHotkey: null,
  },
  favorites: [],
  display: { main: "Ready", status: null, hint: null },
  driverStatus: null,
  update: null,
};

const listeners = new Set<() => void>();

function replace(next: Partial<BackendState>) {
  state = { ...state, ...next };
  listeners.forEach((listener) => listener());
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => void listeners.delete(listener);
}

export function useBackend<T>(select: (state: BackendState) => T): T {
  return useSyncExternalStore(subscribe, () => select(state));
}

export function loadStart(view: StartView) {
  replace({ ...view, started: true });
}

/** The mixer has no event: the backend only receives its values, so the knobs keep them. */
export function setMixer(change: Partial<MixerView>) {
  replace({ mixer: { ...state.mixer, ...change } });
}

export function applyEvent(event: AppEvent) {
  switch (event.kind) {
    case "board":
      return replace({ board: event.data });
    case "favorites":
      return replace({ favorites: event.data });
    case "setup":
      // A routing change replaces whatever the installer last said.
      return replace({ setup: event.data, driverStatus: null });
    case "driverStatus":
      return replace({ driverStatus: event.data });
    case "update":
      return replace({ update: event.data });
    case "display": {
      const shown = event.data;
      if (shown.kind === "show") return replace({ display: { ...state.display, main: shown.message } });
      if (shown.kind === "error") {
        return replace({ display: { ...state.display, main: "Error", status: shown.message } });
      }
      return replace({ display: { ...state.display, status: shown.status, hint: shown.hint } });
    }
  }
}
