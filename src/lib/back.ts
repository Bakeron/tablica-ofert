import type { PluginListener } from "@tauri-apps/api/core";
import { isPhone } from "./platform";

// Android back button / gesture: it closes what is on top (a window, the QR scanner) instead
// of leaving the app. With nothing open the app does not listen, so Android does what it
// always does (the app goes to the background).

type Step = () => void;

const steps: Step[] = [];
let listener: Promise<PluginListener | null> | null = null;

async function listen(): Promise<PluginListener | null> {
  const { onBackButtonPress } = await import("@tauri-apps/api/app");
  return onBackButtonPress(() => steps[steps.length - 1]?.());
}

/** While something is open: "back" calls `step`. Returns the function that stops it. */
export function onBack(step: Step): () => void {
  if (!isPhone() || !("__TAURI_INTERNALS__" in window)) return () => {};
  steps.push(step);
  if (!listener) listener = listen().catch(() => null);
  return () => {
    const i = steps.lastIndexOf(step);
    if (i >= 0) steps.splice(i, 1);
    if (steps.length === 0 && listener) {
      const done = listener;
      listener = null;
      void done.then((l) => l?.unregister());
    }
  };
}
