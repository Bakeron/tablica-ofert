// The demo version for the website (`vite --mode demo`): the browser preview with made-up data,
// where everything that reaches outside (AI, offer sources, mail, Google Drive, files) says
// "does not work in the demo" instead of pretending.

/** Built or started with `--mode demo`. */
export const DEMO = import.meta.env.MODE === "demo";

/** Thrown by the demo stand-ins of calls that need the real app - shown as `error.demo`. */
export const DEMO_ERROR = { code: "demo", detail: "" } as const;

/** The public code of the app, linked from the demo bar. */
export const DEMO_REPO = "https://github.com/Bakeron/tablica-ofert";
