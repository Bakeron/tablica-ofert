// Mirrors src-tauri/src/model.rs - keep both in sync.

export const STAGES = [
  "new",
  "interested",
  "applied",
  "replied",
  "hr_interview",
  "tech_stage",
  "job_offer",
  "rejected",
  "withdrawn",
] as const;
export type Stage = (typeof STAGES)[number];

export type Interest = "high" | "maybe" | "no";
export type Remote = "yes" | "hybrid" | "no" | "unknown";
export type SalaryPeriod = "month" | "hour";
export type FilterResult = "pass" | "fail" | "unsure";
export type View = "board" | "review" | "rejected" | "declined" | "trash";
export type Contract = "uop" | "b2b" | "mandate";
export type Level = "junior" | "mid" | "senior";
export type OfferLanguage = "pl" | "en";
export type PayBasis = "gross" | "net";
export type ReasonKind = "fail" | "unsure" | "note";

/** Why the hard filter gave its result; the UI translates `code` (`filter.reason.<code>`). */
export interface Reason {
  kind: ReasonKind;
  code: string;
  value?: string;
}

export interface CityRule {
  name: string;
  /** 0 = only this city. */
  radiusKm: number;
}

export interface SalaryRule {
  /** Hard filter - offers that cannot reach it are rejected. */
  min: number | null;
  /** Only for the match score. */
  expected: number | null;
  basis: PayBasis;
  period: SalaryPeriod;
}

/** Which criteria are hard filters; the rest only feeds the match score. */
export interface HardFlags {
  location: boolean;
  contract: boolean;
  salary: boolean;
  level: boolean;
  language: boolean;
  companies: boolean;
  excludeWords: boolean;
  keywords: boolean;
}

export interface Criteria {
  keywords: string[];
  /** Accepted forms of work: "yes" (remote), "hybrid", "no" (office). */
  workModes: Remote[];
  cities: CityRule[];
  /** Office trips outside my cities: null = no, n = up to n a month. */
  commutePerMonth: number | null;
  contracts: Contract[];
  levels: Level[];
  languages: OfferLanguage[];
  salaryUop: SalaryRule;
  salaryB2b: SalaryRule;
  skipCompanies: string[];
  excludeWords: string[];
  hard: HardFlags;
}

export interface Profile {
  id: number;
  name: string;
  description: string;
  criteria: Criteria;
  position: number;
  /** The description before the last change - "Tidy up" can be undone. */
  descriptionPrev: string | null;
}

export interface ProfileInput {
  /** null = new profile. */
  id: number | null;
  name: string;
  description: string;
  criteria: Criteria;
}

export interface ProfileFilter {
  profileId: number;
  profileName: string;
  result: FilterResult;
  reasons: Reason[];
}

export interface ViewCounts {
  board: number;
  review: number;
  rejected: number;
  /** Offers marked "no" - out of every other tab until the interest changes. */
  declined: number;
  trash: number;
  /** Offers on the board hidden by "hide offers scored below X". */
  hidden: number;
}

export interface OfferRow {
  id: number;
  title: string;
  company: string;
  city: string | null;
  remote: Remote;
  salaryMin: number | null;
  salaryMax: number | null;
  salaryCurrency: string | null;
  salaryPeriod: SalaryPeriod | null;
  stage: Stage;
  stageChangedAt: string;
  interest: Interest | null;
  favorite: boolean;
  score: number | null;
  scoreSource: "ai" | "keywords" | null;
  sources: string[];
  expired: boolean;
  filterResult: FilterResult;
  /** Reasons behind `filterResult` (for the chosen profile, or the best one). */
  filterReasons: Reason[];
  addedAt: string;
  /** The nearest date (from 3 hours ago on) - for "INTERVIEW TODAY". */
  nextEvent: string | null;
  nextEventKind: EventKind | null;
  /** Language of the AI translation - used only when it is the app language. */
  translationLang: OfferLanguage | null;
  titleTranslated: string | null;
}

export type EventKind = "interview" | "deadline" | "other";
export type LogKind = "call" | "mail" | "meeting" | "message";

export interface CalEvent {
  id: number;
  offerId: number;
  kind: EventKind;
  /** UTC, RFC 3339 */
  at: string;
  minutes: number;
  note: string;
}

export interface Contact {
  id: number;
  name: string;
  role: string;
  email: string;
  phone: string;
  linkedin: string;
}

export interface LogEntry {
  id: number;
  at: string;
  kind: LogKind;
  note: string;
}

export interface Agenda {
  events: CalEvent[];
  contacts: Contact[];
  log: LogEntry[];
}

export interface SourceLink {
  source: string;
  url: string | null;
  firstSeen: string;
}

export interface StageEvent {
  stage: Stage;
  at: string;
}

export interface OfferDetail extends OfferRow {
  url: string | null;
  contract: string | null;
  level: string | null;
  language: string | null;
  requirements: string | null;
  description: string | null;
  notes: string;
  expiresAt: string | null;
  deletedAt: string | null;
  links: SourceLink[];
  history: StageEvent[];
  /** Hard filter result for every profile. */
  filters: ProfileFilter[];
  /** What AI read from the offer and the match score for every profile. */
  ai: OfferAi;
  /** Dates, contacts and contact history. */
  agenda: Agenda;
}

