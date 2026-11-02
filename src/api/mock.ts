// In-memory stand-in for the Rust side, used only when the UI runs in a normal browser
// (`npm run dev`) - for design work and screenshots. Data is made up and not saved.
// Filter results are precomputed in the demo data; the real filter runs only in Rust.
// `?firstrun` in the address starts without profiles, with "First steps".

import { DEMO_OFFERS, DEMO_PROFILES, type DemoFilter } from "../demo/offers";
import { bestResult, isKept } from "../lib/profile";
import type { Api } from "./index";
import type { OfferDetail, Profile, Settings, View } from "./types";

const hoursAgo = (h: number) => new Date(Date.now() - h * 3_600_000).toISOString().replace(/\.\d{3}Z$/, "Z");

export function createMockApi(): Api {
  const firstRun = typeof location !== "undefined" && new URLSearchParams(location.search).has("firstrun");
  let nextId = 1;
  const offers = new Map<number, OfferDetail>();
  /** offer id → profile id → filter result */
  const filters = new Map<number, Map<number, DemoFilter>>();
  let profiles: Profile[] = firstRun
    ? []
    : DEMO_PROFILES.map((p, i) => ({ id: i + 1, position: i, ...structuredClone(p) }));
  let nextProfileId = profiles.length + 1;
  let experience = firstRun ? "" : "4 lata jako frontend developer (React, TypeScript), angielski B2.";
  const settings: Settings = {
    theme: "system",
    language: "system",
    motion: "system",
    sound: "off",
    onboarding: firstRun ? "pending" : "done",
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
    if (view === "board") return o.filterResult === "pass" || isKept(o);
    return !isKept(o) && o.filterResult === (view === "review" ? "unsure" : "fail");
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
        trash: rowsOf("trash", profile).length,
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
        Object.assign(p, structuredClone({ name: input.name.trim(), description: input.description.trim(), criteria: input.criteria }));
        return p.id;
      }
      const id = nextProfileId++;
      profiles.push({
        id,
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
    async setExperience(text) {
      experience = text.trim();
    },
  };
}
