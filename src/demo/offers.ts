// Made-up sample offers for screenshots and the design preview.
// Every company, person and number here is invented.

import type { Interest, OfferInput, Stage } from "../api/types";

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
}

export const DEMO_OFFERS: DemoOffer[] = [
  {
    hoursAgo: 1,
    score: 87,
    scoreSource: "ai",
    extraSources: ["jjit", "nfj"],
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
    interest: "high",
    favorite: true,
    extraSources: ["remotive"],
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
];
