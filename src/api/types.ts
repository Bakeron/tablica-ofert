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
export type View = "own" | "today" | "board" | "review" | "rejected" | "declined" | "archive" | "trash" | "bin";
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

/** A job the AI thinks fits (model.rs `CareerIdea`). */
export interface CareerIdea {
  name: string;
  why: string;
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
  /** Own offers the CV is still to be sent to ("new" or "interested") - the number on "Own". */
  toSend: number;
  /** Offers in "Today" (as many as the setting says, plus the ones added by hand or starred). */
  today: number;
  board: number;
  review: number;
  rejected: number;
  /** Offers marked "no" - out of every other tab until the interest changes. */
  declined: number;
  /** Expired offers (the archive) - "Expired" in the bin. */
  archive: number;
  trash: number;
  /** Everything in the bin: "not for me", expired and deleted. */
  bin: number;
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
  /** When the offer was published (the source's date, the alert mail's date or when it was added). */
  publishedAt: string;
  /** Past its fresh days and still here - "probably expired". */
  stale: boolean;
  /** Applied and further - never greyed out or pushed down. */
  inRecruitment: boolean;
  /** Added by hand - always at the top of "Today". */
  manual: boolean;
  /** Set = in the archive. */
  archivedAt: string | null;
  /** One sentence from AI: why this score. */
  scoreWhy: string | null;
  /** What the "Very" / "No" ratings add to `score` when shown; `score` itself stays as it is. */
  learned: number;
  /** The words behind `learned`. */
  learnedWords: string[];
  /** No link to the offer anywhere - "NO LINK" on the list, below the ones with a link. */
  noLink: boolean;
  /** The person's own offer (tab "Own"): added by hand, moved there, or past the stage "new". */
  own: boolean;
  /** In the bin - and why; `null` = not there. */
  bin: BinKind | null;
  /** The ad was renewed after the CV went - the mark RENEWED and the question on the card. */
  renewAsked: boolean;
  /** Set = a child: another ad of the job of that offer (the tree of duplicates). */
  parentId: number | null;
  /** The same company and title as another offer, not joined (another city or salary) - the
   *  mark "SAME?" and the question on the card. */
  dupAsk: boolean;
  /** When the CV went (the first stage past "interested"); `null` = not in a recruitment. */
  sentAt: string | null;
  /** On a parent: the CV went through one of its children - "sent through: OLX, 5.11". */
  sentVia: SentVia | null;
  /** The other ads of this job, the first seen on top - shown under it on the list. */
  children: OfferRow[];
}

/** Through which ad of a family the CV went: the ad, its portal (the link's, else the source) and when. */
export interface SentVia {
  id: number;
  source: string;
  at: string;
}

/** Why an offer is in the bin: "not for me", expired (archived) or deleted. */
export type BinKind = "declined" | "expired" | "deleted";

/** What "Expired" did: nobody touched the offer - archived; otherwise marked "expired". */
export type ExpiredBy = "archived" | "marked";

/** How an offer sat in the bin before "Restore" - "Undo" puts it back exactly so (Rust `BinPlace`). */
export interface BinPlace {
  deletedAt: string | null;
  archivedAt: string | null;
  keptAt: string | null;
  /** Marked "not for me". */
  declined: boolean;
}

/** One offer for "Export to Excel (CSV)" - raw values (mirrors `model::ExportRow`). */
export interface ExportRow {
  id: number;
  title: string;
  company: string;
  city: string | null;
  remote: Remote;
  salaryMin: number | null;
  salaryMax: number | null;
  salaryCurrency: string | null;
  salaryPeriod: SalaryPeriod | null;
  contract: string | null;
  level: string | null;
  stage: Stage;
  interest: Interest | null;
  favorite: boolean;
  score: number | null;
  scoreSource: "ai" | "keywords" | null;
  sources: string[];
  url: string | null;
  addedAt: string;
  notes: string | null;
  /** The tab the offer is in (all profiles together). */
  list: View;
  publishedAt: string;
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
  /** The city this link gave - one job in two cities keeps both links. */
  city: string | null;
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
  /** Source whose text was removed when its job closed (its terms), e.g. "himalayas". */
  contentRemoved: string | null;
  /** When "Update the existing one" last brought the ad again (the history: "ad renewed"). */
  renewedAt: string | null;
  links: SourceLink[];
  history: StageEvent[];
  /** Hard filter result for every profile. */
  filters: ProfileFilter[];
  /** What AI read from the offer and the match score for every profile. */
  ai: OfferAi;
  /** Dates, contacts and contact history. */
  agenda: Agenda;
  /** "The same company and title as that offer - the same ad?" */
  dup: DupNote | null;
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
  /** "Rate more precisely" was asked for once - Claude rates it every time (a mark on the card). */
  precise: boolean;
}

