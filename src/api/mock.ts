// In-memory stand-in for the Rust side, used only when the UI runs in a normal browser
// (`npm run dev`) - for design work and screenshots. Data is made up and not saved.
// Filter results are precomputed in the demo data; the real filter runs only in Rust.
// `?firstrun` in the address starts without profiles and offers, with "First steps".

import { shownScore } from "../lib/format";
import { DEMO_OFFERS, DEMO_PROFILES, type DemoFilter } from "../demo/offers";
import { AI_PROVIDERS, aiOn, type AiProvider, type AiStatus, type CareerIdea, type Criteria, type LowFunds, type OfferAi, type RateReport, type WorkProgress } from "./types";
import { bestResult, isKept, viewFor } from "../lib/profile";
import { JOOBLE_KEY_CALLS } from "../lib/limits";
import type { Api } from "./index";
import { DEMO, DEMO_ERROR } from "../lib/demo";
import { csvFileName, csvText } from "../lib/csv";
import { DEFAULT_SETTINGS, type DupCandidate, type EnglishNeed, type FieldNote, type OfferDetail, type OfferInput, type OfferRow, type SentVia, type SameField, type SameOffer, type SameWas, type Profile, type SecretHints, type SecretName, type Settings, type SourceStatus, type View } from "./types";

/** "English required?" as Rust `english.rs` reads it - a smaller copy for the browser demo. */
function englishOf(o: Pick<OfferInput, "title" | "language" | "requirements" | "description">): EnglishNeed {
  if (o.language === "en") return "required";
  let found: EnglishNeed = "none";
  for (const part of [o.title, o.requirements, o.description].filter(Boolean).join("\n").toLowerCase().split(/[\n.;•]/)) {
    if (!part.includes("angielsk") && !part.includes("english")) continue;
    if (/mile widzian|atut|nice to have|is a plus|advantage|preferred/.test(part)) found = "nice";
    else return "required";
  }
  return found;
}

/** Listeners of the line at the bottom ("work-progress"). */
const progressListeners = new Set<(step: WorkProgress | null) => void>();
/** Made-up models "in Ollama". */
const ollamaModels = ["llama3.2:3b"];
const tellProgress = (step: WorkProgress | null) => progressListeners.forEach((l) => l(step));
/** In the website demo: calls that need the real app (internet, files, the system) stop here. */
const outside = () => {
  if (DEMO) throw DEMO_ERROR;
};

const hoursAgo = (h: number) => new Date(Date.now() - h * 3_600_000).toISOString().replace(/\.\d{3}Z$/, "Z");

