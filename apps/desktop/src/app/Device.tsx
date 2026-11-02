import { getCurrentWindow } from "@tauri-apps/api/window";
import { useLayoutEffect, useRef, useState, type MouseEvent } from "react";
import { InterfaceBar } from "../features/board/InterfaceBar";
import { PadGrid } from "../features/board/PadGrid";
import { useBoardActions } from "../features/board/useBoardActions";
import { Lcd } from "../features/display/Lcd";
import { MixerStrip } from "../features/mixer/MixerStrip";
import { EffectList } from "../features/voice/EffectList";
import { VoiceControls } from "../features/voice/VoiceControls";
import { useBackend } from "../shared/store/backend";
import { useText } from "../shared/store/text";
import { RollingText } from "../shared/ui/RollingText";
import { ToggleKey } from "../shared/ui/ToggleKey";
import "./Device.css";
import { RightPanel } from "./RightPanel";
import { useFullscreen } from "./useFullscreen";

/**
 * Scales the device to the window like a WPF Viewbox: the whole of it, never cropped or stretched.
 * In full screen it then grows to the shape of the screen, so no desktop shows around it.
 */
function useFitToWindow(isFullscreen: boolean) {
  const box = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const fit = () => {
      const { style } = box.current!;
      style.removeProperty("--device-width");
      style.removeProperty("--device-height");
      const { offsetWidth, offsetHeight } = box.current!;
      const scale = Math.min(window.innerWidth / offsetWidth, window.innerHeight / offsetHeight);
      style.setProperty("--fit", String(scale));
      if (!isFullscreen) return;
      style.setProperty("--device-width", `${window.innerWidth / scale}px`);
      style.setProperty("--device-height", `${window.innerHeight / scale}px`);
    };
    fit();
    window.addEventListener("resize", fit);
    return () => window.removeEventListener("resize", fit);
  }, [isFullscreen]);

  return box;
}

// Anything that is not a control or a screen drags the window: the chassis, the logo, the LCD.
function dragWindow(event: MouseEvent) {
  const target = (event.target as Element).closest("button, input, select, [data-nodrag], [data-drag]");
  if (event.button === 0 && target?.hasAttribute("data-drag")) void getCurrentWindow().startDragging();
}

export function Device() {
  const [screen, setScreen] = useState<"pad" | "voice">("pad");
  const isVoice = screen === "voice";
  const isChangingVoice = useBackend((state) => state.voice.active !== "natural");

  const { t } = useText();
  const { isFullscreen, toggle: toggleFullscreen } = useFullscreen();
  const box = useFitToWindow(isFullscreen);
  const actions = useBoardActions();

  return (
    <>
      <div ref={box} className="viewbox" data-fullscreen={isFullscreen ? "" : undefined}>
        <div className="device" data-screen={screen} data-drag onMouseDown={isFullscreen ? undefined : dragWindow}>
          <div className="chassis" />
          <i className="screw screw-top-left" />
          <i className="screw screw-bottom-left" />
          <i className="screw screw-bottom-right" />

          <div className="power-keys">
            <button
              className="key key-bare power-key"
              title={t("Minimize")}
              onClick={() => void getCurrentWindow().minimize()}
            >
              –
            </button>
            <button
              className="key key-bare power-key"
              title={t(isFullscreen ? "ExitFullscreen" : "Fullscreen")}
              onClick={toggleFullscreen}
            >
              <i className="icon icon-fullscreen" />
            </button>
            <button
              className="key key-bare key-danger power-key"
              title={t("Close")}
              onClick={() => void getCurrentWindow().close()}
            >
              ✕
            </button>
          </div>

          <div className="layout">
            <div className="strip">
              <div className="logo">
                <div className="logo-word">Instant</div>
                <RollingText className="logo-word logo-pad" text={isVoice ? "Voice" : "Pad"} />
                <RollingText className="silk logo-tagline" text={isVoice ? "Voice changer" : "Sound sampler"} />
              </div>
              <div className="mode-keys" role="tablist">
                <ToggleKey kind="tab" label="Pad" on={!isVoice} onClick={() => setScreen("pad")} />
                <ToggleKey
                  kind="tab"
                  className={isChangingVoice ? "mode-voice-on" : undefined}
                  label="Voice"
                  on={isVoice}
                  onClick={() => setScreen("voice")}
                />
              </div>
              <MixerStrip />
            </div>

            <div className="centre">
              <Lcd />
              {isVoice ? (
                <EffectList />
              ) : (
                <>
                  <InterfaceBar actions={actions} />
                  <PadGrid onMenu={actions.openPadMenu} />
                </>
              )}
            </div>

            <div className="side">
              <div className="side-page" hidden={isVoice}>
                <RightPanel />
              </div>
              {isVoice && (
                <div className="panel-screen side-page">
                  <VoiceControls />
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
      {!isFullscreen && (
        <div className="grip" onMouseDown={() => void getCurrentWindow().startResizeDragging("SouthEast")} />
      )}
    </>
  );
}
