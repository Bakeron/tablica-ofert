import { useCallback, useEffect, useState } from "react";
import type { Api } from "../api";
import { AI_PROVIDERS, type AiProvider, type AiStatus, type Machine, type SecretHints, type SecretName, type Settings } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { IconArrowOut, IconWarning } from "./Icons";
import { SecretField } from "./SecretField";
import { TextSetting } from "./TextSetting";

/** Where to get a key - each provider's own page. */
const KEY_PAGES: Partial<Record<AiProvider, string>> = {
  gemini: "https://aistudio.google.com/apikey",
  groq: "https://console.groq.com/keys",
  openrouter: "https://openrouter.ai/keys",
  ollama: "https://ollama.com/download",
  anthropic: "https://platform.claude.com/settings/keys",
  openai: "https://platform.openai.com/api-keys",
};

const FREE: AiProvider[] = ["gemini", "groq", "openrouter", "ollama"];

/** Settings → "AI": provider, key, model, daily limit, usage, "Check my computer". */
export function AiSettings({
  api,
  settings,
  onChange,
  onOpenLink,
  onError,
  onToast,
}: {
  api: Api;
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  onOpenLink: (url: string) => void;
  onError: (e: unknown) => void;
  onToast: (text: string) => void;
}) {
  const { t } = useI18n();
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [hints, setHints] = useState<SecretHints>({});
  const [machine, setMachine] = useState<Machine | null>(null);
  const [checking, setChecking] = useState(false);
  const [rating, setRating] = useState(false);

  const load = useCallback(async () => {
    try {
      const [s, h] = await Promise.all([api.aiStatus(), api.secretHints()]);
      setStatus(s);
      setHints(h);
    } catch (e) {
      onError(e);
    }
  }, [api, onError]);

  useEffect(() => {
    load();
  }, [load, settings.ai_provider, settings.ai_model, settings.ai_daily_limit]);

  const provider = settings.ai_provider;
  const keyName = provider !== "none" && provider !== "ollama" ? (`ai-key-${provider}` as SecretName) : null;

  async function saveSecret(name: SecretName, value: string) {
    // An error is shown by the field itself, under the input (the app's error bar is behind this window).
    await api.setSecret(name, value);
    onToast(t("secret.saved"));
    await load();
    return true;
  }

  async function deleteSecret(name: SecretName) {
    // An error is shown by the field itself, under the input (the app's error bar is behind this window).
    await api.deleteSecret(name);
    onToast(t("secret.deleted"));
    await load();
    return true;
  }

  async function check() {
    setChecking(true);
    try {
      setMachine(await api.checkHardware());
    } catch (e) {
      onError(e);
    } finally {
      setChecking(false);
    }
  }

  async function rateNow() {
    setRating(true);
    try {
      const report = await api.aiRateNow();
      onToast(t("ai.rated", { count: report.rated, waiting: report.waiting }));
      await load();
    } catch (e) {
      onError(e);
    } finally {
      setRating(false);
    }
  }

  return (
    <div className="settings-part">
      <p>{t("ai.intro")}</p>

      <fieldset className="choice ai-providers">
        <legend>{t("ai.provider")}</legend>
        {AI_PROVIDERS.map((p) => (
          <label key={p} className="choice-item ai-provider">
            <input type="radio" name="ai_provider" value={p} checked={provider === p} onChange={() => onChange("ai_provider", p)} />
            <span>
              <strong>{t(`ai.p.${p}` as TextKey)}</strong>
              {p !== "none" && <span className={FREE.includes(p) ? "price price-free" : "price"}>{FREE.includes(p) ? t("ai.free") : t("ai.paid")}</span>}
              <span className="hint">{t(`ai.p.${p}.hint` as TextKey)}</span>
            </span>
          </label>
        ))}
      </fieldset>

      {provider !== "none" && (
        <section className="source" aria-label={t(`ai.p.${provider}` as TextKey)}>
          {KEY_PAGES[provider] && (
            <div>
              <button type="button" className="btn btn-ghost btn-small" onClick={() => onOpenLink(KEY_PAGES[provider]!)}>
                {provider === "ollama" ? t("ai.getOllama") : t("ai.getKey")} <IconArrowOut />
              </button>
            </div>
          )}
          {keyName && (
            <SecretField name={keyName} label={t("ai.key")} hint={hints[keyName]} onSave={saveSecret} onDelete={deleteSecret} onReveal={api.revealSecret} />
          )}
          {provider === "ollama" && (
            <>
              <TextSetting label={t("ai.ollamaUrl")} value={settings.ollama_url} onSave={(v) => onChange("ollama_url", v)} />
              <div className="machine">
                <button type="button" className="btn btn-ghost btn-small" onClick={check} disabled={checking} aria-busy={checking}>
                  {checking ? t("ai.checking") : t("ai.check")}
                </button>
                {machine && <MachineView m={machine} />}
              </div>
            </>
          )}
          <div className="mail-grid">
            <TextSetting
              label={t("ai.model")}
              hint={status?.defaultModel ? t("ai.modelHint", { model: status.defaultModel }) : undefined}
              value={settings.ai_model}
              placeholder={status?.defaultModel}
              onSave={(v) => onChange("ai_model", v)}
            />
            <TextSetting
              label={t("ai.limit")}
              hint={t("ai.limitHint")}
              value={settings.ai_daily_limit}
              inputMode="numeric"
              onSave={(v) => onChange("ai_daily_limit", v)}
            />
          </div>
          {status && !status.ready && (
            <p className="source-warn">
              <IconWarning /> {t("ai.noKey")}
            </p>
          )}
          {status && (
            <p className="source-state">
              {t("ai.usage", { today: status.callsToday, limit: status.dailyLimit, month: status.callsMonth })}{" "}
              {t("ai.counts", { rated: status.rated, waiting: status.waiting })}
            </p>
          )}
          <div className="source-row">
            <label className="choice-item">
              <input type="checkbox" checked={settings.ai_auto === "on"} onChange={(e) => onChange("ai_auto", e.target.checked ? "on" : "off")} />
              <span>{t("ai.auto")}</span>
            </label>
            <button type="button" className="btn btn-ghost btn-small" onClick={rateNow} disabled={rating || !status?.ready} aria-busy={rating}>
              {rating ? t("ai.rating") : t("ai.rateNow")}
            </button>
          </div>
        </section>
      )}

      <label className="field field-inline">
        <span>{t("ai.hideBelow")}</span>
        <select value={settings.hide_below} onChange={(e) => onChange("hide_below", e.target.value as Settings["hide_below"])}>
          {(["off", "30", "40", "50", "60", "70"] as const).map((v) => (
            <option key={v} value={v}>
              {v === "off" ? t("ai.hideOff") : t("ai.hideValue", { value: v })}
            </option>
          ))}
        </select>
        <span className="hint">{t("ai.hideHint")}</span>
      </label>

      <p className="hint">{t("ai.privacy")}</p>
      <div className="warn-box" role="note">
        <IconWarning />
        <p>
          <strong>{t("mail.warnTitle")}: </strong>
          {t("ai.noPersonal")}
        </p>
      </div>
    </div>
  );
}

function MachineView({ m }: { m: Machine }) {
  const { t } = useI18n();
  const gb = (mb: number) => (mb / 1024).toFixed(mb >= 10240 ? 0 : 1).replace(".", ",");
  return (
    <div className="machine-result" role="status">
      <p>
        {t("ai.ram", { gb: gb(m.ramMb) })}
        {m.gpus.length === 0
          ? ` · ${t("ai.noGpu")}`
          : m.gpus.map((g) => ` · ${g.name}${g.vramMb > 0 ? ` (${t("ai.vram", { gb: gb(g.vramMb) })})` : ""}`).join("")}
      </p>
      <p className={m.verdict === "no" ? "source-warn" : undefined}>
        <strong>{t(`ai.verdict.${m.verdict}` as TextKey)}</strong>
        {m.model && ` ${t("ai.suggestModel", { model: m.model })}`}
      </p>
    </div>
  );
}
