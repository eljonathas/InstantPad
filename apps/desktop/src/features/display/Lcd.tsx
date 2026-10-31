import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import "./Lcd.css";

/** The green screen at the top of the device: what just happened, the routing and a hint. */
export function Lcd() {
  const { render } = useText();
  const { main, status, hint } = useBackend((state) => state.display);

  return (
    <div className="lcd">
      <div className="lcd-main screen-text">{render(main)}</div>
      <div className="lcd-line screen-text">{status && render(status)}</div>
      <div className="lcd-line lcd-hint screen-text">{hint && render(hint)}</div>
    </div>
  );
}
