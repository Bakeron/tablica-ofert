/** A piece of a line of the privacy policy. */
export type Inline = { kind: "text" | "strong" | "code"; text: string } | { kind: "link"; text: string; url: string };

export type Block = { kind: "heading" | "text"; parts: Inline[] } | { kind: "list"; items: Inline[][] };

const ENGLISH_START = /^# Privacy policy/m;
const INLINE = /\*\*(.+?)\*\*|`(.+?)`|\[(.+?)\]\((.+?)\)/g;

export function inline(line: string): Inline[] {
  const parts: Inline[] = [];
  let at = 0;
  for (const m of line.matchAll(INLINE)) {
    if (m.index > at) parts.push({ kind: "text", text: line.slice(at, m.index) });
    if (m[1] !== undefined) parts.push({ kind: "strong", text: m[1] });
    else if (m[2] !== undefined) parts.push({ kind: "code", text: m[2] });
    else if (/^https?:\/\//.test(m[4])) parts.push({ kind: "link", text: m[3], url: m[4] });
    else parts.push({ kind: "text", text: m[3] });
    at = m.index + m[0].length;
  }
  if (at < line.length) parts.push({ kind: "text", text: line.slice(at) });
  return parts;
}

/**
 * The part of PRIVACY.md in the language of the app, as blocks to lay out: `PRIVACY.md` has
 * both languages, one after the other. Only what that file uses: `##`, lists, bold, code, links.
 */
export function privacyBlocks(raw: string, lang: "pl" | "en"): Block[] {
  const text = raw.replace(/\r\n/g, "\n");
  const english = text.search(ENGLISH_START);
  const part = lang === "en" ? text.slice(english) : text.slice(0, english);
  const blocks: Block[] = [];
  for (const line of part.split("\n")) {
    const value = line.trim();
    // Title (the window has its own), the jump to the other language, the line between them.
    if (!value || value === "---" || value.startsWith("# ") || /^\[[^\]]+\]\(#/.test(value)) continue;
    if (value.startsWith("## ")) {
      blocks.push({ kind: "heading", parts: inline(value.slice(3)) });
    } else if (value.startsWith("- ")) {
      const last = blocks[blocks.length - 1];
      if (last?.kind === "list") last.items.push(inline(value.slice(2)));
      else blocks.push({ kind: "list", items: [inline(value.slice(2))] });
    } else {
      blocks.push({ kind: "text", parts: inline(value) });
    }
  }
  return blocks;
}