export interface ScoreView {
  profileId: number;
  profileName: string;
  score: number;
  source: "ai" | "keywords";
  why: string | null;
  /** A sentence copied from the offer (checked: it really is there). */
  quote: string | null;
}

export interface OfferAi {
  /** null = waiting for AI */
  status: "done" | "failed" | null;
  at: string | null;
  summary: string | null;
  /** Short warnings - "Uwagi". */
  flags: string[];
  remoteQuote: string | null;
  error: string | null;
  scores: ScoreView[];
  /** The description translated by AI, in `translationLang` of the row. */
  descriptionTranslated: string | null;
  /** Language of the offer text when known ("AI would translate it"). */
  offerLanguage: OfferLanguage | null;
}

export type AiProvider = "none" | "gemini" | "groq" | "openrouter" | "ollama" | "anthropic" | "openai";
export const AI_PROVIDERS: AiProvider[] = ["none", "gemini", "groq", "openrouter", "ollama", "anthropic", "openai"];

export interface AiStatus {
  provider: AiProvider;
  model: string;
  defaultModel: string;
  /** Key present (always true for Ollama). */
  ready: boolean;
  callsToday: number;
  callsMonth: number;
  dailyLimit: number;
  /** Offers waiting for AI. */
  waiting: number;
  rated: number;
}

export interface RateReport {
  rated: number;
  failed: number;
  waiting: number;
  error: { code: AppError["code"]; detail: string } | null;
}

export type AssistKind = "message" | "questions" | "gaps";

export interface Machine {
  gpus: { name: string; vramMb: number }[];
  ramMb: number;
  verdict: "good" | "ok" | "slow" | "no";
  model: string | null;
}

export interface OfferInput {
  title: string;
  company: string;
  source: string;
  url?: string | null;
  city?: string | null;
  remote?: Remote | null;
  salaryMin?: number | null;
  salaryMax?: number | null;
  salaryCurrency?: string | null;
  salaryPeriod?: SalaryPeriod | null;
  contract?: string | null;
  level?: string | null;
  language?: string | null;
  requirements?: string | null;
  description?: string | null;
  expiresAt?: string | null;
}

export type AddOutcome =
  | { status: "added"; id: number }
  | { status: "merged"; id: number }
  | { status: "unchanged"; id: number }
  | { status: "blocked" };

export interface AppStatus {
  ok: boolean;
  problem: string | null;
  version: string;
}

/** From the smallest to the largest - Ctrl + / Ctrl - step through this list. */
export const TEXT_SIZES = ["small", "normal", "large", "xlarge"] as const;
export type TextSize = (typeof TEXT_SIZES)[number];

export interface Settings {
  theme: "system" | "dark" | "light";
  language: "system" | "pl" | "en";
  motion: "system" | "full" | "reduced";
  /** Size of all text; on the PC also Ctrl + / Ctrl - / Ctrl 0. */
  text_size: TextSize;
  sound: "off" | "on";
  /** "First steps" at the first start, until finished or skipped. */
  onboarding: "pending" | "done";
  /** Collect offers at start and every few hours while the app is open. */
  fetch_auto: "on" | "off";
  fetch_every: "3" | "6" | "12";
  /** Task Scheduler runs the app hourly without a window. */
  background_task: "on" | "off";
  /** Windows notification about new offers. */
  notify_new: "on" | "off";
  /** Reminders: interviews and deadlines, follow-up, high scores. */
  notify_reminders: "on" | "off";
  /** Extra parts of the screen - a turned-off part disappears, its data stays. */
  feature_dates: "on" | "off";
  feature_contacts: "on" | "off";
  feature_keys: "on" | "off";
  /** Sync with Google Drive (off by default). */
  sync_drive: "off" | "on";
  /** Own Google project (empty = the one built into the app). */
  google_client_id: string;
  source_adzuna: "off" | "on";
  source_remotive: "off" | "on";
  source_weworkremotely: "off" | "on";
  source_remoteok: "off" | "on";
  source_jooble: "off" | "on";
  adzuna_country: AdzunaCountry;
  source_mail: "off" | "on";
  mail_host: string;
  mail_port: string;
  mail_user: string;
  ai_provider: AiProvider;
  /** Empty = the provider's default model. */
  ai_model: string;
  ai_daily_limit: string;
  /** AI reads new offers by itself. */
  ai_auto: "on" | "off";
  ollama_url: string;
  hide_below: "off" | "30" | "40" | "50" | "60" | "70";
}