export type AiProvider = "gemini" | "groq" | "openrouter" | "ollama" | "anthropic" | "openai";
/** Hidden for now (11.11.2026, `ai::HIDDEN` in Rust): Claude won the test - taking one off brings it back. */
export const AI_HIDDEN: AiProvider[] = ["openai", "gemini"];
/** In the order AI is asked (the same as `ai::ORDER` in Rust), without the hidden ones. */
export const AI_PROVIDERS: AiProvider[] = (["groq", "openrouter", "ollama", "anthropic", "openai", "gemini"] as AiProvider[]).filter(
  (p) => !AI_HIDDEN.includes(p),
);

export interface AiProviderStatus {
  provider: AiProvider;
  enabled: boolean;
  /** Key present (always true for Ollama). */
  ready: boolean;
  model: string;
  defaultModel: string;
  callsToday: number;
  callsMonth: number;
  /** Not asked today: "app" = the daily cap is used, "provider" = the provider's own limit. */
  usedUp: "app" | "budget" | "provider" | null;
}

export interface AiStatus {
  /** Every provider, in the order they are asked. */
  providers: AiProviderStatus[];
  dailyLimit: number;
  /** Offers waiting for AI. */
  waiting: number;
  rated: number;
  /** Offers "Rate all again" would send to AI. */
  rateAgain: number;
  /** Of them added by hand or rated more precisely once - Claude reads them when it is turned on (its cost is shown first). */
  rateAgainClaude: number;
  /** What the app spent on Claude (in $, counted from tokens and the price list). */
  claudeSpent: ClaudeSpent;
  /** Less than 10% of the top-up is left. */
  claudeLow: LowFunds | null;
  /** AI is reading offers now - "Stop rating" can end it. */
  running: boolean;
}

/** Claude's spending in $: today, this calendar month and since the top-up day (`null` = no day given). */
export interface ClaudeSpent {
  today: number;
  month: number;
  sinceTopUp: number | null;
}

/** AI is set up: at least one provider turned on. */
/** AI can be asked: paid Claude chosen, or a free provider ticked (Claude's tick counts only in the paid mode). */
export const aiOn = (s: Settings) => s.ai_mode === "claude" || s.ai_providers.split(",").some((p) => p !== "" && p !== "anthropic");

export interface RateReport {
  rated: number;
  failed: number;
  waiting: number;
  /** Offers left out: no profile they fit any more. */
  noProfile: number;
  error: { code: AppError["code"]; detail: string } | null;
  /** Every provider that could not be asked in this run and why (also when the next one took over). */
  problems: AiProblem[];
  /** Claude was asked in this run and less than 10% of the top-up is left. */
  lowFunds: LowFunds | null;
  /** "Stop rating" ended the run. */
  stopped: boolean;
}

/** What is left of the last top-up of the Claude account, in $ (mirrors `spend::LowFunds`). */
export interface LowFunds {
  left: number;
  topUp: number;
}

/** What went wrong with an AI provider (`client::Problem` in Rust). */
export type AiProblemKind =
  | "key"
  | "api_off"
  | "no_free_tier"
  | "verify"
  | "denied"
  | "model"
  | "limit"
  | "busy"
  | "cap"
  | "budget"
  | "no_key"
  | "offline"
  | "answer"
  | "other";

export interface AiProblem {
  provider: AiProvider;
  model: string;
  kind: AiProblemKind;
  /** The provider's own words (may be empty). */
  message: string;
}

/** "Check connection" next to a provider: `problem` null = it works, `model` answered. */
export interface AiCheck {
  model: string;
  problem: AiProblem | null;
}

/** Free models for the "Model" field: from the provider today, or the list saved in the app. */
export interface FreeModels {
  models: string[];
  fromProvider: boolean;
  problem: AiProblem | null;
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
  /** When the source published it; none = now, a date in the future becomes now. */
  publishedAt?: string | null;
}

/** What "Add offer" finds before saving (`same.rs`): nothing, the same link with the same
 *  fields, the same link changed, a new link of the portal with almost the same text (the first
 *  ad gone), a second ad of the portal while the first is open, the same link read before only in
 *  part ("partial" - empty fields filled in), another portal with the same company and title but
 *  another city or salary ("ask"), offers that are probably the same ("probable"). "complete" =
 *  "Fill in that one": what the form would change in the chosen offer. "joined" = another portal,
 *  joined as a child by itself, but the CV already went for this job - said before saving. */
export type SameKind = "none" | "identical" | "changed" | "similar" | "second" | "partial" | "ask" | "probable" | "complete" | "joined";

