// Stands in for the backend when the interface runs in a plain browser. It keeps just enough
// state to make every screen work; the real rules live in the Rust crates.
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type {
  AppEvent,
  BoardView,
  Calibration,
  CloneStep,
  CutEditorView,
  CutPiece,
  CutSource,
  EffectView,
  Hotkey,
  InterfaceLayout,
  Message,
  MeterFrame,
  NumericScheme,
  SetupView,
  SoundPage,
  SoundSource,
  StartView,
  TextKey,
  VoiceEffect,
  VoiceIcon,
  VoiceParam,
  VoiceRecipe,
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
  trendingRegions: ["br", "de", "fr", "pt", "us"],
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

const text = (key: TextKey): Message => ({ kind: "text", key, args: [] });
/** A limit the recording broke, with the number of seconds it allows. */
const limit = (key: TextKey, seconds: number): Message => ({
  kind: "text",
  key,
  args: [{ kind: "plain", text: String(seconds) }],
});

interface BuiltIn {
  effect: VoiceEffect;
  name: TextKey;
  icon: VoiceIcon;
  color: string;
  needsAi?: boolean;
  params: [VoiceParam, TextKey][];
}

const BUILT_IN: BuiltIn[] = [
  { effect: "natural", name: "EffectNatural", icon: "mic", color: PAD_COLORS[4], params: [] },
  {
    effect: "thin",
    name: "EffectThin",
    icon: "feather",
    color: PAD_COLORS[4],
    params: [
      ["thinIntensity", "ParamIntensity"],
      ["thinFormant", "ParamFormant"],
    ],
  },
  {
    effect: "deep",
    name: "EffectDeep",
    icon: "mountain",
    color: PAD_COLORS[6],
    params: [
      ["deepIntensity", "ParamIntensity"],
      ["deepFormant", "ParamFormant"],
    ],
  },
  {
    effect: "pitchCycle",
    name: "EffectCycle",
    icon: "wave",
    color: PAD_COLORS[3],
    params: [
      ["cycleTime", "ParamTime"],
      ["cycleHigh", "ParamHigh"],
      ["cycleLow", "ParamLow"],
    ],
  },
  {
    effect: "feminine",
    name: "EffectFeminine",
    icon: "flower",
    color: PAD_COLORS[7],
    params: [
      ["feminineIntensity", "ParamIntensity"],
      ["feminineFormant", "ParamFormant"],
    ],
  },
  {
    effect: "ghost",
    name: "EffectGhost",
    icon: "ghost",
    color: PAD_COLORS[5],
    params: [
      ["ghostWhisper", "ParamWhisper"],
      ["ghostEcho", "ParamEcho"],
      ["ghostPitch", "ParamPitch"],
    ],
  },
  {
    effect: "ogre",
    name: "EffectOgre",
    icon: "horns",
    color: PAD_COLORS[1],
    params: [
      ["ogreDepth", "ParamIntensity"],
      ["ogreGrowl", "ParamGrowl"],
      ["ogreFormant", "ParamFormant"],
    ],
  },
  {
    effect: "feminineAi",
    name: "EffectFeminineAi",
    icon: "sparkles",
    color: PAD_COLORS[7],
    needsAi: true,
    params: [["feminineAiPitch", "ParamPitch"]],
  },
];

type Waiting = { kind: "voice"; effect: VoiceEffect; cloneId: string | null } | { kind: "cloning" } | null;

const CLONE_PITCH: [VoiceParam, TextKey][] = [["clonePitch", "ParamPitch"]];
const AI_DOWNLOAD_MEGABYTES = 180;
const AI_STEP_MS = 60;
const AI_STEP_PERCENT = 20;
const MIN_CALIBRATION_SECONDS = 8;
const LISTEN_MS = 500;
const MIN_SAMPLE_SECONDS = 20;
const MAX_SAMPLE_SECONDS = 300;
const ANALYZE_MS = 400;

function seedVoice() {
  return {
    active: "natural" as VoiceEffect,
    activeCloneId: null as string | null,
    isHearing: false,
    /** Where each knob stands, by its param. A knob that was never turned is at its default. */
    values: new Map<VoiceParam, number>(),
    clones: [] as { id: string; name: string }[],
    designs: [] as { id: string; name: string; recipe: VoiceRecipe }[],
    // The AI as the backend reports it: nothing downloaded, and it does not know the user yet.
    ai: {
      isInstalled: false,
      percent: null as number | null,
      isCalibrated: false,
      calibration: "idle" as Calibration,
      /** How much of the talk the AI needs it has heard, from 0 to 100. */
      heard: 0,
    },
    /** What the user asked for that the AI setup holds back. */
    waiting: null as Waiting,
    /** The designer while it is open, with the hearing to give back when its preview ends. */
    designer: null as { isPreviewing: boolean; hearingBefore: boolean | null } | null,
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
  /** The piece of the file the pad plays, or null for the whole of it. */
  cut: { startMs: number; endMs: number } | null;
}

/** Every sound the mock cuts lasts this long: four sounds with pauses between them. */
const CUT_DURATION_MS = 8000;
const CUT_SOUNDS = [
  [0, 1500],
  [2100, 3600],
  [4300, 5600],
  [6000, 7600],
];

/** How loud the mock sound is at `ms`: a swell inside each of its four sounds, silence between. */
function cutLevel(ms: number) {
  const sound = CUT_SOUNDS.find(([start, end]) => ms >= start && ms < end);
  if (!sound) return 0;
  const [start, end] = sound;
  const inside = (ms - start) / (end - start);
  return Math.sin(Math.PI * inside) * (0.6 + 0.4 * Math.abs(Math.sin(ms / 37)));
}

/** Like the backend: "0:01.2-0:02.5". */
function cutLabel({ startMs, endMs }: { startMs: number; endMs: number }) {
  const at = (ms: number) => {
    const tenths = Math.floor(ms / 100);
    return `${Math.floor(tenths / 600)}:${String(Math.floor(tenths / 10) % 60).padStart(2, "0")}.${tenths % 10}`;
  };
  return `${at(startMs)}-${at(endMs)}`;
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

/** Far more trending sounds than the list shows at once, for the tests of its scrolling. */
const LONG_CATALOG = Array.from({ length: 100 }, (_, index) => source(`Sound ${index + 1}`, index));

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
    cut: null,
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
      trendingRegion: "us",
      chosenTrendingRegion: null,
      numericScheme: "numpad",
      numericHintKeys: NUMERIC_HINT_KEYS.numpad,
      voiceKeys: VOICE_KEYS.numpad,
      stopHotkey: null,
      oneSoundAtATime: true,
      pressAgainToStop: false,
      quietUpdates: false,
    } as SetupView,
    mixer: { micVolume: 1, gameVolume: 0.8, monitorVolume: 0.6, micEnabled: true },
    voice: seedVoice(),
  };
}

