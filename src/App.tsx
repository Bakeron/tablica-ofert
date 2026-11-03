import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createApi, type Api } from "./api";
import {
  DEFAULT_SETTINGS,
  isAppError,
  type AppStatus,
  type AssistKind,
  type Captured,
  type Draft,
  type Criteria,
  type FetchReport,
  type RateReport,
  type Interest,
  type OfferDetail,
  type OfferInput,
  type OfferRow,
  type Profile,
  type ProfileInput,
  type Settings,
  type Stage,
  type View,
  type ViewCounts,
} from "./api/types";
import { AddOfferDialog } from "./components/AddOfferDialog";
import { AssistDialog } from "./components/AiDialogs";
import { Board, sortRows, type Sort, type SortKey } from "./components/Board";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { FirstSteps } from "./components/FirstSteps";
import { ToastContext, ToastView, type Toast } from "./components/Toast";
import { IconBriefcase, IconDownload, IconGear, IconPlus, IconWarning } from "./components/Icons";
import { OfferCard } from "./components/OfferCard";
import { ProfileDialog } from "./components/ProfileDialog";
import { ProfileTabs } from "./components/ProfileTabs";
import { Reveal } from "./components/Reveal";
import { SettingsDialog } from "./components/SettingsDialog";
import { I18nContext, plural, resolveLang, translate, type TextKey } from "./i18n";
import { playClack, playGong } from "./lib/sound";
import { isPhone, scheduleReminders } from "./lib/platform";
import { isWebUrl } from "./lib/format";
import { viewFor } from "./lib/profile";

