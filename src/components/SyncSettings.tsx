import { useCallback, useEffect, useRef, useState } from "react";
import type { Api } from "../api";
import { describeError, type DeviceCode, type SecretHints, type Settings, type SyncStatus } from "../api/types";
import { useI18n } from "../i18n";
import { fullDate } from "../lib/format";
import { onBack } from "../lib/back";
import { cancelScan, isPhone, openAppSettings, scanSyncKey } from "../lib/platform";
import { TEXT_MAX } from "../lib/limits";
import { rememberUnfolded, settingsMemory } from "../lib/settingsMemory";
import { IconArrowOut, IconWarning } from "./Icons";
import { SecretField } from "./SecretField";
import { TextSetting } from "./TextSetting";

/** Name of the "How to get them" part in the memory of the Settings window. */
const CLIENT_HOWTO = "sync-client-howto";
const CLOUD_CONSOLE = "https://console.cloud.google.com/projectcreate";
const CLIENT_STEPS = [
  "sync.client.step1",
  "sync.client.step2",
  "sync.client.step3",
  "sync.client.step4",
  "sync.client.step5",
  "sync.client.step6",
  "sync.client.step7",
] as const;
/** Same text as `drive::LOGIN_EXPIRED` in Rust - the Google login has ended, connect again. */
const LOGIN_EXPIRED = "google login expired";
const loginExpired = (text: string | null | undefined) => !!text && text.includes(LOGIN_EXPIRED);
/** Same text as `AppError::Outdated` in Rust - the other device has a newer version of the app. */
const OUTDATED = "newer app version";
/** Same text as `AppError::OtherOutdated` in Rust - the other device has an older version of the app. */
const OTHER_OUTDATED = "older app version on the other device";
/** `NO_KEY` in sync/mod.rs - the phone syncs only with the key from the PC's QR code. */
const NO_KEY = "no sync key yet";
/// `CONFIRM_WAITING` in sync/mod.rs.
const CONFIRM_WAITING = "waits for confirmation";
/** `crypto::open` in Rust - the file on Drive was sealed with another key than this device's. */
const BAD_KEY = "cannot be read with this key";

const QR_SECONDS = 120;
/** Asking Google again survives a short loss of network (the phone is in the browser meanwhile). */
const POLL_NET_FAILS = 3;

