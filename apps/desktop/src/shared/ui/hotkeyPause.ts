import { commands } from "../ipc";

// Global keys swallow their presses, so they stay off while a text field or a dialog is open.
// The search box and a dialog can both be holding at once, hence the count.
let holds = 0;

/** Returns the function that gives the keys back. Calling it twice does nothing. */
export function holdHotkeys() {
  if (holds++ === 0) void commands.pauseHotkeys();

  let released = false;
  return () => {
    if (released) return;
    released = true;
    if (--holds === 0) void commands.resumeHotkeys();
  };
}

/** Native dialogs (file pickers, questions) do not stop the global keys either. */
export async function whileHeld<T>(work: () => Promise<T>) {
  const release = holdHotkeys();
  try {
    return await work();
  } finally {
    release();
  }
}
