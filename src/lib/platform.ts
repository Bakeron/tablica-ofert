// The phone is the helper (zalozenia.md §2): viewing offers, stages, notes, "Share",
// reminders and sync. Collecting offers, AI and the alert mailbox stay on the PC.

export function isPhone(): boolean {
  return typeof navigator !== "undefined" && /Android/i.test(navigator.userAgent);
}

function insideTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** The text of our QR code (Rust checks it), or why there is none: not our code, cancelled by the person, or no camera permission. */
export type ScanResult = { text: string } | "none" | "cancelled" | "denied";

/** "Cancel" or "back" may come before the camera starts - then it must not start at all. */
let scanCancelled = false;

/**
 * Scans the PC's QR code with the sync key. The camera shows under the app (`windowed`), so
 * the app draws its own frame and "Cancel" on top (`cancelScan`).
 */
export async function scanSyncKey(): Promise<ScanResult> {
  if (!insideTauri()) return "none";
  scanCancelled = false;
  const scanner = await import("@tauri-apps/plugin-barcode-scanner");
  const allowed = (await scanner.checkPermissions()) === "granted" || (await scanner.requestPermissions()) === "granted";
  if (!allowed) return "denied";
  if (scanCancelled) return "cancelled";
  try {
    const result = await scanner.scan({ windowed: true, formats: [scanner.Format.QRCode] });
    const text = result.content.trim();
    return text.startsWith("tablica-ofert:sync:") && text.length <= 1000 ? { text } : "none";
  } catch {
    // cancel() makes scan() fail
    return scanCancelled ? "cancelled" : "none";
  }
}

export async function cancelScan(): Promise<void> {
  scanCancelled = true;
  if (!insideTauri()) return;
  const scanner = await import("@tauri-apps/plugin-barcode-scanner");
  await scanner.cancel();
}

/** Android's settings page of this app (to allow the camera after saying no). */
export async function openAppSettings(): Promise<void> {
  if (!insideTauri()) return;
  const scanner = await import("@tauri-apps/plugin-barcode-scanner");
  await scanner.openAppSettings();
}

export interface PhoneReminder {
  /** Stable number for the notification (from the date's id and kind). */
  id: number;
  at: Date;
  title: string;
  body: string;
}

/**
 * Asked straight from the system: the plugin's `isPermissionGranted()` remembers the answer
 * from the app's start, so it would miss a change made in Android's settings.
 */
export async function notificationsAllowed(): Promise<boolean> {
  if (!insideTauri() || !isPhone()) return true;
  const { invoke } = await import("@tauri-apps/api/core");
  return (await invoke<boolean | null>("plugin:notification|is_permission_granted")) === true;
}

/** Android's question about notifications, unless they are allowed already. */
export async function askNotifications(): Promise<boolean> {
  if (await notificationsAllowed()) return true;
  const n = await import("@tauri-apps/plugin-notification");
  return (await n.requestPermission()) === "granted";
}

/** One planning at a time: a later one must not be overtaken by an earlier, slower one. */
let planning: Promise<void> = Promise.resolve();

/**
 * Plans reminders with the phone's own notifications (they come even when the app is closed).
 * An empty list cancels them all (reminders switched off). Never asks for the permission:
 * that waits for the first date or the setting (`askNotifications`).
 */
export function scheduleReminders(list: PhoneReminder[]): Promise<void> {
  const run = () => planReminders(list).catch((e) => console.warn("reminders", e));
  planning = planning.then(run, run);
  return planning;
}

async function planReminders(list: PhoneReminder[]): Promise<void> {
  if (!insideTauri() || !isPhone()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  const n = await import("@tauri-apps/plugin-notification");
  const now = Date.now();
  const future = list.filter((r) => r.at.getTime() > now);
  // Old plans are replaced - dates may have moved or been deleted. The plugin remembers the ids of
  // plans made with `batch`, so `cancelAll` reaches them in any later session; the ids still to
  // come are cancelled too, for plans made before (app 1.7 and older). Reminders already shown stay.
  // Not `pending()`: plugin 2.8.0 stores `batch` plans without their content and fails reading them.
  await n.cancelAll();
  if (future.length) await n.cancel(future.map((r) => r.id));
  if (!future.length || !(await notificationsAllowed())) return;
  // allowWhileIdle: a sleeping phone (Doze) still shows it, within minutes of the time.
  const notifications = future.map((r) => ({ id: r.id, title: r.title, body: r.body, schedule: n.Schedule.at(r.at, false, true) }));
  await invoke("plugin:notification|batch", { notifications });
}
