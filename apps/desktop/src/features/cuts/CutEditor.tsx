import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { commands, type CutEditorView } from "../../shared/ipc";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { Dialog } from "../../shared/ui/Dialog";
import { cutEditor, useCutEditor } from "./cutEditorStore";
import { clock, msToX, pan, pieceAt, piecesOf, xToMs, zoom, type View } from "./cutMath";
import "./CutEditor.css";

/** How close to a cue, in pixels, a press grabs it instead of moving the playhead. */
const GRAB_PX = 7;
/** How much one notch of the wheel or one press of a zoom key zooms. */
const ZOOM_STEP = 0.75;
/** How far the arrow keys move a cue, and with Shift. */
const NUDGE_MS = 10;
const NUDGE_FAR_MS = 100;
/** The waveform is asked for again this long after the view stops changing. */
const REDRAW_AFTER_MS = 40;

interface Cue {
  id: number;
  ms: number;
}

interface Playing {
  fromMs: number;
  toMs: number;
  startedAt: number;
}

/** A color of the theme, read from the stylesheet so the canvas follows it. */
function themeColor(element: Element, name: string) {
  return getComputedStyle(element).getPropertyValue(name).trim();
}

/** The cut editor, while a sound is open in it. */
export function CutEditor() {
  const state = useCutEditor();
  if (state.status !== "open") return null;
  return <Editor key={state.id} opened={state.view} />;
}

