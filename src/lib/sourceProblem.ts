/**
 * What went wrong with an offer source, read from the error saved in the database (`source_runs`
 * keeps only the text `AppError` prints: "network: 503 Service Unavailable: …", "refused: …").
 * The kind picks a plain sentence with what happened and what to do (`src.problem.<kind>`).
 */
export type SourceProblem = "down" | "offline" | "auth" | "limit" | "missing" | "answer" | "closed_at_once" | "other";

export function sourceProblem(detail: string): SourceProblem {
  const d = detail.trim().toLowerCase();
  if (d.includes("closed at once")) return "closed_at_once";
  if (d.startsWith("refused:")) return "auth";
  if (d.startsWith("limit:")) return "limit";
  if (d.startsWith("missing:")) return "missing";
  const status = /^network:\s*(\d{3})\b/.exec(d)?.[1];
  if (status) {
    const n = Number(status);
    if (n >= 500) return "down";
    if (n === 401 || n === 403) return "auth";
    if (n === 429) return "limit";
    return "other";
  }
  if (d.startsWith("network:")) return /json|decod|too big|empty answer/.test(d) ? "answer" : "offline";
  return "other";
}
