// In-memory stand-in for the Rust side, used only when the UI runs in a normal browser
// (`npm run dev`) - for design work and screenshots. Data is made up and not saved.

import { DEMO_OFFERS } from "../demo/offers";
import type { Api } from "./index";
import type { OfferDetail, Settings } from "./types";

const hoursAgo = (h: number) => new Date(Date.now() - h * 3_600_000).toISOString().replace(/\.\d{3}Z$/, "Z");

export function createMockApi(): Api {
  let nextId = 1;
  const offers = new Map<number, OfferDetail>();
  const settings: Settings = { theme: "system", language: "system", motion: "system", sound: "off" };

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
      addedAt: added,
      url: input.url ?? null,
      contract: input.contract ?? null,
      level: input.level ?? null,
      language: input.language ?? null,
      requirements: input.requirements ?? null,
      description: input.description ?? null,
      notes: demo.notes ?? "",
      filterReason: null,
      expiresAt: null,
      deletedAt: null,
      links: [input.source, ...(demo.extraSources ?? [])].map((source) => ({
        source,
        url: input.url ?? null,
        firstSeen: added,
      })),
      history: stage === "new" ? [{ stage, at: added }] : [{ stage: "new", at: added }, { stage, at: added }],
    });
  }

  const find = (id: number) => {
    const offer = offers.get(id);
    if (!offer) throw { code: "not_found", detail: "not found" };
    return offer;
  };
  const now = () => new Date().toISOString().replace(/\.\d{3}Z$/, "Z");

  return {
    async appStatus() {
      return { ok: true, problem: null, version: "0.1.0-preview" };
    },
    async listOffers(view) {
      return [...offers.values()]
        .filter((o) =>
          view === "trash" ? o.deletedAt : !o.deletedAt && (view === "rejected") === (o.filterResult === "fail"),
        )
        .sort((a, b) => b.addedAt.localeCompare(a.addedAt))
        .map((o) => ({ ...o }));
    },
    async getOffer(id) {
      return structuredClone(find(id));
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
        addedAt: at,
        url: input.url ?? null,
        contract: input.contract ?? null,
        level: input.level ?? null,
        language: input.language ?? null,
        requirements: input.requirements ?? null,
        description: input.description ?? null,
        notes: "",
        filterReason: null,
        expiresAt: null,
        deletedAt: null,
        links: [{ source: input.source, url: input.url ?? null, firstSeen: at }],
        history: [{ stage: "new", at }],
      });
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
  };
}
