import { useCallback, useContext, useEffect, useState } from "react";
import type { Api } from "../api";
import {
  AI_PROVIDERS,
  describeError,
  type AiCheck,
  type AiProblem,
  type AiProvider,
  type AiProviderStatus,
  type AiStatus,
  type ClaudeSpent,
  type LowFunds,
  type FreeModels,
  type Machine,
  type RateReport,
  type SecretHints,
  type SecretName,
  type Settings,
} from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { IconArrowOut, IconWarning } from "./Icons";
import { TEXT_MAX, LEARN_MAX_POINTS, LEARN_MIN_RATINGS } from "../lib/limits";
import { rememberUnfolded, settingsMemory } from "../lib/settingsMemory";
import { lowFundsLine, noFreeProvider, problemLine, rateSummary } from "../lib/aiProblem";
import { claudeCostText, localDay, plnText, topUpTyped, USD_PLN, usdText } from "../lib/aiCost";
import { fullDay } from "../lib/format";
import { Reveal } from "./Reveal";
import { ConfirmContext, SecretField } from "./SecretField";
import { SourceBox } from "./SourcesSettings";
import { TextSetting } from "./TextSetting";
import { RatingTest } from "./RatingTest";

/** Where to get a key - each provider's own page. */
const KEY_PAGES: Partial<Record<AiProvider, string>> = {
  gemini: "https://aistudio.google.com/apikey",
  groq: "https://console.groq.com/keys",
  openrouter: "https://openrouter.ai/keys",
  ollama: "https://ollama.com/download",
  anthropic: "https://platform.claude.com/settings/keys",
  openai: "https://platform.openai.com/api-keys",
};

const FREE: AiProvider[] = ["groq", "openrouter", "ollama"];

/** Steps of "How to get a key" (texts `ai.how.<provider>.<n>`); Ollama has its own steps. */
const KEY_STEPS: Partial<Record<AiProvider, number>> = { groq: 3, openrouter: 3, anthropic: 3, openai: 3, gemini: 3 };

const EVERY: Settings["ai_every"][] = ["30", "60", "180", "1440"];

/** Settings → "AI": every provider folded like the offer sources (on / off, key, model), asked in
 *  turn; the daily limit, AI on its own and how often, usage, "Check my computer". */
