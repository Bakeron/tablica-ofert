import { useCallback, useEffect, useRef, useState } from "react";
import type { Api } from "../api";
import type { DeviceCode, SecretHints, Settings, SyncStatus } from "../api/types";
import { useI18n } from "../i18n";
import { fullDate } from "../lib/format";
import { isPhone, scanSyncKey } from "../lib/platform";
import { IconWarning } from "./Icons";
import { SecretField } from "./SecretField";
import { TextSetting } from "./TextSetting";

const QR_SECONDS = 120;

/** Settings → "Sync": Google Drive (hidden app folder), encrypted, key to the phone by QR code. */
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
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
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
      onOpenLink(c.verificationUrl);
      polling.current = true;
      const until = Date.now() + c.expiresIn * 1000;
      while (polling.current && Date.now() < until) {
        await new Promise((r) => setTimeout(r, Math.max(c.interval, 5) * 1000));
        if (!polling.current) return;
        if (await api.syncLoginPoll(c.deviceCode)) {
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
      onError(e);
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
      onError(e);
    } finally {
      setBusy(false);
      await load();
    }
  }

  const connected = status?.connected ?? false;

  return (
    <div className="settings-part">
      <p>{t("sync.intro")}</p>
      <div className="warn-box" role="note">
        <IconWarning />
        <p>
          <strong>{t("mail.warnTitle")}: </strong>
          {t("sync.lostWarning")}
        </p>
      </div>

      <label className="choice-item">
        <input type="checkbox" checked={settings.sync_drive === "on"} onChange={(e) => onChange("sync_drive", e.target.checked ? "on" : "off")} />
        <span>{t("sync.enable")}</span>
      </label>

      {settings.sync_drive === "on" && (
        <>
          {status && !status.client && (
            <section className="source" aria-labelledby="client-title">
              <h3 id="client-title" className="source-title">{t("sync.clientTitle")}</h3>
              <p>{t("sync.clientText")}</p>
              <div className="mail-grid">
                <TextSetting label={t("sync.clientId")} value={settings.google_client_id} onSave={(v) => onChange("google_client_id", v)} />
                <SecretField
                  name="google-client-secret"
                  label={t("sync.clientSecret")}
                  hint={hints["google-client-secret"]}
                  // Errors are shown by the field itself, under the input.
                  onSave={async (name, value) => {
                    await api.setSecret(name, value);
                    await load();
                    return true;
                  }}
                  onDelete={async (name) => {
                    await api.deleteSecret(name);
                    await load();
                    return true;
                  }}
                />
              </div>
            </section>
          )}

          <section className="source" aria-labelledby="drive-title">
            <h3 id="drive-title" className="source-title">{t("sync.driveTitle")}</h3>
            {code ? (
              <div className="device-code" role="status">
                <p>{t("sync.typeCode")}</p>
                <p className="device-code-value mono">{code.userCode}</p>
                <button type="button" className="btn btn-ghost btn-small" onClick={() => onOpenLink(code.verificationUrl)}>
                  {code.verificationUrl}
                </button>
              </div>
            ) : (
              <p>{connected ? t("sync.isConnected") : t("sync.notConnected")}</p>
            )}
            {status?.lastSync && <p className="source-state">{t("sync.last", { when: fullDate(status.lastSync, lang) })}</p>}
            {status?.lastError && (
              <p className="source-warn">
                <IconWarning /> {status.lastError}
              </p>
            )}
            <div className="ai-row">
              {!connected && (
                <button type="button" className="btn btn-sign btn-small" disabled={!status?.client || code !== null} onClick={connect}>
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
                    try {
                      const key = await scanSyncKey();
                      if (!key) return onToast(t("sync.scanNone"));
                      await api.syncSetKey(key);
                      onToast(t("sync.scanDone"));
                      await load();
                    } catch (e) {
                      onError(e);
                    }
                  }}
                >
                  {t("sync.scan")}
                </button>
              </div>
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
                  onClick={() => onConfirm(t("sync.newKeyConfirm"), () => act(() => api.syncNewKey(), t("sync.newKeyDone")), t("sync.newKey"))}
                >
                  {t("sync.newKey")}
                </button>
              </div>
            )}
          </section>
          )}
        </>
      )}
    </div>
  );
}