const VIEWS: View[] = ["board", "review", "rejected", "declined", "trash"];

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
  const [confirm, setConfirm] = useState<{ message: string; action: () => void; label?: string } | null>(null);
  const [profiles, setProfiles] = useState<Profile[]>([]);
  /** null = all profiles together */
  const [profileId, setProfileId] = useState<number | null>(null);
  const [counts, setCounts] = useState<ViewCounts | null>(null);
  /** Open profile window: `profile = null` - a new one. */
  const [editing, setEditing] = useState<{ profile: Profile | null } | null>(null);
  const [experience, setExperience] = useState("");
  const [firstSteps, setFirstSteps] = useState(false);
  /** Offer to select once the list of a newly opened tab has loaded. */
  const [pendingSelect, setPendingSelect] = useState<number | null>(null);
  const [fetching, setFetching] = useState(false);
  /** Show offers hidden by "hide offers scored below X" (until the app is closed). */
  const [showLow, setShowLow] = useState(false);
  const [experiencePrev, setExperiencePrev] = useState<string | null>(null);
  const [assist, setAssist] = useState<{ kind: AssistKind; text: string; busy: boolean } | null>(null);
  /** The preview window shows the page of this offer ("Fill in the full description"). */
  const [fullFor, setFullFor] = useState<{ id: number; title: string } | null>(null);
  const [capturing, setCapturing] = useState(false);
  /** "Share" from the phone's browser - opens "Add offer" with the shared text. */
  const [shared, setShared] = useState<{ id: number; text: string } | null>(null);

  useEffect(() => {
    const onShare = (e: Event) => {
      const text = (e as CustomEvent<unknown>).detail;
      if (typeof text !== "string" || !text.trim()) return;
      setShared({ id: Date.now(), text: text.slice(0, 100_000) });
      setAdding(true);
    };
    window.addEventListener("tablica-share", onShare);
    // A share that started the app arrives before React - it waits in a global.
    const early = (window as unknown as { __tablicaShared?: unknown }).__tablicaShared;
    if (typeof early === "string") onShare(new CustomEvent("tablica-share", { detail: early }));
    return () => window.removeEventListener("tablica-share", onShare);
  }, []);
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
        const known = ["locked", "invalid", "not_found", "net", "auth", "limit", "missing", "busy"];
        const key = (known.includes(e.code) ? `error.${e.code}` : "error.generic") as TextKey;
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
      if (!s.ok) return;
      const [loaded, list] = await Promise.all([a.getSettings(), a.listProfiles()]);
      setSettings(loaded);
      setProfiles(list);
      setFirstSteps(loaded.onboarding === "pending" && list.length === 0);
    }, fail);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const reload = useCallback(
    async (keepSelection = true) => {
      if (!api || !status?.ok) return;
      try {
        const [list, numbers] = await Promise.all([api.listOffers(view, profileId, showLow), api.countOffers(profileId)]);
        setRows(list);
        setCounts(numbers);
        setSelectedId((id) => (keepSelection && id !== null && list.some((r) => r.id === id) ? id : (list[0]?.id ?? null)));
      } catch (e) {
        fail(e);
      }
    },
    [api, status, view, profileId, showLow, fail],
  );

  useEffect(() => {
    reload(false);
  }, [reload]);

  useEffect(() => {
    if (pendingSelect !== null && rows.some((r) => r.id === pendingSelect)) {
      setSelectedId(pendingSelect);
      setPendingSelect(null);
    }
  }, [rows, pendingSelect]);

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

  // A message stays until it is closed ("X" or Esc) - a new one takes its place.
  const closeToast = useCallback(() => setToast(null), []);
  const toastValue = useMemo(() => ({ toast, close: closeToast }), [toast, closeToast]);
  useEffect(() => {
    if (!toast) return;
    const onKey = (e: KeyboardEvent) => {
      // Esc belongs to an open window or "?" bubble first.
      if (e.key !== "Escape" || document.querySelector("dialog[open], .help-tip:popover-open")) return;
      setToast(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toast]);

  // Offers collected in the background (at start and every few hours) - refresh the board.
  const reloadRef = useRef(reload);
  reloadRef.current = reload;
  const soundRef = useRef(false);
  soundRef.current = settings.sound === "on";
  useEffect(() => {
    if (!api) return;
    let stop: (() => void) | undefined;
    let alive = true;
    api
      .onFetched((report) => {
        if (report.added > 0) {
          reloadRef.current();
          setToast({ text: fetchSummary(report, t) });
          if (soundRef.current) playClack();
        }
      })
      .then((unlisten) => (alive ? (stop = unlisten) : unlisten()));
    let stopReminder: (() => void) | undefined;
    api
      .onReminder(() => {
        if (soundRef.current) playGong();
      })
      .then((unlisten) => (alive ? (stopReminder = unlisten) : unlisten()));
    let stopRated: (() => void) | undefined;
    api
      .onRated((report) => {
        reloadRef.current();
        if (report.error) setToast({ text: rateSummary(report, t) });
      })
      .then((unlisten) => (alive ? (stopRated = unlisten) : unlisten()));
    return () => {
      alive = false;
      stop?.();
      stopRated?.();
      stopReminder?.();
    };
  }, [api, t]);

  async function rateAgain(id: number) {
    if (!api) return;
    setToast({ text: t("ai.working") });
    try {
      const report = await api.aiRateOffer(id);
      await reload();
      setToast({ text: rateSummary(report, t) });
    } catch (e) {
      fail(e);
    }
  }

  async function runAssist(id: number, kind: AssistKind) {
    if (!api) return;
    setAssist({ kind, text: "", busy: true });
    try {
      const text = await api.aiAssist(id, kind);
      setAssist({ kind, text, busy: false });
    } catch (e) {
      setAssist(null);
      fail(e);
    }
  }

  async function openPreview(url: string): Promise<boolean> {
    if (!api) return false;
    try {
      await api.openPreview(url);
      return true;
    } catch (e) {
      fail(e);
      return false;
    }
  }

  async function capturePage(): Promise<Captured | null> {
    if (!api) return null;
    try {
      return await api.capturePreview();
    } catch (e) {
      fail(e);
      return null;
    }
  }

  async function fillOffer(text: string, title: string | null): Promise<Draft | null> {
    if (!api) return null;
    try {
      return await api.fillOffer(text, title);
    } catch (e) {
      fail(e);
      return null;
    }
  }

  async function startFullText(offer: OfferDetail) {
    if (offer.url && (await openPreview(offer.url))) setFullFor({ id: offer.id, title: offer.title });
  }

  async function takeFullText() {
    if (!api || !fullFor) return;
    setCapturing(true);
    const page = await capturePage();
    setCapturing(false);
    if (!page) return;
    try {
      const changed = await api.updateDescription(fullFor.id, page.text);
      await reload();
      setToast({ text: changed ? t("full.saved") : t("full.notLonger") });
      setFullFor(null);
      api.closePreview().catch(() => {});
    } catch (e) {
      fail(e);
    }
  }

  async function suggestCriteria(description: string): Promise<Criteria | null> {
    if (!api) return null;
    try {
      return await api.aiSuggestCriteria(description);
    } catch (e) {
      fail(e);
      return null;
    }
  }

  async function tidyText(text: string): Promise<string | null> {
    if (!api) return null;
    try {
      return await api.aiTidy(text);
    } catch (e) {
      fail(e);
      return null;
    }
  }

  async function fetchNow() {
    if (!api || fetching) return;
    setFetching(true);
    try {
      const report = await api.fetchOffers();
      await reload();
      setToast({ text: fetchSummary(report, t) });
      if (report.added > 0 && settings.sound === "on") playClack();
    } catch (e) {
      fail(e);
    } finally {
      setFetching(false);
    }
  }

  // Phone: reminders planned with the system's own notifications (a day and an hour before).
  useEffect(() => {
    if (!api || !isPhone() || settings.notify_reminders !== "on") return;
    api
      .upcomingEvents()
      .then((list) =>
        scheduleReminders(
          list.flatMap((e) => {
            const at = new Date(e.at);
            const what = `${e.title} - ${e.company}`;
            const when = at.toLocaleString(lang === "pl" ? "pl-PL" : "en-GB", { weekday: "short", hour: "2-digit", minute: "2-digit" });
            const interview = e.kind === "interview";
            return [
              { id: e.id * 2, at: new Date(at.getTime() - 24 * 3_600_000), title: t(interview ? "phone.tomorrowInterview" : "phone.tomorrowDate"), body: `${what}, ${when}` },
              { id: e.id * 2 + 1, at: new Date(at.getTime() - 3_600_000), title: t(interview ? "phone.hourInterview" : "phone.hourDate"), body: `${what}, ${when}` },
            ];
          }),
        ),
      )
      .catch(() => {});
  }, [api, rows, settings.notify_reminders, lang, t]);

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

  /** Like `run`, but tells the form whether it worked (so it can close). */
  async function act(action: () => Promise<unknown>): Promise<boolean> {
    if (!api) return false;
    try {
      await action();
      await reload();
      return true;
    } catch (e) {
      fail(e);
      return false;
    }
  }

  const onSort = (key: SortKey) =>
    setSort((s) => (s.key === key ? { key, dir: s.dir === "asc" ? "desc" : "asc" } : { key, dir: key === "offer" || key === "place" ? "asc" : "desc" }));

  /** Interest changes can move the offer to another tab ("very" from "To check" to "Offers", "no" to
   *  "Not for me" and back) - then the next row gets selected and a message names the new tab. */
  const setInterest = (id: number, next: Interest | null) => {
    const row = rows.find((r) => r.id === id);
    const target = row ? viewFor({ ...row, interest: next }) : view;
    if (!row || target === view) return run(() => api!.setInterest(id, next));
    const before = row.interest;
    selectNeighbour(id);
    return run(
      () => api!.setInterest(id, next),
      () =>
        setToast({
          text: t("board.movedTo", { tab: t(`tabs.${target}`) }),
          undo: () => run(() => api!.setInterest(id, before)),
        }),
    );
  };
  const toggleInterest = (id: number, interest: Interest) => {
    const row = rows.find((r) => r.id === id);
    return setInterest(id, row?.interest === interest ? null : interest);
  };

  /** The row is about to leave the list: the next one (or the previous one at the end) becomes selected. */
  function selectNeighbour(id: number) {
    const i = sorted.findIndex((r) => r.id === id);
    const neighbour = sorted[i + 1] ?? sorted[i - 1];
    if (id === selectedId) setSelectedId(neighbour?.id ?? null);
  }

  const trash = (id: number) => {
    selectNeighbour(id);
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
      // Show the offer in the tab where the filter put it (all profiles together).
      const added = await api.getOffer(outcome.id);
      const target = viewFor(added);
      const text =
        outcome.status === "merged"
          ? t("add.merged")
          : outcome.status === "unchanged"
            ? t("add.known")
            : target === "board"
            ? t("add.added")
            : t(target === "review" ? "add.landed.review" : "add.landed.rejected");
      setToast({ text });
      setFreshIds(new Set([outcome.id]));
      if (view !== target || profileId !== null) {
        // The list reloads for the new tab; the offer gets selected once it is there.
        setPendingSelect(outcome.id);
        setProfileId(null);
        setView(target);
      } else {
        await reload();
        setSelectedId(outcome.id);
      }
      return true;
    } catch (e) {
      fail(e);
      return false;
    }
  }

  async function saveProfile(input: ProfileInput, newExperience: string): Promise<boolean> {
    if (!api) return false;
    try {
      await api.saveProfile(input);
      if (newExperience.trim() !== experience.trim()) {
        await api.setExperience(newExperience);
        setExperience(newExperience.trim());
      }
      setProfiles(await api.listProfiles());
      await reload();
      setToast({ text: t("profile.saved") });
      return true;
    } catch (e) {
      fail(e);
      return false;
    }
  }

  function deleteProfile(profile: Profile) {
    setConfirm({
      message: t("profile.deleteConfirm", { name: profile.name }),
      action: async () => {
        if (!api) return;
        try {
          await api.deleteProfile(profile.id);
          setEditing(null);
          setProfiles(await api.listProfiles());
          if (profileId === profile.id) setProfileId(null);
          else await reload();
          setToast({ text: t("profile.deleted") });
        } catch (e) {
          fail(e);
        }
      },
    });
  }

  async function openProfile(profile: Profile | null) {
    if (!api) return;
    try {
      const [text, prev] = await Promise.all([api.getExperience(), api.getExperiencePrev()]);
      setExperience(text);
      setExperiencePrev(prev);
      setEditing({ profile });
    } catch (e) {
      fail(e);
    }
  }

  function finishFirstSteps() {
    setFirstSteps(false);
    changeSetting("onboarding", "done");
  }

  function changeSetting<K extends keyof Settings>(key: K, value: Settings[K]) {
    setSettings((s) => ({ ...s, [key]: value }));
    api?.setSetting(key, value).catch((e) => {
      fail(e);
      // Refused by Rust (wrong value) - show what is really saved.
      api.getSettings().then(setSettings, fail);
    });
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
      <ToastContext.Provider value={toastValue}>
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
                {counts && (v === "review" || v === "rejected" || v === "declined") && counts[v] > 0 && (
                  <span className="tab-count">{counts[v]}</span>
                )}
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
            <button type="button" className="btn btn-ghost btn-fetch" onClick={fetchNow} disabled={locked || fetching} aria-busy={fetching}>
              <IconDownload /> {fetching ? t("fetch.busy") : t("fetch.button")}
            </button>
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

        <Reveal show={locked || error !== null}>
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
        </Reveal>

        <Reveal show={fullFor !== null}>
          {fullFor && (
          <div className="preview-bar" role="region" aria-label={t("full.title")}>
            <p>
              <strong>{t("full.title")}:</strong> {fullFor.title}. {t("full.barHint")}
            </p>
            <button type="button" className="btn btn-sign btn-small" onClick={takeFullText} disabled={capturing} aria-busy={capturing}>
              {capturing ? t("quick.capturing") : t("quick.capture")}
            </button>
            <button
              type="button"
              className="btn btn-ghost btn-small"
              onClick={() => {
                setFullFor(null);
                api?.closePreview().catch(() => {});
              }}
            >
              {t("full.cancel")}
            </button>
          </div>
          )}
        </Reveal>

        <main className="split">
          <section className="board-panel" aria-labelledby="board-heading">
            {view !== "trash" && (
              <ProfileTabs
                profiles={profiles}
                current={profileId}
                onPick={setProfileId}
                onEdit={(p) => openProfile(p)}
                onAdd={() => openProfile(null)}
                disabled={locked}
              />
            )}
            <div className="board-head">
              <h2 id="board-heading">
                {t(`tabs.${view}`)}
                {view !== "trash" && profileId !== null && (
                  <span className="count"> · {profiles.find((p) => p.id === profileId)?.name}</span>
                )}{" "}
                <span className="count">· {t(plural(lang, "board.count", rows.length), { count: rows.length })}</span>
              </h2>
              {settings.feature_keys === "on" && <p className="keys-hint">{t("board.keys")}</p>}
            </div>
            {view === "board" && counts && (counts.hidden > 0 || showLow) && (
              <p className="hidden-note">
                {showLow ? t("board.lowShown") : t(plural(lang, "board.lowHidden", counts.hidden), { count: counts.hidden, value: settings.hide_below })}{" "}
                <button type="button" className="btn btn-ghost btn-small" onClick={() => setShowLow((v) => !v)}>
                  {showLow ? t("board.lowHide") : t("board.lowShow")}
                </button>
              </p>
            )}
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
            aiOn={settings.ai_provider !== "none"}
            onFullText={() => detail && startFullText(detail)}
            agenda={{
              addEvent: (event) => (selectedId === null ? Promise.resolve(false) : act(() => api!.addEvent(selectedId, event))),
              deleteEvent: (id) => run(() => api!.deleteEvent(id)),
              google: (id) => run(() => api!.calendarGoogle(id)),
              ics: (id) => api!.calendarIcs(id).then((path) => setToast({ text: t("agenda.icsSaved", { path }) }), fail),
              saveContact: (contact) => (selectedId === null ? Promise.resolve(false) : act(() => api!.saveContact(selectedId, contact))),
              deleteContact: (id) => run(() => api!.deleteContact(id)),
              addLog: (entry) => (selectedId === null ? Promise.resolve(false) : act(() => api!.addLog(selectedId, entry))),
              deleteLog: (id) => run(() => api!.deleteLog(id)),
              openLink,
            }}
            showDates={settings.feature_dates === "on"}
            showContacts={settings.feature_contacts === "on"}
            onRateAgain={() => selectedId !== null && rateAgain(selectedId)}
            onAssist={(kind) => selectedId !== null && runAssist(selectedId, kind)}
          />
        </main>

        <ToastView />

        <AddOfferDialog
          open={adding}
          shared={shared}
          aiOn={settings.ai_provider !== "none"}
          onClose={() => {
            setAdding(false);
            api?.closePreview().catch(() => {});
          }}
          onSave={addOffer}
          onOpenPreview={openPreview}
          onCapture={capturePage}
          onFill={fillOffer}
        />
        <ProfileDialog
          open={editing !== null}
          profile={editing?.profile ?? null}
          experience={experience}
          experiencePrev={experiencePrev}
          aiOn={settings.ai_provider !== "none"}
          onSuggest={suggestCriteria}
          onTidy={tidyText}
          onClose={() => setEditing(null)}
          onSave={saveProfile}
          onDelete={deleteProfile}
        />
        <FirstSteps open={firstSteps} onSaveProfile={saveProfile} onFinish={finishFirstSteps} />
        <SettingsDialog
          open={settingsOpen}
          api={api}
          settings={settings}
          profiles={profiles}
          onClose={() => setSettingsOpen(false)}
          onChange={changeSetting}
          onOpenLink={openLink}
          onError={fail}
          onToast={(text) => setToast({ text })}
          version={status?.version ?? ""}
          onConfirm={(message, action, label) => setConfirm({ message, action, label })}
          onLoaded={async () => {
            if (!api) return;
            setSettings(await api.getSettings());
            setProfiles(await api.listProfiles());
            await reload(false);
          }}
        />
        <AssistDialog
          open={assist !== null}
          title={assist ? t(`assist.${assist.kind}`) : ""}
          text={assist?.text ?? ""}
          busy={assist?.busy ?? false}
          onClose={() => setAssist(null)}
          onCopied={(ok) => setToast({ text: ok ? t("mail.copied") : t("mail.copyFailed") })}
        />
        <ConfirmDialog
          message={confirm?.message ?? null}
          confirmLabel={confirm?.label ?? t("trash.confirm")}
          onConfirm={() => confirm?.action()}
          onClose={() => setConfirm(null)}
        />
      </div>
      </ToastContext.Provider>
    </I18nContext.Provider>
  );
}

