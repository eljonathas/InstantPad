import { getCurrentWindow } from "@tauri-apps/api/window";
import { useLayoutEffect, useRef, type MouseEvent } from "react";
import { InterfaceBar } from "../features/board/InterfaceBar";
import { PadGrid } from "../features/board/PadGrid";
import { useBoardActions } from "../features/board/useBoardActions";
import { Lcd } from "../features/display/Lcd";
import { MixerStrip } from "../features/mixer/MixerStrip";
import { useText } from "../shared/store/text";
import "./Device.css";
import { RightPanel } from "./RightPanel";

/** Scales the device to the window like a WPF Viewbox: the whole of it, never cropped or stretched. */
function useFitToWindow() {
  const box = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const fit = () => {
      const { offsetWidth, offsetHeight, style } = box.current!;
      style.setProperty("--fit", String(Math.min(window.innerWidth / offsetWidth, window.innerHeight / offsetHeight)));
    };
    fit();
    window.addEventListener("resize", fit);
    return () => window.removeEventListener("resize", fit);
  }, []);

  return box;
}

// Anything that is not a control or a screen drags the window: the chassis, the logo, the LCD.
function dragWindow(event: MouseEvent) {
  const target = (event.target as Element).closest("button, input, select, [data-nodrag], [data-drag]");
  if (event.button === 0 && target?.hasAttribute("data-drag")) void getCurrentWindow().startDragging();
}

export function Device() {
  const { t } = useText();
  const box = useFitToWindow();
  const actions = useBoardActions();

  return (
    <>
      <div ref={box} className="viewbox">
        <div className="device" data-drag onMouseDown={dragWindow}>
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
                <div className="logo-word logo-pad">Pad</div>
                <div className="silk logo-tagline">Sound sampler</div>
              </div>
              <MixerStrip />
            </div>

            <div className="centre">
              <Lcd />
              <InterfaceBar actions={actions} />
              <PadGrid onMenu={actions.openPadMenu} />
            </div>

            <RightPanel />
          </div>
        </div>
      </div>
      <div className="grip" onMouseDown={() => void getCurrentWindow().startResizeDragging("SouthEast")} />
    </>
  );
}
