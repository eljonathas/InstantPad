import { useSyncExternalStore } from "react";
import type { MeterFrame } from "../ipc";

// Frames arrive about 30 times per second, so they live outside React state: the LED meters
// paint straight into the DOM and each pad only re-renders when its own lit flag flips.
let frame: MeterFrame = { micLevel: 0, outputLevel: 0, playingSlots: [] };
const listeners = new Set<() => void>();

export const meter = {
  read: () => frame,
  publish(next: MeterFrame) {
    frame = next;
    listeners.forEach((listener) => listener());
  },
  subscribe(listener: () => void) {
    listeners.add(listener);
    return () => void listeners.delete(listener);
  },
};

export function useIsPlaying(slot: number) {
  return useSyncExternalStore(meter.subscribe, () => meter.read().playingSlots.includes(slot));
}
