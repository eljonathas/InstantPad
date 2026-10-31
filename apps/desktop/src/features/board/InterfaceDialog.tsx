import { useState } from "react";
import type { InterfaceLayout } from "../../shared/ipc";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { Dialog, DialogLabel } from "../../shared/ui/Dialog";
import "./InterfaceDialog.css";

const upTo = (limit: number) => Array.from({ length: limit }, (_, index) => index + 1);

interface InterfaceDialogProps {
  title: string;
  initial: InterfaceLayout;
  soundCount: number;
  onDone: (layout: InterfaceLayout | null) => void;
}

/** Edits name, grid and color. A grid too small for the sounds cannot be saved. */
export function InterfaceDialog({ title, initial, soundCount, onDone }: InterfaceDialogProps) {
  const { t } = useText();
  const limits = useBackend((state) => state.limits);
  const [layout, setLayout] = useState(initial);
  const change = (patch: Partial<InterfaceLayout>) => setLayout({ ...layout, ...patch });

  const name = layout.name.trim();
  const capacity = layout.rows * layout.columns;
  const fits = capacity >= soundCount;

  return (
    <Dialog title={title} onCancel={() => onDone(null)}>
      <DialogLabel>{t("Name")}</DialogLabel>
      <input
        className="field interface-name"
        aria-label={t("Name")}
        value={layout.name}
        autoFocus
        onFocus={(event) => event.target.select()}
        onChange={(event) => change({ name: event.target.value })}
      />

      <div className="interface-grid">
        <label>
          <DialogLabel>{t("Rows")}</DialogLabel>
          <select
            className="field"
            value={layout.rows}
            onChange={(event) => change({ rows: Number(event.target.value) })}
          >
            {upTo(limits.maxRows).map((rows) => (
              <option key={rows}>{rows}</option>
            ))}
          </select>
        </label>
        <label>
          <DialogLabel>{t("Columns")}</DialogLabel>
          <select
            className="field"
            value={layout.columns}
            onChange={(event) => change({ columns: Number(event.target.value) })}
          >
            {upTo(limits.maxColumns).map((columns) => (
              <option key={columns}>{columns}</option>
            ))}
          </select>
        </label>
      </div>

      <p className={fits ? "interface-summary" : "interface-summary interface-summary-error"}>
        {fits ? t("PadCount", [String(capacity)]) : t("PadCountTooSmall", [String(capacity), String(soundCount)])}
      </p>

      <DialogLabel>{t("Color")}</DialogLabel>
      <div className="interface-swatches">
        {limits.accentColors.map((color) => (
          <button
            key={color}
            type="button"
            className="swatch"
            aria-label={color}
            aria-pressed={layout.accent === color}
            title={color}
            style={{ background: color }}
            onClick={() => change({ accent: color })}
          />
        ))}
      </div>

      <div className="dialog-buttons">
        <button type="button" className="key" onClick={() => onDone(null)}>
          {t("Cancel")}
        </button>
        <button
          type="button"
          className="key"
          disabled={!fits || name.length === 0}
          onClick={() => onDone({ ...layout, name })}
        >
          {t("Save")}
        </button>
      </div>
    </Dialog>
  );
}
