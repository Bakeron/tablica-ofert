// Made-up sample offers for screenshots and the design preview.
// Every company, person and number here is invented.

import type { Criteria, FilterResult, Interest, OfferAi, OfferInput, Reason, Stage } from "../api/types";
import { emptyCriteria } from "../lib/profile";

export interface DemoProfile {
  name: string;
  description: string;
  criteria: Criteria;
}

/** Made-up job seeker with two plans. */
export const DEMO_PROFILES: DemoProfile[] = [
  {
    name: "Plan A - zdalnie",
    description:
      "Szukam pracy jako frontend developer (React, TypeScript), w pełni zdalnie, w małym zespole produktowym. Nie chcę pracy w software house z ciągłą zmianą klientów.",
    criteria: {
      ...emptyCriteria(),
      keywords: ["React", "TypeScript"],
      workModes: ["yes"],
      commutePerMonth: 2,
      contracts: ["uop", "b2b"],
      levels: ["mid"],
      salaryUop: { min: 10000, expected: 13000, basis: "gross", period: "month" },
      salaryB2b: { min: 13000, expected: 16000, basis: "net", period: "month" },
      excludeWords: ["sprzedaż bezpośrednia"],
    },
  },
  {
    name: "Plan B - Wrocław",
    description: "Praca w biurze albo hybrydowo we Wrocławiu lub okolicy, na umowę o pracę. Może być szerzej niż frontend.",
    criteria: {
      ...emptyCriteria(),
      workModes: ["hybrid", "no"],
      cities: [{ name: "Wrocław", radiusKm: 30 }],
      contracts: ["uop"],
      salaryUop: { min: 9000, expected: 11000, basis: "gross", period: "month" },
    },
  },
];

/** Precomputed filter result for one demo profile (the preview does not run the Rust filter). */
export interface DemoFilter {
  result: FilterResult;
  reasons?: Reason[];
}

const fit: DemoFilter = { result: "pass" };

export interface DemoOffer {
  input: OfferInput;
  extraSources?: string[];
  stage?: Stage;
  interest?: Interest;
  favorite?: boolean;
  score?: number;
  scoreSource?: "ai" | "keywords";
  hoursAgo: number;
  notes?: string;
  /** One entry per DEMO_PROFILES item. */
  filters?: [DemoFilter, DemoFilter];
  /** A date in the agenda (made up), `inHours` from now. */
  event?: { kind: "interview" | "deadline" | "other"; inHours: number; note: string };
  contact?: { name: string; role: string; email: string; phone: string; linkedin: string };
  /** What AI "read" (made up); scores use the profile index instead of the id. */
  ai?: Omit<OfferAi, "scores"> & { scores: { profile: number; score: number; why: string; quote: string | null }[] };
}

