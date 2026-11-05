// The phone is the helper (zalozenia.md §2): viewing offers, stages, notes, "Share",
// reminders and sync. Collecting offers, AI and the alert mailbox stay on the PC.

export function isPhone(): boolean {
  return typeof navigator !== "undefined" && /Android/i.test(navigator.userAgent);
}

function insideTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** The key from the QR code, or why there is none: cancelled / not our code, or no camera permission. */
export type ScanResult = { key: string } | "none" | "denied";

/**
 * Scans the PC's QR code with the sync key. The camera shows under the app (`windowed`), so
 * the app draws its own frame and "Cancel" on top (`cancelScan`).
 */
export async function scanSyncKey(): Promise<ScanResult> {
  if (!insideTauri()) return "none";
  const scanner = await import("@tauri-apps/plugin-barcode-scanner");
  const allowed = (await scanner.checkPermissions()) === "granted" || (await scanner.requestPermissions()) === "granted";
  if (!allowed) return "denied";
  try {
    const result = await scanner.scan({ windowed: true, formats: [scanner.Format.QRCode] });
    const match = /^tablica-ofert:sync:1:([0-9a-f]{64})$/.exec(result.content.trim());
    return match ? { key: match[1] } : "none";
  } catch {
    // cancel() makes scan() fail
    return "none";
  }
}

export async function cancelScan(): Promise<void> {
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

/**
 * Plans reminders with the phone's own notifications (they come even when the app is closed).
 * Never asks for the permission: that waits for the first date or the setting (`askNotifications`).
 */
export async function scheduleReminders(list: PhoneReminder[]): Promise<void> {
  if (!insideTauri() || !isPhone() || !(await notificationsAllowed())) return;
  const n = await import("@tauri-apps/plugin-notification");
  // Old plans are replaced - dates may have moved or been deleted.
  const pending = await n.pending();
  if (pending.length) await n.cancel(pending.map((p) => p.id));
  const now = Date.now();
  for (const r of list) {
    if (r.at.getTime() <= now) continue;
    n.sendNotification({ id: r.id, title: r.title, body: r.body, schedule: n.Schedule.at(r.at) });
  }
}
