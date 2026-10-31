// Stands in for the backend when the interface runs in a plain browser. It keeps just enough
// state to make every screen work; the real rules live in the Rust crates.
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type {
  AppEvent,
  BoardView,
  Hotkey,
  InterfaceLayout,
  Message,
  MeterFrame,
  NumericScheme,
  SetupView,
  SoundPage,
  SoundSource,
  StartView,
  VoiceEffect,
  VoiceParam,
  VoiceView,
} from "./bindings";

const DOWNLOAD_MS = 400;
const SEARCH_MS = 300;
const PAD_COLORS = ["#ff3b5c", "#ff8a1f", "#ffd21f", "#35f27a", "#1fd6ff", "#4f7dff", "#b45cff", "#ff5cc8"];
const ACCENT_COLORS = ["#ffb03a", "#35f27a", "#1fd6ff", "#4f7dff", "#b45cff", "#ff5cc8", "#ff3b5c", "#e8e8ea"];

const LIMITS = {
  padColors: PAD_COLORS,
  accentColors: ACCENT_COLORS,
  maxRows: 8,
  maxColumns: 10,
  defaultRows: 4,
  defaultColumns: 6,
  defaultMixer: { micVolume: 1, gameVolume: 0.8, monitorVolume: 0.6, micEnabled: true },
};

const VOICE_KEYS: Record<NumericScheme, string[]> = {
  numpad: ["Num*", "Num/"],
  ctrlNumpad: ["Ctrl+Num*", "Ctrl+Num/"],
  altDigits: ["Alt+V", "Alt+B"],
  off: [],
};

const MOCK_PEAKS = Array.from({ length: 44 }, (_, index) => 0.25 + 0.75 * Math.abs(Math.sin(index * 0.7)));

function seedVoice(): VoiceView {
  const param = (param: VoiceParam, label: "ParamIntensity" | "ParamFormant" | "ParamTime" | "ParamHigh" | "ParamLow") => ({
    param,
    label,
    value: 0.5,
    defaultValue: 0.5,
    text: "50%",
  });
  return {
    active: "natural",
    isHearing: false,
    effects: [
      { effect: "natural", name: "EffectNatural", icon: "mic", color: "#1fd6ff", params: [] },
      {
        effect: "thin",
        name: "EffectThin",
        icon: "feather",
        color: "#1fd6ff",
        params: [param("thinIntensity", "ParamIntensity"), param("thinFormant", "ParamFormant")],
      },
      {
        effect: "deep",
        name: "EffectDeep",
        icon: "mountain",
        color: "#b45cff",
        params: [param("deepIntensity", "ParamIntensity"), param("deepFormant", "ParamFormant")],
      },
      {
        effect: "pitchCycle",
        name: "EffectCycle",
        icon: "wave",
        color: "#35f27a",
        params: [param("cycleTime", "ParamTime"), param("cycleHigh", "ParamHigh"), param("cycleLow", "ParamLow")],
      },
    ],
  };
}

const NUMERIC_HINT_KEYS: Record<NumericScheme, string[]> = {
  numpad: ["Num1", "Num0", "Num+", "Num-", "Num,"],
  ctrlNumpad: ["Ctrl+Num1", "Ctrl+Num0", "Ctrl+Num+", "Ctrl+Num-", "Ctrl+Num,"],
  altDigits: ["Alt+1", "Alt+0", "Alt+=", "Alt+-", "Alt+."],
  off: [],
};

const METER_INTERVAL_MS = 33;
const PLAY_MS = 1800;
const PAGE_SIZE = 8;
const LIGHT_LUMINANCE = 0.55;

type Args = Record<string, unknown>;

interface MockSound {
  id: string;
  interfaceId: string;
  slot: number;
  name: string;
  path: string;
  color: string;
  volume: number;
  hotkey: Hotkey | null;
}

