import type { PluginListener } from "@tauri-apps/api/core";
import { isPhone } from "./platform";

// Android back button / gesture: it undoes the last move -
// closes what is on top (a window, the QR scanner, the card), then what is open on the main
// screen (an unfolded tree, a search, another tab). With nothing to undo the app does not listen,
// so Android does what it always does (the app goes to the background).

type Step = () => void;

/** Windows and screens over the list - "back" closes the one on top first. */
const steps: Step[] = [];
/** Things open on the list itself - only when no window is over it. */
const under: Step[] = [];
let listener: Promise<PluginListener | null> | null = null;

async function listen(): Promise<PluginListener | null> {
  const { onBackButtonPress } = await import("@tauri-apps/api/app");
  return onBackButtonPress(() => {
    const list = steps.length > 0 ? steps : under;
    list[list.length - 1]?.();
  });
}

/**
 * While something is open: "back" calls `step`. `onList` - a thing on the main list (an unfolded
 * tree, a search, another tab): it waits below every window, whenever it came. Returns the
 * function that stops it.
 */
export function onBack(step: Step, onList = false): () => void {
  if (!isPhone() || !("__TAURI_INTERNALS__" in window)) return () => {};
  const list = onList ? under : steps;
  list.push(step);
  if (!listener) listener = listen().catch(() => null);
  return () => {
    const i = list.lastIndexOf(step);
    if (i >= 0) list.splice(i, 1);
    if (steps.length === 0 && under.length === 0 && listener) {
      const done = listener;
      listener = null;
      void done.then((l) => l?.unregister());
    }
  };
}
