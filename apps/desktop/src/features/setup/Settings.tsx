import { useMemo, useState, type ReactNode } from "react";
import { regionName } from "../../shared/i18n";
import { commands, type AudioDevice, type NumericScheme } from "../../shared/ipc";
import { useBackend } from "../../shared/store/backend";
import { useText } from "../../shared/store/text";
import { useAskHotkey } from "../../shared/ui/HotkeyDialog";
import { Switch } from "../../shared/ui/Switch";
import { describeNumericKeys } from "./numericHint";
import "./Settings.css";

const SCHEMES: {
  scheme: NumericScheme;
  text: "NumericNumpad" | "NumericCtrlNumpad" | "NumericAltDigits" | "NumericOff";
}[] = [
  { scheme: "numpad", text: "NumericNumpad" },
  { scheme: "ctrlNumpad", text: "NumericCtrlNumpad" },
  { scheme: "altDigits", text: "NumericAltDigits" },
  { scheme: "off", text: "NumericOff" },
];

interface DeviceSelectProps {
  title: string;
  devices: AudioDevice[];
  selected: string | null;
  onChoose: (id: string | null) => Promise<unknown>;
}

/** Keeps showing what the user picked while the device opens, which a slow driver can take seconds to do. */
export function DeviceSelect({ title, devices, selected, onChoose }: DeviceSelectProps) {
  const [picked, setPicked] = useState<{ id: string | null } | null>(null);
  const shown = picked ? picked.id : selected;

  async function choose(id: string | null) {
    const choice = { id };
    setPicked(choice);
    try {
      await onChoose(id);
    } finally {
      // A newer pick is still opening and stays on screen.
      setPicked((current) => (current === choice ? null : current));
    }
  }

  return (
    <label className="setting">
      <span className="setting-title screen-text">{title}</span>
      <select className="field" value={shown ?? ""} onChange={(event) => void choose(event.target.value || null)}>
        {!devices.some((device) => device.id === shown) && <option value="" />}
        {devices.map((device) => (
          <option key={device.id} value={device.id}>
            {device.name}
          </option>
        ))}
      </select>
    </label>
  );
}

/** Language, trending country, driver, audio routing and numeric keys. The app adds the update status at the end. */
export function Settings({ children }: { children: ReactNode }) {
  const { t, render, language } = useText();
  const askHotkey = useAskHotkey();
  const setup = useBackend((state) => state.setup);
  const trendingRegions = useBackend((state) => state.limits.trendingRegions);
  const regions = useMemo(
    () =>
      trendingRegions
        .map((code) => ({ code, name: regionName(language, code) }))
        .sort((a, b) => a.name.localeCompare(b.name, language)),
    [trendingRegions, language],
  );
  const driverStatus = useBackend((state) => state.driverStatus);
  const stopShortcut = useBackend((state) => state.board.stopShortcut);
  const [installing, setInstalling] = useState(false);

  async function install() {
    setInstalling(true);
    try {
      await commands.installDriver();
    } finally {
      setInstalling(false);
    }
  }

  async function chooseStopHotkey() {
    const hotkey = await askHotkey();
    if (hotkey) await commands.setStopHotkey(hotkey);
  }

  return (
    <section className="settings" aria-label={t("SettingsTab")}>
      <label className="setting">
        <span className="setting-title screen-text">{t("Language")}</span>
        <select
          className="field"
          value={setup.language ?? ""}
          onChange={(event) => commands.setLanguage(event.target.value || null)}
        >
          <option value="">{t("LanguageSystem")}</option>
          <option value="pt">Português</option>
          <option value="en">English</option>
        </select>
      </label>

      <label className="setting">
        <span className="setting-title screen-text">{t("TrendingRegion")}</span>
        <select
          className="field"
          value={setup.chosenTrendingRegion ?? ""}
          onChange={(event) => commands.setTrendingRegion(event.target.value || null)}
        >
          <option value="">{t("TrendingRegionSystem")}</option>
          {regions.map(({ code, name }) => (
            <option key={code} value={code}>
              {name}
            </option>
          ))}
        </select>
      </label>

      <h3 className="setting-title screen-text">{t("VirtualDriver")}</h3>
      <p className="screen-text setting-note" role="status">
        {driverStatus ? render(driverStatus) : t(setup.hasCable ? "DriverFound" : "DriverMissing")}
      </p>
      {setup.isSystemOutputOnCable && (
        <>
          <p className="screen-text setting-warning">{t("SystemOutputOnCable")}</p>
          <button className="key setting-key" onClick={() => commands.openSoundSettings()}>
            {t("OpenSoundSettings")}
          </button>
        </>
      )}
      {!setup.hasCable && (
        <button className="key key-amber setting-key" disabled={installing} onClick={() => void install()}>
          {t("InstallCable", [setup.cableName])}
        </button>
      )}

      <DeviceSelect
        title={t("SetupMicrophone")}
        devices={setup.microphones}
        selected={setup.microphoneId}
        onChoose={commands.chooseMicrophone}
      />
      <DeviceSelect
        title={t("SetupGameOutput")}
        devices={setup.gameOutputs}
        selected={setup.gameOutputId}
        onChoose={commands.chooseGameOutput}
      />
      <DeviceSelect
        title={t("SetupMonitor")}
        devices={setup.monitorOutputs}
        selected={setup.monitorOutputId}
        onChoose={commands.chooseMonitorOutput}
      />
      <button className="key setting-key setting-refresh" onClick={() => commands.loadDevices()}>
        {t("RefreshDevices")}
      </button>

      <h3 className="setting-title screen-text">{t("InGame")}</h3>
      <p className="screen-text setting-instruction">{t("GameInstruction", [setup.gameMicrophoneName])}</p>
      <p className="screen-text setting-instruction setting-spaced">{t("DiscordInstruction")}</p>

      <label className="setting">
        <span className="setting-title screen-text">{t("NumericShortcuts")}</span>
        <select
          className="field"
          value={setup.numericScheme}
          onChange={(event) => commands.setNumericScheme(event.target.value as NumericScheme)}
        >
          {SCHEMES.map(({ scheme, text }) => (
            <option key={scheme} value={scheme}>
              {t(text)}
            </option>
          ))}
        </select>
      </label>
      <p className="screen-text setting-note setting-spaced">
        {describeNumericKeys(setup.numericScheme, setup.numericHintKeys, t)}
      </p>

      <h3 className="setting-title screen-text">{t("Playback")}</h3>
      <Switch
        label={t("OneSoundAtATime")}
        hint={t("OneSoundAtATimeHint")}
        on={setup.oneSoundAtATime}
        onChange={(on) => void commands.setOneSoundAtATime(on)}
      />
      <Switch
        label={t("PressAgainToStop")}
        hint={t("PressAgainToStopHint")}
        on={setup.pressAgainToStop}
        onChange={(on) => void commands.setPressAgainToStop(on)}
      />

      <h3 className="setting-title screen-text">{t("StopShortcut")}</h3>
      <button className="key setting-key setting-spaced" onClick={() => void chooseStopHotkey()}>
        {stopShortcut ?? t("SetShortcut")}
      </button>

      {children}
    </section>
  );
}