const source = (name: string, index: number): SoundSource => {
  const slug = name.toLowerCase().replaceAll(" ", "-");
  return {
    name,
    color: PAD_COLORS[index % PAD_COLORS.length],
    path: `/mock/${slug}.mp3`,
    soundUrl: `https://mock/${slug}.mp3`,
  };
};

const catalog = [
  "Vine boom",
  "Bruh",
  "Sad trombone",
  "Airhorn",
  "Wilhelm scream",
  "Rimshot",
  "Crickets",
  "Dun dun dun",
  "Sudden Discord ping",
  "Oof",
  "Windows XP startup",
  "Nope",
  "Applause",
  "Record scratch",
  "Fail horn",
  "Taco bell bong",
  "Metal pipe",
  "Sparta kick",
  "Mission failed",
  "Hello there",
  "Bonk",
  "Yeet",
  "Omae wa mou",
  "Surprise",
].map(source);

const NO_HOTKEY = { ctrl: false, alt: false, shift: false, meta: false };

function hotkeyLabel({ key, modifiers }: Hotkey) {
  const names: Record<string, string> = {
    NumpadAdd: "Num+",
    NumpadSubtract: "Num-",
    NumpadDecimal: "Num,",
    Equal: "=",
    Minus: "-",
    Period: ".",
  };
  const mods = [modifiers.ctrl && "Ctrl", modifiers.alt && "Alt", modifiers.shift && "Shift", modifiers.meta && "Win"];
  const name = names[key] ?? key.replace(/^(Numpad)/, "Num").replace(/^(Digit|Key)/, "");
  return [...mods.filter(Boolean), name].join("+");
}

function numericHotkeys(scheme: NumericScheme): Hotkey[] {
  if (scheme === "off") return [];
  const modifiers = { ...NO_HOTKEY, ctrl: scheme === "ctrlNumpad", alt: scheme === "altDigits" };
  const prefix = scheme === "altDigits" ? "Digit" : "Numpad";
  return Array.from({ length: 10 }, (_, index) => ({ key: `${prefix}${(index + 1) % 10}`, modifiers }));
}

function defaultStop(scheme: NumericScheme): Hotkey | null {
  if (scheme === "off") return null;
  return scheme === "altDigits"
    ? { key: "Period", modifiers: { ...NO_HOTKEY, alt: true } }
    : { key: "NumpadDecimal", modifiers: { ...NO_HOTKEY, ctrl: scheme === "ctrlNumpad" } };
}