export function createMockApi(): Api {
  const firstRun = typeof location !== "undefined" && new URLSearchParams(location.search).has("firstrun");
  let nextId = 1;
  const offers = new Map<number, OfferDetail>();
  /** offer id → profile id → filter result */
  const filters = new Map<number, Map<number, DemoFilter>>();
  let profiles: Profile[] = firstRun
    ? []
    : DEMO_PROFILES.map((p, i) => ({ id: i + 1, position: i, descriptionPrev: null, ...structuredClone(p) }));
  let nextProfileId = profiles.length + 1;
  let experience = firstRun ? "" : "4 lata jako frontend developer (React, TypeScript), angielski B2.";
  let experiencePrev: string | null = null;
  let nextEventId = 1;
  let syncConnected = false;
  let aiCalls = 3;
  // "Rate now" in the preview: a slow run that "Stop rating" can end.
  let mockRunning = false;
  let mockStop = false;
  // Made up: what the app "spent" on Claude since the top-up (the real one is counted in Rust).
  const mockSpent = 1.23;
  const mockLow = (): LowFunds | null => {
    const topUp = Number(settings.ai_topup_usd);
    if (!settings.ai_topup_date || !(topUp > 0)) return null;
    const left = Math.max(0, topUp - mockSpent);
    return left < topUp * 0.1 ? { left, topUp } : null;
  };

  /** Made-up AI reading of a demo offer (profile index -> profile id). */
  function demoAi(
    ai: (typeof DEMO_OFFERS)[number]["ai"],
    score: number | null,
    source: "ai" | "keywords" | null,
    extra: Pick<OfferAi, "descriptionTranslated" | "offerLanguage"> = { descriptionTranslated: null, offerLanguage: null },
  ): OfferAi {
    if (ai) {
      return {
        ...ai,
        precise: false,
        ...extra,
        scores: ai.scores.map((s) => ({
          profileId: s.profile + 1,
          profileName: DEMO_PROFILES[s.profile].name,
          score: s.score,
          source: "ai" as const,
          why: s.why,
          quote: s.quote,
        })),
      };
    }
    return {
      status: source === "ai" ? "done" : null,
      at: null,
      summary: null,
      flags: [],
      remoteQuote: null,
      error: null,
      precise: false,
      ...extra,
      scores:
        score === null
          ? []
          : [{ profileId: 1, profileName: DEMO_PROFILES[0].name, score, source: source ?? "keywords", why: null, quote: null }],
    };
  }
  const settings: Settings = {
    ...DEFAULT_SETTINGS,
    onboarding: firstRun ? "pending" : "done",
    // The guide in bubbles: in the website demo for every visitor; in the preview after first steps or with `?tour`.
    tour: DEMO || firstRun || (typeof location !== "undefined" && new URLSearchParams(location.search).has("tour")) ? "pending" : "done",
    source_remotive: "on",
    // The demo shows a working setup: paid Claude chosen, a few sources on (nothing is called).
    ...(DEMO ? { ai_mode: "claude", source_himalayas: "on", source_weworkremotely: "on", source_mail: "on", mail_user: "alerty.przyklad@example.com" } : {}),
  };
  const secrets: SecretHints = DEMO ? { "ai-key-anthropic": "••••demo", "mail-password": "••••demo" } : {};
  const secretValues: Partial<Record<SecretName, string>> = {};
  const sourceRuns: Partial<Record<string, Pick<SourceStatus, "lastTry" | "lastOk" | "lastError" | "added" | "merged" | "callsToday">>> = {
    remotive: { lastTry: hoursAgo(5), lastOk: hoursAgo(5), lastError: null, added: 4, merged: 1, callsToday: 2 },
    ...(DEMO
      ? {
          himalayas: { lastTry: hoursAgo(5), lastOk: hoursAgo(5), lastError: null, added: 2, merged: 0, callsToday: 3 },
          weworkremotely: { lastTry: hoursAgo(5), lastOk: hoursAgo(5), lastError: null, added: 1, merged: 1, callsToday: 1 },
          mail: { lastTry: hoursAgo(1), lastOk: hoursAgo(1), lastError: null, added: 6, merged: 2, callsToday: 0 },
        }
      : {}),
  };

  /** The same company and title as another offer, not joined (`dup_of` in Rust). */
  const dupOf = new Map<number, { other: number; why: "city" | "salary" }>();

  for (const demo of firstRun ? [] : DEMO_OFFERS) {
    const id = nextId++;
    const added = hoursAgo(demo.hoursAgo);
    const { input } = demo;
    // Added by hand starts as "interested", like in Rust (`offers::add`).
    const stage = demo.stage ?? (demo.input.source === "manual" ? "interested" : "new");
    offers.set(id, {
      id,
      title: input.title,
      company: input.company,
      city: input.city ?? null,
      remote: input.remote ?? "unknown",
      salaryMin: input.salaryMin ?? null,
      salaryMax: input.salaryMax ?? null,
      salaryCurrency: input.salaryMin || input.salaryMax ? (input.salaryCurrency ?? "PLN") : null,
      salaryPeriod: input.salaryMin || input.salaryMax ? (input.salaryPeriod ?? "month") : null,
      english: englishOf(input),
      stage,
      stageChangedAt: added,
      interest: demo.interest ?? null,
      favorite: demo.favorite ?? false,
      score: demo.score ?? null,
      scoreSource: demo.scoreSource ?? null,
      sources: [input.source],
      expired: false,
      filterResult: "pass",
      filterReasons: [],
      addedAt: added,
      nextEvent: demo.event ? hoursAgo(-demo.event.inHours) : null,
      nextEventKind: demo.event ? demo.event.kind : null,
      translationLang: demo.translation ? "pl" : null,
      titleTranslated: demo.translation?.title ?? null,
      publishedAt: added,
      stale: false,
      inRecruitment: false,
      manual: false,
      archivedAt: null,
      scoreWhy: null,
      learned: demo.learned ?? 0,
      learnedWords: demo.learnedWords ?? [],
      noLink: false,
      own: input.source === "manual" || stage !== "new",
      bin: null,
      renewAsked: demo.renewedHoursAgo !== undefined,
      parentId: null,
      dupAsk: false,
      sentAt: null,
      sentVia: null,
      children: [],
      url: input.url ?? null,
      contract: input.contract ?? null,
      level: input.level ?? null,
      language: input.language ?? null,
      requirements: input.requirements ?? null,
      description: input.description ?? null,
      notes: demo.notes ?? "",
      expiresAt: null,
      deletedAt: demo.deletedHoursAgo === undefined ? null : hoursAgo(demo.deletedHoursAgo),
      contentRemoved: null,
      renewedAt: demo.renewedHoursAgo === undefined ? null : hoursAgo(demo.renewedHoursAgo),
      links: [
        ...(demo.linkCities ?? [[null, input.url ?? null] as const]).map(([city, url]) => ({
          source: input.source,
          url,
          firstSeen: added,
          city,
        })),
      ],
      history: stage === "new" ? [{ stage, at: added }] : [{ stage: "new", at: added }, { stage, at: added }],
      filters: [],
      ai: demoAi(demo.ai, demo.score ?? null, demo.scoreSource ?? null, {
        descriptionTranslated: demo.translation?.description ?? null,
        offerLanguage: input.language === "pl" || input.language === "en" ? input.language : null,
      }),
      agenda: demo.event
        ? {
            events: [{ id: nextEventId++, offerId: id, kind: demo.event.kind, at: hoursAgo(-demo.event.inHours), minutes: 60, note: demo.event.note }],
            contacts: demo.contact ? [{ id: nextEventId++, ...demo.contact }] : [],
            log: [],
          }
        : { events: [], contacts: [], log: [] },
      dup: null,
    });
    if (demo.dupOf) {
      const dup = demo.dupOf;
      const other = [...offers.values()].find((o) => o.id !== id && o.parentId === null && o.title === dup.title);
      if (other) dupOf.set(id, { other: other.id, why: dup.why });
    }
    const byProfile = new Map<number, DemoFilter>();
    profiles.forEach((p, i) => byProfile.set(p.id, demo.filters?.[i] ?? { result: "pass" }));
    filters.set(id, byProfile);
    // The same job on other portals: children in the tree (as `tree::convert` in Rust).
    for (const [i, source] of (demo.extraSources ?? []).entries()) {
      const parent = offers.get(id)!;
      const childId = nextId++;
      // Found later than the parent, but never in the future: spread between the parent and now.
      const seen = hoursAgo(demo.hoursAgo * (1 - (i + 1) / ((demo.extraSources?.length ?? 0) + 1)));
      const url = `${input.url ?? "https://example.com/oferty/x"}-${source}`;
      offers.set(childId, {
        ...structuredClone(parent),
        id: childId,
        parentId: id,
        // Own when the parent is (as in Rust); its own status starts as "new".
        own: parent.own || parent.sources.includes("manual") || parent.stage !== "new",
        stage: "new",
        stageChangedAt: seen,
        interest: null,
        favorite: false,
        notes: "",
        sources: [source],
        addedAt: seen,
        publishedAt: seen,
        url,
        links: [{ source, url, firstSeen: seen, city: null }],
        history: [{ stage: "new", at: seen }],
        agenda: { events: [], contacts: [], log: [] },
        nextEvent: null,
        nextEventKind: null,
        renewAsked: false,
        renewedAt: null,
      });
      filters.set(childId, byProfile);
      if (demo.sentThrough === source) {
        const sent = seen;
        Object.assign(offers.get(childId)!, { own: true, stage: "applied", stageChangedAt: sent, history: [{ stage: "new", at: seen }, { stage: "applied", at: sent }] });
        for (const f of offers.values()) if (f.id === id || f.parentId === id) f.own = true;
      }
    }
  }

  const find = (id: number) => {
    const offer = offers.get(id);
    if (!offer) throw { code: "not_found", detail: "not found" };
    return offer;
  };
  const now = () => new Date().toISOString().replace(/\.\d{3}Z$/, "Z");

  /** Result and reasons for one profile, or the best one over all profiles. */
  function filterOf(id: number, profile: number | null): DemoFilter {
    const byProfile = filters.get(id) ?? new Map();
    if (profile !== null) return byProfile.get(profile) ?? { result: "pass" };
    const ordered = profiles.map((p) => byProfile.get(p.id) ?? { result: "pass" as const });
    const result = bestResult(ordered.map((f) => f.result));
    return ordered.find((f) => f.result === result) ?? { result };
  }

  /** Brought back from the archive - never leaves by itself again (`kept_at` in Rust). */
  const kept = new Set<number>();
  const freshMs = () => Number(settings.fresh_days) * 24 * 3_600_000;
  const isOld = (o: OfferDetail) => Date.now() - Date.parse(o.publishedAt) > freshMs();
  const inRecruitment = (o: OfferDetail) => o.stage !== "new" && o.stage !== "interested";
  const isManual = (o: OfferDetail) => o.sources.includes("manual");
  /** As `TOUCHED` in offers.rs. */
  const touched = (o: OfferDetail) =>
    o.stage !== "new" || o.favorite || o.interest !== null || o.notes !== "" || kept.has(o.id) || isManual(o) ||
    o.agenda.events.length + o.agenda.contacts.length + o.agenda.log.length > 0;
  /** As `archive_old` in offers.rs - runs before every list. */
  function archiveOld() {
    for (const o of offers.values()) if (o.parentId === null && !o.deletedAt && !o.archivedAt && !touched(o) && isOld(o)) o.archivedAt = now();
  }

  const sameWas = (o: OfferDetail): SameWas => ({
    id: o.id,
    title: o.title,
    company: o.company,
    city: o.city,
    remote: o.remote,
    salaryMin: o.salaryMin,
    salaryMax: o.salaryMax,
    salaryCurrency: o.salaryCurrency,
    salaryPeriod: o.salaryPeriod,
    contract: o.contract,
    level: o.level,
    publishedAt: o.publishedAt,
    expiresAt: o.expiresAt,
    own: row(o, null).own,
    stage: o.stage,
    appliedAt: sentAt(o),
    sentVia: sentVia(o.parentId ?? o.id, false),
  });
  /** As `SENT_AT` in offers.rs. */
  const sentAt = (o: OfferDetail) =>
    inRecruitment(o) ? (o.history.find((h) => h.stage !== "new" && h.stage !== "interested")?.at ?? o.stageChangedAt) : null;
  /** As `tree::sent_via` in Rust (the portal is the source here). */
  function sentVia(root: number, childrenOnly: boolean): SentVia | null {
    const sent = [...offers.values()]
      .filter((o) => (o.parentId === root || (!childrenOnly && o.id === root)) && !o.deletedAt && inRecruitment(o))
      .map((o) => ({ id: o.id, source: o.sources.find((s) => s !== "manual") ?? o.sources[0] ?? "manual", at: sentAt(o)! }))
      .sort((a, b) => a.at.localeCompare(b.at) || a.id - b.id);
    return sent[0] ?? null;
  }
  function differences(o: OfferDetail, input: OfferInput): SameField[] {
    const fields: SameField[] = [];
    const norm = (v: string | null | undefined) => (v ?? "").trim().toLowerCase();
    const day = (v: string | null | undefined) => (v ?? "").slice(0, 10);
    if (norm(o.title) !== norm(input.title)) fields.push("title");
    if (norm(o.company) !== norm(input.company) && !/^klient|^\? \(/i.test(input.company)) fields.push("company");
    if (input.city && !norm(o.city).includes(norm(input.city))) fields.push("city");
    if (input.remote && input.remote !== "unknown" && input.remote !== o.remote) fields.push("remote");
    if ((input.salaryMin ?? input.salaryMax) != null && (input.salaryMin !== o.salaryMin || input.salaryMax !== o.salaryMax)) fields.push("salary");
    if (input.contract && norm(input.contract) !== norm(o.contract)) fields.push("contract");
    if (input.level && norm(input.level) !== norm(o.level)) fields.push("level");
    if (input.publishedAt && day(input.publishedAt) !== day(o.publishedAt)) fields.push("published");
    if (input.expiresAt && day(input.expiresAt) !== day(o.expiresAt)) fields.push("expires");
    if (input.description && input.description.trim() !== (o.description ?? "").trim()) fields.push("description");
    return fields;
  }
  /** Of the differences, the fields the offer does not have yet (as `same::filled` in Rust). */
  function filledOf(o: OfferDetail, input: OfferInput, fields: SameField[]): SameField[] {
    return fields.filter((f) =>
      f === "company"
        ? /^klient|^\? \(/i.test(o.company)
        : f === "city"
          ? !o.city
          : f === "remote"
            ? o.remote === "unknown"
            : f === "salary"
              ? o.salaryMin === null && o.salaryMax === null
              : f === "contract"
                ? !o.contract
                : f === "level"
                  ? !o.level
                  : f === "expires"
                    ? !o.expiresAt
                    : f === "description"
                      ? (input.description ?? "").trim().length > (o.description ?? "").trim().length
                      : false,
    );
  }
  /** "Probably the same" (as `dups::candidates` in Rust, simpler): the same company with a title of
   *  a shared word, or a hidden company in the same city. */
  function findCandidates(input: Pick<OfferInput, "title" | "company" | "city">, skip: number[]): DupCandidate[] {
    const hidden = (c: string) => /^klient|^\? \(/i.test(c);
    const words = (t: string) => new Set(t.toLowerCase().split(/[^a-ząćęłńóśźż0-9]+/).filter((w) => w.length > 2));
    return [...offers.values()]
      .filter((o) => !o.deletedAt && o.parentId === null && !skip.includes(o.id))
      .filter((o) => {
        const sameCompany = !hidden(input.company) && !hidden(o.company) && o.company.toLowerCase() === input.company.toLowerCase();
        const sameCity = Boolean(input.city && o.city?.toLowerCase().includes(input.city.toLowerCase()));
        const shared = [...words(o.title)].some((w) => words(input.title).has(w));
        return (sameCompany && shared) || ((hidden(input.company) || hidden(o.company)) && sameCity && shared);
      })
      .slice(0, 5)
      .map((o) => ({
        was: sameWas(o),
        sources: o.sources,
        reasons: hidden(input.company) || hidden(o.company) ? ["hidden", "city", "titleWords"] : ["company", "titleWords"],
        similarity: 40,
      }));
  }
  /** The question "the same ad?" (as `dups::note` in Rust): the other offer must be on the board. */
  function dupNote(id: number): { other: OfferDetail; why: "city" | "salary" } | null {
    const d = dupOf.get(id);
    const other = d ? offers.get(d.other) : undefined;
    return d && other && !other.deletedAt && other.parentId === null ? { other, why: d.why } : null;
  }

  /** As `portal_label` in offers.rs, simpler: a known portal by its domain, any other by its host. */
  function portalOf(url: string): string {
    const PORTALS: Record<string, string> = {
      "pracuj.pl": "pracuj",
      "nofluffjobs.com": "nfj",
      "justjoin.it": "jjit",
      "rocketjobs.pl": "rocketjobs",
      "theprotocol.it": "protocol",
      "bulldogjob.pl": "bulldogjob",
      "linkedin.com": "linkedin",
      "indeed.com": "indeed",
    };
    try {
      const host = new URL(url).hostname.replace(/^www\./, "");
      return Object.entries(PORTALS).find(([d]) => host === d || host.endsWith(`.${d}`))?.[1] ?? host;
    } catch {
      return "manual";
    }
  }

  function row(o: OfferDetail, profile: number | null): OfferDetail {
    const f = filterOf(o.id, profile);
    const manualUrl = o.links.find((l) => l.source === "manual")?.url ?? o.url;
    return {
      ...o,
      // As `shown_source` in offers.rs: an ad added by hand with a link is named after its portal.
      sources: [...new Set(o.sources.map((s) => (s === "manual" && manualUrl ? portalOf(manualUrl) : s)))].sort(),
      links: o.links.map((l) => (l.source === "manual" && l.url ? { ...l, source: portalOf(l.url) } : l)),
      dupAsk: dupNote(o.id) !== null,
      sentAt: sentAt(o),
      sentVia: o.parentId === null ? sentVia(o.id, true) : null,
      filterResult: f.result,
      filterReasons: f.reasons ?? [],
      stale: isOld(o) && !inRecruitment(o) && !o.expiresAt,
      inRecruitment: inRecruitment(o),
      manual: isManual(o),
      noLink: !o.url && o.links.every((l) => !l.url),
      own: o.own,
      bin: o.deletedAt ? "deleted" : o.archivedAt ? "expired" : o.interest === "no" ? "declined" : null,
      scoreWhy: o.ai.scores.find((s) => s.source === "ai")?.why ?? null,
      children: [...offers.values()]
        .filter((c) => c.parentId === o.id && !c.deletedAt)
        .sort((a, b) => a.addedAt.localeCompare(b.addedAt) || a.id - b.id)
        .map((c) => ({ ...row(c, profile), children: [] })),
    };
  }


  function inView(o: OfferDetail, view: View): boolean {
    if (view === "bin") return o.deletedAt !== null || o.archivedAt !== null || o.interest === "no";
    if (view === "trash") return o.deletedAt !== null;
    if (o.deletedAt) return false;
    if (view === "archive") return o.archivedAt !== null;
    if (o.archivedAt) return false;
    if (view === "today") {
      return !o.own && o.interest !== "no" && (o.filterResult !== "fail" || isKept(o)) && !inRecruitment(o) && !o.expired && !isOld(o);
    }
    // "All" in Collected holds "to check" too (as in offers.rs).
    if (view === "board") return viewFor(o) === "board" || viewFor(o) === "review";
    return viewFor(o) === view;
  }

  const pinned = (o: OfferDetail) => o.favorite || isManual(o);
  const rowsOf = (view: View, profile: number | null) => {
    archiveOld();
    // Children stay under their parent (a child deleted by itself is in the bin on its own).
    const top = (o: OfferDetail) => o.parentId === null || ((view === "bin" || view === "trash") && o.deletedAt !== null);
    const rows = [...offers.values()].filter(top).map((o) => row(o, profile)).filter((o) => inView(o, view));
    // "Own": the CV still to be sent, then recruitments, then the ones that are over (as in offers.rs).
    const group = (o: OfferRow) => (["new", "interested"].includes(o.stage) && !o.sentVia ? 0 : ["rejected", "withdrawn"].includes(o.stage) ? 2 : 1);
    if (view === "own") {
      return rows.sort((a, b) => group(a) - group(b) || Number(a.noLink) - Number(b.noLink) || b.publishedAt.localeCompare(a.publishedAt));
    }
    if (view === "bin") {
      // The last thrown in on top (as in offers.rs; "not for me" has no date here - after the rest).
      const at = (id: number) => offers.get(id)?.deletedAt ?? offers.get(id)?.archivedAt ?? "";
      return rows.sort((a, b) => at(b.id).localeCompare(at(a.id)) || b.id - a.id);
    }
    if (view !== "today") return rows.sort((a, b) => b.publishedAt.localeCompare(a.publishedAt));
    rows.sort(
      (a, b) =>
        Number(pinned(b)) - Number(pinned(a)) ||
        Number(a.noLink) - Number(b.noLink) ||
        Number(a.score === null) - Number(b.score === null) ||
        (shownScore(b) ?? 0) - (shownScore(a) ?? 0) ||
        b.publishedAt.localeCompare(a.publishedAt),
    );
    const room = Math.max(Number(settings.today_count), rows.filter(pinned).length);
    return rows.filter((o, i) => pinned(o) || i < room);
  };

  return {
    async appStatus() {
      return { ok: true, problem: null, version: "0.1.0-preview" };
    },
    async listOffers(view, profile) {
      return rowsOf(view, profile);
    },
    async countOffers(profile) {
      return {
        toSend: [...offers.values()].filter((o) => !o.deletedAt && !o.archivedAt && o.parentId === null && row(o, null).own && ["new", "interested"].includes(o.stage) && !sentVia(o.id, true) && o.interest !== "no").length,
        today: rowsOf("today", profile).length,
        board: rowsOf("board", profile).length,
        review: rowsOf("review", profile).length,
        rejected: rowsOf("rejected", profile).length,
        declined: rowsOf("declined", profile).length,
        archive: rowsOf("archive", profile).length,
        trash: rowsOf("trash", profile).length,
        bin: rowsOf("bin", profile).length,
      };
    },
    async getOffer(id) {
      const o = find(id);
      const note = dupNote(id);
      return {
        ...structuredClone(row(o, null)),
        dup: note ? { was: sameWas(note.other), sources: note.other.sources, why: note.why } : null,
        filters: profiles.map((p) => {
          const f = filterOf(id, p.id);
          return { profileId: p.id, profileName: p.name, result: f.result, reasons: f.reasons ?? [] };
        }),
      };
    },
    async addOffer(input, _reading, place) {
      const key = (input.company + "|" + input.title).toLowerCase();
      let parent: OfferDetail | null = place && "parent" in place ? find(place.parent) : null;
      let dup: { other: number; why: "city" | "salary" } | null = null;
      for (const o of place ? [] : offers.values()) {
        if (o.parentId === null && (o.company + "|" + o.title).toLowerCase() === key) {
          if (o.deletedAt) return { status: "blocked" };
          // Another city: a separate offer with the question (as `dups::first_fit` in Rust).
          if (input.city && o.city && !o.city.toLowerCase().includes(input.city.toLowerCase())) {
            dup = { other: o.id, why: "city" };
            break;
          }
          // Another ad of the same job: a child (as `tree::add_child` in Rust).
          const family = [...offers.values()].filter((c) => c.id === o.id || c.parentId === o.id);
          const known = family.some((c) => c.url === input.url || c.links.some((l) => l.url === input.url));
          if (input.url && !known && family.some((c) => c.url || c.links.some((l) => l.url))) {
            parent = o;
            break;
          }
          if (!o.sources.includes(input.source)) o.sources.push(input.source);
          return { status: "merged", id: o.id };
        }
      }
      const id = nextId++;
      const at = now();
      offers.set(id, {
        id,
        title: input.title.trim(),
        // No company: "? (…)" like the app ("unknown company").
        company: input.company.trim() || `? (${id.toString(16).padStart(8, "0")})`,
        city: input.city ?? null,
        remote: input.remote ?? "unknown",
        salaryMin: input.salaryMin ?? null,
        salaryMax: input.salaryMax ?? null,
        salaryCurrency: input.salaryMin || input.salaryMax ? (input.salaryCurrency ?? "PLN") : null,
        english: englishOf(input),
        salaryPeriod: input.salaryMin || input.salaryMax ? (input.salaryPeriod ?? "month") : null,
        stage: input.source === "manual" ? "interested" : "new",
        stageChangedAt: at,
        interest: null,
        favorite: false,
        score: null,
        scoreSource: null,
        sources: [input.source],
        expired: false,
        filterResult: "pass",
        filterReasons: [],
        addedAt: at,
        nextEvent: null,
        nextEventKind: null,
        translationLang: null,
        titleTranslated: null,
        publishedAt: input.publishedAt ?? at,
        stale: false,
        inRecruitment: false,
        manual: false,
        archivedAt: null,
        scoreWhy: null,
        learned: 0,
        learnedWords: [],
        noLink: false,
        own: input.source === "manual",
        bin: null,
        renewAsked: false,
        parentId: null,
        dupAsk: false,
        sentAt: null,
        sentVia: null,
        children: [],
        url: input.url ?? null,
        contract: input.contract ?? null,
        level: input.level ?? null,
        language: input.language ?? null,
        requirements: input.requirements ?? null,
        description: input.description ?? null,
        notes: "",
        expiresAt: input.expiresAt ?? null,
        deletedAt: null,
        contentRemoved: null,
        renewedAt: null,
        links: [{ source: input.source, url: input.url ?? null, firstSeen: at, city: input.city ?? null }],
        history: [{ stage: input.source === "manual" ? "interested" : "new", at }],
        filters: [],
        ai: demoAi(undefined, null, null),
        agenda: { events: [], contacts: [], log: [] },
        dup: null,
      });
      filters.set(id, new Map());
      if (dup) dupOf.set(id, dup);
      if (parent) {
        Object.assign(offers.get(id)!, { parentId: parent.id, own: parent.own || input.source === "manual" });
        // Added by hand - the whole job is own (as `tree::add_child`).
        if (input.source === "manual") for (const f of offers.values()) if (f.id === parent.id || f.parentId === parent.id) f.own = true;
        return { status: "merged", id: parent.id };
      }
      return { status: "added", id };
    },
    async checkSameOffer(input) {
      const none: SameOffer = { kind: "none", was: null, fields: [], filled: [], similarity: 0, candidates: [] };
      const live = [...offers.values()].filter((o) => !o.deletedAt);
      // Another portal (as `same::other_portal` in Rust, simpler).
      const otherPortal = (): SameOffer => {
        const name = (o: { company: string; title: string }) => `${o.company}|${o.title}`.toLowerCase();
        const twin = live.find((o) => o.parentId === null && name(o) === name(input));
        if (twin) {
          const otherCity = Boolean(input.city && twin.city && !twin.city.toLowerCase().includes(input.city.toLowerCase()));
          if (otherCity) return { ...none, kind: "ask", was: sameWas(twin), fields: ["city"] };
          // Joins as a child by itself - said first when the CV already went for this job.
          const was = sameWas(twin);
          return was.sentVia ? { ...none, kind: "joined", was } : none;
        }
        const candidates = findCandidates(input, []);
        return candidates.length ? { ...none, kind: "probable", was: candidates[0].was, candidates, similarity: candidates[0].similarity } : none;
      };
      if (!input.url) return otherPortal();
      // As `same::check` in Rust, simpler: a link without its query, a portal by its host.
      const key = (u: string) => u.split(/[?#]/)[0].replace(/\/$/, "").toLowerCase();
      const host = (u: string) => {
        try {
          return new URL(u).hostname.replace(/^www\./, "");
        } catch {
          return null;
        }
      };
      const url = input.url;
      const linksOf = (o: OfferDetail) => [o.url, ...o.links.map((l) => l.url)].filter((u): u is string => Boolean(u));
      const was = sameWas;
      const differ = (o: OfferDetail) => differences(o, input);
      const same = live.find((o) => linksOf(o).some((u) => key(u) === key(url)));
      if (same) {
        const fields = differ(same);
        const filled = filledOf(same, input, fields);
        const kind = !fields.length ? "identical" : fields.every((f) => filled.includes(f)) ? "partial" : "changed";
        return { ...none, kind, was: was(same), fields, filled, similarity: fields.includes("description") ? 90 : 100 };
      }
      const name = (o: { company: string; title: string }) => `${o.company}|${o.title}`.toLowerCase();
      const twin = live.find((o) => name(o) === name(input) && linksOf(o).some((u) => host(u) === host(url)));
      if (!twin) return otherPortal();
      if (!twin.expired && !twin.archivedAt && twin.expiresAt !== null && twin.expiresAt > now()) {
        return { ...none, kind: "second", was: was(twin) };
      }
      // The preview has no real texts to compare - "almost the same".
      const fields = differ(twin);
      return { ...none, kind: "similar", was: was(twin), fields, filled: filledOf(twin, input, fields), similarity: 97 };
    },
    async compareOffer(id, input) {
      const o = find(id);
      const fields = differences(o, input);
      return { kind: "complete", was: sameWas(o), fields, filled: filledOf(o, input, fields), similarity: fields.includes("description") ? 60 : 100, candidates: [] };
    },
    async findDuplicates(id) {
      const o = find(id);
      const family = [...offers.values()].filter((c) => c.id === o.id || c.parentId === o.id || c.id === o.parentId).map((c) => c.id);
      return findCandidates(o, family);
    },
    async judgeDuplicates(_of, ids) {
      outside();
      await new Promise((r) => setTimeout(r, 600));
      return ids.map((id, i) => ({
        id,
        same: i === 0 ? ("maybe" as const) : ("no" as const),
        why: i === 0 ? "Ta sama firma i stanowisko, ale inne miasto - to raczej druga rekrutacja tej samej firmy." : "Inne obowiązki i inne widełki.",
      }));
    },
    async joinOffers(id, other) {
      const [a, b] = [find(id), find(other)];
      const [parent, child] = a.addedAt.localeCompare(b.addedAt) <= 0 ? [a, b] : [b, a];
      for (const c of offers.values()) if (c.parentId === child.id) c.parentId = parent.id;
      child.parentId = parent.id;
      dupOf.delete(child.id);
      return parent.id;
    },
    async answerDuplicate(id, join) {
      const note = dupOf.get(id);
      if (!note) throw { code: "not_found", detail: "not found" };
      dupOf.delete(id);
      return join ? this.joinOffers(id, note.other) : null;
    },
    async updateSameOffer(id, original, _reading, keep = [], renew = true) {
      const o = find(id);
      // The fields the person kept stay as they are (as `same::update` in Rust).
      const input: OfferInput = { ...original };
      for (const f of keep) {
        if (f === "title") input.title = o.title;
        if (f === "company") input.company = o.company;
        if (f === "salary") Object.assign(input, { salaryMin: null, salaryMax: null });
        if (f === "published") input.publishedAt = null;
        if (f === "expires") input.expiresAt = null;
        if (f === "city" || f === "remote" || f === "contract" || f === "level" || f === "description") input[f] = null;
      }
      const newer = Boolean(input.publishedAt && input.publishedAt.slice(0, 10) > o.publishedAt.slice(0, 10));
      const renewed = renew && (newer || o.expired || o.archivedAt !== null);
      Object.assign(o, {
        title: input.title,
        company: input.company,
        city: input.city ?? o.city,
        remote: input.remote && input.remote !== "unknown" ? input.remote : o.remote,
        contract: input.contract ?? o.contract,
        level: input.level ?? o.level,
        description: input.description ?? o.description,
        publishedAt: newer ? input.publishedAt : o.publishedAt,
        expiresAt: input.expiresAt ?? o.expiresAt,
        expired: renew ? false : o.expired,
        archivedAt: renew ? null : o.archivedAt,
        own: true,
        renewedAt: renew ? now() : o.renewedAt,
        renewAsked: o.renewAsked || (renewed && inRecruitment(o)),
      });
      if ((input.salaryMin ?? input.salaryMax) != null) {
        Object.assign(o, {
          salaryMin: input.salaryMin ?? null,
          salaryMax: input.salaryMax ?? null,
          salaryCurrency: input.salaryCurrency ?? "PLN",
          salaryPeriod: input.salaryPeriod ?? "month",
        });
      }
      if (input.url && !o.links.some((l) => l.url === input.url)) o.links.push({ source: input.source, url: input.url, firstSeen: now(), city: input.city ?? null });
    },
    async setOfferParent(id, parent) {
      find(id).parentId = parent;
    },
    async answerRenewed(id, resent) {
      const o = find(id);
      o.renewAsked = false;
      if (resent) o.history.push({ stage: "applied", at: now() });
    },
    async setStage(id, stage) {
      const o = find(id);
      if (o.stage !== stage) {
        o.stage = stage;
        // Past "new" = own (as `set_stage` in Rust).
        if (stage !== "new") o.own = true;
        // A child's recruitment is the family's - the whole family moves to "Own".
        if (stage !== "new") {
          const root = o.parentId ?? o.id;
          for (const f of offers.values()) if (f.id === root || f.parentId === root) f.own = true;
        }
        o.stageChangedAt = now();
        o.history.push({ stage, at: o.stageChangedAt });
      }
    },
    async setInterest(id, interest) {
      find(id).interest = interest;
    },
    async setFavorite(id, favorite) {
      find(id).favorite = favorite;
    },
    async setOwn(id, own) {
      // One for the whole family (as `tree::set_family_own`).
      const root = find(id).parentId ?? id;
      for (const o of offers.values()) if (o.id === root || o.parentId === root) o.own = own;
    },
    async setNotes(id, notes) {
      find(id).notes = notes.trim();
    },
    async trashOffer(id) {
      find(id).deletedAt = now();
    },
    async restoreOffer(id) {
      find(id).deletedAt = null;
    },
    async setExpired(id) {
      const o = find(id);
      if (touched(o)) {
        o.expired = true;
        return "marked";
      }
      o.archivedAt = now();
      return "archived";
    },
    async unsetExpired(id) {
      const o = find(id);
      o.expired = false;
      o.archivedAt = null;
    },
    async bringBackOffer(id) {
      const o = find(id);
      const place = { deletedAt: o.deletedAt, archivedAt: o.archivedAt, keptAt: kept.has(id) ? now() : null, declined: o.interest === "no" };
      if (o.archivedAt) kept.add(id);
      o.deletedAt = null;
      o.archivedAt = null;
      if (o.interest === "no") o.interest = null;
      return place;
    },
    async putBackOffer(id, place) {
      const o = find(id);
      o.deletedAt = place.deletedAt;
      o.archivedAt = place.archivedAt;
      if (place.keptAt === null) kept.delete(id);
      if (place.declined) o.interest = "no";
    },
    async purgeOffers(id, view) {
      let count = 0;
      for (const o of [...offers.values()]) {
        if (inView(o, view) && (id === null || o.id === id)) {
          offers.delete(o.id);
          count++;
        }
      }
      return count;
    },
    async getSettings() {
      return { ...settings };
    },
    async setSetting(key, value) {
      settings[key] = value;
    },
    async listProfiles() {
      return structuredClone(profiles);
    },
    async saveProfile(input) {
      if (!input.name.trim() || !input.description.trim()) throw { code: "invalid", detail: "name and description are required" };
      if (input.id !== null) {
        const p = profiles.find((x) => x.id === input.id);
        if (!p) throw { code: "not_found", detail: "not found" };
        const description = input.description.trim();
        Object.assign(p, structuredClone({
          name: input.name.trim(),
          descriptionPrev: description !== p.description ? p.description : p.descriptionPrev,
          description,
          criteria: input.criteria,
        }));
        return p.id;
      }
      const id = nextProfileId++;
      profiles.push({
        id,
        descriptionPrev: null,
        position: profiles.length,
        name: input.name.trim(),
        description: input.description.trim(),
        criteria: structuredClone(input.criteria),
      });
      return id;
    },
    async deleteProfile(id) {
      profiles = profiles.filter((p) => p.id !== id);
      for (const byProfile of filters.values()) byProfile.delete(id);
    },
    async getExperience() {
      return experience;
    },
    async getExperiencePrev() {
      return experiencePrev;
    },
    async setExperience(text) {
      if (text.trim() !== experience) experiencePrev = experience;
      experience = text.trim();
    },
    async secretHints() {
      return { ...secrets };
    },
    async setSecret(name, value) {
      // Like `secrets::set_user`: an app password copied in four groups loses the spaces.
      const v = (name === "mail-password" ? value.split(" ").join("") : value).trim();
      if (!v || /\s/.test(v)) throw { code: "invalid", detail: "the key has a wrong format" };
      secrets[name] = v.length > 8 ? `••••${v.slice(-4)}` : "••••";
      secretValues[name] = v;
    },
    async deleteSecret(name) {
      delete secrets[name];
      delete secretValues[name];
    },
    async revealSecret(name) {
      return secretValues[name] ?? null;
    },
    async sourceStatus() {
      const one = (source: SourceStatus["source"], ready: boolean, callsADay: number, callsLeft: number | null = null): SourceStatus => ({
        source,
        enabled: settings[`source_${source}` as "source_adzuna"] === "on",
        ready,
        lastTry: null,
        lastOk: null,
        lastError: null,
        added: 0,
        merged: 0,
        callsToday: 0,
        ...sourceRuns[source],
        callsADay,
        callsLeft,
      });
      return [
        one("adzuna", Boolean(secrets["adzuna-app-id"] && secrets["adzuna-app-key"]), 25),
        one("remotive", true, 4),
        one("himalayas", true, 8),
        one("weworkremotely", true, 4),
        one("remoteok", true, 4),
        one("jooble", Boolean(secrets["jooble-key"]), 1, JOOBLE_KEY_CALLS),
        one("mail", Boolean(settings.mail_user && secrets["mail-password"]), 0),
      ];
    },
    async mailTest() {
      outside();
      await new Promise((r) => setTimeout(r, 600));
      if (!settings.mail_user || !secrets["mail-password"]) throw { code: "missing", detail: "mailbox" };
      return 42;
    },
    async sourceTest(source) {
      outside();
      await new Promise((r) => setTimeout(r, 600));
      const run = sourceRuns[source];
      if (run) run.lastError = null;
    },
    async fetchOffers() {
      outside();
      // A few made-up steps, slow enough to see the line at the bottom.
      const steps: WorkProgress[] = [
        { step: "remotive", source: null, done: 1, total: 2, part: 1, parts: 2 },
        { step: "wait", source: "remotive", done: 2, total: 2, part: 1, parts: 2 },
        { step: "mail", source: null, done: 1, total: 1, part: 2, parts: 2 },
        // The check of Himalayas offers goes after every source - not one of them.
        { step: "check", source: "himalayas", done: 3, total: 12, part: 0, parts: 0 },
      ];
      for (const step of steps) {
        tellProgress(step);
        await new Promise((r) => setTimeout(r, 1500));
      }
      tellProgress(null);
      const runs = [];
      let added = 0;
      if (settings.source_remotive === "on") {
        const outcome = await this.addOffer({
          title: "Frontend Engineer (React)",
          company: "Zmyślone Studio Zdalne",
          source: "remotive",
          url: "https://remotive.com/remote-jobs/software-dev/frontend-engineer-0",
          remote: "yes",
          description: "Candidate location: Europe\nJob type: full time\n\nWe build tools for small teams. React, TypeScript, CSS.",
        });
        const isNew = outcome.status === "added";
        added += isNew ? 1 : 0;
        sourceRuns.remotive = { lastTry: now(), lastOk: now(), lastError: null, added: isNew ? 1 : 0, merged: isNew ? 0 : 1, callsToday: 3 };
        runs.push({ source: "remotive" as const, skipped: null, added: isNew ? 1 : 0, merged: isNew ? 0 : 1, blocked: 0, invalid: 0, old: 0, error: null });
      }
      if (settings.source_adzuna === "on") {
        const ready = Boolean(secrets["adzuna-app-id"] && secrets["adzuna-app-key"]);
        runs.push({ source: "adzuna" as const, skipped: ready ? null : ("missing_key" as const), added: 0, merged: 0, blocked: 0, invalid: 0, old: 0, error: null });
      }
      return { runs, added, newOnBoard: added, newReview: 0, newRejected: 0, closed: 0 };
    },
    async onFetched() {
      return () => {};
    },
    async aiStatus(): Promise<AiStatus> {
      const defaults: Record<AiProvider, string> = {
        gemini: "gemini-3.5-flash-lite", groq: "openai/gpt-oss-120b", openrouter: "openrouter/free",
        ollama: "llama3.1:8b", anthropic: "claude-sonnet-5-5", openai: "gpt-6-luna",
      };
      // Paid Claude alone, or the free ones ticked - like `ai::configs`.
      const on = settings.ai_mode === "claude" ? ["anthropic"] : settings.ai_providers.split(",").filter((p) => p !== "anthropic");
      return {
        providers: AI_PROVIDERS.map((provider) => ({
          provider,
          enabled: on.includes(provider),
          ready: provider === "ollama" || Boolean(secrets[`ai-key-${provider}` as keyof typeof secrets]),
          model: settings[`ai_model_${provider}`] || defaults[provider],
          defaultModel: defaults[provider],
          callsToday: provider === "groq" ? aiCalls : 0,
          callsMonth: provider === "groq" ? aiCalls + 41 : 0,
          // Made up: Groq "said" its limit is used, to show the warning.
          usedUp: provider === "groq" && on.includes("groq") ? ("provider" as const) : null,
        })),
        dailyLimit: Number(settings.ai_daily_limit),
        waiting: [...offers.values()].filter((o) => o.ai.status === null && !o.deletedAt).length,
        rated: [...offers.values()].filter((o) => o.ai.status === "done").length,
        rateAgain: [...offers.values()].filter((o) => o.ai.status !== null && !o.deletedAt).length,
        rateAgainClaude: [...offers.values()].filter((o) => o.ai.status !== null && !o.deletedAt && o.sources.includes("manual")).length,
        // Made up, like the Claude calls of a few days.
        claudeSpent: { today: 0.04, month: mockSpent, sinceTopUp: settings.ai_topup_date ? mockSpent : null },
        claudeLow: mockLow(),
        running: mockRunning,
      };
    },
    async aiStop() {
      mockStop = true;
    },
    async usdRate() {
      // Made up - the preview asks nobody.
      return { pln: 3.65, day: "2026-11-13" };
    },
    async aiRateNow() {
      outside();
      if (!aiOn(settings)) throw { code: "missing", detail: "ai provider" };
      let rated = 0;
      let stopped = false;
      mockRunning = true;
      mockStop = false;
      for (const o of offers.values()) {
        if (o.ai.status === null && !o.deletedAt) {
          if (mockStop) {
            stopped = true;
            break;
          }
          await new Promise((r) => setTimeout(r, 900));
          o.ai = { ...o.ai, status: "done", summary: "Przykładowe streszczenie z podglądu - tu pisze AI.", flags: [] };
          rated++;
          aiCalls++;
        }
      }
      mockRunning = false;
      const waiting = [...offers.values()].filter((o) => o.ai.status === null && !o.deletedAt).length;
      // Made up: Gemini without a model for this account, Groq took over.
      const problems: RateReport["problems"] = settings.ai_providers.includes("gemini")
        ? [{ provider: "gemini", model: settings.ai_model_gemini || "gemini-3.5-flash-lite", kind: "model", message: "Quota exceeded for metric: generate_content_free_tier_requests, limit: 0" }]
        : [];
      return { rated, failed: 0, waiting, noProfile: 2, error: null, problems, lowFunds: null, stopped };
    },
    async aiRateAllAgain() {
      outside();
      for (const o of offers.values()) {
        if (!o.deletedAt) o.ai = { ...o.ai, status: null };
      }
      return this.aiRateNow();
    },
    async aiRateOffer(id, precise) {
      outside();
      const o = find(id);
      if (!aiOn(settings)) throw { code: "missing", detail: "ai provider" };
      await new Promise((r) => setTimeout(r, 700));
      o.ai = {
        ...o.ai,
        status: "done",
        summary: o.ai.summary ?? "Przykładowe streszczenie z podglądu - tu pisze AI.",
        precise: o.ai.precise || !!precise,
      };
      aiCalls++;
      return { rated: 1, failed: 0, waiting: 0, noProfile: 0, error: null, problems: [], lowFunds: precise ? mockLow() : null, stopped: false };
    },
    async aiTranslate(id) {
      outside();
      const o = find(id);
      if (!aiOn(settings)) throw { code: "missing", detail: "ai provider" };
      await new Promise((r) => setTimeout(r, 700));
      o.translationLang = "pl";
      o.titleTranslated = `${o.title} (tłumaczenie z podglądu)`;
      o.ai = { ...o.ai, descriptionTranslated: "Przykładowe tłumaczenie opisu z podglądu - tu pisze AI." };
      aiCalls++;
    },
    async aiCheck(provider) {
      outside();
      await new Promise((r) => setTimeout(r, 600));
      aiCalls++;
      const model = (await this.aiStatus()).providers.find((x) => x.provider === provider)?.model ?? "";
      if (provider === "ollama") return { model, problem: { provider, model, kind: "offline", message: "error sending request" } };
      if (!secrets[`ai-key-${provider}` as keyof typeof secrets]) return { model, problem: { provider, model, kind: "no_key", message: "" } };
      if (provider === "gemini" && model.startsWith("gemini-2")) {
        return { model, problem: { provider, model, kind: "model", message: "Quota exceeded for metric: generate_content_free_tier_requests, limit: 0, model: " + model } };
      }
      return { model, problem: null };
    },
    async aiFreeModels(provider) {
      await new Promise((r) => setTimeout(r, 400));
      const saved: Partial<Record<AiProvider, string[]>> = {
        groq: ["openai/gpt-oss-120b", "openai/gpt-oss-20b", "llama-3.3-70b-versatile", "llama-3.1-8b-instant", "qwen/qwen3.8-27b"],
        openrouter: ["openrouter/free", "google/gemma-4-31b-it:free", "google/gemma-4-26b-a4b-it:free", "nvidia/nemotron-3.5-super-vl:free"],
      };
      const models = saved[provider] ?? [];
      if (provider === "openrouter") return { models: [...models, "liquid/lfm-3-1.7b:free"].sort(), fromProvider: true, problem: null };
      return { models, fromProvider: false, problem: { provider, model: "", kind: "key", message: "API key not valid. Please pass a valid API key." } };
    },
    async aiAssist(id, kind) {
      outside();
      const o = find(id);
      if (!aiOn(settings)) throw { code: "missing", detail: "ai provider" };
      await new Promise((r) => setTimeout(r, 700));
      aiCalls++;
      if (kind === "message") {
        return `Dzień dobry,\n\nz zainteresowaniem przeczytałem ofertę „${o.title}” w ${o.company}. Od 4 lat pracuję z React i TypeScript, a ostatnio budowałem panel dla małego zespołu produktowego - podobny do Waszego.\n\nChętnie porozmawiam o szczegółach.\n\n(Przykład z podglądu - tu pisze AI.)`;
      }
      if (kind === "questions") {
        return "- Jak wygląda typowy tydzień w zespole?\n- Kto podejmuje decyzje o wyglądzie produktu?\n- Jak często spotykacie się na żywo?\n- Jak wygląda wdrożenie nowej osoby?\n\n(Przykład z podglądu - tu pisze AI.)";
      }
      return "- Testy end-to-end: oferta wymienia Playwright - warto zrobić mały projekt próbny.\n\n(Przykład z podglądu - tu pisze AI.)";
    },
    async aiSuggestCareers(): Promise<CareerIdea[]> {
      outside();
      if (!aiOn(settings)) throw { code: "missing", detail: "ai provider" };
      await new Promise((r) => setTimeout(r, 700));
      aiCalls++;
      return [
        { name: "Product Engineer (Frontend)", why: "Łączy React i TypeScript z pracą nad całym produktem - od pomysłu do działającej wersji. (Przykład z podglądu.)" },
        { name: "Accessibility Specialist", why: "Twoje doświadczenie z interfejsami przydaje się przy dostępności aplikacji - czytniki ekranu, klawiatura, kontrast." },
        { name: "React", why: "Już w Twoich słowach kluczowych - przycisk dodawania jest wyłączony. (Przykład z podglądu.)" },
        { name: "Technical Content Engineer", why: "Pisanie dokumentacji i przykładów kodu dla narzędzi AI - łączy programowanie z jasnym tłumaczeniem." },
      ];
    },
    async aiSuggestCriteria(description): Promise<Criteria> {
      outside();
      if (!aiOn(settings)) throw { code: "missing", detail: "ai provider" };
      await new Promise((r) => setTimeout(r, 700));
      aiCalls++;
      const base: Criteria = {
        keywords: [], workModes: [], cities: [], commutePerMonth: null, contracts: [], levels: [], languages: [],
        salaryUop: { min: null, expected: null, basis: "gross", period: "month" },
        salaryB2b: { min: null, expected: null, basis: "net", period: "month" },
        skipCompanies: [], excludeWords: [],
        hard: { location: true, contract: true, salary: true, level: true, language: true, companies: true, excludeWords: true, keywords: false },
      };
      const lower = description.toLowerCase();
      return {
        ...base,
        keywords: ["React", "TypeScript", "Frontend"].filter((k) => lower.includes(k.toLowerCase())),
        workModes: lower.includes("zdal") ? ["yes"] : [],
        cities: lower.includes("wrocław") ? [{ name: "Wrocław", radiusKm: 0 }] : [],
        contracts: lower.includes("umow") ? ["uop"] : [],
      };
    },
    async aiTidy(text) {
      outside();
      if (!aiOn(settings)) throw { code: "missing", detail: "ai provider" };
      await new Promise((r) => setTimeout(r, 700));
      aiCalls++;
      return text
        .split(/[.,;]\s+/)
        .map((s) => s.trim().replace(/\.$/, ""))
        .filter(Boolean)
        .map((s) => `- ${s[0].toUpperCase()}${s.slice(1)}`)
        .join("\n");
    },
    async ollamaModels() {
      await new Promise((r) => setTimeout(r, 300));
      return [...ollamaModels];
    },
    async ollamaPull(model) {
      outside();
      for (let done = 0; done <= 100; done += 20) {
        tellProgress({ step: "pull", source: model, done, total: 100, part: 0, parts: 0 });
        await new Promise((r) => setTimeout(r, 400));
      }
      tellProgress(null);
      ollamaModels.push(model);
    },
    async ollamaInstall() {
      outside();
      for (let done = 0; done <= 100; done += 25) {
        tellProgress({ step: "ollama-download", source: null, done, total: 100, part: 0, parts: 0 });
        await new Promise((r) => setTimeout(r, 300));
      }
      tellProgress({ step: "ollama-run", source: null, done: 0, total: 0, part: 0, parts: 0 });
      await new Promise((r) => setTimeout(r, 900));
      tellProgress(null);
    },
    async pickFolder() {
      outside();
      return "D:\\AI\\Ollama";
    },
    async checkHardware() {
      outside();
      await new Promise((r) => setTimeout(r, 600));
      return { gpus: [{ name: "Przykładowa karta graficzna", vramMb: 8192 }], ramMb: 32768, verdict: "good", model: "llama3.1:8b" };
    },
    async onRated() {
      return () => {};
    },
    async onProgress(listener) {
      progressListeners.add(listener);
      return () => progressListeners.delete(listener);
    },
    async fillOffer(text, title) {
      await new Promise((r) => setTimeout(r, 500));
      const lines = text.split("\n").map((l) => l.trim()).filter(Boolean);
      const money = lines.find((l) => /\d/.test(l) && /(zł|pln|eur)/i.test(l));
      const nums = money?.replace(/(\d)\s(\d{3})/g, "$1$2").match(/\d+/g)?.map(Number) ?? [];
      // The demo reads the text with plain code only (as the app without AI).
      const ai = aiOn(settings) && !DEMO;
      // A rough copy of `fieldcheck` - enough to see the marks in screenshots.
      const line = (re: RegExp) => lines.find((l) => re.test(l)) ?? null;
      const notes: FieldNote[] = [];
      if (ai && lines[1]) notes.push({ field: "company", problem: null, fragment: lines[1], more: null });
      if (money) notes.push({ field: "salary", problem: null, fragment: money, more: null });
      else if (line(/\d.*(brutto|netto)/i)) notes.push({ field: "salary", problem: "missing", fragment: line(/\d.*(brutto|netto)/i), more: null });
      if (/zdaln|remote/i.test(text)) notes.push({ field: "remote", problem: null, fragment: line(/zdaln|remote/i), more: null });
      if (/b2b/i.test(text) && /umow\S* o prac/i.test(text))
        notes.push({ field: "contract", problem: "more", fragment: line(/umow\S* o prac/i), more: "UoP" });
      if (/\b(junior|mid|senior)\b/i.test(text)) notes.push({ field: "level", problem: "missing", fragment: line(/\b(junior|mid|senior)\b/i), more: null });
      const ago = line(/^\d+ (dni|dzień) temu$/i);
      const publishedAt = ago ? new Date(Date.now() - Number(ago.split(" ")[0]) * 86_400_000).toISOString() : null;
      if (ago) notes.push({ field: "published", problem: null, fragment: ago, more: null });
      const until = line(/ważna do:? *\d\d\.\d\d\.\d{4}/i);
      if (until) notes.push({ field: "expires", problem: "missing", fragment: until, more: null });
      return {
        title: title || lines[0] || null,
        company: ai ? (lines[1] ?? null) : null,
        city: lines.find((l) => /wrocław|warszawa|kraków|gdańsk|poznań/i.test(l)) ?? null,
        remote: /zdaln|remote/i.test(text) ? "yes" : null,
        salaryMin: nums[0] ?? null,
        salaryMax: nums[1] ?? null,
        salaryCurrency: money ? "PLN" : null,
        salaryPeriod: money ? "month" : null,
        contract: /b2b/i.test(text) ? "B2B" : null,
        level: null,
        requirements: null,
        description: text.trim() || null,
        publishedAt,
        expiresAt: null,
        byAi: ai,
        readingId: null,
        notes,
        lowFunds: ai && settings.ai_mode === "claude" ? mockLow() : null,
      };
    },
    async openPreview(url) {
      outside();
      if (!/^https?:\/\//i.test(url)) throw { code: "invalid", detail: "the link must start with http:// or https://" };
    },
    async capturePreview() {
      outside();
      await new Promise((r) => setTimeout(r, 500));
      return {
        url: "https://example.com/oferty/przyklad-tester",
        title: "Tester manualny (k/m)",
        text: "Tester manualny (k/m)\nZmyślona Firma Testowa\nWrocław · praca zdalna\n9 000 - 11 000 zł brutto\n\nTestujemy aplikację do rezerwacji. Umowa o pracę.\n\n(Przykład z podglądu - w aplikacji tu jest tekst otwartej strony.)",
        expiredPhrase: "oferta wygasła",
      };
    },
    async fetchPage(url) {
      await this.openPreview(url);
      return this.capturePreview();
    },
    async closePreview() {},
    async backgroundStatus() {
      return { exists: settings.background_task === "on" };
    },
    async addEvent(offerId, event) {
      const o = find(offerId);
      if (Number.isNaN(new Date(event.at).getTime())) throw { code: "invalid", detail: "date has a wrong format" };
      const id = nextEventId++;
      o.agenda.events.push({ id, offerId, ...event });
      o.agenda.events.sort((a, b) => a.at.localeCompare(b.at));
      o.nextEvent = o.agenda.events[0].at;
      o.nextEventKind = o.agenda.events[0].kind;
      return id;
    },
    async deleteEvent(id) {
      for (const o of offers.values()) {
        o.agenda.events = o.agenda.events.filter((e) => e.id !== id);
        o.nextEvent = o.agenda.events[0]?.at ?? null;
        o.nextEventKind = o.agenda.events[0]?.kind ?? null;
      }
    },
    async saveContact(offerId, contact) {
      const o = find(offerId);
      if (!contact.name.trim()) throw { code: "invalid", detail: "name is required" };
      if (contact.id) {
        o.agenda.contacts = o.agenda.contacts.map((c) => (c.id === contact.id ? { ...contact } : c));
        return contact.id;
      }
      const id = nextEventId++;
      o.agenda.contacts.push({ ...contact, id });
      return id;
    },
    async deleteContact(id) {
      for (const o of offers.values()) o.agenda.contacts = o.agenda.contacts.filter((c) => c.id !== id);
    },
    async addLog(offerId, entry) {
      const id = nextEventId++;
      const o = find(offerId);
      o.agenda.log = [{ id, ...entry }, ...o.agenda.log];
      return id;
    },
    async deleteLog(id) {
      for (const o of offers.values()) o.agenda.log = o.agenda.log.filter((l) => l.id !== id);
    },
    async calendarGoogle() {
      outside();
    },
    async saveCopy(password) {
      outside();
      if (password.length < 8) throw { code: "invalid", detail: "the password must have at least 8 characters" };
      return "C:\\Users\\Przyklad\\Documents\\tablica-ofert.tablica";
    },
    async loadCopy() {
      outside();
      throw { code: "invalid", detail: "not available in the browser preview" };
    },
    async exportRows(view) {
      return [...offers.values()]
        .filter((o) => view === null || inView(row(o, null), view))
        .sort((a, b) => b.publishedAt.localeCompare(a.publishedAt))
        .map((o) => ({
          id: o.id,
          title: o.title,
          company: o.company,
          city: o.city,
          remote: o.remote,
          salaryMin: o.salaryMin,
          salaryMax: o.salaryMax,
          salaryCurrency: o.salaryCurrency,
          salaryPeriod: o.salaryPeriod,
          contract: o.contract,
          level: o.level,
          stage: o.stage,
          interest: o.interest,
          favorite: o.favorite,
          score: o.score,
          scoreSource: o.scoreSource,
          sources: o.sources,
          url: o.url,
          addedAt: o.addedAt,
          notes: o.notes || null,
          list: o.deletedAt ? ("trash" as const) : o.archivedAt ? ("archive" as const) : viewFor(row(o, null)),
          publishedAt: o.publishedAt,
        }));
    },
    async saveCsv(name, cells, separator) {
      if (!DEMO) return `C:\\Users\\Przyklad\\Documents\\tablica-${name}.csv`;
      // The website demo: the same file as in the app, downloaded by the browser.
      if (!/^[a-z0-9-]{1,40}$/.test(name) || (separator !== ";" && separator !== ",")) throw { code: "invalid", detail: "csv" };
      const file = csvFileName(name);
      const link = document.createElement("a");
      link.href = URL.createObjectURL(new Blob([csvText(cells, separator)], { type: "text/csv;charset=utf-8" }));
      link.download = file;
      document.body.append(link);
      link.click();
      link.remove();
      setTimeout(() => URL.revokeObjectURL(link.href), 10_000);
      return file;
    },
    async wipeAll() {
      outside();
      throw { code: "invalid", detail: "not available in the browser preview" };
    },
    async onReminder() {
      return () => {};
    },
    async syncStatus() {
      return { client: true, connected: syncConnected, hasKey: true, lastSync: syncConnected ? now() : null, lastError: null };
    },
    async syncLoginStart() {
      outside();
      return { deviceCode: "demo", userCode: "ABCD-EFGH", verificationUrl: "https://www.google.com/device", expiresIn: 600, interval: 5 };
    },
    async syncLoginPoll() {
      syncConnected = true;
      return true;
    },
    async syncNow() {
      outside();
      await new Promise((r) => setTimeout(r, 800));
      return { status: "done" as const, changed: 2, removed: 0 };
    },
    async syncQr() {
      outside();
      // A made-up pattern - the preview has no real key.
      const cells = Array.from({ length: 21 * 21 }, (_, i) => ((i * 7919) % 11 < 5 ? 1 : 0));
      const rects = cells.map((c, i) => (c ? `<rect x="${(i % 21) * 12 + 4}" y="${Math.floor(i / 21) * 12 + 4}" width="12" height="12"/>` : "")).join("");
      return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 260 260"><rect width="260" height="260" fill="#fff"/><g fill="#111">${rects}</g></svg>`;
    },
    async syncNewKey() {
      outside();
    },
    async syncFromQr() {
      outside();
      return null;
    },
    async upcomingEvents() {
      return [...offers.values()].flatMap((o) =>
        o.agenda.events.map((e) => ({ id: e.id, kind: e.kind, at: e.at, title: o.title, company: o.company })),
      );
    },
    async syncDeleteRemote() {
      outside();
    },
    async syncDisconnect() {
      syncConnected = false;
    },
    async calendarIcs() {
      outside();
      return "C:\\Users\\Przyklad\\Downloads\\tablica-ofert-rozmowa.ics";
    },
    async setBackground(on) {
      outside();
      settings.background_task = on ? "on" : "off";
      return { exists: on };
    },
    async updateDescription(id, text) {
      const o = find(id);
      if (text.trim().length <= (o.description ?? "").length) return false;
      o.description = text.trim();
      o.ai = { ...o.ai, status: null };
      return true;
    },
  };
}
