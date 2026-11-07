import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { createApi, type Api } from "./api";
import {
  DEFAULT_SETTINGS,
  aiOn,
  TEXT_SIZES,
  describeError,
  type AppStatus,
  type AssistKind,
  type CareerIdea,
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
  type WorkProgress,
} from "./api/types";
import { AddOfferDialog } from "./components/AddOfferDialog";
import { AssistDialog } from "./components/AiDialogs";
import { Board, DEFAULT_SORT, sortRows, type Sort, type SortKey } from "./components/Board";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { FirstSteps } from "./components/FirstSteps";
import { ErrorContext, ToastContext, ToastView, WorkContext, type Toast } from "./components/Toast";
import { IconArchive, IconBoard, IconBriefcase, IconDownload, IconFilter, IconGear, IconNoEntry, IconPlus, IconQuestion, IconSun, IconTrash, IconWarning } from "./components/Icons";
import { OfferCard } from "./components/OfferCard";
import { ProfileDialog } from "./components/ProfileDialog";
import { ProfileTabs } from "./components/ProfileTabs";
import { Reveal } from "./components/Reveal";
import { SettingsDialog, type SettingsSection } from "./components/SettingsDialog";
import { I18nContext, plural, resolveLang, translate, type TextKey } from "./i18n";
import { playClack, playGong } from "./lib/sound";
import { askNotifications, isPhone, scheduleReminders } from "./lib/platform";
import { isWebUrl } from "./lib/format";
import { isNarrowLayout, useFitLevels } from "./lib/fit";
import { onBack } from "./lib/back";
import { leavesView, viewFor } from "./lib/profile";
import { csvCells, csvName } from "./lib/csv";
import { JOOBLE_KEY_CALLS } from "./lib/limits";

const VIEWS: View[] = ["today", "board", "review", "rejected", "declined", "archive", "trash"];
const TAB_ICONS: Record<View, ReactNode> = {
  today: <IconSun />,
  board: <IconBoard />,
  review: <IconQuestion />,
  rejected: <IconFilter />,
  declined: <IconNoEntry />,
  archive: <IconArchive />,
  trash: <IconTrash />,
};

const TOPBAR_FIT = ["", "is-compact", "is-stacked", "is-stacked is-compact"];

/** What can scroll the list, depending on the layout - remembered while the card is on screen. */
const LIST_SCROLLERS = [".app", ".split", ".board-wrap"];