export function AiSettings({
  api,
  settings,
  onChange,
  onOpenLink,
  onError,
  onToast,
  onRated,
}: {
  api: Api;
  settings: Settings;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void | Promise<boolean>;
  onOpenLink: (url: string) => void;
  onError: (e: unknown) => void;
  onToast: (text: string) => void;
  /** New scores are in - the board behind this window shows them. */
  onRated: () => void;
}) {
  const { t, lang } = useI18n();
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [hints, setHints] = useState<SecretHints>({});
  const [machine, setMachine] = useState<Machine | null>(null);
  const [checking, setChecking] = useState(false);
  const [rating, setRating] = useState(false);
  // The last "Rate now" - its reason stays under the button, not only in a toast.
  const [report, setReport] = useState<RateReport | null>(null);

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
  }, [load, settings]);

  const turnedOn = settings.ai_providers.split(",").filter(Boolean);
  // The list keeps the order of AI_PROVIDERS - the order AI asks them in.
  const toggle = (p: AiProvider, on: boolean) =>
    onChange("ai_providers", AI_PROVIDERS.filter((x) => (x === p ? on : turnedOn.includes(x))).join(","));
  const of = (p: AiProvider) => status?.providers.find((x) => x.provider === p);
  const enabled = status?.providers.filter((x) => x.enabled) ?? [];
  const usedUp = enabled.filter((x) => x.usedUp);
  const canRate = enabled.some((x) => x.ready && !x.usedUp);
  // Nothing is waiting, so the same button sends everything to AI once more (e.g. a new model).
  const again = status !== null && status.waiting === 0 && status.rateAgain > 0;
  const confirm = useContext(ConfirmContext);

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

  async function rate(ask: () => ReturnType<Api["aiRateNow"]>) {
    setRating(true);
    setReport(null);
    try {
      const done = await ask();
      setReport(done);
      onToast(rateSummary(done, t, lang));
      await load();
      onRated();
    } catch (e) {
      onError(e);
    } finally {
      setRating(false);
    }
  }

  function rateNow() {
    if (again && status && confirm) {
      // Offers added by hand or rated more precisely once go to Claude (paid) - its rough cost is said before anything is sent.
      const claude = of("anthropic");
      const paid =
        claude?.enabled && status.rateAgainClaude > 0
          ? ` ${t("ai.rateAllAskClaude", { count: status.rateAgainClaude, cost: claudeCostText(status.rateAgainClaude, claude.model, lang) })}`
          : "";
      confirm(`${t("ai.rateAllAsk", { count: status.rateAgain })}${paid}`, () => void rate(() => api.aiRateAllAgain()), t("ai.rateAllAgain"));
    } else {
      void rate(() => api.aiRateNow());
    }
  }

  return (
    <div className="settings-part">
      <p>{t("ai.intro")}</p>

      <p className="hint">{t("ai.queue")}</p>

      {/* TEMPORARY - the author's blind rating for the AI test; removed after it. */}
      <RatingTest api={api} onOpenLink={onOpenLink} onError={onError} />

      {AI_PROVIDERS.map((p) => {
        const st = of(p);
        const keyName = p !== "ollama" ? (`ai-key-${p}` as SecretName) : null;
        return (
          <SourceBox
            key={p}
            id={`ai-${p}`}
            name={t(`ai.p.${p}` as TextKey)}
            short={
              FREE.includes(p) ? (
                `${t("ai.free")} - ${t(`ai.p.${p}.hint` as TextKey)}`
              ) : (
                <>
                  <strong className="source-paid">{t("ai.paid")}</strong> - {t(`ai.p.${p}.hint` as TextKey)}
                </>
              )
            }
            on={turnedOn.includes(p)}
            onToggle={(on) => toggle(p, on)}
            alert={st && <ProviderAlert st={st} limit={status?.dailyLimit ?? 0} />}
          >
            <p className="hint">{t(`ai.p.${p}.accuracy` as TextKey)}</p>
            {KEY_STEPS[p] ? (
              <KeySteps provider={p} steps={KEY_STEPS[p]!} open={st !== undefined && !st.ready} onOpenLink={onOpenLink} />
            ) : (
              KEY_PAGES[p] && (
                <div>
                  <button type="button" className="btn btn-ghost btn-small" onClick={() => onOpenLink(KEY_PAGES[p]!)}>
                    {p === "ollama" ? t("ai.getOllama") : t("ai.getKey")} <IconArrowOut />
                  </button>
                </div>
              )
            )}
            {keyName && (
              <SecretField name={keyName} label={t("ai.key")} hint={hints[keyName]} onSave={saveSecret} onDelete={deleteSecret} onReveal={api.revealSecret} />
            )}
            {p === "ollama" && (
              <>
                <ol className="ollama-steps">
                  <li>{t("ai.ollamaStep1")}</li>
                  <li>{t("ai.ollamaStep2")}</li>
                  <li>{t("ai.ollamaStep3")}</li>
                </ol>
                <OllamaInstall api={api} onError={onError} onToast={onToast} />
                <TextSetting label={t("ai.ollamaUrl")} maxLength={TEXT_MAX.ollama_url} value={settings.ollama_url} onSave={(v) => onChange("ollama_url", v)} />
                <div className="machine">
                  <button type="button" className="btn btn-ghost btn-small" onClick={check} disabled={checking} aria-busy={checking}>
                    {checking ? t("ai.checking") : t("ai.check")}
                  </button>
                  {machine && <MachineView m={machine} />}
                </div>
                <OllamaModels
                  api={api}
                  want={machine?.model ?? settings.ai_model_ollama ?? ""}
                  current={st?.model ?? ""}
                  onPick={(model) => onChange("ai_model_ollama", model)}
                  url={settings.ollama_url}
                  onReady={(model) => {
                    onChange("ai_model_ollama", model);
                    if (!turnedOn.includes("ollama")) toggle("ollama", true);
                    onToast(t("ai.ollamaPulled", { model }));
                  }}
                  onError={onError}
                />
              </>
            )}
            <TextSetting
              label={t("ai.model")}
              maxLength={TEXT_MAX.ai_model}
              hint={st ? t("ai.modelHint", { model: st.defaultModel }) : undefined}
              value={settings[`ai_model_${p}`]}
              placeholder={st?.defaultModel}
              onSave={(v) => onChange(`ai_model_${p}`, v)}
            />
            {FREE.includes(p) && p !== "ollama" && (
              <ModelPicker api={api} provider={p} current={st?.model ?? ""} onPick={(model) => onChange(`ai_model_${p}`, model)} />
            )}
            {p === "anthropic" && <TopUp settings={settings} spent={status?.claudeSpent ?? null} low={status?.claudeLow ?? null} onChange={onChange} />}
            <ConnectionCheck api={api} provider={p} ready={st?.ready ?? false} onChecked={load} />
            {st && (
              <p className="source-state">{t("ai.usage", { today: st.callsToday, limit: status?.dailyLimit ?? 0, month: st.callsMonth })}</p>
            )}
          </SourceBox>
        );
      })}

      <fieldset className="crit-group ai-common">
        <TextSetting
          label={t("ai.limit")}
          maxLength={TEXT_MAX.ai_daily_limit}
          hint={t("ai.limitHint")}
          value={settings.ai_daily_limit}
          inputMode="numeric"
          onSave={(v) => onChange("ai_daily_limit", v)}
        />
        <label className="choice-item">
          <input type="checkbox" checked={settings.ai_auto === "on"} onChange={(e) => onChange("ai_auto", e.target.checked ? "on" : "off")} />
          <span>{t("ai.auto")}</span>
        </label>
        <Reveal show={settings.ai_auto === "on"}>
          <label className="field field-inline">
            <span>{t("ai.every")}</span>
            <select value={settings.ai_every} onChange={(e) => onChange("ai_every", e.target.value as Settings["ai_every"])}>
              {EVERY.map((v) => (
                <option key={v} value={v}>
                  {t(`ai.every.${v}` as TextKey)}
                </option>
              ))}
            </select>
          </label>
        </Reveal>
        <p className="hint">{t(settings.ai_auto === "on" ? "ai.autoOnHint" : "ai.autoOffHint")}</p>
        <label className="choice-item">
          <input type="checkbox" checked={settings.learn_scores === "on"} onChange={(e) => onChange("learn_scores", e.target.checked ? "on" : "off")} />
          <span>{t("ai.learn")}</span>
        </label>
        <p className="hint">{t("ai.learnHint", { min: LEARN_MIN_RATINGS, max: LEARN_MAX_POINTS })}</p>
        {usedUp.length > 0 && (
          <p className="source-warn">
            <IconWarning />{" "}
            {t("ai.limitNow", {
              list: usedUp.map((x) => `${t(`ai.p.${x.provider}` as TextKey)} (${t(x.usedUp === "app" ? "ai.limitApp" : "ai.limitProvider")})`).join(", "),
            })}{" "}
            {t(usedUp.length === enabled.length ? "ai.limitAll" : "ai.limitNext")}
          </p>
        )}
        {status && <p className="source-state">{t("ai.counts", { rated: status.rated, waiting: status.waiting })}</p>}
        <div>
          <button type="button" className="btn btn-ghost btn-small" onClick={rateNow} disabled={rating || !canRate} aria-busy={rating}>
            {rating ? t("ai.rating") : t(again ? "ai.rateAllAgain" : "ai.rateNow")}
          </button>
        </div>
        <div aria-live="polite">
          <Reveal show={report !== null}>{report && <RunResult report={report} />}</Reveal>
        </div>
      </fieldset>

      <p className="hint">{t("ai.orderHint")}</p>
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

/**
 * What Ollama has (checked only on a click - nothing is asked before) and "Download model X" for the
 * suggested or entered model that is not there yet; after the download it is set and turned on.
 */
function OllamaModels({
  api,
  want,
  current,
  onPick,
  url,
  onReady,
  onError,
}: {
  api: Api;
  want: string;
  /** The model AI uses now (the default one when the field is empty). */
  current: string;
  onPick: (model: string) => void;
  url: string;
  onReady: (model: string) => void;
  onError: (e: unknown) => void;
}) {
  const { t } = useI18n();
  // null = not checked yet, "off" = Ollama does not answer.
  const [models, setModels] = useState<string[] | "off" | null>(null);
  const [busy, setBusy] = useState(false);
  const [looking, setLooking] = useState(false);
  // When Ollama last answered - a second click with the same models still shows it checked again.
  const [checkedAt, setCheckedAt] = useState("");
  const look = useCallback(async () => {
    setLooking(true);
    const found = await api.ollamaModels().catch(() => "off" as const);
    setModels(found);
    setCheckedAt(new Date().toTimeString().slice(0, 8));
    setLooking(false);
  }, [api]);
  // Another address - the old answer no longer counts.
  useEffect(() => {
    setModels(null);
  }, [url]);
  const has = (m: string) => Array.isArray(models) && models.some((x) => x === m || x === `${m}:latest`);

  async function pull() {
    setBusy(true);
    try {
      await api.ollamaPull(want);
      await look();
      onReady(want);
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="ollama-models" aria-live="polite">
      {models === "off" && (
        <p className="source-warn">
          <IconWarning /> {t("ai.ollamaOff")}
        </p>
      )}
      {Array.isArray(models) &&
        (models.length > 0 ? (
          <>
            <p className="source-ok">{t("ai.ollamaOn", { time: checkedAt })}</p>
            <ModelChips name="ollama" label={t("ai.ollamaPick")} models={models} current={current} onPick={onPick} />
          </>
        ) : (
          <p className="source-state">{t("ai.ollamaEmpty")}</p>
        ))}
      <div className="source-row">
        <button type="button" className="btn btn-ghost btn-small" onClick={look} disabled={busy || looking} aria-busy={looking}>
          {looking ? t("ai.checking") : t("ai.ollamaLook")}
        </button>
        {Array.isArray(models) && want && !has(want) && (
          <button type="button" className="btn btn-sign btn-small" onClick={pull} disabled={busy} aria-busy={busy}>
            {busy ? t("ai.ollamaPulling") : t("ai.ollamaPull", { model: want })}
          </button>
        )}
      </div>
    </div>
  );
}

const INSTALL = "ollama-install";

function FolderChoice({
  label,
  value,
  onPick,
  onClear,
  disabled,
}: {
  label: string;
  value: string;
  onPick: () => void;
  onClear: () => void;
  disabled: boolean;
}) {
  const { t } = useI18n();
  return (
    <div className="folder-choice">
      <span className="folder-label">{label}</span>
      <span className="folder-path">{value || t("ai.installDefault")}</span>
      <div className="about-buttons">
        <button type="button" className="btn btn-ghost btn-small" onClick={onPick} disabled={disabled} aria-label={`${label}: ${t("ai.installPick")}`}>
          {t("ai.installPick")}
        </button>
        {value && (
          <button type="button" className="btn btn-ghost btn-small" onClick={onClear} disabled={disabled} aria-label={`${label}: ${t("ai.installClear")}`}>
            {t("ai.installClear")}
          </button>
        )}
      </div>
    </div>
  );
}

/** For those who would rather not download and set up Ollama by hand. */
function OllamaInstall({ api, onError, onToast }: { api: Api; onError: (e: unknown) => void; onToast: (text: string) => void }) {
  const { t } = useI18n();
  const confirm = useContext(ConfirmContext);
  const [program, setProgram] = useState("");
  const [models, setModels] = useState("");
  const [busy, setBusy] = useState(false);

  async function pick(set: (folder: string) => void) {
    try {
      const folder = await api.pickFolder();
      if (folder) set(folder);
    } catch (e) {
      onError(e);
    }
  }

  async function install() {
    setBusy(true);
    try {
      await api.ollamaInstall(program, models);
      onToast(t("ai.installDone"));
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  }

  return (
    <details
      className="howto ollama-install"
      open={settingsMemory.unfolded.has(INSTALL)}
      onToggle={(e) => rememberUnfolded(INSTALL, e.currentTarget.open)}
    >
      <summary>{t("ai.installTitle")}</summary>
      <p className="hint">{t("ai.installText")}</p>
      <FolderChoice label={t("ai.installProgram")} value={program} onPick={() => pick(setProgram)} onClear={() => setProgram("")} disabled={busy} />
      <FolderChoice label={t("ai.installModels")} value={models} onPick={() => pick(setModels)} onClear={() => setModels("")} disabled={busy} />
      <div>
        <button
          type="button"
          className="btn btn-sign btn-small"
          onClick={() => confirm?.(t("ai.installAsk"), () => void install(), t("ai.installGo"))}
          disabled={busy}
          aria-busy={busy}
        >
          {busy ? t("ai.installBusy") : t("ai.installRun")}
        </button>
      </div>
    </details>
  );
}

/** "How to get a key" with the link to the provider's page - unfolded while no key is saved. */
function KeySteps({
  provider,
  steps,
  open,
  onOpenLink,
}: {
  provider: AiProvider;
  steps: number;
  /** No key yet - the steps are the next thing to do. */
  open: boolean;
  onOpenLink: (url: string) => void;
}) {
  const { t } = useI18n();
  const id = `ai-how-${provider}`;
  return (
    <details className="howto" open={open || settingsMemory.unfolded.has(id)} onToggle={(e) => rememberUnfolded(id, e.currentTarget.open)}>
      <summary>{t("ai.howKey")}</summary>
      <div>
        <button type="button" className="btn btn-ghost btn-small" onClick={() => onOpenLink(KEY_PAGES[provider]!)}>
          {t("ai.getKey")} <IconArrowOut />
        </button>
      </div>
      <ol className="ollama-steps">
        {Array.from({ length: steps }, (_, i) => (
          <li key={i}>{t(`ai.how.${provider}.${i + 1}` as TextKey)}</li>
        ))}
      </ol>
    </details>
  );
}

/** What went wrong and what to do, in plain words; the provider's own (English) words only
 *  folded under "Technical details". */
function ProblemView({ problem, status }: { problem: AiProblem; status?: boolean }) {
  const { t } = useI18n();
  return (
    <div className="source-warn" role={status ? "status" : undefined}>
      <IconWarning />
      <div>
        {problemLine(problem, t)}
        {problem.message && (
          <details>
            <summary>{t("error.details")}</summary>
            <code>{problem.message}</code>
          </details>
        )}
      </div>
    </div>
  );
}

/** The last "Rate now": how many were rated and every provider that failed, with the reason. */
function RunResult({ report }: { report: RateReport }) {
  const { t, lang } = useI18n();
  const known = ["net", "auth", "limit", "missing", "busy"];
  return (
    <div className="ai-run">
      {noFreeProvider(report) ? (
        <>
          <p className="source-state">{t("ai.rated", { count: report.rated, waiting: report.waiting })}</p>
          <p className="source-warn">
            <IconWarning /> {t("ai.noFree")}
          </p>
        </>
      ) : report.error ? (
        <p className="source-warn">
          <IconWarning /> {t("ai.stopped")}
          {report.problems.length === 0 &&
            ` ${t((known.includes(report.error.code) ? `error.${report.error.code}` : "error.generic") as TextKey)}`}
        </p>
      ) : (
        <p className="source-state">{t("ai.rated", { count: report.rated, waiting: report.waiting })}</p>
      )}
      {report.problems.map((p) => (
        <ProblemView key={`${p.provider}-${p.kind}`} problem={p} />
      ))}
      {report.noProfile > 0 && <p className="source-state">{t("ai.noProfile", { count: report.noProfile })}</p>}
      {report.lowFunds && (
        <p className="source-warn">
          <IconWarning /> {lowFundsLine(report.lowFunds, t, lang)}
        </p>
      )}
    </div>
  );
}

/** The last top-up of the Claude account (in $, as Anthropic counts; about how much in PLN) and its
 *  day - the spending counter counts from that day on. A sum without a day gets today. Below:
 *  spent of it and what is left, today and this month. */
function TopUp({
  settings,
  spent,
  low,
  onChange,
}: {
  settings: Settings;
  spent: ClaudeSpent | null;
  low: LowFunds | null;
  onChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void | Promise<boolean>;
}) {
  const { t, lang } = useI18n();
  const usd = settings.ai_topup_usd;
  const shown = lang === "pl" ? usd.replace(".", ",") : usd;
  const today = localDay();
  return (
    <div className="ai-topup">
      <TextSetting
        label={t("ai.topup")}
        // Room for a typed "$" or "USD", taken off before saving.
        maxLength={TEXT_MAX.ai_topup_usd + 4}
        hint={usd ? t("ai.topupPln", { pln: plnText(Number(usd), lang), rate: USD_PLN }) : t("ai.topupHint")}
        value={shown}
        placeholder={lang === "pl" ? "5,00" : "5.00"}
        inputMode="decimal"
        onSave={async (text) => {
          const v = topUpTyped(text);
          if (v === null) return t("ai.topupWrong");
          const saved = await onChange("ai_topup_usd", v);
          if (saved !== false && v && !settings.ai_topup_date) await onChange("ai_topup_date", today);
          return saved;
        }}
      />
      <label className="field">
        <span>{t("ai.topupDate")}</span>
        <input
          type="date"
          max={today}
          value={settings.ai_topup_date}
          onChange={(e) => {
            const v = e.target.value;
            if (v === "" || /^\d{4}-\d{2}-\d{2}$/.test(v)) onChange("ai_topup_date", v);
          }}
        />
        <span className="hint">{t("ai.topupDateHint")}</span>
      </label>
      {spent && (
        <div className="ai-spent" aria-live="polite">
          {usd && spent.sinceTopUp !== null && (
            <p className="source-state">
              {t("ai.spent", {
                day: fullDay(settings.ai_topup_date, lang),
                spent: usdText(spent.sinceTopUp, lang),
                topUp: usdText(Number(usd), lang),
                left: usdText(Number(usd) - spent.sinceTopUp, lang),
                pln: plnText(Math.max(0, Number(usd) - spent.sinceTopUp), lang),
              })}
            </p>
          )}
          {low && (
            <p className="source-warn">
              <IconWarning /> {lowFundsLine(low, t, lang)} {t("ai.lowFundsHere")}
            </p>
          )}
          <p className="source-state">{t("ai.spentPeriods", { today: usdText(spent.today, lang), month: usdText(spent.month, lang) })}</p>
          <p className="hint">{t("ai.spentHint")}</p>
        </div>
      )}
    </div>
  );
}

/** "Check connection": one tiny question with the model from the settings. */
function ConnectionCheck({ api, provider, ready, onChecked }: { api: Api; provider: AiProvider; ready: boolean; onChecked: () => void }) {
  const { t } = useI18n();
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<AiCheck | null>(null);
  const [failed, setFailed] = useState<string | null>(null);

  async function check() {
    setBusy(true);
    setResult(null);
    setFailed(null);
    try {
      setResult(await api.aiCheck(provider));
      // The call counts in today's usage, and a working provider loses its "limit" mark.
      onChecked();
    } catch (e) {
      setFailed(t(describeError(e).key as TextKey));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="source-check">
      <button type="button" className="btn btn-ghost btn-small" onClick={check} disabled={busy || !ready} aria-busy={busy}>
        {busy ? t("ai.testing") : t("ai.test")}
      </button>
      {result?.problem ? (
        <ProblemView problem={result.problem} status />
      ) : result ? (
        <p className="source-ok" role="status">
          {t("ai.testOk", { model: result.model })}
        </p>
      ) : failed ? (
        <p className="source-warn" role="status">
          <IconWarning /> {failed}
        </p>
      ) : busy && provider === "ollama" ? (
        <p className="hint" role="status">
          {t("ai.testingOllama")}
        </p>
      ) : (
        <p className="hint" role="status">
          {t("ai.testHint")}
        </p>
      )}
    </div>
  );
}

/** Model names to choose with one click; the one AI uses now is ticked. */
function ModelChips({ name, label, models, current, onPick }: { name: string; label: string; models: string[]; current: string; onPick: (model: string) => void }) {
  return (
    <fieldset className="model-chips">
      <legend>{label}</legend>
      <div className="chips">
        {models.map((m) => (
          <label key={m} className="chip">
            <input type="radio" name={`model-${name}`} checked={m === current} onChange={() => onPick(m)} />
            <span>{m}</span>
          </label>
        ))}
      </div>
    </fieldset>
  );
}

/** Free models of a provider - asked only on a click (its own list with the key, or the saved one). */
function ModelPicker({ api, provider, current, onPick }: { api: Api; provider: AiProvider; current: string; onPick: (model: string) => void }) {
  const { t } = useI18n();
  const [list, setList] = useState<FreeModels | null>(null);
  const [busy, setBusy] = useState(false);

  async function look() {
    setBusy(true);
    try {
      setList(await api.aiFreeModels(provider));
    } catch {
      setList({ models: [], fromProvider: false, problem: null });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="model-picker" aria-live="polite">
      <div>
        <button type="button" className="btn btn-ghost btn-small" onClick={look} disabled={busy} aria-busy={busy}>
          {busy ? t("ai.freeModelsFetching") : t("ai.freeModelsFetch")}
        </button>
      </div>
      {list && (
        <>
          {list.problem ? (
            <p className="source-warn">
              <IconWarning /> {t("ai.freeModelsFailed", { why: t(`ai.problem.${list.problem.kind}` as TextKey) })}
            </p>
          ) : (
            <p className="hint">{t(list.fromProvider ? "ai.freeModelsLive" : "ai.freeModelsSaved")}</p>
          )}
          {list.models.length > 0 && <ModelChips name={provider} label={t("ai.freeModels")} models={list.models} current={current} onPick={onPick} />}
        </>
      )}
    </div>
  );
}

/** A provider that is on but cannot be asked now: no key, or its daily limit is used. */
function ProviderAlert({ st, limit }: { st: AiProviderStatus; limit: number }) {
  const { t } = useI18n();
  if (!st.enabled) return null;
  if (!st.ready) {
    return (
      <p className="source-warn">
        <IconWarning /> {t("ai.noKey")}
      </p>
    );
  }
  if (!st.usedUp) return null;
  return (
    <p className="source-warn">
      <IconWarning /> {st.usedUp === "app" ? t("ai.usedUpApp", { limit }) : t("ai.usedUpProvider", { name: t(`ai.p.${st.provider}` as TextKey) })}
    </p>
  );
}

function MachineView({ m }: { m: Machine }) {
  const { t, lang } = useI18n();
  const gb = (mb: number) => (mb / 1024).toFixed(mb >= 10240 ? 0 : 1).replace(".", lang === "pl" ? "," : ".");
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
