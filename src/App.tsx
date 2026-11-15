import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { createApi, type Api } from "./api";
import { DEMO, DEMO_ERROR, DEMO_REPO } from "./lib/demo";
import {
  DEFAULT_SETTINGS,
  aiOn,
  TEXT_SIZES,
  describeError,
  isAppError,
  type AppStatus,
  type AssistKind,
  type CareerIdea,
  type Captured,
  type Draft,
  type Criteria,
  type FetchReport,
  type Interest,
  type OfferDetail,
  type OfferInput,
  type SameOffer,
  type SameField,
  type DupCandidate,
  type DupVerdict,
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
import { Board, DEFAULT_SORT, isClosed, sortRows, sourceLabel, type Sort, type SortKey } from "./components/Board";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { FirstSteps } from "./components/FirstSteps";
import { Tour, type TourStep } from "./components/Tour";
import { ErrorContext, ProfileDraftContext, ToastContext, ToastView, WorkContext, type Toast } from "./components/Toast";
import { IconBoard, IconBriefcase, IconDownload, IconFilter, IconGear, IconHome, IconList, IconPlus, IconTrash, IconWarning } from "./components/Icons";
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
import { REVEAL_MS } from "./lib/presence";
import { keepOrder } from "./lib/browse";
import { applyFilters, FILTER_KEYS, NO_FILTERS, type ListFilters } from "./lib/portals";
import { availableFilters, ListFilterBar } from "./components/ListFilterBar";
import { BIN, isBin, leavesView, placeName, viewFor, type BinView } from "./lib/profile";
import { csvCells, csvName, csvSeparator } from "./lib/csv";
import { JOOBLE_KEY_CALLS } from "./lib/limits";
import { rateSummary } from "./lib/aiProblem";
import { sourceProblem } from "./lib/sourceProblem";
import { lostFields, type LostField } from "./lib/savedCheck";

const LOST_LABEL: Record<LostField, TextKey> = {
  title: "add.fieldTitle",
  company: "add.fieldCompany",
  url: "add.fieldUrl",
  city: "add.fieldCity",
  remote: "add.fieldRemote",
  salary: "add.lostSalary",
  contract: "add.fieldContract",
  level: "add.fieldLevel",
  published: "add.fieldPublished",
  expires: "add.fieldExpires",
  description: "add.fieldDescription",
};

/** The tabs on top. "Collected" holds four lists - best today, all, to check, rejected by the
 *  filter (`COLLECTED`, the switch inside it); "Bin" four too - all, not for me, expired, deleted. */
type Tab = "own" | "collected" | "bin";
const TABS: Tab[] = ["own", "collected", "bin"];
const COLLECTED = ["today", "board", "review", "rejected"] as const;
type CollectedView = (typeof COLLECTED)[number];
const isCollected = (v: View): v is CollectedView => (COLLECTED as readonly View[]).includes(v);
const tabOf = (v: View): Tab => (isCollected(v) ? "collected" : isBin(v) ? "bin" : "own");

/** The list of "Bin" chosen last - kept between starts of the app (only a convenience). */
const BIN_KEY = "tablica.bin";
function savedBin(): BinView {
  try {
    const v = localStorage.getItem(BIN_KEY) as View | null;
    return v && isBin(v) ? v : "bin";
  } catch {
    return "bin";
  }
}

/** The list of "Collected" chosen last - kept between starts of the app (only a convenience). */
const COLLECTED_KEY = "tablica.collected";
function savedCollected(): CollectedView {
  try {
    const v = localStorage.getItem(COLLECTED_KEY) as View | null;
    return v && isCollected(v) ? v : "today";
  } catch {
    return "today";
  }
}
const TAB_ICONS: Record<Tab, ReactNode> = {
  own: <IconHome />,
  collected: <IconBoard />,
  bin: <IconTrash />,
};

const TOPBAR_FIT = ["", "is-compact", "is-stacked", "is-stacked is-compact"];

/** What can scroll the list, depending on the layout - remembered while the card is on screen. */
const LIST_SCROLLERS = [".app", ".split", ".board-wrap"];

export default function App() {
  const [api, setApi] = useState<Api | null>(null);
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  // The person's own offers first - that is what the app is for now; collected ones are an extra.
  const [view, setView] = useState<View>("own");
  /** The list of "Collected" chosen last - the tab opens on it again. */
  const [lastCollected, setLastCollected] = useState<CollectedView>(savedCollected);
  useEffect(() => {
    if (!isCollected(view)) return;
    setLastCollected(view);
    try {
      localStorage.setItem(COLLECTED_KEY, view);
    } catch {
      // No storage (private mode): the choice lasts until the app closes.
    }
  }, [view]);
  /** The list of "Bin" chosen last - the tab opens on it again. */
  const [lastBin, setLastBin] = useState<BinView>(savedBin);
  useEffect(() => {
    if (!isBin(view)) return;
    setLastBin(view);
    try {
      localStorage.setItem(BIN_KEY, view);
    } catch {
      // No storage (private mode): the choice lasts until the app closes.
    }
  }, [view]);
  /** "Collect offers automatically" off: only own offers - no "Collected" and no "Fetch offers"
   *  (the collected offers stay in the database and come back when it is on again). */
  const collecting = settings.collect === "on";
  const shownTabs = collecting ? TABS : TABS.filter((tab) => tab !== "collected");
  useEffect(() => {
    if (!collecting && isCollected(view)) setView("own");
  }, [collecting, view]);
  const [rows, setRows] = useState<OfferRow[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [detail, setDetail] = useState<OfferDetail | null>(null);
  /** Narrow layout: the card takes the whole screen instead of the list. */
  const [cardOpen, setCardOpen] = useState(false);
  /** Quick browsing (cards, keys 1 / 2 / 3): the order it started with (`keepOrder`), else `null`. */
  const [browseOrder, setBrowseOrder] = useState<number[] | null>(null);
  const listScroll = useRef<number[]>([]);
  /** What the search box above the list holds (title, company or city) - one word for all tabs, never remembered after a restart. */
  const [query, setQuery] = useState("");
  // The filters above every list (portal, status, place, salary) - one choice for all tabs, never remembered after a restart.
  const [filters, setFilters] = useState<ListFilters>(NO_FILTERS);
  /** "Show filters": the filters slide out above the list (hidden at the start - they take room). */
  const [filtersOpen, setFiltersOpen] = useState(false);
  /** "Hide categories": the switch of Collected's / the Bin's lists folds away (shown at the start). */
  const [catsOpen, setCatsOpen] = useState(true);
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
  // Settings → Profile left with changes not saved - a reminder in the corner until they are.
  const [profileDraft, setProfileDraft] = useState(false);
  /** Its reminder closed with "X" - back with the next change not saved. */
  const [draftDismissed, setDraftDismissed] = useState(false);
  useEffect(() => {
    if (!profileDraft) setDraftDismissed(false);
  }, [profileDraft]);
  const [confirm, setConfirm] = useState<{ message: string; action: () => void; label?: string } | null>(null);
  const [profiles, setProfiles] = useState<Profile[]>([]);
  /** null = all profiles together */
  const [profileId, setProfileId] = useState<number | null>(null);
  const [counts, setCounts] = useState<ViewCounts | null>(null);
  /** Open profile window: `profile = null` - a new one. */
  const [editing, setEditing] = useState<{ profile: Profile | null } | null>(null);
  const [experience, setExperience] = useState("");
  const [firstSteps, setFirstSteps] = useState(false);
  /** The guide shown again from Settings → About (the first time it comes from the `tour` setting). */
  const [tourAgain, setTourAgain] = useState(false);
  /** Offer to select once the list of a newly opened tab has loaded. */
  const [pendingSelect, setPendingSelect] = useState<number | null>(null);
  const [fetching, setFetching] = useState(false);
  /** What fetching or AI is doing now - the line at the bottom. */
  const [work, setWork] = useState<WorkProgress | null>(null);
  const [experiencePrev, setExperiencePrev] = useState<string | null>(null);
  const [assist, setAssist] = useState<{ kind: AssistKind; text: string; busy: boolean } | null>(null);
  /** The preview window shows the page of this offer ("Fill in the full description"). */
  const [fullFor, setFullFor] = useState<{ id: number; title: string; url: string } | null>(null);
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
    // The phone (MainActivity.kt) waits for this before it hands over a share.
    const ready = window as unknown as { __tablicaReady?: boolean };
    ready.__tablicaReady = true;
    return () => {
      ready.__tablicaReady = false;
      window.removeEventListener("tablica-share", onShare);
    };
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
      root.dataset.tilt = settings.tilt;
    };
    apply();
    media.addEventListener("change", apply);
    motionMedia.addEventListener("change", apply);
    return () => {
      media.removeEventListener("change", apply);
      motionMedia.removeEventListener("change", apply);
    };
  }, [settings.theme, settings.motion, settings.text_size, settings.tilt, lang]);

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
        // A child of the tree stays chosen too (it is under its parent).
        const there = (id: number) => list.some((r) => r.id === id || r.children.some((c) => c.id === id));
        setSelectedId((id) => (keepSelection && id !== null && there(id) ? id : (list[0]?.id ?? null)));
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
  const reloadNow = useRef(reload);
  reloadNow.current = reload;

  // "Several profiles" off: one profile, no signs above the list - the list shows what it sorts.
  const multiProfile = settings.multi_profile === "on";
  useEffect(() => {
    if (!multiProfile) setProfileId(null);
  }, [multiProfile]);

  // Every tab opens in its own order - and at its top.
  useEffect(() => setSort(null), [view]);
  useEffect(() => {
    for (const s of LIST_SCROLLERS) document.querySelector(s)?.scrollTo(0, 0);
  }, [view, profileId]);

  useEffect(() => {
    if (pendingSelect !== null && rows.some((r) => r.id === pendingSelect || r.children.some((c) => c.id === pendingSelect))) {
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
    setBrowseOrder(sorted.map((r) => r.id));
    setCardOpen(true);
  }

  // Browsing ends - back to the list, another tab or profile, another order: the new order shows.
  useEffect(() => {
    if (!cardOpen) setBrowseOrder(null);
  }, [cardOpen]);
  useEffect(() => setBrowseOrder(null), [view, profileId, sort]);

  // Android "back" leaves the card like "Back to the list" does (an open window goes first).
  // The field in use loses focus first - a note saves on leaving its field, so it is kept.
  useEffect(
    () =>
      cardOpen
        ? onBack(() => {
            (document.activeElement as HTMLElement | null)?.blur();
            setCardOpen(false);
          })
        : undefined,
    [cardOpen],
  );
  // On the list: "back" clears the search, then goes from "Collected" or "Bin" to "Own" -
  // the app is left only from "Own" with nothing open.
  const currentTab = tabOf(view);
  useEffect(() => (currentTab !== "own" ? onBack(() => setView("own"), true) : undefined), [currentTab]);
  const searching = query !== "";
  useEffect(() => (searching ? onBack(() => setQuery(""), true) : undefined), [searching]);
  const filtering = FILTER_KEYS.some((k) => filters[k].length > 0);
  useEffect(() => (filtering ? onBack(() => setFilters(NO_FILTERS), true) : undefined), [filtering]);
  // No offer left (the list emptied, or the layout is wide again after turning the phone): the card
  // closes - otherwise "back" would close an invisible card and the next offer's card open by itself.
  useEffect(() => {
    if (!cardOpen) return;
    const check = () => {
      if (selectedId === null || !isNarrowLayout()) setCardOpen(false);
    };
    check();
    window.addEventListener("resize", check);
    return () => window.removeEventListener("resize", check);
  }, [cardOpen, selectedId]);

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

  /** The offer the card's buttons act on - none while the card still shows the previous offer. */
  const cardId = detail !== null && detail.id === selectedId ? selectedId : null;

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
  // On a phone a screen reader moves its own focus, not the page's - give "Undo" more time.
  const toastMs = toast?.undo && isPhone() ? 20_000 : 10_000;
  const toastValue = useMemo(
    () => ({ toast, close: closeToast, hold: setToastHeld, ms: toastMs, held: toastHeld }),
    [toast, closeToast, toastMs, toastHeld],
  );
  // The same error is shown inside an open window (the bar below is behind it).
  const errorValue = useMemo(() => ({ error, close: () => setError(null) }), [error]);
  useEffect(() => {
    if (!toast || toastHeld) return;
    const timer = window.setTimeout(() => setToast(null), toastMs);
    return () => window.clearTimeout(timer);
  }, [toast, toastHeld, toastMs]);
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
        if (report.error || report.lowFunds) setToast({ text: rateSummary(report, t, lang) });
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

  async function rateAgain(id: number, precise = false) {
    if (!api) return;
    setToast({ text: t("ai.working") });
    try {
      const report = await api.aiRateOffer(id, precise);
      await reload();
      setToast({ text: rateSummary(report, t, lang) });
    } catch (e) {
      // "AI is working…" must not stay next to the error. Busy = another AI run (e.g. AI on its own).
      if (isAppError(e) && e.code === "busy") return setToast({ text: t("error.busyAi") });
      setToast(null);
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

  async function capturePage(url: string): Promise<Captured | null> {
    if (!api) return null;
    try {
      return await api.capturePreview();
    } catch (e) {
      // The preview window is gone (closed by the person or by the page) - open it again
      // (`preview::CLOSED`).
      if (isAppError(e) && e.detail === "the preview is closed" && url && (await openPreview(url))) {
        setToast({ text: t("quick.reopened") });
        return null;
      }
      fail(e);
      return null;
    }
  }

  async function fetchPage(url: string): Promise<Captured | null> {
    if (!api) return null;
    try {
      return await api.fetchPage(url);
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
    if (offer.url && (await openPreview(offer.url))) setFullFor({ id: offer.id, title: offer.title, url: offer.url });
  }

  async function takeFullText() {
    if (!api || !fullFor) return;
    setCapturing(true);
    const page = await capturePage(fullFor.url);
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
    if (!api || !isPhone()) return;
    if (settings.notify_reminders !== "on") {
      void scheduleReminders([]);
      return;
    }
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

  // "Today" as Rust ranks it, "Own" in its groups (the CV to send on top, then recruitments, without
  // a link lower), the bin the last thrown in on top; the others the newest first.
  const shownSort = sort ?? (view === "today" || view === "own" || isBin(view) ? null : DEFAULT_SORT);
  const searched = useMemo(() => {
    // Rust's order, with the finished offers in their own group below (as `sortRows` keeps them).
    const ranked = isBin(view) ? rows : [...rows.filter((r) => !isClosed(r)), ...rows.filter(isClosed)];
    const list = keepOrder(shownSort === null ? ranked : sortRows(rows, shownSort), browseOrder);
    const q = query.trim().toLocaleLowerCase();
    if (!q) return list;
    return list.filter((r) => [r.title, r.titleTranslated, r.company, r.city].some((v) => v?.toLocaleLowerCase().includes(q)));
  }, [rows, shownSort, query, browseOrder, view]);
  const sorted = useMemo(() => applyFilters(searched, filters), [searched, filters]);
  const filterChoices = useMemo(() => availableFilters(searched, filters), [searched, filters]);
  // A filter chosen (or a new list under it): an offer the filter hid does not stay on the card -
  // also one hidden by English unticked or by the search (no filter chosen, so only a hidden one is swapped).
  useEffect(() => {
    const has = (list: OfferRow[], id: number) => list.some((r) => r.id === id || r.children.some((c) => c.id === id));
    if (filtering) setSelectedId((id) => (id !== null && has(sorted, id) ? id : (sorted[0]?.id ?? null)));
    // None chosen (the search found nothing a moment ago): the first offer of the list again.
    else setSelectedId((id) => (id === null || (!has(sorted, id) && has(rows, id)) ? (sorted[0]?.id ?? null) : id));
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only when the list or the filters change
  }, [searched, filters]);

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
      const path = await api.saveCsv(csvName(view, lang), csvCells(shown, lang), csvSeparator(lang));
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
          text: t("board.movedTo", { tab: placeName(target, t) }),
          undo: () => run(() => api!.setInterest(id, before)),
        }),
    );
  };
  /** A stage or a star keeps a filtered-out offer, so it can leave this tab like with interest. */
  const changeKept = (id: number, change: Partial<Pick<OfferRow, "stage" | "favorite" | "own">>, save: () => Promise<unknown>, back: () => Promise<unknown>) => {
    const row = rows.find((r) => r.id === id);
    // A child: "own" is the family's, so its parent's row leaves (or stays) - with the child's "own" only.
    const parent = row ? undefined : rows.find((r) => r.children.some((c) => c.id === id));
    const target = row ? leavesView({ ...row, ...change }, view) : parent && change.own !== undefined ? leavesView({ ...parent, own: change.own }, view) : null;
    if ((!row && !parent) || target === null || target === undefined) return run(save);
    if (row) selectNeighbour(id);
    else if (parent && (selectedId === id || selectedId === parent.id)) {
      const i = sorted.findIndex((r) => r.id === parent.id);
      const neighbour = sorted[i + 1] ?? sorted[i - 1];
      setSelectedId(neighbour?.id ?? null);
      if (!neighbour) setCardOpen(false);
    }
    return run(save, () => setToast({ text: t("board.movedTo", { tab: placeName(target, t) }), undo: () => run(back) }));
  };
  /** A stage past "new" makes the offer the person's own (Rust `set_stage`) - it moves to "Own";
   *  "Undo" brings back the stage and, for a collected offer, its place in "Collected". */
  const setStage = (id: number, next: Stage) => {
    // A child too - its own stage and the family's "own" come back with "Undo".
    const row = rows.find((r) => r.id === id) ?? rows.flatMap((r) => r.children).find((c) => c.id === id);
    const family = rows.find((r) => r.id === id || r.children.some((c) => c.id === id));
    const before = row?.stage ?? next;
    const wasOwn = family?.own ?? true;
    return changeKept(
      id,
      { stage: next, own: wasOwn || next !== "new" },
      () => api!.setStage(id, next),
      async () => {
        await api!.setStage(id, before);
        if (!wasOwn) await api!.setOwn(id, false);
      },
    );
  };
  const setFavorite = (id: number, next: boolean) =>
    changeKept(id, { favorite: next }, () => api!.setFavorite(id, next), () => api!.setFavorite(id, !next));
  /** "To own": the offer leaves "Collected" for "Own" - the message can undo it. */
  const toOwn = (id: number) => changeKept(id, { own: true }, () => api!.setOwn(id, true), () => api!.setOwn(id, false));
  /** The checkbox off: the whole job back to "Collected" - the message can undo it. */
  const toCollected = (id: number) => changeKept(id, { own: false }, () => api!.setOwn(id, false), () => api!.setOwn(id, true));
  const toggleInterest = (id: number, interest: Interest) => {
    const row = rows.find((r) => r.id === id);
    return setInterest(id, row?.interest === interest ? null : interest);
  };

  /** The rows in the order the list shows them around `id`: a child's family unfolded (the list
   *  opens it while a child is chosen), so the next of a child is its next sibling, not the top row. */
  function shownAround(id: number): OfferRow[] {
    return sorted.flatMap((r) => (r.children.some((c) => c.id === id) ? [r, ...r.children] : [r]));
  }

  /** The row is about to leave the list: the next one (or the previous one at the end) becomes
   *  selected - with the card open, its card comes next (back to the list after the last one). */
  function selectNeighbour(id: number) {
    const list = shownAround(id);
    const i = list.findIndex((r) => r.id === id);
    const neighbour = list[i + 1] ?? list[i - 1];
    if (id !== selectedId) return;
    setSelectedId(neighbour?.id ?? null);
    if (!neighbour) setCardOpen(false);
  }

  /** Quick browsing: on to the next offer after a decision (keys, a swipe on the card). */
  function goNext(id: number) {
    // Keys on the list: from the first decision on, the order stays (on the card it already does).
    if (browseOrder === null) setBrowseOrder(sorted.map((r) => r.id));
    const list = shownAround(id);
    const i = list.findIndex((r) => r.id === id);
    const next = list[i + 1];
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
    const i = sorted.findIndex((r) => r.id === id);
    // The last offer: back to the list (the card would show the first offer, or none at all).
    const last = !sorted[i + 1];
    if (!last) goNext(id);
    else if (id === selectedId) setCardOpen(false);
    return run(
      () => api.setInterest(id, value),
      () => setToast({ text: t(last ? "swipe.cardLast" : "swipe.card", { value: t(`interest.${value}`) }), undo: () => run(() => api.setInterest(id, before)) }),
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

  /** "Restore" in the bin, one for every kind: back where the offer came from ("Own" or
   *  "Collected"); "Undo" puts it back in the bin exactly as it was. */
  const restore = (id: number) => {
    const row = rows.find((r) => r.id === id);
    selectNeighbour(id);
    return run(async () => {
      const place = await api!.bringBackOffer(id);
      const back = row && viewFor({ ...row, interest: row.interest === "no" ? null : row.interest });
      setToast({
        text: back ? t("board.movedTo", { tab: placeName(back, t) }) : t("bin.restored"),
        undo: () => run(() => api!.putBackOffer(id, place)),
      });
    });
  };

  /** Selects offer `id` in the tab where it is now (all profiles together). */
  async function showOnBoard(id: number) {
    const shown = await api!.getOffer(id);
    // In the bin (deleted, or expired = the archive) it is shown there, not where it would belong.
    const target = shown.deletedAt ? "trash" : shown.archivedAt ? "archive" : viewFor(shown);
    setFreshIds(new Set([id]));
    if (view !== target || profileId !== null) {
      setPendingSelect(id);
      setProfileId(null);
      setView(target);
    } else {
      await reload();
      setSelectedId(id);
    }
  }

  /** "Add offer" asks before saving: is the same offer of the same portal already there? */
  async function checkSame(offer: OfferInput): Promise<SameOffer | null> {
    if (!api) return null;
    try {
      return await api.checkSameOffer(offer);
    } catch (e) {
      fail(e);
      return null;
    }
  }

  /** "Show it" - an identical offer collected before goes to "Own" first (the person chose it). */
  async function showSame(id: number, own: boolean): Promise<boolean> {
    if (!api) return false;
    try {
      if (own) await api.setOwn(id, true);
      setToast({ text: t(own ? "same.movedOwn" : "same.shown") });
      await showOnBoard(id);
      return true;
    } catch (e) {
      fail(e);
      return false;
    }
  }

  async function updateSame(id: number, offer: OfferInput, reading: string | null, keep: SameField[], renew: boolean): Promise<boolean> {
    if (!api) return false;
    try {
      await api.updateSameOffer(id, offer, reading, keep, renew);
      setToast({ text: t(renew ? "same.updated" : "same.filled") });
      await showOnBoard(id);
      return true;
    } catch (e) {
      fail(e);
      return false;
    }
  }

  /** "Fill in that one": what the form would change in offer `id`. */
  async function compareSame(id: number, offer: OfferInput): Promise<SameOffer | null> {
    if (!api) return null;
    try {
      return await api.compareOffer(id, offer);
    } catch (e) {
      fail(e);
      return null;
    }
  }

  /** AI's word about "probably the same" offers - no answer is said in place, not in a message. */
  async function judgeDuplicates(of: { offer: OfferInput } | { id: number }, ids: number[]): Promise<DupVerdict[] | null> {
    if (!api) return null;
    try {
      return await api.judgeDuplicates(of, ids);
    } catch {
      return null;
    }
  }

  /** "Check duplicates" on the card. */
  async function findDuplicates(id: number): Promise<DupCandidate[] | null> {
    if (!api) return null;
    try {
      return await api.findDuplicates(id);
    } catch (e) {
      fail(e);
      return null;
    }
  }

  /** "Join as a child": one tree; the card shows the parent (the child is under it). */
  const joinOffers = (id: number, other: number) =>
    run(
      () => api!.joinOffers(id, other),
      () => setToast({ text: t("dup.joined") }),
    );

  /** "The same company and title - the same ad?" answered on the card. */
  const answerDuplicate = (id: number, join: boolean) =>
    run(
      () => api!.answerDuplicate(id, join),
      () => setToast({ text: t(join ? "dup.joined" : "dup.keptSeparate") }),
    );

  /** "Detach" a child of the tree: an offer of its own; "Undo" puts it back under its parent. */
  const detach = (id: number, parent: number) =>
    run(
      () => api!.setOfferParent(id, null),
      () => setToast({ text: t("tree.detached"), undo: () => run(() => api!.setOfferParent(id, parent)) }),
    );

  /** "Renewed after your CV - send it again?" on the card. */
  const answerRenewed = (id: number, resent: boolean) => run(() => api!.answerRenewed(id, resent));

  async function addOffer(offer: OfferInput, reading: string | null, place?: { separate: true } | { parent: number }): Promise<boolean> {
    if (!api) return false;
    try {
      const outcome = await api.addOffer(offer, reading, place);
      if (outcome.status === "blocked") {
        setToast({ text: t("add.blocked") });
        return true;
      }
      // Show the offer in the tab where the filter put it (all profiles together).
      const added = await api.getOffer(outcome.id);
      // Added by hand = own (tab "Own"); a collected one stays in "Today" unless the filter rejected it.
      // Joined with an offer in the archive (Bin → Expired): it is shown there.
      const target = added.archivedAt ? "archive" : !added.own && view === "today" && added.filterResult !== "fail" ? "today" : viewFor(added);
      // A new offer is read back: every field as the person confirmed it on the screen.
      const lost = outcome.status === "added" ? lostFields(offer, added) : [];
      const text =
        lost.length > 0
          ? t("add.lost", { fields: lost.map((f) => t(LOST_LABEL[f])).join(", ") })
          : outcome.status === "merged"
          ? t("add.merged")
          : outcome.status === "unchanged"
            ? t("add.known")
            : target === "own" || target === "board" || target === "today"
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

  /** Saves at once; the answer says whether it really got saved (a text field shows "Saved"). */
  function changeSetting<K extends keyof Settings>(key: K, value: Settings[K]): Promise<boolean> {
    setSettings((s) => ({ ...s, [key]: value }));
    if (!api) return Promise.resolve(false);
    return api.setSetting(key, value).then(
      () => {
        // Rust has sorted the offers again by the profiles that count now.
        if (key === "multi_profile") reloadNow.current();
        return true;
      },
      (e) => {
        fail(e);
        // Refused by Rust (wrong value) - show what is really saved.
        api.getSettings().then(setSettings, fail);
        return false;
      },
    );
  }

  const changeSettingRef = useRef(changeSetting);
  changeSettingRef.current = changeSetting;

  function openLink(url: string) {
    if (!isWebUrl(url)) return;
    // The made-up offers and contacts of the demo point at example.com - no page to show.
    if (DEMO && /^https?:\/\/([^/]+\.)?example\.com(\/|$)/i.test(url)) return fail(DEMO_ERROR);
    if ("__TAURI_INTERNALS__" in window) {
      import("@tauri-apps/plugin-opener").then((m) => m.openUrl(url)).catch(fail);
    } else {
      window.open(url, "_blank", "noopener,noreferrer");
    }
  }

  const locked = status !== null && !status.ok;

  /** The guide in bubbles: once on each device after "First steps", or again from Settings → About. */
  const tourOpen = !locked && !firstSteps && !settingsOpen && !adding && editing === null && (settings.tour === "pending" || tourAgain);
  const narrowNow = tourOpen && isNarrowLayout();
  const tourSteps: TourStep[] = [
    { target: ".topbar .tabs .tab", title: "tour.tabsTitle", text: collecting ? "tour.tabs" : "tour.tabsOwn" },
    { target: ".topbar .btn-add", title: "tour.addTitle", text: isPhone() ? "tour.addPhone" : "tour.add" },
    { target: ".board-wrap, .board-empty", title: "tour.listTitle", text: isPhone() ? "tour.listPhone" : narrowNow ? "tour.listNarrow" : "tour.list" },
    // The card stands next to the list only on a wide screen.
    ...(narrowNow ? [] : [{ target: ".pass", title: "tour.cardTitle", text: "tour.card" } as TourStep]),
    { target: ".topbar-actions .icon-btn", title: "tour.settingsTitle", text: isPhone() ? "tour.settingsPhone" : "tour.settings" },
  ];

  function finishTour() {
    setTourAgain(false);
    if (settings.tour === "pending") changeSetting("tour", "done");
  }

  function showTourAgain() {
    setSettingsOpen(false);
    // After the settings window has slid away.
    window.setTimeout(() => setTourAgain(true), REVEAL_MS);
  }

  /** Phone: the first date is when the question about notifications makes sense. */
  async function addEvent(event: Parameters<Api["addEvent"]>[1]): Promise<boolean> {
    if (selectedId === null) return false;
    const added = await act(() => api!.addEvent(selectedId, event));
    if (added && isPhone() && settings.notify_reminders === "on" && (await askNotifications())) setReplan((n) => n + 1);
    return added;
  }

  /** Collected's or the Bin's list now open ("Best today", "Expired"...); `null` in "Own". */
  const category = isCollected(view) ? t(`collected.${view}`) : isBin(view) ? t(`bin.${view}`) : null;

  /** "Empty the bin": what the bin's switch shows (all, or one kind), after a question with the number. */
  const emptyBin = isBin(view) && rows.length > 0 && (
    <button
      type="button"
      className="btn btn-danger"
      onClick={() =>
        setConfirm({
          message: t(view === "bin" ? "bin.confirmAll" : "bin.confirmKind", { count: rows.length, list: t(`bin.${view}`) }),
          action: () => {
            setSelectedId(null);
            return run(() => api!.purgeOffers(null, view), () => setToast({ text: t("trash.purged") }));
          },
        })
      }
    >
      {t("trash.empty")}
    </button>
  );

  return (
    <I18nContext.Provider value={i18n}>
      <ToastContext.Provider value={toastValue}>
      <ProfileDraftContext.Provider
        value={{ on: profileDraft && !multiProfile && !draftDismissed, open: () => openSettings("profile"), dismiss: () => setDraftDismissed(true) }}
      >
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
            {shownTabs.map((tab) => (
              <button
                key={tab}
                type="button"
                className="tab"
                aria-current={tabOf(view) === tab ? "page" : undefined}
                onClick={() =>
                  setView(tab === "collected" ? (isCollected(view) ? view : lastCollected) : tab === "bin" ? (isBin(view) ? view : lastBin) : tab)
                }
              >
                {/* The phone's bottom bar: a pictogram and a short name (the full one stays for a screen reader). */}
                <span className="tab-icon">{TAB_ICONS[tab]}</span>
                <span className="tab-name">{t(`tabs.${tab}`)}</span>
                <span className="tab-short" aria-hidden="true">
                  {t(`tabs.short.${tab}`)}
                </span>
                {/* "Own": how many CVs are still to be sent. */}
                {counts && tab === "own" && counts.toSend > 0 && <span className="tab-count">{counts.toSend}</span>}
                {/* "Collected": always how many offers "Best today" has - the number does not jump with the switch. */}
                {counts && tab === "collected" && <span className="tab-count tab-count-narrow">{counts.today}</span>}
              </button>
            ))}
          </nav>
          <div className="topbar-actions">
            {/* Empties what the bin's switch shows; on a phone it stands by the bin's search, so the top bar keeps one row. */}
            {!isPhone() && emptyBin}
            {!isPhone() && collecting && (
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

        {status?.version.endsWith("preview") && (
          <p className="preview-badge">
            {t(DEMO ? "demo.badge" : "preview.badge")}
            {/* The guide is published with the demo; a new tab, since the demo sits in a frame. */}
            {DEMO && (
              <>
                {" · "}
                <a href="/instrukcja/" target="_blank" rel="noopener">
                  {t("demo.guide")}
                </a>
                {" · "}
                <a href={DEMO_REPO} target="_blank" rel="noopener noreferrer">
                  {t("about.repo")}
                </a>
              </>
            )}
          </p>
        )}

        <Reveal show={locked || error !== null}>
          {(locked || error) && (
          <div className="alert" role="alert">
            <IconWarning />
            <div>
              <strong>{t("error.title")}: </strong>
              {locked ? t(isPhone() ? "error.lockedPhone" : "error.locked") : error?.text}
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
            {/* Profiles sort out collected offers; the bin keeps everything, whatever the profile. */}
            {!isBin(view) && multiProfile && (
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
                {t(`tabs.${tabOf(view)}`)}
                {/* Which of the tab's lists is open - the switch itself is usually hidden. */}
                {category && <span className="count"> · {category}</span>}
                {!isBin(view) && profileId !== null && (
                  <span className="count"> · {profiles.find((p) => p.id === profileId)?.name}</span>
                )}{" "}
                <span className="count">· {t(plural(lang, "board.count", sorted.length), { count: sorted.length })}</span>
              </h2>
              {settings.feature_keys === "on" && !isPhone() && <p className="keys-hint">{t(isBin(view) ? "board.keysBin" : view === "own" ? "board.keysOwn" : "board.keys")}</p>}
            </div>
            {/* One row of buttons under the heading in every tab: export on the left, what folds out on the right. */}
            <div className="board-tools">
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
              {/* A phone hides the heading (the tab says it) - the open list's name stands here instead. */}
              {category && (
                <span className="tools-category" aria-hidden="true">
                  {category}
                </span>
              )}
              <div className="board-tools-end">
                {category && (
                  <button
                    type="button"
                    className="btn btn-ghost btn-small"
                    aria-expanded={catsOpen}
                    aria-controls="list-cats"
                    onClick={() => setCatsOpen(!catsOpen)}
                  >
                    <IconList /> <FlipLabel open={catsOpen} show={t("cats.show")} hide={t("cats.hide")} />
                  </button>
                )}
                {filterChoices.length > 0 && (
                  <button
                    type="button"
                    className="btn btn-ghost btn-small"
                    aria-expanded={filtersOpen}
                    aria-controls="list-filters"
                    onClick={() => setFiltersOpen(!filtersOpen)}
                  >
                    <IconFilter /> <FlipLabel open={filtersOpen} show={t("narrow.show")} hide={t("narrow.hide")} />
                  </button>
                )}
              </div>
            </div>
            <div className="archive-search">
              <input
                type="search"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t(isBin(view) ? (isPhone() ? "bin.searchShort" : "bin.search") : isPhone() ? "list.searchShort" : "list.search")}
                aria-label={t(isBin(view) ? "bin.search" : "list.search")}
                maxLength={100}
              />
              {isPhone() && emptyBin}
            </div>
            {/* "Collected": best today / all / to check / rejected by the filter; "Bin": all / not for me /
                expired / deleted - with their numbers, under "Show categories". */}
            <Reveal show={catsOpen && category !== null}>
              {isCollected(view) && (
                <div id="list-cats" className="collected-switch" role="group" aria-label={t("collected.label")}>
                  {COLLECTED.map((v) => (
                    <button key={v} type="button" className="seg" aria-pressed={view === v} onClick={() => setView(v)}>
                      {t(`collected.${v}`)}
                      {counts && <span className="seg-count">{counts[v]}</span>}
                    </button>
                  ))}
                </div>
              )}
              {isBin(view) && (
                <div id="list-cats" className="collected-switch" role="group" aria-label={t("bin.label")}>
                  {BIN.map((v) => (
                    <button key={v} type="button" className="seg" aria-pressed={view === v} onClick={() => setView(v)}>
                      {t(`bin.${v}`)}
                      {counts && <span className="seg-count">{counts[v]}</span>}
                    </button>
                  ))}
                </div>
              )}
            </Reveal>
            <Reveal show={filtersOpen && filterChoices.length > 0}>
              <ListFilterBar rows={searched} filters={filters} onChange={setFilters} />
            </Reveal>
            <Board
              rows={sorted}
              changeKey={`${view}|${JSON.stringify(filters)}|${query.trim()}`}
              emptyText={
                searching && searched.length === 0 && rows.length > 0
                  ? t("list.searchEmpty", { query: query.trim() })
                  : searched.length === 0
                  ? undefined
                  : !filtering
                    ? t("narrow.englishHidden")
                  : filters.portal.length === 1 && FILTER_KEYS.every((k) => k === "portal" || filters[k].length === 0)
                    ? t("portal.empty", { portal: sourceLabel(filters.portal[0], t) })
                    : t("narrow.empty")
              }
              view={view}
              selectedId={selectedId}
              freshIds={freshIds}
              sort={shownSort}
              onSort={onSort}
              onSelect={(id) => {
                // A row chosen by hand (click, arrows) ends quick browsing with the keys.
                if (!cardOpen) setBrowseOrder(null);
                setSelectedId(id);
              }}
              onOpen={() => (isNarrowLayout() ? openCard() : cardHeading.current?.focus())}
              onInterest={toggleInterest}
              onTrash={trash}
              onRestore={restore}
              onExpired={expire}
              onOwn={toOwn}
              onNext={goNext}
            />
          </section>

          <OfferCard
            ref={cardHeading}
            offer={detail}
            onStage={(stage: Stage) => cardId !== null && setStage(cardId, stage)}
            onInterest={(i) => cardId !== null && setInterest(cardId, i)}
            onFavorite={(f) => cardId !== null && setFavorite(cardId, f)}
            onOwn={() => cardId !== null && toOwn(cardId)}
            onOwnChange={(own) => cardId !== null && (own ? toOwn(cardId) : toCollected(cardId))}
            onRenewed={(resent) => cardId !== null && answerRenewed(cardId, resent)}
            onShowOffer={(id) => setSelectedId(id)}
            onDetach={() => cardId !== null && detail?.parentId != null && detach(cardId, detail.parentId)}
            onDupAnswer={(join) => cardId !== null && answerDuplicate(cardId, join)}
            onFindDuplicates={() => (cardId !== null ? findDuplicates(cardId) : Promise.resolve(null))}
            onJudgeDuplicates={(ids) => (cardId !== null ? judgeDuplicates({ id: cardId }, ids) : Promise.resolve(null))}
            onJoin={(other) => cardId !== null && joinOffers(cardId, other)}
            onNotes={(id, n) => run(() => api!.setNotes(id, n), () => setToast({ text: t("card.notesSaved") }))}
            onOpenLink={openLink}
            onTrash={() => cardId !== null && trash(cardId)}
            onRestore={() => cardId !== null && restore(cardId)}
            onExpired={() => {
              setExpiredPage(null);
              return cardId !== null && expire(cardId);
            }}
            expiredPage={expiredPage?.id === cardId ? expiredPage.phrase : null}
            onKeepExpiredPage={() => setExpiredPage(null)}
            onSwipe={(value) => cardId !== null && swipeCard(cardId, value)}
            onPurge={() =>
              cardId !== null &&
              setConfirm({
                message: t("trash.confirmOne"),
                action: () => {
                  selectNeighbour(cardId);
                  return run(() => api!.purgeOffers(cardId, "bin"), () => setToast({ text: t("trash.purged") }));
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
            // Free and paid AI never mix: with Claude chosen every rating is Claude's, without it there is none.
            claudeOn={false}
            onRateAgain={() => selectedId !== null && rateAgain(selectedId)}
            onRatePrecise={() => selectedId !== null && rateAgain(selectedId, true)}
            onTranslate={() => (selectedId === null || !api ? Promise.resolve() : api.aiTranslate(selectedId).then(() => reload(), fail))}
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
            // A later "Add offer" starts empty, not with the old shared text.
            setShared(null);
            api?.closePreview().catch(() => {});
          }}
          onSave={addOffer}
          onCheck={checkSame}
          onShow={showSame}
          onUpdate={updateSame}
          onCompare={compareSame}
          onJudge={(offer, ids) => judgeDuplicates({ offer }, ids)}
          onCapture={fetchPage}
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
        <FirstSteps
          open={firstSteps}
          multi={multiProfile}
          collect={settings.collect === "on"}
          onCollect={(on) => changeSetting("collect", on ? "on" : "off")}
          onSaveProfile={saveProfile}
          onFinish={finishFirstSteps}
        />
        <Tour open={tourOpen} steps={tourSteps} onFinish={finishTour} />
        <SettingsDialog
          open={settingsOpen}
          api={api}
          settings={settings}
          profiles={profiles}
          profilePart={{
            experience,
            experiencePrev,
            aiOn: aiOn(settings),
            onSuggest: suggestCriteria,
            onCareers: suggestCareers,
            onTidy: tidyText,
            onSave: saveProfile,
            onDraft: setProfileDraft,
            onLoadExperience: () => {
              if (!api) return;
              Promise.all([api.getExperience(), api.getExperiencePrev()]).then(([text, prev]) => {
                setExperience(text);
                setExperiencePrev(prev);
              }, fail);
            },
          }}
          initialSection={settingsSection}
          onClose={() => setSettingsOpen(false)}
          onChange={changeSetting}
          onOpenLink={openLink}
          onError={fail}
          onToast={(text) => setToast({ text })}
          onNotificationsAllowed={() => setReplan((n) => n + 1)}
          version={status?.version ?? ""}
          onConfirm={(message, action, label) => setConfirm({ message, action, label })}
          onTour={showTourAgain}
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
      </ProfileDraftContext.Provider>
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
    // What happened and what to do, in plain words (the technical text is in the source's settings).
    const error = failed[0].error;
    const why = error ? ` ${t(`src.problem.${sourceProblem(error.detail)}` as TextKey)}` : "";
    parts.push(`${t("fetch.failed", { list: names })}.${why}`);
  }
  if (parts.length > 0) return parts.join(" · ");
  if (report.runs.length === 0) return t("fetch.noSources");
  const ran = report.runs.some((r) => !r.skipped);
  const skipped = report.runs.find((r) => r.skipped);
  if (!ran && skipped?.skipped) return t(`fetch.skip.${skipped.skipped}` as TextKey, { calls: JOOBLE_KEY_CALLS });
  return t("fetch.none");
}

/** A button's two names in one place - it is as wide as the longer one, so it never jumps when the name changes. */
function FlipLabel({ open, show, hide }: { open: boolean; show: string; hide: string }) {
  return (
    <span className="flip-label">
      <span aria-hidden={open || undefined}>{show}</span>
      <span aria-hidden={!open || undefined}>{hide}</span>
    </span>
  );
}
