import type { ExportRow } from "../api/types";
import { DICTS, translate, type Lang, type TextKey } from "../i18n";
import { placeName } from "./profile";

/** The column separator Excel expects for the language: a semicolon in Polish (a comma is the
 *  decimal mark there), a comma in English. */
export const csvSeparator = (lang: Lang) => (lang === "pl" ? ";" : ",");

/** What "Export to Excel (CSV)" can save: one tab (in the order of the tabs) or every offer. */
export const CSV_LISTS = ["own", "today", "board", "review", "rejected", "bin", "declined", "archive", "trash", "all"] as const;
export type CsvList = (typeof CSV_LISTS)[number];

const HEADERS: TextKey[] = [
  "csv.title",
  "csv.company",
  "csv.city",
  "csv.remote",
  "csv.salaryFrom",
  "csv.salaryTo",
  "csv.currency",
  "csv.period",
  "csv.contract",
  "csv.level",
  "csv.stage",
  "csv.interest",
  "csv.favorite",
  "csv.score",
  "csv.scoreSource",
  "csv.sources",
  "csv.link",
  "csv.published",
  "csv.added",
  "csv.notes",
  "csv.list",
];

/** A code as a word of the app language (`remote` + `yes` → "zdalnie"); an unknown code stays as it is. */
function word(lang: Lang, prefix: string, code: string | null): string {
  if (!code) return "";
  const key = `${prefix}.${code}`;
  return key in DICTS[lang] ? translate(lang, key as TextKey) : code;
}

const num = (n: number | null) => (n === null ? "" : String(n));
const pad = (n: number) => String(n).padStart(2, "0");

/** Local date and time that Excel reads as a date: "2026-11-05 19:14". */
export function csvDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** The header and one row per offer, in words of the app language. Rust makes every cell safe for Excel. */
export function csvCells(rows: ExportRow[], lang: Lang): string[][] {
  const t = (key: TextKey) => translate(lang, key);
  return [
    HEADERS.map(t),
    ...rows.map((r) => [
      r.title,
      r.company,
      r.city ?? "",
      word(lang, "remote", r.remote),
      num(r.salaryMin),
      num(r.salaryMax),
      r.salaryCurrency ?? "",
      word(lang, "csv.periodOf", r.salaryPeriod),
      word(lang, "contract", r.contract),
      word(lang, "level", r.level),
      word(lang, "stage", r.stage),
      word(lang, "interest", r.interest),
      r.favorite ? t("csv.yes") : "",
      num(r.score),
      word(lang, "csv.scoreOf", r.scoreSource),
      r.sources.map((s) => word(lang, "source", s)).join(", "),
      r.url ?? "",
      csvDate(r.publishedAt),
      csvDate(r.addedAt),
      r.notes ?? "",
      placeName(r.list, (key, params) => translate(lang, key, params)),
    ]),
  ];
}

/** The list as a part of the file name ("oferty" → tablica-oferty-<date>.csv). */
export const csvName = (list: CsvList, lang: Lang) => translate(lang, `csv.file.${list}` as TextKey);

/** Longest cell, as in Rust (`backup.rs` - Excel keeps at most 32 767 characters in a cell). */
const CSV_MAX_CELL = 32_000;

/** One cell safe for Excel - the same as `csv_cell` in Rust: a formula start gets a `'`, quotes doubled. */
export function csvCell(value: string): string {
  const chars = Array.from(value);
  const cut = chars.length > CSV_MAX_CELL ? chars.slice(0, CSV_MAX_CELL).join("").trimEnd() : value;
  const safe = /^[=+\-@\t\r]/.test(cut) ? `'${cut}` : cut;
  return `"${safe.replace(/"/g, '""')}"`;
}

/** The file text with a BOM (Excel opens Polish letters right) - the same as `csv_text` in Rust.
 *  Used only by the website demo, which downloads the file instead of asking Rust to save it. */
export function csvText(cells: string[][], separator: string): string {
  return "﻿" + cells.map((row) => row.map(csvCell).join(separator) + "\r\n").join("");
}

/** The file name the app saves: `tablica-<list>-<UTC date and time>.csv`, as `file_stamp` in Rust. */
export function csvFileName(name: string, now = new Date()): string {
  const stamp = now.toISOString().slice(0, 19).replace(/-|:/g, "").replace("T", "-");
  return `tablica-${name}-${stamp}.csv`;
}
