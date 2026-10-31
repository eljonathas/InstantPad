import type { TextId } from "../../shared/i18n";

// A sketch of what the backend will send, so the screen builds its knobs from data and a new
// effect needs no new interface code.
export interface EffectParam {
  id: string;
  label: TextId;
  min: number;
  max: number;
  value: number;
}

export type EffectIconName = "mic" | "feather" | "mountain" | "wave";

export interface Effect {
  id: string;
  name: TextId;
  icon: EffectIconName;
  color: string;
  params: EffectParam[];
}

export const EFFECTS: Effect[] = [
  { id: "natural", name: "EffectNatural", icon: "mic", color: "#1fd6ff", params: [] },
  {
    id: "thin",
    name: "EffectThin",
    icon: "feather",
    color: "#1fd6ff",
    params: [
      { id: "intensity", label: "ParamIntensity", min: 0, max: 1, value: 0.5 },
      { id: "formant", label: "ParamFormant", min: 0, max: 1, value: 0.5 },
    ],
  },
  {
    id: "deep",
    name: "EffectDeep",
    icon: "mountain",
    color: "#b45cff",
    params: [
      { id: "intensity", label: "ParamIntensity", min: 0, max: 1, value: 0.5 },
      { id: "formant", label: "ParamFormant", min: 0, max: 1, value: 0.5 },
    ],
  },
  {
    id: "cycle",
    name: "EffectCycle",
    icon: "wave",
    color: "#35f27a",
    params: [
      { id: "time", label: "ParamTime", min: 0, max: 1, value: 0.4 },
      { id: "high", label: "ParamHigh", min: 0, max: 1, value: 0.6 },
      { id: "low", label: "ParamLow", min: 0, max: 1, value: 0.6 },
    ],
  },
];