/** Defaults - the same as the first value of every rule in settings.rs. */
export const DEFAULT_SETTINGS: Settings = {
  theme: "system",
  language: "system",
  motion: "system",
  text_size: "normal",
  sound: "off",
  onboarding: "done",
  fetch_auto: "on",
  fetch_every: "6",
  background_task: "on",
  notify_new: "on",
  notify_reminders: "on",
  feature_dates: "on",
  feature_contacts: "off",
  feature_keys: "on",
  sync_drive: "off",
  google_client_id: "",
  source_adzuna: "off",
  source_remotive: "off",
  source_weworkremotely: "off",
  source_remoteok: "off",
  source_jooble: "off",
  adzuna_country: "pl",
  source_mail: "off",
  mail_host: "imap.gmail.com",
  mail_port: "993",
  mail_user: "",
  ai_provider: "none",
  ai_model: "",
  ai_daily_limit: "200",
  ai_auto: "on",
  ollama_url: "http://localhost:11434",
  hide_below: "off",
};

/** Countries covered by Adzuna (mirrors `ADZUNA_COUNTRIES` in settings.rs). */
export const ADZUNA_COUNTRIES = [
  "pl", "at", "au", "be", "br", "ca", "ch", "de", "es", "fr", "gb", "in", "it", "mx", "nl", "nz", "sg", "us", "za",
] as const;
export type AdzunaCountry = (typeof ADZUNA_COUNTRIES)[number];

/** Secrets the user can type in the settings (mirrors `USER_SECRETS` in secrets.rs). */
export type SecretName =
  | "adzuna-app-id"
  | "adzuna-app-key"
  | "jooble-key"
  | "mail-password"
  | "ai-key-gemini"
  | "ai-key-groq"
  | "ai-key-openrouter"
  | "ai-key-anthropic"
  | "ai-key-openai"
  | "google-client-secret";

/** Set secrets: name -> last 4 characters ("••••1234"). */
export type SecretHints = Partial<Record<SecretName, string>>;

export type SourceName = "adzuna" | "remotive" | "weworkremotely" | "remoteok" | "jooble" | "mail";

export interface SourceStatus {
  source: SourceName;
  enabled: boolean;
  /** Key / login present. */
  ready: boolean;
  lastTry: string | null;
  lastOk: string | null;
  lastError: string | null;
  added: number;
  merged: number;
  callsToday: number;
  callsADay: number;
  /** Calls left on the key for its whole life (Jooble), null = no such limit. */
  callsLeft: number | null;
}

export type SkipReason = "off" | "missing_key" | "no_profiles" | "no_keywords" | "not_due" | "limit" | "used_up";

export interface SourceRun {
  source: SourceName;
  skipped: SkipReason | null;
  added: number;
  merged: number;
  blocked: number;
  invalid: number;
  error: { code: AppError["code"]; detail: string } | null;
}

export interface FetchReport {
  runs: SourceRun[];
  added: number;
  /** New offers that passed the filter of at least one profile. */
  newOnBoard: number;
  /** New offers in "To check" - the filter could not decide. */
  newReview: number;
  /** New offers in "Rejected by filter". */
  newRejected: number;
}

/** Error shape sent by the Rust side (`AppError`). */
export interface AppError {
  code: "db" | "io" | "secret" | "locked" | "invalid" | "not_found" | "net" | "auth" | "limit" | "missing" | "busy";
  detail: string;
}

export function isAppError(value: unknown): value is AppError {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as AppError).code === "string" &&
    typeof (value as AppError).detail === "string"
  );
}

/** Any thrown value as a text key (`error.*`) for the person plus the technical detail. */
export function describeError(e: unknown): { key: string; detail: string } {
  if (!isAppError(e)) return { key: "error.generic", detail: String(e) };
  const known = ["locked", "invalid", "not_found", "net", "auth", "limit", "missing", "busy"];
  return { key: known.includes(e.code) ? `error.${e.code}` : "error.generic", detail: e.detail };
}

/** Fields read from a pasted text or a preview page (mirrors `extract::Draft`). */
export interface Draft {
  title: string | null;
  company: string | null;
  city: string | null;
  remote: Remote | null;
  salaryMin: number | null;
  salaryMax: number | null;
  salaryCurrency: string | null;
  salaryPeriod: SalaryPeriod | null;
  contract: string | null;
  level: string | null;
  requirements: string | null;
  description: string | null;
  /** AI filled the fields - the person should check them. */
  byAi: boolean;
}

/** Text taken from the page in the preview window. */
export interface Captured {
  url: string;
  title: string;
  text: string;
}

/** The Task Scheduler task as Windows reports it. */
export interface TaskStatus {
  exists: boolean;
}

export interface SyncStatus {
  /** A Google project is available (built in or typed in). */
  client: boolean;
  connected: boolean;
  hasKey: boolean;
  lastSync: string | null;
  lastError: string | null;
}

/** Login with a code at google.com/device. */
export interface DeviceCode {
  deviceCode: string;
  userCode: string;
  verificationUrl: string;
  expiresIn: number;
  interval: number;
}

export type SyncOutcome =
  | { status: "done"; changed: number; removed: number }
  /** Several offers would be deleted - ask first. */
  | { status: "confirm"; removed: number };

/** A future date with its offer - for reminders planned on the phone. */
export interface Upcoming {
  id: number;
  kind: EventKind;
  at: string;
  title: string;
  company: string;
}