/** Fields "Add offer" compares with the offer on the board. */
export type SameField = "title" | "company" | "city" | "remote" | "salary" | "contract" | "level" | "published" | "expires" | "description";

/** The offer already on the board, as the form compares it. */
export interface SameWas {
  id: number;
  title: string;
  company: string;
  city: string | null;
  remote: Remote;
  salaryMin: number | null;
  salaryMax: number | null;
  salaryCurrency: string | null;
  salaryPeriod: SalaryPeriod | null;
  contract: string | null;
  level: string | null;
  publishedAt: string | null;
  expiresAt: string | null;
  own: boolean;
  stage: Stage;
  /** When the CV went (the first "applied" of the history); `null` before a recruitment. */
  appliedAt: string | null;
  /** The CV already went for this job through some ad of its family (the offer, its parent or a
   *  child) - the warning "you sent your CV on 5.11 through pracuj.pl". */
  sentVia: SentVia | null;
}

export interface SameOffer {
  kind: SameKind;
  was: SameWas | null;
  /** Fields of the form that differ from the offer on the board ("ask": "city" or "salary"). */
  fields: SameField[];
  /** Of `fields`, the ones the offer on the board does not have yet - filled in, nothing to choose. */
  filled: SameField[];
  /** How much of the description is the same, in %. */
  similarity: number;
  /** "Probably the same" offers the code found, the closest first. */
  candidates: DupCandidate[];
}

/** An offer that may be the same job under another title or a hidden company (`dups.rs`). */
export interface DupCandidate {
  was: SameWas;
  sources: string[];
  /** What agrees. */
  reasons: ("company" | "hidden" | "title" | "titleWords" | "city" | "salary" | "text")[];
  /** How much of the description is the same, in %. */
  similarity: number;
}

/** AI's word about one "probably the same" offer. */
export interface DupVerdict {
  id: number;
  same: "yes" | "maybe" | "no";
  why: string;
}

/** "The same company and title as that offer, but another city / other salary" on the card. */
export interface DupNote {
  was: SameWas;
  sources: string[];
  why: "city" | "salary";
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
  /** Tilted decorations (the score stamp); off = everything stands straight. */
  tilt: "off" | "on";
  /** "Several profiles": off = one profile (the first), edited in Settings → Profile; the
   *  others wait asleep and sort no offers. Travels with sync. */
  multi_profile: "off" | "on";
  /** "First steps" at the first start, until finished or skipped. */
  onboarding: "pending" | "done";
  /** "Collect offers automatically": off = only own offers ("Collected", "Fetch offers" and the
   *  sources' settings disappear; collected offers stay in the database). */
  collect: "on" | "off";
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
  source_himalayas: "off" | "on";
  source_weworkremotely: "off" | "on";
  source_remoteok: "off" | "on";
  source_jooble: "off" | "on";
  adzuna_country: AdzunaCountry;
  source_mail: "off" | "on";
  mail_host: string;
  mail_port: string;
  mail_user: string;
  /** Words for alerts on the portals and the Adzuna / Jooble searches, comma separated; empty = profile keywords. */
  search_words: string;
  /** Providers turned on, comma separated (asked in the order of `AI_PROVIDERS`); empty = AI off. */
  ai_providers: string;
  /** Model per provider; empty = the provider's default. */
  ai_model_gemini: string;
  ai_model_groq: string;
  ai_model_openrouter: string;
  ai_model_ollama: string;
  ai_model_anthropic: string;
  ai_model_openai: string;
  /** Daily cap of calls, for each provider. */
  ai_daily_limit: string;
  /** AI reads new offers by itself (off unless turned on; never with paid Claude). */
  ai_auto: "off" | "on";
  /** Free providers, or paid Claude alone - asked only on a click. */
  ai_mode: "free" | "claude";
  /** Claude's daily spending limit in dollars ("1", "2.50"); empty = none. */
  ai_day_usd: string;
  /** Minutes between the runs of AI on its own. */
  ai_every: "30" | "60" | "180" | "1440";
  /** The last top-up of the Claude account in dollars ("5", "20.50"); empty = not given. */
  ai_topup_usd: string;
  /** Its day (YYYY-MM-DD) - spending is counted from that day on; empty = not given. */
  ai_topup_date: string;
  ollama_url: string;
  /** Days after publication an offer stays fresh: then untouched ones leave for the archive. */
  fresh_days: "10" | "7" | "14" | "21" | "30";
  /** How many offers "Today" shows (the ones added by hand or starred on top). */
  today_count: "15" | "10" | "20" | "30";
  /** "Very" and "No" move the score of similar offers. */
  learn_scores: "on" | "off";
}

