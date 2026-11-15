import type { PluginListener } from "@tauri-apps/api/core";
import { DEMO } from "./demo";
import { isPhone } from "./platform";

// Android back button / gesture: it undoes the last move -
// closes what is on top (a window, the QR scanner, the card), then what is open on the main
// screen (an unfolded tree, a search, another tab). With nothing to undo the app does not listen,
// so Android does what it always does (the app goes to the background).
// The phone demo in its own browser tab: the browser Back button does the same and leaves the
// page only with nothing to undo. Not inside the demo window on the home page - there Back
// belongs to that page (it closes the window with the demo).

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

let browserListening = false;

/** The phone demo: one extra history entry catches the browser Back button. */
function listenInBrowser(): void {
  if (browserListening) return;
  browserListening = true;
  if (!(history.state as { demo?: number } | null)?.demo) history.pushState({ demo: 1 }, ""); // after a reload the entry is already there
  addEventListener("popstate", () => {
    const list = steps.length > 0 ? steps : under;
    const step = list[list.length - 1];
    if (step) {
      step();
      history.pushState({ demo: 1 }, "");
    } else history.back();
  });
}

/**
 * While something is open: "back" calls `step`. `onList` - a thing on the main list (an unfolded
 * tree, a search, another tab): it waits below every window, whenever it came. Returns the
 * function that stops it.
 */
export function onBack(step: Step, onList = false): () => void {
  if (!isPhone()) return () => {};
  if (!("__TAURI_INTERNALS__" in window)) {
    if (!DEMO || window.top !== window) return () => {};
    listenInBrowser();
    const list = onList ? under : steps;
    list.push(step);
    return () => {
      const i = list.lastIndexOf(step);
      if (i >= 0) list.splice(i, 1);
    };
  }
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