export const DEMO_OFFERS: DemoOffer[] = [
  {
    hoursAgo: 1,
    score: 87,
    scoreSource: "ai",
    extraSources: ["jjit", "nfj"],
    filters: [fit, fit],
    ai: {
      status: "done",
      at: null,
      summary: "Frontend w małym zespole produktowym: panel do planowania rejsów dla przystani. W pełni zdalnie, spotkanie raz na kwartał.",
      flags: ["Umowa tylko UoP - przy B2B zapytaj o możliwość zmiany."],
      remoteQuote: "praca w pełni zdalna, spotkanie zespołu raz na kwartał we Wrocławiu",
      error: null,
      scores: [
        { profile: 0, score: 87, why: "React i TypeScript, mały zespół produktowy, w pełni zdalnie - jak w opisie.", quote: "Zespół 5 osób, praca w pełni zdalna" },
        { profile: 1, score: 58, why: "Biuro we Wrocławiu, ale praca zdalna - Plan B szuka biura albo hybrydy.", quote: null },
      ],
    },
    input: {
      title: "Frontend Developer (React)",
      company: "Latarnia Labs",
      source: "pracuj",
      url: "https://example.com/oferty/latarnia-frontend",
      city: "Wrocław",
      remote: "yes",
      salaryMin: 12000,
      salaryMax: 16000,
      contract: "UoP",
      level: "mid",
      language: "pl",
      requirements: "React, TypeScript, testy jednostkowe, dobra znajomość CSS.",
      description:
        "Budujemy panel do planowania rejsów dla małych przystani. Zespół 5 osób, praca w pełni zdalna, spotkanie zespołu raz na kwartał we Wrocławiu.",
    },
  },
  {
    hoursAgo: 2,
    score: 74,
    scoreSource: "ai",
    stage: "interested",
    interest: "high",
    filters: [
      { result: "pass", reasons: [{ kind: "note", code: "commute_ok", value: "2" }] },
      { result: "fail", reasons: [{ kind: "fail", code: "hybrid_outside", value: "Poznań" }] },
    ],
    input: {
      title: "Android Developer",
      company: "Bursztyn Soft",
      source: "nfj",
      url: "https://example.com/oferty/bursztyn-php",
      city: "Poznań",
      remote: "hybrid",
      salaryMin: 10000,
      salaryMax: 13000,
      contract: "UoP",
      level: "mid",
      language: "pl",
      requirements: "Kotlin, Jetpack Compose, REST API, Git.",
      description: "Rozwój systemu rezerwacji dla sieci hosteli. Biuro w Poznaniu dwa razy w miesiącu.",
    },
  },
  {
    hoursAgo: 26,
    score: 61,
    scoreSource: "keywords",
    stage: "applied",
    interest: "maybe",
    notes: "Wysłane przez formularz na stronie firmy.",
    filters: [
      {
        result: "fail",
        reasons: [
          { kind: "unsure", code: "commute_unknown" },
          { kind: "fail", code: "salary_low" },
        ],
      },
      {
        result: "fail",
        reasons: [
          { kind: "fail", code: "hybrid_outside", value: "Łódź" },
          { kind: "fail", code: "contract_mismatch", value: "B2B" },
        ],
      },
    ],
    input: {
      title: "Data Analyst",
      company: "Mewa Systems",
      source: "adzuna",
      url: "https://example.com/oferty/mewa-web",
      city: "Łódź",
      remote: "hybrid",
      salaryMin: 9000,
      salaryMax: 12000,
      contract: "B2B",
      level: "mid",
      language: "pl",
      description: "Raporty sprzedaży i prognozy dla sieci piekarni. Praca hybrydowa.",
    },
  },
  {
    hoursAgo: 30,
    score: 92,
    scoreSource: "ai",
    stage: "hr_interview",
    event: { kind: "interview", inHours: 3, note: "Rozmowa wideo - link w mailu od rekruterki." },
    contact: { name: "Anna Przykładowa", role: "Rekruterka", email: "anna@example.com", phone: "", linkedin: "" },
    interest: "high",
    favorite: true,
    extraSources: ["remotive"],
    filters: [fit, { result: "unsure", reasons: [{ kind: "unsure", code: "remote_not_wanted" }] }],
    input: {
      title: "Backend Developer (Node.js)",
      company: "Pieczątka.io",
      source: "jjit",
      url: "https://example.com/oferty/pieczatka-ai",
      city: null,
      remote: "yes",
      salaryMin: 14000,
      salaryMax: 18000,
      contract: "UoP",
      level: "mid",
      language: "pl",
      requirements: "Node.js, TypeScript, PostgreSQL, pisanie testów.",
      description: "Tworzymy system obiegu dokumentów dla biur projektowych. 100% zdalnie.",
    },
  },
  {
    hoursAgo: 50,
    score: 55,
    scoreSource: "ai",
    filters: [
      {
        result: "fail",
        reasons: [
          { kind: "unsure", code: "remote_unknown" },
          { kind: "fail", code: "level_mismatch", value: "junior" },
          { kind: "note", code: "no_salary" },
        ],
      },
      {
        result: "fail",
        reasons: [
          { kind: "fail", code: "office_outside", value: "Warszawa" },
          { kind: "note", code: "no_salary" },
        ],
      },
    ],
    input: {
      title: "QA Engineer",
      company: "Zielony Kwadrat",
      source: "pracuj",
      url: "https://example.com/oferty/kwadrat-qa",
      city: "Warszawa",
      remote: "unknown",
      contract: "UoP",
      level: "junior",
      language: "pl",
      description: "Testy ręczne i automatyczne aplikacji mobilnej do zamawiania jedzenia.",
    },
  },
  {
    hoursAgo: 75,
    score: 68,
    scoreSource: "ai",
    stage: "tech_stage",
    interest: "maybe",
    filters: [
      { result: "unsure", reasons: [{ kind: "unsure", code: "salary_currency", value: "EUR" }] },
      {
        result: "fail",
        reasons: [
          { kind: "unsure", code: "remote_not_wanted" },
          { kind: "fail", code: "contract_mismatch", value: "B2B" },
        ],
      },
    ],
    input: {
      title: "Full Stack Developer",
      company: "Szkło i Stal",
      source: "remotive",
      url: "https://example.com/oferty/szklo-fullstack",
      city: null,
      remote: "yes",
      salaryMin: 45,
      salaryMax: 60,
      salaryCurrency: "EUR",
      salaryPeriod: "hour",
      contract: "B2B",
      level: "mid",
      language: "en",
      description: "Remote-first team building tools for architecture studios.",
    },
  },
  {
    hoursAgo: 120,
    score: 40,
    scoreSource: "keywords",
    stage: "rejected",
    interest: "no",
    filters: [
      {
        result: "fail",
        reasons: [
          { kind: "fail", code: "office_not_wanted" },
          { kind: "fail", code: "level_mismatch", value: "junior" },
        ],
      },
      { result: "fail", reasons: [{ kind: "fail", code: "office_outside", value: "Kraków" }] },
    ],
    input: {
      title: "Junior Developer",
      company: "Kawiarnia Kodu",
      source: "nfj",
      url: "https://example.com/oferty/kawiarnia-junior",
      city: "Kraków",
      remote: "no",
      salaryMin: 6000,
      salaryMax: 8000,
      contract: "UoP",
      level: "junior",
      language: "pl",
      description: "Praca wyłącznie stacjonarna w biurze w Krakowie.",
    },
  },
  {
    hoursAgo: 0.3,
    score: 79,
    scoreSource: "keywords",
    filters: [
      { result: "pass", reasons: [{ kind: "note", code: "work_mode_in_text", value: "praca zdalna" }] },
      { result: "unsure", reasons: [{ kind: "unsure", code: "remote_not_wanted" }] },
    ],
    input: {
      title: "React Developer",
      company: "Przystań Tech",
      source: "manual",
      url: "https://example.com/oferty/przystan-react",
      city: "Katowice",
      remote: "unknown",
      salaryMin: 11000,
      salaryMax: 14000,
      contract: "UoP",
      level: "mid",
      language: "pl",
      requirements: "React, TypeScript, React Query.",
      description: "Aplikacja do rezerwacji kortów i hal sportowych. Biuro w Katowicach, możliwa praca zdalna.",
    },
  },
  {
    hoursAgo: 3,
    filters: [
      {
        result: "unsure",
        reasons: [
          { kind: "unsure", code: "remote_unknown" },
          { kind: "unsure", code: "level_unknown" },
          { kind: "unsure", code: "contract_unknown" },
          { kind: "note", code: "no_salary" },
        ],
      },
      { result: "fail", reasons: [{ kind: "fail", code: "office_outside", value: "Gdańsk" }] },
    ],
    input: {
      title: "Specjalista wsparcia IT",
      company: "Wydma Serwis",
      source: "pracuj",
      url: "https://example.com/oferty/wydma-wsparcie",
      city: "Gdańsk",
      remote: "unknown",
    },
  },
  {
    hoursAgo: 5,
    filters: [
      {
        result: "unsure",
        reasons: [
          { kind: "unsure", code: "remote_unknown" },
          { kind: "unsure", code: "contract_unknown" },
          { kind: "note", code: "no_salary" },
        ],
      },
      {
        result: "unsure",
        reasons: [
          { kind: "unsure", code: "contract_unknown" },
          { kind: "note", code: "no_salary" },
        ],
      },
    ],
    input: {
      title: "Frontend Developer (Vue)",
      company: "Lampion Studio",
      source: "jjit",
      url: "https://example.com/oferty/lampion-vue",
      city: "Wrocław",
      remote: "unknown",
      level: "mid",
      description: "Sklep internetowy z ręcznie robioną ceramiką. Vue 3, Nuxt, Tailwind.",
    },
  },
  {
    hoursAgo: 8,
    filters: [
      {
        result: "fail",
        reasons: [
          { kind: "fail", code: "commute_too_often", value: "12" },
          { kind: "fail", code: "level_mismatch", value: "senior" },
        ],
      },
      {
        result: "fail",
        reasons: [
          { kind: "fail", code: "hybrid_outside", value: "Warszawa" },
          { kind: "fail", code: "contract_mismatch", value: "B2B" },
        ],
      },
    ],
    input: {
      title: "Senior Frontend Engineer",
      company: "Kometa Group",
      source: "nfj",
      url: "https://example.com/oferty/kometa-senior",
      city: "Warszawa",
      remote: "hybrid",
      salaryMin: 20000,
      salaryMax: 26000,
      contract: "B2B",
      level: "senior",
      language: "pl",
      description: "Platforma rozliczeń dla sieci kin. Praca hybrydowa: 3 dni w tygodniu w biurze.",
    },
  },
  {
    hoursAgo: 28,
    score: 66,
    scoreSource: "keywords",
    filters: [
      { result: "pass", reasons: [{ kind: "note", code: "commute_ok", value: "1" }] },
      { result: "fail", reasons: [{ kind: "fail", code: "hybrid_outside", value: "Poznań" }] },
    ],
    input: {
      title: "Projektant UI (TypeScript)",
      company: "Kreska i Punkt",
      source: "adzuna",
      url: "https://example.com/oferty/kreska-ui",
      city: "Poznań",
      remote: "hybrid",
      salaryMin: 9000,
      salaryMax: 11000,
      contract: "UoP",
      level: "mid",
      language: "pl",
      description: "Projektujemy i składamy interfejsy aplikacji dla bibliotek miejskich. Raz w miesiącu w biurze.",
    },
  },
];
