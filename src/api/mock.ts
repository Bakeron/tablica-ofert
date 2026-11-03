// In-memory stand-in for the Rust side, used only when the UI runs in a normal browser
// (`npm run dev`) - for design work and screenshots. Data is made up and not saved.
// Filter results are precomputed in the demo data; the real filter runs only in Rust.
// `?firstrun` in the address starts without profiles, with "First steps".

import { DEMO_OFFERS, DEMO_PROFILES, type DemoFilter } from "../demo/offers";
import type { AiStatus, Criteria, OfferAi } from "./types";
import { bestResult, viewFor } from "../lib/profile";
import type { Api } from "./index";
import { DEFAULT_SETTINGS, type OfferDetail, type Profile, type SecretHints, type Settings, type SourceStatus, type View } from "./types";

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

  /** Made-up AI reading of a demo offer (profile index -> profile id). */
  function demoAi(ai: (typeof DEMO_OFFERS)[number]["ai"], score: number | null, source: "ai" | "keywords" | null): OfferAi {
    if (ai) {
      return {
        ...ai,
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
      scores:
        score === null
          ? []
          : [{ profileId: 1, profileName: DEMO_PROFILES[0].name, score, source: source ?? "keywords", why: null, quote: null }],
    };
  }
  const settings: Settings = { ...DEFAULT_SETTINGS, onboarding: firstRun ? "pending" : "done", source_remotive: "on" };
  const secrets: SecretHints = {};
  const sourceRuns: Partial<Record<string, Pick<SourceStatus, "lastTry" | "lastOk" | "lastError" | "added" | "merged" | "callsToday">>> = {
    remotive: { lastTry: hoursAgo(5), lastOk: hoursAgo(5), lastError: null, added: 4, merged: 1, callsToday: 2 },
  };

  for (const demo of DEMO_OFFERS) {
    const id = nextId++;
    const added = hoursAgo(demo.hoursAgo);
    const { input } = demo;
    const stage = demo.stage ?? "new";
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
      stage,
      stageChangedAt: added,
      interest: demo.interest ?? null,
      favorite: demo.favorite ?? false,
      score: demo.score ?? null,
      scoreSource: demo.scoreSource ?? null,
      sources: [input.source, ...(demo.extraSources ?? [])].sort(),
      expired: false,
      filterResult: "pass",
      filterReasons: [],
      addedAt: added,
      nextEvent: demo.event ? hoursAgo(-demo.event.inHours) : null,
      nextEventKind: demo.event ? demo.event.kind : null,
      url: input.url ?? null,
      contract: input.contract ?? null,
      level: input.level ?? null,
      language: input.language ?? null,
      requirements: input.requirements ?? null,
      description: input.description ?? null,
      notes: demo.notes ?? "",
      expiresAt: null,
      deletedAt: null,
      links: [input.source, ...(demo.extraSources ?? [])].map((source) => ({
        source,
        url: input.url ?? null,
        firstSeen: added,
      })),
      history: stage === "new" ? [{ stage, at: added }] : [{ stage: "new", at: added }, { stage, at: added }],
      filters: [],
      ai: demoAi(demo.ai, demo.score ?? null, demo.scoreSource ?? null),
      agenda: demo.event
        ? {
            events: [{ id: nextEventId++, offerId: id, kind: demo.event.kind, at: hoursAgo(-demo.event.inHours), minutes: 60, note: demo.event.note }],
            contacts: demo.contact ? [{ id: nextEventId++, ...demo.contact }] : [],
            log: [],
          }
        : { events: [], contacts: [], log: [] },
    });
    const byProfile = new Map<number, DemoFilter>();
    profiles.forEach((p, i) => byProfile.set(p.id, demo.filters?.[i] ?? { result: "pass" }));
    filters.set(id, byProfile);
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

  function row(o: OfferDetail, profile: number | null): OfferDetail {
    const f = filterOf(o.id, profile);
    return { ...o, filterResult: f.result, filterReasons: f.reasons ?? [] };
  }

  function inView(o: OfferDetail, view: View): boolean {
    if (view === "trash") return o.deletedAt !== null;
    if (o.deletedAt) return false;
    return viewFor(o) === view;
  }

  const rowsOf = (view: View, profile: number | null) =>
    [...offers.values()].map((o) => row(o, profile)).filter((o) => inView(o, view));

  return {
    async appStatus() {
      return { ok: true, problem: null, version: "0.1.0-preview" };
    },
    async listOffers(view, profile) {
      return rowsOf(view, profile).sort((a, b) => b.addedAt.localeCompare(a.addedAt));
    },
    async countOffers(profile) {
      return {
        board: rowsOf("board", profile).length,
        review: rowsOf("review", profile).length,
        rejected: rowsOf("rejected", profile).length,
        declined: rowsOf("declined", profile).length,
        trash: rowsOf("trash", profile).length,
        hidden: 0,
      };
    },
    async getOffer(id) {
      const o = find(id);
      return {
        ...structuredClone(row(o, null)),
        filters: profiles.map((p) => {
          const f = filterOf(id, p.id);
          return { profileId: p.id, profileName: p.name, result: f.result, reasons: f.reasons ?? [] };
        }),
      };
    },
    async addOffer(input) {
      const key = (input.company + "|" + input.title).toLowerCase();
      for (const o of offers.values()) {
        if ((o.company + "|" + o.title).toLowerCase() === key) {
          if (o.deletedAt) return { status: "blocked" };
          if (!o.sources.includes(input.source)) o.sources.push(input.source);
          return { status: "merged", id: o.id };
        }
      }
      const id = nextId++;
      const at = now();
      offers.set(id, {
        id,
        title: input.title.trim(),
        company: input.company.trim(),
        city: input.city ?? null,
        remote: input.remote ?? "unknown",
        salaryMin: input.salaryMin ?? null,
        salaryMax: input.salaryMax ?? null,
        salaryCurrency: input.salaryMin || input.salaryMax ? (input.salaryCurrency ?? "PLN") : null,
        salaryPeriod: input.salaryMin || input.salaryMax ? (input.salaryPeriod ?? "month") : null,
        stage: "new",
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
        url: input.url ?? null,
        contract: input.contract ?? null,
        level: input.level ?? null,
        language: input.language ?? null,
        requirements: input.requirements ?? null,
        description: input.description ?? null,
        notes: "",
        expiresAt: null,
        deletedAt: null,
        links: [{ source: input.source, url: input.url ?? null, firstSeen: at }],
        history: [{ stage: "new", at }],
        filters: [],
        ai: demoAi(undefined, null, null),
        agenda: { events: [], contacts: [], log: [] },
      });
      filters.set(id, new Map());
      return { status: "added", id };
    },
    async setStage(id, stage) {
      const o = find(id);
      if (o.stage !== stage) {
        o.stage = stage;
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
    async setNotes(id, notes) {
      find(id).notes = notes.trim();
    },
    async trashOffer(id) {
      find(id).deletedAt = now();
    },
    async restoreOffer(id) {
      find(id).deletedAt = null;
    },
    async purgeOffers(id) {
      let count = 0;
      for (const o of [...offers.values()]) {
        if (o.deletedAt && (id === null || o.id === id)) {
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
      const v = value.trim();
      if (!v || /\s/.test(v)) throw { code: "invalid", detail: "the key has a wrong format" };
      secrets[name] = v.length > 8 ? `••••${v.slice(-4)}` : "••••";
    },
    async deleteSecret(name) {
      delete secrets[name];
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
        one("weworkremotely", true, 4),
        one("remoteok", true, 4),
        one("jooble", Boolean(secrets["jooble-key"]), 1, 500),
        one("mail", Boolean(settings.mail_user && secrets["mail-password"]), 0),
      ];
    },
    async mailTest() {
      await new Promise((r) => setTimeout(r, 600));
      if (!settings.mail_user || !secrets["mail-password"]) throw { code: "missing", detail: "mailbox" };
      return 42;
    },
    async fetchOffers() {
      await new Promise((r) => setTimeout(r, 1200));
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
        runs.push({ source: "remotive" as const, skipped: null, added: isNew ? 1 : 0, merged: isNew ? 0 : 1, blocked: 0, invalid: 0, error: null });
      }
      if (settings.source_adzuna === "on") {
        const ready = Boolean(secrets["adzuna-app-id"] && secrets["adzuna-app-key"]);
        runs.push({ source: "adzuna" as const, skipped: ready ? null : ("missing_key" as const), added: 0, merged: 0, blocked: 0, invalid: 0, error: null });
      }
      return { runs, added, newOnBoard: added, newReview: 0, newRejected: 0 };
    },
    async onFetched() {
      return () => {};
    },
    async aiStatus(): Promise<AiStatus> {
      const provider = settings.ai_provider;
      const needsKey = provider !== "none" && provider !== "ollama";
      const defaults: Record<string, string> = {
        none: "", gemini: "gemini-2.5-flash", groq: "openai/gpt-oss-120b", openrouter: "openai/gpt-oss-20b:free",
        ollama: "llama3.1:8b", anthropic: "claude-opus-5-5", openai: "gpt-5-mini",
      };
      return {
        provider,
        model: settings.ai_model || defaults[provider],
        defaultModel: defaults[provider],
        ready: provider !== "none" && (!needsKey || Boolean(secrets[`ai-key-${provider}` as keyof typeof secrets])),
        callsToday: aiCalls,
        callsMonth: aiCalls + 41,
        dailyLimit: Number(settings.ai_daily_limit),
        waiting: [...offers.values()].filter((o) => o.ai.status === null && !o.deletedAt).length,
        rated: [...offers.values()].filter((o) => o.ai.status === "done").length,
      };
    },
    async aiRateNow() {
      if (settings.ai_provider === "none") throw { code: "missing", detail: "ai provider" };
      await new Promise((r) => setTimeout(r, 900));
      let rated = 0;
      for (const o of offers.values()) {
        if (o.ai.status === null && !o.deletedAt) {
          o.ai = { ...o.ai, status: "done", summary: "Przykładowe streszczenie z podglądu - tu pisze AI.", flags: [] };
          rated++;
          aiCalls++;
        }
      }
      return { rated, failed: 0, waiting: 0, error: null };
    },
    async aiRateOffer(id) {
      const o = find(id);
      if (settings.ai_provider === "none") throw { code: "missing", detail: "ai provider" };
      await new Promise((r) => setTimeout(r, 700));
      o.ai = { ...o.ai, status: "done", summary: o.ai.summary ?? "Przykładowe streszczenie z podglądu - tu pisze AI." };
      aiCalls++;
      return { rated: 1, failed: 0, waiting: 0, error: null };
    },
    async aiAssist(id, kind) {
      const o = find(id);
      if (settings.ai_provider === "none") throw { code: "missing", detail: "ai provider" };
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
    async aiSuggestCriteria(description): Promise<Criteria> {
      if (settings.ai_provider === "none") throw { code: "missing", detail: "ai provider" };
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
      if (settings.ai_provider === "none") throw { code: "missing", detail: "ai provider" };
      await new Promise((r) => setTimeout(r, 700));
      aiCalls++;
      return text
        .split(/[.,;]\s+/)
        .map((s) => s.trim().replace(/\.$/, ""))
        .filter(Boolean)
        .map((s) => `- ${s[0].toUpperCase()}${s.slice(1)}`)
        .join("\n");
    },
    async checkHardware() {
      await new Promise((r) => setTimeout(r, 600));
      return { gpus: [{ name: "Przykładowa karta graficzna", vramMb: 8192 }], ramMb: 32768, verdict: "good", model: "llama3.1:8b" };
    },
    async onRated() {
      return () => {};
    },
    async fillOffer(text, title) {
      await new Promise((r) => setTimeout(r, 500));
      const lines = text.split("\n").map((l) => l.trim()).filter(Boolean);
      const money = lines.find((l) => /\d/.test(l) && /(zł|pln|eur)/i.test(l));
      const nums = money?.replace(/(\d)\s(\d{3})/g, "$1$2").match(/\d+/g)?.map(Number) ?? [];
      const ai = settings.ai_provider !== "none";
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
        byAi: ai,
      };
    },
    async openPreview(url) {
      if (!/^https?:\/\//i.test(url)) throw { code: "invalid", detail: "the link must start with http:// or https://" };
    },
    async capturePreview() {
      await new Promise((r) => setTimeout(r, 500));
      return {
        url: "https://example.com/oferty/przyklad-tester",
        title: "Tester manualny (k/m)",
        text: "Tester manualny (k/m)\nZmyślona Firma Testowa\nWrocław · praca zdalna\n9 000 - 11 000 zł brutto\n\nTestujemy aplikację do rezerwacji. Umowa o pracę.\n\n(Przykład z podglądu - w aplikacji tu jest tekst otwartej strony.)",
      };
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
    async calendarGoogle() {},
    async saveCopy(password) {
      if (password.length < 8) throw { code: "invalid", detail: "the password must have at least 8 characters" };
      return "C:\\Users\\Przyklad\\Documents\\tablica-ofert.tablica";
    },
    async loadCopy() {
      throw { code: "invalid", detail: "not available in the browser preview" };
    },
    async exportCsv() {
      return "C:\\Users\\Przyklad\\Documents\\tablica-ofert.csv";
    },
    async wipeAll() {
      throw { code: "invalid", detail: "not available in the browser preview" };
    },
    async onReminder() {
      return () => {};
    },
    async syncStatus() {
      return { client: true, connected: syncConnected, hasKey: true, lastSync: syncConnected ? now() : null, lastError: null };
    },
    async syncLoginStart() {
      return { deviceCode: "demo", userCode: "ABCD-EFGH", verificationUrl: "https://www.google.com/device", expiresIn: 600, interval: 5 };
    },
    async syncLoginPoll() {
      syncConnected = true;
      return true;
    },
    async syncNow() {
      await new Promise((r) => setTimeout(r, 800));
      return { status: "done" as const, changed: 2, removed: 0 };
    },
    async syncQr() {
      // A made-up pattern - the preview has no real key.
      const cells = Array.from({ length: 21 * 21 }, (_, i) => ((i * 7919) % 11 < 5 ? 1 : 0));
      const rects = cells.map((c, i) => (c ? `<rect x="${(i % 21) * 12 + 4}" y="${Math.floor(i / 21) * 12 + 4}" width="12" height="12"/>` : "")).join("");
      return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 260 260"><rect width="260" height="260" fill="#fff"/><g fill="#111">${rects}</g></svg>`;
    },
    async syncNewKey() {},
    async syncSetKey() {},
    async upcomingEvents() {
      return [...offers.values()].flatMap((o) =>
        o.agenda.events.map((e) => ({ id: e.id, kind: e.kind, at: e.at, title: o.title, company: o.company })),
      );
    },
    async syncDeleteRemote() {},
    async syncDisconnect() {
      syncConnected = false;
    },
    async calendarIcs() {
      return "C:\\Users\\Przyklad\\Downloads\\tablica-ofert-rozmowa.ics";
    },
    async setBackground(on) {
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