/** Settings → "Sync": one encrypted file in Google Drive (`drive.file`), key to the phone by QR code. */
export function SyncSettings({
  api,
  settings,
  onChange,
  onOpenLink,
  onError,
  onToast,
  onConfirm,
  onSynced,
}: {
  api: Api;
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void | Promise<boolean>;
  onOpenLink: (url: string) => void;
  onError: (e: unknown) => void;
  onToast: (text: string) => void;
  onConfirm: (message: string, action: () => void, label?: string) => void;
  onSynced: () => void;
}) {
  const { t, lang } = useI18n();
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [hints, setHints] = useState<SecretHints>({});
  const [code, setCode] = useState<DeviceCode | null>(null);
  const [busy, setBusy] = useState(false);
  const [scanning, setScanning] = useState(false);
  // "Change Google project": the project part shows again although a project is set.
  const [editClient, setEditClient] = useState(false);
  const [qr, setQr] = useState<{ svg: string; left: number } | null>(null);
  const polling = useRef(false);

  const load = useCallback(async () => {
    try {
      const [s, h] = await Promise.all([api.syncStatus(), api.secretHints()]);
      setStatus(s);
      setHints(h);
    } catch (e) {
      onError(e);
    }
  }, [api, onError]);

  useEffect(() => {
    load();
  }, [load, settings.google_client_id]);

  // The QR code disappears by itself - nobody can photograph it later.
  useEffect(() => {
    if (!qr) return;
    if (qr.left <= 0) {
      setQr(null);
      return;
    }
    const timer = setTimeout(() => setQr((q) => (q ? { ...q, left: q.left - 1 } : q)), 1000);
    return () => clearTimeout(timer);
  }, [qr]);

  useEffect(() => () => void (polling.current = false), []);

  async function connect() {
    try {
      const c = await api.syncLoginStart();
      setCode(c);
      polling.current = true;
      const until = Date.now() + c.expiresIn * 1000;
      let netFails = 0;
      while (polling.current && Date.now() < until) {
        await new Promise((r) => setTimeout(r, Math.max(c.interval, 5) * 1000));
        if (!polling.current) return;
        let done = false;
        try {
          done = await api.syncLoginPoll(c.deviceCode);
          netFails = 0;
        } catch (e) {
          if ((e as { code?: string })?.code !== "net" || ++netFails >= POLL_NET_FAILS) throw e;
        }
        if (done) {
          polling.current = false;
          setCode(null);
          onToast(t("sync.connected"));
          await load();
          return;
        }
      }
      setCode(null);
    } catch (e) {
      polling.current = false;
      setCode(null);
      onError(e);
    }
  }

  async function syncNow(confirm = false) {
    setBusy(true);
    try {
      const outcome = await api.syncNow(confirm);
      if (outcome.status === "confirm") {
        onConfirm(t("sync.confirmRemove", { count: outcome.removed }), () => syncNow(true), t("sync.now"));
      } else {
        onToast(t("sync.done", { changed: outcome.changed, removed: outcome.removed }));
        onSynced();
      }
    } catch (e) {
      const detail = describeError(e).detail;
      // An ended login is said in the Google Drive part, with the "Connect" button next to it.
      if (detail.includes(NO_KEY)) onToast(t("sync.noKey"));
      else if (detail.includes(BAD_KEY)) onToast(t(isPhone() ? "sync.badKeyPhone" : "sync.badKey"));
      else if (!loginExpired(detail)) onError(e);
    } finally {
      setBusy(false);
      await load();
    }
  }

  async function act(action: () => Promise<unknown>, done: string) {
    setBusy(true);
    try {
      await action();
      onToast(done);
    } catch (e) {
      if (!loginExpired(describeError(e).detail)) onError(e);
    } finally {
      setBusy(false);
      await load();
    }
  }

  const connected = status?.connected ?? false;
  const expired = !connected && loginExpired(status?.lastError);
  const ownProject = settings.google_client_id.trim() !== "";

  /** A login belongs to one Google project - after a change the person connects again. */
  async function dropOldLogin() {
    if (!connected) return;
    try {
      await api.syncDisconnect();
    } catch (e) {
      onError(e);
    }
  }

  const about = (
    <>
      <p>{t(isPhone() ? "sync.introPhone" : "sync.intro")}</p>
      <div className="warn-box" role="note">
        <IconWarning />
        <p>
          <strong>{t("mail.warnTitle")}: </strong>
          {t(isPhone() ? "sync.lostWarningPhone" : "sync.lostWarning")}
        </p>
      </div>
    </>
  );

  return (
    <div className="settings-part">
      {/* Connecting is the first thing to do on a phone - the explanation comes under it. */}
      {!isPhone() && about}

      <label className="choice-item">
        <input type="checkbox" checked={settings.sync_drive === "on"} onChange={(e) => onChange("sync_drive", e.target.checked ? "on" : "off")} />
        <span>{t("sync.enable")}</span>
      </label>

      {settings.sync_drive === "on" && (
        <>
          {status && (!status.client || editClient) && (
            <section className="source" aria-labelledby="client-title">
              <h3 id="client-title" className="source-title">{t("sync.clientTitle")}</h3>
              <p>{t("sync.clientText")}</p>
              <details
                className="howto"
                open={settingsMemory.unfolded.has(CLIENT_HOWTO)}
                onToggle={(e) => rememberUnfolded(CLIENT_HOWTO, e.currentTarget.open)}
              >
                <summary>{t("sync.client.howto")}</summary>
                <ol className="steps-list">
                  {CLIENT_STEPS.map((k) => (
                    <li key={k}>{t(k)}</li>
                  ))}
                </ol>
                <p className="hint">{t("sync.client.note")}</p>
                <button type="button" className="btn btn-ghost btn-small" onClick={() => onOpenLink(CLOUD_CONSOLE)}>
                  {t("sync.client.open")} <IconArrowOut />
                </button>
              </details>
              <div className="mail-grid">
                <TextSetting
                  label={t("sync.clientId")}
                  maxLength={TEXT_MAX.google_client_id}
                  value={settings.google_client_id}
                  onSave={async (raw) => {
                    // The same cleaning as in Rust (settings::without_spaces), so the field shows what was saved.
                    const v = raw.replace(/[\s\u200B-\u200F\u202A-\u202E\u2060-\u2064\uFEFF]/g, "");
                    try {
                      await api.setSetting("google_client_id", v);
                    } catch {
                      return t("sync.clientIdBad");
                    }
                    await dropOldLogin();
                    onChange("google_client_id", v);
                    return null;
                  }}
                />
                <SecretField
                  name="google-client-secret"
                  label={t("sync.clientSecret")}
                  hint={hints["google-client-secret"]}
                  // Errors are shown by the field itself, under the input.
                  onSave={async (name, value) => {
                    await api.setSecret(name, value);
                    await dropOldLogin();
                    await load();
                    return true;
                  }}
                  onDelete={async (name) => {
                    await api.deleteSecret(name);
                    await load();
                    return true;
                  }}
                  onReveal={api.revealSecret}
                />
              </div>
              {editClient && (
                <>
                  {connected && <p className="hint">{t("sync.changeClientHint")}</p>}
                  <div>
                    <button type="button" className="btn btn-ghost btn-small" onClick={() => setEditClient(false)}>
                      {t("sync.changeClientDone")}
                    </button>
                  </div>
                </>
              )}
            </section>
          )}

          <section className="source" aria-labelledby="drive-title">
            <h3 id="drive-title" className="source-title">{t("sync.driveTitle")}</h3>
            {code ? (
              <DeviceCodeSteps code={code} onOpenLink={onOpenLink} onToast={onToast} />
            ) : (
              <p>{connected ? t("sync.isConnected") : t("sync.notConnected")}</p>
            )}
            {status?.lastSync && <p className="source-state">{t("sync.last", { when: fullDate(status.lastSync, lang) })}</p>}
            {status?.lastError &&
              (expired ? (
                <p className="source-warn">
                  <IconWarning /> {t("sync.expired")}
                </p>
              ) : (
                <p className="source-warn">
                  <IconWarning /> {t(
                    status.lastError.includes(OUTDATED)
                      ? "error.outdated"
                      : status.lastError.includes(OTHER_OUTDATED)
                        ? "error.other_outdated"
                        : status.lastError.startsWith(CONFIRM_WAITING)
                        ? "sync.waitsConfirm"
                        : status.lastError.includes(NO_KEY)
                        ? "sync.noKey"
                        : status.lastError.includes(BAD_KEY)
                        ? isPhone()
                          ? "sync.badKeyPhone"
                          : "sync.badKey"
                        : "sync.failed",
                  )}
                </p>
              ))}
            {ownProject && !expired && <p className="hint">{t("sync.ownProject")}</p>}
            <div className="ai-row">
              {!connected && code === null && (
                <button type="button" className="btn btn-sign btn-small" disabled={!status?.client} onClick={connect}>
                  {t("sync.connect")}
                </button>
              )}
              {connected && (
                <>
                  <button type="button" className="btn btn-sign btn-small" disabled={busy} aria-busy={busy} onClick={() => syncNow()}>
                    {busy ? t("sync.working") : t("sync.now")}
                  </button>
                  <button
                    type="button"
                    className="btn btn-ghost btn-small"
                    disabled={busy}
                    onClick={() => onConfirm(t("sync.deleteRemoteConfirm"), () => act(() => api.syncDeleteRemote(), t("sync.deletedRemote")), t("sync.deleteRemote"))}
                  >
                    {t("sync.deleteRemote")}
                  </button>
                  <button type="button" className="btn btn-ghost btn-small" disabled={busy} onClick={() => act(() => api.syncDisconnect(), t("sync.disconnected"))}>
                    {t("sync.disconnect")}
                  </button>
                </>
              )}
              {ownProject && status?.client && !editClient && (
                <button type="button" className="btn btn-ghost btn-small" disabled={busy || code !== null} onClick={() => setEditClient(true)}>
                  {t("sync.changeClient")}
                </button>
              )}
            </div>
          </section>

          {isPhone() ? (
            <section className="source" aria-labelledby="phone-title">
              <h3 id="phone-title" className="source-title">{t("sync.scanTitle")}</h3>
              <p>{t("sync.scanText")}</p>
              <div>
                <button
                  type="button"
                  className="btn btn-sign btn-small"
                  onClick={async () => {
                    setScanning(true);
                    try {
                      const found = await scanSyncKey();
                      setScanning(false);
                      if (found === "denied") return onConfirm(t("sync.cameraDenied"), () => void openAppSettings(), t("sync.openAppSettings"));
                      if (found === "none") return onToast(t("sync.scanNone"));
                      if (found === "cancelled") return;
                      const clientId = await api.syncFromQr(found.text);
                      if (clientId) onChange("google_client_id", clientId);
                      onToast(t(clientId ? "sync.scanDoneProject" : "sync.scanDone"));
                      await load();
                    } catch (e) {
                      onError(e);
                    } finally {
                      setScanning(false);
                    }
                  }}
                >
                  {t("sync.scan")}
                </button>
              </div>
              {scanning && <ScanOverlay />}
            </section>
          ) : (
          <section className="source" aria-labelledby="phone-title">
            <h3 id="phone-title" className="source-title">{t("sync.phoneTitle")}</h3>
            <p>{t("sync.phoneText")}</p>
            {qr ? (
              <div className="qr">
                <img src={`data:image/svg+xml;base64,${btoa(qr.svg)}`} width={260} height={260} alt={t("sync.qrAlt")} />
                <p role="timer">{t("sync.qrLeft", { seconds: qr.left })}</p>
                <button type="button" className="btn btn-ghost btn-small" onClick={() => setQr(null)}>
                  {t("sync.qrHide")}
                </button>
              </div>
            ) : (
              <div className="ai-row">
                <button
                  type="button"
                  className="btn btn-ghost btn-small"
                  onClick={() => api.syncQr().then((svg) => setQr({ svg, left: QR_SECONDS }), onError)}
                >
                  {t("sync.qrShow")}
                </button>
                <button
                  type="button"
                  className="btn btn-ghost btn-small"
                  onClick={() =>
                    connected
                      ? onConfirm(t("sync.newKeyConfirm"), () => act(() => api.syncNewKey(), t("sync.newKeyDone")), t("sync.newKey"))
                      : onToast(t("sync.newKeyLogin"))
                  }
                >
                  {t("sync.newKey")}
                </button>
              </div>
            )}
          </section>
          )}
        </>
      )}

      {isPhone() && about}
    </div>
  );
}

