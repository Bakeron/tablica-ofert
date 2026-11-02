/** One piece of a word-by-word comparison. */
export interface DiffPart {
  text: string;
  kind: "same" | "removed" | "added";
}

const tokens = (text: string) => text.split(/(\s+)/).filter((t) => t !== "");

/** Above this many words on both sides the comparison would be slow - it falls back to "all changed". */
const MAX_CELLS = 4_000_000;

/**
 * Word-by-word difference between the user's text and the AI proposal (longest common subsequence).
 * Used by "Tidy up" to show what AI removed or changed, so nothing important gets lost.
 */
export function wordDiff(before: string, after: string): DiffPart[] {
  const a = tokens(before);
  const b = tokens(after);
  if (a.length * b.length > MAX_CELLS) {
    return [
      { text: before, kind: "removed" },
      { text: after, kind: "added" },
    ];
  }
  // Words compare without case and punctuation: "React," and "react" are the same word.
  const key = (w: string) => w.toLowerCase().replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "");
  const ka = a.map(key);
  const kb = b.map(key);
  const lcs: number[][] = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0));
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      lcs[i][j] = ka[i] === kb[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }
  const out: DiffPart[] = [];
  const push = (text: string, kind: DiffPart["kind"]) => {
    const last = out[out.length - 1];
    if (last && last.kind === kind) last.text += text;
    else out.push({ text, kind });
  };
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (ka[i] === kb[j]) {
      push(b[j], "same");
      i++;
      j++;
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
      push(a[i++], "removed");
    } else {
      push(b[j++], "added");
    }
  }
  while (i < a.length) push(a[i++], "removed");
  while (j < b.length) push(b[j++], "added");
  // Whitespace alone is not a change worth marking.
  return out.map((p) => (p.kind !== "same" && p.text.trim() === "" ? { ...p, kind: "same" } : p));
}

/** Words of `before` that are missing from `after` - shown as a warning list. */
export function droppedWords(before: string, after: string): string[] {
  const words = (t: string) =>
    t
      .toLowerCase()
      .split(/[^\p{L}\p{N}+#.]+/u)
      .map((w) => w.replace(/^\.+|\.+$/g, ""))
      .filter((w) => w.length >= 3);
  const kept = new Set(words(after));
  return [...new Set(words(before))].filter((w) => !kept.has(w));
}