export function installMock() {
  let backend = seed();
  let playing = new Set<string>();
  // The latest start of each sound: a sound started over ends PLAY_MS after its last start.
  const latestStart = new Map<string, number>();
  let counter = 0;
  let startedAt = 0;
  let isFullscreen = false;
  /** The country the system's locale names, which the trending sounds follow by default. */
  let systemRegion = "us";
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
  /** The empty pads of the interface on screen, in order. */
  const freeSlots = () => {
    const taken = new Set(
      backend.sounds.filter((item) => item.interfaceId === backend.activeId).map((item) => item.slot),
    );
    return Array.from({ length: active().rows * active().columns }, (_, index) => index).filter(
      (index) => !taken.has(index),
    );
  };
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
                cut: sound.cut ? cutLabel(sound.cut) : null,
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

  function voiceView(): VoiceView {
    const { values, clones, designs, ai, waiting, designer } = backend.voice;
    const params = (list: [VoiceParam, TextKey][]) =>
      list.map(([param, label]) => {
        const value = values.get(param) ?? 0.5;
        return { param, label, value, defaultValue: 0.5, text: `${Math.round(value * 100)}%` };
      });
    const built: EffectView[] = BUILT_IN.map((item) => ({
      effect: item.effect,
      cloneId: null,
      name: text(item.name),
      icon: item.icon,
      color: item.color,
      needsAi: !!item.needsAi,
      params: params(item.params),
    }));
    const ownVoice = (voice: { id: string; name: string }, icon: VoiceIcon): EffectView => ({
      effect: "cloned",
      cloneId: voice.id,
      name: plain(voice.name),
      icon,
      color: PAD_COLORS[2],
      needsAi: true,
      params: params(CLONE_PITCH),
    });
    const cloned = [
      ...clones.map((clone) => ownVoice(clone, "person")),
      ...designs.map((design) => ownVoice(design, "blend")),
    ];
    const waitingName = !waiting
      ? null
      : waiting.kind === "cloning"
        ? text("CloneVoice")
        : [...built, ...cloned].find(
            (item) => item.effect === waiting.effect && item.cloneId === waiting.cloneId,
          )!.name;
    return {
      active: backend.voice.active,
      activeCloneId: backend.voice.activeCloneId,
      effects: [...built, ...cloned],
      clonedVoices: clones,
      designer: designer && { isPreviewing: designer.isPreviewing, defaultTimbre: 0.5, defaultBlend: 0.3 },
      isHearing: backend.voice.isHearing,
      ai: {
        status: ai.isInstalled
          ? { kind: "ready" }
          : ai.percent === null
            ? { kind: "missing" }
            : { kind: "downloading", percent: ai.percent },
        downloadMegabytes: AI_DOWNLOAD_MEGABYTES,
        calibration: ai.calibration,
        heardPercent: Math.floor(ai.heard),
        waiting: waitingName,
      },
    };
  }

  const publishVoice = () => send({ kind: "voice", data: voiceView() });

  // Like the backend, the AI listens to the user while an AI voice waits for it, and learns their
  // voice once they have talked long enough. Nothing is recorded by the user.
  let listening: ReturnType<typeof setInterval> | null = null;
  // The cloning dialog takes the microphone while it is open.
  let isCloningOpen = false;

  function stopListening() {
    const { ai } = backend.voice;
    if (listening) clearInterval(listening);
    listening = null;
    if (ai.calibration === "listening") {
      ai.calibration = "idle";
      ai.heard = 0;
    }
  }

  function startListening() {
    const { ai, waiting } = backend.voice;
    if (listening || isCloningOpen || ai.isCalibrated || !ai.isInstalled || waiting?.kind !== "voice") return;
    ai.calibration = "listening";
    ai.heard = 0;
    const step = 100 / ((MIN_CALIBRATION_SECONDS * 1000) / LISTEN_MS);
    listening = setInterval(() => {
      ai.heard = Math.min(100, ai.heard + step);
      if (ai.heard < 100) return publishVoice();
      clearInterval(listening!);
      listening = null;
      ai.calibration = "analyzing";
      publishVoice();
      setTimeout(() => {
        ai.calibration = "idle";
        ai.heard = 0;
        ai.isCalibrated = true;
        show(text("AiCalibrationSaved"));
        resume();
        publishVoice();
      }, ANALYZE_MS);
    }, LISTEN_MS);
    publishVoice();
  }

  /** Like the backend, an AI voice waits for the models and the user's own voice, then comes on. */
  function switchTo(effect: VoiceEffect, cloneId: string | null, needsAi: boolean) {
    const { ai } = backend.voice;
    const unready = !needsAi ? null : !ai.isInstalled ? "AiNotInstalled" : !ai.isCalibrated ? "AiNotCalibrated" : null;
    if (unready) {
      backend.voice.waiting = { kind: "voice", effect, cloneId };
      // Not knowing the user is no error: the AI starts learning their voice right now.
      if (unready === "AiNotCalibrated") show(text("AiListening"));
      else fail(text(unready));
      publishVoice();
      startListening();
    } else {
      stopListening();
      backend.voice = { ...backend.voice, active: effect, activeCloneId: cloneId, waiting: null };
      publishVoice();
    }
  }

  /** Takes what waited for the AI, once it can run. */
  function resume() {
    const { waiting } = backend.voice;
    backend.voice.waiting = null;
    if (waiting?.kind !== "voice") return;
    if (backend.voice.ai.isCalibrated) switchTo(waiting.effect, waiting.cloneId, true);
    else {
      backend.voice.waiting = waiting;
      startListening();
    }
  }

  // When each recording began, so Stop knows how long it was.
  const recordings = new Map<string, number>();
  const record = (name: string) => recordings.set(name, Date.now());
  /** How long `name` ran, which ends it. */
  function recordedSeconds(name: string) {
    const seconds = Math.floor((Date.now() - (recordings.get(name) ?? Date.now())) / 1000);
    recordings.delete(name);
    return seconds;
  }
  // Bumped when the dialog opens or closes, so a pending analysis knows it is out of date.
  let cloningGeneration = 0;
  const showCloning = (step: CloneStep) => send({ kind: "cloning", data: step });

  const sampleStep = (isRecording: boolean, problem: Message | null = null): CloneStep => ({
    step: "sample",
    isRecording,
    minSeconds: MIN_SAMPLE_SECONDS,
    maxSeconds: MAX_SAMPLE_SECONDS,
    problem,
  });

  /** The AI listens for a moment, then `next` says where the dialog goes. */
  function analyze(next: () => CloneStep) {
    const generation = cloningGeneration;
    showCloning({ step: "analyzing" });
    // A dialog closed meanwhile stays closed.
    setTimeout(() => generation === cloningGeneration && showCloning(next()), ANALYZE_MS);
  }

  function endPreview() {
    const { designer } = backend.voice;
    if (!designer) return;
    designer.isPreviewing = false;
    if (designer.hearingBefore !== null) backend.voice.isHearing = designer.hearingBefore;
    designer.hearingBefore = null;
    publishVoice();
  }

  function closeDesigner() {
    endPreview();
    backend.voice.designer = null;
    publishVoice();
  }

  function activate(id: string) {
    backend.activeId = id;
    backend.targetSlot = null;
  }

  function play(slot: number) {
    const sound = soundAt(slot);
    if (!sound) return;
    if (backend.setup.pressAgainToStop && playing.has(sound.id)) {
      playing.delete(sound.id);
      if (playing.size === 0) send({ kind: "display", data: { kind: "silent" } });
      return;
    }
    if (backend.setup.oneSoundAtATime) playing.clear();
    playing.add(sound.id);
    startedAt = Date.now();
    const start = startedAt;
    latestStart.set(sound.id, start);
    setTimeout(() => {
      if (latestStart.get(sound.id) !== start) return;
      playing.delete(sound.id);
      if (playing.size === 0) send({ kind: "display", data: { kind: "silent" } });
    }, PLAY_MS);
    send({ kind: "display", data: { kind: "sound", message: plain(`${label(slot)} ${sound.name}`) } });
  }

  function place(
    slot: number,
    name: string,
    path: string,
    color: string,
    cut: { startMs: number; endMs: number } | null = null,
  ) {
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
      cut,
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

  let failNextPage = false;
  /** The sound open in the cut editor. */
  let cutting: { name: string; path: string } | null = null;
  let trendingCatalog = catalog;

  const handlers: Record<string, (args: Args) => unknown> = {
    start: ({ systemLocale }) => {
      backend = seed();
      systemRegion = /[-_]([a-z]{2})(?:[-_.]|$)/i.exec(systemLocale as string)?.[1].toLowerCase() ?? "us";
      backend.setup.trendingRegion = systemRegion;
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
        voice: voiceView(),
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
    import_files: ({ paths, slot }) => {
      const files = paths as string[];
      const first = (slot as number | null) ?? backend.targetSlot ?? freeSlots()[0];
      const targets = first === undefined ? [] : [first, ...freeSlots().filter((free) => free !== first)];
      targets.slice(0, files.length).forEach((target, index) => {
        const stem = files[index]
          .split(/[/\\]/)
          .pop()!
          .replace(/\.[^.]+$/, "");
        const color = PAD_COLORS[target % PAD_COLORS.length];
        arrive(target, () => place(target, stem, files[index], color));
      });
      if (targets.length < files.length) return fail({ kind: "text", key: "InterfaceFullHint", args: [] });
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
    trending: ({ page }) => {
      if (failNextPage && (page as number) > 1) {
        failNextPage = false;
        return Promise.reject(text("CatalogNoConnection"));
      }
      return pageOf(trendingCatalog, page as number);
    },
    open_cut_editor: ({ source }) => {
      const from = source as CutSource;
      let opened: { name: string; path: string; color: string; cues: number[] };
      if (from.kind === "catalog") {
        opened = { name: from.source.name, path: from.source.path, color: from.source.color, cues: [0] };
      } else if (from.kind === "file") {
        const name = from.path.split(/[/\\]/).pop()!.replace(/\.[^.]+$/, "");
        opened = { name, path: from.path, color: PAD_COLORS[0], cues: [0] };
      } else {
        const sound = soundAt(from.slot);
        if (!sound) {
          fail(text("SoundFileMissing"));
          return Promise.reject(text("SoundFileMissing"));
        }
        const cues = sound.cut
          ? [sound.cut.startMs, ...(sound.cut.endMs < CUT_DURATION_MS ? [sound.cut.endMs] : [])]
          : [0];
        opened = { name: sound.name, path: sound.path, color: sound.color, cues };
      }
      cutting = { name: opened.name, path: opened.path };
      return {
        name: opened.name,
        color: opened.color,
        durationMs: CUT_DURATION_MS,
        cues: opened.cues,
        freePads: freeSlots().length,
      } satisfies CutEditorView;
    },
    cut_waveform: ({ startMs, endMs, bars }) => {
      if (!cutting) return Promise.reject(text("SoundFileMissing"));
      const [from, to, count] = [startMs as number, endMs as number, bars as number];
      const peaks = Array.from({ length: count }, (_, bar) => cutLevel(from + ((bar + 0.5) / count) * (to - from)));
      return { durationMs: CUT_DURATION_MS, peaks };
    },
    detect_cues: () => (cutting ? CUT_SOUNDS.map(([start]) => Math.max(0, start - 10)) : Promise.reject(text("SoundFileMissing"))),
    preview_cut: () => (cutting ? null : Promise.reject(text("SoundFileMissing"))),
    stop_cut_preview: () => null,
    place_cuts: ({ pieces }) => {
      if (!cutting) return Promise.reject(text("SoundFileMissing"));
      const { path } = cutting;
      const free = freeSlots();
      const wanted = pieces as CutPiece[];
      wanted.slice(0, free.length).forEach((piece, index) => {
        place(free[index], piece.name.trim() || "-", path, piece.color, { startMs: piece.startMs, endMs: piece.endMs });
      });
      const placed = Math.min(free.length, wanted.length);
      if (wanted.length > free.length) fail(text("InterfaceFullHint"));
      if (placed > 0) show({ kind: "text", key: "CutsPlaced", args: [plain(String(placed))] });
      cutting = null;
      return placed;
    },
    close_cut_editor: () => {
      cutting = null;
    },
    // Only in the browser: the trending sounds become a list long enough to scroll.
    mock_long_catalog: () => {
      trendingCatalog = LONG_CATALOG;
    },
    // Only in the browser: the next page of trending sounds fails once, as with no connection.
    mock_fail_next_page: () => {
      failNextPage = true;
    },
    preview_sound: ({ source }) =>
      show({ kind: "text", key: "Listening", args: [plain((source as SoundSource).name)] }),
    play_in_game: ({ source }) =>
      show({ kind: "text", key: "InGameSound", args: [plain((source as SoundSource).name)] }),
    add_to_pad: ({ source, slot }) => {
      const target = (slot as number | null) ?? backend.targetSlot ?? freeSlots()[0];
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
    select_voice_effect: ({ effect }) =>
      switchTo(effect as VoiceEffect, null, !!BUILT_IN.find((item) => item.effect === effect)?.needsAi),
    select_cloned_voice: ({ id }) => switchTo("cloned", id as string, true),
    set_voice_param: ({ param, value }) => {
      backend.voice.values.set(param as VoiceParam, value as number);
      publishVoice();
    },
    set_hearing_voice: ({ enabled }) => {
      // What the user chose is what comes back when a preview ends.
      const { designer } = backend.voice;
      if (designer?.hearingBefore != null) designer.hearingBefore = enabled as boolean;
      backend.voice.isHearing = enabled as boolean;
      publishVoice();
    },
    install_voice_ai: () =>
      new Promise((done) => {
        const { ai } = backend.voice;
        if (ai.isInstalled || ai.percent !== null) return done(null);
        ai.percent = 0;
        publishVoice();
        const timer = setInterval(() => {
          ai.percent = Math.min(100, (ai.percent ?? 0) + AI_STEP_PERCENT);
          if (ai.percent === 100) {
            clearInterval(timer);
            ai.percent = null;
            ai.isInstalled = true;
            show(text("AiInstalled"));
            resume();
            done(null);
          }
          publishVoice();
        }, AI_STEP_MS);
      }),
    relearn_voice: () => {
      const { ai, active, activeCloneId } = backend.voice;
      if ((active !== "feminineAi" && active !== "cloned") || !ai.isInstalled) return;
      stopListening();
      ai.isCalibrated = false;
      backend.voice = {
        ...backend.voice,
        active: "natural",
        activeCloneId: null,
        waiting: { kind: "voice", effect: active, cloneId: activeCloneId },
      };
      show(text("AiListening"));
      publishVoice();
      startListening();
    },
    delete_cloned_voice: ({ id }) => {
      // A cloned voice takes the voices designed from it along.
      const { clones, designs, activeCloneId } = backend.voice;
      const isMadeFrom = ({ recipe }: { recipe: VoiceRecipe }) => recipe.baseId === id || recipe.blendId === id;
      const gone = new Set([id, ...designs.filter(isMadeFrom).map((design) => design.id)]);
      backend.voice.clones = clones.filter((clone) => !gone.has(clone.id));
      backend.voice.designs = designs.filter((design) => !gone.has(design.id));
      const { waiting } = backend.voice;
      if (waiting?.kind === "voice" && waiting.cloneId && gone.has(waiting.cloneId)) {
        backend.voice.waiting = null;
        stopListening();
      }
      if (activeCloneId && gone.has(activeCloneId)) {
        backend.voice = { ...backend.voice, active: "natural", activeCloneId: null };
      }
      show(text("CloneDeleted"));
      publishVoice();
    },
    open_cloning: () => {
      if (!backend.voice.ai.isInstalled) {
        backend.voice.waiting = { kind: "cloning" };
        fail(text("AiNotInstalled"));
        return publishVoice();
      }
      cloningGeneration++;
      isCloningOpen = true;
      stopListening();
      publishVoice();
      showCloning(sampleStep(false));
    },
    start_clone_sample: () => {
      record("sample");
      showCloning(sampleStep(true));
    },
    finish_clone_sample: () => {
      const seconds = recordedSeconds("sample");
      analyze(() =>
        seconds < MIN_SAMPLE_SECONDS
          ? sampleStep(false, limit("SampleTooShort", MIN_SAMPLE_SECONDS))
          : seconds > MAX_SAMPLE_SECONDS
            ? sampleStep(false, limit("SampleTooLong", MAX_SAMPLE_SECONDS))
            : { step: "name" },
      );
    },
    // A file is as long as a good sample needs, whatever it holds.
    import_clone_sample: () => analyze(() => ({ step: "name" })),
    save_cloned_voice: ({ name }) => {
      backend.voice.clones.push({ id: `clone-${++counter}`, name: name as string });
      isCloningOpen = false;
      showCloning({ step: "closed" });
      show(text("CloneSaved"));
      publishVoice();
      startListening();
    },
    open_designer: () => {
      if (backend.voice.clones.length === 0) return;
      backend.voice.designer = { isPreviewing: false, hearingBefore: null };
      publishVoice();
    },
    preview_designed_voice: () => {
      const { ai, designer } = backend.voice;
      if (!ai.isInstalled) return fail(text("AiNotInstalled"));
      if (!ai.isCalibrated) return fail(text("AiNotCalibrated"));
      if (!designer) return;
      designer.isPreviewing = true;
      designer.hearingBefore ??= backend.voice.isHearing;
      backend.voice.isHearing = true;
      publishVoice();
    },
    end_voice_preview: endPreview,
    close_designer: closeDesigner,
    save_designed_voice: ({ name, recipe }) => {
      if (!backend.voice.designer) return;
      // The designer stays open when the save fails.
      if (!(name as string).trim()) return fail(text("CloneMissingName"));
      const id = `design-${++counter}`;
      backend.voice.designs.push({ id, name: name as string, recipe: recipe as VoiceRecipe });
      closeDesigner();
      show(text("CloneSaved"));
      switchTo("cloned", id, true);
    },
    close_cloning: () => {
      cloningGeneration++;
      recordings.delete("sample");
      isCloningOpen = false;
      showCloning({ step: "closed" });
      startListening();
    },
    set_one_sound_at_a_time: ({ enabled }) => {
      backend.setup.oneSoundAtATime = enabled as boolean;
      publishSetup();
    },
    set_quiet_updates: ({ enabled }) => {
      backend.setup.quietUpdates = enabled as boolean;
      publishSetup();
    },
    // Only in the browser: stands in for a version that finished downloading, for the tests.
    mock_update_ready: ({ version }) => {
      send({
        kind: "update",
        data: {
          status: { kind: "text", key: "VersionInstalled", args: [plain(version as string)] },
          isReadyToRestart: true,
        },
      });
    },
    set_press_again_to_stop: ({ enabled }) => {
      backend.setup.pressAgainToStop = enabled as boolean;
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
    set_trending_region: ({ region }) => {
      backend.setup.chosenTrendingRegion = region as string | null;
      backend.setup.trendingRegion = (region as string | null) ?? systemRegion;
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
    "plugin:dialog|open": ({ options }) =>
      (options as { multiple?: boolean }).multiple
        ? ["/mock/Music/Applause.mp3", "/mock/Music/Drum roll.wav"]
        : "/mock/Music/Applause.mp3",
    "plugin:dialog|save": () => "/mock/Downloads/sound.mp3",
    "plugin:path|resolve_directory": () => "/mock/Music",
    "plugin:path|join": ({ paths }) => (paths as string[]).join("/"),
    "plugin:window|is_fullscreen": () => isFullscreen,
    "plugin:window|set_fullscreen": ({ value }) => {
      isFullscreen = value as boolean;
      window.dispatchEvent(new Event("resize"));
    },
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
