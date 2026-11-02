import type { KeyboardEvent } from "react";
import { STAGES, type Interest, type OfferRow, type View } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { addedLabel, placeLabel, salaryShort } from "../lib/format";
import { Flap } from "./Flap";
import { IconStar } from "./Icons";
import { StatusTag } from "./StatusTag";

export type SortKey = "added" | "offer" | "place" | "salary" | "match" | "status";
export interface Sort {
  key: SortKey;
  dir: "asc" | "desc";
}

const COLUMNS: { key: SortKey; label: TextKey; className: string }[] = [
  { key: "added", label: "col.added", className: "c-added" },
  { key: "offer", label: "col.offer", className: "c-offer" },
  { key: "place", label: "col.place", className: "c-place" },
  { key: "salary", label: "col.salary", className: "c-salary" },
  { key: "match", label: "col.match", className: "c-match" },
  { key: "status", label: "col.status", className: "c-status" },
];

export function sortRows(rows: OfferRow[], sort: Sort): OfferRow[] {
  const value = (o: OfferRow): string | number => {
    switch (sort.key) {
      case "added":
        return o.addedAt;
      case "offer":
        return `${o.title} ${o.company}`.toLocaleLowerCase();
      case "place":
        return (o.remote === "yes" ? "" : (o.city ?? "~")).toLocaleLowerCase();
      case "salary":
        return o.salaryMax ?? o.salaryMin ?? -1;
      case "match":
        return o.score ?? -1;
      case "status":
        return STAGES.indexOf(o.stage);
    }
  };
  const factor = sort.dir === "asc" ? 1 : -1;
  return [...rows].sort((a, b) => {
    const va = value(a);
    const vb = value(b);
    const cmp = typeof va === "number" && typeof vb === "number" ? va - vb : String(va).localeCompare(String(vb));
    return cmp * factor || b.id - a.id;
  });
}

interface Props {
  rows: OfferRow[];
  view: View;
  selectedId: number | null;
  freshIds: Set<number>;
  sort: Sort;
  onSort: (key: SortKey) => void;
  onSelect: (id: number) => void;
  onOpen: () => void;
  onInterest: (id: number, interest: Interest) => void;
  onTrash: (id: number) => void;
}

export function Board({ rows, view, selectedId, freshIds, sort, onSort, onSelect, onOpen, onInterest, onTrash }: Props) {
  const { t, lang } = useI18n();
  const index = rows.findIndex((r) => r.id === selectedId);

  function onKeyDown(e: KeyboardEvent) {
    if (e.target !== e.currentTarget || rows.length === 0) return;
    const go = (i: number) => {
      const row = rows[Math.max(0, Math.min(rows.length - 1, i))];
      onSelect(row.id);
      document.getElementById(`offer-${row.id}`)?.scrollIntoView({ block: "nearest" });
    };
    const interests: Record<string, Interest> = { "1": "high", "2": "maybe", "3": "no" };
    if (e.key === "ArrowDown") go(index + 1);
    else if (e.key === "ArrowUp") go(index < 0 ? 0 : index - 1);
    else if (e.key === "Home") go(0);
    else if (e.key === "End") go(rows.length - 1);
    else if (e.key === "Enter" && selectedId !== null) onOpen();
    else if (interests[e.key] && selectedId !== null && view !== "trash") onInterest(selectedId, interests[e.key]);
    else if (e.key === "Delete" && selectedId !== null && view !== "trash") onTrash(selectedId);
    else return;
    e.preventDefault();
  }

  if (rows.length === 0) {
    return (
      <div className="board board-empty">
        <p>{t(`board.empty.${view}`)}</p>
      </div>
    );
  }

  return (
    <div className="board-wrap">
      <table
        className="board"
        role="grid"
        aria-label={t("board.label")}
        aria-rowcount={rows.length + 1}
        aria-activedescendant={selectedId !== null ? `offer-${selectedId}` : undefined}
        tabIndex={0}
        onKeyDown={onKeyDown}
      >
        <thead>
          <tr>
            {COLUMNS.map((col) => {
              const active = sort.key === col.key;
              return (
                <th
                  key={col.key}
                  className={col.className}
                  scope="col"
                  aria-sort={active ? (sort.dir === "asc" ? "ascending" : "descending") : "none"}
                >
                  <button type="button" className="sort" onClick={() => onSort(col.key)}>
                    {col.key === "match" ? <abbr title={t("col.matchLong")}>{t(col.label)}</abbr> : t(col.label)}
                    <span className="sort-mark" aria-hidden="true">
                      {active ? (sort.dir === "asc" ? "▲" : "▼") : ""}
                    </span>
                  </button>
                </th>
              );
            })}
          </tr>
        </thead>
        <tbody>
          {rows.map((o) => {
            const fresh = freshIds.has(o.id);
            const selected = o.id === selectedId;
            const sourceNames = o.sources.map((s) => sourceLabel(s, t));
            return (
              <tr
                key={o.id}
                id={`offer-${o.id}`}
                aria-selected={selected}
                className={[selected && "is-selected", o.interest === "no" && "is-dim", o.expired && "is-expired"]
                  .filter(Boolean)
                  .join(" ")}
                onClick={() => onSelect(o.id)}
                onDoubleClick={onOpen}
              >
                <td className="c-added">
                  <Flap className="mono" text={addedLabel(o.addedAt, lang, t)} fresh={fresh} />
                </td>
                <td className="c-offer">
                  <span className="offer-line">
                    {o.favorite && (
                      <span className="fav">
                        <IconStar filled />
                        <span className="sr-only">{t("favorite.label")}</span>
                      </span>
                    )}
                    <Flap className="offer-title" text={o.title} fresh={fresh} />
                  </span>
                  <span className="offer-line">
                    <span className="offer-company">{o.company}</span>
                    {o.interest && <span className={`interest interest-${o.interest}`}>{t(`interest.${o.interest}`)}</span>}
                  </span>
                  {o.sources.length > 1 && (
                    <span className="offer-sources">
                      {t("sources.many", { count: o.sources.length, list: sourceNames.join(" · ") })}
                    </span>
                  )}
                </td>
                <td className="c-place">
                  <Flap text={placeLabel(o, t)} fresh={fresh} />
                </td>
                <td className="c-salary">
                  <Flap className="mono" text={salaryShort(o, t)} fresh={fresh} />
                </td>
                <td className="c-match">
                  {o.score === null ? (
                    <span aria-label={t("card.matchNone")}>·</span>
                  ) : (
                    <span className={o.scoreSource === "keywords" ? "score score-plain" : "score"}>
                      <Flap className="mono" text={String(o.score)} fresh={fresh} />
                      {o.scoreSource === "keywords" && <span className="sr-only"> ({t("card.matchKeywords")})</span>}
                    </span>
                  )}
                </td>
                <td className="c-status">
                  <StatusTag offer={o} fresh={fresh} />
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

export function sourceLabel(source: string, t: (key: TextKey) => string): string {
  const key = `source.${source}` as TextKey;
  const text = t(key);
  return text === key ? source : text;
}
