import type { TextId } from "../../shared/i18n";
import { commands, type AiView, type EffectView, type Message } from "../../shared/ipc";

/** An AI voice speaks only once its models are here and it knows the user's own voice. */
export const isAiReady = (ai: AiView) => ai.status.kind === "ready" && ai.isCalibrated;

export function selectEffect({ effect, cloneId }: EffectView) {
  return cloneId === null ? commands.selectVoiceEffect(effect) : commands.selectClonedVoice(cloneId);
}

/**
 * What the user asked for while the AI was not ready. The backend refuses it and says why on the
 * LCD; the controls then lead through the setup, and `apply` asks again once it is done.
 */
export interface PendingAi {
  name: Message | TextId;
  apply: () => void;
}