/**
 * The Google page covers the app, so the code comes first: "Copy code" is the lead button until
 * the code is copied, then the Google page is. Google is asked all the time, so coming back
 * finishes the connection by itself.
 */
function DeviceCodeSteps({ code, onOpenLink, onToast }: { code: DeviceCode; onOpenLink: (url: string) => void; onToast: (text: string) => void }) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);
  const [opened, setOpened] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(code.userCode);
      onToast(t("mail.copied"));
    } catch {
      onToast(t("sync.copyFailed"));
    }
    // Also after a failed copy: the code can be typed by hand, the way must not stay shut.
    setCopied(true);
  }

  return (
    <div className="device-code">
      <h4 className="device-code-title">{t("sync.stepsTitle")}</h4>
      <ol className="device-code-steps">
        <li>{t("sync.step1")}</li>
        <li>{t("sync.step2")}</li>
        <li>{t("sync.step3")}</li>
      </ol>
      {/* Copied by hand (long press, Ctrl+C) counts too - the button only leads, it never locks. */}
      <p className="device-code-value mono" onCopy={() => setCopied(true)}>
        {code.userCode}
      </p>
      <div className="ai-row">
        <button type="button" className={`btn btn-small ${copied ? "btn-ghost" : "btn-sign"}`} onClick={copy}>
          {t("sync.copyCode")}
        </button>
        <button
          type="button"
          className={`btn btn-small ${copied ? "btn-sign" : "btn-ghost"}`}
          onClick={() => {
            setOpened(true);
            onOpenLink(code.verificationUrl);
          }}
        >
          {t("sync.openGoogle")} <IconArrowOut />
        </button>
      </div>
      <p role="status">{t(opened ? "sync.waiting" : copied ? "sync.copiedNext" : "sync.copyFirst")}</p>
    </div>
  );
}

/**
 * While scanning, the camera picture is under the app: the whole app turns see-through and
 * only this frame, its hint and "Cancel" stay. Android "back" cancels too.
 */
function ScanOverlay() {
  const { t } = useI18n();
  useEffect(() => {
    const root = document.documentElement;
    root.classList.add("scanning");
    const stopBack = onBack(() => void cancelScan());
    return () => {
      root.classList.remove("scanning");
      stopBack();
    };
  }, []);
  return (
    <div className="scan-overlay" role="dialog" aria-modal="true" aria-labelledby="scan-hint">
      <p id="scan-hint" className="scan-hint">
        {t("sync.scanHint")}
      </p>
      <div className="scan-frame" aria-hidden="true" />
      <button type="button" className="btn btn-sign" onClick={() => void cancelScan()} autoFocus>
        {t("sync.scanCancel")}
      </button>
    </div>
  );
}