/** Defaults - the same as the first value of every rule in settings.rs, except `onboarding`:
 * "done" before the settings load, so "First steps" do not flash on every start. */
export const DEFAULT_SETTINGS: Settings = {
  theme: "system",
  language: "system",
  motion: "system",
  text_size: "normal",
  sound: "off",
  tilt: "off",
  multi_profile: "off",
  onboarding: "done",
  collect: "on",
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
  source_himalayas: "off",
  source_weworkremotely: "off",
  source_remoteok: "off",
  source_jooble: "off",
  adzuna_country: "pl",
  source_mail: "off",
  mail_host: "imap.gmail.com",
  mail_port: "993",
  mail_user: "",
  search_words: "",
  ai_providers: "",
  ai_model_gemini: "",
  ai_model_groq: "",
  ai_model_openrouter: "",
  ai_model_ollama: "",
  ai_model_anthropic: "",
  ai_model_openai: "",
  ai_daily_limit: "200",
  ai_auto: "off",
  ai_mode: "free",
  ai_day_usd: "1",
  ai_every: "30",
  ai_topup_usd: "",
  ai_topup_date: "",
  ollama_url: "http://localhost:11434",
  fresh_days: "10",
  today_count: "15",
  learn_scores: "on",
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

export type SourceName = "adzuna" | "remotive" | "himalayas" | "weworkremotely" | "remoteok" | "jooble" | "mail";

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

export type SkipReason = "missing_key" | "no_profiles" | "no_keywords" | "not_due" | "limit" | "used_up";

export interface SourceRun {
  source: SourceName;
  skipped: SkipReason | null;
  added: number;
  merged: number;
  blocked: number;
  invalid: number;
  /** Published longer ago than offers stay fresh - not saved. */
  old: number;
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
  /** Offers closed on Himalayas (deleted or stripped of their text) or past their expiry date - the board must reload. */
  closed: number;
}

/** What the work in the background is doing now - the line at the bottom (`Progress` in Rust). */
export interface WorkProgress {
  /** A source ("adzuna", "himalayas" ...), "wait" (a pause the source asks for), "check" (are Himalayas offers still open) or "ai". */
  step: string;
  /** The source of a "wait" or "check" step. */
  source: string | null;
  done: number;
  total: number;
  /** While offers are fetched: which turned-on source is asked now (1-based) of how many -
   *  "Source 3 of 7" on the left of the line. 0 = not part of a fetch. */
  part: number;
  parts: number;
}

/** Error shape sent by the Rust side (`AppError`). */
export interface AppError {
  code: "db" | "io" | "secret" | "locked" | "invalid" | "not_found" | "net" | "auth" | "limit" | "missing" | "busy" | "outdated" | "other_outdated";
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
  // Claude's daily spending limit from the settings (`ai::BUDGET`) - not the service's limit.
  if (e.code === "limit" && e.detail === "claude daily budget") return { key: "error.budget", detail: e.detail };
  const known = ["locked", "invalid", "not_found", "net", "auth", "limit", "missing", "busy", "outdated", "other_outdated"];
  return { key: known.includes(e.code) ? `error.${e.code}` : "error.generic", detail: e.detail };
}

/** The dollar in zloty from NBP (mirrors `nbp::UsdRate`). */
export interface UsdRate {
  pln: number;
  /** The day of the NBP table (YYYY-MM-DD). */
  day: string;
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
  /** When the offer was published and until when it is open (RFC 3339) - from its text. */
  publishedAt: string | null;
  expiresAt: string | null;
  /** AI filled the fields - the person should check them. */
  byAi: boolean;
  /** AI also scored the offer in the same call - passed to `addOffer`, so it is not read again. */
  readingId: string | null;
  /** "Check" marks and source lines of the fields (mirrors `fieldcheck::FieldNote`). */
  notes: FieldNote[];
  /** Claude read the offer and less than 10% of the top-up is left. */
  lowFunds: LowFunds | null;
}

export type CheckedField = "title" | "company" | "city" | "remote" | "salary" | "contract" | "level" | "published" | "expires";

/** A field of "Add offer" checked against the offer's text by plain code. */
export interface FieldNote {
  field: CheckedField;
  /** notInText - the value is not in the text; missing - the text has it, the field is empty;
   *  more - the text names more than the field (`more`). */
  problem: "notInText" | "missing" | "more" | null;
  /** The line of the offer the value comes from. */
  fragment: string | null;
  more: string | null;
}

/** Text taken from the page in the preview window. */
export interface Captured {
  url: string;
  title: string;
  text: string;
  /** The phrase saying the offer is over - only a proposal for the person. */
  expiredPhrase: string | null;
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