export default function App() {
  const [api, setApi] = useState<Api | null>(null);
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [view, setView] = useState<View>("today");
  const [rows, setRows] = useState<OfferRow[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [detail, setDetail] = useState<OfferDetail | null>(null);
  /** Narrow layout: the card takes the whole screen instead of the list. */
  const [cardOpen, setCardOpen] = useState(false);
  const listScroll = useRef<number[]>([]);
  /** What the archive's search box holds (title, company or city). */
  const [archiveQuery, setArchiveQuery] = useState("");
  /** `null` = the tab's own order: "Today" as Rust ranks it, the others the newest first. */
  const [sort, setSort] = useState<Sort | null>(null);
  const [freshIds, setFreshIds] = useState<Set<number>>(new Set());
  const [toast, showToast] = useState<Toast | null>(null);
  const toastCount = useRef(0);
  /** Each message counts as new - the same text twice in a row is still read out and takes its own "Undo". */
  const setToast = useCallback((next: Toast | null) => showToast(next && { ...next, id: ++toastCount.current }), []);
  const [error, setError] = useState<{ text: string; detail?: string } | null>(null);
  const [adding, setAdding] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsSection, setSettingsSection] = useState<SettingsSection | undefined>(undefined);
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
  /** What fetching or AI is doing now - the line at the bottom. */
  const [work, setWork] = useState<WorkProgress | null>(null);
  const [experiencePrev, setExperiencePrev] = useState<string | null>(null);
  const [assist, setAssist] = useState<{ kind: AssistKind; text: string; busy: boolean } | null>(null);
  /** The preview window shows the page of this offer ("Fill in the full description"). */
  const [fullFor, setFullFor] = useState<{ id: number; title: string } | null>(null);
  const [capturing, setCapturing] = useState(false);
  /** The page just taken says its offer is over - the card proposes "Mark as expired". */
  const [expiredPage, setExpiredPage] = useState<{ id: number; phrase: string } | null>(null);
  /** "Share" from the phone's browser - opens "Add offer" with the shared text. */
  const [shared, setShared] = useState<{ id: number; text: string } | null>(null);
  /** Goes up when the phone's notifications get allowed - the reminders are planned again. */
  const [replan, setReplan] = useState(0);

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
  const topbarRef = useRef<HTMLElement>(null);
  // One line in the top bar: full size, a bit tighter, then the tabs in their own row.
  useFitLevels(topbarRef, TOPBAR_FIT);

  // The tabs scroll sideways on a phone - the chosen one must not stay out of sight.
  useEffect(() => {
    topbarRef.current?.querySelector(".tab[aria-current]")?.scrollIntoView({ block: "nearest", inline: "center" });
  }, [view]);

  // ----- language, theme, motion -----
  const lang = resolveLang(settings.language, navigator.language);
  const i18n = useMemo(
    () => ({ lang, t: (key: TextKey, params?: Record<string, string | number>) => translate(lang, key, params) }),
    [lang],
  );
  const { t } = i18n;

  // The browser's "fill in this field" bubble speaks the system's language - this one speaks the app's.
  useEffect(() => {
    const onInvalid = (e: Event) => {
      const field = e.target as HTMLInputElement;
      if (field.validity?.valueMissing) field.setCustomValidity(t("form.required"));
    };
    // Before each check: a form can be filled by code (no input event), and an old message would block it.
    const onClick = (e: MouseEvent) => {
      const form = (e.target as Element).closest?.("button")?.form;
      if (!form) return;
      for (const field of form.elements) (field as HTMLInputElement).setCustomValidity?.("");
    };
    document.addEventListener("invalid", onInvalid, true);
    document.addEventListener("click", onClick, true);
    return () => {
      document.removeEventListener("invalid", onInvalid, true);
      document.removeEventListener("click", onClick, true);
    };
  }, [t]);

  useEffect(() => {
    const root = document.documentElement;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const motionMedia = window.matchMedia("(prefers-reduced-motion: reduce)");
    const apply = () => {
      root.dataset.theme = settings.theme === "system" ? (media.matches ? "dark" : "light") : settings.theme;
      root.dataset.motion =
        settings.motion === "system" ? (motionMedia.matches ? "reduced" : "full") : settings.motion;
      root.lang = lang;
      root.dataset.textSize = settings.text_size;
    };
    apply();
    media.addEventListener("change", apply);
    motionMedia.addEventListener("change", apply);
    return () => {
      media.removeEventListener("change", apply);
      motionMedia.removeEventListener("change", apply);
    };
  }, [settings.theme, settings.motion, settings.text_size, lang]);

  // ----- errors -----
  const fail = useCallback(
    (e: unknown) => {
      const { key, detail } = describeError(e);
      setError({ text: t(key as TextKey), detail });
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
      // The phone gets its profiles from the PC (sync), so no first steps there.
      setFirstSteps(!isPhone() && loaded.onboarding === "pending" && list.length === 0);
    }, fail);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const reload = useCallback(
    async (keepSelection = true) => {
      if (!api || !status?.ok) return;
      try {
        const [list, numbers] = await Promise.all([api.listOffers(view, profileId), api.countOffers(profileId)]);
        setRows(list);
        setCounts(numbers);
        setSelectedId((id) => (keepSelection && id !== null && list.some((r) => r.id === id) ? id : (list[0]?.id ?? null)));
      } catch (e) {
        fail(e);
      }
    },
    // The freshness settings are read by Rust - a new value must reload the board at once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [api, status, view, profileId, fail, settings.fresh_days, settings.today_count, settings.learn_scores],
  );

  useEffect(() => {
    reload(false);
  }, [reload]);

  // Every tab opens in its own order.
  useEffect(() => setSort(null), [view]);

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

  // The old offer's details stay until the new ones arrive - the list (or, going on to the next
  // offer, the card) waits on screen till then.
  const cardWasShown = useRef(false);
  const cardShown = cardOpen && detail !== null && (detail.id === selectedId || cardWasShown.current);

  function openCard() {
    listScroll.current = LIST_SCROLLERS.map((s) => document.querySelector(s)?.scrollTop ?? 0);
    setCardOpen(true);
  }

  // Android "back" leaves the card like "Back to the list" does (an open window goes first).
  useEffect(() => (cardOpen ? onBack(() => setCardOpen(false)) : undefined), [cardOpen]);

  // Before painting: the screen must not show the wrong place for a moment.
  useLayoutEffect(() => {
    if (cardShown === cardWasShown.current) return;
    cardWasShown.current = cardShown;
    if (cardShown) {
      for (const s of [...LIST_SCROLLERS, ".pass"]) document.querySelector(s)?.scrollTo(0, 0);
      cardHeading.current?.focus({ preventScroll: true });
    } else {
      LIST_SCROLLERS.forEach((s, i) => document.querySelector(s)?.scrollTo(0, listScroll.current[i] ?? 0));
      // The row may have moved on the list while its card was open.
      document.getElementById(`offer-${selectedId}`)?.scrollIntoView({ block: "nearest" });
      document.querySelector<HTMLElement>("table.board")?.focus({ preventScroll: true });
    }
  }, [cardShown, selectedId]);

  // Going on to the next offer with the card open: its top, not the middle of the last one.
  const shownId = cardShown ? detail?.id : undefined;
  useLayoutEffect(() => {
    if (shownId === undefined) return;
    for (const s of [...LIST_SCROLLERS, ".pass"]) document.querySelector(s)?.scrollTo(0, 0);
    cardHeading.current?.focus({ preventScroll: true });
  }, [shownId]);

  // A message goes by itself after 10 s, or earlier with "X" or Esc; a new one takes its place.
  // The pointer over it or focus inside it (e.g. on "Undo") holds it - the time starts again after.
  const closeToast = useCallback(() => setToast(null), []);
  const [toastHeld, setToastHeld] = useState(false);
  const toastValue = useMemo(() => ({ toast, close: closeToast, hold: setToastHeld }), [toast, closeToast]);
  // The same error is shown inside an open window (the bar below is behind it).
  const errorValue = useMemo(() => ({ error, close: () => setError(null) }), [error]);
  useEffect(() => {
    if (!toast || toastHeld) return;
    // On a phone a screen reader moves its own focus, not the page's - give "Undo" more time.
    const timer = window.setTimeout(() => setToast(null), toast.undo && isPhone() ? 20_000 : 10_000);
    return () => window.clearTimeout(timer);
  }, [toast, toastHeld]);
  useEffect(() => {
    if (!toast) setToastHeld(false);
  }, [toast]);
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

  // Ctrl + / Ctrl - / Ctrl 0 - the text size setting (instead of zooming the window, so it is
  // remembered and the same as the choice in Settings → Look).
  const textSize = settings.text_size;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
      const step =
        e.key === "+" || e.key === "=" || e.code === "NumpadAdd" ? 1
        : e.key === "-" || e.code === "NumpadSubtract" ? -1
        : e.key === "0" || e.code === "Numpad0" ? 0
        : null;
      if (step === null) return;
      e.preventDefault();
      const now = TEXT_SIZES.indexOf(textSize);
      const next = step === 0 ? "normal" : TEXT_SIZES[Math.min(TEXT_SIZES.length - 1, Math.max(0, now + step))];
      if (next !== textSize) changeSettingRef.current("text_size", next);
      setToast({ text: t("settings.text_size.changed", { value: t(`settings.text_size.${next}`) }) });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [textSize, t]);

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
        if (report.added > 0 || report.closed > 0) reloadRef.current();
        if (report.added > 0) {
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
    let stopProgress: (() => void) | undefined;
    api.onProgress(setWork).then((unlisten) => (alive ? (stopProgress = unlisten) : unlisten()));
    return () => {
      alive = false;
      stopProgress?.();
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
    setExpiredPage(page.expiredPhrase ? { id: fullFor.id, phrase: page.expiredPhrase } : null);
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

  async function suggestCareers(description: string, experience: string): Promise<CareerIdea[] | null> {
    if (!api) return null;
    try {
      return await api.aiSuggestCareers(description, experience);
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
  }, [api, rows, settings.notify_reminders, replan, lang, t]);

  const shownSort = sort ?? (view === "today" ? null : DEFAULT_SORT);
  const sorted = useMemo(() => {
    const list = shownSort === null ? rows : sortRows(rows, shownSort);
    const q = archiveQuery.trim().toLocaleLowerCase();
    if (view !== "archive" || !q) return list;
    return list.filter((r) => [r.title, r.titleTranslated, r.company, r.city].some((v) => v?.toLocaleLowerCase().includes(q)));
  }, [rows, shownSort, view, archiveQuery]);

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

  /** "Export this list": exactly the rows the board shows (tab, profile, hidden ones, sorting), in that order. */
  async function exportList() {
    if (!api) return;
    try {
      const all = new Map((await api.exportRows(null)).map((r) => [r.id, r]));
      const shown = sorted.flatMap((r) => all.get(r.id) ?? []);
      const path = await api.saveCsv(csvName(view, lang), csvCells(shown, lang));
      if (path) setToast({ text: t("data.csvSaved", { path }) });
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
    setSort(() =>
      shownSort?.key === key
        ? { key, dir: shownSort.dir === "asc" ? "desc" : "asc" }
        : { key, dir: key === "offer" || key === "place" ? "asc" : "desc" },
    );

  /** Interest changes can move the offer to another tab ("very" from "To check" to "Offers", "no" to
   *  "Not for me" and back) - then the next row gets selected and a message names the new tab. */
  const setInterest = (id: number, next: Interest | null) => {
    const row = rows.find((r) => r.id === id);
    const target = row ? leavesView({ ...row, interest: next }, view) : null;
    if (!row || target === null) return run(() => api!.setInterest(id, next));
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
  /** A stage or a star keeps a filtered-out offer, so it can leave this tab like with interest. */
  const changeKept = (id: number, change: Partial<Pick<OfferRow, "stage" | "favorite">>, save: () => Promise<unknown>, back: () => Promise<unknown>) => {
    const row = rows.find((r) => r.id === id);
    const target = row ? leavesView({ ...row, ...change }, view) : null;
    if (!row || target === null) return run(save);
    selectNeighbour(id);
    return run(save, () => setToast({ text: t("board.movedTo", { tab: t(`tabs.${target}`) }), undo: () => run(back) }));
  };
  const setStage = (id: number, next: Stage) => {
    const before = rows.find((r) => r.id === id)?.stage ?? next;
    return changeKept(id, { stage: next }, () => api!.setStage(id, next), () => api!.setStage(id, before));
  };
  const setFavorite = (id: number, next: boolean) =>
    changeKept(id, { favorite: next }, () => api!.setFavorite(id, next), () => api!.setFavorite(id, !next));
  const toggleInterest = (id: number, interest: Interest) => {
    const row = rows.find((r) => r.id === id);
    return setInterest(id, row?.interest === interest ? null : interest);
  };

  /** The row is about to leave the list: the next one (or the previous one at the end) becomes
   *  selected - with the card open, its card comes next (back to the list after the last one). */
  function selectNeighbour(id: number) {
    const i = sorted.findIndex((r) => r.id === id);
    const neighbour = sorted[i + 1] ?? sorted[i - 1];
    if (id !== selectedId) return;
    setSelectedId(neighbour?.id ?? null);
    if (!neighbour) setCardOpen(false);
  }

  /** Quick browsing: on to the next offer after a decision (keys, a swipe on the card). */
  function goNext(id: number) {
    const i = sorted.findIndex((r) => r.id === id);
    const next = sorted[i + 1];
    if (id !== selectedId || !next) return;
    setSelectedId(next.id);
    document.getElementById(`offer-${next.id}`)?.scrollIntoView({ block: "nearest" });
  }

  /** "Expired": nobody touched it - to the archive; otherwise marked. Then the next offer. */
  const expire = (id: number) => {
    const row = rows.find((r) => r.id === id);
    if (!row || !api) return;
    if (row.expired) {
      return run(() => api.unsetExpired(id), () => setToast({ text: t("expired.unmarked") }));
    }
    // Marked offers stay on "Offers" (greyed, at the bottom); everywhere else the offer leaves.
    selectNeighbour(id);
    return run(
      async () => {
        const by = await api.setExpired(id);
        setToast({ text: t(by === "archived" ? "expired.archived" : "expired.marked"), undo: () => run(() => api.unsetExpired(id)) });
      },
    );
  };

  /** A swipe on the card (phone): right = "very", left = "no" - and the next offer's card. */
  const swipeCard = (id: number, value: Interest) => {
    const row = rows.find((r) => r.id === id);
    if (!row || !api) return;
    const before = row.interest;
    goNext(id);
    return run(
      () => api.setInterest(id, value),
      () => setToast({ text: t("swipe.card", { value: t(`interest.${value}`) }), undo: () => run(() => api.setInterest(id, before)) }),
    );
  };

  const unarchive = (id: number) => {
    selectNeighbour(id);
    return run(
      () => api!.unarchiveOffer(id),
      () => setToast({ text: t("board.movedTo", { tab: t("tabs.board") }) }),
    );
  };

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

  const restore = (id: number) => {
    const row = rows.find((r) => r.id === id);
    selectNeighbour(id);
    return run(
      () => api!.restoreOffer(id),
      () =>
        row &&
        setToast({
          text: t("board.movedTo", { tab: t(`tabs.${viewFor(row)}`) }),
          undo: () => run(() => api!.trashOffer(id)),
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
      // Added by hand = always at the top of "Today" (unless the filter rejected it).
      const target = view === "today" && added.filterResult !== "fail" ? "today" : viewFor(added);
      const text =
        outcome.status === "merged"
          ? t("add.merged")
          : outcome.status === "unchanged"
            ? t("add.known")
            : target === "board" || target === "today"
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

  function openSettings(section?: SettingsSection) {
    setSettingsSection(section);
    setSettingsOpen(true);
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

  const changeSettingRef = useRef(changeSetting);
  changeSettingRef.current = changeSetting;

  function openLink(url: string) {
    if (!isWebUrl(url)) return;
    if ("__TAURI_INTERNALS__" in window) {
      import("@tauri-apps/plugin-opener").then((m) => m.openUrl(url)).catch(fail);
    } else {
      window.open(url, "_blank", "noopener,noreferrer");
    }
  }

  const locked = status !== null && !status.ok;
  // The filter's rejects stay on the PC, so on the phone this tab is empty - unless an offer added here landed there.
  const shownViews = VIEWS.filter((v) => v !== "rejected" || !isPhone() || view === v || (counts?.rejected ?? 0) > 0);

  /** Phone: the first date is when the question about notifications makes sense. */
  async function addEvent(event: Parameters<Api["addEvent"]>[1]): Promise<boolean> {
    if (selectedId === null) return false;
    const added = await act(() => api!.addEvent(selectedId, event));
    if (added && isPhone() && settings.notify_reminders === "on" && (await askNotifications())) setReplan((n) => n + 1);
    return added;
  }

  return (
    <I18nContext.Provider value={i18n}>
      <ToastContext.Provider value={toastValue}>
      <ErrorContext.Provider value={errorValue}>
      <WorkContext.Provider value={work}>
      <div className={cardShown ? "app is-card" : "app"}>
        <header className="topbar" ref={topbarRef}>
          <div className="brand">
            <span className="brand-mark">
              <IconBriefcase />
            </span>
            <h1>{t("app.name")}</h1>
          </div>
          <nav className="tabs" aria-label={t("tabs.label")} data-own-row>
            {shownViews.map((v) => (
              <button key={v} type="button" className="tab" aria-current={view === v ? "page" : undefined} onClick={() => setView(v)}>
                {/* The phone's bottom bar: a pictogram and a short name (the full one stays for a screen reader). */}
                <span className="tab-icon">{TAB_ICONS[v]}</span>
                <span className="tab-name">{t(`tabs.${v}`)}</span>
                <span className="tab-short" aria-hidden="true">
                  {t(`tabs.short.${v}`)}
                </span>
                {counts && v === "board" && <span className="tab-count tab-count-narrow">{view === "board" ? rows.length : counts.board}</span>}
                {counts && (v === "today" || v === "rejected" || v === "declined") && counts[v] > 0 && (
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
            {!isPhone() && (
              <button type="button" className="btn btn-ghost btn-fetch" onClick={fetchNow} disabled={locked || fetching} aria-busy={fetching}>
                <IconDownload /> {fetching ? t("fetch.busy") : t("fetch.button")}
              </button>
            )}
            <button type="button" className="btn btn-sign btn-add" onClick={() => setAdding(true)} disabled={locked}>
              <IconPlus /> <span className="btn-add-text">{t("add.button")}</span>
            </button>
            <button
              type="button"
              className="icon-btn"
              onClick={() => openSettings()}
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
                <span className="count">· {t(plural(lang, "board.count", sorted.length), { count: sorted.length })}</span>
              </h2>
              <div className="board-tools">
                {settings.feature_keys === "on" && !isPhone() && view !== "archive" && view !== "trash" && <p className="keys-hint">{t("board.keys")}</p>}
                {!isPhone() && (
                  <button
                    type="button"
                    className="btn btn-ghost btn-small"
                    title={t("board.exportHint")}
                    disabled={sorted.length === 0}
                    onClick={exportList}
                  >
                    <IconDownload /> {t("board.export")}
                  </button>
                )}
              </div>
            </div>
            {view === "archive" && (
              <div className="archive-search">
                <input
                  type="search"
                  value={archiveQuery}
                  onChange={(e) => setArchiveQuery(e.target.value)}
                  placeholder={t("archive.search")}
                  aria-label={t("archive.search")}
                  maxLength={100}
                />
              </div>
            )}
            <Board
              rows={sorted}
              view={view}
              selectedId={selectedId}
              freshIds={freshIds}
              sort={shownSort}
              onSort={onSort}
              onSelect={setSelectedId}
              onOpen={() => (isNarrowLayout() ? openCard() : cardHeading.current?.focus())}
              onInterest={toggleInterest}
              onTrash={trash}
              onExpired={expire}
              onNext={goNext}
            />
          </section>

          <OfferCard
            ref={cardHeading}
            offer={detail}
            onStage={(stage: Stage) => selectedId !== null && setStage(selectedId, stage)}
            onInterest={(i) => selectedId !== null && setInterest(selectedId, i)}
            onFavorite={(f) => selectedId !== null && setFavorite(selectedId, f)}
            onNotes={(n) => selectedId !== null && run(() => api!.setNotes(selectedId, n), () => setToast({ text: t("card.notesSaved") }))}
            onOpenLink={openLink}
            onTrash={() => selectedId !== null && trash(selectedId)}
            onRestore={() => selectedId !== null && restore(selectedId)}
            onExpired={() => {
              setExpiredPage(null);
              return selectedId !== null && expire(selectedId);
            }}
            expiredPage={expiredPage?.id === selectedId ? expiredPage.phrase : null}
            onKeepExpiredPage={() => setExpiredPage(null)}
            onUnarchive={() => selectedId !== null && unarchive(selectedId)}
            onSwipe={(value) => selectedId !== null && swipeCard(selectedId, value)}
            onPurge={() =>
              selectedId !== null &&
              setConfirm({
                message: t("trash.confirmOne"),
                action: () => {
                  selectNeighbour(selectedId);
                  return run(() => api!.purgeOffers(selectedId), () => setToast({ text: t("trash.purged") }));
                },
              })
            }
            aiOn={aiOn(settings)}
            onOpenAiSettings={() => openSettings("ai")}
            onFullText={() => detail && startFullText(detail)}
            onBackToList={() => setCardOpen(false)}
            agenda={{
              addEvent,
              deleteEvent: (id) =>
                setConfirm({ message: t("agenda.deleteEventConfirm"), label: t("agenda.delete"), action: () => run(() => api!.deleteEvent(id)) }),
              google: (id) => run(() => api!.calendarGoogle(id)),
              ics: (id) => api!.calendarIcs(id).then((path) => setToast({ text: t("agenda.icsSaved", { path }) }), fail),
              saveContact: (contact) => (selectedId === null ? Promise.resolve(false) : act(() => api!.saveContact(selectedId, contact))),
              deleteContact: (id) =>
                setConfirm({ message: t("agenda.deleteContactConfirm"), label: t("agenda.delete"), action: () => run(() => api!.deleteContact(id)) }),
              addLog: (entry) => (selectedId === null ? Promise.resolve(false) : act(() => api!.addLog(selectedId, entry))),
              deleteLog: (id) =>
                setConfirm({ message: t("agenda.deleteLogConfirm"), label: t("agenda.delete"), action: () => run(() => api!.deleteLog(id)) }),
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
          aiOn={aiOn(settings)}
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
          aiOn={aiOn(settings)}
          onSuggest={suggestCriteria}
          onCareers={suggestCareers}
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
          initialSection={settingsSection}
          onClose={() => setSettingsOpen(false)}
          onChange={changeSetting}
          onOpenLink={openLink}
          onError={fail}
          onToast={(text) => setToast({ text })}
          onNotificationsAllowed={() => setReplan((n) => n + 1)}
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
      </WorkContext.Provider>
      </ErrorContext.Provider>
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
  if (!ran && skipped?.skipped) return t(`fetch.skip.${skipped.skipped}` as TextKey, { calls: JOOBLE_KEY_CALLS });
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

