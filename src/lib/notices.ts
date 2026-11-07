/** Licence texts come hard-wrapped at ~75 characters, so in a wide window they fill only part of
 *  the box. This joins the wrapped lines of a paragraph back into one line, so the text flows to
 *  the full width; headings, list items, separators and short lines stay where they were. */

const SEPARATOR = /^\s*[=\-_*]{5,}\s*$/;
/** A line shorter than this is the natural end of a line (a heading, the last line of a paragraph). */
const WRAPPED_AT_LEAST = 45;

/** A line that starts something new: a separator, a heading in capitals ("TERMINATION"),
 *  a list item ("1)", "2.", "(a)", "-", "*"), a copyright line or a link on its own line. */
function startsNew(line: string): boolean {
  if (SEPARATOR.test(line)) return true;
  const trimmed = line.trim();
  // A short line all in capitals is a heading; a long one is a paragraph written in capitals
  // (the "AS IS" disclaimer) and joins like any other.
  if (trimmed.length < WRAPPED_AT_LEAST && /^[A-Z0-9 ,'"()&/-]+$/.test(trimmed) && /[A-Z]{3}/.test(trimmed)) return true;
  return /^\s*(?:[-*•]\s|\(?[0-9a-zA-Z]{1,3}[.)]\s|copyright\s+(?:\(c\)|©|\d)|©|https?:\/\/)/i.test(line);
}

export function unwrapNotices(text: string): string {
  const out: string[] = [];
  for (const line of text.replace(/\r\n?/g, "\n").split("\n")) {
    const prev = out[out.length - 1];
    const joins =
      prev !== undefined &&
      line.trim() !== "" &&
      prev.trim().length >= WRAPPED_AT_LEAST &&
      !prev.trimEnd().endsWith(":") &&
      !SEPARATOR.test(prev) &&
      !startsNew(line);
    if (joins) out[out.length - 1] = `${prev.trimEnd()} ${line.trim()}`;
    else out.push(line);
  }
  return out.join("\n");
}
