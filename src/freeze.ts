// Guards the app window against prototype pollution - what Tauri's `freezePrototype` did.
// Tauri put it into every window, also the offer page preview, where it broke pages
// (justjoin.it stopped with "This page couldn't load"), so the app does it only for itself.
// Imported first in `main.tsx`, so it runs before any other code. Only in the app, not in the
// web demo.
if ("__TAURI_INTERNALS__" in window) Object.freeze(Object.prototype);

export {};
