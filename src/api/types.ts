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
export type View = "board" | "rejected" | "trash";

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
  addedAt: string;
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
  filterReason: string | null;
  expiresAt: string | null;
  deletedAt: string | null;
  links: SourceLink[];
  history: StageEvent[];
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
  | { status: "blocked" };

export interface AppStatus {
  ok: boolean;
  problem: string | null;
  version: string;
}

export interface Settings {
  theme: "system" | "dark" | "light";
  language: "system" | "pl" | "en";
  motion: "system" | "full" | "reduced";
  sound: "off" | "on";
}

/** Error shape sent by the Rust side (`AppError`). */
export interface AppError {
  code: "db" | "io" | "secret" | "locked" | "invalid" | "not_found";
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
