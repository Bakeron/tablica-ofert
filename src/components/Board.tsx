import { Fragment, useRef, useState } from "react";
import type { KeyboardEvent, TouchEvent } from "react";
import { STAGES, type Interest, type OfferRow, type View } from "../api/types";
import { useI18n, type TextKey } from "../i18n";
import { addedLabel, placeLabel, salaryShort, translatedTitle } from "../lib/format";
import { isNarrowLayout } from "../lib/fit";
import { isPhone } from "../lib/platform";
import { motionReduced } from "../lib/presence";
import { reasonText, sortedReasons } from "../lib/profile";
import { SWIPE_AT, isSwipe, lockAxis, swipeOffset, type SwipeAxis } from "../lib/swipe";
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

/** Finished offers - rejected, withdrawn or no longer listed. Kept for later, greyed out at the bottom. */
export function isClosed(o: OfferRow): boolean {
  return o.stage === "rejected" || o.stage === "withdrawn" || o.expired;
}

/** Sorted by the chosen column, with finished offers always in their own group below the rest. */
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
    const group = Number(isClosed(a)) - Number(isClosed(b));
    if (group) return group;
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
  const touch = useRef<{ id: number; x: number; y: number; axis: SwipeAxis | null } | null>(null);
  /** The row whose interest mark just came from a swipe - the mark flips in. */
  const [flipped, setFlipped] = useState<number | null>(null);
  const { t, lang } = useI18n();
  const index = rows.findIndex((r) => r.id === selectedId);
  const firstClosed = rows.findIndex(isClosed);
  const closedCount = firstClosed < 0 ? 0 : rows.length - firstClosed;

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

  /** The row follows the finger and uncovers what letting go will do. */
  function followFinger(e: TouchEvent<HTMLTableRowElement>, o: OfferRow) {
    const start = touch.current;
    if (!start || start.id !== o.id) return;
    const dx = e.touches[0].clientX - start.x;
    start.axis ??= lockAxis(dx, e.touches[0].clientY - start.y);
    if (start.axis !== "x" || motionReduced()) return;
    const row = e.currentTarget;
    const offset = swipeOffset(dx, row.clientWidth);
    const value: Interest = dx > 0 ? "high" : "no";
    row.dataset.swipe = dx > 0 ? "right" : "left";
    row.dataset.swipeText = o.interest === value ? t("swipe.clear") : t(`interest.${value}`);
    row.toggleAttribute("data-dragging", true);
    row.toggleAttribute("data-armed", Math.abs(dx) > SWIPE_AT);
    row.style.setProperty("--swipe", `${offset}px`);
    row.style.setProperty("--swipe-width", `${Math.abs(offset)}px`);
  }

  /** The row glides back to its place; the marks go once it is there. */
  function letGo(row: HTMLTableRowElement) {
    if (!row.dataset.swipe) return;
    row.removeAttribute("data-dragging");
    row.style.setProperty("--swipe", "0px");
    row.style.setProperty("--swipe-width", "0px");
    window.setTimeout(() => {
      if (row.hasAttribute("data-dragging")) return;
      row.removeAttribute("data-swipe");
      row.removeAttribute("data-swipe-text");
      row.removeAttribute("data-armed");
    }, 200);
  }

  if (rows.length === 0) {
    return (
      <div className="board board-empty">
        <p>{t(view === "board" && isPhone() ? "board.empty.phone" : `board.empty.${view}`)}</p>
      </div>
    );
  }

  return (
    <div className="board-wrap">
      <table
        className="board"
        role="grid"
        aria-label={t("board.label")}
        aria-rowcount={rows.length + 1 + (closedCount > 0 ? 1 : 0)}
        aria-activedescendant={selectedId !== null ? `offer-${selectedId}` : undefined}
        tabIndex={0}
        onKeyDown={onKeyDown}
      >
        <thead role="rowgroup">
          <tr role="row">
            {COLUMNS.map((col) => {
              const active = sort.key === col.key;
              return (
                <th
                  key={col.key}
                  role="columnheader"
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
        <tbody role="rowgroup">
          {rows.map((o, i) => {
            const fresh = freshIds.has(o.id);
            const selected = o.id === selectedId;
            const sourceNames = o.sources.map((s) => sourceLabel(s, t));
            return (
              <Fragment key={o.id}>
              {i === firstClosed && (
                <tr role="row" className="board-group">
                  <th role="columnheader" colSpan={COLUMNS.length} scope="colgroup">
                    {t("board.closed", { count: closedCount })}
                  </th>
                </tr>
              )}
              <tr
                role="row"
                id={`offer-${o.id}`}
                aria-selected={selected}
                className={[
                  selected && "is-selected",
                  o.interest === "no" && "is-dim",
                  o.expired && "is-expired",
                  i >= firstClosed && firstClosed >= 0 && "is-closed",
                ]
                  .filter(Boolean)
                  .join(" ")}
                onClick={() => {
                  onSelect(o.id);
                  // On a phone the card is a screen of its own.
                  if (isNarrowLayout()) onOpen();
                }}
                onDoubleClick={onOpen}
                onTouchStart={(e) => (touch.current = { id: o.id, x: e.touches[0].clientX, y: e.touches[0].clientY, axis: null })}
                onTouchMove={(e) => followFinger(e, o)}
                onTouchCancel={(e) => {
                  touch.current = null;
                  letGo(e.currentTarget);
                }}
                onTouchEnd={(e) => {
                  const start = touch.current;
                  touch.current = null;
                  letGo(e.currentTarget);
                  if (!start || start.id !== o.id || start.axis !== "x") return;
                  // Sideways is never a tap, also when the finger came back.
                  e.preventDefault();
                  const dx = e.changedTouches[0].clientX - start.x;
                  const dy = e.changedTouches[0].clientY - start.y;
                  if (isSwipe(dx, dy)) {
                    setFlipped(o.id);
                    onInterest(o.id, dx > 0 ? "high" : "no");
                  }
                }}
              >
                <td role="gridcell" className="c-added">
                  <CellName text={t("col.added")} />
                  <Flap className="mono" text={addedLabel(o.addedAt, lang, t)} fresh={fresh} />
                </td>
                <td role="gridcell" className="c-offer">
                  <span className="offer-line">
                    {o.favorite && (
                      <span className="fav">
                        <IconStar filled />
                        <span className="sr-only">{t("favorite.label")}</span>
                      </span>
                    )}
                    <Flap className="offer-title" text={translatedTitle(o, lang) ?? o.title} fresh={fresh} />
                  </span>
                  <span className="offer-line">
                    <span className="offer-company">{o.company}</span>
                    {o.interest && (
                      <span key={o.interest} className={`interest interest-${o.interest}${flipped === o.id ? " flap-in" : ""}`}>
                        <span className="sr-only">{t("interest.label")}: </span>
                        {t(`interest.${o.interest}`)}
                      </span>
                    )}
                  </span>
                  {o.sources.length > 1 ? (
                    <span className="offer-sources">
                      {t("sources.many", { count: o.sources.length, list: sourceNames.join(" · ") })}
                    </span>
                  ) : (
                    // Adzuna's and Himalayas' terms: their offers carry their name also in the list (the credit link is on the card).
                    (o.sources[0] === "adzuna" || o.sources[0] === "himalayas") && (
                      <span className="offer-sources">{t("sources.one", { list: sourceNames[0] })}</span>
                    )
                  )}
                  {(view === "review" || view === "rejected") && <RowReasons row={o} />}
                </td>
                <td role="gridcell" className="c-place">
                  <CellName text={t("cell.place")} />
                  <Flap text={placeLabel(o, t)} fresh={fresh} />
                </td>
                <td role="gridcell" className="c-salary">
                  <CellName text={t("col.salary")} />
                  <Flap className="mono" text={salaryShort(o, t)} fresh={fresh} />
                </td>
                <td role="gridcell" className="c-match">
                  <CellName text={t("col.matchLong")} />
                  {o.score !== null && (
                    <span className="match-cap" aria-hidden="true">
                      {t("col.match")}
                    </span>
                  )}
                  {o.score === null ? (
                    <>
                      <span aria-hidden="true">·</span>
                      <span className="sr-only">{t("card.matchNone")}</span>
                    </>
                  ) : (
                    <span className={o.scoreSource === "keywords" ? "score score-plain" : "score"}>
                      <Flap className="mono" text={String(o.score)} fresh={fresh} />
                      {o.scoreSource === "keywords" && <span className="sr-only"> ({t("card.matchKeywords")})</span>}
                    </span>
                  )}
                </td>
                <td role="gridcell" className="c-status">
                  <StatusTag offer={o} fresh={fresh} />
                </td>
              </tr>
              </Fragment>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

/** The phone's rows have no column headings - a screen reader gets the column's name here. */
function CellName({ text }: { text: string }) {
  return <span className="cell-name">{text}: </span>;
}

/** Why the offer is in "To check" / "Rejected by filter" - the first two reasons, in words. */
function RowReasons({ row }: { row: OfferRow }) {
  const { t } = useI18n();
  const reasons = sortedReasons(row.filterReasons).filter((r) => r.kind !== "note");
  if (reasons.length === 0) return null;
  const more = reasons.length > 2 ? ` · +${reasons.length - 2}` : "";
  return (
    <span className={`offer-reason offer-reason-${row.filterResult}`}>
      <span className="sr-only">{t(`filter.result.${row.filterResult}`)}: </span>
      {reasons
        .slice(0, 2)
        .map((r) => reasonText(r, t))
        .join(" · ")}
      {more}
    </span>
  );
}

export function sourceLabel(source: string, t: (key: TextKey) => string): string {
  const key = `source.${source}` as TextKey;
  const text = t(key);
  return text === key ? source : text;
}
