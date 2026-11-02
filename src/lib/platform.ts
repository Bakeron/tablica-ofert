// The phone is the helper (zalozenia.md §2): viewing offers, stages, notes, "Share",
// reminders and sync. Collecting offers, AI and the alert mailbox stay on the PC.

export function isPhone(): boolean {
  return typeof navigator !== "undefined" && /Android/i.test(navigator.userAgent);
}

function insideTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Scans the PC's QR code with the sync key; `null` = cancelled or not a Tablica Ofert code. */
export async function scanSyncKey(): Promise<string | null> {
  if (!insideTauri()) return null;
  const scanner = await import("@tauri-apps/plugin-barcode-scanner");
  const allowed = (await scanner.checkPermissions()) === "granted" || (await scanner.requestPermissions()) === "granted";
  if (!allowed) return null;
  const result = await scanner.scan({ formats: [scanner.Format.QRCode] });
  const match = /^tablica-ofert:sync:1:([0-9a-f]{64})$/.exec(result.content.trim());
  return match ? match[1] : null;
}

export interface PhoneReminder {
  /** Stable number for the notification (from the date's id and kind). */
  id: number;
  at: Date;
  title: string;
  body: string;
}

/** Plans reminders with the phone's own notifications (they come even when the app is closed). */
export async function scheduleReminders(list: PhoneReminder[]): Promise<void> {
  if (!insideTauri() || !isPhone()) return;
  const n = await import("@tauri-apps/plugin-notification");
  if (!(await n.isPermissionGranted()) && (await n.requestPermission()) !== "granted") return;
  // Old plans are replaced - dates may have moved or been deleted.
  const pending = await n.pending();
  if (pending.length) await n.cancel(pending.map((p) => p.id));
  const now = Date.now();
  for (const r of list) {
    if (r.at.getTime() <= now) continue;
    n.sendNotification({ id: r.id, title: r.title, body: r.body, schedule: n.Schedule.at(r.at) });
  }
}
