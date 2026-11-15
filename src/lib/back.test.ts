import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The Android "back" order without a phone: the platform says "phone" and the Tauri listener is a
// stand-in that remembers the handler and whether it was dropped.
let handler: (() => void) | null = null;
const unregister = vi.fn();
const listened = vi.fn();

const demo = vi.hoisted(() => ({ on: false }));
vi.mock("./platform", () => ({ isPhone: () => true }));
vi.mock("./demo", () => ({
  get DEMO() {
    return demo.on;
  },
}));
vi.mock("@tauri-apps/api/app", () => ({
  onBackButtonPress: async (h: () => void) => {
    listened();
    handler = h;
    return { unregister };
  },
}));

const flush = () => new Promise((r) => setTimeout(r, 0));

async function load() {
  vi.resetModules();
  return (await import("./back")).onBack;
}

describe("onBack", () => {
  beforeEach(() => {
    (globalThis as { window?: object }).window = { __TAURI_INTERNALS__: {} };
    handler = null;
    unregister.mockClear();
    listened.mockClear();
  });
  afterEach(() => {
    delete (globalThis as { window?: object }).window;
  });

  it("closes only what is on top, then what is below", async () => {
    const onBack = await load();
    const calls: string[] = [];
    const stopWindow = onBack(() => calls.push("window"));
    const stopDialog = onBack(() => calls.push("dialog"));
    await flush();
    handler!();
    expect(calls).toEqual(["dialog"]);
    stopDialog();
    handler!();
    expect(calls).toEqual(["dialog", "window"]);
    stopWindow();
  });

  it("listens once and stops listening when nothing is open, so Android may leave the app", async () => {
    const onBack = await load();
    const stopA = onBack(() => {});
    const stopB = onBack(() => {});
    await flush();
    expect(listened).toHaveBeenCalledTimes(1);
    stopA();
    await flush();
    expect(unregister).not.toHaveBeenCalled();
    stopB();
    await flush();
    expect(unregister).toHaveBeenCalledTimes(1);
  });

  it("a closed step that is not on top is taken out without touching the others", async () => {
    const onBack = await load();
    const calls: string[] = [];
    const stopBelow = onBack(() => calls.push("below"));
    onBack(() => calls.push("top"));
    await flush();
    stopBelow();
    handler!();
    expect(calls).toEqual(["top"]);
  });

  it("a thing on the list waits below every window, even one opened before it", async () => {
    const onBack = await load();
    const calls: string[] = [];
    const stopTab = onBack(() => calls.push("tab"), true);
    const stopCard = onBack(() => calls.push("card"));
    const stopTree = onBack(() => calls.push("tree"), true);
    await flush();
    handler!();
    expect(calls).toEqual(["card"]);
    stopCard();
    handler!();
    expect(calls).toEqual(["card", "tree"]);
    stopTree();
    handler!();
    expect(calls).toEqual(["card", "tree", "tab"]);
    stopTab();
    await flush();
    expect(unregister).toHaveBeenCalledTimes(1);
  });

  it("does nothing outside the phone app", async () => {
    delete (globalThis as { window?: object }).window;
    (globalThis as { window?: object }).window = {};
    const onBack = await load();
    const stop = onBack(() => {});
    await flush();
    expect(listened).not.toHaveBeenCalled();
    stop();
  });
});

describe("onBack in the phone demo in its own browser tab", () => {
  afterEach(() => {
    demo.on = false;
    vi.unstubAllGlobals();
  });

  it("the browser Back closes what is on top and leaves the page only with nothing to undo", async () => {
    demo.on = true;
    let pop: (() => void) | null = null;
    const pushState = vi.fn();
    const back = vi.fn();
    const win: Record<string, unknown> = {};
    win.top = win;
    vi.stubGlobal("window", win);
    vi.stubGlobal("history", { state: null, pushState, back });
    vi.stubGlobal("addEventListener", (_: string, h: () => void) => (pop = h));
    const onBack = await load();
    const calls: string[] = [];
    const stopCard = onBack(() => calls.push("card"));
    expect(pushState).toHaveBeenCalledTimes(1);
    pop!();
    expect(calls).toEqual(["card"]);
    expect(pushState).toHaveBeenCalledTimes(2);
    stopCard();
    pop!();
    expect(back).toHaveBeenCalledTimes(1);
    expect(listened).not.toHaveBeenCalled();
  });

  it("inside the demo window on the home page Back belongs to that page", async () => {
    demo.on = true;
    const pushState = vi.fn();
    vi.stubGlobal("window", { top: {} });
    vi.stubGlobal("history", { state: null, pushState, back: vi.fn() });
    const onBack = await load();
    onBack(() => {})();
    expect(pushState).not.toHaveBeenCalled();
  });
});