function Editor({ opened }: { opened: CutEditorView }) {
  const { t } = useText();
  const colors = useBackend((state) => state.limits.padColors);
  const { durationMs } = opened;

  const nextId = useRef(0);
  const [cues, setCues] = useState<Cue[]>(() => opened.cues.map((ms) => ({ id: nextId.current++, ms })));
  const [selected, setSelected] = useState<number | null>(null);
  const [playhead, setPlayhead] = useState(0);
  const [view, setView] = useState<View>({ startMs: 0, endMs: durationMs });
  const [names, setNames] = useState<Record<number, string>>({});
  const [left, setLeft] = useState<Set<number>>(new Set());
  const [playing, setPlaying] = useState<Playing | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [isPlacing, setIsPlacing] = useState(false);

  const pieces = useMemo(() => piecesOf(cues.map((cue) => cue.ms), durationMs), [cues, durationMs]);
  const nameOf = (index: number) => names[index] ?? `${opened.name} ${index + 1}`;
  const colorOf = (index: number) => (colors.length > 0 ? colors[index % colors.length] : opened.color);
  const chosen = pieces.map((_, index) => index).filter((index) => !left.has(index));
  const tooMany = chosen.length > opened.freePads;

  // ---- Hearing ----

  const stop = useCallback(() => {
    void commands.stopCutPreview();
    setPlaying(null);
  }, []);

  const play = useCallback(async (fromMs: number, toMs: number) => {
    const answer = await commands.previewCut(fromMs, toMs);
    if (answer.status === "ok") setPlaying({ fromMs, toMs, startedAt: performance.now() });
  }, []);

  // The playhead follows what plays, and the playing stops at the end of the piece.
  useEffect(() => {
    if (!playing) return;
    let frame = requestAnimationFrame(function step() {
      const at = playing.fromMs + (performance.now() - playing.startedAt);
      if (at >= playing.toMs) {
        setPlayhead(playing.toMs);
        setPlaying(null);
        return;
      }
      setPlayhead(at);
      frame = requestAnimationFrame(step);
    });
    return () => cancelAnimationFrame(frame);
  }, [playing]);

  // Nothing keeps playing once the editor is gone.
  useEffect(() => () => void commands.stopCutPreview(), []);

  function playPiece(index: number) {
    const piece = pieces[index];
    if (!piece) return;
    setPlayhead(piece.startMs);
    void play(piece.startMs, piece.endMs);
  }

  function togglePlay() {
    if (playing) return stop();
    void play(playhead >= durationMs ? 0 : playhead, durationMs);
  }

  // ---- Cues ----

  function addCue(ms: number) {
    const id = nextId.current++;
    setCues((before) => [...before, { id, ms: Math.round(ms) }]);
    setSelected(id);
    setNotice(null);
  }

  function removeSelected() {
    if (selected === null) return;
    setCues((before) => before.filter((cue) => cue.id !== selected));
    setSelected(null);
  }

  function moveCue(id: number, ms: number) {
    const kept = Math.min(durationMs, Math.max(0, Math.round(ms)));
    setCues((before) => before.map((cue) => (cue.id === id ? { ...cue, ms: kept } : cue)));
  }

  async function detect() {
    const answer = await commands.detectCues();
    if (answer.status !== "ok") return;
    if (answer.data.length === 0) return setNotice(t("CutDetectNone"));
    setCues(answer.data.map((ms) => ({ id: nextId.current++, ms })));
    setSelected(null);
    setNames({});
    setLeft(new Set());
    setNotice(null);
  }

  // ---- The waveform ----

  const canvas = useRef<HTMLCanvasElement>(null);
  const [width, setWidth] = useState(0);
  const [drawn, setDrawn] = useState<{ view: View; peaks: number[] } | null>(null);
  const dragging = useRef<number | null>(null);

  useEffect(() => {
    const element = canvas.current?.parentElement;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => setWidth(Math.floor(entry.contentRect.width)));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  // The bars of the part on screen, one per pixel, asked again when the view settles.
  useEffect(() => {
    if (width === 0) return;
    let current = true;
    const timer = setTimeout(async () => {
      const answer = await commands.cutWaveform(Math.floor(view.startMs), Math.ceil(view.endMs), width);
      if (current && answer.status === "ok") setDrawn({ view, peaks: answer.data.peaks });
    }, REDRAW_AFTER_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [view, width]);

  useEffect(() => {
    const element = canvas.current;
    if (!element || width === 0) return;
    const height = element.clientHeight;
    const ratio = window.devicePixelRatio || 1;
    element.width = width * ratio;
    element.height = height * ratio;
    const context = element.getContext("2d");
    if (!context) return;
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, width, height);

    const lit = themeColor(element, "--color-lcd");
    const dim = themeColor(element, "--color-lcd-dim");
    const white = themeColor(element, "--color-white");
    const x = (ms: number) => msToX(ms, view, width);

    // Each piece tinted with the color its pad gets.
    pieces.forEach((piece, index) => {
      context.fillStyle = colorOf(index);
      context.globalAlpha = left.has(index) ? 0.05 : 0.2;
      context.fillRect(x(piece.startMs), 0, x(piece.endMs) - x(piece.startMs), height);
    });
    context.globalAlpha = 1;

    // The bars, lit inside a piece that becomes a pad.
    if (drawn) {
      const middle = height / 2;
      drawn.peaks.forEach((peak, bar) => {
        const ms = drawn.view.startMs + ((bar + 0.5) / drawn.peaks.length) * (drawn.view.endMs - drawn.view.startMs);
        const index = pieceAt(pieces, ms);
        context.fillStyle = index >= 0 && !left.has(index) ? lit : dim;
        const half = Math.max(0.5, peak * (middle - 4));
        context.fillRect(x(ms) - 0.5, middle - half, 1, half * 2);
      });
    }

    // The cues, numbered like the hot cues of a player, in the color of their piece.
    const sorted = [...cues].sort((a, b) => a.ms - b.ms);
    sorted.forEach((cue) => {
      const index = pieces.findIndex((piece) => piece.startMs === cue.ms);
      const at = Math.round(x(cue.ms)) + 0.5;
      context.fillStyle = index >= 0 ? colorOf(index) : dim;
      context.fillRect(at - (cue.id === selected ? 1 : 0.5), 0, cue.id === selected ? 2 : 1, height);
      context.fillRect(at - 0.5, 0, 16, 14);
      context.fillStyle = "#000";
      context.font = "bold 10px sans-serif";
      context.fillText(index >= 0 ? String(index + 1) : "·", at + 3, 11);
      if (cue.id === selected) {
        context.strokeStyle = white;
        context.lineWidth = 1;
        context.strokeRect(at - 0.5, 0.5, 16, 13);
      }
    });

    // The playhead.
    context.fillStyle = white;
    context.fillRect(Math.round(x(playhead)), 0, 1, height);
  });

  function msUnder(event: { clientX: number }) {
    const box = canvas.current!.getBoundingClientRect();
    return xToMs(event.clientX - box.left, view, width, durationMs);
  }

  function cueNear(event: { clientX: number }) {
    const box = canvas.current!.getBoundingClientRect();
    const pointer = event.clientX - box.left;
    const near = cues
      .map((cue) => ({ cue, distance: Math.abs(msToX(cue.ms, view, width) - pointer) }))
      .filter(({ distance }) => distance <= GRAB_PX)
      .sort((a, b) => a.distance - b.distance);
    return near[0]?.cue ?? null;
  }

  function onPointerDown(event: ReactPointerEvent<HTMLCanvasElement>) {
    if (event.button !== 0) return;
    const cue = cueNear(event);
    if (cue) {
      setSelected(cue.id);
      dragging.current = cue.id;
      event.currentTarget.setPointerCapture(event.pointerId);
      return;
    }
    setSelected(null);
    if (playing) stop();
    setPlayhead(msUnder(event));
  }

  function onPointerMove(event: ReactPointerEvent<HTMLCanvasElement>) {
    if (dragging.current !== null) moveCue(dragging.current, msUnder(event));
  }

  function onPointerUp() {
    dragging.current = null;
  }

  // The wheel zooms around the pointer, and pans when it moves sideways or with Shift.
  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const box = element.getBoundingClientRect();
      const length = view.endMs - view.startMs;
      if (event.shiftKey || Math.abs(event.deltaX) > Math.abs(event.deltaY)) {
        const delta = (event.shiftKey ? event.deltaY : event.deltaX) / box.width;
        setView(pan(view, delta * length, durationMs));
        return;
      }
      const anchor = xToMs(event.clientX - box.left, view, box.width, durationMs);
      setView(zoom(view, event.deltaY > 0 ? 1 / ZOOM_STEP : ZOOM_STEP, anchor, durationMs));
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  }, [view, durationMs]);

  // ---- Keys ----

  const keys = useRef<(event: KeyboardEvent) => void>(() => {});
  keys.current = (event: KeyboardEvent) => {
    const target = event.target as HTMLElement | null;
    if (target?.closest("input, select, textarea") || event.ctrlKey || event.metaKey || event.altKey) return;
    if (event.code === "Space") {
      event.preventDefault();
      togglePlay();
    } else if (event.code === "KeyC") {
      addCue(playhead);
    } else if (event.key === "Delete" || event.key === "Backspace") {
      removeSelected();
    } else if (/^Digit[1-9]$/.test(event.code)) {
      playPiece(Number(event.code.slice(5)) - 1);
    } else if ((event.key === "ArrowLeft" || event.key === "ArrowRight") && selected !== null) {
      event.preventDefault();
      const step = (event.shiftKey ? NUDGE_FAR_MS : NUDGE_MS) * (event.key === "ArrowLeft" ? -1 : 1);
      const cue = cues.find((item) => item.id === selected);
      if (cue) moveCue(cue.id, cue.ms + step);
    }
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => keys.current(event);
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // ---- Placing ----

  async function create() {
    setIsPlacing(true);
    stop();
    const wanted = chosen.map((index) => ({
      name: nameOf(index),
      color: colorOf(index),
      startMs: pieces[index].startMs,
      endMs: pieces[index].endMs,
    }));
    const answer = await commands.placeCuts(wanted);
    setIsPlacing(false);
    if (answer.status === "ok") cutEditor.closed();
  }

  const zoomBy = (factor: number) => setView(zoom(view, factor, playhead, durationMs));
  const createLabel = chosen.length === 1 ? t("CutCreateOne") : t("CutCreate", [String(chosen.length)]);

  return (
    <Dialog title={t("CutIntoPadsTitle", [opened.name])} isLarge onCancel={cutEditor.cancel}>
      <div className="cut-screen">
        <canvas
          ref={canvas}
          className="cut-wave"
          role="img"
          aria-label={t("CutWaveform")}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
          onDoubleClick={(event) => addCue(msUnder(event))}
        />
        <div className="cut-ruler screen-text">
          <span>{clock(view.startMs)}</span>
          <span className="cut-time" aria-label={t("CutPlayhead")}>
            {clock(playhead)} / {clock(durationMs)}
          </span>
          <span>{clock(view.endMs)}</span>
        </div>
      </div>

      <div className="cut-tools">
        <button type="button" className="key key-accent cut-play" onClick={togglePlay}>
          {playing ? t("CutStop") : t("CutPlay")}
        </button>
        <button type="button" className="key" onClick={() => addCue(playhead)}>
          {t("CutAddCue")}
        </button>
        <button type="button" className="key" disabled={selected === null} onClick={removeSelected}>
          {t("CutRemoveCue")}
        </button>
        <button type="button" className="key" onClick={() => void detect()}>
          {t("CutDetect")}
        </button>
        <span className="cut-gap" />
        <button type="button" className="key cut-zoom" aria-label={t("CutZoomOut")} title={t("CutZoomOut")} onClick={() => zoomBy(1 / ZOOM_STEP)}>
          −
        </button>
        <button type="button" className="key cut-zoom" aria-label={t("CutZoomIn")} title={t("CutZoomIn")} onClick={() => zoomBy(ZOOM_STEP)}>
          +
        </button>
        <button type="button" className="key" onClick={() => setView({ startMs: 0, endMs: durationMs })}>
          {t("CutFit")}
        </button>
      </div>
      {notice && <p className="cut-notice">{notice}</p>}

      <h3 className="cut-heading">{t("CutPieces", [String(pieces.length)])}</h3>
      {pieces.length === 0 ? (
        <p className="dialog-tip">{t("CutNoPieces")}</p>
      ) : (
        <ol className="cut-pieces">
          {pieces.map((piece, index) => (
            <li key={`${index}-${piece.startMs}`} className="cut-piece" data-left={left.has(index) ? "" : undefined}>
              <span className="cut-swatch" style={{ background: colorOf(index) }}>
                {index + 1}
              </span>
              <input
                className="field cut-name"
                aria-label={t("CutPieceName", [String(index + 1)])}
                value={nameOf(index)}
                onChange={(event) => setNames((before) => ({ ...before, [index]: event.target.value }))}
              />
              <span className="cut-range screen-text">
                {clock(piece.startMs)}–{clock(piece.endMs)}
              </span>
              <button
                type="button"
                className="key cut-piece-play"
                aria-label={t("CutPiecePlay", [String(index + 1)])}
                title={t("CutPiecePlay", [String(index + 1)])}
                onClick={() => playPiece(index)}
              >
                ▶
              </button>
              <input
                type="checkbox"
                className="cut-include"
                aria-label={t("CutPieceInclude", [String(index + 1)])}
                checked={!left.has(index)}
                onChange={() =>
                  setLeft((before) => {
                    const next = new Set(before);
                    if (next.has(index)) next.delete(index);
                    else next.add(index);
                    return next;
                  })
                }
              />
            </li>
          ))}
        </ol>
      )}

      <p className="dialog-tip">{t("CutHelp")}</p>
      {tooMany && <p className="cut-notice">{t("CutTooMany", [String(opened.freePads)])}</p>}
      <div className="dialog-buttons">
        <button type="button" className="key" onClick={cutEditor.cancel}>
          {t("Cancel")}
        </button>
        <button
          type="button"
          className="key key-accent cut-create"
          disabled={chosen.length === 0 || tooMany || isPlacing}
          onClick={() => void create()}
        >
          {createLabel}
        </button>
      </div>
    </Dialog>
  );
}
