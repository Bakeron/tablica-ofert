import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createApi, type Api } from "./api";
import {
  isAppError,
  type AppStatus,
  type Interest,
  type OfferDetail,
  type OfferInput,
  type OfferRow,
  type Settings,
  type Stage,
  type View,
} from "./api/types";
import { AddOfferDialog } from "./components/AddOfferDialog";
import { Board, sortRows, type Sort, type SortKey } from "./components/Board";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { IconBriefcase, IconGear, IconPlus, IconWarning } from "./components/Icons";
import { OfferCard } from "./components/OfferCard";
import { SettingsDialog } from "./components/SettingsDialog";
import { I18nContext, plural, resolveLang, translate, type TextKey } from "./i18n";
import { isWebUrl } from "./lib/format";

const DEFAULT_SETTINGS: Settings = { theme: "system", language: "system", motion: "system", sound: "off" };
const VIEWS: View[] = ["board", "rejected", "trash"];

interface Toast {
  text: string;
  undo?: () => void;
}

export default function App() {
  const [api, setApi] = useState<Api | null>(null);
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [view, setView] = useState<View>("board");
  const [rows, setRows] = useState<OfferRow[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [detail, setDetail] = useState<OfferDetail | null>(null);
  const [sort, setSort] = useState<Sort>({ key: "added", dir: "desc" });
  const [freshIds, setFreshIds] = useState<Set<number>>(new Set());
  const [toast, setToast] = useState<Toast | null>(null);
  const [error, setError] = useState<{ text: string; detail?: string } | null>(null);
  const [adding, setAdding] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [confirm, setConfirm] = useState<{ message: string; action: () => void } | null>(null);
  const cardHeading = useRef<HTMLHeadingElement>(null);

  // ----- language, theme, motion -----
  const lang = resolveLang(settings.language, navigator.language);
  const i18n = useMemo(
    () => ({ lang, t: (key: TextKey, params?: Record<string, string | number>) => translate(lang, key, params) }),
    [lang],
  );
  const { t } = i18n;

  useEffect(() => {
    const root = document.documentElement;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const motionMedia = window.matchMedia("(prefers-reduced-motion: reduce)");
    const apply = () => {
      root.dataset.theme = settings.theme === "system" ? (media.matches ? "dark" : "light") : settings.theme;
      root.dataset.motion =
        settings.motion === "system" ? (motionMedia.matches ? "reduced" : "full") : settings.motion;
      root.lang = lang;
    };
    apply();
    media.addEventListener("change", apply);
    motionMedia.addEventListener("change", apply);
    return () => {
      media.removeEventListener("change", apply);
      motionMedia.removeEventListener("change", apply);
    };
  }, [settings.theme, settings.motion, lang]);

  // ----- errors -----
  const fail = useCallback(
    (e: unknown) => {
      if (isAppError(e)) {
        const key = (["locked", "invalid", "not_found"].includes(e.code) ? `error.${e.code}` : "error.generic") as TextKey;
        setError({ text: t(key), detail: e.detail });
      } else {
        setError({ text: t("error.generic"), detail: String(e) });
      }
    },
    [t],
  );

  // ----- loading -----
  useEffect(() => {
    createApi().then(async (a) => {
      setApi(a);
      const s = await a.appStatus();
      setStatus(s);
      if (s.ok) setSettings(await a.getSettings());
    }, fail);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const reload = useCallback(
    async (keepSelection = true) => {
      if (!api || !status?.ok) return;
      try {
        const list = await api.listOffers(view);
        setRows(list);
        setSelectedId((id) => (keepSelection && id !== null && list.some((r) => r.id === id) ? id : (list[0]?.id ?? null)));
      } catch (e) {
        fail(e);
      }
    },
    [api, status, view, fail],
  );

  useEffect(() => {
    reload(false);
  }, [reload]);

  useEffect(() => {
    if (!api || selectedId === null) {
      setDetail(null);
      return;
    }
    let current = true;
    api.getOffer(selectedId).then((d) => current && setDetail(d), fail);
    return () => {
      current = false;
    };
  }, [api, selectedId, rows, fail]);

  useEffect(() => {
    if (!toast) return;
    const timer = setTimeout(() => setToast(null), toast.undo ? 8000 : 4000);
    return () => clearTimeout(timer);
  }, [toast]);

  const sorted = useMemo(() => sortRows(rows, sort), [rows, sort]);

  // ----- actions -----
  async function run(action: () => Promise<unknown>, after?: () => void) {
    if (!api) return;
    try {
      await action();
      await reload();
      after?.();
    } catch (e) {
      fail(e);
    }
  }

  const onSort = (key: SortKey) =>
    setSort((s) => (s.key === key ? { key, dir: s.dir === "asc" ? "desc" : "asc" } : { key, dir: key === "offer" || key === "place" ? "asc" : "desc" }));

  const setInterest = (id: number, interest: Interest | null) => run(() => api!.setInterest(id, interest));
  const toggleInterest = (id: number, interest: Interest) => {
    const row = rows.find((r) => r.id === id);
    setInterest(id, row?.interest === interest ? null : interest);
  };

  const trash = (id: number) => {
    // Keep the keyboard flow: the next row (or the previous one at the end) becomes selected.
    const i = sorted.findIndex((r) => r.id === id);
    const neighbour = sorted[i + 1] ?? sorted[i - 1];
    if (id === selectedId) setSelectedId(neighbour?.id ?? null);
    return run(
      () => api!.trashOffer(id),
      () =>
        setToast({
          text: t("trash.done"),
          undo: () => run(() => api!.restoreOffer(id)),
        }),
    );
  };

  async function addOffer(offer: OfferInput): Promise<boolean> {
    if (!api) return false;
    try {
      const outcome = await api.addOffer(offer);
      if (outcome.status === "blocked") {
        setToast({ text: t("add.blocked") });
        return true;
      }
      setToast({ text: t(outcome.status === "added" ? "add.added" : "add.merged") });
      if (view !== "board") setView("board");
      setFreshIds(new Set([outcome.id]));
      await reload();
      setSelectedId(outcome.id);
      return true;
    } catch (e) {
      fail(e);
      return false;
    }
  }

  function changeSetting<K extends keyof Settings>(key: K, value: Settings[K]) {
    setSettings((s) => ({ ...s, [key]: value }));
    api?.setSetting(key, value).catch(fail);
  }

  function openLink(url: string) {
    if (!isWebUrl(url)) return;
    if ("__TAURI_INTERNALS__" in window) {
      import("@tauri-apps/plugin-opener").then((m) => m.openUrl(url)).catch(fail);
    } else {
      window.open(url, "_blank", "noopener,noreferrer");
    }
  }

  const locked = status !== null && !status.ok;

  return (
    <I18nContext.Provider value={i18n}>
      <div className="app">
        <header className="topbar">
          <div className="brand">
            <span className="brand-mark">
              <IconBriefcase />
            </span>
            <h1>{t("app.name")}</h1>
          </div>
          <Clock label={t("app.clock")} lang={lang} />
          <nav className="tabs" aria-label={t("tabs.label")}>
            {VIEWS.map((v) => (
              <button key={v} type="button" className="tab" aria-current={view === v ? "page" : undefined} onClick={() => setView(v)}>
                {t(`tabs.${v}`)}
              </button>
            ))}
          </nav>
          <div className="topbar-actions">
            {view === "trash" && rows.length > 0 && (
              <button
                type="button"
                className="btn btn-danger"
                onClick={() =>
                  setConfirm({
                    message: t("trash.confirmAll", { count: rows.length }),
                    action: () => run(() => api!.purgeOffers(null), () => setToast({ text: t("trash.purged") })),
                  })
                }
              >
                {t("trash.empty")}
              </button>
            )}
            <button type="button" className="btn btn-sign" onClick={() => setAdding(true)} disabled={locked}>
              <IconPlus /> {t("add.button")}
            </button>
            <button
              type="button"
              className="icon-btn"
              onClick={() => setSettingsOpen(true)}
              aria-label={t("settings.button")}
              title={t("settings.button")}
              disabled={locked}
            >
              <IconGear />
            </button>
          </div>
        </header>

        {status?.version.endsWith("preview") && <p className="preview-badge">{t("preview.badge")}</p>}

        {(locked || error) && (
          <div className="alert" role="alert">
            <IconWarning />
            <div>
              <strong>{t("error.title")}: </strong>
              {locked ? t("error.locked") : error?.text}
              {(locked ? status?.problem : error?.detail) && (
                <details>
                  <summary>{t("error.details")}</summary>
                  <code>{locked ? status?.problem : error?.detail}</code>
                </details>
              )}
            </div>
            {!locked && (
              <button type="button" className="icon-btn" onClick={() => setError(null)} aria-label={t("error.dismiss")}>
                ×
              </button>
            )}
          </div>
        )}

        <main className="split">
          <section className="board-panel" aria-labelledby="board-heading">
            <div className="board-head">
              <h2 id="board-heading">
                {t(`tabs.${view}`)} <span className="count">· {t(plural(lang, "board.count", rows.length), { count: rows.length })}</span>
              </h2>
              <p className="keys-hint">{t("board.keys")}</p>
            </div>
            <Board
              rows={sorted}
              view={view}
              selectedId={selectedId}
              freshIds={freshIds}
              sort={sort}
              onSort={onSort}
              onSelect={setSelectedId}
              onOpen={() => cardHeading.current?.focus()}
              onInterest={toggleInterest}
              onTrash={trash}
            />
          </section>

          <OfferCard
            ref={cardHeading}
            offer={detail}
            onStage={(stage: Stage) => selectedId !== null && run(() => api!.setStage(selectedId, stage))}
            onInterest={(i) => selectedId !== null && setInterest(selectedId, i)}
            onFavorite={(f) => selectedId !== null && run(() => api!.setFavorite(selectedId, f))}
            onNotes={(n) => selectedId !== null && run(() => api!.setNotes(selectedId, n), () => setToast({ text: t("card.notesSaved") }))}
            onOpenLink={openLink}
            onTrash={() => selectedId !== null && trash(selectedId)}
            onRestore={() => selectedId !== null && run(() => api!.restoreOffer(selectedId))}
            onPurge={() =>
              selectedId !== null &&
              setConfirm({
                message: t("trash.confirmOne"),
                action: () => run(() => api!.purgeOffers(selectedId), () => setToast({ text: t("trash.purged") })),
              })
            }
          />
        </main>

        <div className="toast-zone" aria-live="polite">
          {toast && (
            <div className="toast">
              <span>{toast.text}</span>
              {toast.undo && (
                <button
                  type="button"
                  className="btn btn-ghost"
                  onClick={() => {
                    toast.undo?.();
                    setToast(null);
                  }}
                >
                  {t("trash.undo")}
                </button>
              )}
            </div>
          )}
        </div>

        <AddOfferDialog open={adding} onClose={() => setAdding(false)} onSave={addOffer} />
        <SettingsDialog open={settingsOpen} settings={settings} onClose={() => setSettingsOpen(false)} onChange={changeSetting} />
        <ConfirmDialog
          message={confirm?.message ?? null}
          confirmLabel={t("trash.confirm")}
          onConfirm={() => confirm?.action()}
          onClose={() => setConfirm(null)}
        />
      </div>
    </I18nContext.Provider>
  );
}

/** Station clock in the top bar - updates every 15 s. */
function Clock({ label, lang }: { label: string; lang: string }) {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const timer = setInterval(() => setNow(new Date()), 15_000);
    return () => clearInterval(timer);
  }, []);
  return (
    <time className="clock" dateTime={now.toISOString()} aria-label={label}>
      {now.toLocaleTimeString(lang === "pl" ? "pl-PL" : "en-GB", { hour: "2-digit", minute: "2-digit" })}
    </time>
  );
}