const isLight = (hex: string) => {
  const [red, green, blue] = [1, 3, 5].map((at) => parseInt(hex.slice(at, at + 2), 16));
  return (0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255 > LIGHT_LUMINANCE;
};

const plain = (text: string): Message => ({ kind: "plain", text });

function seed() {
  const interfaces = [
    { id: "main", name: "Main", rows: 4, columns: 6, accent: ACCENT_COLORS[0] },
    { id: "memes", name: "Memes", rows: 2, columns: 4, accent: ACCENT_COLORS[1] },
  ];
  const sounds: MockSound[] = catalog.slice(0, 3).map((item, slot) => ({
    id: `sound-${slot}`,
    interfaceId: "main",
    slot,
    name: item.name,
    path: item.path,
    color: item.color,
    volume: 1,
    hotkey: null,
  }));

  return {
    interfaces,
    sounds,
    activeId: "main",
    targetSlot: null as number | null,
    favorites: [catalog[1]],
    setup: {
      microphones: [
        { id: "mic", name: "USB Microphone" },
        { id: "headset", name: "Headset Microphone" },
      ],
      gameOutputs: [{ id: "cable", name: "CABLE Input (VB-Audio Virtual Cable)" }],
      monitorOutputs: [
        { id: "phones", name: "Headphones" },
        { id: "speakers", name: "Speakers" },
      ],
      microphoneId: "mic",
      gameOutputId: "cable",
      monitorOutputId: "phones",
      hasCable: true,
      cableName: "VB-CABLE",
      isSystemOutputOnCable: false,
      gameMicrophoneName: "CABLE Output",
      language: null,
      numericScheme: "numpad",
      numericHintKeys: NUMERIC_HINT_KEYS.numpad,
      voiceKeys: VOICE_KEYS.numpad,
      stopHotkey: null,
      oneSoundAtATime: false,
    } as SetupView,
    mixer: { micVolume: 1, gameVolume: 0.8, monitorVolume: 0.6, micEnabled: true },
    voice: seedVoice(),
  };
}

export function installMock() {
  let backend = seed();
  let playing = new Set<string>();
  let counter = 0;
  let startedAt = 0;
  const loading = new Set<number>();

  /** Shows `slot` as loading for a moment, like a sound that has to download. */
  function arrive(slot: number, place: () => void) {
    loading.add(slot);
    publish();
    setTimeout(() => {
      loading.delete(slot);
      place();
    }, DOWNLOAD_MS);
  }

  const active = () => backend.interfaces.find((item) => item.id === backend.activeId)!;
  const soundAt = (slot: number) =>
    backend.sounds.find((item) => item.interfaceId === backend.activeId && item.slot === slot);
  const label = (slot: number) => String(slot + 1).padStart(2, "0");
  const send = (event: AppEvent) => void emit("backend-event", event);
  const show = (message: Message) => send({ kind: "display", data: { kind: "show", message } });
  const fail = (message: Message) => send({ kind: "display", data: { kind: "error", message } });
  const isFavorite = (path: string) => backend.favorites.some((item) => item.path.toLowerCase() === path.toLowerCase());

  function board(): BoardView {
    const { setup } = backend;
    const numeric = numericHotkeys(setup.numericScheme);
    const stop = setup.stopHotkey ?? defaultStop(setup.numericScheme);
    const { rows, columns } = active();

    return {
      interfaces: backend.interfaces.map((item) => ({
        ...item,
        soundCount: backend.sounds.filter((sound) => sound.interfaceId === item.id).length,
      })),
      activeId: backend.activeId,
      targetSlot: backend.targetSlot,
      stopShortcut: stop ? hotkeyLabel(stop) : null,
      nextAccent: ACCENT_COLORS[backend.interfaces.length % ACCENT_COLORS.length],
      pads: Array.from({ length: rows * columns }, (_, slot) => {
        const sound = soundAt(slot);
        const shortcut = sound?.hotkey ?? numeric[slot];
        return {
          slot,
          label: label(slot),
          shortcut: shortcut ? hotkeyLabel(shortcut) : "",
          isFavorite: !!sound && isFavorite(sound.path),
          isLoading: loading.has(slot),
          sound: sound
            ? {
                id: sound.id,
                name: sound.name,
                path: sound.path,
                color: sound.color,
                volume: sound.volume,
                hasHotkey: !!sound.hotkey,
                isLight: isLight(sound.color),
              }
            : null,
        };
      }),
    };
  }

  const publish = () => send({ kind: "board", data: board() });
  function publishSetup() {
    const { setup } = backend;
    const absent = plain("--");
    const ok = (present: boolean, missing: Message): Message =>
      present ? { kind: "text", key: "Ok", args: [] } : missing;
    const status: Message = {
      kind: "text",
      key: "RoutingStatus",
      args: [
        ok(!!setup.microphoneId, absent),
        ok(!!setup.gameOutputId, absent),
        ok(setup.hasCable, { kind: "text", key: "Missing", args: [] }),
      ],
    };
    const hint: Message = !setup.hasCable
      ? { kind: "text", key: "NoDriverHint", args: [] }
      : setup.isSystemOutputOnCable
        ? { kind: "text", key: "SystemOutputOnCableHint", args: [] }
        : { kind: "text", key: "GameMicrophoneHint", args: [plain(setup.gameMicrophoneName)] };
    send({ kind: "display", data: { kind: "routing", status, hint } });
    send({ kind: "setup", data: { ...setup } });
  }

  function activate(id: string) {
    backend.activeId = id;
    backend.targetSlot = null;
  }

  function play(slot: number) {
    const sound = soundAt(slot);
    if (!sound) return;
    playing.add(sound.id);
    startedAt = Date.now();
    setTimeout(() => {
      playing.delete(sound.id);
      if (playing.size === 0) send({ kind: "display", data: { kind: "silent" } });
    }, PLAY_MS);
    send({ kind: "display", data: { kind: "sound", message: plain(`${label(slot)} ${sound.name}`) } });
  }

  function place(slot: number, name: string, path: string, color: string) {
    backend.sounds = backend.sounds.filter((item) => !(item.interfaceId === backend.activeId && item.slot === slot));
    backend.sounds.push({
      id: `sound-${++counter}-${slot}`,
      interfaceId: backend.activeId,
      slot,
      name,
      path,
      color,
      volume: 1,
      hotkey: null,
    });
    backend.targetSlot = null;
    publish();
    show(plain(`${label(slot)} ${name}`));
  }

  function edit(slot: number, change: (sound: MockSound) => void) {
    const sound = soundAt(slot);
    if (sound) change(sound);
    publish();
  }

  function pageOf(list: SoundSource[], page: number): SoundPage {
    return { sounds: list.slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE), hasMore: list.length > page * PAGE_SIZE };
  }

  const handlers: Record<string, (args: Args) => unknown> = {
    start: () => {
      backend = seed();
      playing = new Set();
      // The update line of the settings screen, once the listeners are in place.
      setTimeout(
        () =>
          send({
            kind: "update",
            data: { status: { kind: "text", key: "DevelopmentVersion", args: [] }, isReadyToRestart: false },
          }),
        0,
      );
      return {
        board: board(),
        mixer: backend.mixer,
        setup: backend.setup,
        favorites: backend.favorites,
        limits: LIMITS,
        voice: backend.voice,
      } satisfies StartView;
    },
    press_pad: ({ slot }) => {
      if (soundAt(slot as number)) return play(slot as number);
      backend.targetSlot = slot as number;
      publish();
      show({ kind: "text", key: "PadSelected", args: [plain(label(slot as number))] });
    },
    select_interface: ({ id }) => {
      activate(id as string);
      publish();
      show(plain(active().name));
    },
    step_interface: ({ offset }) => {
      const index = backend.interfaces.findIndex((item) => item.id === backend.activeId) + (offset as number);
      const next = backend.interfaces[index];
      if (!next) return;
      activate(next.id);
      publish();
      show(plain(next.name));
    },
    add_interface: ({ layout }) => {
      const id = `interface-${++counter}`;
      backend.interfaces.push({ id, ...(layout as InterfaceLayout) });
      activate(id);
      publish();
      show(plain((layout as InterfaceLayout).name));
    },
    configure_interface: ({ id, layout }) => {
      const target = backend.interfaces.find((item) => item.id === id);
      if (target) Object.assign(target, layout);
      publish();
    },
    move_interface: ({ id, offset }) => {
      const from = backend.interfaces.findIndex((item) => item.id === id);
      const to = from + (offset as number);
      if (from < 0 || to < 0 || to >= backend.interfaces.length) return;
      backend.interfaces.splice(to, 0, ...backend.interfaces.splice(from, 1));
      publish();
    },
    delete_interface: ({ id }) => {
      if (backend.interfaces.length < 2) return;
      const index = backend.interfaces.findIndex((item) => item.id === id);
      backend.interfaces.splice(index, 1);
      backend.sounds = backend.sounds.filter((item) => item.interfaceId !== id);
      if (backend.activeId === id) activate(backend.interfaces[Math.max(0, index - 1)].id);
      publish();
    },
    import_file: ({ path, slot }) => {
      const stem = (path as string)
        .split(/[/\\]/)
        .pop()!
        .replace(/\.[^.]+$/, "");
      const color = PAD_COLORS[(slot as number) % PAD_COLORS.length];
      arrive(slot as number, () => place(slot as number, stem, path as string, color));
    },
    swap_pads: ({ fromSlot, toSlot }) => {
      const [from, to] = [soundAt(fromSlot as number), soundAt(toSlot as number)];
      if (from) from.slot = toSlot as number;
      if (to) to.slot = fromSlot as number;
      publish();
    },
    send_to_interface: ({ slot, targetId }) => {
      const sound = soundAt(slot as number);
      const target = backend.interfaces.find((item) => item.id === targetId);
      if (!sound || !target) return;
      const taken = new Set(backend.sounds.filter((item) => item.interfaceId === targetId).map((item) => item.slot));
      const free = Array.from({ length: target.rows * target.columns }, (_, index) => index).find(
        (index) => !taken.has(index),
      );
      if (free === undefined) return fail({ kind: "text", key: "InterfaceFull", args: [plain(target.name)] });
      Object.assign(sound, { interfaceId: targetId, slot: free });
      publish();
      show({ kind: "text", key: "SentTo", args: [plain(target.name)] });
    },
    rename_pad: ({ slot, name }) =>
      edit(slot as number, (sound) => (sound.name = (name as string).trim() || sound.name)),
    set_pad_volume: ({ slot, volume }) =>
      edit(slot as number, (sound) => (sound.volume = (volume as number | null) ?? 1)),
    set_pad_color: ({ slot, color }) => edit(slot as number, (sound) => (sound.color = color as string)),
    set_pad_hotkey: ({ slot, hotkey }) => {
      backend.sounds.forEach(
        (sound) => sound.hotkey && hotkeyLabel(sound.hotkey) === hotkeyLabel(hotkey as Hotkey) && (sound.hotkey = null),
      );
      edit(slot as number, (sound) => (sound.hotkey = hotkey as Hotkey));
    },
    clear_pad_hotkey: ({ slot }) => edit(slot as number, (sound) => (sound.hotkey = null)),
    remove_pad: ({ slot }) => {
      backend.sounds = backend.sounds.filter((item) => item !== soundAt(slot as number));
      publish();
    },
    stop_all: () => {
      playing.clear();
      show({ kind: "text", key: "Stopped", args: [] });
    },
    search: ({ query, page }) =>
      new Promise((done) =>
        setTimeout(() => {
          const found = catalog.filter((item) => item.name.toLowerCase().includes((query as string).toLowerCase()));
          done(pageOf(found, page as number));
        }, SEARCH_MS),
      ),
    trending: ({ page }) => pageOf(catalog, page as number),
    preview_sound: ({ source }) =>
      show({ kind: "text", key: "Listening", args: [plain((source as SoundSource).name)] }),
    play_in_game: ({ source }) =>
      show({ kind: "text", key: "InGameSound", args: [plain((source as SoundSource).name)] }),
    add_to_pad: ({ source, slot }) => {
      const taken = new Set(
        backend.sounds.filter((item) => item.interfaceId === backend.activeId).map((item) => item.slot),
      );
      const free = Array.from({ length: active().rows * active().columns }, (_, index) => index).find(
        (index) => !taken.has(index),
      );
      const target = (slot as number | null) ?? backend.targetSlot ?? free;
      if (target === undefined) return fail({ kind: "text", key: "InterfaceFullHint", args: [] });
      const { name, path, color } = source as SoundSource;
      arrive(target, () => place(target, name, path, color));
    },
    toggle_favorite: ({ source }) => {
      const sound = source as SoundSource;
      const added = !isFavorite(sound.path);
      backend.favorites = added
        ? [...backend.favorites, sound]
        : backend.favorites.filter((item) => item.path.toLowerCase() !== sound.path.toLowerCase());
      send({ kind: "favorites", data: backend.favorites });
      publish();
      show(plain(`${added ? "★ " : ""}${sound.name}`));
    },
    save_to_computer: ({ destination }) =>
      show({ kind: "text", key: "Saved", args: [plain((destination as string).split("/").pop()!)] }),
    suggested_file_name: ({ source }) => `${(source as SoundSource).name}.mp3`,
    set_mic_volume: ({ volume }) => void (backend.mixer.micVolume = volume as number),
    set_game_volume: ({ volume }) => void (backend.mixer.gameVolume = volume as number),
    set_monitor_volume: ({ volume }) => void (backend.mixer.monitorVolume = volume as number),
    set_mic_enabled: ({ enabled }) => void (backend.mixer.micEnabled = enabled as boolean),
    load_devices: publishSetup,
    choose_microphone: ({ id }) => ((backend.setup.microphoneId = id as string | null), publishSetup()),
    choose_game_output: ({ id }) => ((backend.setup.gameOutputId = id as string | null), publishSetup()),
    choose_monitor_output: ({ id }) => ((backend.setup.monitorOutputId = id as string | null), publishSetup()),
    select_voice_effect: ({ effect }) => {
      backend.voice = { ...backend.voice, active: effect as VoiceEffect };
      send({ kind: "voice", data: backend.voice });
    },
    set_voice_param: ({ param, value }) => {
      const effects = backend.voice.effects.map((item) => ({
        ...item,
        params: item.params.map((entry) =>
          entry.param === param
            ? { ...entry, value: value as number, text: `${Math.round((value as number) * 100)}%` }
            : entry,
        ),
      }));
      backend.voice = { ...backend.voice, effects };
      send({ kind: "voice", data: backend.voice });
    },
    set_hearing_voice: ({ enabled }) => {
      backend.voice = { ...backend.voice, isHearing: enabled as boolean };
      send({ kind: "voice", data: backend.voice });
    },
    set_one_sound_at_a_time: ({ enabled }) => {
      backend.setup.oneSoundAtATime = enabled as boolean;
      publishSetup();
    },
    set_numeric_scheme: ({ scheme }) => {
      backend.setup.numericScheme = scheme as NumericScheme;
      backend.setup.numericHintKeys = NUMERIC_HINT_KEYS[scheme as NumericScheme];
      backend.setup.voiceKeys = VOICE_KEYS[scheme as NumericScheme];
      publishSetup();
      publish();
    },
    set_stop_hotkey: ({ hotkey }) => {
      backend.setup.stopHotkey = hotkey as Hotkey;
      publishSetup();
      publish();
    },
    set_language: ({ language }) => {
      backend.setup.language = language as string | null;
      publishSetup();
    },
    install_driver: () => {
      send({ kind: "driverStatus", data: { kind: "text", key: "DriverSearching", args: [] } });
      return new Promise((done) =>
        setTimeout(() => {
          backend.setup.hasCable = true;
          publishSetup();
          done(null);
        }, 800),
      );
    },
    // The dialog plugin: a confirmation always passes, and the pickers answer with a path.
    "plugin:dialog|message": () => "Yes",
    "plugin:dialog|open": () => "/mock/Music/Applause.mp3",
    "plugin:dialog|save": () => "/mock/Downloads/sound.mp3",
    "plugin:path|resolve_directory": () => "/mock/Music",
    "plugin:path|join": ({ paths }) => (paths as string[]).join("/"),
  };

  mockWindows("main");
  mockIPC((command, args) => handlers[command]?.((args ?? {}) as Args) ?? null, { shouldMockEvents: true });

  setInterval(() => {
    const now = Date.now() / 1000;
    const frame: MeterFrame = {
      micLevel: backend.mixer.micEnabled ? 0.3 + 0.25 * Math.abs(Math.sin(now * 3)) : 0,
      outputLevel: playing.size ? 0.55 + 0.35 * Math.abs(Math.sin(now * 5)) : 0,
      playingSlots: backend.sounds
        .filter((item) => item.interfaceId === backend.activeId && playing.has(item.id))
        .map((item) => item.slot),
      nowPlaying: playing.size
        ? { position: (Date.now() - startedAt) / 1000, duration: PLAY_MS / 1000, peaks: MOCK_PEAKS }
        : null,
    };
    void emit("meter-event", frame);
  }, METER_INTERVAL_MS);
}