/** One sentence about a fetch: new offers, or why nothing came. */
export function fetchSummary(report: FetchReport, t: (key: TextKey, params?: Record<string, string | number>) => string): string {
  const failed = report.runs.filter((r) => r.error);
  const parts: string[] = [];
  if (report.added > 0) {
    parts.push(t("fetch.new", { count: report.added, board: report.newOnBoard, review: report.newReview, rejected: report.newRejected }));
  }
  if (failed.length > 0) {
    const names = failed.map((r) => t(`source.${r.source}` as TextKey)).join(", ");
    const code = failed[0].error?.code;
    const known = ["net", "auth", "limit", "missing", "busy"];
    const why = code && known.includes(code) ? ` ${t(`error.${code}` as TextKey)}` : "";
    parts.push(`${t("fetch.failed", { list: names })}.${why}`);
  }
  if (parts.length > 0) return parts.join(" · ");
  if (report.runs.length === 0) return t("fetch.noSources");
  const ran = report.runs.some((r) => !r.skipped);
  const skipped = report.runs.find((r) => r.skipped);
  if (!ran && skipped?.skipped) return t(`fetch.skip.${skipped.skipped}` as TextKey);
  return t("fetch.none");
}

/** One sentence about an AI run. */
export function rateSummary(report: RateReport, t: (key: TextKey, params?: Record<string, string | number>) => string): string {
  if (report.error) {
    const known = ["net", "auth", "limit", "missing", "busy"];
    const why = known.includes(report.error.code) ? t(`error.${report.error.code}` as TextKey) : report.error.detail;
    return `${t("ai.stopped")} ${why}`;
  }
  return t("ai.rated", { count: report.rated, waiting: report.waiting });
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
